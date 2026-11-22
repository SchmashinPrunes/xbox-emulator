include!("util.rs");
include!("link.rs");

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use x::{Allocator, Desc, XDXRCreateDevice};

extern "system" {
    fn AddVectoredExceptionHandler(first: u32, h: unsafe extern "system" fn(*mut [usize; 2]) -> i32) -> *mut c_void;
    fn LoadLibraryW(p: *const u16) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, n: *const u8) -> *mut c_void;
    fn VirtualProtect(p: *mut c_void, n: usize, prot: u32, old: *mut u32) -> i32;
}

static DEBUG: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ALLOCS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
static FREES: Mutex<Vec<usize>> = Mutex::new(Vec::new());
const CTX: usize = 0x5eed;

unsafe extern "system" fn debug_strings(info: *mut [usize; 2]) -> i32 {
    let rec = (*info)[0] as *const u32;
    if *rec == 0xE06D_7363 && *rec.add(6) >= 3 {
        let obj = *(rec.add(10) as *const *const u32);
        let hr = if obj.is_null() { 0 } else { *obj };
        eprintln!("{} threw a C++ exception (object HRESULT {hr:#010x}); the failing d3d12_xs call is logged in {}", CONSOLE.spec.name, std::env::var("TOMBOX_LOG").unwrap_or_default());
    }
    if *rec == 0x4001_0006 {
        let n = *(rec.add(8) as *const usize);
        let s = *(rec.add(10) as *const *const u8);
        let line = String::from_utf8_lossy(core::slice::from_raw_parts(s, n)).trim_end().to_string();
        eprintln!("{} debug: {line}", CONSOLE.spec.name);
        if let Ok(mut d) = DEBUG.try_lock() {
            d.push(line);
        }
    }
    0
}

unsafe extern "system" fn alloc(ctx: *mut c_void, n: usize) -> *mut c_void {
    assert_eq!(ctx as usize, CTX);
    let p = std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(n.max(1), 64).unwrap()) as usize;
    ALLOCS.lock().unwrap().push((n, p));
    p as *mut c_void
}

unsafe extern "system" fn free(ctx: *mut c_void, p: *mut c_void) {
    assert_eq!(ctx as usize, CTX);
    FREES.lock().unwrap().push(p as usize);
}

fn debug_log() -> String {
    DEBUG.lock().unwrap().join("\n")
}

unsafe fn console_d3d12_device() -> *mut c_void {
    let src = near_exe("d3d12_xs.dll");
    let dir = std::env::temp_dir().join(format!("xdxr_d3d12_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&src, dir.join("d3d12_xs.dll")).unwrap();
    std::env::set_var("TOMBOX_LOG", dir.join("d3d12_xs.log"));
    let h = LoadLibraryW(wide(&dir.join("d3d12_xs.dll").to_string_lossy()).as_ptr());
    assert!(!h.is_null(), "emulator d3d12_xs.dll did not load");
    let create: extern "system" fn(*mut c_void, *const c_void, *const c_void, *mut *mut c_void) -> i32 = core::mem::transmute(GetProcAddress(h, b"D3D12XboxCreateDevice\0".as_ptr()));
    let mut params = [0u32; 32];
    params[0] = 0x0002_0002;
    let iid: [u8; 16] = [0xF1, 0x19, 0x98, 0x18, 0xB6, 0x1D, 0x57, 0x4B, 0xBE, 0x54, 0x18, 0x21, 0x33, 0x9B, 0x85, 0xF7];
    let mut dev = core::ptr::null_mut();
    let hr = create(core::ptr::null_mut(), params.as_ptr() as *const c_void, iid.as_ptr() as *const c_void, &mut dev);
    assert!(hr == 0 && !dev.is_null(), "D3D12XboxCreateDevice {hr:#x}");
    dev
}

#[derive(Clone)]
struct Created {
    d3d: usize,
    device: usize,
    allocs: Vec<(usize, usize)>,
}

fn create() -> Created {
    static ONCE: std::sync::OnceLock<Created> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        console_module();
        AddVectoredExceptionHandler(1, debug_strings);
        install_link_recorder();
        let d3d = console_d3d12_device();
        let a = Allocator { ctx: CTX as *mut c_void, alloc: Some(alloc), free: Some(free) };
        let desc = Desc { memory: usize::MAX as *mut c_void, memory_size: 0, flags: 0, reserved: [0; 3] };
        let before = ALLOCS.lock().unwrap().len();
        let xd = XDXRCreateDevice(&a, d3d, &desc) as usize;
        assert!(xd != 0, "XDXRCreateDevice returned NULL\n{}", debug_log());
        Created { d3d: d3d as usize, device: xd, allocs: ALLOCS.lock().unwrap()[before..].to_vec() }
    })
    .clone()
}

