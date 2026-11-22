use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};
use compat_common::{record, E_FAIL};

const M: &str = "d3d12_xs";
pub const DXGI_ERROR_UNSUPPORTED: i32 = 0x887a_0004u32 as i32;

const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> [u8; 16] {
    let a = d1.to_le_bytes();
    let b = d2.to_le_bytes();
    let c = d3.to_le_bytes();
    [a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d4[0], d4[1], d4[2], d4[3], d4[4], d4[5], d4[6], d4[7]]
}

#[no_mangle]
pub static IID_ID3D12CommandAllocator: [u8; 16] = guid(0x6102dee4, 0xaf59, 0x4b09, [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24]);

#[no_mangle]
pub static IID_ID3D12CommandList: [u8; 16] = guid(0x7116d91c, 0xe7e4, 0x47ce, [0xb8, 0xc6, 0xec, 0x81, 0x68, 0xf4, 0x37, 0xe5]);

#[no_mangle]
pub static IID_ID3D12CommandQueue: [u8; 16] = guid(0x0ec870a6, 0x5d7e, 0x4c22, [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed]);

#[no_mangle]
pub static IID_ID3D12Debug: [u8; 16] = guid(0x344488b7, 0x6846, 0x474b, [0xb9, 0x89, 0xf0, 0x27, 0x44, 0x82, 0x45, 0xe0]);

#[no_mangle]
pub static IID_ID3D12DescriptorHeap: [u8; 16] = guid(0x8efb471d, 0x616c, 0x4f49, [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51]);

#[no_mangle]
pub static IID_ID3D12DeviceChild: [u8; 16] = guid(0x905db94b, 0xa00c, 0x4140, [0x9d, 0xf5, 0x2b, 0x64, 0xca, 0x9e, 0xa3, 0x57]);

#[no_mangle]
pub static IID_ID3D12Fence: [u8; 16] = guid(0x0a753dcf, 0xc4d8, 0x4b91, [0xad, 0xf6, 0xbe, 0x5a, 0x60, 0xd9, 0x5a, 0x76]);

#[no_mangle]
pub static IID_ID3D12GraphicsCommandList: [u8; 16] = guid(0x5b160d0f, 0xac1b, 0x4185, [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55]);

#[no_mangle]
pub static IID_ID3D12Heap: [u8; 16] = guid(0x6b3b2502, 0x6e51, 0x45b3, [0x90, 0xee, 0x98, 0x84, 0x26, 0x5e, 0x8d, 0xf3]);

#[no_mangle]
pub static IID_ID3D12Object: [u8; 16] = guid(0xc4fec28f, 0x7966, 0x4e95, [0x9f, 0x94, 0xf4, 0x31, 0xcb, 0x56, 0xc3, 0xb8]);

#[no_mangle]
pub static IID_ID3D12Pageable: [u8; 16] = guid(0x63ee58fb, 0x1268, 0x4835, [0x86, 0xda, 0xf0, 0x08, 0xce, 0x62, 0xf0, 0xd6]);

#[no_mangle]
pub static IID_ID3D12PipelineState: [u8; 16] = guid(0x765a30f3, 0xf624, 0x4c6f, [0xa8, 0x28, 0xac, 0xe9, 0x48, 0x62, 0x24, 0x45]);

#[no_mangle]
pub static IID_ID3D12Resource: [u8; 16] = guid(0x696442be, 0xa72e, 0x4059, [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad]);

#[no_mangle]
pub static IID_ID3D12RootSignature: [u8; 16] = guid(0xc54a6b66, 0x72df, 0x4ee8, [0x8b, 0xe5, 0xa9, 0x46, 0xa1, 0x42, 0x92, 0x14]);

#[no_mangle]
pub static IID_ID3D12RootSignatureDeserializer: [u8; 16] = guid(0x34ab647b, 0x3cc8, 0x46ac, [0x84, 0x1b, 0xc0, 0x96, 0x56, 0x45, 0xc0, 0x46]);

#[no_mangle]
pub static IID_ID3D12XboxDmaCommandList: [u8; 16] = guid(0x30bc77b9, 0x8379, 0x4b25, [0x95, 0xde, 0xb8, 0x58, 0x8c, 0x42, 0x27, 0x5d]);

