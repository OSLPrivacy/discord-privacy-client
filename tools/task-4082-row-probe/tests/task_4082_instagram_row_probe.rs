use std::process::Command;

fn fixture(name: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    root.join("tests")
        .join("fixtures")
        .join("task_4082")
        .join(name)
        .display()
        .to_string()
}

fn run_probe(surface: &str, fixture_name: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_task-4082-row-probe"))
        .args(["--surface", surface, "--fixture", &fixture(fixture_name)])
        .output()
        .expect("task 4082 probe runs")
}

#[test]
fn task_4082_instagram_row_author_probe_has_discord_control_and_signed_out_refusal() {
    let discord = run_probe("discord", "discord-control.json");
    let discord_stdout = String::from_utf8(discord.stdout).expect("discord stdout is utf8");
    let discord_stderr = String::from_utf8(discord.stderr).expect("discord stderr is utf8");
    print!("{discord_stdout}");
    eprint!("{discord_stderr}");
    assert!(discord.status.success());
    let first_line = discord_stdout
        .lines()
        .next()
        .expect("discord printed a first line");
    let discord_found = first_line
        .split_whitespace()
        .next()
        .expect("first token is count")
        .parse::<usize>()
        .expect("first token parses as count");
    assert!(discord_found >= 8, "discord control found {discord_found}");
    assert!(discord_stdout.contains("TASK4082_DISCORD_CONTROL_TOTAL_ROWS=10"));
    assert!(discord_stdout.contains("TASK4082_DISCORD_CONTROL_COLOUR_OR_POSITION_FINDINGS=0"));

    let instagram = run_probe("instagram", "instagram-open-conversation.json");
    let instagram_stdout = String::from_utf8(instagram.stdout).expect("instagram stdout is utf8");
    let instagram_stderr = String::from_utf8(instagram.stderr).expect("instagram stderr is utf8");
    print!("{instagram_stdout}");
    eprint!("{instagram_stderr}");
    assert!(instagram.status.success());
    assert_eq!(
        instagram_stdout
            .lines()
            .filter(|line| line.starts_with("TASK4082_INSTAGRAM_ROW "))
            .count(),
        10
    );
    assert!(instagram_stdout.contains("TASK4082_INSTAGRAM_TOTAL_ROWS=10"));
    assert!(instagram_stdout.contains("TASK4082_INSTAGRAM_ROWS_WITH_NO_LINE=0"));
    assert!(instagram_stdout.contains("TASK4082_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS=0"));
    assert!(instagram_stdout.contains("TASK4082_INSTAGRAM_PAGE_READ_DATE=2026-08-07"));
    assert!(!instagram_stdout.contains(" evidence=none"));

    let signed_out = run_probe("instagram", "instagram-signed-out.json");
    let signed_out_stderr =
        String::from_utf8(signed_out.stderr).expect("signed-out stderr is utf8");
    eprint!("{signed_out_stderr}");
    println!(
        "TASK4082_SIGNED_OUT_EXIT={}",
        signed_out.status.code().unwrap_or(255)
    );
    assert_eq!(signed_out.status.code(), Some(1));
    assert!(signed_out_stderr.contains("refused signed-out instagram page"));
}
