use std::process::Command;

const X_WINDOW_COMPOSER: &str = env!("CARGO_BIN_EXE_x-window-composer");

fn run(fixture: &str) -> String {
    let output = Command::new(X_WINDOW_COMPOSER)
        .arg(fixture)
        .output()
        .expect("run direct X window/composer command");
    assert!(output.status.success(), "{fixture} failed: {output:?}");
    String::from_utf8(output.stdout).expect("command stdout is UTF-8")
}

fn field(output: &str, name: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1101_{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {output:?}"))
        .to_owned()
}

#[test]
fn direct_command_returns_the_prepared_x_records_and_never_leaks_x_kind() {
    let x = run("x-direct");
    assert_eq!(field(&x, "BROWSER_TITLE"), "Messages / X");
    assert_eq!(field(&x, "PLACE_KIND"), "direct_message");
    assert_eq!(field(&x, "COMPOSER"), "Message");

    let instagram = run("instagram-direct");
    assert_eq!(field(&instagram, "BROWSER_TITLE"), "Messages • Instagram");
    assert_eq!(field(&instagram, "COMPOSER"), "Message...");
    assert_ne!(field(&instagram, "PLACE_KIND"), field(&x, "PLACE_KIND"));

    let signal = run("signal-direct");
    for (name, output) in [("instagram", &instagram), ("signal", &signal)] {
        assert_ne!(
            field(output, "PLACE_KIND"),
            "direct_message",
            "{name} must not be classified as X"
        );
    }

    println!(
        "TASK1101_X title={} kind={} composer={}",
        field(&x, "BROWSER_TITLE"),
        field(&x, "PLACE_KIND"),
        field(&x, "COMPOSER")
    );
    println!("TASK1101_INSTAGRAM kind={}", field(&instagram, "PLACE_KIND"));
    println!("TASK1101_NON_X_FIXTURES_WITH_X_KIND=0");
}
