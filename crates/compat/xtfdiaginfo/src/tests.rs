use crate::com::*;
use crate::debugaction::*;
use crate::entry::*;
use crate::imports::{self, Imports};
use crate::messaging::*;
use crate::monitor::*;
use crate::process;
use crate::sys::*;
use crate::system;
use crate::titles;
use crate::titlememory;
use crate::processdelta;
use crate::xmemi::{self, XMem, XMemBasicInformation};
use crate::OperationFn;
use core::ffi::c_void;
use core::mem::transmute;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicU32, Ordering};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

pub(crate) fn test_imports() -> Result<Imports, HRESULT> {
    unsafe {
        Ok(Imports {
            create_json_object: transmute(xbtp::xbtp2_json::XbtpCreateJsonObject as *const () as usize),
            create_json_array: transmute(xbtp::xbtp2_json::XbtpCreateJsonArray as *const () as usize),
            get_json_object: transmute(xbtp::xbtp2_json::XbtpGetJsonObject as *const () as usize),
            get_json_object_from_utf8: transmute(xbtp::xbtp2_json::XbtpGetJsonObjectFromUtf8 as *const () as usize),
            create_extension_metadata: transmute(xbtp::xbtp2_ext::XbtpCreateExtensionMetadata as *const () as usize),
            create_message: transmute(xbtp::xbtp2_ext::XbtpCreateMessage as *const () as usize),
            has_system_gone_dark: transmute(xbtp::xbtp2_sys::XbtpHasSystemGoneDark as *const () as usize),
            set_system_go_dark: transmute(xbtp::xbtp2_sys::XbtpSetSystemGoDark as *const () as usize),
            log_info_v: transmute(xbtp::base::LogInfoV as *const () as usize),
            log_error_v: transmute(xbtp::base::LogErrorV as *const () as usize),
            logging_instance: transmute(xtfbaseservices::manager::LoggingManager_Instance as *const () as usize),
            get_logging_properties: transmute(xtfbaseservices::manager::LoggingManager_GetLoggingProperties as *const () as usize),
            set_logging_properties: transmute(xtfbaseservices::manager::LoggingManager_SetLoggingProperties as *const () as usize),
        })
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn AddVectoredExceptionHandler(first: u32, h: unsafe extern "system" fn(*mut c_void) -> i32) -> *mut c_void;
    fn RemoveVectoredExceptionHandler(h: *mut c_void) -> u32;
}

type QiFn = unsafe extern "system" fn(Obj, *const GUID, *mut Obj) -> HRESULT;
type RefFn = unsafe extern "system" fn(Obj) -> u32;
type GetBodyFn = unsafe extern "system" fn(Obj, *mut u32, *mut *mut u8) -> HRESULT;
type SetBodyFn = unsafe extern "system" fn(Obj, u32, *const u8) -> HRESULT;
type SetBodyWFn = unsafe extern "system" fn(Obj, *const u16) -> HRESULT;

#[repr(C)]
struct ReplyVtbl {
    qi: QiFn,
    add_ref: RefFn,
    release: RefFn,
    get_json_body: GetBodyFn,
    get_binary_body: GetBodyFn,
    set_json_body: SetBodyFn,
    set_json_body_w: SetBodyWFn,
    set_binary_body: SetBodyFn,
}

#[repr(C)]
struct Reply {
    vtbl: *const ReplyVtbl,
    refs: AtomicU32,
    inner: Obj,
}

unsafe fn inner(this: Obj) -> Obj {
    (*(this as *mut Reply)).inner
}

unsafe extern "system" fn rp_qi(this: Obj, _iid: *const GUID, out: *mut Obj) -> HRESULT {
    *out = this;
    rp_add_ref(this);
    S_OK
}

unsafe extern "system" fn rp_add_ref(this: Obj) -> u32 {
    (*(this as *mut Reply)).refs.fetch_add(1, Ordering::AcqRel) + 1
}

unsafe extern "system" fn rp_release(this: Obj) -> u32 {
    let n = (*(this as *mut Reply)).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let b = Box::from_raw(this as *mut Reply);
        release(b.inner);
    }
    n
}

unsafe extern "system" fn rp_get_json_body(this: Obj, n: *mut u32, b: *mut *mut u8) -> HRESULT {
    let f: GetBodyFn = transmute(slot(inner(this), 3));
    f(inner(this), n, b)
}

unsafe extern "system" fn rp_get_binary_body(this: Obj, n: *mut u32, b: *mut *mut u8) -> HRESULT {
    let f: GetBodyFn = transmute(slot(inner(this), 4));
    f(inner(this), n, b)
}

unsafe extern "system" fn rp_set_json_body(this: Obj, n: u32, b: *const u8) -> HRESULT {
    let f: SetBodyFn = transmute(slot(inner(this), 5));
    f(inner(this), n, b)
}

unsafe extern "system" fn rp_set_json_body_w(this: Obj, w: *const u16) -> HRESULT {
    let f: SetBodyWFn = transmute(slot(inner(this), 7));
    f(inner(this), w)
}

unsafe extern "system" fn rp_set_binary_body(this: Obj, n: u32, b: *const u8) -> HRESULT {
    let f: SetBodyFn = transmute(slot(inner(this), 8));
    f(inner(this), n, b)
}

static REPLY_VTBL: ReplyVtbl = ReplyVtbl {
    qi: rp_qi,
    add_ref: rp_add_ref,
    release: rp_release,
    get_json_body: rp_get_json_body,
    get_binary_body: rp_get_binary_body,
    set_json_body: rp_set_json_body,
    set_json_body_w: rp_set_json_body_w,
    set_binary_body: rp_set_binary_body,
};

unsafe fn new_message(json: Option<&str>) -> Ptr {
    let mut m = Ptr::null();
    assert_eq!(xbtp::xbtp2_ext::XbtpCreateMessage(&xbtp::xbtp2_ext::IID_XBTP_MESSAGE, m.out()), S_OK);
    if let Some(j) = json {
        let mut b = j.as_bytes().to_vec();
        b.push(0);
        let f: SetBodyFn = transmute(slot(m.0, 6));
        assert_eq!(f(m.0, b.len() as u32, b.as_ptr()), S_OK);
    }
    m
}

unsafe fn new_reply() -> Ptr {
    let i = new_message(None).detach();
    Ptr(Box::into_raw(Box::new(Reply { vtbl: &REPLY_VTBL, refs: AtomicU32::new(1), inner: i })) as Obj)
}

unsafe fn reply_text(r: &Ptr) -> String {
    let mut n = 0u32;
    let mut p: *mut u8 = null_mut();
    assert_eq!(msg_get_json_body(r.0, &mut n, &mut p), S_OK);
    if p.is_null() {
        return String::new();
    }
    let s = core::slice::from_raw_parts(p, n as usize);
    String::from_utf8(s.iter().copied().take_while(|c| *c != 0).collect()).unwrap()
}

