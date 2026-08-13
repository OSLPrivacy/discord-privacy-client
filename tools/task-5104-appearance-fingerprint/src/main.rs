use std::{env, fs};
use task_5104_appearance_fingerprint::*;

fn fail(message: &str) -> ! {
    eprintln!("TASK5104 CHECK FAILED: {message}");
    std::process::exit(1)
}
fn main() {
    if AppearanceGuard::configure(5).is_err() {
        fail("foreground interval exceeds 5 seconds")
    }
    let args: Vec<_> = env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--matrix") {
        println!("TASK5104 foreground_pixel_interval_secs=5 full_uia_backstop_secs=30 minimum_distinct_rgb_colours=32 status=green");
        return;
    }
    if args.get(1).map(String::as_str) != Some("--live-evidence") || args.len() != 3 {
        fail("missing carrier state: provide shipping Windows real-carrier evidence");
    }
    let raw = fs::read_to_string(&args[2])
        .unwrap_or_else(|_| fail("missing carrier state: evidence artifact unreadable"));
    let evidence: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|_| fail("missing carrier state: evidence artifact is not JSON"));
    let required_true = [
        "shipping_windows_build",
        "independently_signed_in",
        "carrier_visible_before_after",
        "shipping_integration_enabled",
    ];
    for field in required_true {
        if evidence.get(field).and_then(serde_json::Value::as_bool) != Some(true) {
            fail(&format!("missing carrier state: {field}"));
        }
    }
    for field in [
        "unique_marker",
        "carrier_visible_before",
        "carrier_visible_after",
        "date_utc",
    ] {
        if evidence
            .get(field)
            .and_then(serde_json::Value::as_str)
            .filter(|v| !v.is_empty())
            .is_none()
        {
            fail(&format!("missing carrier state: {field}"));
        }
    }
    if evidence
        .get("distinct_rgb_colours")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        < u64::from(MIN_DISTINCT_RGB_COLOURS)
    {
        fail(
            "missing carrier state: CopyFromScreen capture has fewer than 32 distinct RGB colours",
        );
    }
    let tools = evidence
        .get("observation_tools")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| fail("missing carrier state: Windows observation tools"));
    for required in ["tasklist.exe", "Windows PowerShell", "CopyFromScreen"] {
        if !tools.iter().any(|v| v.as_str() == Some(required)) {
            fail(&format!(
                "missing carrier state: required Windows observation tool {required}"
            ));
        }
    }
    if raw.to_ascii_lowercase().contains("fixture")
        || raw.to_ascii_lowercase().contains("harness")
        || raw.to_ascii_lowercase().contains("simulated")
        || raw.to_ascii_lowercase().contains("canned")
    {
        fail("missing carrier state: non-live source is forbidden");
    }
    println!(
        "TASK5104 LIVE evidence=accepted marker={} distinct_rgb_colours={}",
        evidence["unique_marker"].as_str().unwrap(),
        evidence["distinct_rgb_colours"].as_u64().unwrap()
    );
}
