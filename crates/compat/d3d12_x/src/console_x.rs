use compat_common::{record, E_NOTIMPL};
use core::ffi::c_void;
use core::sync::atomic::Ordering;

const M: &str = "d3d12_x";
pub const DXGI_ERROR_UNSUPPORTED: i32 = 0x887a_0004u32 as i32;
pub const E_INVALIDARG: i32 = 0x8007_0057u32 as i32;
pub const HRESULT_FROM_NT_SUCCESS: i32 = 0x1000_0000;
const MAX_PAGE_TABLE_PAGES: u32 = 0x21;
const FLAG_MASK: u32 = !0b101;
const NO_FRAME_BUFFER_PRESERVE: u32 = 4;
const FRAME_BUFFER_PRESERVE_BYTES: u32 = 0xa0_0000;

extern "system" {
    fn GetModuleHandleA(name: *const u8) -> *mut c_void;
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(h: *mut c_void, name: *const u8) -> *mut c_void;
    fn OutputDebugStringA(s: *const u8);
    fn FlushProcessWriteBuffers();
}

#[repr(C)]
pub struct VirtualMemoryConfiguration {
    pub flags: u32,
    pub page_table_pages: u32,
    pub output0: u64,
    pub output1: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsMemoryConfiguration {
    pub page_table_bytes: u32,
    pub frame_buffer_preserve_bytes: u32,
    pub output0: u64,
    pub output1: u64,
}

pub fn graphics_config(flags: u32, pages: u32) -> GraphicsMemoryConfiguration {
    GraphicsMemoryConfiguration {
        page_table_bytes: pages << 22,
        frame_buffer_preserve_bytes: if flags & NO_FRAME_BUFFER_PRESERVE != 0 { 0 } else { FRAME_BUFFER_PRESERVE_BYTES },
        output0: 0,
        output1: 0,
    }
}

unsafe fn register_with_xmem(cfg: &GraphicsMemoryConfiguration) -> bool {
    let mut h = GetModuleHandleA(b"xmem.dll\0".as_ptr());
    if h.is_null() {
        h = LoadLibraryA(b"xmem.dll\0".as_ptr());
    }
    if h.is_null() {
        return false;
    }
    let f = GetProcAddress(h, b"XMemRegisterGraphicsMemoryConfiguration\0".as_ptr());
    if f.is_null() {
        return false;
    }
    let f: unsafe extern "system" fn(*const GraphicsMemoryConfiguration) = core::mem::transmute(f);
    f(cfg);
    true
}

#[no_mangle]
pub unsafe extern "system" fn D3DConfigureVirtualMemory(desc: *mut VirtualMemoryConfiguration) -> i32 {
    if desc.is_null() || (*desc).flags & FLAG_MASK != 0 {
        return E_INVALIDARG;
    }
    let d = &mut *desc;
    if d.page_table_pages > MAX_PAGE_TABLE_PAGES {
        let msg = format!(
            "ERROR: Can only support a maximum of {} MB ({} 4MB pages) for graphics page tables.\n\0",
            MAX_PAGE_TABLE_PAGES * 4,
            MAX_PAGE_TABLE_PAGES
        );
        OutputDebugStringA(msg.as_ptr());
        return E_INVALIDARG;
    }
    d.output0 = 0;
    d.output1 = 0;
    let cfg = graphics_config(d.flags, d.page_table_pages);
    let registered = register_with_xmem(&cfg);
    record(M, "D3DConfigureVirtualMemory", &format!("flags={:#x} pages={} -> {cfg:?} xmem={registered}", d.flags, d.page_table_pages));
    HRESULT_FROM_NT_SUCCESS
}

#[no_mangle]
pub extern "system" fn D3DAllocateGraphicsMemory(_a: usize, _b: usize, _c: usize, _d: usize, _e: usize) -> i32 {
    E_NOTIMPL
}

#[no_mangle]
pub extern "system" fn D3DFreeGraphicsMemory(_a: usize) -> i32 {
    E_NOTIMPL
}

#[no_mangle]
pub extern "system" fn D3DFlushEntireCpuCache() {
    core::sync::atomic::fence(Ordering::SeqCst);
    unsafe { FlushProcessWriteBuffers() };
}

#[no_mangle]
pub extern "system" fn D3D12XboxUnloadDriverModule() {
    let h = unsafe { GetModuleHandleA(b"d3d12_xs.dll\0".as_ptr()) };
    if h.is_null() {
        return;
    }
    let p = unsafe { GetProcAddress(h, b"D3D12XboxUnloadDriverModule\0".as_ptr()) };
    if p.is_null() || p as usize == D3D12XboxUnloadDriverModule as usize {
        return;
    }
    let f: extern "system" fn() = unsafe { core::mem::transmute(p) };
    f();
    record(M, "D3D12XboxUnloadDriverModule", "driver module state lives in the d3d12_xs runtime that created the device; its unload ran");
}

#[no_mangle]
pub extern "system" fn DXGIXGetFrameStatistics(_a: u32, _b: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXPresentArray(_a: u32, _b: u32, _c: u32, _d: u32, _e: *const c_void, _f: *const c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXSetFrameNotification(_a: u32, _b: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[no_mangle]
pub extern "system" fn DXGIXSetVLineNotification(_a: u32, _b: u32, _c: *mut c_void) -> i32 {
    DXGI_ERROR_UNSUPPORTED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configure_virtual_memory_matches_console_rules() {
        unsafe {
            assert_eq!(D3DConfigureVirtualMemory(core::ptr::null_mut()), E_INVALIDARG);
            for bad in [2u32, 8, 0x10, 0x8000_0000] {
                let mut d = VirtualMemoryConfiguration { flags: bad, page_table_pages: 1, output0: 7, output1: 7 };
                assert_eq!(D3DConfigureVirtualMemory(&mut d), E_INVALIDARG, "flags {bad:#x}");
                assert_eq!((d.output0, d.output1), (7, 7));
            }
            let mut d = VirtualMemoryConfiguration { flags: 5, page_table_pages: 0x22, output0: 7, output1: 7 };
            assert_eq!(D3DConfigureVirtualMemory(&mut d), E_INVALIDARG);
            d.page_table_pages = 0x21;
            assert_eq!(D3DConfigureVirtualMemory(&mut d), HRESULT_FROM_NT_SUCCESS);
            assert_eq!((d.output0, d.output1), (0, 0));
        }
    }

    #[test]
    fn frame_buffer_preserve_follows_flag_bit_2() {
        assert_eq!(graphics_config(0, 2), GraphicsMemoryConfiguration { page_table_bytes: 0x80_0000, frame_buffer_preserve_bytes: 0xa0_0000, output0: 0, output1: 0 });
        assert_eq!(graphics_config(1, 2).frame_buffer_preserve_bytes, 0xa0_0000);
        assert_eq!(graphics_config(4, 0x21), GraphicsMemoryConfiguration { page_table_bytes: 0x21 << 22, frame_buffer_preserve_bytes: 0, output0: 0, output1: 0 });
    }

    #[test]
    fn graphics_memory_entry_points_return_console_e_notimpl() {
        assert_eq!(D3DAllocateGraphicsMemory(0x1000, 0x10000, 0, 0, 0), E_NOTIMPL);
        assert_eq!(D3DFreeGraphicsMemory(0x1000), E_NOTIMPL);
    }

    #[test]
    fn dxgix_entry_points_return_console_unsupported() {
        assert_eq!(DXGIXGetFrameStatistics(0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXPresentArray(0, 0, 0, 0, core::ptr::null(), core::ptr::null()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXSetFrameNotification(0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
        assert_eq!(DXGIXSetVLineNotification(0, 0, core::ptr::null_mut()), DXGI_ERROR_UNSUPPORTED);
    }

    #[test]
    fn flush_and_unload_are_callable_repeatedly() {
        use std::sync::atomic::{AtomicBool, AtomicU64};
        static V: AtomicU64 = AtomicU64::new(0);
        static R: AtomicBool = AtomicBool::new(false);
        let t = std::thread::spawn(|| {
            while !R.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
            V.load(Ordering::Relaxed)
        });
        V.store(0xbeef, Ordering::Relaxed);
        D3DFlushEntireCpuCache();
        R.store(true, Ordering::Relaxed);
        assert_eq!(t.join().unwrap(), 0xbeef);
        D3D12XboxUnloadDriverModule();
        D3D12XboxUnloadDriverModule();
    }
}

#[cfg(test)]
mod console_fixed_tests {
    #[path = "../../../../console-dll/oracle/leaf.rs"]
    mod leaf;

    #[test]
    fn fixed_result_exports_match_every_console_d3d12_x() {
        let rows = [
            ("D3DAllocateGraphicsMemory", super::D3DAllocateGraphicsMemory as *const () as usize, 0xffff_ffff, 0x8000_4001),
            ("D3DFreeGraphicsMemory", super::D3DFreeGraphicsMemory as *const () as usize, 0xffff_ffff, 0x8000_4001),
            ("DXGIXGetFrameStatistics", super::DXGIXGetFrameStatistics as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXPresentArray", super::DXGIXPresentArray as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXSetFrameNotification", super::DXGIXSetFrameNotification as *const () as usize, 0xffff_ffff, 0x887a_0004),
            ("DXGIXSetVLineNotification", super::DXGIXSetVLineNotification as *const () as usize, 0xffff_ffff, 0x887a_0004),
        ];
        let seen = leaf::check_fixed("d3d12_x.dll", &rows);
        assert!(seen.len() >= 4, "{seen:?}");
    }
}
