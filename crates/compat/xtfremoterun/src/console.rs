use crate::chk::chk;
use crate::win::*;
use core::ffi::c_void;
use core::ptr::{null, null_mut};
use xtfmsg::com::{self, json, Com, Obj, IID_XBTP_MESSAGE};
use xtfmsg::msg::{deserialize_body, serialize_body};
use xtfmsg::sys::*;
use xtfmsg::xbtp;

pub const RUN_CPP: &str = "xbox\\devex\\xtf\\dll\\remoterun\\run.cpp";

pub const MSGID_RUN_INPUT: u32 = 0;
pub const MSGID_RUN_OUTPUT: u32 = 1;
pub const MSGID_RUN_RESULT: u32 = 2;
pub const MSGID_RUN_ERROR: u32 = 3;

pub const P_MSG_ID: &str = "MsgId";
pub const P_RUN_INPUT: &str = "Run_Input";
pub const P_RUN_OUTPUT: &str = "Run_Output";
pub const P_RUN_ERROR_OUTPUT: &str = "Run_Error_Output";
pub const P_FLAGS: &str = "dwFlags";
pub const P_PERIOD: &str = "dwPeriod";
pub const P_COMMAND_LINE: &str = "CommandLine";
pub const P_WORKING_DIRECTORY: &str = "WorkingDirectory";
pub const P_RESULT: &str = "Result";
pub const P_EXIT_CODE: &str = "ExitCode";

pub fn ck(func: &str, line: u32, name: &str, hr: HRESULT, msg: &str) {
    chk(RUN_CPP, func, line, name, hr, msg);
}

pub fn w(s: &str) -> Vec<u16> {
    wide(s)
}

pub unsafe fn free_bstr(b: BSTR) {
    if !b.is_null() {
        SysFreeString(b);
    }
}

pub unsafe fn read_console(h: HANDLE, out: &mut Option<Vec<u8>>) -> HRESULT {
    *out = None;
    let mut avail = 0u32;
    if PeekNamedPipe(h, null_mut(), 0, null_mut(), &mut avail, null_mut()) == 0 {
        return last_error_or_fail();
    }
    if avail == 0 {
        return S_OK;
    }
    let mut buf = vec![0u8; avail as usize + 1];
    let mut read = 0u32;
    if ReadFile(h, buf.as_mut_ptr() as *mut c_void, avail, &mut read, null_mut()) == 0 {
        return last_error_or_fail();
    }
    buf[read as usize] = 0;
    buf.truncate(read as usize + 1);
    *out = Some(buf);
    S_OK
}

pub unsafe fn set_ansi(text: *const u8, out: &mut BSTR) -> HRESULT {
    *out = null_mut();
    let n = MultiByteToWideChar(CP_ACP, 0, text, -1, null_mut(), 0);
    if n <= 0 {
        return last_error_or_fail();
    }
    let b = SysAllocStringLen(null(), (n - 1) as u32);
    if b.is_null() {
        return E_OUTOFMEMORY;
    }
    if MultiByteToWideChar(CP_ACP, 0, text, -1, b, n) == 0 {
        let hr = last_error_or_fail();
        SysFreeString(b);
        return hr;
    }
    *out = b;
    S_OK
}

pub unsafe fn read_console_w(h: HANDLE, out: &mut BSTR) -> HRESULT {
    *out = null_mut();
    let mut ansi = None;
    let hr = read_console(h, &mut ansi);
    ck("ReadConsoleW", 55, "ReadConsole(hConsole, &pszAnsi)", hr, "");
    if hr < 0 {
        return hr;
    }
    match ansi {
        Some(a) if a[0] != 0 => {
            let mut b: BSTR = null_mut();
            let hr = set_ansi(a.as_ptr(), &mut b);
            ck("ReadConsoleW", 59, "strText.SetAnsi(pszAnsi)", hr, "");
            if hr < 0 {
                return hr;
            }
            *out = b;
            S_OK
        }
        _ => S_OK,
    }
}

pub unsafe fn write_console(h: HANDLE, text: &[u8]) -> HRESULT {
    let n = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    if n == 0 {
        return S_OK;
    }
    let mut written = 0u32;
    if WriteFile(h, text.as_ptr() as *const c_void, n as u32, &mut written, null_mut()) == 0 {
        return last_error_or_fail();
    }
    if written as usize != n {
        ck("WriteConsoleW", 96, "E_UNEXPECTED", E_UNEXPECTED, "Console pipe write truncated");
        return E_UNEXPECTED;
    }
    S_OK
}

