#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

use core::ffi::c_void;

#[test]
fn e_notimpl_link_exports_match_every_console_xbtp() {
    let rows = [
        ("XbtpLinkGetServiceVersion", xbtp::link::XbtpLinkGetServiceVersion as *const () as usize, 0xffff_ffff, 0x8000_4001),
        ("XbtpLinkSetServiceOptions", xbtp::link::XbtpLinkSetServiceOptions as *const () as usize, 0xffff_ffff, 0x8000_4001),
        ("XbtpLinkQueryStatistics", xbtp::link::XbtpLinkQueryStatistics as *const () as usize, 0xffff_ffff, 0x8000_4001),
        ("XbtpLinkQueryPacketLog", xbtp::link::XbtpLinkQueryPacketLog as *const () as usize, 0xffff_ffff, 0x8000_4001),
        ("XbtpLinkSetupRoutingLog", xbtp::link::XbtpLinkSetupRoutingLog as *const () as usize, 0xffff_ffff, 0x8000_4001),
    ];
    let seen = leaf::check_fixed("xbtp.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}

#[test]
fn get_service_location_clears_the_caller_buffer_like_every_console_xbtp() {
    type F = unsafe extern "C" fn(*mut u16, *mut u32) -> i32;
    let images = leaf::images("xbtp.dll");
    assert!(images.len() >= 4);
    let run = |f: F, len: u32| {
        let mut buf = [0x4141u16; 8];
        let mut n = len;
        let (hr, le) = leaf::with_sentinel(|| unsafe { f(buf.as_mut_ptr(), &mut n) });
        (hr, le, buf, n)
    };
    for img in &images {
        let theirs: F = unsafe { core::mem::transmute(leaf::export(img, "XbtpLinkGetServiceLocation")) };
        for len in [0u32, 1, 8, u32::MAX] {
            let mine = run(xbtp::link::XbtpLinkGetServiceLocation, len);
            assert_eq!(mine.0 as u32, 0x8000_4001);
            assert_eq!(mine.1, leaf::SENTINEL);
            assert_eq!(mine.3, 0);
            assert_eq!(mine.2[0], if len == 0 { 0x4141 } else { 0 });
            assert!(mine.2[1..].iter().all(|c| *c == 0x4141));
            assert_eq!(run(theirs, len), mine, "len {len}: {}", img.source.display());
        }
    }
}

#[test]
fn open_xtc_session_nulls_the_out_handle_like_every_console_xbtp() {
    type F = unsafe extern "C" fn(u64, *const [u8; 16], *const [u8; 16], *mut *mut c_void) -> i32;
    let ours: F = unsafe { core::mem::transmute(xbtp::xbtp2_sys::XbtpOpenXtcSession as *const ()) };
    let name = "?XbtpOpenXtcSession@@YAJ_KAEBU_GUID@@1PEAPEAX@Z";
    let images = leaf::images("xbtp.dll");
    assert!(images.len() >= 4);
    let g = [0x11u8; 16];
    for img in &images {
        let theirs: F = unsafe { core::mem::transmute(leaf::export(img, name)) };
        for id in [0u64, 0xfeed_face_cafe_beef] {
            let mut a = 0x1234usize as *mut c_void;
            let mut b = 0x1234usize as *mut c_void;
            let (ha, la) = leaf::with_sentinel(|| unsafe { ours(id, &g, &g, &mut a) });
            let (hb, lb) = leaf::with_sentinel(|| unsafe { theirs(id, &g, &g, &mut b) });
            assert_eq!((ha as u32, la, a), (0x8000_4001, leaf::SENTINEL, core::ptr::null_mut()));
            assert_eq!((hb, lb, b), (ha, la, a), "{}", img.source.display());
        }
    }
}
