//! Getting the friend invite out of OSL on every desktop OSL runs on.
//!
//! The invite is not a secret. It is the exact value the operator hands to the
//! person they want to talk to, so there was never a reason for one platform to
//! be able to export it and another not. Export used to be a single Win32
//! clipboard write, which left Linux and macOS with no path at all: the copy
//! button reported "Copy invite is available in the Windows app" and the invite
//! string itself was rendered nowhere, so adding a friend off Windows was
//! impossible rather than merely awkward.
//!
//! Two independent routes exist now, and the renderer always draws the second:
//!
//! 1. This module, which hands the invite to whichever clipboard helper the
//!    desktop session actually has.
//! 2. The invite rendered as selectable text in the invite card, which needs no
//!    helper, no daemon and no permission, and is therefore the route that
//!    cannot fail.
//!
//! Route 1 failing is an inconvenience, not a dead end. That is why a desktop
//! with no helper at all gets a named refusal that points at route 2, instead
//! of a generic failure or a platform apology.

use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One clipboard helper: a program, plus the arguments that make it take the
/// value from standard input.
///
/// Borrowed rather than `'static` so a test can assemble a helper whose
/// arguments are only known at run time and exercise the real spawn path,
/// instead of asserting against a hard-coded table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipboardHelper<'a> {
    pub program: &'a str,
    pub args: &'a [&'a str],
}

/// What OSL says when this desktop has no clipboard helper at all.
///
/// It names the tools that would have worked and points at the invite already
/// on screen. A refusal the operator cannot act on is precisely the failure
/// this module exists to remove, so this string must stay actionable and must
/// never become a statement about which platform is supported.
pub const NO_CLIPBOARD_HELPER: &str = "This desktop has no clipboard tool OSL can use (wl-copy, xclip, xsel or pbcopy). Your full invite is shown on screen: select it there and copy it by hand.";

/// How long a helper may stay silent before OSL treats it as having taken the
/// value. An X11 clipboard owner has to keep running to serve the selection, so
/// "still alive after it was given the bytes" is the success case for `xclip`
/// and `xsel`, not a hang.
const HELPER_GRACE: Duration = Duration::from_millis(250);
const HELPER_POLL: Duration = Duration::from_millis(10);

/// The helpers OSL will try on this desktop, in preference order.
///
/// Wayland comes first, because on a Wayland session the X11 tools reach at
/// best an XWayland clipboard the rest of the session cannot read.
#[cfg(all(unix, not(target_os = "macos")))]
pub const DESKTOP_CLIPBOARD_HELPERS: &[ClipboardHelper<'static>] = &[
    ClipboardHelper {
        program: "wl-copy",
        args: &["--type", "text/plain"],
    },
    ClipboardHelper {
        program: "xclip",
        args: &["-selection", "clipboard"],
    },
    ClipboardHelper {
        program: "xsel",
        args: &["--clipboard", "--input"],
    },
];

#[cfg(target_os = "macos")]
pub const DESKTOP_CLIPBOARD_HELPERS: &[ClipboardHelper<'static>] = &[ClipboardHelper {
    program: "pbcopy",
    args: &[],
}];

