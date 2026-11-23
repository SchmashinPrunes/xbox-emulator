use tombox_xshader::hlsl::translate;
use tombox_xshader::meta::{RootParam, RootParamKind, RootRange, RootSig, SigElement};
use tombox_xshader::{translate_with_fssm_root, ShaderExtract, Stage};

const HEAD: [u32; 4] = [0xf408_0104, 0xfa00_0000, 0xf40c_0304, 0xfa00_0020];
const BUF_LOAD_V4: [u32; 2] = [0xe030_1000, 0x8001_0401];
const IMG_LOAD_V8: [u32; 2] = [0xf000_0f08, 0x0003_0801];
const TAIL: [u32; 4] = [0x7e00_0304, 0xf800_180f, 0x0302_0100, 0xbf81_0000];

fn fssm(user_sgprs: u8, l1: &[[u32; 5]], l2: &[[u32; 5]]) -> Vec<u8> {
    let mut f = vec![0u8; 0x100];
    f[0..4].copy_from_slice(b"FSSM");
    f[0xD6] = user_sgprs;
    f[0x40..0x48].copy_from_slice(&0xB0u64.to_le_bytes());
    let p1 = f.len() as u64 - 0x10;
    f[0xE8..0xF0].copy_from_slice(&p1.to_le_bytes());
    f[0xF8..0xFC].copy_from_slice(&(l1.len() as u32).to_le_bytes());
    for w in l1.iter().flatten() {
        f.extend_from_slice(&w.to_le_bytes());
    }
    let p2 = f.len() as u64 - 0x10;
    f[0xF0..0xF8].copy_from_slice(&p2.to_le_bytes());
    f[0xFC..0x100].copy_from_slice(&(l2.len() as u32).to_le_bytes());
    for w in l2.iter().flatten() {
        f.extend_from_slice(&w.to_le_bytes());
    }
    f
}

fn table_with_buffer_and_texture() -> Vec<u8> {
    fssm(9, &[[3, 0, 1, 0, 8]], &[[0x300, 0, 0, 4, 0x0804], [0x300, 8, 1, 4, 0x0808]])
}

fn pixel_shader(words: &[u32], fssm: Vec<u8>) -> ShaderExtract {
    let isa: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    ShaderExtract {
        stage: Stage::Pixel,
        shader_model: (6, 0),
        bitcode_stripped: true,
        fssm,
        entry: Some("main".into()),
        isa_data: isa.clone(),
        isa,
        isa_offset_in_fssm: 0,
        isa_preamble: [0; 4],
        psv_numthreads: None,
        has_input_sig: true,
        has_output_sig: true,
        has_psv0: false,
        has_root_sig: false,
        input_sig: vec![],
        output_sig: vec![SigElement { name: "SV_Target".into(), index: 0, sysval: 64, comp_type: 3, register: 0, mask: 0xf, rw_mask: 0 }],
        prim_sig: Vec::new(),
        resources: Vec::new(),
        gs_info: None,
        root_sig: None,
    }
}

fn program() -> Vec<u32> {
    [&HEAD[..], &BUF_LOAD_V4, &IMG_LOAD_V8, &TAIL].concat()
}

fn engine_srv_table() -> RootSig {
    RootSig {
        version: 2,
        flags: 0,
        params: vec![RootParam { visibility: 5, kind: RootParamKind::Table(vec![RootRange { range_type: 0, num: 64, base_register: 0, space: 0, offset_in_table: u32::MAX }]) }],
        num_static_samplers: 0,
        static_samplers: Vec::new(),
    }
}

fn compile(label: &str, hlsl: &str, profile: &str) {
    #[cfg(windows)]
    {
        assert!(tombox_xshader::dxc::available(), "{label}: DXC (dxcompiler.dll + dxil.dll) not found");
        let profile = tombox_xshader::dxc::profile_for(profile);
        let code = tombox_xshader::dxc::compile(hlsl, &profile).unwrap_or_else(|e| panic!("{label}: dxc: {e}\n{hlsl}"));
        assert!(code.len() > 4 && &code[..4] == b"DXBC", "{label}: DXC returned no container");
    }
    #[cfg(not(windows))]
    let _ = (label, hlsl, profile);
}