pub unsafe fn unicode_to_ansi(text: *const u16, buf: *mut u8, cc: i32, needed: Option<&mut i32>) -> HRESULT {
    let n = WideCharToMultiByte(CP_ACP, 0, text, -1, buf, cc, null(), null_mut());
    if n == 0 {
        return last_error_or_fail();
    }
    if let Some(o) = needed {
        *o = n;
    }
    S_OK
}

pub unsafe fn write_console_w(h: HANDLE, text: *const u16) -> HRESULT {
    let mut cc = 0i32;
    let hr = unicode_to_ansi(text, null_mut(), 0, Some(&mut cc));
    ck("WriteConsoleW", 117, "UnicodeToAnsi(pszText, nullptr, 0, &ccText)", hr, "");
    if hr < 0 {
        return hr;
    }
    if cc <= 1 {
        return S_OK;
    }
    let mut buf = vec![0u8; cc as usize];
    let hr = unicode_to_ansi(text, buf.as_mut_ptr(), cc, None);
    ck("WriteConsoleW", 125, "UnicodeToAnsi(pszText, pszAnsi, ccText, nullptr)", hr, "");
    if hr < 0 {
        return hr;
    }
    let hr = write_console(h, &buf);
    ck("WriteConsoleW", 126, "WriteConsole(hConsole, pszAnsi)", hr, "");
    hr
}

