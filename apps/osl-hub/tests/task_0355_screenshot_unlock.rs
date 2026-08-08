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

fn assert_product_unlock_markup(root: &Path) {
    let source =
        fs::read_to_string(root.join("apps/osl-hub-ui/src/main.ts")).expect("main.ts is readable");
    let unlock_start = source
        .find(r#"<section class="unlock-card""#)
        .expect("unlock route markup exists");
    let unlock_end = source[unlock_start..]
        .find("</section>`;")
        .map(|offset| unlock_start + offset)
        .expect("unlock route markup closes");
    let unlock = &source[unlock_start..unlock_end];

    for required in [
        r#"<h1 id="route-heading" tabindex="-1">Unlock</h1>"#,
        r#"<label class="sr-only" for="identity-password">Password</label>"#,
        r#"placeholder="Password""#,
        r#">Unlock</button>"#,
        r#">Forgot password</button>"#,
        r#">Back</button>"#,
    ] {
        assert!(
            unlock.contains(required),
            "Unlock product markup must contain {required}; branch={unlock}"
        );
    }
    assert!(
        !unlock.contains(">Sign in</h1>"),
        "Unlock product title must be the exact task title"
    );
    assert_eq!(
        unlock.matches(r#"type="password""#).count(),
        1,
        "Unlock product markup must expose one password control"
    );
    println!("TASK0355_PRODUCT_TITLE=Unlock");
    println!("TASK0355_PRODUCT_CONTROL=password");
    println!("TASK0355_PRODUCT_CONTROL=Unlock");
    println!("TASK0355_PRODUCT_CONTROL=Forgot password");
    println!("TASK0355_PRODUCT_CONTROL=Back");
}

#[test]
fn unlock_fixed_fixture_capture_has_required_screen_tree_and_nonblank_png() {
    let root = repo_root();
    assert_product_unlock_markup(&root);

    let output = Command::new("bash")
        .arg(root.join("scripts/qa/osl-unlock-fixed-screen-capture.sh"))
        .env(
            "OSL_UNLOCK_SCREEN_OUT",
            root.join("evidence/task-0355-unlock-screen"),
        )
        .env("OSL_UNLOCK_SCREEN_SIZE", "1024x768x24")
        .output()
        .expect("Unlock fixed-screen capture script can be launched");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Unlock fixed-screen capture must pass; stdout={stdout}; stderr={stderr}"
    );
    print!("{stdout}");
    eprint!("{stderr}");
    assert!(
        stdout.contains("TASK0355_SCREEN_TREE_TITLE=Unlock"),
        "screen tree must show title Unlock: {stdout}"
    );
    for control in ["password", "Unlock", "Forgot password", "Back"] {
        assert!(
            stdout.contains(&format!("TASK0355_SCREEN_TREE_CONTROL={control}")),
            "screen tree must show control {control}: {stdout}"
        );
    }
    let image_line = stdout
        .lines()
        .find(|line| line.starts_with("TASK0355_IMAGE "))
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
            "TASK0355_DONE title=Unlock controls=password,Unlock,Forgot password,Back blank=false"
        ),
        "finish line must be printed: {stdout}"
    );
}
