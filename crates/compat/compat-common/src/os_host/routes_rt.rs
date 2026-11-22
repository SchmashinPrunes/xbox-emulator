use super::patch::{self, dll_matches, Hook};
use super::routes;
use core::cell::RefCell;
use core::ffi::c_void;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(n: *const u16) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const u8) -> *mut c_void;
    fn GetLastError() -> u32;
    fn SetLastError(e: u32);
    fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
    fn VirtualProtect(addr: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    fn FlushInstructionCache(process: *mut c_void, addr: *const c_void, size: usize) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
}

pub const LOADER_DLLS: &[&str] = &["kernel32.dll", "kernelbase.dll", "ntdll.dll", "kernel32legacy.dll","api-ms-win-core-libraryloader-", "api-ms-win-core-kernel32-legacy-"];
pub const LOADER_FUNCS: &[&str] = &[
    "GetProcAddress",
    "LoadLibraryA",
    "LoadLibraryW",
    "LoadLibraryExA",
    "LoadLibraryExW",
    "LoadPackagedLibrary",
    "GetModuleHandleA",
    "GetModuleHandleW",
    "GetModuleHandleExA",
    "GetModuleHandleExW",
    "GetProcAddressForCaller",
    "LdrGetProcedureAddress",
    "LdrGetProcedureAddressEx",
    "LdrLoadDll",
    "LdrGetDllHandle",
];
const NF: usize = 15;
const F_GPA: usize = 0;
const F_LA: usize = 1;
const F_LW: usize = 2;
const F_LXA: usize = 3;
const F_LXW: usize = 4;
const F_PKG: usize = 5;
const F_GMA: usize = 6;
const F_GMW: usize = 7;
const F_GMXA: usize = 8;
const F_GMXW: usize = 9;
const F_GPAC: usize = 10;
const F_LDR_GPA: usize = 11;
const F_LDR_GPAX: usize = 12;
const F_LDR_LOAD: usize = 13;
const F_LDR_HANDLE: usize = 14;
const DATAFILE_FLAGS: u32 = 0x2 | 0x20 | 0x40;
const FROM_ADDRESS: u32 = 0x4;
const STATUS_SUCCESS: i32 = 0;
const STUB_SIZE: usize = 32;
const STUB_PAGE: usize = 0x10000;

type Gpa = unsafe extern "system" fn(*mut c_void, *const u8) -> *mut c_void;
type GpaCaller = unsafe extern "system" fn(*mut c_void, *const u8, *mut c_void) -> *mut c_void;
type LoadA = unsafe extern "system" fn(*const u8) -> *mut c_void;
type LoadW = unsafe extern "system" fn(*const u16) -> *mut c_void;
type LoadExA = unsafe extern "system" fn(*const u8, *mut c_void, u32) -> *mut c_void;
type LoadExW = unsafe extern "system" fn(*const u16, *mut c_void, u32) -> *mut c_void;
type LoadPkg = unsafe extern "system" fn(*const u16, u32) -> *mut c_void;
type HandleExA = unsafe extern "system" fn(u32, *const u8, *mut *mut c_void) -> i32;
type HandleExW = unsafe extern "system" fn(u32, *const u16, *mut *mut c_void) -> i32;
type LdrGpa = unsafe extern "system" fn(*mut c_void, *const Counted<u8>, u32, *mut *mut c_void) -> i32;
type LdrGpaEx = unsafe extern "system" fn(*mut c_void, *const Counted<u8>, u32, *mut *mut c_void, u32) -> i32;
type LdrDll = unsafe extern "system" fn(*const u16, *const u32, *const Counted<u16>, *mut *mut c_void) -> i32;

#[repr(C)]
pub struct Counted<T> {
    pub len: u16,
    pub max: u16,
    pub buf: *const T,
}

