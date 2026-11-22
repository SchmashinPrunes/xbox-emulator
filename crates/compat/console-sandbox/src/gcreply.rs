use crate::gchost::{DISPLAY_MODE_1, DISPLAY_MODE_2, PROXY_APP};
use compat_common::gdk::setting;

pub const PROXY_REQUEST_LEN: usize = 0x24;
pub const DISPLAY_MODE_1_LEN: usize = 0x14;
pub const DISPLAY_MODE_2_LEN: usize = 0x35;
pub const PROXY_REPLY_LEN: usize = 0x10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
    pub hdr: bool,
}

#[repr(C)]
struct DevMode {
    device_name: [u16; 32],
    spec_version: u16,
    driver_version: u16,
    size: u16,
    driver_extra: u16,
    fields: u32,
    position: [i32; 2],
    orientation: u32,
    fixed_output: u32,
    color: i16,
    duplex: i16,
    y_resolution: i16,
    tt_option: i16,
    collate: i16,
    form_name: [u16; 32],
    log_pixels: u16,
    bits_per_pel: u32,
    pels_width: u32,
    pels_height: u32,
    display_flags: u32,
    display_frequency: u32,
    rest: [u32; 8],
}

#[link(name = "user32")]
extern "system" {
    fn EnumDisplaySettingsW(device: *const u16, mode: u32, out: *mut DevMode) -> i32;
}

fn monitor() -> Option<(u32, u32, u32)> {
    let mut m: DevMode = unsafe { core::mem::zeroed() };
    m.size = core::mem::size_of::<DevMode>() as u16;
    let ok = unsafe { EnumDisplaySettingsW(core::ptr::null(), 0xFFFF_FFFF, &mut m) };
    (ok != 0 && m.pels_width != 0 && m.pels_height != 0).then(|| (m.pels_width, m.pels_height, m.display_frequency))
}

fn num(key: &str) -> Option<u32> {
    setting(key).and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0).map(|v| v as u32)
}

pub fn display() -> Display {
    let (mw, mh, mf) = monitor().unwrap_or((1920, 1080, 60));
    let width = num("display_width").unwrap_or(mw);
    let height = num("display_height").unwrap_or(mh);
    let refresh_mhz = setting("display_refresh").and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0).map(|v| (v * 1000.0).round() as u32).unwrap_or(if mf > 1 { mf * 1000 } else { 60_000 });
    Display { width, height, refresh_mhz, hdr: compat_common::gdk::basic::hdr_active() }
}

fn put(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

pub const FLAG_120HZ: u32 = 1;
pub const FLAG_40HZ: u32 = 2;

pub fn display_mode_1_flags(d: &Display) -> u32 {
    if d.refresh_mhz >= 119_000 {
        FLAG_120HZ | FLAG_40HZ
    } else {
        0
    }
}

pub fn display_mode_1(key: &[u8], d: &Display) -> Vec<u8> {
    let mut v = key[..8].to_vec();
    v.extend_from_slice(&0f32.to_le_bytes());
    v.extend_from_slice(&0f32.to_le_bytes());
    put(&mut v, display_mode_1_flags(d));
    v
}

pub fn display_mode_2(key: &[u8], d: &Display) -> Vec<u8> {
    let mut v = key[..8].to_vec();
    put(&mut v, d.width);
    put(&mut v, d.height);
    v.push(d.hdr as u8);
    v.extend_from_slice(&[0; 3]);
    for x in [d.refresh_mhz, 1000, d.width, d.height, d.refresh_mhz, 1000, d.width, d.height] {
        put(&mut v, x);
    }
    v.push(0);
    v
}

pub fn proxy_app(key: &[u8]) -> Vec<u8> {
    let mut v = key[..8].to_vec();
    put(&mut v, 0);
    put(&mut v, 0);
    v
}

pub fn reply(kind: u32, request: &[u8]) -> Option<Vec<u8>> {
    if request.len() < 8 {
        return None;
    }
    match kind {
        DISPLAY_MODE_1 if request.len() == 8 => Some(display_mode_1(request, &display())),
        DISPLAY_MODE_2 if request.len() == 8 => Some(display_mode_2(request, &display())),
        PROXY_APP if request.len() == PROXY_REQUEST_LEN => Some(proxy_app(request)),
        _ => None,
    }
}
