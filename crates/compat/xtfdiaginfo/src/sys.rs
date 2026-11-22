use core::ffi::c_void;

pub type HRESULT = i32;
pub type HANDLE = *mut c_void;
pub type BOOL = i32;
pub type BSTR = *mut u16;

pub const S_OK: HRESULT = 0;
pub const E_FAIL: HRESULT = 0x8000_4005u32 as i32;
pub const E_NOTIMPL: HRESULT = 0x8000_4001u32 as i32;
pub const E_POINTER: HRESULT = 0x8000_4003u32 as i32;
pub const E_INVALIDARG: HRESULT = 0x8007_0057u32 as i32;
pub const E_OUTOFMEMORY: HRESULT = 0x8007_000Eu32 as i32;
pub const HR_NOT_SUPPORTED: HRESULT = 0x8007_0032u32 as i32;
pub const HR_PATH_NOT_FOUND: HRESULT = 0x8007_0003u32 as i32;
pub const E_JSON_NOT_FOUND: HRESULT = 0x8375_0009u32 as i32;
pub const E_PROCESS_NOT_TRACKED: HRESULT = 0x8C12_01FEu32 as i32;
pub const E_TITLE_VM_NOT_AVAILABLE: HRESULT = 0x8C11_040Du32 as i32;
pub const STATUS_VM_PARTITION_UNAVAILABLE: HRESULT = 0xC035_000Du32 as i32;
pub const STATUS_NO_MORE_ENTRIES: i32 = 0x8000_001Au32 as i32;

pub const INVALID_HANDLE_VALUE: HANDLE = usize::MAX as HANDLE;
pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;
pub const ERROR_INVALID_PARAMETER: u32 = 87;
pub const ERROR_SEM_TIMEOUT: u32 = 0x79;
pub const ERROR_PIPE_CONNECTED: u32 = 0x217;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct GUID {
    pub d1: u32,
    pub d2: u16,
    pub d3: u16,
    pub d4: [u8; 8],
}

impl GUID {
    pub const fn new(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Self {
        GUID { d1, d2, d3, d4 }
    }
}

#[repr(C)]
pub struct DebugEvent {
    pub code: u32,
    pub pid: u32,
    pub tid: u32,
    pub pad: u32,
    pub u: [u64; 20],
}

pub type ThreadProc = unsafe extern "system" fn(*mut c_void) -> u32;

#[link(name = "kernel32")]
extern "system" {
    pub fn GetLastError() -> u32;
    pub fn SetLastError(e: u32);
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn GetTickCount64() -> u64;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn OpenProcess(access: u32, inherit: BOOL, pid: u32) -> HANDLE;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    pub fn DebugActiveProcess(pid: u32) -> BOOL;
    pub fn DebugActiveProcessStop(pid: u32) -> BOOL;
    pub fn WaitForDebugEvent(ev: *mut DebugEvent, ms: u32) -> BOOL;
    pub fn ContinueDebugEvent(pid: u32, tid: u32, status: u32) -> BOOL;
    pub fn SuspendThread(h: HANDLE) -> u32;
    pub fn ResumeThread(h: HANDLE) -> u32;
    pub fn GetThreadId(h: HANDLE) -> u32;
    pub fn OpenThread(access: u32, inherit: BOOL, tid: u32) -> HANDLE;
    pub fn CreateThread(sa: *mut c_void, stack: usize, start: ThreadProc, param: *mut c_void, flags: u32, tid: *mut u32) -> HANDLE;
    pub fn SetThreadPriority(h: HANDLE, prio: i32) -> BOOL;
    pub fn Sleep(ms: u32);
    pub fn GetFileAttributesW(name: *const u16) -> u32;
    pub fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void, disp: u32, flags: u32, tmpl: HANDLE) -> HANDLE;
    pub fn DeviceIoControl(h: HANDLE, code: u32, inb: *const c_void, inn: u32, outb: *mut c_void, outn: u32, ret: *mut u32, ov: *mut c_void) -> BOOL;
    pub fn CreateNamedPipeW(name: *const u16, open: u32, mode: u32, max: u32, out: u32, inb: u32, timeout: u32, sa: *mut c_void) -> HANDLE;
    pub fn ConnectNamedPipe(h: HANDLE, ov: *mut c_void) -> BOOL;
    pub fn ReadFile(h: HANDLE, buf: *mut c_void, n: u32, read: *mut u32, ov: *mut c_void) -> BOOL;
    pub fn WriteFile(h: HANDLE, buf: *const c_void, n: u32, written: *mut u32, ov: *mut c_void) -> BOOL;
    pub fn FlushFileBuffers(h: HANDLE) -> BOOL;
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
    pub fn LoadLibraryExW(name: *const u16, file: HANDLE, flags: u32) -> HANDLE;
    pub fn GetProcAddress(m: HANDLE, name: *const u8) -> *mut c_void;
    pub fn K32EnumProcesses(pids: *mut u32, cb: u32, needed: *mut u32) -> BOOL;
    pub fn QueryFullProcessImageNameW(h: HANDLE, flags: u32, name: *mut u16, size: *mut u32) -> BOOL;
    pub fn K32GetModuleFileNameExW(h: HANDLE, module: HANDLE, name: *mut u16, size: u32) -> u32;
}

#[link(name = "ntdll")]
extern "system" {
    pub fn DbgUiIssueRemoteBreakin(h: HANDLE) -> i32;
    pub fn NtQueryInformationProcess(h: HANDLE, class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
    pub fn NtGetNextThread(process: HANDLE, thread: HANDLE, access: u32, attributes: u32, flags: u32, next: *mut HANDLE) -> i32;
    pub fn RtlNtStatusToDosError(status: i32) -> u32;
    pub fn NtQuerySystemInformation(class: u32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    pub fn IIDFromString(s: *const u16, iid: *mut GUID) -> HRESULT;
}

#[link(name = "oleaut32")]
extern "system" {
    pub fn SysAllocString(s: *const u16) -> BSTR;
    pub fn SysAllocStringLen(s: *const u16, n: u32) -> BSTR;
    pub fn SysFreeString(s: BSTR);
    pub fn SysStringLen(s: BSTR) -> u32;
}

pub fn hr_from_win32(e: u32) -> HRESULT {
    if (e as i32) <= 0 {
        e as i32
    } else {
        ((e & 0xFFFF) | 0x8007_0000) as i32
    }
}

pub fn hr_last_error() -> HRESULT {
    hr_from_win32(unsafe { GetLastError() })
}

pub fn hr_from_nt(status: i32) -> HRESULT {
    status | 0x1000_0000
}

pub fn wz(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

pub unsafe fn wslice<'a>(p: *const u16) -> &'a [u16] {
    if p.is_null() {
        return &[];
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    core::slice::from_raw_parts(p, n)
}

pub fn cvtt_i64(d: f64) -> i64 {
    if d.is_nan() || d >= 9.223_372_036_854_775_808e18 || d < -9.223_372_036_854_775_808e18 {
        i64::MIN
    } else {
        d as i64
    }
}
