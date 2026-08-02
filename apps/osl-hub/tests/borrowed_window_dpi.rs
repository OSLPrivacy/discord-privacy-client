//! T20-D2 / TW-7 guard for the physical-pixel seam shared by borrowed hosts.

#[test]
fn every_borrow_substrate_keeps_physical_pixel_bounds_at_the_shared_seam() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let host = std::fs::read_to_string(root.join("native_window_host.rs")).unwrap();
    assert!(host.contains("physical"), "BORROW-CORE must document physical-pixel bounds");
    assert!(host.contains("scale_factor"), "mixed-DPI conversion seam disappeared");
    for substrate in ["browser_companion.rs", "mullvad_window_host.rs"] {
        let text = std::fs::read_to_string(root.join(substrate)).unwrap();
        assert!(!text.contains("logical borrowed bounds"), "{substrate} reintroduced logical bounds");
    }
}
