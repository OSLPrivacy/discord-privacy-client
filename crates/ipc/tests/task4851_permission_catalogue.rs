use std::io::Write;
use std::process::{Command, Stdio};

use ipc::permission_catalogue::{
    check_permission_catalogue_text, render_permission_catalogue, REQUIRED_PERMISSION_ROWS,
    SECTION_NAMES,
};

#[test]
fn task4851_catalogue_has_sections_rows_tags_required_words_and_red_no_tag_check() {
    let catalogue = render_permission_catalogue();
    let report = check_permission_catalogue_text(&catalogue).expect("catalogue is valid");

    println!("TASK4851 catalogue_start");
    print!("{catalogue}");
    println!("TASK4851 catalogue_end");
    println!("TASK4851 section_names={}", report.section_names);
    println!("TASK4851 permission_rows={}", report.permission_rows);
    println!("TASK4851 enforcement_tags={}", report.enforcement_tags);
    println!("TASK4851 allowed_tags=KEY,RELAY,TRUST");
    println!(
        "TASK4851 required_rows={}",
        REQUIRED_PERMISSION_ROWS.join("|")
    );

    assert_eq!(report.section_names, 7);
    assert_eq!(report.section_names, SECTION_NAMES.len());
    assert_eq!(report.permission_rows, 40);
    assert_eq!(report.enforcement_tags, 40);
    assert!(report
        .tags
        .iter()
        .all(|tag| ["KEY", "RELAY", "TRUST"].contains(tag)));
    for row in REQUIRED_PERMISSION_ROWS {
        assert!(
            catalogue.contains(row),
            "catalogue should include required row: {row}"
        );
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_osl-permission-catalogue"))
        .arg("check")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn permission catalogue checker");
    child
        .stdin
        .as_mut()
        .expect("checker stdin")
        .write_all(b"seeing\n- read a text channel `KEY`\ntalking\n- send a message\n")
        .expect("write invalid catalogue");
    let output = child
        .wait_with_output()
        .expect("checker returns output for invalid catalogue");
    let stderr = String::from_utf8(output.stderr).expect("checker stderr is utf8");

    println!(
        "TASK4851 no_tag_exit={}",
        output.status.code().unwrap_or_default()
    );
    println!("TASK4851 no_tag_stderr={stderr}");

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("row has no enforcement tag: send a message"));
}
