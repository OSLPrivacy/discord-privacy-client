//! Preview policy for decrypted attachment thumbnails.
//!
//! A thumbnail is a second rendering of an attachment.  View-once content must
//! only be rendered by its protected viewer, so this policy withholds a
//! thumbnail before the generating closure can observe the attachment bytes.

/// Generate a thumbnail only for an attachment that is not view-once.
///
/// The closure is deliberately lazy: view-once content returns `Ok(None)`
/// without invoking it, so callers cannot accidentally decode or retain a
/// preview while deciding whether to show one. Progress remains independent of
/// this policy because it does not require attachment content.
pub fn thumbnail_if_allowed<Thumbnail, Error, Generate>(
    view_once: bool,
    generate: Generate,
) -> Result<Option<Thumbnail>, Error>
where
    Generate: FnOnce() -> Result<Thumbnail, Error>,
{
    if view_once {
        return Ok(None);
    }

    generate().map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn view_once_attachment_never_generates_a_thumbnail() {
        let generated = Cell::new(false);

        let thumbnail = thumbnail_if_allowed(true, || {
            generated.set(true);
            Ok::<_, ()>(vec![1, 2, 3, 4])
        })
        .unwrap();

        assert_eq!(thumbnail, None);
        assert!(!generated.get());
    }

    #[test]
    fn ordinary_attachment_can_generate_a_thumbnail() {
        let thumbnail = thumbnail_if_allowed(false, || Ok::<_, ()>(vec![1, 2, 3, 4])).unwrap();

        assert_eq!(thumbnail, Some(vec![1, 2, 3, 4]));
    }
}