struct Pc {
    first: [usize; NF],
    bindings: HashMap<&'static str, Vec<usize>>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn pc() -> &'static Pc {
    static P: OnceLock<Pc> = OnceLock::new();
    P.get_or_init(|| unsafe {
        let mods: Vec<*mut c_void> = ["kernelbase.dll", "kernel32.dll", "ntdll.dll"].iter().map(|m| GetModuleHandleW(wide(m).as_ptr())).filter(|h| !h.is_null()).collect();
        let mut bindings = HashMap::new();
        let mut first = [0usize; NF];
        for (i, f) in LOADER_FUNCS.iter().enumerate() {
            let c = format!("{f}\0");
            let mut v: Vec<usize> = Vec::new();
            for h in &mods {
                let p = GetProcAddress(*h, c.as_ptr()) as usize;
                if p != 0 && !v.contains(&p) {
                    v.push(p);
                }
            }
            first[i] = v.first().copied().unwrap_or(0);
            bindings.insert(*f, v);
        }
        Pc { first, bindings }
    })
}

#[repr(C)]
struct Ctx {
    f: usize,
    next: usize,
}

thread_local! {
    static ACTIVE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

struct Guard(usize);

impl Drop for Guard {
    fn drop(&mut self) {
        let k = self.0;
        let _ = ACTIVE.try_with(|a| {
            if let Ok(mut v) = a.try_borrow_mut() {
                if let Some(i) = v.iter().rposition(|x| *x == k) {
                    v.remove(i);
                }
            }
        });
    }
}

fn enter(key: usize, f: usize, next: usize) -> (usize, Option<Guard>) {
    let entered = ACTIVE.try_with(|a| match a.try_borrow_mut() {
        Ok(mut v) if !v.contains(&key) => {
            v.push(key);
            true
        }
        _ => false,
    });
    match entered {
        Ok(true) => (next, Some(Guard(key))),
        _ => (pc().first[f], None),
    }
}

core::arch::global_asm!(
    ".text",
    ".p2align 4",
    ".globl tombox_route_shim",
    "tombox_route_shim:",
    ".seh_proc tombox_route_shim",
    "sub rsp, 0x48",
    ".seh_stackalloc 0x48",
    ".seh_endprologue",
    "mov rax, [rsp + 0x70]",
    "mov [rsp + 0x28], rax",
    "mov [rsp + 0x20], r9",
    "mov r9, r8",
    "mov r8, rdx",
    "mov rdx, rcx",
    "mov rcx, r10",
    "call {d}",
    "nop",
    "add rsp, 0x48",
    "ret",
    ".seh_endproc",
    d = sym dispatch,
);

extern "C" {
    fn tombox_route_shim();
}

unsafe extern "system" fn dispatch(ctx: *const Ctx, a: usize, b: usize, c: usize, d: usize, e: usize) -> usize {
    let x = &*ctx;
    let (next, _g) = enter(ctx as usize, x.f, x.next);
    call_impl(x.f, next, a, b, c, d, e)
}

unsafe fn call_impl(f: usize, next: usize, a: usize, b: usize, c: usize, d: usize, e: usize) -> usize {
    match f {
        F_GPA => gpa_impl(next, a as _, b as _) as usize,
        F_LA => load_a_impl(next, a as _) as usize,
        F_LW => load_w_impl(next, a as _) as usize,
        F_LXA => load_ex_a_impl(next, a as _, b as _, c as u32) as usize,
        F_LXW => load_ex_w_impl(next, a as _, b as _, c as u32) as usize,
        F_PKG => load_pkg_impl(next, a as _, b as u32) as usize,
        F_GMA => handle_a_impl(next, a as _) as usize,
        F_GMW => handle_w_impl(next, a as _) as usize,
        F_GMXA => handle_ex_a_impl(next, a as u32, b as _, c as _) as u32 as usize,
        F_GMXW => handle_ex_w_impl(next, a as u32, b as _, c as _) as u32 as usize,
        F_GPAC => gpac_impl(next, a as _, b as _, c as _) as usize,
        F_LDR_GPA => ldr_gpa_impl(next, a as _, b as _, c as u32, d as _) as u32 as usize,
        F_LDR_GPAX => ldr_gpax_impl(next, a as _, b as _, c as u32, d as _, e as u32) as u32 as usize,
        F_LDR_LOAD => ldr_load_impl(next, a as _, b as _, c as _, d as _) as u32 as usize,
        F_LDR_HANDLE => ldr_handle_impl(next, a as _, b as _, c as _, d as _) as u32 as usize,
        _ => 0,
    }
}

struct Stubs {
    page: usize,
    used: usize,
    by_key: HashMap<(usize, usize), usize>,
    next_of: HashMap<usize, usize>,
}

fn stubs() -> &'static Mutex<Stubs> {
    static S: OnceLock<Mutex<Stubs>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Stubs { page: 0, used: STUB_PAGE, by_key: HashMap::new(), next_of: HashMap::new() }))
}