#[no_mangle]
pub static IID_IDXGIAdapter: [u8; 16] = guid(0x2411e7e1, 0x12ac, 0x4ccf, [0xbd, 0x14, 0x97, 0x98, 0xe8, 0x53, 0x4d, 0xc0]);

#[no_mangle]
pub static IID_IDXGIAdapter1: [u8; 16] = guid(0x29038f61, 0x3839, 0x4626, [0x91, 0xfd, 0x08, 0x68, 0x79, 0x01, 0x1a, 0x05]);

#[no_mangle]
pub static IID_IDXGIAdapter2: [u8; 16] = guid(0x0aa1ae0a, 0xfa0e, 0x4b84, [0x86, 0x44, 0xe0, 0x5f, 0xf8, 0xe5, 0xac, 0xb5]);

#[no_mangle]
pub static IID_IDXGIDevice: [u8; 16] = guid(0x54ec77fa, 0x1377, 0x44e6, [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c]);

#[no_mangle]
pub static IID_IDXGIDevice1: [u8; 16] = guid(0x77db970f, 0x6276, 0x48ba, [0xba, 0x28, 0x07, 0x01, 0x43, 0xb4, 0x39, 0x2c]);

#[no_mangle]
pub static IID_IDXGIDevice2: [u8; 16] = guid(0x05008617, 0xfbfd, 0x4051, [0xa7, 0x90, 0x14, 0x48, 0x84, 0xb4, 0xf6, 0xa9]);

#[no_mangle]
pub static IID_IDXGIDeviceSubObject: [u8; 16] = guid(0x3d3e0379, 0xf9de, 0x4d58, [0xbb, 0x6c, 0x18, 0xd6, 0x29, 0x92, 0xf1, 0xa6]);

#[no_mangle]
pub static IID_IDXGIFactory: [u8; 16] = guid(0x7b7166ec, 0x21c7, 0x44ae, [0xb2, 0x1a, 0xc9, 0xae, 0x32, 0x1a, 0xe3, 0x69]);

#[no_mangle]
pub static IID_IDXGIFactory1: [u8; 16] = guid(0x770aae78, 0xf26f, 0x4dba, [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87]);

#[no_mangle]
pub static IID_IDXGIFactory2: [u8; 16] = guid(0x50c83a1c, 0xe072, 0x4c48, [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0]);

#[no_mangle]
pub static IID_IDXGIObject: [u8; 16] = guid(0xaec22fb8, 0x76f3, 0x4639, [0x9b, 0xe0, 0x28, 0xeb, 0x43, 0xa6, 0x7a, 0x2e]);

#[no_mangle]
pub static IID_IDXGISwapChain: [u8; 16] = guid(0x310d36a0, 0xd2e7, 0x4c0a, [0xaa, 0x04, 0x6a, 0x9d, 0x23, 0xb8, 0x88, 0x6a]);

#[no_mangle]
pub static IID_IDXGISwapChain1: [u8; 16] = guid(0x790a45f7, 0x0d42, 0x4876, [0x98, 0x3a, 0x0a, 0x55, 0xcf, 0xe6, 0xf4, 0xaa]);

#[no_mangle]
pub static IID_IGraphicsUnknown: [u8; 16] = guid(0xaceeea63, 0xe0a9, 0x4a1c, [0xbb, 0xec, 0x71, 0xb2, 0xf4, 0x85, 0xf7, 0x58]);

#[no_mangle]
pub static WKPDID_D3DDebugObjectNameW: [u8; 16] = guid(0x4cca5fd8, 0x921f, 0x42c8, [0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41]);

extern "system" {
    fn LoadLibraryW(p: *const u16) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, n: *const u8) -> *mut c_void;
    fn FlushProcessWriteBuffers();
}

fn d3dcompiler(name: &[u8]) -> usize {
    static H: AtomicUsize = AtomicUsize::new(0);
    let mut h = H.load(Ordering::Acquire);
    if h == 0 {
        let w: Vec<u16> = r"C:\Windows\System32\d3dcompiler_47.dll".encode_utf16().chain(Some(0)).collect();
        h = unsafe { LoadLibraryW(w.as_ptr()) } as usize;
        if h == 0 {
            return 0;
        }
        H.store(h, Ordering::Release);
    }
    unsafe { GetProcAddress(h as *mut c_void, name.as_ptr()) as usize }
}

