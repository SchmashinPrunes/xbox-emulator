use compat_common::gdk::{self, package, system, ui, user, xasync, Guid};
use core::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[repr(C)]
struct Block {
    queue: usize,
    context: usize,
    callback: usize,
    internal: [u8; 32],
}

fn env() {
    std::env::set_var("TOMBOX_GDK_XASYNC", "emulated");
    std::env::set_var("TOMBOX_CAPTURE_HISTORY", "0");
    std::env::set_var("TOMBOX_GDK_ROOT", std::env::temp_dir().join(format!("tombox_gdk_mc_{}", std::process::id())));
    assert!(gdk::rt::active());
}

unsafe fn method(clsid: Guid, iid: Guid, slot: usize) -> (*mut c_void, usize) {
    let mut obj: *mut c_void = core::ptr::null_mut();
    assert_eq!(gdk::query_api_impl(&clsid, &iid, &mut obj), 0);
    (obj, *(*(obj as *const *const usize)).add(slot))
}

#[test]
fn minecraft_call_sites_are_served() {
    env();
    let g = |s: &str| Guid::parse(s);
    let want: &[(&str, &str, &[usize])] = &[
        ("0a2192aa-b2d5-4d58-83be-383b6d80799e", "5f5e5169-746c-4001-ad1c-da728d01c9eb", &[3, 6, 7, 8, 12, 13, 17]),
        ("073b7dcb-1fcf-4030-94be-e3c9eb623428", "073b7dcb-1fcf-4030-94be-e3c9eb623428", &[23, 24, 26]),
        ("dfcd4649-4ff8-4043-ba07-35d607df98b0", "fc0b9bd0-8898-4643-bee0-4ee6ee034cb6", &[17, 18, 19]),
        ("af406016-e850-4aa8-a88d-2f3dcb9dac7e", "3720de07-e8e4-44a3-ad32-b359e8adbe55", &[11]),
        ("f4faf4d4-2d04-4fce-b3e0-474a713a3e84", "d29411df-0794-4553-8b27-95fc02d0f75d", &[6, 7]),
        ("e349bd1a-fc20-4e40-b99c-4178cc6b409f", "6fd71f09-7513-49f0-89bc-bfaf5df6f852", &[5]),
        ("01acd177-91f9-4763-a38e-ccbb55ce32e0", "eb9bf948-18dc-4d82-bbcc-40e0a809c4c0", &[13]),
    ];
    let have = gdk::implemented();
    for (c, i, slots) in want {
        let row = have.iter().find(|r| r.0 == g(c) && r.1 == g(i)).unwrap_or_else(|| panic!("{i} not registered"));
        for s in *slots {
            assert!(row.3.contains(s), "{} slot {s} not served", row.2);
        }
    }
}

#[test]
fn shared_vtable_iids_reuse_the_same_slot_functions() {
    env();
    unsafe {
        for s in [17, 18, 19] {
            assert_eq!(method(ui::CLSID, ui::IID_FC0B, s).1, method(ui::CLSID, ui::IID_V1, s).1);
        }
        assert_eq!(method(user::CLSID, user::IID_V1, 13).1, method(user::CLSID, user::IID_V2, 13).1);
        let (o, guest) = method(user::CLSID, user::IID_V1, 13);
        let guest: unsafe extern "system" fn(*mut c_void, usize, *mut bool) -> i32 = core::mem::transmute(guest);
        let mut b = true;
        assert_eq!(guest(o, user::handle(), &mut b), 0);
        assert!(!b);
        assert_eq!(guest(o, 0, &mut b), 0x8007_0057u32 as i32);

        let locale = |iid: Guid| {
            let (o, f) = method(package::CLSID, iid, 11);
            let f: unsafe extern "system" fn(*mut c_void, usize, *mut u8) -> i32 = core::mem::transmute(f);
            let mut buf = [0u8; 86];
            assert_eq!(f(o, buf.len(), buf.as_mut_ptr()), 0);
            buf
        };
        assert_eq!(locale(package::IID_3720), locale(package::IID_V2));
    }
}

