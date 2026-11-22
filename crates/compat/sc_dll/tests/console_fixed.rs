#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn retail_assembler_and_metadata_entry_points_match_every_console_sc_dll() {
    let rows = [
        ("SC_Assemble", sc_dll::SC_Assemble as *const () as usize, u64::MAX, 0),
        ("SC_AssembleHandle_GetShader", sc_dll::SC_AssembleHandle_GetShader as *const () as usize, 0xff, 0),
        ("SC_CompileShaderFromMetadata", sc_dll::SC_CompileShaderFromMetadata as *const () as usize, 0xffff_ffff, 4),
    ];
    let seen = leaf::check_fixed("sc_dll.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}

#[test]
fn sc_get_export_functions_publishes_the_same_entry_points() {
    let mut table = [0usize; 0x188 / 8];
    assert_eq!(unsafe { sc_dll::SCGetExportFunctions(table.as_mut_ptr() as *mut u8) }, 0);
    assert_eq!(table[0xf0 / 8], sc_dll::SC_Assemble as *const () as usize);
    assert_eq!(table[0xf8 / 8], sc_dll::SC_AssembleHandle_GetShader as *const () as usize);
    assert_eq!(table[0x150 / 8], sc_dll::SC_CompileShaderFromMetadata as *const () as usize);
}