unsafe fn parse(text: &str) -> Ptr {
    let mut b = text.as_bytes().to_vec();
    b.push(0);
    let mut o = Ptr::null();
    assert_eq!(xbtp::xbtp2_json::XbtpGetJsonObjectFromUtf8(b.as_ptr(), o.out()), S_OK, "{}", text);
    o
}

unsafe fn num(o: &Ptr, k: &str) -> Option<f64> {
    let mut d = 0f64;
    if obj_get_number(o.0, wz(k).as_ptr(), &mut d) == S_OK {
        Some(d)
    } else {
        None
    }
}

unsafe fn text(o: &Ptr, k: &str) -> Option<String> {
    let mut s = Bstr::null();
    if obj_get_string(o.0, wz(k).as_ptr(), s.out()) == S_OK {
        Some(String::from_utf16(s.as_slice()).unwrap())
    } else {
        None
    }
}

unsafe fn child(o: &Ptr, k: &str) -> Option<Ptr> {
    let mut c = Ptr::null();
    if obj_get_object(o.0, wz(k).as_ptr(), c.out()) == S_OK {
        Some(c)
    } else {
        None
    }
}

unsafe fn call(h: OperationFn, json: Option<&str>) -> (HRESULT, String) {
    let req = new_message(json);
    let rep = new_reply();
    let hr = h(0, req.0 as usize, rep.0 as usize);
    (hr, reply_text(&rep))
}

fn spawn_sleeper() -> Child {
    Command::new("cmd").args(["/c", "pause"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap()
}

static LOGGING_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn reply_wrapper_slot6_is_set_json_body_w() {
    unsafe {
        let r = new_reply();
        assert_eq!(reply_set_json_body_w(r.0, wz("{\"a\":1}").as_ptr()), S_OK);
        assert_eq!(reply_text(&r), "{\"a\":1}");
    }
}

#[test]
fn system_up_time_reports_tick_count_halves() {
    unsafe {
        let before = GetTickCount64();
        let (hr, t) = call(system::system_up_time_handler, None);
        let after = GetTickCount64();
        assert_eq!(hr, S_OK);
        let o = parse(&t);
        let hi = num(&o, "SystemUptimeHDW").unwrap() as u64;
        let lo = num(&o, "SystemUptimeLDW").unwrap() as u64;
        let v = (hi << 32) | lo;
        assert!(v >= before && v <= after, "{} {} {}", before, v, after);
    }
}

#[test]
fn go_dark_set_then_fetch() {
    unsafe {
        let (hr, t) = call(system::set_system_go_dark_handler, None);
        assert_eq!(hr, S_OK);
        assert_eq!(t, "{}");
        assert_ne!(xbtp::xbtp2_sys::XbtpHasSystemGoneDark() & 0xFF, 0);
        let (hr, t) = call(system::fetch_system_go_dark_handler, None);
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "SysIsDark"), Some(1.0));
    }
}

#[test]
fn logging_properties_round_trip() {
    let _g = LOGGING_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let (hr, t) = call(system::set_logging_properties_handler, Some("{\"LogLevel\":3.9,\"LogFile\":\"default\",\"LogTimestamps\":0.005}"));
        assert_eq!(hr, S_OK);
        assert_eq!(t, "");
        let (hr, t) = call(system::get_logging_properties_handler, None);
        assert_eq!(hr, S_OK);
        let o = parse(&t);
        assert_eq!(num(&o, "LogLevel"), Some(3.0));
        assert_eq!(num(&o, "LogTimestamps"), Some(0.0));
        assert!(text(&o, "LogFile").is_some());
        let (hr, _) = call(system::set_logging_properties_handler, Some("{\"LogTimestamps\":0.02}"));
        assert_eq!(hr, S_OK);
        let (_, t) = call(system::get_logging_properties_handler, None);
        let o = parse(&t);
        assert_eq!(num(&o, "LogLevel"), Some(3.0));
        assert_eq!(num(&o, "LogTimestamps"), Some(1.0));
        let (hr, _) = call(system::set_logging_properties_handler, Some("{\"LogLevel\":6}"));
        assert_eq!(hr, S_OK);
    }
}

#[test]
fn set_logging_properties_bad_body_writes_operation_error() {
    let _g = LOGGING_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let (hr, t) = call(system::set_logging_properties_handler, Some("{not json"));
        assert!(hr < 0);
        let o = parse(&t);
        let e = child(&o, "XtfOperationError").unwrap();
        assert_eq!(num(&e, "ReturnCode"), Some(hr as f64));
        assert_eq!(num(&e, "Checkpoint"), Some(0.0));
        assert!(t.contains(&format!("Error 0x{:X} File xbox", hr as u32)), "{}", t);
        assert!(t.contains("loggingserver.cpp : 39"), "{}", t);
        let (hr, t) = call(system::set_logging_properties_handler, Some("{\"LogLevel\":1,\"LogFile\":5}"));
        assert!(hr < 0);
        let e = child(&parse(&t), "XtfOperationError").unwrap();
        assert_eq!(num(&e, "Checkpoint"), Some(1.0));
        assert!(t.contains("loggingserver.cpp : 60"), "{}", t);
    }
}

#[test]
fn query_profiling_directory_defaults_to_d() {
    unsafe {
        let (hr, t) = call(system::query_profiling_directory_handler, None);
        assert_eq!(hr, S_OK);
        let o = parse(&t);
        let expect = String::from_utf16(wslice(system::profiling_directory().as_ptr())).unwrap();
        assert_eq!(text(&o, "PD"), Some(expect.clone()));
        if GetFileAttributesW(wz(system::PROFILING_PATH).as_ptr()) == u32::MAX {
            assert_eq!(expect, "D:");
        }
        let e = child(&o, "XtfOperationError").unwrap();
        assert_eq!(num(&e, "ReturnCode"), Some(0.0));
        assert_eq!(num(&e, "Checkpoint"), Some(0.0));
    }
}

#[test]
fn messaging_helpers() {
    unsafe {
        assert_eq!(String::from_utf16(wslice(error_text(0x80070057u32 as i32, "a.cpp", 7).as_ptr())).unwrap(), "Error 0x80070057 File a.cpp : 7");
        let o = parse("{\"a\":4294967297.9,\"b\":-1,\"c\":1e30}");
        let mut v = 0u32;
        assert_eq!(get_u32(o.0, &wz("a"), &mut v), S_OK);
        assert_eq!(v, 1);
        assert_eq!(get_u32(o.0, &wz("b"), &mut v), S_OK);
        assert_eq!(v, u32::MAX);
        assert_eq!(get_u32(o.0, &wz("c"), &mut v), S_OK);
        assert_eq!(v, 0);
        v = 9;
        assert_eq!(get_u32(o.0, &wz("zz"), &mut v), E_JSON_NOT_FOUND);
        assert_eq!(v, 9);
        assert_eq!(get_u32(null_mut(), &wz("a"), &mut v), E_INVALIDARG);
        assert_eq!(set_i32(null_mut(), &wz("a"), 1), E_INVALIDARG);
        assert_eq!(set_bool(o.0, &wz("t"), true), S_OK);
        assert_eq!(num(&o, "t"), Some(1.0));
        assert_eq!(deserialize_body(null_mut(), null_mut()), E_INVALIDARG);
        assert_eq!(serialize_body(null_mut(), null_mut()), E_INVALIDARG);
    }
}

