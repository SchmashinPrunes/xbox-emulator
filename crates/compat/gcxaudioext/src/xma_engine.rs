use core::ffi::c_void;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::acp_state::{self, heap_alloc, heap_free, ShapeAllocator, STATE_ALLOCATOR, STATE_DEBUG};

pub const IID_IUNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46];
pub const IID_XMA_BUFFER_ENGINE: [u8; 16] = [0x6e, 0x81, 0xc8, 0xb2, 0xd2, 0x04, 0xde, 0x4a, 0x9b, 0x99, 0x4b, 0x43, 0x74, 0xe2, 0xa7, 0xaa];
pub const S_OK: i32 = 0;
pub const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
pub const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
pub const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
pub const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;
pub const XAUDIO2_E_INVALID_CALL: i32 = 0x8896_0001u32 as i32;
pub const QUEUE_EMPTY: i32 = 0x8031_0002u32 as i32;
pub const XAUDIO2_END_OF_STREAM: u32 = 0x40;
pub const XAUDIO2_LOOP_INFINITE: u32 = 0xff;
pub const MAX_QUEUED_BUFFERS: usize = 64;
pub const XMA_BYTES_PER_PACKET: u32 = 0x800;
pub const XMA_READBUFFER_MAX_BYTES: u32 = 0x7f_f800;
pub const XMA_SUBFRAME_SAMPLES: u32 = 128;
pub const VTABLE_SLOTS: usize = 20;
const OUTPUT_BLOCK_BYTES: usize = 0x100;
const OUTPUT_MAX_BYTES: u32 = 0x1f00;
const DEFAULT_SUBFRAMES: u32 = 4;

#[repr(C, packed)]
#[derive(Clone, Copy, Default, Debug)]
pub struct XmaBuffer {
    pub flags: u32,
    pub audio_bytes: u32,
    pub audio_data: u64,
    pub play_begin: u32,
    pub play_length: u32,
    pub loop_begin: u32,
    pub loop_length: u32,
    pub loop_count: u32,
    pub context: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ProcessOutput {
    pub data: *mut u8,
    pub context: u64,
    pub bytes: u32,
    pub reserved: u32,
    pub bytes_valid: u32,
    pub end_of_stream: u8,
    pub buffer_end: u8,
    pub pad: [u8; 2],
}

#[derive(Clone, Copy, Debug)]
struct Done {
    remaining: u32,
    flags: u32,
    context: u64,
}

pub struct StreamConfig {
    pub sample_rate: u32,
    pub out_frames: u32,
    pub channels: u32,
    pub subframes: u32,
}

struct Decode {
    packets: Vec<usize>,
    next: usize,
    decoder: Option<xma::Xma2Stream>,
    pcm: VecDeque<i16>,
    pos: u64,
    cursor: u64,
    end: u64,
    loop_begin: u64,
    loop_end: u64,
    loops_left: u32,
    loops_done: u32,
    exhausted: bool,
}

struct Stream {
    context: u32,
    channels: usize,
    rate: u32,
    capacity: usize,
    ring: VecDeque<i16>,
    decode: Option<Decode>,
}

struct Inner {
    channels: u32,
    stream_count: u32,
    out: usize,
    out_frames: u32,
    warn: u32,
    queue_ready: bool,
    queue: VecDeque<XmaBuffer>,
    active: VecDeque<XmaBuffer>,
    completed: VecDeque<Done>,
    streams: Option<Vec<Stream>>,
    loop_count: u32,
    running: bool,
    commit: bool,
    staging: Vec<i16>,
}

#[repr(C)]
pub struct XmaBufferEngine {
    vtbl: *const usize,
    refs: AtomicU32,
    state: *mut u8,
    inner: Mutex<Inner>,
}

static MODULE_REFS: AtomicU32 = AtomicU32::new(0);

pub fn module_refs() -> u32 {
    MODULE_REFS.load(Ordering::SeqCst)
}

#[link(name = "kernel32")]
extern "system" {
    fn OutputDebugStringA(s: *const u8);
}

fn debug_print(warn: u32, msg: &str) {
    if warn == 0 {
        return;
    }
    let mut s = format!("XAudio2: {msg}").into_bytes();
    s.push(0);
    unsafe { OutputDebugStringA(s.as_ptr()) };
}

fn round_up(x: u32, y: u32) -> u32 {
    let v = x.wrapping_add(y).wrapping_sub(1);
    v - v % y
}

pub fn stream_configs(sample_rate: u32, channels: u32, out_frames: u32) -> Vec<StreamConfig> {
    let size_channels = if channels > 1 { 2 } else { 1 };
    let bytes_per_frame = size_channels * 2;
    let max_frames = OUTPUT_MAX_BYTES / bytes_per_frame;
    let block_frames = OUTPUT_BLOCK_BYTES as u32 / bytes_per_frame;
    let mut base = round_up(out_frames, block_frames);
    let mut subframes = DEFAULT_SUBFRAMES;
    let mut sub_frames = subframes * XMA_SUBFRAME_SAMPLES;
    while subframes != 0 && base + sub_frames * 2 > max_frames {
        subframes /= 2;
        sub_frames /= 2;
    }
    base += sub_frames * 2;
    let count = (channels + 1) / 2;
    (0..count)
        .map(|i| StreamConfig { sample_rate, out_frames: base, channels: if i == count - 1 && channels % 2 == 1 { 1 } else { 2 }, subframes })
        .collect()
}

fn ring_capacity(c: &StreamConfig) -> usize {
    let bytes = c.out_frames as usize * c.channels as usize * 2;
    let blocks = (bytes / OUTPUT_BLOCK_BYTES) & 0x1f;
    let blocks = if blocks == 0 { 32 } else { blocks };
    (blocks - 1) * OUTPUT_BLOCK_BYTES / (c.channels as usize * 2)
}

unsafe fn allocator_of(state: *mut u8) -> *mut ShapeAllocator {
    *(state.add(STATE_ALLOCATOR) as *const *mut ShapeAllocator)
}

unsafe fn create_group(state: *mut u8, rate: u32, channels: u32, out_frames: u32) -> Result<Vec<Stream>, i32> {
    let a = allocator_of(state);
    if a.is_null() {
        return Err(E_UNEXPECTED);
    }
    let mut v: Vec<Stream> = Vec::new();
    for c in stream_configs(rate, channels, out_frames) {
        match acp_state::alloc_xma_context(a) {
            Ok(i) => v.push(Stream { context: i, channels: c.channels as usize, rate, capacity: ring_capacity(&c), ring: VecDeque::new(), decode: None }),
            Err(hr) => {
                free_group(v);
                return Err(hr);
            }
        }
    }
    Ok(v)
}

fn free_group(v: Vec<Stream>) {
    for s in v {
        acp_state::free_xma_context(s.context);
    }
}

unsafe fn packet_offsets(data: usize, bytes: u32, stream: u32) -> Vec<usize> {
    let mut v = Vec::new();
    let count = (bytes / XMA_BYTES_PER_PACKET) as usize;
    let mut idx = stream as usize;
    while idx < count {
        let p = data + idx * XMA_BYTES_PER_PACKET as usize;
        v.push(p);
        let skip = *(p as *const u8).add(3) as usize;
        if skip == 0xff {
            break;
        }
        idx += skip + 1;
    }
    v
}

impl Decode {
    unsafe fn new(b: &XmaBuffer, stream: u32) -> Decode {
        let packets = packet_offsets(b.audio_data as usize, b.audio_bytes, stream);
        let begin = b.play_begin as u64;
        let end = if b.play_length != 0 { begin + b.play_length as u64 } else { u64::MAX };
        let (loop_begin, loop_end, loops_left) = if b.loop_count != 0 {
            let lb = b.loop_begin as u64;
            let le = if b.loop_length != 0 { lb + b.loop_length as u64 } else { end };
            (lb, le, b.loop_count)
        } else {
            (0, u64::MAX, 0)
        };
        Decode { packets, next: 0, decoder: None, pcm: VecDeque::new(), pos: 0, cursor: begin, end, loop_begin, loop_end, loops_left, loops_done: 0, exhausted: false }
    }

