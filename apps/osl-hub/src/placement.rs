//! Explicit-user-action capsule placement without page injection or service APIs.
//!
//! This module never presses Enter. It emits deterministic Unicode keyboard
//! events only after the trusted OSL Privacy has focused its exact hosted service
//! webview. The caller must still report the outcome as unverified because a
//! platform can reject, transform, or route the text differently.

use serde::Serialize;

const MAX_CAPSULE_UTF16_UNITS: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementReceipt {
    pub utf16_units: usize,
    pub placement_attempted: bool,
    pub sent: bool,
    pub outcome_verified: bool,
}

pub fn place_unicode_capsule(capsule: &str) -> Result<PlacementReceipt, String> {
    let units: Vec<u16> = capsule.encode_utf16().collect();
    if units.is_empty() || units.len() > MAX_CAPSULE_UTF16_UNITS || units.contains(&0) {
        return Err("The encrypted message cannot be entered safely".to_owned());
    }
    emit_unicode(&units)?;
    Ok(PlacementReceipt {
        utf16_units: units.len(),
        placement_attempted: true,
        sent: false,
        outcome_verified: false,
    })
}

#[cfg(windows)]
fn emit_unicode(units: &[u16]) -> Result<(), String> {
    use std::mem::size_of;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    let mut inputs = Vec::with_capacity(units.len() * 2);
    for unit in units {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: *unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    let inserted = unsafe {
        SendInput(
            u32::try_from(inputs.len())
                .map_err(|_| "The encrypted message is too large to enter".to_owned())?,
            inputs.as_ptr(),
            i32::try_from(size_of::<INPUT>())
                .map_err(|_| "Entering the encrypted message is unavailable".to_owned())?,
        )
    };
    if inserted != inputs.len() as u32 {
        return Err("Windows did not accept the complete encrypted message".to_owned());
    }
    Ok(())
}

#[cfg(not(windows))]
fn emit_unicode(_units: &[u16]) -> Result<(), String> {
    Err("Entering encrypted messages is available only in the Windows desktop build".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_nul_and_oversized_capsules_before_platform_input() {
        let empty_error = place_unicode_capsule("").unwrap_err();
        assert_eq!(
            empty_error,
            "The encrypted message cannot be entered safely"
        );
        assert!(!empty_error.to_ascii_lowercase().contains("capsule"));
        assert!(place_unicode_capsule("a\0b").is_err());
        assert!(place_unicode_capsule(&"x".repeat(MAX_CAPSULE_UTF16_UNITS + 1)).is_err());
    }
}
