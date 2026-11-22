use crate::com::*;
use crate::imports;
use crate::sys::*;
use core::ptr::null_mut;

pub unsafe fn get_u32(o: Obj, name: &[u16], out: *mut u32) -> HRESULT {
    if o.is_null() || out.is_null() {
        return E_INVALIDARG;
    }
    let mut d = 0f64;
    let hr = obj_get_number(o, name.as_ptr(), &mut d);
    if hr == E_JSON_NOT_FOUND || hr < 0 {
        return hr;
    }
    *out = cvtt_i64(d) as u32;
    hr
}

pub unsafe fn set_i32(o: Obj, name: &[u16], v: i32) -> HRESULT {
    if o.is_null() {
        return E_INVALIDARG;
    }
    obj_set_number(o, name.as_ptr(), v as f64)
}

pub unsafe fn set_u32(o: Obj, name: &[u16], v: u32) -> HRESULT {
    if o.is_null() {
        return E_INVALIDARG;
    }
    obj_set_number(o, name.as_ptr(), v as f64)
}

pub unsafe fn set_bool(o: Obj, name: &[u16], v: bool) -> HRESULT {
    if o.is_null() {
        return E_INVALIDARG;
    }
    obj_set_number(o, name.as_ptr(), v as i32 as f64)
}

unsafe fn setup_json_body(unparsed: *const u8, body: *mut Obj) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    let hr = if unparsed.is_null() { (i.create_json_object)(body) } else { (i.get_json_object_from_utf8)(unparsed, body) };
    if hr < 0 {
        hr
    } else {
        S_OK
    }
}

pub unsafe fn deserialize_body(message: Obj, body: *mut Obj) -> HRESULT {
    if message.is_null() || body.is_null() {
        return E_INVALIDARG;
    }
    let mut len = 0u32;
    let mut unparsed: *mut u8 = null_mut();
    let hr = msg_get_json_body(message, &mut len, &mut unparsed);
    if hr < 0 {
        return hr;
    }
    setup_json_body(unparsed, body)
}

pub unsafe fn serialize_body(body: Obj, message: Obj) -> HRESULT {
    if body.is_null() || message.is_null() {
        return E_INVALIDARG;
    }
    let mut w = Bstr::null();
    let hr = obj_stringify(body, w.out());
    if hr < 0 {
        return hr;
    }
    let hr = reply_set_json_body_w(message, w.0);
    if hr < 0 {
        hr
    } else {
        S_OK
    }
}

pub unsafe fn stringify_into_reply(reply_msg: Obj, reply: Obj) -> HRESULT {
    let mut w = Bstr::null();
    let hr = obj_stringify(reply_msg, w.out());
    if hr < 0 {
        return hr;
    }
    reply_set_json_body_w(reply, w.0)
}

pub fn error_text(hr: HRESULT, file: &str, line: u32) -> Vec<u16> {
    format!("Error 0x{:X} File {} : {}", hr as u32, file, line).encode_utf16().chain(core::iter::once(0)).collect()
}

pub struct ErrorContext {
    pub errors: Obj,
    pub body: Obj,
    pub end_message: Obj,
    pub checkpoint: u32,
    pub hr: HRESULT,
    pub completed: bool,
}

impl ErrorContext {
    pub fn new(end_message: Obj) -> Self {
        ErrorContext { errors: null_mut(), body: null_mut(), end_message, checkpoint: 0, hr: S_OK, completed: false }
    }

    unsafe fn create_errors_array(&mut self) -> HRESULT {
        if !self.errors.is_null() {
            return S_OK;
        }
        let i = match imports::get() {
            Ok(i) => i,
            Err(e) => return e,
        };
        let hr = (i.create_json_array)(&mut self.errors);
        if hr < 0 {
            hr
        } else {
            S_OK
        }
    }

    pub unsafe fn add_error(&mut self, hr: HRESULT, file: &str, line: u32) -> HRESULT {
        self.add_error_text(&error_text(hr, file, line))
    }

    pub unsafe fn add_error_text(&mut self, text: &[u16]) -> HRESULT {
        let s = Bstr(SysAllocString(text.as_ptr()));
        if s.0.is_null() {
            return E_OUTOFMEMORY;
        }
        let r = self.create_errors_array();
        if r < 0 {
            return r;
        }
        let r = arr_append_string(self.errors, s.0);
        if r < 0 {
            r
        } else {
            S_OK
        }
    }

    pub unsafe fn set_body(&mut self, body: Obj) {
        let old = core::mem::replace(&mut self.body, null_mut());
        if !old.is_null() {
            release(old);
        }
        self.body = body;
        if !body.is_null() {
            add_ref(body);
        }
    }

    pub unsafe fn complete(&mut self, hr: &mut HRESULT) {
        if self.end_message.is_null() {
            return;
        }
        if self.hr >= 0 && *hr >= 0 && self.body.is_null() {
            return;
        }
        let i = match imports::get() {
            Ok(i) => i,
            Err(_) => return,
        };
        let mut tmp = Ptr::null();
        let body = if self.body.is_null() {
            if deserialize_body(self.end_message, tmp.out()) < 0 {
                return;
            }
            tmp.0
        } else {
            self.body
        };
        let mut eb = Ptr::null();
        if (i.create_json_object)(eb.out()) < 0 {
            return;
        }
        let code = if self.hr < 0 { self.hr } else { *hr };
        if obj_set_number(eb.0, wz("ReturnCode").as_ptr(), code as f64) < 0 {
            return;
        }
        if obj_set_number(eb.0, wz("Checkpoint").as_ptr(), self.checkpoint as f64) < 0 {
            return;
        }
        if self.create_errors_array() < 0 {
            return;
        }
        if obj_set_array(eb.0, wz("Errors").as_ptr(), self.errors) < 0 {
            return;
        }
        if obj_set_object(body, wz("XtfOperationError").as_ptr(), eb.0) < 0 {
            return;
        }
        if serialize_body(body, self.end_message) < 0 {
            return;
        }
        *hr = S_OK;
        self.completed = true;
    }
}

impl Drop for ErrorContext {
    fn drop(&mut self) {
        unsafe {
            if !self.completed {
                let mut h = self.hr;
                self.complete(&mut h);
            }
            let b = core::mem::replace(&mut self.body, null_mut());
            if !b.is_null() {
                release(b);
            }
            let e = core::mem::replace(&mut self.errors, null_mut());
            if !e.is_null() {
                release(e);
            }
        }
    }
}

pub unsafe fn complete_operation_handler(reply: Obj, body: Obj, handler_result: &mut HRESULT) {
    let mut hr = S_OK;
    'done: {
        if body.is_null() {
            break 'done;
        }
        let i = match imports::get() {
            Ok(i) => i,
            Err(e) => {
                hr = e;
                break 'done;
            }
        };
        if *handler_result < 0 {
            hr = set_bool(body, &wz("Successful"), false);
            if hr < 0 {
                break 'done;
            }
            let mut result = Ptr::null();
            hr = (i.create_json_object)(result.out());
            if hr < 0 {
                break 'done;
            }
            hr = set_i32(result.0, &wz("ResultCode"), *handler_result);
            if hr < 0 {
                break 'done;
            }
            hr = obj_set_object(body, wz("Result").as_ptr(), result.0);
            if hr < 0 {
                break 'done;
            }
            *handler_result = S_OK;
        } else {
            hr = set_bool(body, &wz("Successful"), true);
            if hr < 0 {
                break 'done;
            }
        }
        hr = serialize_body(body, reply);
    }
    if *handler_result >= 0 {
        *handler_result = hr;
    }
}