unsafe fn make_stub(f: usize, foreign: usize) -> Option<usize> {
    let mut s = stubs().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = s.by_key.get(&(f, foreign)) {
        return Some(*p);
    }
    let mut old = 0u32;
    if s.used + STUB_SIZE > STUB_PAGE {
        let p = VirtualAlloc(core::ptr::null_mut(), STUB_PAGE, 0x3000, 0x04);
        if p.is_null() {
            return None;
        }
        s.page = p as usize;
        s.used = 0;
    } else if VirtualProtect(s.page as *mut c_void, STUB_PAGE, 0x40, &mut old) == 0 {
        return None;
    }
    let at = s.page + s.used;
    let ctx = Box::leak(Box::new(Ctx { f, next: foreign })) as *const Ctx as u64;
    let shim = tombox_route_shim as *const () as u64;
    let mut code = [0xCCu8; STUB_SIZE];
    code[0] = 0x49;
    code[1] = 0xBA;
    code[2..10].copy_from_slice(&ctx.to_le_bytes());
    code[10] = 0x49;
    code[11] = 0xBB;
    code[12..20].copy_from_slice(&shim.to_le_bytes());
    code[20] = 0x41;
    code[21] = 0xFF;
    code[22] = 0xE3;
    core::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, STUB_SIZE);
    VirtualProtect(s.page as *mut c_void, STUB_PAGE, 0x20, &mut old);
    FlushInstructionCache(GetCurrentProcess(), at as *const c_void, STUB_SIZE);
    s.used += STUB_SIZE;
    s.by_key.insert((f, foreign), at);
    s.next_of.insert(at, foreign);
    Some(at)
}

pub unsafe fn chain_stub(func: &str, foreign: usize) -> Option<usize> {
    let f = LOADER_FUNCS.iter().position(|x| *x == func)?;
    if foreign == 0 {
        return None;
    }
    make_stub(f, foreign)
}

pub fn chained_to(stub: usize) -> Option<usize> {
    stubs().lock().unwrap_or_else(|e| e.into_inner()).next_of.get(&stub).copied()
}

pub fn shim_address() -> usize {
    tombox_route_shim as *const () as usize
}

pub fn chain_count() -> usize {
    stubs().lock().unwrap_or_else(|e| e.into_inner()).next_of.len()
}

fn names() -> &'static Mutex<HashMap<usize, Vec<String>>> {
    static N: OnceLock<Mutex<HashMap<usize, Vec<String>>>> = OnceLock::new();
    N.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn bare(name: &str) -> String {
    let b = name.rsplit(['\\', '/']).next().unwrap_or(name).trim().to_ascii_lowercase();
    if b.contains('.') {
        b
    } else {
        format!("{b}.dll")
    }
}

pub fn record(h: *mut c_void, name: &str) {
    if h.is_null() || name.is_empty() {
        return;
    }
    let b = bare(name);
    let mut g = names().lock().unwrap_or_else(|e| e.into_inner());
    let v = g.entry(h as usize).or_default();
    v.retain(|x| *x != b);
    v.insert(0, b);
}

pub fn recorded(h: *mut c_void) -> Vec<String> {
    names().lock().unwrap_or_else(|e| e.into_inner()).get(&(h as usize)).cloned().unwrap_or_default()
}

unsafe fn cstr(p: *const u8) -> String {
    let mut n = 0;
    while n < 1024 && *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf8_lossy(core::slice::from_raw_parts(p, n)).into_owned()
}

unsafe fn wstr(p: *const u16) -> String {
    let mut n = 0;
    while n < 1024 && *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(core::slice::from_raw_parts(p, n))
}

unsafe fn counted_a(s: *const Counted<u8>) -> Option<Vec<u8>> {
    if s.is_null() || (*s).buf.is_null() {
        return None;
    }
    let mut v = core::slice::from_raw_parts((*s).buf, (*s).len as usize).to_vec();
    v.push(0);
    Some(v)
}

unsafe fn counted_w(s: *const Counted<u16>) -> String {
    if s.is_null() || (*s).buf.is_null() {
        return String::new();
    }
    String::from_utf16_lossy(core::slice::from_raw_parts((*s).buf, (*s).len as usize / 2))
}

fn is_system_path(path: &str) -> bool {
    path.to_ascii_lowercase().starts_with(patch::windows_dir())
}