fn image_size(base: usize) -> usize {
    unsafe {
        let nt = base + (base as *const u32).byte_add(0x3c).read_unaligned() as usize;
        ((nt + 0x50) as *const u32).read_unaligned() as usize
    }
}

fn embedded_containers(base: usize) -> Vec<(usize, usize)> {
    let img = unsafe { core::slice::from_raw_parts(base as *const u8, image_size(base)) };
    (0..img.len().saturating_sub(32))
        .step_by(4)
        .filter(|&o| &img[o..o + 4] == b"DXBC")
        .map(|o| (base + o, u32::from_le_bytes(img[o + 24..o + 28].try_into().unwrap()) as usize))
        .filter(|&(p, n)| n >= 32 && p + n <= base + img.len() && unsafe { ((p + 28) as *const u32).read_unaligned() } >= 1)
        .collect()
}

#[derive(Clone, Debug)]
struct LinkCall {
    libs: Vec<(usize, Vec<u8>)>,
    names: Vec<String>,
    flags: u32,
    provider: usize,
    mode: u32,
    hr: Hr,
    outs: Vec<u64>,
    code: Vec<u8>,
}

static LINK_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static LINK_CALLS: Mutex<Vec<LinkCall>> = Mutex::new(Vec::new());

unsafe extern "system" fn recorded_link(this: *mut c_void, libs: *const Lib, nlibs: u32, names: *const *const u16, outs: *mut u64, nnames: u32, flags: u32, result: *mut *mut c_void) -> Hr {
    let original: LinkFn = core::mem::transmute(LINK_ORIGINAL.load(Ordering::SeqCst));
    let hr = original(this, libs, nlibs, names, outs, nnames, flags, result);
    if libs.is_null() || names.is_null() || result.is_null() {
        return hr;
    }
    let libs_v = (0..nlibs as usize).map(|i| { let l = &*libs.add(i); (l.ptr as usize, core::slice::from_raw_parts(l.ptr, l.size as usize).to_vec()) }).collect();
    let names_v = (0..nnames as usize)
        .map(|i| {
            let p = *names.add(i);
            let n = (0..).take_while(|&k| *p.add(k) != 0).count();
            String::from_utf16_lossy(core::slice::from_raw_parts(p, n))
        })
        .collect();
    let outs_v = if outs.is_null() { vec![] } else { core::slice::from_raw_parts(outs, nnames as usize).to_vec() };
    let mut code = vec![];
    if hr >= 0 && !(*result).is_null() {
        let r = *result;
        let mut b = core::ptr::null_mut();
        if slot::<GetFn>(r, 4)(r, &mut b) >= 0 && !b.is_null() {
            code = blob_bytes(b);
            release(b);
        }
    }
    let this_b = this as *const u8;
    LINK_CALLS.lock().unwrap().push(LinkCall {
        libs: libs_v,
        names: names_v,
        flags,
        provider: *(this_b.add(0x18) as *const usize),
        mode: *(this_b.add(0x30) as *const u32),
        hr,
        outs: outs_v,
        code,
    });
    hr
}

unsafe fn install_link_recorder() {
    let create: Create2Fn = core::mem::transmute(console_dxcompiler().proc("CreateXdxrCompiler2").unwrap());
    let mut x = core::ptr::null_mut();
    assert_eq!(create(core::ptr::null_mut(), &CLSID_XDXR_COMPILER, &IID_IXDXR_COMPILER, &mut x), S_OK);
    let entry = (*(x as *const *mut usize)).add(8);
    let mut old = 0;
    assert!(VirtualProtect(entry as *mut c_void, 8, 0x04, &mut old) != 0);
    LINK_ORIGINAL.store(*entry, Ordering::SeqCst);
    *entry = recorded_link as usize;
    VirtualProtect(entry as *mut c_void, 8, old, &mut old);
    release(x);
}