    fn restart(&mut self, at: u64) {
        self.next = 0;
        self.decoder = None;
        self.pcm.clear();
        self.pos = 0;
        self.cursor = at;
        self.exhausted = false;
    }

    fn looping(&self) -> bool {
        self.loops_left != 0
    }

    fn take_loop(&mut self) -> bool {
        if !self.looping() {
            return false;
        }
        if self.loops_left != XAUDIO2_LOOP_INFINITE {
            self.loops_left -= 1;
        }
        self.loops_done = self.loops_done.wrapping_add(1);
        let lb = self.loop_begin;
        if lb >= self.pos && lb <= self.pos + self.pcm.len() as u64 {
            self.cursor = lb;
        } else {
            self.restart(lb);
        }
        true
    }

    unsafe fn decode_more(&mut self, channels: usize, rate: u32) -> bool {
        if self.next >= self.packets.len() {
            return false;
        }
        if self.decoder.is_none() {
            match xma::Xma2Stream::new_hardware(rate, channels) {
                Ok(d) => self.decoder = Some(d),
                Err(_) => {
                    self.next = self.packets.len();
                    return false;
                }
            }
        }
        let p = core::slice::from_raw_parts(self.packets[self.next] as *const u8, XMA_BYTES_PER_PACKET as usize);
        self.next += 1;
        let mut out = Vec::new();
        self.decoder.as_mut().unwrap().decode_packet(p, &mut out);
        self.pcm.extend(out);
        true
    }

