#[path = "../../console-dll/oracle/leaf.rs"]
mod leaf;

#[test]
fn elf_flush_event_log_matches_every_console_advapi32legacy_that_exports_it() {
    let rows = [("ElfFlushEventLog", advapi32legacy::ElfFlushEventLog as *const () as usize, 0xffff_ffffu64, 0xc000_0022u64)];
    let images = leaf::images("advapi32legacy.dll");
    let with: Vec<&leaf::Image> = images.iter().filter(|i| leaf::try_export(i, "ElfFlushEventLog").is_some()).collect();
    assert!(with.len() >= 3, "{}", with.len());
    for img in with {
        let theirs: leaf::Any6 = unsafe { core::mem::transmute(leaf::export(img, "ElfFlushEventLog")) };
        let mine: leaf::Any6 = unsafe { core::mem::transmute(rows[0].1) };
        for a in leaf::ARGS {
            let (m, mle) = leaf::with_sentinel(|| unsafe { mine(a[0], a[1], a[2], a[3], a[4], a[5]) } & 0xffff_ffff);
            let (t, tle) = leaf::with_sentinel(|| unsafe { theirs(a[0], a[1], a[2], a[3], a[4], a[5]) } & 0xffff_ffff);
            assert_eq!((m, mle), (rows[0].3, leaf::SENTINEL), "emulator");
            assert_eq!((t, tle), (m, mle), "{}", img.source.display());
        }
    }
}
