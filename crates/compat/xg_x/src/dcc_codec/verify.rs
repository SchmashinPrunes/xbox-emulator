use super::run::{self, Scratch};

pub(super) struct VRng(pub(super) u64);

impl VRng {
    pub(super) fn n(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
}

pub(super) struct Case {
    pub(super) enc: bool,
    pub(super) fmt: i32,
    pub(super) sw: i32,
    pub(super) p: [u32; 4],
    pub(super) n: usize,
    pub(super) src: Vec<u8>,
    pub(super) meta: Vec<u8>,
}

fn vblock(r: &mut VRng) -> [u8; 256] {
    let mut b = [0u8; 256];
    match r.n() % 6 {
        0 => b.iter_mut().for_each(|x| *x = r.n() as u8),
        1 => b.fill(r.n() as u8),
        2 => {
            let per = [1usize, 2, 4, 8, 16][(r.n() % 5) as usize];
            let e: Vec<u8> = (0..per).map(|_| r.n() as u8).collect();
            for i in 0..256 {
                b[i] = e[i % per];
            }
        }
        3 => {
            let s = (r.n() % 5) as u8;
            let base = r.n() as u8;
            for i in 0..256 {
                b[i] = base.wrapping_add(s.wrapping_mul(i as u8));
            }
        }
        4 => {
            let base = r.n() as u8;
            b.iter_mut().for_each(|x| *x = base.wrapping_add((r.n() % 3) as u8));
        }
        _ => {
            for _ in 0..(r.n() % 12) {
                b[(r.n() % 256) as usize] = r.n() as u8;
            }
        }
    }
    b
}

pub(super) fn case(i: u64) -> Case {
    let mut r = VRng(0x9e37_79b9_7f4a_7c15 ^ (i.wrapping_mul(0x2545_F491_4F6C_DD1D)) | 1);
    for _ in 0..4 {
        r.n();
    }
    let enc = r.n() % 2 == 0;
    let fmt = 1 + r.n() % 0x78;
    let sw = r.n() % 0x1b;
    let _dim = r.n() % 5;
    let mut p = [r.n() % 3, [0u32, 0, 1, 2, 3][(r.n() % 5) as usize], 0, 0];
    if r.n() % 8 == 0 {
        p[2] = r.n();
        p[3] = r.n();
    }
    if r.n() % 16 == 0 {
        p[0] = r.n() % 40;
    }
    let n = 1 + (r.n() % 3) as usize;
    let mut src = Vec::new();
    let mut meta = Vec::new();
    for _ in 0..n {
        src.extend_from_slice(&vblock(&mut r));
        meta.push([0x00u8, 0x10, 0x20, 0x30, 0x50, 0x11, 0x21, 0x33, 0x44, 0x55, 0x01, 0x02, 0x03, 0x04, 0x05, 0x13, 0x23][(r.n() % 17) as usize] ^ if r.n() % 4 == 0 { r.n() as u8 } else { 0 });
    }
    Case { enc, fmt: fmt as i32, sw: sw as i32, p, n, src, meta }
}

pub(super) fn reason(c: &Case, sc: &Scratch) -> Option<String> {
    let mut runs = run::setup_runs(c.fmt, c.sw, [c.p[0], c.p[1]], sc).ok()?;
    let out = if c.enc {
        let (mut d, mut m) = (vec![0u8; c.n * 256], vec![0u8; c.n]);
        run::encode_blocks(&mut runs, &c.src, &mut d, &mut m, None)
    } else {
        let (mut d, mut notes) = (vec![0u8; c.n * 256], Vec::new());
        run::decode_blocks(&mut runs, &c.src, &c.meta, &mut d, &mut notes)
    };
    match out {
        run::Outcome::Done => None,
        run::Outcome::Stopped { block, why } => Some(format!("block {block}: {why}")),
    }
}

#[test]
fn dccx_stable_reasons() {
    let Ok(ids) = std::env::var("XGX_DCCX_IDS") else { return };
    let mut by: std::collections::BTreeMap<String, Vec<u64>> = Default::default();
    for id in ids.split_whitespace().map(|x| x.parse::<u64>().unwrap()) {
        let c = case(id);
        let w = reason(&c, &Scratch { b: [0; 64], unknown: 0 }).unwrap_or_else(|| "native accepts from a zero scratch".into());
        by.entry(format!("{} {}", if c.enc { "E" } else { "D" }, w)).or_default().push(id);
    }
    for (w, v) in &by {
        println!("{} {w} {:?}", v.len(), &v[..v.len().min(10)]);
    }
}