#[test]
fn xsystem_6fd71f09_layout_matches_the_console_vtable() {
    env();
    unsafe {
        let call = |slot: usize| {
            let (o, f) = method(system::XSYSTEM, system::XSYSTEM_V3, slot);
            let f: unsafe extern "system" fn(*mut c_void, usize, *mut u8, *mut usize) -> i32 = core::mem::transmute(f);
            let mut buf = [0u8; 128];
            let mut used = 0usize;
            assert_eq!(f(o, buf.len(), buf.as_mut_ptr(), &mut used), 0);
            String::from_utf8_lossy(&buf[..used.saturating_sub(1)]).into_owned()
        };
        assert_eq!(call(3), system::console_id());
        assert_eq!(call(4), system::sandbox_id());
        assert_eq!(call(5), system::app_device_id());
        assert_eq!(method(system::XSYSTEM, system::XSYSTEM_V3, 5).1, method(system::XSYSTEM, system::XSYSTEM_V2, 5).1);
    }
}

unsafe fn wait_status(b: *mut Block) -> i32 {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let hr = xasync::get_status(b as *mut c_void, false);
        if hr != 0x8000_000Au32 as i32 || Instant::now() > until {
            return hr;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn pls_prompt_for_space_completes_from_free_space() {
    env();
    assert_eq!(system::pls_prompt_result(10, 10), 0);
    assert_eq!(system::pls_prompt_result(11, 10), 0x8000_4004u32 as i32);
    unsafe {
        let (o, prompt) = method(system::PLS, system::PLS_IID, 6);
        let prompt: unsafe extern "system" fn(*mut c_void, u64, *mut c_void) -> i32 = core::mem::transmute(prompt);
        let (_, result) = method(system::PLS, system::PLS_IID, 7);
        let result: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32 = core::mem::transmute(result);
        assert_eq!(prompt(o, 1, core::ptr::null_mut()), 0x8007_0057u32 as i32);
        let mut b = Box::new(Block { queue: 0, context: 0, callback: 0, internal: [0; 32] });
        let bp = &mut *b as *mut Block;
        assert_eq!(prompt(o, 1, bp as *mut c_void), 0);
        assert_eq!(wait_status(bp), 0);
        assert_eq!(result(o, bp as *mut c_void), 0);
        let mut b2 = Box::new(Block { queue: 0, context: 0, callback: 0, internal: [0; 32] });
        let bp2 = &mut *b2 as *mut Block;
        assert_eq!(prompt(o, u64::MAX, bp2 as *mut c_void), 0);
        assert_eq!(wait_status(bp2), 0x8000_4004u32 as i32);
        assert_eq!(result(o, bp2 as *mut c_void), 0x8000_4004u32 as i32);
    }
}

type Cb = unsafe extern "system" fn(*mut c_void, bool);
type MonCb = unsafe extern "system" fn(*mut c_void, usize, u32);
type TermCb = unsafe extern "system" fn(*mut c_void);

static SEEN: Mutex<Vec<(usize, usize, u32)>> = Mutex::new(Vec::new());
static RAN: AtomicUsize = AtomicUsize::new(0);
static DISPATCH: AtomicUsize = AtomicUsize::new(0);
static TERMINATED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "system" fn task(_c: *mut c_void, canceled: bool) {
    if !canceled {
        RAN.fetch_add(1, Ordering::SeqCst);
    }
}

unsafe extern "system" fn monitor(ctx: *mut c_void, q: usize, port: u32) {
    SEEN.lock().unwrap().push((ctx as usize, q, port));
    let d = DISPATCH.load(Ordering::SeqCst);
    if d != 0 {
        let f: unsafe extern "system" fn(*mut c_void, usize, u32, u32) -> bool = core::mem::transmute(d);
        let (o, _) = method(xasync::XASYNC, xasync::XASYNC, 16);
        f(o, q, port, 0);
    }
}

unsafe extern "system" fn on_terminated(_c: *mut c_void) {
    TERMINATED.fetch_add(1, Ordering::SeqCst);
}

#[test]
fn task_queue_monitors_fire_on_submit_and_drive_a_manual_terminate() {
    env();
    unsafe {
        let x = xasync::XASYNC;
        let (o, create) = method(x, x, 12);
        let create: unsafe extern "system" fn(*mut c_void, u32, u32, *mut usize) -> i32 = core::mem::transmute(create);
        let dispatch = method(x, x, 16).1;
        let dispatch_f: unsafe extern "system" fn(*mut c_void, usize, u32, u32) -> bool = core::mem::transmute(dispatch);
        let submit: unsafe extern "system" fn(*mut c_void, usize, u32, *mut c_void, Cb) -> i32 = core::mem::transmute(method(x, x, 18).1);
        let delayed: unsafe extern "system" fn(*mut c_void, usize, u32, u32, *mut c_void, Cb) -> i32 = core::mem::transmute(method(x, x, 19).1);
        let terminate: unsafe extern "system" fn(*mut c_void, usize, bool, *mut c_void, Option<TermCb>) -> i32 = core::mem::transmute(method(x, x, 22).1);
        let register: unsafe extern "system" fn(*mut c_void, usize, *mut c_void, Option<MonCb>, *mut u64) -> i32 = core::mem::transmute(method(x, x, 23).1);
        let unregister: unsafe extern "system" fn(*mut c_void, usize, u64) = core::mem::transmute(method(x, x, 24).1);

        let mut q = 0usize;
        assert_eq!(create(o, 0, 0, &mut q), 0);
        let mut tok = 0u64;
        assert_eq!(register(o, q, 9 as *mut c_void, None, &mut tok), 0x8000_4003u32 as i32);
        assert_eq!(register(o, q, 9 as *mut c_void, Some(monitor), core::ptr::null_mut()), 0x8000_4003u32 as i32);
        let fake = [0u64; 16];
        assert_eq!(register(o, fake.as_ptr() as usize, 9 as *mut c_void, Some(monitor), &mut tok), 0x8007_0057u32 as i32);
        assert_eq!(register(o, q, 9 as *mut c_void, Some(monitor), &mut tok), 0);
        assert_ne!(tok, 0);

        assert_eq!(submit(o, q, 0, core::ptr::null_mut(), task), 0);
        assert_eq!(submit(o, q, 1, core::ptr::null_mut(), task), 0);
        assert_eq!(*SEEN.lock().unwrap(), vec![(9, q, 0), (9, q, 1)]);
        assert!(dispatch_f(o, q, 0, 0));
        assert!(dispatch_f(o, q, 1, 0));
        assert_eq!(RAN.load(Ordering::SeqCst), 2);

        assert_eq!(delayed(o, q, 0, 30, core::ptr::null_mut(), task), 0);
        assert_eq!(SEEN.lock().unwrap().len(), 2);
        let until = Instant::now() + Duration::from_secs(5);
        while SEEN.lock().unwrap().len() < 3 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(SEEN.lock().unwrap()[2], (9, q, 0));
        assert!(dispatch_f(o, q, 0, 0));
        assert_eq!(RAN.load(Ordering::SeqCst), 3);

        unregister(o, q, tok);
        assert_eq!(submit(o, q, 0, core::ptr::null_mut(), task), 0);
        assert_eq!(SEEN.lock().unwrap().len(), 3);
        assert!(dispatch_f(o, q, 0, 0));

        let mut q2 = 0usize;
        assert_eq!(create(o, 0, 0, &mut q2), 0);
        let mut tok2 = 0u64;
        assert_eq!(register(o, q2, 4 as *mut c_void, Some(monitor), &mut tok2), 0);
        assert_ne!(tok2, tok);
        DISPATCH.store(dispatch, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        let qq = q2;
        let oo = o as usize;
        std::thread::spawn(move || {
            tx.send(terminate(oo as *mut c_void, qq, true, core::ptr::null_mut(), Some(on_terminated))).unwrap();
        });
        let hr = rx.recv_timeout(Duration::from_secs(5)).expect("Terminate(wait) on a monitor-driven manual queue hung");
        DISPATCH.store(0, Ordering::SeqCst);
        assert_eq!(hr, 0);
        assert_eq!(TERMINATED.load(Ordering::SeqCst), 1);
        let s = SEEN.lock().unwrap();
        assert!(s[3..].contains(&(4, q2, 0)) && s[3..].contains(&(4, q2, 1)));
    }
}
