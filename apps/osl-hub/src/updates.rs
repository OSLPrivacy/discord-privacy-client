//! Pure validation helpers for the trusted OSL Privacy updater boundary.

pub const RELEASES_URL: &str = "https://github.com/OSLPrivacy/discord-privacy-client/releases";
pub const MAX_RELEASE_NOTES_CHARS: usize = 2_000;

pub fn bounded_plain_notes(input: Option<&str>) -> String {
    let input = input.unwrap_or_default();
    input
        .chars()
        .filter(|character| !character.is_control() && *character != '<' && *character != '>')
        .take(MAX_RELEASE_NOTES_CHARS)
        .collect()
}

pub fn bounded_version(input: &str) -> Option<String> {
    if input.is_empty()
        || input.len() > 64
        || !input
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-".contains(character))
    {
        return None;
    }
    Some(input.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_metadata_is_plain_bounded_text() {
        assert_eq!(
            bounded_plain_notes(Some("safe <b>notes</b>\0")),
            "safe bnotes/b"
        );
        assert_eq!(bounded_plain_notes(Some(&"x".repeat(2_100))).len(), 2_000);
    }

    #[test]
    fn versions_are_bounded_and_never_markup() {
        assert_eq!(
            bounded_version("0.2.0-beta.1").as_deref(),
            Some("0.2.0-beta.1")
        );
        assert!(bounded_version("<script>").is_none());
        assert!(bounded_version(&"1".repeat(65)).is_none());
    }
}