#[test]
fn complete_operation_handler_shapes() {
    unsafe {
        let r = new_reply();
        let mut b = Ptr::null();
        xbtp::xbtp2_json::XbtpCreateJsonObject(b.out());
        let mut hr = E_NOTIMPL;
        complete_operation_handler(r.0, b.0, &mut hr);
        assert_eq!(hr, S_OK);
        let o = parse(&reply_text(&r));
        assert_eq!(num(&o, "Successful"), Some(0.0));
        assert_eq!(num(&child(&o, "Result").unwrap(), "ResultCode"), Some(E_NOTIMPL as f64));
        let r = new_reply();
        let mut b = Ptr::null();
        xbtp::xbtp2_json::XbtpCreateJsonObject(b.out());
        let mut hr = S_OK;
        complete_operation_handler(r.0, b.0, &mut hr);
        assert_eq!(hr, S_OK);
        let o = parse(&reply_text(&r));
        assert_eq!(num(&o, "Successful"), Some(1.0));
        assert!(child(&o, "Result").is_none());
        let mut hr = E_FAIL;
        complete_operation_handler(r.0, null_mut(), &mut hr);
        assert_eq!(hr, E_FAIL);
    }
}

#[test]
fn error_context_completion_rules() {
    unsafe {
        let r = new_reply();
        {
            let mut c = ErrorContext::new(r.0);
            let mut hr = S_OK;
            c.complete(&mut hr);
            assert!(!c.completed);
        }
        assert_eq!(reply_text(&r), "");
        {
            let mut c = ErrorContext::new(r.0);
            c.checkpoint = 2;
            assert_eq!(c.add_error(E_FAIL, "x.cpp", 5), S_OK);
            c.hr = E_FAIL;
        }
        let t = reply_text(&r);
        let e = child(&parse(&t), "XtfOperationError").unwrap();
        assert_eq!(num(&e, "ReturnCode"), Some(E_FAIL as f64));
        assert_eq!(num(&e, "Checkpoint"), Some(2.0));
        assert!(t.contains("Error 0x80004005 File x.cpp : 5"), "{}", t);
        let mut c = ErrorContext::new(null_mut());
        let mut hr = E_FAIL;
        c.complete(&mut hr);
        assert_eq!(hr, E_FAIL);
        assert!(!c.completed);
    }
}

#[test]
fn title_gating() {
    unsafe {
        assert!(titles::is_title_process(GetCurrentProcessId()));
        let mut c = spawn_sleeper();
        let pid = c.id();
        assert!(!titles::is_title_process(pid));
        assert!(titles::open_process(0x400, pid).is_null());
        assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
        assert_eq!(titles::debug_active_process(pid), 0);
        assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
        titles::register_title_process(pid);
        assert!(titles::is_title_process(pid));
        let h = titles::open_process(0x400, pid);
        assert!(!h.is_null());
        CloseHandle(h);
        titles::unregister_title_process(pid);
        assert!(!titles::is_title_process(pid));
        c.kill().ok();
        c.wait().ok();
    }
}

#[test]
fn terminate_process_only_for_titles() {
    unsafe {
        let mut c = spawn_sleeper();
        let pid = c.id();
        let (hr, t) = call(process::terminate_process_handler, Some(&format!("{{\"Pid\":{},\"ExitCode\":7}}", pid)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(E_INVALIDARG as f64));
        assert!(c.try_wait().unwrap().is_none());
        titles::register_title_process(pid);
        let (hr, t) = call(process::terminate_process_handler, Some(&format!("{{\"Pid\":{},\"WaitOnTerminate\":1,\"WaitOnTerminateTimeout\":5000,\"ExitCode\":7}}", pid)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(0.0));
        assert_eq!(c.wait().unwrap().code(), Some(7));
        titles::unregister_title_process(pid);
        let (hr, t) = call(process::terminate_process_handler, Some("{}"));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(E_JSON_NOT_FOUND as f64));
    }
}

#[test]
fn resume_all_threads_untracked_pid() {
    unsafe {
        let (hr, t) = call(process::resume_all_threads_handler, Some("{\"Pid\":4000000000}"));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(E_PROCESS_NOT_TRACKED as f64));
        assert_eq!(process::request_resume(4000000001), E_PROCESS_NOT_TRACKED);
    }
}

#[test]
fn debug_break_tracks_then_resume_releases() {
    unsafe {
        let mut c = spawn_sleeper();
        let pid = c.id();
        let (hr, t) = call(process::debug_break_handler, Some(&format!("{{\"Pid\":{}}}", pid)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(E_INVALIDARG as f64));
        assert!(!process::is_tracked(pid));
        titles::register_title_process(pid);
        let (hr, t) = call(process::debug_break_handler, Some(&format!("{{\"Pid\":{}}}", pid)));
        assert_eq!(hr, S_OK);
        let r = num(&parse(&t), "Result").unwrap();
        if r == 0.0 {
            assert!(process::is_tracked(pid));
            let (hr, t) = call(process::resume_all_threads_handler, Some(&format!("{{\"Pid\":{}}}", pid)));
            assert_eq!(hr, S_OK);
            assert_eq!(num(&parse(&t), "Result"), Some(0.0));
            let mut n = 0;
            while process::is_tracked(pid) && n < 100 {
                Sleep(50);
                n += 1;
            }
            assert!(!process::is_tracked(pid));
        } else {
            eprintln!("debug_break Result 0x{:X}", r as i64 as u32);
            assert!(r < 0.0, "{}", t);
            assert!(!process::is_tracked(pid));
            DebugActiveProcessStop(pid);
        }
        titles::unregister_title_process(pid);
        c.kill().ok();
        c.wait().ok();
    }
}

#[repr(C)]
struct ProcessInformation {
    process: HANDLE,
    thread: HANDLE,
    pid: u32,
    tid: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateProcessW(app: *const u16, cmd: *mut u16, pa: *mut c_void, ta: *mut c_void, inherit: BOOL, flags: u32, env: *mut c_void, dir: *const u16, si: *mut u8, pi: *mut ProcessInformation) -> BOOL;
    fn GetExitCodeProcess(h: HANDLE, code: *mut u32) -> BOOL;
}

unsafe fn spawn_suspended(cmdline: &str) -> ProcessInformation {
    let mut cmd = wz(cmdline);
    let mut si = [0u8; 104];
    si[0] = 104;
    let mut pi: ProcessInformation = core::mem::zeroed();
    assert_ne!(CreateProcessW(core::ptr::null(), cmd.as_mut_ptr(), null_mut(), null_mut(), 0, 4 | 0x0800_0000, null_mut(), core::ptr::null(), si.as_mut_ptr(), &mut pi), 0);
    pi
}

#[test]
fn resume_deferred_execution_resumes_suspended_title() {
    unsafe {
        let pi = spawn_suspended("cmd /c exit 5");
        assert_eq!(WaitForSingleObject(pi.process, 300), 0x102);
        let (hr, r) = call(process::resume_deferred_execution_handler, Some(&format!("{{\"Pid\":{}}}", pi.pid)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&r), "Result"), Some(E_INVALIDARG as f64));
        assert_eq!(WaitForSingleObject(pi.process, 300), 0x102);
        titles::register_title_process(pi.pid);
        let (hr, r) = call(process::resume_deferred_execution_handler, Some(&format!("{{\"Pid\":{}}}", pi.pid)));
        titles::unregister_title_process(pi.pid);
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&r), "Result"), Some(0.0));
        assert_eq!(WaitForSingleObject(pi.process, 10000), 0);
        let mut code = 0u32;
        assert_ne!(GetExitCodeProcess(pi.process, &mut code), 0);
        assert_eq!(code, 5);
        CloseHandle(pi.thread);
        CloseHandle(pi.process);
    }
}

