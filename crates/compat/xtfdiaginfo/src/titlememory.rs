use crate::com::*;
use crate::imports;
use crate::messaging::*;
use crate::sys::*;
use crate::titles;
use crate::xmemi::{self, XMemBasicInformation};
use core::ffi::c_void;

pub const TITLEMEMORY_FILE: &str = "xbox\\devex\\xtf\\dll\\diaginfo\\titlememory.cpp";
pub const MEM_COMMIT: u32 = 0x1000;
pub const WS_FLAGS_NO_VA: u32 = 0x2000_0003;
pub const WS_FLAGS_WITH_VA: u32 = 0x2000_001F;

pub unsafe fn set_u64(body: Obj, name: &[u16], v: u64) -> HRESULT {
    if body.is_null() {
        return E_INVALIDARG;
    }
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut num = Ptr::null();
    let hr = (i.create_json_object)(num.out());
    if hr < 0 {
        return hr;
    }
    let hr = obj_set_number(num.0, wz("L").as_ptr(), v as u32 as f64);
    if hr < 0 {
        return hr;
    }
    let hr = obj_set_number(num.0, wz("H").as_ptr(), (v >> 32) as u32 as i32 as f64);
    if hr < 0 {
        return hr;
    }
    obj_set_object(body, name.as_ptr(), num.0)
}

pub unsafe fn get_all_process_ids(ids: &mut Vec<u32>, count: &mut u32) -> HRESULT {
    *count = 0;
    loop {
        let cap = ids.len() + 0x100;
        ids.resize(cap, 0);
        let mut needed = 0u32;
        if K32EnumProcesses(ids.as_mut_ptr(), (cap * 4) as u32, &mut needed) == 0 {
            let e = GetLastError();
            let hr = hr_from_win32(e);
            if e != 0 {
                return hr;
            }
        }
        *count = needed >> 2;
        if (*count as usize) < cap {
            break;
        }
    }
    let mut titles_only: Vec<u32> = ids[..*count as usize].iter().copied().filter(|p| titles::is_title_process(*p)).collect();
    *count = titles_only.len() as u32;
    titles_only.resize(ids.len(), 0);
    *ids = titles_only;
    S_OK
}

struct RegionContext {
    array: Obj,
    count: u32,
}

unsafe fn add_region(ctx: &mut RegionContext, mbi: &XMemBasicInformation) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut va = Ptr::null();
    let mut hr = (i.create_json_object)(va.out());
    if hr < 0 {
        return hr;
    }
    hr = set_u64(va.0, &wz("RBA"), mbi.base);
    if hr < 0 {
        return hr;
    }
    hr = set_u64(va.0, &wz("RSZ"), mbi.region_size);
    if hr < 0 {
        return hr;
    }
    hr = set_u32(va.0, &wz("RST"), mbi.state);
    if hr < 0 {
        return hr;
    }
    hr = set_u32(va.0, &wz("RP"), mbi.protect);
    if hr < 0 {
        return hr;
    }
    hr = set_u32(va.0, &wz("RT"), mbi.kind);
    if hr < 0 {
        return hr;
    }
    hr = set_u64(va.0, &wz("RXF"), mbi.xmem_flags);
    if hr < 0 {
        return hr;
    }
    hr = arr_append_object(ctx.array, va.0);
    if hr >= 0 {
        ctx.count += 1;
    }
    hr
}

unsafe fn visit_process_memory(process: HANDLE, ctx: &mut RegionContext) -> HRESULT {
    let x = match xmemi::get() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let mut hr = S_OK;
    let mut mbi = XMemBasicInformation::default();
    loop {
        let address = mbi.base.wrapping_add(mbi.region_size);
        if (x.virtual_query)(process, address as usize as *const c_void, &mut mbi) == 0 {
            let e = GetLastError();
            if e == ERROR_INVALID_PARAMETER {
                return hr;
            }
            hr = hr_from_win32(e);
            if e != 0 {
                return hr;
            }
        }
        if mbi.base.wrapping_add(mbi.region_size) < address {
            return hr;
        }
        if mbi.state & MEM_COMMIT != 0 {
            hr = add_region(ctx, &mbi);
            if hr < 0 {
                return hr;
            }
        }
    }
}

pub unsafe fn collect_process_memory_info(pid: u32, data: Obj, va_count: &mut u32) -> HRESULT {
    *va_count = 0;
    let h = titles::open_process(0x400, pid);
    if h.is_null() {
        let e = GetLastError();
        let hr = hr_from_win32(e);
        if e != 0 {
            return hr;
        }
    }
    let hr = collect_opened(h, pid, data, va_count);
    if !h.is_null() {
        CloseHandle(h);
    }
    hr
}

