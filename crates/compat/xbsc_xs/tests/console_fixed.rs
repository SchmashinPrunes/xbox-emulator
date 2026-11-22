#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn deserialize_pipeline_compile_input_matches_every_console_xbsc_xs() {
    let rows = [("XBSC_DeserializePipelineCompileInput", xbsc_xs::XBSC_DeserializePipelineCompileInput as *const () as usize, 0xffff_ffff, 0xffff_ffff)];
    let seen = leaf::check_fixed("xbsc_xs.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}