pub unsafe fn proc_override(h: *mut c_void, name: *const u8) -> Option<*mut c_void> {
    if !routes::enabled() || h.is_null() || name.is_null() || (h as usize) & 3 != 0 {
        return None;
    }
    let path = patch::module_path(h);
    if routes::skip_module(&path, h as usize) {
        return None;
    }
    let func = if (name as usize) < 0x10000 { format!("#{}", name as usize) } else { cstr(name) };
    let mut cands = recorded(h);
    if is_system_path(&path) {
        let b = bare(&path);
        if !cands.contains(&b) {
            cands.push(b);
        }
    }
    let build = routes::current_build();
    for c in cands {
        if let Some(r) = routes::lookup(&c, &func, build) {
            let t = routes::target(r);
            if t != 0 {
                return Some(t as *mut c_void);
            }
        }
    }
    None
}

pub fn console_only_lib(name: &str) -> Option<&'static str> {
    let b = bare(name);
    if routes::is_api_set(&b) || routes::shadows_system_dll(&b) {
        return None;
    }
    routes::lib_for(&b, routes::current_build())
}

pub unsafe fn load_override(name: &str, flags: u32) -> Option<*mut c_void> {
    if !routes::enabled() || flags & DATAFILE_FLAGS != 0 || name.is_empty() {
        return None;
    }
    if name.contains(['\\', '/']) && std::path::Path::new(name).is_file() {
        return None;
    }
    let lib = console_only_lib(name)?;
    let h = routes::module(lib);
    if h == 0 {
        return None;
    }
    let add_ref = pc().first[F_GMXW];
    if add_ref == 0 {
        return None;
    }
    let e = GetLastError();
    let mut held: *mut c_void = core::ptr::null_mut();
    if core::mem::transmute::<usize, HandleExW>(add_ref)(FROM_ADDRESS, h as *const u16, &mut held) == 0 || held as usize != h {
        SetLastError(e);
        return None;
    }
    SetLastError(e);
    record(h as *mut c_void, name);
    Some(h as *mut c_void)
}

pub unsafe fn handle_override(name: &str) -> Option<*mut c_void> {
    if !routes::enabled() || name.is_empty() {
        return None;
    }
    let lib = console_only_lib(name)?;
    let h = routes::loaded(lib);
    (h != 0).then_some(h as *mut c_void)
}

pub unsafe fn resolve_proc(h: *mut c_void, name: *const u8) -> *mut c_void {
    proc_override(h, name).unwrap_or(core::ptr::null_mut())
}

pub unsafe fn resolve_load(name: &str, flags: u32) -> *mut c_void {
    load_override(name, flags).unwrap_or(core::ptr::null_mut())
}

pub unsafe fn resolve_handle(name: &str) -> *mut c_void {
    handle_override(name).unwrap_or(core::ptr::null_mut())
}

pub unsafe fn fs_load_override(func: &str, a: &[usize]) -> Option<usize> {
    let (name, flags) = load_args(func, a)?;
    if crate::gdk::fs::translate(&name).is_some_and(|t| std::path::Path::new(&t).is_file()) {
        return None;
    }
    load_override(&name, flags).map(|h| h as usize)
}

pub unsafe fn fs_load_record(func: &str, a: &[usize], r: usize) {
    if let Some((name, _)) = load_args(func, a) {
        record(r as *mut c_void, &name);
    }
}

unsafe fn load_args(func: &str, a: &[usize]) -> Option<(String, u32)> {
    let p = *a.first()?;
    if p == 0 {
        return None;
    }
    match func {
        "LoadLibraryW" => Some((wstr(p as *const u16), 0)),
        "LoadLibraryA" => Some((cstr(p as *const u8), 0)),
        "LoadLibraryExW" => Some((wstr(p as *const u16), *a.get(2)? as u32)),
        "LoadLibraryExA" => Some((cstr(p as *const u8), *a.get(2)? as u32)),
        _ => None,
    }
}

unsafe fn gpa_impl(next: usize, h: *mut c_void, name: *const u8) -> *mut c_void {
    if let Some(p) = proc_override(h, name) {
        return p;
    }
    core::mem::transmute::<usize, Gpa>(next)(h, name)
}