#[test]
fn tracking_thread_resumes_recorded_threads_on_request() {
    unsafe {
        let pi = spawn_suspended("cmd /c exit 3");
        let h = OpenProcess(0x1F_FFFF, 0, pi.pid);
        assert!(!h.is_null());
        process::track(h, pi.pid, vec![pi.tid]);
        assert!(process::is_tracked(pi.pid));
        let t = CreateThread(null_mut(), 0, process::tracking_thread, pi.pid as usize as *mut c_void, 0, null_mut());
        assert!(!t.is_null());
        assert_eq!(WaitForSingleObject(pi.process, 300), 0x102);
        let (hr, r) = call(process::resume_all_threads_handler, Some(&format!("{{\"Pid\":{}}}", pi.pid)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&r), "Result"), Some(0.0));
        assert_eq!(WaitForSingleObject(t, 10000), 0);
        assert!(!process::is_tracked(pi.pid));
        assert_eq!(WaitForSingleObject(pi.process, 10000), 0);
        let mut code = 0u32;
        GetExitCodeProcess(pi.process, &mut code);
        assert_eq!(code, 3);
        CloseHandle(t);
        CloseHandle(pi.thread);
        CloseHandle(pi.process);
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateRemoteThread(h: HANDLE, sa: *mut c_void, stack: usize, start: *mut c_void, param: *mut c_void, flags: u32, tid: *mut u32) -> HANDLE;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(h: HANDLE) -> i32;
}

#[test]
fn debug_break_records_pre_suspended_threads_and_tracks() {
    unsafe {
        let mut c = spawn_sleeper();
        let pid = c.id();
        Sleep(300);
        let h = OpenProcess(0x1F_FFFF, 0, pid);
        assert!(!h.is_null());
        let k32 = GetModuleHandleW(wz("kernel32.dll").as_ptr());
        let sleep = GetProcAddress(k32, b"Sleep\0".as_ptr());
        assert!(!sleep.is_null());
        let mut tid = 0u32;
        let rt = CreateRemoteThread(h, null_mut(), 0, sleep, 0xFFFF_FFFFusize as *mut c_void, 0, &mut tid);
        assert!(!rt.is_null());
        Sleep(100);
        assert!(NtSuspendProcess(h) >= 0);
        titles::register_title_process(pid);
        let (hr, t) = call(process::debug_break_handler, Some(&format!("{{\"Pid\":{}}}", pid)));
        assert_eq!(hr, S_OK);
        let r = num(&parse(&t), "Result").unwrap();
        if r != 0.0 {
            DebugActiveProcessStop(pid);
        }
        assert_eq!(r, 0.0, "{}", t);
        assert!(process::is_tracked(pid));
        assert_eq!(process::request_resume(pid), S_OK);
        let mut n = 0;
        while process::is_tracked(pid) && n < 100 {
            Sleep(50);
            n += 1;
        }
        assert!(!process::is_tracked(pid));
        let count = SuspendThread(rt);
        assert_ne!(count, u32::MAX);
        ResumeThread(rt);
        assert_eq!(count, 0);
        titles::unregister_title_process(pid);
        CloseHandle(rt);
        CloseHandle(h);
        c.kill().ok();
        c.wait().ok();
    }
}

static BREAK_LOCK: Mutex<()> = Mutex::new(());

unsafe extern "system" fn skip_any_breakpoint(p: *mut c_void) -> i32 {
    let rec = *(p as *const *const u32);
    let ctx = *(p as *const *mut u8).add(1);
    if *rec == 0x8000_0003 {
        *(ctx.add(0xF8) as *mut u64) += 1;
        return -1;
    }
    0
}

#[test]
fn self_pid_requests_take_console_paths() {
    let _g = BREAK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let me = GetCurrentProcessId();
        let v = AddVectoredExceptionHandler(1, skip_any_breakpoint);
        let (hr, t) = call(process::debug_break_handler, Some(&format!("{{\"Pid\":{}}}", me)));
        Sleep(200);
        RemoveVectoredExceptionHandler(v);
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(0.0));
        assert!(!process::is_tracked(me));
        let (hr, t) = call(process::resume_deferred_execution_handler, Some(&format!("{{\"Pid\":{}}}", me)));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Result"), Some(0.0));
    }
}

#[test]
fn debug_action_flag_mapping_and_request_layout() {
    assert_eq!(vm_flags(0), (0, 0));
    assert_eq!(vm_flags(1), (1, 0));
    assert_eq!(vm_flags(2), (2, 2));
    assert_eq!(vm_flags(4), (4, 4));
    assert_eq!(vm_flags(8), (8, 1));
    assert_eq!(vm_flags(0x1f), (0xf, 7));
    assert_eq!(core::mem::size_of::<DebugActionRequest>(), 0x28);
    let r = request_for(0xff, DIAGINFO_EXTENSION_ID);
    assert_eq!((r.flags, r.version, r.reserved), (7, 4, 0));
    assert_eq!(r.correlation, DIAGINFO_EXTENSION_ID);
}

#[test]
fn debug_action_correlation_guid() {
    unsafe {
        let o = parse("{\"CorrelationGuid\":\"fd21ac23-3a3a-45ce-a785-c973372dd881\"}");
        let mut g = GUID::default();
        assert_eq!(get_correlation_guid(o.0, &mut g), S_OK);
        assert_eq!(g, DIAGINFO_EXTENSION_ID);
        let o = parse("{}");
        assert_eq!(get_correlation_guid(o.0, &mut g), E_JSON_NOT_FOUND);
        assert_eq!(get_correlation_guid(null_mut(), &mut g), E_INVALIDARG);
        let o = parse("{\"CorrelationGuid\":\"nope\"}");
        assert!(get_correlation_guid(o.0, &mut g) < 0);
    }
}

