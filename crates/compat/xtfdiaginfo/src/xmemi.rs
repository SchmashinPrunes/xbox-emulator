use crate::sys::*;
use core::ffi::c_void;
use std::sync::OnceLock;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct XMemBasicInformation {
    pub base: u64,
    pub alloc_base: u64,
    pub alloc_protect: u32,
    pub reserved0: u32,
    pub region_size: u64,
    pub state: u32,
    pub protect: u32,
    pub kind: u32,
    pub reserved1: u32,
    pub xmem_flags: u64,
    pub tail: [u64; 1],
}

pub type VirtualQueryFn = unsafe extern "system" fn(HANDLE, *const c_void, *mut XMemBasicInformation) -> i32;
pub type WorkingSetFn = unsafe extern "system" fn(u32, *mut c_void) -> i32;

#[derive(Clone, Copy)]
pub struct XMem {
    pub virtual_query: VirtualQueryFn,
    pub get_working_set_statistics: WorkingSetFn,
}

static XMEM: OnceLock<Result<XMem, HRESULT>> = OnceLock::new();

#[cfg(not(test))]
fn load() -> Result<XMem, HRESULT> {
    unsafe {
        let w = wz("xmem.dll");
        let mut m = GetModuleHandleW(w.as_ptr());
        if m.is_null() {
            m = LoadLibraryExW(w.as_ptr(), core::ptr::null_mut(), 0);
        }
        if m.is_null() {
            return Err(hr_last_error());
        }
        let vq = GetProcAddress(m, b"XMemVirtualQuery\0".as_ptr());
        if vq.is_null() {
            return Err(hr_last_error());
        }
        let ws = GetProcAddress(m, b"XMemGetWorkingSetStatistics\0".as_ptr());
        if ws.is_null() {
            return Err(hr_last_error());
        }
        Ok(XMem { virtual_query: core::mem::transmute(vq), get_working_set_statistics: core::mem::transmute(ws) })
    }
}

#[cfg(test)]
fn load() -> Result<XMem, HRESULT> {
    Ok(crate::tests::test_xmem())
}

pub fn get() -> Result<&'static XMem, HRESULT> {
    match XMEM.get_or_init(load) {
        Ok(x) => Ok(x),
        Err(e) => Err(*e),
    }
}
