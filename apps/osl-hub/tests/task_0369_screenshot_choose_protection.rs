use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn run_choose_protection_capture(root: &Path) -> Output {
    let out = root.join("evidence/task-0369-choose-protection/out");
    let run = root.join("evidence/task-0369-choose-protection/run");
    let fixture = root.join("scripts/qa/osl-choose-protection-fixture.sh");
    Command::new("bash")
        .arg(root.join("scripts/qa/osl-fixed-screen-test-starter.sh"))
        .env("OSL_FIXED_SCREEN_BIN", fixture)
        .env("OSL_FIXED_SCREEN_OUT", out)
        .env("OSL_FIXED_SCREEN_RUN_DIR", run)
        .env("OSL_FIXED_SCREEN_SIZE", "1440x900x24")
        .env("OSL_FIXED_SCREEN_WAIT_SECONDS", "10")
        .env("OSL_FIXED_SCREEN_CAPTURE_ATTEMPTS", "30")
        .output()
        .expect("choose-protection fixed-screen capture can launch")
}

fn command_stdout(command: &mut Command) -> String {
    let output = command.output().expect("command can run");
    assert!(
        output.status.success(),
        "command failed; stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("command stdout is utf8")
}

fn bright_pixels(image: &Path, label: &str, crop: &str) -> u32 {
    let expression = "%[fx:mean*w*h]";
    let value = command_stdout(
        Command::new("convert")
            .arg(format!("{}[{}]", image.display(), crop))
            .args([
                "-colorspace",
                "Gray",
                "-threshold",
                "55%",
                "-format",
                expression,
                "info:",
            ]),
    );
    let count = value.trim().parse::<f64>().expect("bright pixel count");
    println!("TASK0369_IMAGE_REGION label={label:?} bright_pixels={count:.0} crop={crop}");
    count.round() as u32
}

#[test]
fn choose_protection_capture_contains_required_screen_tree_and_visible_controls() {
    let root = repo_root();
    let output = run_choose_protection_capture(&root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "capture must pass; stdout={stdout}; stderr={stderr}"
    );
    println!("{stdout}");
    if !stderr.trim().is_empty() {
        println!(
            "TASK0369_CAPTURE_STDERR {}",
            stderr.trim().replace('\n', " | ")
        );
    }

    assert!(
        stdout.contains("TASK0063_SWITCHES")
            && stdout.contains("screen=1440x900x24")
            && stdout.contains("window=OSL\\ Privacy"),
        "fixed fixture switches must name window and size: {stdout}"
    );
    assert!(
        stdout.contains("TASK0063_GEOMETRY width=1440 height=900"),
        "captured window must use the fixed 1440x900 size: {stdout}"
    );
    assert!(
        stdout.contains("TASK0063_CAPTURE_CHECK status=ok image_count=1 metadata_count=1"),
        "starter must save one PNG and its metadata: {stdout}"
    );

    let out = root.join("evidence/task-0369-choose-protection/out");
    let image = out.join("osl-fixed-screen.png");
    let metadata = out.join("osl-fixed-screen.json");
    let screen_tree = out.join("choose-protection-screen-tree.txt");
    assert!(
        image.is_file(),
        "captured PNG must exist at {}",
        image.display()
    );
    assert!(
        metadata.is_file(),
        "capture metadata must exist at {}",
        metadata.display()
    );
    assert!(
        screen_tree.is_file(),
        "screen tree must exist at {}",
        screen_tree.display()
    );

    let tree = fs::read_to_string(&screen_tree).expect("screen tree can be read");
    for required in [
        "Choose protection",
        "Basic",
        "Balanced",
        "Maximum",
        "Continue",
        "Back",
    ] {
        assert!(
            tree.contains(required),
            "screen tree must contain {required:?}: {tree}"
        );
        println!("TASK0369_TREE_ASSERT label={required:?} present=true");
    }

    let identify = command_stdout(
        Command::new("identify")
            .args([
                "-format",
                "TASK0369_IMAGE width=%w height=%h colors=%k bytes=%b\n",
            ])
            .arg(&image),
    );
    print!("{identify}");
    assert!(identify.contains("width=1440 height=900"), "{identify}");
    let colors = identify
        .split("colors=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u32>().ok())
        .expect("image color count is printed");
    assert!(
        colors > 5,
        "PNG must not be blank or nearly blank: {identify}"
    );

    for (label, crop, minimum) in [
        ("Choose protection", "650x90+105+100", 2_000),
        ("Basic", "180x70+145+260", 350),
        ("Balanced", "245x70+570+260", 650),
        ("Maximum", "265x70+995+260", 700),
        ("Continue", "210x65+790+720", 650),
        ("Back", "120x65+465+720", 350),
    ] {
        let count = bright_pixels(&image, label, crop);
        assert!(
            count >= minimum,
            "{label} must be visible in the captured image; got {count}, need {minimum}"
        );
    }
}
