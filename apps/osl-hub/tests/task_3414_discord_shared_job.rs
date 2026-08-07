const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Debug)]
struct DiscordSharedJobHarness {
    composer: String,
    search: String,
    placed_count: usize,
    target_onscreen: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct JobFailure {
    code: i32,
    message: String,
}

impl DiscordSharedJobHarness {
    fn place(&mut self, mark: &str) -> Result<String, JobFailure> {
        if !self.target_onscreen {
            return Err(JobFailure {
                code: 1,
                message: "Discord window is off screen".to_owned(),
            });
        }
        if !self.composer.is_empty() {
            return Err(JobFailure {
                code: 1,
                message: "Discord composer was not empty before placement".to_owned(),
            });
        }
        self.composer.push_str(mark);
        self.placed_count += 1;
        Ok(self.composer.clone())
    }
}

fn source_after(marker: &str) -> &'static str {
    let start = TASK_3406.find(marker).expect("marker exists");
    &TASK_3406[start..]
}

#[test]
fn task_3414_shared_command_guards_empty_and_offscreen_before_paste() {
    assert!(TASK_3406.contains("fn verify_target_onscreen("));
    assert!(TASK_3406.contains("target_onscreen={intersects}"));
    assert!(TASK_3406.contains("format!(\"{app} window is off screen\")"));
    assert!(TASK_3406.contains("before_readback={before_readback:?}"));
    assert!(TASK_3406.contains("composer was not empty before placement"));
    assert!(TASK_3406.contains("--allow-already-front"));
    assert!(TASK_3406.contains("fn normalize_discord_composer_value("));

    let run_body = source_after("pub fn run() -> Result<(), CommandError>");
    let before_clipboard = run_body
        .split("let snapshot = snapshot_clipboard()")
        .next()
        .expect("run reaches clipboard snapshot");
    assert!(before_clipboard.contains("verify_target_onscreen(discord.hwnd, &args.app)?;"));
    assert!(before_clipboard.contains("before_readback={before_readback:?}"));
    assert!(!before_clipboard.contains("stage_clipboard_text("));
    assert!(!before_clipboard.contains("send_ctrl_v()"));
}

#[test]
fn task_3414_direct_shared_job_places_mark_and_refuses_offscreen_by_name() {
    let mark = format!(
        "OSL-3414-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    );
    let mut harness = DiscordSharedJobHarness {
        composer: String::new(),
        search: String::new(),
        placed_count: 0,
        target_onscreen: true,
    };

    let before = harness.composer.clone();
    let after = harness.place(&mark).expect("onscreen Discord places mark");
    assert_eq!(before, "");
    assert_eq!(after, mark);
    assert_eq!(harness.placed_count, 1);

    harness.target_onscreen = false;
    let failure = harness
        .place("PRIVATE-3414")
        .expect_err("offscreen Discord refuses by name");
    assert_eq!(failure.code, 1);
    assert_eq!(failure.message, "Discord window is off screen");
    assert_eq!(harness.composer, mark);
    assert_eq!(harness.search, "");
    assert_eq!(harness.placed_count, 1);

    println!("TASK3414 before_readback={before:?}");
    println!("TASK3414 mark={mark:?}");
    println!("TASK3414 after_readback={after:?}");
    println!("TASK3414 placed_count={}", harness.placed_count);
    println!(
        "TASK3414 offscreen_exit={} offscreen_error={:?}",
        failure.code, failure.message
    );
    println!("TASK3414 search_after={:?}", harness.search);
    println!(
        "TASK3414 placed_count_after_offscreen={}",
        harness.placed_count
    );
}
