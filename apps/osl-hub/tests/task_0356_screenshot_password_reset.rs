use std::{path::{Path, PathBuf}, process::Command};
fn root()->PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).unwrap().to_path_buf() }
#[test]
fn password_reset_fixed_fixture_capture_has_required_screen_tree_and_nonblank_png() {
 let r=root(); let o=Command::new("bash").arg(r.join("scripts/qa/osl-password-reset-fixed-screen-capture.sh")).env("OSL_PASSWORD_RESET_SCREEN_OUT",r.join("evidence/task-0356-password-reset")).env("OSL_PASSWORD_RESET_SCREEN_SIZE","1024x768x24").output().unwrap(); let s=String::from_utf8_lossy(&o.stdout); let e=String::from_utf8_lossy(&o.stderr); print!("{s}"); eprint!("{e}"); assert!(o.status.success(),"capture failed: {s} {e}");
 for x in ["Password reset","recovery phrase","password","Continue","Back"] { assert!(s.contains(&format!("TASK0356_SCREEN_TREE_CONTROL={x}")) || (x=="Password reset" && s.contains("TASK0356_SCREEN_TREE_TITLE=Password reset")),"missing {x}: {s}"); }
 assert!(s.contains("width=1024")&&s.contains("height=768")&&s.contains("bytes=")&&s.contains("colors=")); assert!(s.contains("TASK0356_DONE title=Password reset controls=recovery phrase,Continue,password,Back blank=false"));
}