#[test]
fn debug_action_results() {
    unsafe {
        let (hr, t) = call(debug_action_handler, Some("{\"Flags\":0}"));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&parse(&t), "Successful"), Some(1.0));
        let (hr, t) = call(debug_action_handler, Some("{\"Flags\":16}"));
        assert_eq!(hr, S_OK);
        let o = parse(&t);
        assert_eq!(num(&o, "Successful"), Some(0.0));
        assert_eq!(num(&child(&o, "Result").unwrap(), "ResultCode"), Some(E_NOTIMPL as f64));
        let (hr, t) = call(debug_action_handler, Some("{}"));
        assert_eq!(hr, S_OK);
        assert_eq!(num(&child(&parse(&t), "Result").unwrap(), "ResultCode"), Some(E_JSON_NOT_FOUND as f64));
        if GetFileAttributesW(wz(XVMCTRL_DEVICE).as_ptr()) == u32::MAX {
            let (hr, t) = call(debug_action_handler, Some("{\"Flags\":2}"));
            assert_eq!(hr, S_OK);
            let o = parse(&t);
            assert_eq!(num(&o, "Successful"), Some(0.0));
            assert!(num(&child(&o, "Result").unwrap(), "ResultCode").unwrap() < 0.0);
        }
    }
}

static BREAK_THREAD: AtomicU32 = AtomicU32::new(0);
static BREAK_HITS: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn skip_breakpoint(p: *mut c_void) -> i32 {
    let rec = *(p as *const *const u32);
    let ctx = *(p as *const *mut u8).add(1);
    if *rec == 0x8000_0003 && GetCurrentThreadId() == BREAK_THREAD.load(Ordering::Acquire) {
        *(ctx.add(0xF8) as *mut u64) += 1;
        BREAK_HITS.fetch_add(1, Ordering::AcqRel);
        return -1;
    }
    0
}

#[test]
fn debug_action_flag1_raises_breakpoint() {
    let _g = BREAK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        BREAK_THREAD.store(GetCurrentThreadId(), Ordering::Release);
        let h = AddVectoredExceptionHandler(1, skip_breakpoint);
        assert!(!h.is_null());
        let (hr, t) = call(debug_action_handler, Some("{\"Flags\":1}"));
        RemoveVectoredExceptionHandler(h);
        assert_eq!(hr, S_OK);
        assert_eq!(BREAK_HITS.load(Ordering::Acquire), 1);
        assert_eq!(num(&parse(&t), "Successful"), Some(1.0));
    }
}

#[test]
fn entry_point_validates_guids_and_registers_handlers() {
    unsafe {
        let mut out: *mut c_void = null_mut();
        let other = GUID::new(1, 2, 3, [0; 8]);
        assert_eq!(extension_entry_point(&other, &IID_XBTP_EXTENSION_METADATA_IMPL, &mut out, &IMPLEMENTED_HANDLERS), E_INVALIDARG);
        assert_eq!(extension_entry_point(&DIAGINFO_EXTENSION_ID, &other, &mut out, &IMPLEMENTED_HANDLERS), E_INVALIDARG);
        assert!(out.is_null());
        assert_eq!(extension_entry_point(&DIAGINFO_EXTENSION_ID, &IID_XBTP_EXTENSION_METADATA_IMPL, &mut out, &IMPLEMENTED_HANDLERS), S_OK);
        assert!(!out.is_null());
        let count: unsafe extern "system" fn(Obj, *mut u32) -> HRESULT = transmute(slot(out, 3));
        let mut n = 0u32;
        assert_eq!(count(out, &mut n), S_OK);
        assert_eq!(n, IMPLEMENTED_HANDLERS.len() as u32);
        release(out);
        let mut pos = 0;
        for (id, _) in IMPLEMENTED_HANDLERS.iter() {
            let p = CONSOLE_REGISTRATION_ORDER.iter().position(|x| x == id).unwrap();
            assert!(p >= pos);
            pos = p;
        }
        let dup = [(OP_SYSTEM_UP_TIME, system::system_up_time_handler as OperationFn), (OP_SYSTEM_UP_TIME, system::system_up_time_handler as OperationFn)];
        let mut out2: *mut c_void = null_mut();
        assert!(extension_entry_point(&DIAGINFO_EXTENSION_ID, &IID_XBTP_EXTENSION_METADATA_IMPL, &mut out2, &dup) < 0);
        assert!(out2.is_null());
    }
}

struct RecordingService {
    calls: Mutex<Vec<(u32, Option<f64>)>>,
}

impl OperationService for RecordingService {
    unsafe fn route(&self, op: u32, body: Obj, response: *mut Obj) -> HRESULT {
        let a = if body.is_null() {
            None
        } else {
            let mut d = 0f64;
            if obj_get_number(body, wz("a").as_ptr(), &mut d) == S_OK {
                Some(d)
            } else {
                None
            }
        };
        self.calls.lock().unwrap().push((op, a));
        if op == 3 {
            let mut o = Ptr::null();
            xbtp::xbtp2_json::XbtpCreateJsonObject(o.out());
            obj_set_number(o.0, wz("x").as_ptr(), 5.0);
            *response = o.detach();
            return S_OK;
        }
        E_NOTIMPL
    }

    unsafe fn check_pending(&self) -> HRESULT {
        S_OK
    }
}

unsafe fn bstr_text(b: &Bstr) -> String {
    String::from_utf16(b.as_slice()).unwrap()
}

#[test]
fn operation_response_format() {
    unsafe {
        let mut b = Bstr::null();
        assert_eq!(create_operation_response(E_FAIL, null_mut(), b.out()), S_OK);
        let o = parse(&bstr_text(&b));
        assert_eq!(num(&o, "version"), Some(1.0));
        assert_eq!(num(&o, "opresult"), Some(E_FAIL as f64));
        assert!(child(&o, "opbody").is_none());
        assert_eq!(create_operation_response(S_OK, null_mut(), null_mut()), E_POINTER);
    }
}

