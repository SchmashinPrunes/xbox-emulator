use crate::com::*;
use crate::imports;
use crate::messaging::*;
use crate::process;
use crate::sys::*;
use crate::titlememory::set_u64;
use crate::titles;
use core::ffi::c_void;
use core::mem::transmute;

pub const SYSTEM_PROCESS_INFORMATION_CLASS: u32 = 5;
pub const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;
pub const SNAPSHOT_GROWTH: usize = 0x3e8;
pub const MIN_REFRESH_MS: u32 = 0x3e8;
pub const IID_XBTP_MESSAGE_IMPL: GUID = GUID::new(0x6965e04a, 0x2829, 0x41a8, [0xb1, 0x94, 0x48, 0x73, 0x15, 0x07, 0x5e, 0x7a]);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub thread_count: u32,
    pub suspended_thread_count: u32,
    pub handle_count: u32,
    pub working_set_private: u64,
    pub debugger_attached: bool,
    pub image_name: Vec<u16>,
    pub image_path: Vec<u16>,
}

#[derive(Default)]
pub struct ProcessMap(pub Vec<ProcessEntry>);

impl ProcessMap {
    pub fn find(&self, pid: u32) -> Option<usize> {
        self.0.iter().position(|e| e.pid == pid)
    }

    pub fn insert(&mut self, e: ProcessEntry) {
        if self.find(e.pid).is_none() {
            self.0.push(e);
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

fn eq_ignore_case(a: &[u16], b: &[u16]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| lower(*x) == lower(*y))
}

fn lower(c: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&c) {
        c + 32
    } else {
        c
    }
}

unsafe fn rd<T: Copy>(b: &[u8], o: usize) -> T {
    core::ptr::read_unaligned(b.as_ptr().add(o) as *const T)
}

unsafe fn image_name(b: &[u8], o: usize) -> Vec<u16> {
    let len: u16 = rd(b, o + 0x38);
    let p: usize = rd(b, o + 0x40);
    if p == 0 {
        return Vec::new();
    }
    let s = core::slice::from_raw_parts(p as *const u16, (len / 2) as usize);
    s.iter().copied().take_while(|c| *c != 0).collect()
}

unsafe fn query_process(pid: u32, debugger: &mut bool, path: &mut Vec<u16>) {
    let h = titles::open_process(0x400, pid);
    if h.is_null() {
        return;
    }
    let mut port: usize = 0;
    let st = NtQueryInformationProcess(h, 7, &mut port as *mut usize as *mut c_void, 8, core::ptr::null_mut());
    *debugger = st >= 0 && port != 0;
    let mut buf = vec![0u16; 0x104];
    let n = K32GetModuleFileNameExW(h, core::ptr::null_mut(), buf.as_mut_ptr(), 0x104);
    path.clear();
    if n != 0 {
        path.extend(buf.iter().copied().take_while(|c| *c != 0));
    }
    CloseHandle(h);
}

pub unsafe fn build_delta_process_list(data: &mut Vec<u8>, baseline: &mut ProcessMap, unchanged: &mut ProcessMap, refreshed: &mut ProcessMap) -> HRESULT {
    let mut ret = 0u32;
    let st = loop {
        let st = NtQuerySystemInformation(SYSTEM_PROCESS_INFORMATION_CLASS, data.as_mut_ptr() as *mut c_void, data.len() as u32, &mut ret);
        if st != STATUS_INFO_LENGTH_MISMATCH {
            break st;
        }
        let n = data.len() + SNAPSHOT_GROWTH;
        data.resize(n, 0);
    };
    let hr = hr_from_nt(st);
    if hr < 0 || ret == 0 {
        return hr;
    }
    let mut o = 0usize;
    loop {
        let pid: u32 = rd(data, o + 0x50);
        if titles::is_title_process(pid) {
            let threads: u32 = rd(data, o + 4);
            let suspended = if process::is_tracked(pid) {
                threads
            } else {
                (0..threads as usize).filter(|t| {
                    let base = o + 0x100 + t * 0x50;
                    rd::<u32>(data, base + 0x44) == 5 && rd::<u32>(data, base + 0x48) == 5
                }).count() as u32
            };
            let mut debugger = false;
            let mut path = Vec::new();
            query_process(pid, &mut debugger, &mut path);
            let cur = ProcessEntry {
                pid,
                thread_count: threads,
                suspended_thread_count: suspended,
                handle_count: rd(data, o + 0x60),
                working_set_private: rd(data, o + 8),
                debugger_attached: debugger,
                image_name: image_name(data, o),
                image_path: path,
            };
            match baseline.find(pid) {
                Some(i) => {
                    let mut e = baseline.0.remove(i);
                    let same = e.thread_count == cur.thread_count
                        && e.suspended_thread_count == cur.suspended_thread_count
                        && e.handle_count == cur.handle_count
                        && e.working_set_private == cur.working_set_private
                        && e.debugger_attached == cur.debugger_attached
                        && eq_ignore_case(&e.image_name, &cur.image_name);
                    if same {
                        unchanged.insert(e);
                    } else {
                        e = cur;
                        refreshed.insert(e);
                    }
                }
                None => refreshed.insert(cur),
            }
        }
        let next: u32 = rd(data, o);
        if next == 0 {
            break;
        }
        o += next as usize;
    }
    hr
}

pub unsafe fn serialize_entry(e: &ProcessEntry, deleted: bool, array: Obj) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(_) => return S_OK,
    };
    let mut o = Ptr::null();
    if (i.create_json_object)(o.out()) < 0 {
        return S_OK;
    }
    if set_u32(o.0, &wz("Pid"), e.pid) < 0 {
        return S_OK;
    }
    if deleted {
        if obj_set_string(o.0, wz("Action").as_ptr(), wz("Deleted").as_ptr()) >= 0 {
            arr_append_object(array, o.0);
        }
        return S_OK;
    }
    if obj_set_string(o.0, wz("Action").as_ptr(), wz("Changed").as_ptr()) < 0 {
        return S_OK;
    }
    if set_u32(o.0, &wz("ThreadCount"), e.thread_count) < 0 {
        return S_OK;
    }
    if set_u32(o.0, &wz("SuspendedThreadCount"), e.suspended_thread_count) < 0 {
        return S_OK;
    }
    if set_u32(o.0, &wz("HandleCount"), e.handle_count) < 0 {
        return S_OK;
    }
    if set_u64(o.0, &wz("WorkingSetPrivate"), e.working_set_private) < 0 {
        return S_OK;
    }
    if set_i32(o.0, &wz("DebuggerAttached"), e.debugger_attached as i32) < 0 {
        return S_OK;
    }
    let mut name = e.image_name.clone();
    name.push(0);
    if obj_set_string(o.0, wz("ImagePath").as_ptr(), name.as_ptr()) < 0 {
        return S_OK;
    }
    let mut path = e.image_path.clone();
    path.push(0);
    if obj_set_string(o.0, wz("ImageFullPath").as_ptr(), path.as_ptr()) < 0 {
        return S_OK;
    }
    arr_append_object(array, o.0);
    S_OK
}