unsafe fn send_console_stream(func: &str, lines: [u32; 7], msg_id: u32, prop: &str, h: HANDLE, session: Obj, read_any: Option<&mut bool>) -> HRESULT {
    let mut text: BSTR = null_mut();
    let mut message = Com::null();
    let mut body = Com::null();
    let hr = 'run: {
        let hr = read_console_w(h, &mut text);
        ck(func, lines[0], "ReadConsole(hConsole, &str)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        if text.is_null() || *text == 0 {
            break 'run S_OK;
        }
        let hr = xbtp::create_message(&IID_XBTP_MESSAGE, message.put());
        ck(func, lines[1], "XbtpCreateMessage(IID_PPV_ARGS(&message))", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = xbtp::create_json_object(body.put());
        ck(func, lines[2], "XbtpCreateJsonObject(&msgBody)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let n = w(P_MSG_ID);
        let hr = json::set_number(body.get(), n.as_ptr(), msg_id as f64);
        ck(func, lines[3], "msgBody->SetNumber(MsgIdProperty, MsgId)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let n = w(prop);
        let hr = json::set_string(body.get(), n.as_ptr(), text);
        ck(func, lines[4], "msgBody->SetString(Property, str)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = serialize_body(body.get(), message.get());
        ck(func, lines[5], "XtfSerializeJsonBody(msgBody, message)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = com::session::send_more_message(session, message.get());
        ck(func, lines[6], "session->SendMoreMessage(message)", hr, "");
        hr
    };
    if let Some(f) = read_any {
        *f = !text.is_null() && *text != 0;
    }
    free_bstr(text);
    hr
}

pub unsafe fn send_console_output(h: HANDLE, session: Obj, read_any: Option<&mut bool>) -> HRESULT {
    send_console_stream("SendConsoleOutput", [185, 192, 193, 194, 195, 196, 197], MSGID_RUN_OUTPUT, P_RUN_OUTPUT, h, session, read_any)
}

pub unsafe fn send_console_error(h: HANDLE, session: Obj, read_any: Option<&mut bool>) -> HRESULT {
    send_console_stream("SendConsoleError", [223, 230, 231, 232, 233, 234, 235], MSGID_RUN_ERROR, P_RUN_ERROR_OUTPUT, h, session, read_any)
}

pub unsafe fn recv_console_input(h: HANDLE, reply: Obj) -> HRESULT {
    const F: &str = "RecvConsoleInput";
    let mut body = Com::null();
    let hr = deserialize_body(reply, body.put());
    ck(F, 260, "XtfDeserializeJsonBody(reply, &replyBody)", hr, "");
    if hr < 0 {
        return hr;
    }
    let mut text: BSTR = null_mut();
    let n = w(P_RUN_INPUT);
    let hr = json::get_string(body.get(), n.as_ptr(), &mut text);
    ck(F, 261, "replyBody->GetString(RunInputProperty, &psz)", hr, "");
    if hr < 0 {
        free_bstr(text);
        return hr;
    }
    let hr = write_console_w(h, text);
    ck(F, 262, "WriteConsole(hConsole, psz)", hr, "");
    free_bstr(text);
    hr
}

#[repr(C)]
pub struct RemoteRunCallbackVtbl {
    pub query_interface: unsafe extern "system" fn(Obj, *const GUID, *mut Obj) -> HRESULT,
    pub add_ref: unsafe extern "system" fn(Obj) -> u32,
    pub release: unsafe extern "system" fn(Obj) -> u32,
    pub input: unsafe extern "system" fn(Obj, *mut BSTR) -> HRESULT,
    pub output: unsafe extern "system" fn(Obj, *const u16) -> HRESULT,
}

#[repr(C)]
pub struct RemoteRunCallbackExVtbl {
    pub base: RemoteRunCallbackVtbl,
    pub error: unsafe extern "system" fn(Obj, *const u16) -> HRESULT,
}

type InputFn = unsafe extern "system" fn(Obj, *mut BSTR) -> HRESULT;
type TextFn = unsafe extern "system" fn(Obj, *const u16) -> HRESULT;
type ClientSendMoreFn = unsafe extern "system" fn(Obj, u32, Obj) -> HRESULT;

pub unsafe fn client_send_more_message(session: Obj, exchange: u32, msg: Obj) -> HRESULT {
    let f: ClientSendMoreFn = core::mem::transmute(com::slot(session, 4));
    f(session, exchange, msg)
}

pub unsafe fn send_console_input(callback: Obj, session: Obj, exchange: u32) -> HRESULT {
    const F: &str = "SendConsoleInput";
    let mut text: BSTR = null_mut();
    let input: InputFn = core::mem::transmute(com::slot(callback, 3));
    let hr = input(callback, &mut text);
    ck(F, 151, "pCallback->Input(&str)", hr, "");
    let hr = 'run: {
        if hr < 0 {
            break 'run hr;
        }
        if text.is_null() || *text == 0 {
            break 'run S_OK;
        }
        let mut message = Com::null();
        let mut body = Com::null();
        let hr = xbtp::create_message(&IID_XBTP_MESSAGE, message.put());
        ck(F, 158, "XbtpCreateMessage(IID_PPV_ARGS(&message))", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = xbtp::create_json_object(body.put());
        ck(F, 159, "XbtpCreateJsonObject(&msgBody)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let n = w(P_RUN_INPUT);
        let hr = json::set_string(body.get(), n.as_ptr(), text);
        ck(F, 160, "msgBody->SetString(RunInputProperty, str)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let n = w(P_MSG_ID);
        let hr = json::set_number(body.get(), n.as_ptr(), MSGID_RUN_INPUT as f64);
        ck(F, 161, "msgBody->SetNumber(MsgIdProperty, MsgId_Run_Input)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = serialize_body(body.get(), message.get());
        ck(F, 162, "XtfSerializeJsonBody(msgBody, message)", hr, "");
        if hr < 0 {
            break 'run hr;
        }
        let hr = client_send_more_message(session, exchange, message.get());
        ck(F, 163, "session->SendMoreMessage(exchangeId, message)", hr, "");
        hr
    };
    free_bstr(text);
    hr
}

unsafe fn recv_console_text(func: &str, lines: [u32; 3], prop: &str, call: &str, callback: Obj, slot: usize, reply: Obj) -> HRESULT {
    let mut body = Com::null();
    let hr = deserialize_body(reply, body.put());
    ck(func, lines[0], "XtfDeserializeJsonBody(reply, &replyBody)", hr, "");
    if hr < 0 {
        return hr;
    }
    let mut text: BSTR = null_mut();
    let n = w(prop);
    let hr = json::get_string(body.get(), n.as_ptr(), &mut text);
    ck(func, lines[1], "replyBody->GetString(Property, &psz)", hr, "");
    if hr < 0 {
        free_bstr(text);
        return hr;
    }
    let f: TextFn = core::mem::transmute(com::slot(callback, slot));
    let hr = f(callback, text);
    ck(func, lines[2], call, hr, "");
    free_bstr(text);
    hr
}

pub unsafe fn recv_console_output(callback: Obj, reply: Obj) -> HRESULT {
    recv_console_text("RecvConsoleOutput", [282, 283, 284], P_RUN_OUTPUT, "pCallback->Output(psz)", callback, 4, reply)
}

pub unsafe fn recv_console_error_output(callback: Obj, reply: Obj) -> HRESULT {
    recv_console_text("RecvConsoleErrorOutput", [303, 304, 305], P_RUN_ERROR_OUTPUT, "pCallback->Error(psz)", callback, 5, reply)
}
