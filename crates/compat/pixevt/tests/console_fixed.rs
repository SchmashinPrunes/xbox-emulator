#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn pix_begin_capture1_matches_every_console_pixevt() {
    let rows = [("PIXBeginCapture1", pixevt::PIXBeginCapture1 as *const () as usize, 0xffff_ffff, 0x8000_4001)];
    let seen = leaf::check_fixed("pixevt.dll", &rows);
    assert!(seen.len() >= 4, "{seen:?}");
}