#[test]
fn operation_request_decode_and_route() {
    unsafe {
        let s = RecordingService { calls: Mutex::new(Vec::new()) };
        let mut b = Bstr::null();
        process_operation_request(&s, wz("{\"version\":1,\"opid\":3,\"opbody\":{\"a\":2}}").as_ptr(), b.out());
        let o = parse(&bstr_text(&b));
        assert_eq!(num(&o, "opresult"), Some(0.0));
        assert_eq!(num(&child(&o, "opbody").unwrap(), "x"), Some(5.0));
        process_operation_request(&s, wz("{\"version\":1,\"opid\":12}").as_ptr(), b.out());
        let o = parse(&bstr_text(&b));
        assert_eq!(num(&o, "opresult"), Some(E_NOTIMPL as f64));
        assert!(child(&o, "opbody").is_none());
        process_operation_request(&s, wz("{\"version\":2,\"opid\":3}").as_ptr(), b.out());
        assert_eq!(num(&parse(&bstr_text(&b)), "opresult"), Some(HR_NOT_SUPPORTED as f64));
        process_operation_request(&s, wz("{\"opid\":3}").as_ptr(), b.out());
        assert_eq!(num(&parse(&bstr_text(&b)), "opresult"), Some(E_JSON_NOT_FOUND as f64));
        process_operation_request(&s, wz("{\"version\":1}").as_ptr(), b.out());
        assert_eq!(num(&parse(&bstr_text(&b)), "opresult"), Some(E_JSON_NOT_FOUND as f64));
        process_operation_request(&s, wz("{\"version\":1,\"opid\":4,\"opbody\":7}").as_ptr(), b.out());
        assert!(num(&parse(&bstr_text(&b)), "opresult").unwrap() < 0.0);
        process_operation_request(&s, wz("garbage").as_ptr(), b.out());
        assert!(num(&parse(&bstr_text(&b)), "opresult").unwrap() < 0.0);
        process_operation_request(&s, core::ptr::null(), b.out());
        assert_eq!(num(&parse(&bstr_text(&b)), "opresult"), Some(E_POINTER as f64));
        assert_eq!(*s.calls.lock().unwrap(), vec![(3, Some(2.0)), (12, None)]);
    }
}

static PIPE_SERVICE: RecordingService = RecordingService { calls: Mutex::new(Vec::new()) };

unsafe fn pipe_round_trip(name: &[u16], request: &str) -> String {
    let mut h = INVALID_HANDLE_VALUE;
    for _ in 0..250 {
        h = CreateFileW(name.as_ptr(), 0xC000_0000, 0, null_mut(), 3, 0, null_mut());
        if h != INVALID_HANDLE_VALUE {
            break;
        }
        Sleep(20);
    }
    assert_ne!(h, INVALID_HANDLE_VALUE);
    let req = wz(request);
    let mut n = 0u32;
    assert_ne!(WriteFile(h, req.as_ptr() as *const c_void, (req.len() * 2) as u32, &mut n, null_mut()), 0);
    let mut buf = vec![0u16; 0x2000];
    let mut got = 0u32;
    assert_ne!(ReadFile(h, buf.as_mut_ptr() as *mut c_void, 0x4000, &mut got, null_mut()), 0);
    CloseHandle(h);
    assert_eq!(got % 2, 0);
    let chars = (got / 2) as usize;
    assert!(chars > 0);
    assert_eq!(buf[chars - 1], 0);
    String::from_utf16(&buf[..chars - 1]).unwrap()
}

#[test]
fn operation_request_monitor_serves_pipe() {
    unsafe {
        let name = pipe_name(GetCurrentProcessId());
        assert!(String::from_utf16(wslice(name.as_ptr())).unwrap().starts_with("\\\\.\\pipe\\XTF_CONSOLE_EXTENSION_API_SYSTEM_TO_TITLE_OPERATION_REQUEST_"));
        assert_eq!(start_operation_request_monitor(&PIPE_SERVICE), S_OK);
        assert_eq!(start_operation_request_monitor(&PIPE_SERVICE), S_OK);
        let t = pipe_round_trip(&name, "{\"version\":1,\"opid\":3,\"opbody\":{\"a\":9}}");
        let o = parse(&t);
        assert_eq!(num(&o, "version"), Some(1.0));
        assert_eq!(num(&o, "opresult"), Some(0.0));
        assert_eq!(num(&child(&o, "opbody").unwrap(), "x"), Some(5.0));
        let t = pipe_round_trip(&name, "{\"version\":3,\"opid\":3}");
        assert_eq!(num(&parse(&t), "opresult"), Some(HR_NOT_SUPPORTED as f64));
        assert_eq!(*PIPE_SERVICE.calls.lock().unwrap(), vec![(3, Some(9.0))]);
    }
}

#[repr(C)]
#[derive(Default)]
struct PcMbi {
    base: u64,
    alloc_base: u64,
    alloc_protect: u32,
    partition: u16,
    pad: u16,
    region_size: u64,
    state: u32,
    protect: u32,
    kind: u32,
    pad2: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn VirtualQueryEx(h: HANDLE, a: *const c_void, m: *mut PcMbi, n: usize) -> usize;
}

unsafe extern "system" fn fx_virtual_query(h: HANDLE, a: *const c_void, out: *mut XMemBasicInformation) -> i32 {
    if out.is_null() {
        SetLastError(ERROR_INVALID_PARAMETER);
        return 0;
    }
    let mut m = PcMbi::default();
    if VirtualQueryEx(h, a, &mut m, core::mem::size_of::<PcMbi>()) == 0 {
        return 0;
    }
    *out = XMemBasicInformation { base: m.base, alloc_base: m.alloc_base, alloc_protect: m.alloc_protect, region_size: m.region_size, state: m.state, protect: m.protect, kind: m.kind, xmem_flags: if m.partition == 0 { 1 } else { 0 }, ..Default::default() };
    1
}

static WS_FLAGS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static WS_FAIL: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn fx_working_set(flags: u32, out: *mut c_void) -> i32 {
    WS_FLAGS.lock().unwrap_or_else(|e| e.into_inner()).push(flags);
    let e = WS_FAIL.load(Ordering::Acquire);
    if e != 0 {
        SetLastError(e);
        return 0;
    }
    let o = out as *mut u8;
    core::ptr::write_unaligned(o as *mut u64, 0x3_6000_0000);
    core::ptr::write_unaligned(o.add(8) as *mut u64, 0x1_2345_6789);
    core::ptr::write_unaligned(o.add(0x10) as *mut u64, 0x8000_0000);
    core::ptr::write_unaligned(o.add(0x18) as *mut u64, 0x1000);
    core::ptr::write_unaligned(o.add(0x20) as *mut u32, 0x4_0000);
    1
}

pub(crate) fn test_xmem() -> XMem {
    XMem { virtual_query: fx_virtual_query, get_working_set_statistics: fx_working_set }
}

static TITLEMEM_LOCK: Mutex<()> = Mutex::new(());

unsafe fn u64_field(o: &Ptr, k: &str) -> Option<u64> {
    let c = child(o, k)?;
    let l = num(&c, "L")? as u64;
    let h = num(&c, "H")? as i64 as u64;
    Some((h << 32) | l)
}

unsafe fn json_array(o: &Ptr, k: &str) -> Option<(Ptr, u32)> {
    let mut a = Ptr::null();
    let f: unsafe extern "system" fn(Obj, *const u16, *mut Obj) -> HRESULT = transmute(slot(o.0, 10));
    if f(o.0, wz(k).as_ptr(), a.out()) != S_OK {
        return None;
    }
    let size: unsafe extern "system" fn(Obj, *mut u32) -> HRESULT = transmute(slot(a.0, 11));
    let mut n = 0u32;
    assert_eq!(size(a.0, &mut n), S_OK);
    Some((a, n))
}

