use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn assert_product_mullvad_markup(root: &Path) {
    let source =
        fs::read_to_string(root.join("apps/osl-hub-ui/src/main.ts")).expect("main.ts is readable");
    let mullvad_start = source
        .find("function mullvadSetupContent()")
        .expect("Mullvad setup markup exists");
    let mullvad_end = source[mullvad_start..]
        .find("function scrubCategoryChooserMarkup")
        .map(|offset| mullvad_start + offset)
        .expect("Mullvad setup markup closes before the next function");
    let mullvad = &source[mullvad_start..mullvad_end];

    for required in [
        r#"<h1 id="route-heading" tabindex="-1" class="mv-title">Mullvad</h1>"#,
        r#""found session""#,
        r#""install""#,
        r#"id="continue-mullvad""#,
        r#">Not now</button>"#,
        r#"id="skip-mullvad""#,
    ] {
        assert!(
            mullvad.contains(required),
            "Mullvad product markup must contain {required}; branch={mullvad}"
        );
    }
    let render_start = source
        .find("function renderOnboarding()")
        .expect("onboarding renderer exists");
    let render_end = source[render_start..]
        .find("function bindOnboarding")
        .map(|offset| render_start + offset)
        .expect("onboarding renderer closes before binding");
    let renderer = &source[render_start..render_end];
    assert!(
        renderer.contains(r#""mullvad""#) && renderer.contains(r#"id="onboarding-back""#),
        "Mullvad setup must receive the shared Back control; renderer={renderer}"
    );
    println!("TASK0374_PRODUCT_TITLE=Mullvad");
    for control in ["found session", "install", "Continue", "Not now", "Back"] {
        println!("TASK0374_PRODUCT_CONTROL={control}");
    }
}

#[test]
fn mullvad_fixed_fixture_capture_has_required_screen_tree_and_nonblank_png() {
    let root = repo_root();
    assert_product_mullvad_markup(&root);

    let output = Command::new("bash")
        .arg(root.join("scripts/qa/osl-mullvad-fixed-screen-capture.sh"))
        .env(
            "OSL_MULLVAD_SCREEN_OUT",
            root.join("evidence/task-0374-mullvad-screen"),
        )
        .env("OSL_MULLVAD_SCREEN_SIZE", "1024x768x24")
        .output()
        .expect("Mullvad fixed-screen capture script can be launched");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Mullvad fixed-screen capture must pass; stdout={stdout}; stderr={stderr}"
    );
    print!("{stdout}");
    eprint!("{stderr}");
    assert!(
        stdout.contains("TASK0374_SCREEN_TREE_TITLE=Mullvad"),
        "screen tree must show title Mullvad: {stdout}"
    );
    for control in ["found session", "install", "Continue", "Not now", "Back"] {
        assert!(
            stdout.contains(&format!("TASK0374_SCREEN_TREE_CONTROL={control}")),
            "screen tree must show control {control}: {stdout}"
        );
    }
    let image_line = stdout
        .lines()
        .find(|line| line.starts_with("TASK0374_IMAGE "))
        .expect("capture printed image facts");
    assert!(
        image_line.contains("width=1024")
            && image_line.contains("height=768")
            && image_line.contains("bytes=")
            && image_line.contains("colors="),
        "image facts must include fixed size and nonblank metrics: {image_line}"
    );
    assert!(
        stdout.contains(
            "TASK0374_DONE title=Mullvad controls=found session,install,Continue,Not now,Back blank=false"
        ),
        "finish line must be printed: {stdout}"
    );
}