#[repr(C)]
struct Subobject {
    kind: u32,
    desc: *const c_void,
}

#[repr(C)]
struct StateObjectDesc {
    kind: u32,
    count: u32,
    subobjects: *const Subobject,
}

#[repr(C)]
struct DxilLibraryDesc {
    code: *const u8,
    size: usize,
    exports: u32,
    export_desc: *const c_void,
}

#[repr(C)]
struct HitGroupDesc {
    export: *const u16,
    kind: u32,
    any_hit: *const u16,
    closest_hit: *const u16,
    intersection: *const u16,
}

#[repr(C)]
struct DescriptorRange {
    kind: u32,
    count: u32,
    base: u32,
    space: u32,
    offset: u32,
}

#[repr(C)]
struct RootParameter {
    kind: u32,
    range_pad: u32,
    ranges: u32,
    range_ptr: *const DescriptorRange,
    visibility: u32,
}

#[repr(C)]
struct RootSignatureDesc {
    count: u32,
    params: *const RootParameter,
    samplers: u32,
    sampler_ptr: *const c_void,
    flags: u32,
}

unsafe fn global_root_signature(d3d: usize) -> *mut c_void {
    let host = LoadLibraryW(wide("d3d12.dll").as_ptr());
    let serialize: unsafe extern "system" fn(*const RootSignatureDesc, u32, *mut *mut c_void, *mut *mut c_void) -> Hr = core::mem::transmute(GetProcAddress(host, b"D3D12SerializeRootSignature\0".as_ptr()));
    let ranges = [DescriptorRange { kind: 0, count: 1, base: 0, space: 0, offset: 0 }, DescriptorRange { kind: 1, count: 1, base: 0, space: 0, offset: 1 }];
    let params = [RootParameter { kind: 0, ranges: 2, range_pad: 0, range_ptr: ranges.as_ptr(), visibility: 0 }];
    let desc = RootSignatureDesc { count: 1, params: params.as_ptr(), samplers: 0, sampler_ptr: core::ptr::null(), flags: 0 };
    let (mut blob, mut err) = (core::ptr::null_mut(), core::ptr::null_mut());
    assert_eq!(serialize(&desc, 1, &mut blob, &mut err), S_OK);
    let bytes = blob_bytes(blob);
    let iid = Guid::read(b"\x66\x6b\x4a\xc5\xdf\x72\xe8\x4e\x8b\xe5\xa9\x46\xa1\x42\x92\x14".as_ptr() as *const c_void).unwrap();
    let mut rs = core::ptr::null_mut();
    let f: unsafe extern "system" fn(usize, u32, *const u8, usize, *const Guid, *mut *mut c_void) -> Hr = slot(d3d as *mut c_void, 16);
    let hr = f(d3d, 0, bytes.as_ptr(), bytes.len(), &iid, &mut rs);
    assert!(hr == S_OK && !rs.is_null(), "console d3d12 CreateRootSignature {hr:#x}");
    rs
}

#[test]
fn creates_a_device_on_the_emulator_console_d3d12_device() {
    let c = create();
    let module = console_module();
    assert_eq!(c.allocs.first().copied(), Some((0x298, c.device)), "first allocation is not the 0x298-byte device: {:x?}", c.allocs);
    let vt = unsafe { (c.device as *const usize).read() };
    assert!((module..module + image_size(module)).contains(&vt), "device vtable {vt:#x} outside {}", CONSOLE.spec.name);
    for i in 0..24 {
        let f = unsafe { ((vt + 8 * i) as *const usize).read() };
        assert_eq!(console_dll::own_module(f), module, "device slot {i} {f:#x}");
    }
    assert!(FREES.lock().unwrap().iter().all(|p| *p != c.device));
}