#[test]
fn table_binds_a_shader_without_root_signature() {
    let ex = pixel_shader(&program(), table_with_buffer_and_texture());
    let Err(e) = translate(&ex) else { panic!("a table load without any root signature must be refused") };
    assert!(e.contains("missing root signature"), "{e}");
    let t = translate_with_fssm_root(&ex).unwrap_or_else(|e| panic!("FSSM table translation failed: {e}"));
    assert!(t.hlsl.contains("register(t0)"), "{}", t.hlsl);
    assert!(t.hlsl.contains("Texture2D<float4> tex1 : register(t1);"), "{}", t.hlsl);
    compile("FSSM table", &t.hlsl, t.profile);
}

#[test]
fn table_and_equivalent_root_signature_give_the_same_hlsl() {
    let bare = pixel_shader(&program(), table_with_buffer_and_texture());
    let mut with_rs = bare.clone();
    with_rs.has_root_sig = true;
    with_rs.root_sig = Some(engine_srv_table());
    let from_rs = translate(&with_rs).unwrap_or_else(|e| panic!("root signature translation failed: {e}"));
    let from_table = translate_with_fssm_root(&bare).unwrap_or_else(|e| panic!("FSSM table translation failed: {e}"));
    assert_eq!(from_rs.hlsl, from_table.hlsl);
    let b = tombox_xshader::fssm_bind::parse(&bare.fssm).unwrap().unwrap();
    assert!(b.disagreements(&engine_srv_table()).is_empty());
}

#[test]
fn shaders_with_a_root_signature_ignore_the_table() {
    let mut ex = pixel_shader(&program(), fssm(9, &[[3, 0, 1, 0, 8]], &[[0x300, 0, 5, 4, 0x0804], [0x300, 8, 6, 4, 0x0808]]));
    ex.has_root_sig = true;
    ex.root_sig = Some(engine_srv_table());
    let a = translate(&ex).unwrap_or_else(|e| panic!("{e}"));
    let b = translate_with_fssm_root(&ex).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(a.hlsl, b.hlsl);
    assert!(!a.hlsl.contains("register(t5)"), "{}", a.hlsl);
}

#[test]
fn unreadable_or_missing_tables_are_refused() {
    let mut f = table_with_buffer_and_texture();
    f[0xE8..0xF0].copy_from_slice(&0xDEAD_BEEF_DEAD_BEEFu64.to_le_bytes());
    let Err(e) = translate_with_fssm_root(&pixel_shader(&program(), f)) else { panic!("no table must be refused") };
    assert!(e.contains("no FSSM binding table"), "{e}");
    let Err(e) = translate_with_fssm_root(&pixel_shader(&program(), fssm(10, &[[3, 0, 1, 0, 8]], &[]))) else { panic!("a bad table must be refused") };
    assert!(e.contains("user SGPR count"), "{e}");
    let mut f = table_with_buffer_and_texture();
    f[0x40..0x48].copy_from_slice(&0u64.to_le_bytes());
    f[0x38..0x40].copy_from_slice(&0xB0u64.to_le_bytes());
    let Err(e) = translate_with_fssm_root(&pixel_shader(&program(), f)) else { panic!("a vertex-stage table on a pixel shader must be refused") };
    assert!(e.contains("hardware stage slot"), "{e}");
}

#[test]
fn table_types_the_descriptor_as_written_or_read() {
    let store = [0xe070_1000u32, 0x8001_0801];
    let words: Vec<u32> = [&HEAD[..], &store, &IMG_LOAD_V8, &TAIL].concat();
    let ex = pixel_shader(&words, fssm(9, &[[3, 0, 1, 0, 8]], &[[0x303, 0, 2, 4, 0x0804], [0x300, 8, 1, 4, 0x0808]]));
    let t = translate_with_fssm_root(&ex).unwrap_or_else(|e| panic!("FSSM table translation failed: {e}"));
    assert!(t.hlsl.contains("register(u2)"), "{}", t.hlsl);
    assert!(t.hlsl.contains("register(t1)"), "{}", t.hlsl);
    compile("FSSM UAV + SRV", &t.hlsl, t.profile);
}
