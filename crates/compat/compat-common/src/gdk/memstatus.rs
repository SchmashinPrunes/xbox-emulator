use core::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> usize;
}

const WS_VERSION_2_TITLE: u32 = 0x1000_0001;
const STATUS_SIZE: u32 = 64;

static ORIG_GMSE: AtomicUsize = AtomicUsize::new(0);
static WS_FN: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TitleView {
    pub total: u64,
    pub used: u64,
}

fn working_set_fn() -> usize {
    let f = WS_FN.load(Ordering::Acquire);
    if f != 0 {
        return f;
    }
    let f = unsafe {
        let h = GetModuleHandleA(b"xmem.dll\0".as_ptr());
        if h.is_null() {
            return 0;
        }
        GetProcAddress(h, b"XMemGetWorkingSetStatistics\0".as_ptr())
    };
    WS_FN.store(f, Ordering::Release);
    f
}

pub fn title_view() -> Option<TitleView> {
    let f = working_set_fn();
    if f == 0 {
        return None;
    }
    let mut buf = [0u8; 0x50];
    let ok = unsafe { core::mem::transmute::<usize, unsafe extern "system" fn(u32, *mut u8) -> i32>(f)(WS_VERSION_2_TITLE, buf.as_mut_ptr()) };
    let total = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let used = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    if ok == 0 || total == 0 {
        return None;
    }
    Some(TitleView { total, used: used.min(total) })
}

pub unsafe fn shape(status: *mut u8, v: TitleView) {
    let avail = v.total.saturating_sub(v.used);
    let load = ((v.used as u128 * 100 + v.total as u128 / 2) / v.total as u128) as u32;
    core::ptr::write_unaligned(status.add(4) as *mut u32, load.min(100));
    core::ptr::write_unaligned(status.add(8) as *mut u64, v.total);
    core::ptr::write_unaligned(status.add(16) as *mut u64, avail);
    core::ptr::write_unaligned(status.add(24) as *mut u64, v.total);
    core::ptr::write_unaligned(status.add(32) as *mut u64, avail);
}

unsafe extern "system" fn global_memory_status_ex_hook(status: *mut u8) -> i32 {
    let f: unsafe extern "system" fn(*mut u8) -> i32 = core::mem::transmute(ORIG_GMSE.load(Ordering::Acquire));
    let r = f(status);
    if r == 0 || status.is_null() || core::ptr::read_unaligned(status as *const u32) < STATUS_SIZE {
        return r;
    }
    if let Some(v) = title_view() {
        shape(status, v);
        static ONCE: OnceLock<()> = OnceLock::new();
        ONCE.get_or_init(|| super::log("GlobalMemoryStatusEx", &format!("title sees partition total {:#x}, used {:#x}, no page file (commit limit = title memory)", v.total, v.used)));
    }
    r
}

pub fn targets() -> Vec<(&'static str, usize, &'static AtomicUsize)> {
    if super::setting("console_memstatus").as_deref() == Some("0") {
        return Vec::new();
    }
    vec![("GlobalMemoryStatusEx", global_memory_status_ex_hook as usize, &ORIG_GMSE)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reports_title_partition_without_page_file() {
        let mut s = [0xccu8; 64];
        s[0..4].copy_from_slice(&64u32.to_le_bytes());
        let host_virtual = [0x11u8; 24];
        s[40..64].copy_from_slice(&host_virtual);
        let v = TitleView { total: 0x3_6000_0000, used: 0x1_b000_0000 };
        unsafe { shape(s.as_mut_ptr(), v) };
        let q = |o: usize| u64::from_le_bytes(s[o..o + 8].try_into().unwrap());
        assert_eq!(u32::from_le_bytes(s[0..4].try_into().unwrap()), 64);
        assert_eq!(u32::from_le_bytes(s[4..8].try_into().unwrap()), 50);
        assert_eq!(q(8), 0x3_6000_0000);
        assert_eq!(q(16), 0x1_b000_0000);
        assert_eq!(q(24), q(8));
        assert_eq!(q(32), q(16));
        assert_eq!(&s[40..64], &host_virtual);
    }

    #[test]
    fn full_partition_reports_zero_available() {
        let mut s = [0u8; 64];
        unsafe { shape(s.as_mut_ptr(), TitleView { total: 0x3_6000_0000, used: 0x3_6000_0000 }) };
        assert_eq!(u32::from_le_bytes(s[4..8].try_into().unwrap()), 100);
        assert_eq!(u64::from_le_bytes(s[16..24].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(s[32..40].try_into().unwrap()), 0);
    }

    #[test]
    fn hook_is_offered_for_titles() {
        let names: Vec<&str> = targets().iter().map(|t| t.0).collect();
        assert_eq!(names, ["GlobalMemoryStatusEx"]);
    }
}