unsafe fn gpac_impl(next: usize, h: *mut c_void, name: *const u8, caller: *mut c_void) -> *mut c_void {
    if let Some(p) = proc_override(h, name) {
        return p;
    }
    core::mem::transmute::<usize, GpaCaller>(next)(h, name, caller)
}

unsafe fn load_a_impl(next: usize, name: *const u8) -> *mut c_void {
    let n = if name.is_null() { String::new() } else { cstr(name) };
    if let Some(h) = load_override(&n, 0) {
        return h;
    }
    let h = core::mem::transmute::<usize, LoadA>(next)(name);
    record(h, &n);
    h
}

unsafe fn load_w_impl(next: usize, name: *const u16) -> *mut c_void {
    let n = if name.is_null() { String::new() } else { wstr(name) };
    if let Some(h) = load_override(&n, 0) {
        return h;
    }
    let h = core::mem::transmute::<usize, LoadW>(next)(name);
    record(h, &n);
    h
}

unsafe fn load_ex_a_impl(next: usize, name: *const u8, file: *mut c_void, flags: u32) -> *mut c_void {
    let n = if name.is_null() { String::new() } else { cstr(name) };
    if let Some(h) = load_override(&n, flags) {
        return h;
    }
    let h = core::mem::transmute::<usize, LoadExA>(next)(name, file, flags);
    if flags & DATAFILE_FLAGS == 0 {
        record(h, &n);
    }
    h
}

