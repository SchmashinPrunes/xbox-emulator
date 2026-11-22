#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn dll_register_server_matches_every_console_mfhevcdec() {
    let rows = [("DllRegisterServer", mfhevcdec::DllRegisterServer as *const () as usize, 0xffff_ffff, 0x8000_4005)];
    let seen = leaf::check_fixed("mfhevcdec.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}
