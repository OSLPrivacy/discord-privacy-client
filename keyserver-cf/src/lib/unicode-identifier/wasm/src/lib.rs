use unicode_security::{
    skeleton, GeneralSecurityProfile, MixedScript, RestrictionLevel, RestrictionLevelDetection,
};
use wasm_bindgen::prelude::*;

fn restriction_level_name(level: RestrictionLevel) -> &'static str {
    match level {
        RestrictionLevel::ASCIIOnly => "ascii-only",
        RestrictionLevel::SingleScript => "single-script",
        RestrictionLevel::HighlyRestrictive => "highly-restrictive",
        RestrictionLevel::ModeratelyRestrictive => "moderately-restrictive",
        RestrictionLevel::MinimallyRestrictive => "minimally-restrictive",
        RestrictionLevel::Unrestricted => "unrestricted",
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{001f}' => {
                use core::fmt::Write;
                write!(escaped, "\\u{:04x}", character as u32).expect("writing to String cannot fail");
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Runs the UTS #39 primitives that the username policy consumes.
///
/// This is intentionally the only implementation exposed to JavaScript: the
/// Worker must not grow a second, subtly different TypeScript implementation.
#[wasm_bindgen]
pub fn analyze_identifier(identifier: &str) -> String {
    let identifier_allowed = !identifier.is_empty()
        && identifier.chars().all(GeneralSecurityProfile::identifier_allowed);
    let skeleton = skeleton(identifier).collect::<String>();
    let single_script = identifier.is_single_script();
    let restriction_level = restriction_level_name(identifier.detect_restriction_level());

    format!(
        "{{\"skeleton\":{},\"identifierAllowed\":{},\"singleScript\":{},\"restrictionLevel\":{}}}",
        json_string(&skeleton),
        identifier_allowed,
        single_script,
        json_string(restriction_level),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confusable_skeleton_matches_the_rust_crate_vector() {
        assert_eq!(skeleton("p\u{0430}ypal").collect::<String>(), "paypal");
    }

    #[test]
    fn profile_and_restriction_checks_reject_invisible_mixed_script_input() {
        let identifier = "p\u{0430}ypal\u{200b}";
        assert!(!identifier.chars().all(GeneralSecurityProfile::identifier_allowed));
        assert!(!identifier.is_single_script());
        assert_eq!(identifier.detect_restriction_level(), RestrictionLevel::Unrestricted);
    }
}
