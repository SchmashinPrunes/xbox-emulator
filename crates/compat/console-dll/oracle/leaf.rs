#![allow(dead_code)]

use core::ffi::c_void;
use std::path::{Path, PathBuf};

extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const u8) -> *mut c_void;
    fn GetLastError() -> u32;
    fn SetLastError(e: u32);
    fn FreeLibrary(h: *mut c_void) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn GetExitCodeProcess(h: *mut c_void, code: *mut u32) -> i32;
    fn CloseHandle(h: *mut c_void) -> i32;
}

extern "C" {
    fn atexit(f: extern "C" fn()) -> i32;
}

static LOADED: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

fn work_dir(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("tombox_leaf_oracle_{pid}"))
}

extern "C" fn cleanup() {
    let bases = std::mem::take(&mut *LOADED.lock().unwrap_or_else(|e| e.into_inner()));
    for b in bases {
        unsafe { FreeLibrary(b as *mut c_void) };
    }
    let _ = std::fs::remove_dir_all(work_dir(std::process::id()));
}

fn alive(pid: u32) -> bool {
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h.is_null() {
        return false;
    }
    let mut code = 0u32;
    let ok = unsafe { GetExitCodeProcess(h, &mut code) } != 0;
    unsafe { CloseHandle(h) };
    ok && code == 259
}

fn sweep_stale() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(pid) = name.strip_prefix("tombox_leaf_oracle_").and_then(|p| p.parse::<u32>().ok()) {
            if pid != std::process::id() && !alive(pid) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
}

const GAMES: &str = r"D:\xbox-pc-port\games";
const SYSTEM32: &str = r"EmbeddedXvd\Windows\System32";
const DONT_RESOLVE_DLL_REFERENCES: u32 = 1;
pub const SENTINEL: u32 = 0x5EED_0BAD;

#[derive(Clone)]
pub struct Image {
    pub source: PathBuf,
    pub base: usize,
}

fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub fn dumps(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(titles) = std::fs::read_dir(GAMES) else { return out };
    for t in titles.flatten().map(|e| e.path()) {
        let ext = t.join("extracted");
        let direct = ext.join(SYSTEM32).join(name);
        if direct.is_file() {
            out.push(direct);
            continue;
        }
        if let Ok(inner) = std::fs::read_dir(&ext) {
            out.extend(inner.flatten().map(|e| e.path().join(SYSTEM32).join(name)).filter(|p| p.is_file()));
        }
    }
    out.sort();
    out
}

pub fn images(name: &str) -> Vec<Image> {
    static CACHE: std::sync::Mutex<Vec<(String, Vec<Image>)>> = std::sync::Mutex::new(Vec::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = cache.iter().find(|(n, _)| n == name) {
        return v.clone();
    }
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        sweep_stale();
        unsafe { atexit(cleanup) };
    });
    let dir = work_dir(std::process::id());
    std::fs::create_dir_all(&dir).unwrap();
    let stem = name.trim_end_matches(".dll");
    let v: Vec<Image> = dumps(name)
        .into_iter()
        .enumerate()
        .map(|(i, source)| {
            let copy = dir.join(format!("oracle_{stem}_{i}.dll"));
            std::fs::copy(&source, &copy).unwrap();
            let base = unsafe { LoadLibraryExW(wide(&copy).as_ptr(), core::ptr::null_mut(), DONT_RESOLVE_DLL_REFERENCES) } as usize;
            assert!(base != 0, "console {} did not map", source.display());
            LOADED.lock().unwrap_or_else(|e| e.into_inner()).push(base);
            Image { source, base }
        })
        .collect();
    cache.push((name.to_string(), v.clone()));
    v
}

pub fn try_export(img: &Image, func: &str) -> Option<usize> {
    let c = format!("{func}\0");
    let p = unsafe { GetProcAddress(img.base as *mut c_void, c.as_ptr()) } as usize;
    (p != 0).then_some(p)
}

pub fn export(img: &Image, func: &str) -> usize {
    try_export(img, func).unwrap_or_else(|| panic!("{func} not exported by {}", img.source.display()))
}

pub fn with_sentinel<R>(f: impl FnOnce() -> R) -> (R, u32) {
    unsafe { SetLastError(SENTINEL) };
    let r = f();
    (r, unsafe { GetLastError() })
}

pub type Any6 = unsafe extern "system" fn(usize, usize, usize, usize, usize, usize) -> u64;

pub const ARGS: [[usize; 6]; 2] = [[0; 6], [1, 0x1000, usize::MAX, 0xdead_beef, 7, 0x8000_0000]];

pub fn check_fixed(dll: &str, rows: &[(&str, usize, u64, u64)]) -> Vec<PathBuf> {
    let images = images(dll);
    assert!(!images.is_empty(), "no console {dll} in {GAMES}");
    let mut compared = Vec::new();
    for img in &images {
        for &(name, ours, mask, want) in rows {
            let mine: Any6 = unsafe { core::mem::transmute(ours) };
            let theirs: Any6 = unsafe { core::mem::transmute(export(img, name)) };
            for a in ARGS {
                let (m, mle) = with_sentinel(|| unsafe { mine(a[0], a[1], a[2], a[3], a[4], a[5]) } & mask);
                let (t, tle) = with_sentinel(|| unsafe { theirs(a[0], a[1], a[2], a[3], a[4], a[5]) } & mask);
                assert_eq!((m, mle), (want, SENTINEL), "{name}: emulator");
                assert_eq!((t, tle), (m, mle), "{name}: console {}", img.source.display());
            }
        }
        compared.push(img.source.clone());
    }
    compared
}
