//! T20-C6 / TW-C1 guard: the checked-in substrates must not grow a second
//! repair implementation beside the shared native-window-host engine.

#[test]
fn compositor_repair_has_one_shipping_definition() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut definitions = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|x| x.to_str()) != Some("rs") { continue; }
        let text = std::fs::read_to_string(path).unwrap();
        definitions += text.matches("fn borrowed_tether_repair_plan(").count();
    }
    assert_eq!(definitions, 1, "a substrate forked compositor repair instead of sharing BORROW-CORE");
}
