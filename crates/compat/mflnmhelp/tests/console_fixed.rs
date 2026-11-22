#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn fixed_result_exports_match_every_console_mflnmhelp() {
    let rows = [
        ("RegDeleteTreeW", mflnmhelp::RegDeleteTreeW as *const () as usize, 0xffff_ffff, 0x78),
        ("RegNotifyChangeKeyValue", mflnmhelp::RegNotifyChangeKeyValue as *const () as usize, 0xffff_ffff, 0x78),
        ("VariantChangeType", mflnmhelp::VariantChangeType as *const () as usize, 0xffff_ffff, 0x8000_4001),
    ];
    let seen = leaf::check_fixed("mflnmhelp.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}
