//! TASK 4852: the role screen's 40 rows and 40 tags are TASK 4851's, not a retyped copy.
//!
//! `apps/osl-hub-ui/src/fixtures/permission-catalogue.txt` is what the screen
//! reads at render time, and it is the exact output of TASK 4851's
//! `osl-permission-catalogue print`. If the Rust catalogue ever changes without
//! that file changing with it, the screen would keep showing yesterday's tags,
//! so this test fails instead.
//!
//! It also holds the three enforcement sentences to their exact wording, checked
//! against the screen module that draws them.

use std::fs;
use std::path::{Path, PathBuf};

use ipc::permission_catalogue::{
    check_permission_catalogue_text, render_permission_catalogue, EnforcementTag,
    PERMISSION_CATALOGUE, SECTION_NAMES,
};

const KEY_SENTENCE: &str = "Not a rule. They do not have the key.";
const RELAY_SENTENCE: &str = "OSL's relay refuses it. It still cannot read what you write.";
const TRUST_SENTENCE: &str =
    "A modified app could ignore this. Everyone else's app will still hide it.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/ipc has a workspace root")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn role_screen_shows_task_4851_rows_tags_and_the_three_sentences() {
    let fixture = read("apps/osl-hub-ui/src/fixtures/permission-catalogue.txt");
    let rendered = render_permission_catalogue();
    assert_eq!(
        fixture, rendered,
        "the role screen's catalogue file is not TASK 4851's catalogue; \
         regenerate it with `cargo run -p ipc --bin osl-permission-catalogue -- print`",
    );

    // The screen reads this file, so it has to pass TASK 4851's own check.
    let report = check_permission_catalogue_text(&fixture).expect("catalogue file passes 4851");
    assert_eq!(report.section_names, SECTION_NAMES.len());
    assert_eq!(report.permission_rows, PERMISSION_CATALOGUE.len());
    assert_eq!(report.enforcement_tags, PERMISSION_CATALOGUE.len());

    let screen = read("apps/osl-hub-ui/src/role-permission-rows.ts");
    for (tag, sentence) in [
        (EnforcementTag::Key, KEY_SENTENCE),
        (EnforcementTag::Relay, RELAY_SENTENCE),
        (EnforcementTag::Trust, TRUST_SENTENCE),
    ] {
        assert!(
            screen.contains(&format!("{}: \"{sentence}\"", tag.as_str())),
            "the role screen does not state the {} sentence: {sentence}",
            tag.as_str(),
        );
    }

    let key_rows = PERMISSION_CATALOGUE
        .iter()
        .filter(|row| row.tag == EnforcementTag::Key)
        .count();
    let relay_rows = PERMISSION_CATALOGUE
        .iter()
        .filter(|row| row.tag == EnforcementTag::Relay)
        .count();
    let trust_rows = PERMISSION_CATALOGUE
        .iter()
        .filter(|row| row.tag == EnforcementTag::Trust)
        .count();

    println!("TASK4852 catalogue_file_matches_4851=true");
    println!("TASK4852 section_names={}", report.section_names);
    println!("TASK4852 permission_rows={}", report.permission_rows);
    println!("TASK4852 enforcement_tags={}", report.enforcement_tags);
    println!("TASK4852 key_rows={key_rows}");
    println!("TASK4852 relay_rows={relay_rows}");
    println!("TASK4852 trust_rows={trust_rows}");
    println!("TASK4852 key_sentence={KEY_SENTENCE}");
    println!("TASK4852 relay_sentence={RELAY_SENTENCE}");
    println!("TASK4852 trust_sentence={TRUST_SENTENCE}");
    assert_eq!(key_rows + relay_rows + trust_rows, PERMISSION_CATALOGUE.len());
}