#[test]
fn state_object_links_the_device_traversal_library_through_slot_8() {
    let c = create();
    let module = console_module();
    let embedded = embedded_containers(module);
    unsafe {
        let lib = compile_library(RAYTRACING_LIBRARY);
        let rs = global_root_signature(c.d3d);
        let hit_name = wide("HitGroup");
        let closest = wide("Hit");
        let library = DxilLibraryDesc { code: lib.as_ptr(), size: lib.len(), exports: 0, export_desc: core::ptr::null() };
        let hit = HitGroupDesc { export: hit_name.as_ptr(), kind: 0, any_hit: core::ptr::null(), closest_hit: closest.as_ptr(), intersection: core::ptr::null() };
        let shader_config = [16u32, 8];
        let pipeline_config = [1u32];
        let subobjects = [
            Subobject { kind: 5, desc: &library as *const _ as *const c_void },
            Subobject { kind: 11, desc: &hit as *const _ as *const c_void },
            Subobject { kind: 9, desc: shader_config.as_ptr() as *const c_void },
            Subobject { kind: 10, desc: pipeline_config.as_ptr() as *const c_void },
            Subobject { kind: 1, desc: &rs as *const _ as *const c_void },
        ];
        let desc = StateObjectDesc { kind: 3, count: subobjects.len() as u32, subobjects: subobjects.as_ptr() };
        let iid = Guid::read(b"\x43\x69\x01\x47\xa8\xfc\x94\x45\x93\xea\xaf\x25\x8b\x55\x34\x6d".as_ptr() as *const c_void).unwrap();
        let mut so = core::ptr::null_mut();
        let create_state_object: unsafe extern "system" fn(usize, *const StateObjectDesc, *const Guid, *mut *mut c_void) -> Hr = slot(c.device as *mut c_void, 11);
        let hr = create_state_object(c.device, &desc, &iid, &mut so);
        assert!(hr == S_OK && !so.is_null(), "XDXR CreateStateObject {hr:#x}\n{}", debug_log());
        let calls = LINK_CALLS.lock().unwrap().clone();
        let call = calls.iter().find(|k| k.names.iter().any(|n| n == "XDXR_TraceRay")).unwrap_or_else(|| panic!("no slot 8 link with XDXR_TraceRay: {:?}", calls.iter().map(|k| (&k.names, k.hr)).collect::<Vec<_>>()));
        assert_eq!(call.hr, S_OK, "{:?}\n{}", call.names, debug_log());
        let inside = |p: usize, n: usize| embedded.iter().any(|&(q, m)| q == p && m == n);
        let (traversal, state_machine) = (&call.libs[call.libs.len() - 2], &call.libs[call.libs.len() - 1]);
        assert!(inside(traversal.0, traversal.1.len()), "traversal library {:#x} is not one of {}'s embedded libraries", traversal.0, CONSOLE.spec.name);
        assert!(inside(state_machine.0, state_machine.1.len()), "state machine library {:#x} is not embedded in {}", state_machine.0, CONSOLE.spec.name);
        assert!(call.libs.iter().any(|l| l.1 == lib), "the title library is not among the linked libraries");
        for name in EXPORTS {
            assert!(call.names.iter().any(|n| n == name), "{name} not linked: {:?}", call.names);
        }
        assert!(call.outs.iter().all(|o| *o != 0), "{:x?}", call.outs);
        assert_eq!(&call.code[..4], b"DXBC");
        let parts = container_parts(&call.code);
        let dxil = parts.iter().find(|p| &p.0 == b"DXIL").expect("linked container has no DXIL part");
        assert_eq!(&call.code[dxil.1 + 8..dxil.1 + 12], b"DXIL");
        let names: Vec<&str> = call.names.iter().map(|s| s.as_str()).collect();
        let libs: Vec<&[u8]> = call.libs.iter().map(|l| l.1.as_slice()).collect();
        let direct = xdxr_link_with(call.provider, call.mode, &libs, &names, call.flags);
        assert_eq!(direct.hr, S_OK);
        assert_eq!(direct.outs, call.outs);
        assert_eq!(direct.code.len(), call.code.len());
        assert_eq!(container_parts(&direct.code).iter().map(|p| (p.0, p.2)).collect::<Vec<_>>(), parts.iter().map(|p| (p.0, p.2)).collect::<Vec<_>>());
        release(so);
    }
}