/// Windows never goes through a helper process: it has a real clipboard API,
/// which the desktop shell calls directly. The empty table keeps this module
/// compiling and testable in a Windows build without giving it a second,
/// unexercised write path.
#[cfg(windows)]
pub const DESKTOP_CLIPBOARD_HELPERS: &[ClipboardHelper<'static>] = &[];

/// Put `value` on this desktop's clipboard, or say why that did not happen.
pub fn write_desktop_clipboard_text(value: &str) -> Result<(), String> {
    write_clipboard_text_with(DESKTOP_CLIPBOARD_HELPERS, value)
}

/// Offer `value` to each helper in turn and stop at the first one that takes it.
///
/// The distinction that matters here is between a helper that is *not
/// installed* and one that is installed and *refused*. A desktop with `xclip`
/// but no `wl-copy` must not be told the copy failed, so an absent program is
/// skipped silently; a helper that ran and failed is reported by name, because
/// that is a fault the operator can look into.
pub fn write_clipboard_text_with(helpers: &[ClipboardHelper<'_>], value: &str) -> Result<(), String> {
    let mut refusal: Option<String> = None;
    for helper in helpers {
        match offer_to_helper(helper, value) {
            HelperOutcome::Took => return Ok(()),
            HelperOutcome::Absent => continue,
            HelperOutcome::Refused => {
                refusal.get_or_insert_with(|| format!("{} could not take the invite", helper.program));
            }
        }
    }
    Err(refusal.unwrap_or_else(|| NO_CLIPBOARD_HELPER.to_owned()))
}

enum HelperOutcome {
    Took,
    Absent,
    Refused,
}

fn offer_to_helper(helper: &ClipboardHelper<'_>, value: &str) -> HelperOutcome {
    let Ok(mut child) = Command::new(helper.program)
        .args(helper.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return HelperOutcome::Absent;
    };
    let handed_over = match child.stdin.take() {
        // Taking the pipe out of the child drops it at the end of this arm,
        // which is what tells the helper the value is complete. A helper that
        // never sees the close would wait for more input forever.
        Some(mut stdin) => stdin.write_all(value.as_bytes()).is_ok(),
        None => false,
    };
    if !handed_over {
        let _ = child.kill();
        let _ = child.wait();
        return HelperOutcome::Refused;
    }
    let deadline = Instant::now() + HELPER_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    HelperOutcome::Took
                } else {
                    HelperOutcome::Refused
                }
            }
            Ok(None) if Instant::now() >= deadline => return HelperOutcome::Took,
            Ok(None) => std::thread::sleep(HELPER_POLL),
            Err(_) => return HelperOutcome::Refused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program name no desktop has, so "spawn failed" means "not installed"
    /// and nothing else.
    const ABSENT: ClipboardHelper<'static> = ClipboardHelper {
        program: "osl-no-such-clipboard-helper-b7f3d1",
        args: &[],
    };

    const INVITE: &str = "OSLFR1.eyJwYXlsb2FkIjp7InZlcnNpb24iOjEsIm9zbF91c2VyX2lkIjoib3NsX3Rlc3QifX0";

    #[test]
    fn a_desktop_with_no_helper_is_told_where_its_invite_actually_is() {
        let error = write_clipboard_text_with(&[ABSENT], INVITE).unwrap_err();
        assert_eq!(error, NO_CLIPBOARD_HELPER);
        // The refusal must stay actionable rather than becoming a statement
        // about which platform is supported, which is the defect this replaced.
        assert!(!error.to_lowercase().contains("windows"));
    }

    #[test]
    fn an_empty_helper_table_refuses_rather_than_claiming_success() {
        assert!(write_clipboard_text_with(&[], INVITE).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_helper_falls_through_to_the_next_one_which_gets_the_invite_verbatim() {
        let path = scratch_path("verbatim");
        let script = format!("cat > {}", shell_quote(&path));
        let args = ["-c", script.as_str()];
        let sink = ClipboardHelper {
            program: "sh",
            args: &args,
        };

        write_clipboard_text_with(&[ABSENT, sink], INVITE).expect("the installed helper takes it");

        assert_eq!(read_when_complete(&path, INVITE.len()), INVITE);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn a_helper_that_runs_and_fails_is_named_instead_of_reported_as_a_missing_tool() {
        let failing = ClipboardHelper {
            program: "false",
            args: &[],
        };
        let error = write_clipboard_text_with(&[failing], INVITE).unwrap_err();
        assert!(error.contains("false"), "expected the helper to be named: {error}");
        assert_ne!(error, NO_CLIPBOARD_HELPER);
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_helper_does_not_stop_a_later_one_from_taking_the_invite() {
        let path = scratch_path("after-failure");
        let script = format!("cat > {}", shell_quote(&path));
        let args = ["-c", script.as_str()];
        let failing = ClipboardHelper {
            program: "false",
            args: &[],
        };
        let sink = ClipboardHelper {
            program: "sh",
            args: &args,
        };

        write_clipboard_text_with(&[failing, sink], INVITE).expect("the later helper takes it");

        assert_eq!(read_when_complete(&path, INVITE.len()), INVITE);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    fn scratch_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "osl-invite-clipboard-{}-{name}.txt",
            std::process::id()
        ))
    }

    #[cfg(unix)]
    fn shell_quote(path: &std::path::Path) -> String {
        format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
    }

    /// The grace period can return before a shell has finished flushing, so
    /// wait for the whole value rather than racing it.
    #[cfg(unix)]
    fn read_when_complete(path: &std::path::Path, expected_len: usize) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                if text.len() >= expected_len {
                    return text;
                }
            }
            assert!(Instant::now() < deadline, "helper never wrote the invite");
            std::thread::sleep(HELPER_POLL);
        }
    }
}