unsafe fn array_items(o: &Ptr, k: &str) -> Vec<Ptr> {
    let Some((a, n)) = json_array(o, k) else { return Vec::new() };
    let get: unsafe extern "system" fn(Obj, u32, *mut Obj) -> HRESULT = transmute(slot(a.0, 9));
    (0..n)
        .map(|i| {
            let mut e = Ptr::null();
            assert_eq!(get(a.0, i, e.out()), S_OK);
            e
        })
        .collect()
}

unsafe fn array_strings(o: &Ptr, k: &str) -> Vec<String> {
    let Some((a, n)) = json_array(o, k) else { return Vec::new() };
    let get: unsafe extern "system" fn(Obj, u32, *mut BSTR) -> HRESULT = transmute(slot(a.0, 8));
    (0..n)
        .map(|i| {
            let mut e = Bstr::null();
            assert_eq!(get(a.0, i, e.out()), S_OK);
            String::from_utf16(e.as_slice()).unwrap()
        })
        .collect()
}

#[test]
fn title_memory_u64_property_is_low_high_object() {
    unsafe {
        let o = parse("{}");
        assert_eq!(titlememory::set_u64(o.0, &wz("X"), 0xffff_ffff_0000_0005), S_OK);
        let c = child(&o, "X").unwrap();
        assert_eq!(num(&c, "L"), Some(5.0));
        assert_eq!(num(&c, "H"), Some(-1.0));
        assert_eq!(titlememory::set_u64(null_mut(), &wz("X"), 1), E_INVALIDARG);
    }
}

#[test]
fn title_memory_report_for_own_process() {
    let _g = TITLEMEM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        WS_FAIL.store(0, Ordering::Release);
        WS_FLAGS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let pid = GetCurrentProcessId();
        let probe = Box::new(0u64);
        let here = &*probe as *const u64 as u64;
        let (hr, t) = call(titlememory::title_memory_report_handler, Some(&format!("{{\"ProcessId\":{}}}", pid)));
        assert_eq!(hr, S_OK, "{}", t);
        let r = parse(&t);
        let procs = array_items(&r, "ProcessReports");
        assert_eq!(procs.len(), 1, "{}", t);
        assert_eq!(num(&procs[0], "ProcessId"), Some(pid as f64));
        assert!(text(&procs[0], "ProcessName").unwrap().to_ascii_lowercase().ends_with(".exe"));
        let regions = array_items(&procs[0], "RXF");
        assert!(!regions.is_empty());
        for v in regions.iter() {
            assert_eq!(num(v, "RST"), Some(0x1000 as f64));
            assert!(u64_field(v, "RSZ").unwrap() > 0);
            assert!(num(v, "RP").is_some() && num(v, "RT").is_some());
            assert!(u64_field(v, "RXF").is_some());
        }
        assert!(regions.iter().any(|v| {
            let b = u64_field(v, "RBA").unwrap();
            b <= here && here < b + u64_field(v, "RSZ").unwrap()
        }));
        assert_eq!(num(&r, "TotalVA"), Some(regions.len() as f64));
        assert_eq!(*WS_FLAGS.lock().unwrap_or_else(|e| e.into_inner()), vec![titlememory::WS_FLAGS_WITH_VA]);
        let p = child(&r, "PartitionReport").unwrap();
        assert_eq!(u64_field(&p, "GL"), Some(0x3_6000_0000));
        assert_eq!(u64_field(&p, "GU"), Some(0x1_2345_6789));
        assert_eq!(num(&p, "GS"), Some(0x4_0000 as f64));
        assert_eq!(u64_field(&p, "TL"), Some(0x8000_0000));
        assert_eq!(u64_field(&p, "TU"), Some(0x1000));
        assert_eq!(u64_field(&p, "SR"), Some(0));
        let e = child(&r, "XtfOperationError").unwrap();
        assert_eq!(num(&e, "ReturnCode"), Some(0.0));
        drop(probe);
    }
}

#[test]
fn title_memory_report_skips_low_pids_and_non_titles() {
    let _g = TITLEMEM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        WS_FAIL.store(0, Ordering::Release);
        WS_FLAGS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let (hr, t) = call(titlememory::title_memory_report_handler, Some("{\"ProcessId\":4}"));
        assert_eq!(hr, S_OK);
        let r = parse(&t);
        assert!(array_items(&r, "ProcessReports").is_empty());
        assert_eq!(num(&r, "TotalVA"), Some(0.0));
        assert_eq!(*WS_FLAGS.lock().unwrap_or_else(|e| e.into_inner()), vec![titlememory::WS_FLAGS_NO_VA]);
        let mut sleeper = spawn_sleeper();
        let other = sleeper.id();
        let (hr, t) = call(titlememory::title_memory_report_handler, Some(&format!("{{\"ProcessId\":{}}}", other)));
        let _ = sleeper.kill();
        assert_eq!(hr, S_OK);
        let r = parse(&t);
        assert!(array_items(&r, "ProcessReports").is_empty());
        let e = child(&r, "XtfOperationError").unwrap();
        assert_eq!(array_strings(&e, "Errors"), vec![format!("Failed (80070057) to query memory for process {}", other)]);
    }
}

#[test]
fn title_memory_report_all_processes_lists_only_titles() {
    let _g = TITLEMEM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        WS_FAIL.store(0, Ordering::Release);
        let mut ids = Vec::new();
        let mut n = 0u32;
        assert_eq!(titlememory::get_all_process_ids(&mut ids, &mut n), S_OK);
        let ids = ids[..n as usize].to_vec();
        assert!(ids.contains(&GetCurrentProcessId()));
        assert!(ids.iter().all(|p| titles::is_title_process(*p)));
        let (hr, t) = call(titlememory::title_memory_report_handler, None);
        assert_eq!(hr, S_OK);
        let r = parse(&t);
        let procs = array_items(&r, "ProcessReports");
        assert!(procs.iter().any(|p| num(p, "ProcessId") == Some(GetCurrentProcessId() as f64)));
        assert!(procs.iter().all(|p| titles::is_title_process(num(p, "ProcessId").unwrap() as u32)));
    }
}

#[test]
fn title_memory_report_working_set_failure_is_returned_in_operation_error() {
    let _g = TITLEMEM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        WS_FAIL.store(5, Ordering::Release);
        let (hr, t) = call(titlememory::title_memory_report_handler, Some("{\"ProcessId\":4}"));
        WS_FAIL.store(0, Ordering::Release);
        assert_eq!(hr, S_OK);
        let r = parse(&t);
        assert!(child(&r, "PartitionReport").is_none());
        let e = child(&r, "XtfOperationError").unwrap();
        assert_eq!(num(&e, "ReturnCode"), Some(0x8007_0005u32 as i32 as f64));
    }
}

