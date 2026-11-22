use core::ffi::c_void;

#[repr(C)]
pub struct Allocator {
    pub ctx: *mut c_void,
    pub alloc: Option<unsafe extern "system" fn(*mut c_void, usize) -> *mut c_void>,
    pub free: Option<unsafe extern "system" fn(*mut c_void, *mut c_void)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Desc {
    pub memory: *mut c_void,
    pub memory_size: u32,
    pub flags: u32,
    pub reserved: [u64; 3],
}

pub type CreateDeviceFn = unsafe extern "system-unwind" fn(*const Allocator, *mut c_void, *const Desc) -> *mut c_void;

pub const EMULATED_IMPORTS: &[&str] = &["xmem.dll"];

pub unsafe fn create_device(console: &console_dll::Console, alloc: *const Allocator, device: *mut c_void, desc: *const Desc) -> *mut c_void {
    match console.proc("XDXRCreateDevice") {
        Some(p) => {
            let f: CreateDeviceFn = core::mem::transmute(p);
            f(alloc, device, desc)
        }
        None => {
            console.missing("XDXRCreateDevice", "NULL");
            core::ptr::null_mut()
        }
    }
}
