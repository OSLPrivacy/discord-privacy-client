use unicode_security::{
    skeleton, GeneralSecurityProfile, MixedScript, RestrictionLevel, RestrictionLevelDetection,
};
use unicode_normalization::UnicodeNormalization;
use unicode_general_category::{get_general_category, GeneralCategory};
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

fn has_five_consecutive_nonspacing_marks(identifier: &str) -> bool {
    let mut consecutive = 0usize;
    for character in identifier.chars() {
        if get_general_category(character) == GeneralCategory::NonspacingMark {
            consecutive += 1;
            if consecutive > 4 { return true; }
        } else {
            consecutive = 0;
        }
    }
    false
}

/// Runs the UTS #39 primitives that the username policy consumes.
///
/// This is intentionally the only implementation exposed to JavaScript: the
/// Worker must not grow a second, subtly different TypeScript implementation.
#[wasm_bindgen]
pub fn analyze_identifier(identifier: &str) -> String {
    // Rust's standard Unicode casing is the closest stable case-folding
    // primitive available in this pinned UTS #39 artifact.  Keep it here,
    // beside skeleton/restriction analysis, so Worker callers cannot drift
    // into a hand-written JavaScript normalizer.
    let normalized = identifier.nfkc().flat_map(char::to_lowercase).collect::<String>();
    let identifier_allowed = !normalized.is_empty()
        && normalized.chars().all(GeneralSecurityProfile::identifier_allowed);
    let skeleton = skeleton(&normalized).collect::<String>();
    let single_script = normalized.as_str().is_single_script();
    let restriction_level = restriction_level_name(normalized.as_str().detect_restriction_level());
    let has_excess_marks = has_five_consecutive_nonspacing_marks(identifier);

    format!(
        "{{\"normalized\":{},\"skeleton\":{},\"identifierAllowed\":{},\"singleScript\":{},\"restrictionLevel\":{},\"hasExcessMarks\":{}}}",
        json_string(&normalized),
        json_string(&skeleton),
        identifier_allowed,
        single_script,
        json_string(restriction_level),
        has_excess_marks,
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
