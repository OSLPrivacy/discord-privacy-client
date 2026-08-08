//! Direct bridge from persisted look choices to a window's computed style values.

use crate::security::{self, HubSecurityState};

/// Each persisted choice has one, and only one, window style property.
pub const LOOK_STYLE_BINDINGS: [(&str, &str); 8] = [
    ("theme", "--osl-look-theme"),
    ("named-look", "--osl-look-named-look"),
    ("accent", "--osl-look-accent"),
    ("corners", "--osl-look-corners"),
    ("glow", "--osl-look-glow"),
    ("text", "--osl-look-text"),
    ("spacing", "--osl-look-spacing"),
    ("see-through", "--osl-look-see-through"),
];

/// Minimal window seam so native hosts and the Linux proof use the same binding.
/// The named accent shown for a fresh profile and after Reset. The style value
/// is kept alongside the product-facing name so tests and window bindings read
/// the same documented default instead of inventing separate fallbacks.
pub const DEFAULT_ACCENT_NAME: &str = "OSL blue";
pub const DEFAULT_ACCENT_VALUE: &str = "#2ac0f0";

pub fn resolved_accent_choice(security_state: &HubSecurityState) -> Result<String, String> {
    Ok(
        security::look_choice_value(security_state, "accent".to_owned())?
            .unwrap_or_else(|| DEFAULT_ACCENT_NAME.to_owned()),
    )
}
pub trait LookStyleWindow {
    fn set_style_value(&mut self, property: &str, value: &str) -> Result<(), String>;
}

/// Read every persisted look choice and apply it to its corresponding window style.
pub fn apply_saved_look_choices<W: LookStyleWindow>(
    security_state: &HubSecurityState,
    window: &mut W,
) -> Result<usize, String> {
    let mut changed = 0;
    for (name, property) in LOOK_STYLE_BINDINGS {
        if let Some(value) = security::look_choice_value(security_state, name.to_owned())? {
            window.set_style_value(property, &value)?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Verify the direct saved-to-computed correspondence used by the proof.
pub fn saved_values_match_computed(
    saved: &[(&str, &str); 8],
    computed: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    for ((name, property), (_, saved_value)) in LOOK_STYLE_BINDINGS.iter().zip(saved) {
        let computed_value = computed
            .get(*property)
            .ok_or_else(|| format!("missing computed style for {name}"))?;
        if computed_value != saved_value {
            return Err(format!(
                "saved {name}={saved_value} did not match computed {property}={computed_value}"
            ));
        }
    }
    Ok(())
}

/// Test-double window whose values are the computed window styles on Linux.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinuxTestWindow {
    pub computed_style_values: std::collections::BTreeMap<String, String>,
}

impl LookStyleWindow for LinuxTestWindow {
    fn set_style_value(&mut self, property: &str, value: &str) -> Result<(), String> {
        self.computed_style_values
            .insert(property.to_owned(), value.to_owned());
        Ok(())
    }
}