unsafe fn load_ex_w_impl(next: usize, name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void {
    let n = if name.is_null() { String::new() } else { wstr(name) };
    if let Some(h) = load_override(&n, flags) {
        return h;
    }
    let h = core::mem::transmute::<usize, LoadExW>(next)(name, file, flags);
    if flags & DATAFILE_FLAGS == 0 {
        record(h, &n);
    }
    h
}

unsafe fn load_pkg_impl(next: usize, name: *const u16, reserved: u32) -> *mut c_void {
    let n = if name.is_null() { String::new() } else { wstr(name) };
    if reserved == 0 {
        if let Some(h) = load_override(&n, 0) {
            return h;
        }
    }
    let h = core::mem::transmute::<usize, LoadPkg>(next)(name, reserved);
    record(h, &n);
    h
}

unsafe fn handle_a_impl(next: usize, name: *const u8) -> *mut c_void {
    let h = core::mem::transmute::<usize, LoadA>(next)(name);
    if h.is_null() && !name.is_null() {
        if let Some(m) = handle_override(&cstr(name)) {
            return m;
        }
    }
    h
}

unsafe fn handle_w_impl(next: usize, name: *const u16) -> *mut c_void {
    let h = core::mem::transmute::<usize, LoadW>(next)(name);
    if h.is_null() && !name.is_null() {
        if let Some(m) = handle_override(&wstr(name)) {
            return m;
        }
    }
    h
}

unsafe fn handle_ex_override(flags: u32, name: &str, out: *mut *mut c_void) -> Option<i32> {
    let m = handle_override(name)?;
    let path = patch::module_path(m);
    if path.is_empty() {
        return None;
    }
    let e = GetLastError();
    let r = core::mem::transmute::<usize, HandleExW>(pc().first[F_GMXW])(flags, wide(&path).as_ptr(), out);
    if r == 0 {
        SetLastError(e);
        return None;
    }
    Some(r)
}

unsafe fn handle_ex_a_impl(next: usize, flags: u32, name: *const u8, out: *mut *mut c_void) -> i32 {
    let r = core::mem::transmute::<usize, HandleExA>(next)(flags, name, out);
    if r != 0 || name.is_null() || out.is_null() || flags & FROM_ADDRESS != 0 {
        return r;
    }
    handle_ex_override(flags, &cstr(name), out).unwrap_or(r)
}

unsafe fn handle_ex_w_impl(next: usize, flags: u32, name: *const u16, out: *mut *mut c_void) -> i32 {
    let r = core::mem::transmute::<usize, HandleExW>(next)(flags, name, out);
    if r != 0 || name.is_null() || out.is_null() || flags & FROM_ADDRESS != 0 {
        return r;
    }
    handle_ex_override(flags, &wstr(name), out).unwrap_or(r)
}

unsafe fn ldr_proc(h: *mut c_void, name: *const Counted<u8>, ord: u32, out: *mut *mut c_void) -> bool {
    if out.is_null() {
        return false;
    }
    let owned = counted_a(name);
    let key: *const u8 = match &owned {
        Some(v) => v.as_ptr(),
        None if ord != 0 && ord < 0x10000 => ord as usize as *const u8,
        None => return false,
    };
    match proc_override(h, key) {
        Some(p) => {
            *out = p;
            true
        }
        None => false,
    }
}

unsafe fn ldr_gpa_impl(next: usize, h: *mut c_void, name: *const Counted<u8>, ord: u32, out: *mut *mut c_void) -> i32 {
    if ldr_proc(h, name, ord, out) {
        return STATUS_SUCCESS;
    }
    core::mem::transmute::<usize, LdrGpa>(next)(h, name, ord, out)
}

unsafe fn ldr_gpax_impl(next: usize, h: *mut c_void, name: *const Counted<u8>, ord: u32, out: *mut *mut c_void, flags: u32) -> i32 {
    if ldr_proc(h, name, ord, out) {
        return STATUS_SUCCESS;
    }
    core::mem::transmute::<usize, LdrGpaEx>(next)(h, name, ord, out, flags)
}

unsafe fn ldr_load_impl(next: usize, path: *const u16, chars: *const u32, name: *const Counted<u16>, out: *mut *mut c_void) -> i32 {
    let n = counted_w(name);
    if !out.is_null() {
        if let Some(h) = load_override(&n, 0) {
            *out = h;
            return STATUS_SUCCESS;
        }
    }
    let s = core::mem::transmute::<usize, LdrDll>(next)(path, chars, name, out);
    if s >= 0 && !out.is_null() {
        record(*out, &n);
    }
    s
}

unsafe fn ldr_handle_impl(next: usize, path: *const u16, chars: *const u32, name: *const Counted<u16>, out: *mut *mut c_void) -> i32 {
    let s = core::mem::transmute::<usize, LdrDll>(next)(path, chars, name, out);
    if s < 0 && !out.is_null() {
        if let Some(m) = handle_override(&counted_w(name)) {
            *out = m;
            return STATUS_SUCCESS;
        }
    }
    s
}

fn first(f: usize) -> usize {
    pc().first[f]
}

unsafe extern "system" fn get_proc_address(h: *mut c_void, name: *const u8) -> *mut c_void {
    gpa_impl(first(F_GPA), h, name)
}

unsafe extern "system" fn get_proc_address_for_caller(h: *mut c_void, name: *const u8, caller: *mut c_void) -> *mut c_void {
    gpac_impl(first(F_GPAC), h, name, caller)
}

unsafe extern "system" fn load_library_a(name: *const u8) -> *mut c_void {
    load_a_impl(first(F_LA), name)
}

unsafe extern "system" fn load_library_w(name: *const u16) -> *mut c_void {
    load_w_impl(first(F_LW), name)
}

unsafe extern "system" fn load_library_ex_a(name: *const u8, file: *mut c_void, flags: u32) -> *mut c_void {
    load_ex_a_impl(first(F_LXA), name, file, flags)
}

unsafe extern "system" fn load_library_ex_w(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void {
    load_ex_w_impl(first(F_LXW), name, file, flags)
}

unsafe extern "system" fn load_packaged_library(name: *const u16, reserved: u32) -> *mut c_void {
    load_pkg_impl(first(F_PKG), name, reserved)
}

unsafe extern "system" fn get_module_handle_a(name: *const u8) -> *mut c_void {
    handle_a_impl(first(F_GMA), name)
}

unsafe extern "system" fn get_module_handle_w(name: *const u16) -> *mut c_void {
    handle_w_impl(first(F_GMW), name)
}

unsafe extern "system" fn get_module_handle_ex_a(flags: u32, name: *const u8, out: *mut *mut c_void) -> i32 {
    handle_ex_a_impl(first(F_GMXA), flags, name, out)
}

unsafe extern "system" fn get_module_handle_ex_w(flags: u32, name: *const u16, out: *mut *mut c_void) -> i32 {
    handle_ex_w_impl(first(F_GMXW), flags, name, out)
}

unsafe extern "system" fn ldr_get_procedure_address(h: *mut c_void, name: *const Counted<u8>, ord: u32, out: *mut *mut c_void) -> i32 {
    ldr_gpa_impl(first(F_LDR_GPA), h, name, ord, out)
}

unsafe extern "system" fn ldr_get_procedure_address_ex(h: *mut c_void, name: *const Counted<u8>, ord: u32, out: *mut *mut c_void, flags: u32) -> i32 {
    ldr_gpax_impl(first(F_LDR_GPAX), h, name, ord, out, flags)
}

unsafe extern "system" fn ldr_load_dll(path: *const u16, chars: *const u32, name: *const Counted<u16>, out: *mut *mut c_void) -> i32 {
    ldr_load_impl(first(F_LDR_LOAD), path, chars, name, out)
}

unsafe extern "system" fn ldr_get_dll_handle(path: *const u16, chars: *const u32, name: *const Counted<u16>, out: *mut *mut c_void) -> i32 {
    ldr_handle_impl(first(F_LDR_HANDLE), path, chars, name, out)
}

fn defaults() -> &'static [usize; NF] {
    static H: OnceLock<[usize; NF]> = OnceLock::new();
    H.get_or_init(|| {
        [
            get_proc_address as *const () as usize,
            load_library_a as *const () as usize,
            load_library_w as *const () as usize,
            load_library_ex_a as *const () as usize,
            load_library_ex_w as *const () as usize,
            load_packaged_library as *const () as usize,
            get_module_handle_a as *const () as usize,
            get_module_handle_w as *const () as usize,
            get_module_handle_ex_a as *const () as usize,
            get_module_handle_ex_w as *const () as usize,
            get_proc_address_for_caller as *const () as usize,
            ldr_get_procedure_address as *const () as usize,
            ldr_get_procedure_address_ex as *const () as usize,
            ldr_load_dll as *const () as usize,
            ldr_get_dll_handle as *const () as usize,
        ]
    })
}

pub fn hook_for(func: &str) -> Option<usize> {
    let f = LOADER_FUNCS.iter().position(|x| *x == func)?;
    Some(defaults()[f])
}

pub fn routes_to(cur: usize, func: &str, target: usize) -> bool {
    if !LOADER_FUNCS.contains(&func) {
        return false;
    }
    match chained_to(cur) {
        Some(n) => n == target,
        None => hook_for(func) == Some(cur) && pc_binding(func).contains(&target),
    }
}

pub fn is_routing_hook(p: usize) -> bool {
    defaults().contains(&p) || chained_to(p).is_some()
}

fn cooperative(p: usize) -> bool {
    p == crate::gdk::hook::get_proc_address_hook as *const () as usize || crate::gdk::fs::loader_hooks().contains(&p)
}

pub fn pc_binding(func: &str) -> Vec<usize> {
    pc().bindings.get(func).cloned().unwrap_or_default()
}

fn seen() -> &'static Mutex<HashSet<usize>> {
    static S: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn forget_range(base: usize, len: usize) {
    seen().lock().unwrap_or_else(|e| e.into_inner()).retain(|s| *s < base || *s >= base + len);
}

pub unsafe fn patch_image(base: *mut u8, skip: &[Hook], hits: &mut Vec<String>) -> usize {
    if !routes::enabled() {
        return 0;
    }
    let mut count = 0;
    for imp in routes::imports(base) {
        if !dll_matches(&imp.dll, LOADER_DLLS) || routes::covered(&imp.dll, &imp.func, skip) {
            continue;
        }
        let Some(f) = LOADER_FUNCS.iter().position(|x| *x == imp.func) else { continue };
        let cur = core::ptr::read_volatile(imp.slot);
        if cur == 0 || is_routing_hook(cur) || cooperative(cur) {
            continue;
        }
        let key = imp.slot as usize;
        let (hook, chained) = if pc_binding(&imp.func).contains(&cur) {
            (defaults()[f], false)
        } else {
            if seen().lock().unwrap_or_else(|e| e.into_inner()).contains(&key) {
                continue;
            }
            match make_stub(f, cur) {
                Some(s) => (s, true),
                None => {
                    hits.push(format!("{}!{} -> foreign hook {cur:#x} kept, no stub memory", imp.dll, imp.func));
                    continue;
                }
            }
        };
        if patch::write_slot(imp.slot, hook) {
            seen().lock().unwrap_or_else(|e| e.into_inner()).insert(key);
            count += 1;
            hits.push(if chained { format!("{}!{} -> routed loader, chained to foreign hook {cur:#x}", imp.dll, imp.func) } else { format!("{}!{} -> routed loader", imp.dll, imp.func) });
        }
    }
    count
}
