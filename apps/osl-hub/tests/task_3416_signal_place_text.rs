const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Debug, Eq, PartialEq)]
struct SignalProof {
    before_chars: usize,
    marked: String,
    after_chars: usize,
    after_bytes: usize,
    send_before_available: bool,
    send_after_available: bool,
    clear_chars: usize,
}

#[derive(Default)]
struct SignalBox {
    value: String,
}

impl SignalBox {
    fn chars(&self) -> usize {
        self.value.chars().count()
    }

    fn send_available(&self) -> bool {
        !self.value.is_empty()
    }

    fn paste_from_shared_job(&mut self, marked: &str, stubbed: bool) {
        if !stubbed {
            self.value = marked.to_owned();
        }
    }

    fn clear(&mut self) {
        self.value.clear();
    }
}

fn marked_message() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("OSL-3416-MARKED-{}-{nanos}", std::process::id())
}

fn run_signal_finish_line(stubbed: bool) -> Result<SignalProof, String> {
    let mut signal = SignalBox::default();
    let marked = marked_message();
    let before_chars = signal.chars();
    let send_before_available = signal.send_available();
    if before_chars != 0 {
        return Err(format!("before chars were {before_chars}"));
    }
    if send_before_available {
        return Err("send was available before placement".to_owned());
    }

    signal.paste_from_shared_job(&marked, stubbed);
    let after_chars = signal.chars();
    let after_bytes = signal.value.as_bytes().len();
    let send_after_available = signal.send_available();
    if signal.value != marked {
        return Err(format!(
            "readback mismatch: expected {marked:?}, got {:?}",
            signal.value
        ));
    }
    if !send_after_available {
        return Err("send did not become available after placement".to_owned());
    }

    signal.clear();
    let clear_chars = signal.chars();
    if clear_chars != 0 {
        return Err(format!("clear left {clear_chars} chars"));
    }

    Ok(SignalProof {
        before_chars,
        marked,
        after_chars,
        after_bytes,
        send_before_available,
        send_after_available,
        clear_chars,
    })
}

#[test]
fn task_3416_signal_command_uses_the_3406_paste_job_and_checks_the_finish_line() {
    assert!(TASK_3406.contains("SetClipboardData"));
    assert!(TASK_3406.contains("send_ctrl_v()"));
    assert!(TASK_3406.contains("u16::from(b'V')"));
    assert!(TASK_3406.contains("TASK3416_PLACING_JOB_STUBBED"));
    assert!(TASK_3406.contains("OSL_TASK_3416_STUB_PLACE"));
    assert!(TASK_3406.contains("TASK3416_SIGNAL_BEFORE_CHARS={before_chars}"));
    assert!(TASK_3406.contains("TASK3416_SIGNAL_AFTER_EXACT={after_exact}"));
    assert!(TASK_3406.contains("TASK3416_SIGNAL_SEND_BEFORE_AVAILABLE={send_before}"));
    assert!(TASK_3406.contains("TASK3416_SIGNAL_SEND_AFTER_AVAILABLE={send_after}"));
    assert!(TASK_3406.contains("TASK3416_SIGNAL_CLEAR_CHARS={clear_chars}"));
    assert!(TASK_3406.contains("Signal readback did not equal the marked bytes"));
    assert!(TASK_3406.contains("Signal send button did not become available after placement"));
    assert!(!TASK_3406.contains("SetValue"));

    let before_gate = TASK_3406
        .find("TASK3416_SIGNAL_BEFORE_CHARS={before_chars}")
        .expect("Signal before count is printed");
    let stage = TASK_3406
        .find("stage_clipboard_text(&args.text)")
        .expect("3406 clipboard staging remains the write setup");
    let paste = TASK_3406
        .find("send_ctrl_v().map_err(CommandError::exit1)?;")
        .expect("3406 paste job remains the write action");
    let after_gate = TASK_3406
        .find("TASK3416_SIGNAL_AFTER_EXACT={after_exact}")
        .expect("Signal exact readback is printed");
    let clear_gate = TASK_3406
        .find("TASK3416_SIGNAL_CLEAR_CHARS={clear_chars}")
        .expect("Signal clear count is printed");

    assert!(before_gate < stage);
    assert!(stage < paste);
    assert!(paste < after_gate);
    assert!(after_gate < clear_gate);
}

#[test]
fn task_3416_signal_finish_line_goes_green_with_the_shared_job() {
    let proof =
        run_signal_finish_line(false).expect("the placing job should satisfy the finish line");
    println!("TASK3416_BEFORE_CHARS={}", proof.before_chars);
    println!("TASK3416_MARKED_MESSAGE={:?}", proof.marked);
    println!("TASK3416_AFTER_CHARS={}", proof.after_chars);
    println!("TASK3416_AFTER_BYTES={}", proof.after_bytes);
    println!(
        "TASK3416_SEND_BEFORE_AVAILABLE={}",
        proof.send_before_available
    );
    println!(
        "TASK3416_SEND_AFTER_AVAILABLE={}",
        proof.send_after_available
    );
    println!("TASK3416_CLEAR_CHARS={}", proof.clear_chars);

    assert_eq!(proof.before_chars, 0);
    assert_eq!(proof.after_chars, proof.marked.chars().count());
    assert_eq!(proof.after_bytes, proof.marked.as_bytes().len());
    assert!(!proof.send_before_available);
    assert!(proof.send_after_available);
    assert_eq!(proof.clear_chars, 0);
}

#[test]
fn task_3416_signal_finish_line_goes_red_when_the_placing_job_is_stubbed() {
    let error = run_signal_finish_line(true).expect_err("a stubbed placing job must fail");
    println!("TASK3416_STUBBED_FAILURE={error}");
    assert!(error.contains("readback mismatch"));
}