unsafe fn collect_opened(h: HANDLE, pid: u32, data: Obj, va_count: &mut u32) -> HRESULT {
    let i = match imports::get() {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut va = Ptr::null();
    let mut hr = (i.create_json_array)(va.out());
    if hr < 0 {
        return hr;
    }
    let mut ctx = RegionContext { array: va.0, count: 0 };
    hr = visit_process_memory(h, &mut ctx);
    if hr < 0 {
        return hr;
    }
    *va_count = ctx.count;
    let mut name = [0u16; 0x105];
    let mut size = 0x104u32;
    if QueryFullProcessImageNameW(h, 0, name.as_mut_ptr(), &mut size) != 0 {
        hr = obj_set_string(data, wz("ProcessName").as_ptr(), name.as_ptr());
        if hr < 0 {
            return hr;
        }
    }
    hr = obj_set_number(data, wz("ProcessId").as_ptr(), pid as f64);
    if hr < 0 {
        return hr;
    }
    obj_set_array(data, wz("RXF").as_ptr(), va.0)
}

pub unsafe extern "system" fn title_memory_report_handler(_session: usize, request: usize, reply: usize) -> HRESULT {
    let mut ret: HRESULT;
    let mut ctx = ErrorContext::new(reply as *mut c_void);
    let mut request_body = Ptr::null();
    let mut reply_body = Ptr::null();
    let mut processes = Ptr::null();
    let mut partition = Ptr::null();
    let mut ids: Vec<u32> = Vec::new();
    let mut hr;
    'fail: {
        hr = deserialize_body(request as *mut c_void, request_body.out());
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0xd7);
            break 'fail;
        }
        let mut num_ids = 0u32;
        let mut pid = 0u32;
        if get_u32(request_body.0, &wz("ProcessId"), &mut pid) >= 0 {
            ids.push(pid);
            num_ids = 1;
        } else {
            hr = get_all_process_ids(&mut ids, &mut num_ids);
            if hr < 0 {
                ctx.add_error(hr, TITLEMEMORY_FILE, 0xe4);
                break 'fail;
            }
        }
        let i = match imports::get() {
            Ok(i) => i,
            Err(e) => {
                hr = e;
                break 'fail;
            }
        };
        hr = (i.create_json_object)(reply_body.out());
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0xe8);
            break 'fail;
        }
        hr = (i.create_json_array)(processes.out());
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0xe9);
            break 'fail;
        }
        hr = (i.create_json_object)(partition.out());
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0xea);
            break 'fail;
        }
        ctx.set_body(reply_body.0);
        let mut total_va = 0u32;
        for &pid in ids.iter().take(num_ids as usize) {
            if pid <= 4 {
                continue;
            }
            let mut data = Ptr::null();
            let r = (i.create_json_object)(data.out());
            if r < 0 {
                ctx.add_error(r, TITLEMEMORY_FILE, 0xfb);
                ctx.hr = r;
                ret = r;
                ctx.complete(&mut ret);
                return ret;
            }
            let mut va_count = 0u32;
            let r = collect_process_memory_info(pid, data.0, &mut va_count);
            if r < 0 {
                ctx.add_error_text(&wz(&format!("Failed ({:x}) to query memory for process {}", r as u32, pid)));
                continue;
            }
            total_va = total_va.wrapping_add(va_count);
            let r = arr_append_object(processes.0, data.0);
            if r < 0 {
                ctx.add_error(r, TITLEMEMORY_FILE, 0x101);
                ctx.hr = r;
                ret = r;
                ctx.complete(&mut ret);
                return ret;
            }
        }
        let x = match xmemi::get() {
            Ok(x) => x,
            Err(e) => {
                hr = e;
                break 'fail;
            }
        };
        let mut stats = [0u8; 0x78];
        let flags = if total_va != 0 { WS_FLAGS_WITH_VA } else { WS_FLAGS_NO_VA };
        if (x.get_working_set_statistics)(flags, stats.as_mut_ptr() as *mut c_void) == 0 {
            let e = GetLastError();
            ret = hr_from_win32(e);
            if e != 0 {
                ctx.complete(&mut ret);
                return ret;
            }
        }
        let q = |o: usize| u64::from_le_bytes(stats[o..o + 8].try_into().unwrap());
        let d = |o: usize| u32::from_le_bytes(stats[o..o + 4].try_into().unwrap());
        let fields: [(&str, u64, bool); 6] = [("GL", q(0), true), ("GU", q(8), true), ("GS", d(0x20) as u64, false), ("TL", q(0x10), true), ("TU", q(0x18), true), ("SR", 0, true)];
        for (k, v, wide) in fields {
            ret = if wide { set_u64(partition.0, &wz(k), v) } else { set_u32(partition.0, &wz(k), v as u32) };
            if ret < 0 {
                ctx.complete(&mut ret);
                return ret;
            }
        }
        hr = obj_set_object(reply_body.0, wz("PartitionReport").as_ptr(), partition.0);
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0x125);
            break 'fail;
        }
        hr = obj_set_array(reply_body.0, wz("ProcessReports").as_ptr(), processes.0);
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0x126);
            break 'fail;
        }
        hr = set_u32(reply_body.0, &wz("TotalVA"), total_va);
        if hr < 0 {
            ctx.add_error(hr, TITLEMEMORY_FILE, 0x127);
            break 'fail;
        }
        ret = hr;
        ctx.complete(&mut ret);
        return ret;
    }
    ret = hr;
    ctx.hr = hr;
    ctx.complete(&mut ret);
    ret
}