macro_rules! host {
    ($p:expr, $name:literal, fn($($a:ident: $t:ty),*)) => {{
        let p = $p;
        if p == 0 {
            record(M, $name, "host export missing -> E_FAIL");
            return E_FAIL;
        }
        let f: unsafe extern "system" fn($($t),*) -> i32 = unsafe { core::mem::transmute(p) };
        unsafe { f($($a),*) }
    }};
}

#[no_mangle]
pub extern "system" fn D3D10CreateBlob(size: usize, pp: *mut *mut c_void) -> i32 {
    host!(d3dcompiler(b"D3DCreateBlob\0"), "D3D10CreateBlob", fn(size: usize, pp: *mut *mut c_void))
}

#[no_mangle]
pub extern "system" fn D3D12GetDebugInterface(riid: *const c_void, pp: *mut *mut c_void) -> i32 {
    host!(crate::host_proc(b"D3D12GetDebugInterface\0"), "D3D12GetDebugInterface", fn(riid: *const c_void, pp: *mut *mut c_void))
}

#[no_mangle]
pub extern "system" fn D3D12CreateRootSignatureDeserializer(data: *const c_void, size: usize, riid: *const c_void, pp: *mut *mut c_void) -> i32 {
    let p = crate::host_proc(b"D3D12CreateRootSignatureDeserializer\0");
    if p == 0 {
        record(M, "D3D12CreateRootSignatureDeserializer", "host export missing -> E_FAIL");
        return E_FAIL;
    }
    unsafe { crate::rs_xbox::deserializer(core::mem::transmute(p), false, data, size, riid, pp) }
}

#[no_mangle]
pub extern "system" fn D3D12CreateVersionedRootSignatureDeserializer(data: *const c_void, size: usize, riid: *const c_void, pp: *mut *mut c_void) -> i32 {
    let p = crate::host_proc(b"D3D12CreateVersionedRootSignatureDeserializer\0");
    if p == 0 {
        record(M, "D3D12CreateVersionedRootSignatureDeserializer", "host export missing -> E_FAIL");
        return E_FAIL;
    }
    unsafe { crate::rs_xbox::deserializer(core::mem::transmute(p), true, data, size, riid, pp) }
}