    unsafe fn fill(&mut self, ring: &mut VecDeque<i16>, capacity: usize, channels: usize, rate: u32) {
        let mut guard = 0u32;
        while ring.len() / channels < capacity && !self.exhausted {
            guard += 1;
            if guard > 0x10_0000 {
                self.exhausted = true;
                break;
            }
            let limit = if self.looping() { self.loop_end.min(self.end) } else { self.end };
            if self.cursor >= limit {
                if self.cursor >= self.loop_end.min(self.end) && self.take_loop() {
                    continue;
                }
                self.exhausted = true;
                break;
            }
            let have = self.pos + (self.pcm.len() / channels) as u64;
            if self.cursor >= have {
                if !self.decode_more(channels, rate) {
                    if self.looping() && self.loop_end == u64::MAX || self.looping() && self.loop_end >= have {
                        if self.take_loop() {
                            continue;
                        }
                    }
                    self.exhausted = true;
                    break;
                }
                continue;
            }
            let drop = ((self.cursor - self.pos) as usize) * channels;
            if drop > 0 && !self.looping() {
                self.pcm.drain(..drop.min(self.pcm.len()));
                self.pos = self.cursor;
            }
            let from = ((self.cursor - self.pos) as usize) * channels;
            let avail = (have - self.cursor) as usize;
            let room = capacity - ring.len() / channels;
            let n = avail.min(room).min((limit - self.cursor) as usize);
            ring.extend(self.pcm.range(from..from + n * channels));
            self.cursor += n as u64;
            if !self.looping() && self.loop_begin == 0 {
                let used = ((self.cursor - self.pos) as usize) * channels;
                self.pcm.drain(..used.min(self.pcm.len()));
                self.pos = self.cursor;
            }
        }
    }
}

impl Stream {
    fn busy(&self) -> bool {
        matches!(&self.decode, Some(d) if !d.exhausted)
    }

    fn frames(&self) -> u32 {
        (self.ring.len() / self.channels) as u32
    }

    unsafe fn service(&mut self) {
        let (ch, rate, cap) = (self.channels, self.rate, self.capacity);
        if let Some(d) = self.decode.as_mut() {
            d.fill(&mut self.ring, cap, ch, rate);
        }
    }

    fn reset(&mut self) {
        self.ring.clear();
        self.decode = None;
    }

    fn loops_left(&self) -> u32 {
        self.decode.as_ref().map(|d| d.loops_left).unwrap_or(0)
    }

    fn loops_done(&self) -> u32 {
        self.decode.as_ref().map(|d| d.loops_done).unwrap_or(0)
    }
}

unsafe fn callback(cb: *mut c_void, slot: usize, context: u64, value: u32) {
    if cb.is_null() {
        return;
    }
    let f: unsafe extern "system" fn(*mut c_void, u64, u32) = core::mem::transmute(*(*(cb as *const *const usize)).add(slot));
    f(cb, context, value);
}

impl Inner {
    fn streams(&mut self) -> &mut [Stream] {
        self.streams.as_deref_mut().unwrap_or(&mut [])
    }

    unsafe fn service(&mut self) {
        for s in self.streams() {
            s.service();
        }
    }

    fn any_busy(&self) -> bool {
        self.streams.as_deref().unwrap_or(&[]).iter().any(|s| s.busy())
    }

    fn available(&self) -> u32 {
        self.streams.as_deref().unwrap_or(&[]).iter().map(|s| s.frames()).min().unwrap_or(0)
    }

    unsafe fn start_next(&mut self, cb: *mut c_void) {
        let Some(head) = self.active.front().copied() else { return };
        for (i, s) in self.streams().iter_mut().enumerate() {
            s.ring.clear();
            s.decode = Some(Decode::new(&head, i as u32));
        }
        self.loop_count = if head.loop_count == XAUDIO2_LOOP_INFINITE { 0xfe } else { head.loop_count & 0xff };
        self.service();
        self.commit = true;
        callback(cb, 0, head.context, head.flags);
        self.running = true;
    }

    unsafe fn loop_callbacks(&mut self, cb: *mut c_void) {
        let Some(head) = self.active.front().copied() else { return };
        let done = self.streams.as_deref().unwrap_or(&[]).iter().map(|s| s.loops_done()).min().unwrap_or(0);
        let reported = self.streams.as_deref().unwrap_or(&[]).first().map(|_| self.loops_reported()).unwrap_or(0);
        for _ in reported..done {
            callback(cb, 2, head.context, head.flags);
        }
        self.set_loops_reported(done);
        if head.loop_count != XAUDIO2_LOOP_INFINITE {
            self.loop_count = self.streams.as_deref().unwrap_or(&[]).iter().map(|s| s.loops_left()).max().unwrap_or(0);
        }
    }

    fn loops_reported(&self) -> u32 {
        self.reported
    }

    fn set_loops_reported(&mut self, v: u32) {
        self.reported = v;
    }

    fn pop_active(&mut self) {
        let Some(b) = self.active.pop_front() else { return };
        let remaining = self.streams.as_deref().unwrap_or(&[]).first().map(|s| s.frames()).unwrap_or(0);
        self.completed.push_back(Done { remaining, flags: b.flags, context: b.context });
        self.reported = 0;
    }
}
