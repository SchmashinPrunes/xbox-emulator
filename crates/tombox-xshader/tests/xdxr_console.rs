use std::path::{Path, PathBuf};
use tombox_xshader::{extract, hlsl::translate};

const GAMES: &str = r"D:\xbox-pc-port\games";

fn dumps(name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(PathBuf::from(GAMES), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 6 {
                stack.push((p, depth + 1));
            } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) && p.parent().and_then(Path::file_name).is_some_and(|n| n.eq_ignore_ascii_case("System32")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn containers(img: &[u8]) -> Vec<&[u8]> {
    (0..img.len().saturating_sub(32))
        .filter(|&o| &img[o..o + 4] == b"DXBC")
        .filter_map(|o| {
            let n = u32::from_le_bytes(img[o + 24..o + 28].try_into().unwrap()) as usize;
            (n >= 32 && o + n <= img.len()).then(|| &img[o..o + n])
        })
        .collect()
}

#[test]
fn console_xdxr_compute_shaders_with_an_sgpr_soffset_translate() {
    let images = dumps("xdxr.dll");
    if images.is_empty() {
        eprintln!("no console xdxr.dll in {GAMES}; skipped");
        return;
    }
    let (mut with_soffset, mut translated) = (0, 0);
    for path in images {
        let bytes = std::fs::read(&path).unwrap();
        for c in containers(&bytes) {
            let Ok(ex) = extract(c) else { continue };
            let sgpr_soffset = tombox_xshader::rdna2::disassemble(&ex.isa).iter().any(|i| i.mnemonic.starts_with("buffer_") && !i.mnemonic.contains("format") && !i.mnemonic.contains("atomic") && matches!(i.src.get(2), Some(tombox_xshader::rdna2::Opnd::Sgpr(_))));
            if !sgpr_soffset {
                continue;
            }
            with_soffset += 1;
            match translate(&ex) {
                Ok(t) => {
                    assert!(t.hlsl.contains("[numthreads("), "{}", path.display());
                    translated += 1;
                }
                Err(e) => assert!(!e.contains("SOFFSET"), "{}: {e}", path.display()),
            }
        }
    }
    assert!(with_soffset > 0, "no console xdxr compute shader uses an SGPR SOFFSET");
    assert!(translated > 0, "none of the {with_soffset} SGPR-SOFFSET shaders translated");
}