#[no_mangle]
pub extern "system" fn DXGIXPresentArray(_a: u32, _b: u32, _c: u32, _d: u32, _e: *const c_void, _f: *const c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXGetFrameStatistics(_a: u32, _b: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXSetVLineNotification(_a: u32, _b: u32, _c: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXSetFrameNotification(_a: u32, _b: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

pub fn flush_cpu_cache() {
    core::sync::atomic::fence(Ordering::SeqCst);
    unsafe { FlushProcessWriteBuffers() };
}

#[no_mangle]
pub extern "system" fn D3DFlushEntireCpuCache() {
    flush_cpu_cache();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(g: &[u8; 16]) -> String {
        let d1 = u32::from_le_bytes(g[0..4].try_into().unwrap());
        let d2 = u16::from_le_bytes(g[4..6].try_into().unwrap());
        let d3 = u16::from_le_bytes(g[6..8].try_into().unwrap());
        let t: String = g[8..].iter().map(|b| format!("{b:02x}")).collect();
        format!("{d1:08x}-{d2:04x}-{d3:04x}-{}-{}", &t[..4], &t[4..])
    }

    const CONSOLE: &str = r"D:\xbox-pc-port\games\XDefiant\extracted\EmbeddedXvd\Windows\System32\d3d12_xs.dll";

    fn console_data(name: &str) -> Option<[u8; 16]> {
        let b = std::fs::read(CONSOLE).ok()?;
        let pe = u32::from_le_bytes(b[0x3c..0x40].try_into().unwrap()) as usize;
        let nsec = u16::from_le_bytes(b[pe + 6..pe + 8].try_into().unwrap()) as usize;
        let optsz = u16::from_le_bytes(b[pe + 20..pe + 22].try_into().unwrap()) as usize;
        let opt = pe + 24;
        let secs = opt + optsz;
        let rd = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let map = |rva: u32| -> usize {
            (0..nsec)
                .find_map(|i| {
                    let s = secs + i * 40;
                    let va = rd(s + 12);
                    let vs = rd(s + 8).max(rd(s + 16));
                    (rva >= va && rva < va + vs).then(|| (rva - va + rd(s + 20)) as usize)
                })
                .unwrap()
        };
        let ed = map(rd(opt + 112));
        let n = rd(ed + 24) as usize;
        let names = map(rd(ed + 32));
        let ords = map(rd(ed + 36));
        let funcs = map(rd(ed + 28));
        for i in 0..n {
            let np = map(rd(names + i * 4));
            let end = b[np..].iter().position(|&c| c == 0).unwrap();
            if &b[np..np + end] == name.as_bytes() {
                let o = u16::from_le_bytes(b[ords + i * 2..ords + i * 2 + 2].try_into().unwrap()) as usize;
                let f = map(rd(funcs + o * 4));
                return Some(b[f..f + 16].try_into().unwrap());
            }
        }
        None
    }

    #[test]
    fn guids_match_console_dll_data() {
        let all: [(&str, &[u8; 16]); 33] = [
            ("IID_ID3D12CommandAllocator", &IID_ID3D12CommandAllocator),
            ("IID_ID3D12CommandList", &IID_ID3D12CommandList),
            ("IID_ID3D12CommandQueue", &IID_ID3D12CommandQueue),
            ("IID_ID3D12Debug", &IID_ID3D12Debug),
            ("IID_ID3D12DescriptorHeap", &IID_ID3D12DescriptorHeap),
            ("IID_ID3D12Device", &crate::IID_ID3D12Device),
            ("IID_ID3D12DeviceChild", &IID_ID3D12DeviceChild),
            ("IID_ID3D12Fence", &IID_ID3D12Fence),
            ("IID_ID3D12GraphicsCommandList", &IID_ID3D12GraphicsCommandList),
            ("IID_ID3D12Heap", &IID_ID3D12Heap),
            ("IID_ID3D12Object", &IID_ID3D12Object),
            ("IID_ID3D12Pageable", &IID_ID3D12Pageable),
            ("IID_ID3D12PipelineState", &IID_ID3D12PipelineState),
            ("IID_ID3D12Resource", &IID_ID3D12Resource),
            ("IID_ID3D12RootSignature", &IID_ID3D12RootSignature),
            ("IID_ID3D12RootSignatureDeserializer", &IID_ID3D12RootSignatureDeserializer),
            ("IID_ID3D12XboxDmaCommandList", &IID_ID3D12XboxDmaCommandList),
            ("IID_IDXGIAdapter", &IID_IDXGIAdapter),
            ("IID_IDXGIAdapter1", &IID_IDXGIAdapter1),
            ("IID_IDXGIAdapter2", &IID_IDXGIAdapter2),
            ("IID_IDXGIDevice", &IID_IDXGIDevice),
            ("IID_IDXGIDevice1", &IID_IDXGIDevice1),
            ("IID_IDXGIDevice2", &IID_IDXGIDevice2),
            ("IID_IDXGIDeviceSubObject", &IID_IDXGIDeviceSubObject),
            ("IID_IDXGIFactory", &IID_IDXGIFactory),
            ("IID_IDXGIFactory1", &IID_IDXGIFactory1),
            ("IID_IDXGIFactory2", &IID_IDXGIFactory2),
            ("IID_IDXGIObject", &IID_IDXGIObject),
            ("IID_IDXGISwapChain", &IID_IDXGISwapChain),
            ("IID_IDXGISwapChain1", &IID_IDXGISwapChain1),
            ("IID_IGraphicsUnknown", &IID_IGraphicsUnknown),
            ("WKPDID_D3DDebugObjectNameW", &WKPDID_D3DDebugObjectNameW),
            ("IID_ID3D12Debug", &IID_ID3D12Debug),
        ];
        if !std::path::Path::new(CONSOLE).is_file() {
            assert_eq!(text(&IID_ID3D12Debug), "344488b7-6846-474b-b989-f027448245e0");
            return;
        }
        for (n, g) in all {
            assert_eq!(console_data(n).as_ref(), Some(g), "{n} {}", text(g));
        }
    }

    #[test]
    fn export_table_names_match_source_items() {
        let mut syms = Vec::new();
        for e in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap().flatten() {
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let mut armed = false;
            for l in text.lines().map(str::trim) {
                if l == "#[no_mangle]" {
                    armed = true;
                    continue;
                }
                if armed {
                    for kw in ["fn ", "static "] {
                        if let Some(i) = l.find(kw) {
                            syms.push(l[i + kw.len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>());
                            break;
                        }
                    }
                    armed = false;
                }
            }
        }
        let table = include_str!("../exports.txt");
        let mut n = 0;
        for line in table.lines().filter(|l| !l.trim().is_empty()) {
            let f: Vec<&str> = line.split(" | ").next().unwrap().split_whitespace().collect();
            let has = syms.iter().any(|s| s == f[1]);
            assert_eq!(has, f[2] != "missing", "{}: status {} but exported={has}", f[1], f[2]);
            n += 1;
        }
        assert_eq!(n, 55);
    }

    #[test]
    fn dxgix_entry_points_return_console_unsupported() {
        assert_eq!(DXGIXPresentArray(0, 0, 0, 0, core::ptr::null(), core::ptr::null()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXGetFrameStatistics(0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXSetVLineNotification(0, 0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXSetFrameNotification(0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
    }

    #[test]
    fn blob_forwards_to_host_compiler() {
        let mut b: *mut c_void = core::ptr::null_mut();
        assert_eq!(D3D10CreateBlob(48, &mut b), 0);
        assert!(!b.is_null());
        unsafe {
            let vt = *(b as *const *const usize);
            let size: unsafe extern "system" fn(*mut c_void) -> usize = core::mem::transmute(*vt.add(4));
            assert_eq!(size(b), 48);
            let rel: unsafe extern "system" fn(*mut c_void) -> u32 = core::mem::transmute(*vt.add(2));
            rel(b);
        }
    }

    #[test]
    fn deserializer_and_debug_forward_to_host() {
        let mut p: *mut c_void = core::ptr::null_mut();
        let bad = [0u8; 4];
        assert!(D3D12CreateRootSignatureDeserializer(bad.as_ptr() as *const c_void, 4, IID_ID3D12RootSignatureDeserializer.as_ptr() as *const c_void, &mut p) < 0);
        assert!(D3D12CreateVersionedRootSignatureDeserializer(bad.as_ptr() as *const c_void, 4, IID_ID3D12RootSignatureDeserializer.as_ptr() as *const c_void, &mut p) < 0);
        let wrong = IID_IDXGIObject;
        assert!(D3D12GetDebugInterface(wrong.as_ptr() as *const c_void, &mut p) < 0);
    }

    #[test]
    fn flush_entire_cpu_cache_publishes_prior_writes() {
        use std::sync::atomic::{AtomicBool, AtomicU64};
        static V: AtomicU64 = AtomicU64::new(0);
        static R: AtomicBool = AtomicBool::new(false);
        let t = std::thread::spawn(|| {
            while !R.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
            V.load(Ordering::Relaxed)
        });
        V.store(0xfeed, Ordering::Relaxed);
        D3DFlushEntireCpuCache();
        R.store(true, Ordering::Relaxed);
        assert_eq!(t.join().unwrap(), 0xfeed);
    }
}

#[cfg(test)]
mod console_fixed_tests {
    #[path = "../../../../console-dll/oracle/leaf.rs"]
    mod leaf;

    #[test]
    fn fixed_result_dxgix_exports_match_every_console_d3d12_xs() {
        let rows = [
            ("DXGIXGetFrameStatistics", super::DXGIXGetFrameStatistics as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXPresentArray", super::DXGIXPresentArray as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXSetFrameNotification", super::DXGIXSetFrameNotification as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXSetVLineNotification", super::DXGIXSetVLineNotification as *const () as usize, 0xffff_ffff, 0x887a_0004),
        ];
        let seen = leaf::check_fixed("d3d12_xs.dll", &rows);
        assert!(seen.len() >= 4, "{seen:?}");
    }
}