#[test]
fn xmem_fixture_resolves() {
    assert!(xmemi::get().is_ok());
}

#[repr(C)]
struct MockSession {
    vtbl: *const MsVtbl,
    sent: Mutex<Vec<String>>,
    accept: u32,
    child: Mutex<Option<Child>>,
}

unsafe extern "system" fn ms_qi(this: Obj, _iid: *const GUID, out: *mut Obj) -> HRESULT {
    *out = this;
    S_OK
}

unsafe extern "system" fn ms_ref(_this: Obj) -> u32 {
    1
}

unsafe extern "system" fn ms_send_more(this: Obj, msg: Obj) -> HRESULT {
    let s = &*(this as *const MockSession);
    let mut n = 0u32;
    let mut p: *mut u8 = null_mut();
    assert_eq!(msg_get_json_body(msg, &mut n, &mut p), S_OK);
    let text = String::from_utf8(core::slice::from_raw_parts(p, n as usize).iter().copied().take_while(|c| *c != 0).collect()).unwrap();
    let mut sent = s.sent.lock().unwrap_or_else(|e| e.into_inner());
    sent.push(text);
    if sent.len() == 1 {
        if let Some(c) = s.child.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    if (sent.len() as u32) <= s.accept {
        S_OK
    } else {
        0x8C11_0303u32 as i32
    }
}

#[repr(C)]
struct MsVtbl {
    qi: unsafe extern "system" fn(Obj, *const GUID, *mut Obj) -> HRESULT,
    add_ref: unsafe extern "system" fn(Obj) -> u32,
    release: unsafe extern "system" fn(Obj) -> u32,
    send_more: unsafe extern "system" fn(Obj, Obj) -> HRESULT,
}

static MS_VTBL: MsVtbl = MsVtbl { qi: ms_qi, add_ref: ms_ref, release: ms_ref, send_more: ms_send_more };

unsafe fn entry_for(list: &[Ptr], pid: u32) -> Option<&Ptr> {
    list.iter().find(|e| num(e, "Pid") == Some(pid as f64))
}

#[test]
fn process_delta_snapshot_lists_title_processes() {
    unsafe {
        let me = GetCurrentProcessId();
        let (hr, t) = call(processdelta::process_information_delta_update_handler, None);
        assert_eq!(hr, S_OK, "{}", t);
        let r = parse(&t);
        assert_eq!(num(&r, "Result"), Some(0.0));
        let list = array_items(&r, "ProcessEntries");
        assert!(list.iter().all(|e| titles::is_title_process(num(e, "Pid").unwrap() as u32)));
        let e = entry_for(&list, me).expect("own process listed");
        assert_eq!(text(e, "Action").as_deref(), Some("Changed"));
        assert!(num(e, "ThreadCount").unwrap() >= 1.0);
        assert!(num(e, "HandleCount").unwrap() >= 1.0);
        assert!(u64_field(e, "WorkingSetPrivate").unwrap() > 0);
        assert_eq!(num(e, "DebuggerAttached"), Some(0.0));
        let name = text(e, "ImagePath").unwrap();
        let full = text(e, "ImageFullPath").unwrap();
        assert!(full.to_ascii_lowercase().ends_with(&name.to_ascii_lowercase()), "{} {}", name, full);
        assert!(name.to_ascii_lowercase().ends_with(".exe"));
    }
}

#[test]
fn process_delta_counts_suspended_threads() {
    unsafe {
        let pi = spawn_suspended("cmd /c exit 0");
        titles::register_title_process(pi.pid);
        let mut data = Vec::new();
        let mut base = processdelta::ProcessMap::default();
        let mut same = processdelta::ProcessMap::default();
        let mut fresh = processdelta::ProcessMap::default();
        let hr = processdelta::build_delta_process_list(&mut data, &mut base, &mut same, &mut fresh);
        assert!(hr >= 0);
        let e = fresh.0.iter().find(|e| e.pid == pi.pid).cloned().expect("suspended child listed");
        assert_eq!(e.thread_count, 1);
        assert_eq!(e.suspended_thread_count, 1);
        base = processdelta::ProcessMap(fresh.0.clone());
        let mut same2 = processdelta::ProcessMap::default();
        let mut fresh2 = processdelta::ProcessMap::default();
        assert!(processdelta::build_delta_process_list(&mut data, &mut base, &mut same2, &mut fresh2) >= 0);
        assert!(same2.0.iter().any(|x| x.pid == pi.pid) || fresh2.0.iter().any(|x| x.pid == pi.pid));
        assert!(base.find(pi.pid).is_none());
        TerminateProcess(pi.process, 0);
        WaitForSingleObject(pi.process, 5000);
        CloseHandle(pi.thread);
        CloseHandle(pi.process);
        titles::unregister_title_process(pi.pid);
    }
}

#[test]
fn process_delta_streams_until_send_fails() {
    unsafe {
        let sleeper = spawn_sleeper();
        let cpid = sleeper.id();
        titles::register_title_process(cpid);
        let session = MockSession { vtbl: &MS_VTBL, sent: Mutex::new(Vec::new()), accept: 1, child: Mutex::new(Some(sleeper)) };
        let req = new_message(Some("{\"RefreshRate\":1}"));
        let rep = new_reply();
        let start = std::time::Instant::now();
        let hr = processdelta::process_information_delta_update_handler(&session as *const MockSession as usize, req.0 as usize, rep.0 as usize);
        titles::unregister_title_process(cpid);
        assert_eq!(hr, S_OK);
        assert!(start.elapsed() >= std::time::Duration::from_millis(1000));
        let r = parse(&reply_text(&rep));
        assert_eq!(num(&r, "Result"), Some(0.0));
        assert!(child(&r, "ProcessEntries").is_none());
        let sent = session.sent.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(sent.len(), 2);
        let first = array_items(&parse(&sent[0]), "ProcessEntries");
        assert_eq!(text(entry_for(&first, cpid).unwrap(), "Action").as_deref(), Some("Changed"));
        assert_eq!(text(entry_for(&first, GetCurrentProcessId()).unwrap(), "Action").as_deref(), Some("Changed"));
        let second = array_items(&parse(&sent[1]), "ProcessEntries");
        let gone = entry_for(&second, cpid).expect("deleted entry");
        assert_eq!(text(gone, "Action").as_deref(), Some("Deleted"));
        assert!(num(gone, "ThreadCount").is_none());
    }
}

#[test]
fn imports_resolve_in_tests() {
    assert!(imports::get().is_ok());
}

#[test]
fn export_table_matches_console_dll() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/exports.txt")).unwrap();
    xtfcommon::exports::check_against_titles("xtfdiaginfo.dll", &text);
    for e in xtfcommon::exports::parse(&text) {
        if e.status != "missing" {
            assert!(xtfcommon::exports::source_defines(concat!(env!("CARGO_MANIFEST_DIR"), "/src"), &e));
        }
    }
}
