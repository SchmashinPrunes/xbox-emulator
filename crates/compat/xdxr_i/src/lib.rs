#![allow(non_snake_case, clippy::missing_safety_doc)]

#[path = "../../xdxr/src/device.rs"]
pub mod device;

use console_dll::{Console, Spec};
use core::ffi::c_void;
pub use device::*;

pub static CONSOLE: Console = Console::new(Spec {
    name: "xdxr_i.dll",
    env: "TOMBOX_XDXR_I_CONSOLE",
    alias: "xdxr_i_console.dll",
    probe: "XDXRCreateDevice",
    emulated_imports: EMULATED_IMPORTS,
});

#[no_mangle]
pub unsafe extern "system-unwind" fn XDXRCreateDevice(alloc: *const Allocator, device: *mut c_void, desc: *const Desc) -> *mut c_void {
    create_device(&CONSOLE, alloc, device, desc)
}
