//! Pointer read-back at the shipping mailbox boundary.
//!
//! A mail provider returns rendered text, not the exact text the composer
//! submitted.  In particular Gmail can quote a reply, wrap its lines, and add
//! a signature.  This boundary tries the provider text as returned and then a
//! conservative Gmail rendering normalisation; it never treats arbitrary text
//! as a pointer.

use std::collections::BTreeSet;

use ipc::prose_token::{prose_token_recover_pointer, ProseTokenError};
use ipc::scope::ScopeInput;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShippingMailboxText {
    /// Gmail's immutable provider message id, never an OSL fixture id.
    pub server_id: String,
    /// The body exactly as the mailbox reader returned it.
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShippingMailboxPointer {
    pub server_id: String,
    pub blob_id: String,
}

/// Decode pointers from the text the shipping mailbox reader saw.
///
/// The function has no mailbox fixture, cache, or send capability.  It is the
/// one call every provider reader makes after it has received an immutable
/// provider server id and its rendered text.
pub fn read_shipping_mailbox_pointers(
    messages: &[ShippingMailboxText],
    scope: &ScopeInput,
    detection_key: &[u8; 32],
) -> Result<Vec<ShippingMailboxPointer>, ProseTokenError> {
    let mut ids = BTreeSet::new();
    let mut pointers = Vec::new();
    for message in messages {
        if message.server_id.trim().is_empty() || !ids.insert(message.server_id.clone()) {
            // A malformed provider row cannot be matched safely.  Treat it as
            // ordinary text rather than guessing an identity.
            continue;
        }
        let mut pointer = None;
        for candidate in gmail_rendered_text_candidates(&message.text) {
            if let Some(recovered) = prose_token_recover_pointer(scope, detection_key, &candidate)?
            {
                pointer = Some(recovered);
                break;
            }
        }
        if let Some(pointer) = pointer {
            pointers.push(ShippingMailboxPointer {
                server_id: message.server_id.clone(),
                blob_id: pointer.blob_id,
            });
        }
    }
    Ok(pointers)
}

/// Return the unmodified value first, then the exact text visible inside a
/// Gmail-style reply quote.  It removes only quote markers, the reply header,
/// and a conventional signature delimiter; all other words remain and must
/// pass the authenticated prose-token detector.
fn gmail_rendered_text_candidates(text: &str) -> Vec<String> {
    let mut candidates = vec![text.to_owned()];
    let mut lines = Vec::new();
    let mut in_quote = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.starts_with("On ") && line.ends_with(" wrote:") {
            in_quote = true;
            continue;
        }
        if !in_quote {
            continue;
        }
        let Some(unquoted) = line.strip_prefix('>') else {
            continue;
        };
        let unquoted = unquoted.strip_prefix(' ').unwrap_or(unquoted);
        if unquoted.trim() == "--" {
            break;
        }
        if !unquoted.trim().is_empty() {
            lines.push(unquoted.trim());
        }
    }
    if !lines.is_empty() {
        candidates.push(lines.join(" "));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::gmail_rendered_text_candidates;

    #[test]
    fn normalisation_keeps_only_the_quoted_body() {
        assert_eq!(
            gmail_rendered_text_candidates(
                "Hello\n\nOn Tue wrote:\n> first\n> second\n> --\n> signature"
            ),
            vec![
                "Hello\n\nOn Tue wrote:\n> first\n> second\n> --\n> signature".to_owned(),
                "first second".to_owned(),
            ]
        );
    }
}