unsafe fn serialize_more(body: Obj, message: Obj) -> HRESULT {
    if body.is_null() || message.is_null() {
        return E_INVALIDARG;
    }
    let mut w = Bstr::null();
    let hr = obj_stringify(body, w.out());
    if hr < 0 {
        return hr;
    }
    let f: unsafe extern "system" fn(Obj, *const u16) -> HRESULT = transmute(slot(message, 7));
    let hr = f(message, w.0);
    if hr < 0 {
        hr
    } else {
        S_OK
    }
}

unsafe fn session_send_more(session: Obj, message: Obj) -> HRESULT {
    let f: unsafe extern "system" fn(Obj, Obj) -> HRESULT = transmute(slot(session, 3));
    f(session, message)
}

unsafe fn stream(session: Obj, rate: u32, data: &mut Vec<u8>, maps: &mut [ProcessMap; 3]) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    loop {
        let mut count = 0u32;
        let mut entries = Ptr::null();
        let hr = (i.create_json_array)(entries.out());
        if hr < 0 {
            return hr;
        }
        let mut more = Ptr::null();
        (i.create_message)(&IID_XBTP_MESSAGE_IMPL, more.out());
        let mut delta = Ptr::null();
        (i.create_json_object)(delta.out());
        let [baseline, unchanged, refreshed] = maps;
        let hr = build_delta_process_list(data, baseline, unchanged, refreshed);
        if hr < 0 {
            return hr;
        }
        for e in baseline.0.iter() {
            count += 1;
            serialize_entry(e, true, entries.0);
        }
        baseline.0 = core::mem::take(&mut unchanged.0);
        for e in refreshed.0.drain(..) {
            count += 1;
            serialize_entry(&e, false, entries.0);
            baseline.insert(e);
        }
        if count != 0 {
            let hr = obj_set_array(delta.0, wz("ProcessEntries").as_ptr(), entries.0);
            if hr < 0 {
                return hr;
            }
            let hr = serialize_more(delta.0, more.0);
            if hr < 0 {
                return hr;
            }
            if session_send_more(session, more.0) < 0 {
                return S_OK;
            }
        }
        drop(delta);
        drop(more);
        drop(entries);
        Sleep(rate);
    }
}

unsafe fn snapshot(data: &mut Vec<u8>, maps: &mut [ProcessMap; 3], response: Obj) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut entries = Ptr::null();
    let hr = (i.create_json_array)(entries.out());
    if hr < 0 {
        return hr;
    }
    let [baseline, unchanged, refreshed] = maps;
    let hr = build_delta_process_list(data, baseline, unchanged, refreshed);
    if hr < 0 {
        return hr;
    }
    for e in refreshed.0.iter() {
        serialize_entry(e, false, entries.0);
    }
    obj_set_array(response, wz("ProcessEntries").as_ptr(), entries.0)
}

pub unsafe extern "system" fn process_information_delta_update_handler(session: usize, request: usize, reply: usize) -> HRESULT {
    let mut data: Vec<u8> = Vec::new();
    let mut maps: [ProcessMap; 3] = Default::default();
    let mut response = Ptr::null();
    let mut body = Ptr::null();
    let hr = 'run: {
        let hr = match imports::get() {
            Ok(i) => (i.create_json_object)(response.out()),
            Err(e) => e,
        };
        if hr < 0 {
            break 'run hr;
        }
        let hr = deserialize_body(request as *mut c_void, body.out());
        if hr < 0 {
            break 'run hr;
        }
        let mut rate = 0u32;
        if get_u32(body.0, &wz("RefreshRate"), &mut rate) < 0 {
            rate = 0;
        }
        if rate == 0 {
            break 'run snapshot(&mut data, &mut maps, response.0);
        }
        stream(session as Obj, rate.max(MIN_REFRESH_MS), &mut data, &mut maps)
    };
    set_i32(response.0, &wz("Result"), hr);
    serialize_body(response.0, reply as *mut c_void)
}
