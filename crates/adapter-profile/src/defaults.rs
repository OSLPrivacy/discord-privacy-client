//! Reviewed built-in adapter profiles.
//!
//! Defaults are signed payloads only. They do not carry executable code,
//! credentials, account identifiers, handles, private Signal state, or a
//! permission shortcut around consent, binding, and release authority.

use crate::schema::{SignedProfileDoc, PROFILE_DOC_ENVELOPE_VERSION};

const SIGNAL_DEFAULT_SIGNING_KEY_B64: &str = "Qd4FjrF753bcrQTa6u93vhdOGfOJVXnwRj5QgCfK0sk=";
const SIGNAL_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJzaWduYWwuZGVza3RvcC5uYXRpdmUiLCJhcHAiOnsic3RhYmxlX2lkIjoic2lnbmFsIiwiZGlzcGxheV9uYW1lIjoiU2lnbmFsIiwic2VydmljZV9mYW1pbHkiOiJtZXNzYWdpbmciLCJtaW5fYXBwX3ZlcnNpb24iOm51bGx9LCJyZXZpc2lvbiI6eyJudW1iZXIiOjEsImxhYmVsIjoiMjAyNi0wNy0zMC1zaWduYWwtbmF0aXZlLXN1cHBvcnRlZC12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1MzY5NjAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoid2luZG93IiwibmFtZSI6IlNpZ25hbCIsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29udmVyc2F0aW9uX3RpdGxlIiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9saXN0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9yb3ciLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJyb3ciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZWRpdGFibGVfdGV4dCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoic2VudF9zdGF0ZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJ0ZXh0X2FuY2hvciIsInN0YXJ0c193aXRoIjoiT1NMOiJ9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJTaWduYWwiLCJtYXhfYWdlX3NlY29uZHMiOjM2MDB9fQ==";
const SIGNAL_DEFAULT_SIGNATURE_B64: &str =
    "qRjgUI9+b5eLgXQHCTKdkAq8wOC4TXbjNwa8XzSkLQexD6uhAIUTVX/nkrgl6uDsM0DHk/HgxvANZZUYs2zwCw==";
const WHATSAPP_DEFAULT_SIGNING_KEY_B64: &str = "06S45CxgYCHuFh3n61e9F1l4sAzfQ4BiauVBIOWFUoU=";
const WHATSAPP_DEFAULT_PAYLOAD_B64: &str = "eyJkb21haW4iOiJvc2wvYWRhcHRlci1wcm9maWxlL3YxIiwic2NoZW1hX3ZlcnNpb24iOjEsImFkYXB0ZXJfaWQiOiJ3aGF0c2FwcC53aW5kb3dzLm5hdGl2ZSIsImFwcCI6eyJzdGFibGVfaWQiOiJ3aGF0c2FwcCIsImRpc3BsYXlfbmFtZSI6IldoYXRzQXBwIiwic2VydmljZV9mYW1pbHkiOiJtZXNzYWdpbmciLCJtaW5fYXBwX3ZlcnNpb24iOm51bGx9LCJyZXZpc2lvbiI6eyJudW1iZXIiOjEsImxhYmVsIjoiMjAyNi0wNy0zMC13aGF0c2FwcC13aW5kb3dzLXN1cHBvcnRlZC12MSJ9LCJpc3N1ZWRfYXRfdW5peF9zZWNvbmRzIjoxNzg1MzY5NjAwLCJleHBpcmVzX2F0X3VuaXhfc2Vjb25kcyI6MTkyNDk5MjAwMCwic3VwcG9ydCI6InN1cHBvcnRlZCIsImF1dGhvcml0eSI6eyJ1c2VyX2NvbnNlbnRfcmVxdWlyZWQiOnRydWUsImFjY291bnRfYmluZGluZ19yZXF1aXJlZCI6dHJ1ZSwicmVsZWFzZV9hdXRob3JpdHlfcmVxdWlyZWQiOnRydWUsImhhcm1sZXNzX2NhbmFyeV9yZXF1aXJlZCI6dHJ1ZX0sInNlbGVjdG9ycyI6W3sia2luZCI6ImFwcF9yb290Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoid2luZG93IiwibmFtZSI6IldoYXRzQXBwIiwiYXV0b21hdGlvbl9pZCI6bnVsbH0sInJlcXVpcmVkIjp0cnVlfSx7ImtpbmQiOiJhY2NvdW50X2JhZGdlIiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoiY29udmVyc2F0aW9uX3RpdGxlIiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoidGV4dCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9saXN0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoibGlzdCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoibWVzc2FnZV9yb3ciLCJzdHJhdGVneSI6eyJraW5kIjoiYWNjZXNzaWJpbGl0eSIsInJvbGUiOiJyb3ciLCJuYW1lIjpudWxsLCJhdXRvbWF0aW9uX2lkIjpudWxsfSwicmVxdWlyZWQiOnRydWV9LHsia2luZCI6ImNvbXBvc2VyX2lucHV0Iiwic3RyYXRlZ3kiOnsia2luZCI6ImFjY2Vzc2liaWxpdHkiLCJyb2xlIjoiZWRpdGFibGVfdGV4dCIsIm5hbWUiOm51bGwsImF1dG9tYXRpb25faWQiOm51bGx9LCJyZXF1aXJlZCI6dHJ1ZX0seyJraW5kIjoic2VudF9zdGF0ZSIsInN0cmF0ZWd5Ijp7ImtpbmQiOiJ0ZXh0X2FuY2hvciIsInN0YXJ0c193aXRoIjoiT1NMOiJ9LCJyZXF1aXJlZCI6dHJ1ZX1dLCJmYWxsYmFja3MiOltdLCJjYW5hcnkiOnsic2VsZWN0b3IiOiJhcHBfcm9vdCIsImV4cGVjdGVkX3RleHQiOiJXaGF0c0FwcCIsIm1heF9hZ2Vfc2Vjb25kcyI6MzYwMH19";
const WHATSAPP_DEFAULT_SIGNATURE_B64: &str =
    "KDSnlXnW1jQuLLEBOsZSsZsYxodvLTTRb2KuyTEzPPnADSwMKO0cic7GTLxLlY2ylZgEExalQO272PLeHl4NBw==";

/// A signed, data-only Signal Desktop native support profile.
pub fn signal_default_profile() -> SignedProfileDoc {
    SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: SIGNAL_DEFAULT_PAYLOAD_B64.to_owned(),
        signature_b64: SIGNAL_DEFAULT_SIGNATURE_B64.to_owned(),
        signing_key_b64: SIGNAL_DEFAULT_SIGNING_KEY_B64.to_owned(),
    }
}

/// Public trust anchor for the built-in Signal support profile.
pub fn signal_default_trusted_signing_key_b64() -> &'static str {
    SIGNAL_DEFAULT_SIGNING_KEY_B64
}

/// A signed, data-only WhatsApp for Windows native support profile.
pub fn whatsapp_default_profile() -> SignedProfileDoc {
    SignedProfileDoc {
        envelope_version: PROFILE_DOC_ENVELOPE_VERSION,
        payload_b64: WHATSAPP_DEFAULT_PAYLOAD_B64.to_owned(),
        signature_b64: WHATSAPP_DEFAULT_SIGNATURE_B64.to_owned(),
        signing_key_b64: WHATSAPP_DEFAULT_SIGNING_KEY_B64.to_owned(),
    }
}

/// Public trust anchor for the built-in WhatsApp support profile.
pub fn whatsapp_default_trusted_signing_key_b64() -> &'static str {
    WHATSAPP_DEFAULT_SIGNING_KEY_B64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        verify_profile_doc, ProfileError, SelectorKind, SelectorStrategy, SupportLevel,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    const SIGNAL_PROFILE_TEST_NOW: u64 = 1_800_000_000;

    #[test]
    fn signal_default_profile() {
        let doc = super::signal_default_profile();
        assert_eq!(doc.envelope_version, PROFILE_DOC_ENVELOPE_VERSION);
        assert_eq!(
            doc.signing_key_b64,
            signal_default_trusted_signing_key_b64()
        );

        let payload = verify_profile_doc(
            &doc,
            signal_default_trusted_signing_key_b64(),
            SIGNAL_PROFILE_TEST_NOW,
        )
        .expect("default Signal profile must verify against its trust anchor");
        payload
            .validate_for_use(SIGNAL_PROFILE_TEST_NOW)
            .expect("default Signal profile must be usable");

        assert_eq!(payload.adapter_id, "signal.desktop.native");
        assert_eq!(payload.app.stable_id, "signal");
        assert_eq!(payload.app.display_name, "Signal");
        assert_eq!(payload.support, SupportLevel::Supported);
        assert!(payload.authority.user_consent_required);
        assert!(payload.authority.account_binding_required);
        assert!(payload.authority.release_authority_required);
        assert!(payload.authority.harmless_canary_required);

        let required = payload
            .selectors
            .iter()
            .filter(|selector| selector.required)
            .map(|selector| selector.kind)
            .collect::<Vec<_>>();
        assert!(required.contains(&SelectorKind::MessageRow));
        assert!(required.contains(&SelectorKind::ComposerInput));
        assert!(required.contains(&SelectorKind::SentState));
        assert!(payload.fallbacks.is_empty());
        assert_eq!(payload.canary.selector, SelectorKind::AppRoot);
        assert_eq!(payload.canary.expected_text, "Signal");
        assert!(payload.selectors.iter().any(|selector| matches!(
            &selector.strategy,
            SelectorStrategy::TextAnchor { starts_with } if starts_with == "OSL:"
        )));

        let mut tampered = doc.clone();
        tampered.signature_b64 = STANDARD.encode([0u8; 64]);
        assert_eq!(
            verify_profile_doc(
                &tampered,
                signal_default_trusted_signing_key_b64(),
                SIGNAL_PROFILE_TEST_NOW,
            )
            .unwrap_err(),
            ProfileError::BadSignature
        );

        let mut wrong_key = doc;
        wrong_key.signing_key_b64 = STANDARD.encode([7u8; 32]);
        assert_eq!(
            verify_profile_doc(
                &wrong_key,
                signal_default_trusted_signing_key_b64(),
                SIGNAL_PROFILE_TEST_NOW,
            )
            .unwrap_err(),
            ProfileError::SigningKeyMismatch
        );
    }

    #[test]
    fn whatsapp_default_profile() {
        let doc = super::whatsapp_default_profile();
        assert_eq!(doc.envelope_version, PROFILE_DOC_ENVELOPE_VERSION);
        assert_eq!(
            doc.signing_key_b64,
            whatsapp_default_trusted_signing_key_b64()
        );

        let payload = verify_profile_doc(
            &doc,
            whatsapp_default_trusted_signing_key_b64(),
            SIGNAL_PROFILE_TEST_NOW,
        )
        .expect("default WhatsApp profile must verify against its trust anchor");
        payload
            .validate_for_use(SIGNAL_PROFILE_TEST_NOW)
            .expect("default WhatsApp profile must be usable");

        assert_eq!(payload.adapter_id, "whatsapp.windows.native");
        assert_eq!(payload.app.stable_id, "whatsapp");
        assert_eq!(payload.app.display_name, "WhatsApp");
        assert_eq!(payload.support, SupportLevel::Supported);
        assert!(payload.authority.user_consent_required);
        assert!(payload.authority.account_binding_required);
        assert!(payload.authority.release_authority_required);
        assert!(payload.authority.harmless_canary_required);

        let required = payload
            .selectors
            .iter()
            .filter(|selector| selector.required)
            .map(|selector| selector.kind)
            .collect::<Vec<_>>();
        assert!(required.contains(&SelectorKind::AccountBadge));
        assert!(required.contains(&SelectorKind::ConversationTitle));
        assert!(required.contains(&SelectorKind::MessageRow));
        assert!(required.contains(&SelectorKind::ComposerInput));
        assert!(required.contains(&SelectorKind::SentState));
        assert!(payload.fallbacks.is_empty());
        assert_eq!(payload.canary.selector, SelectorKind::AppRoot);
        assert_eq!(payload.canary.expected_text, "WhatsApp");
        assert!(payload.selectors.iter().any(|selector| matches!(
            &selector.strategy,
            SelectorStrategy::Accessibility { role, name, .. }
                if role == "window" && name.as_deref() == Some("WhatsApp")
        )));

        let mut tampered = doc.clone();
        tampered.signature_b64 = STANDARD.encode([0u8; 64]);
        assert_eq!(
            verify_profile_doc(
                &tampered,
                whatsapp_default_trusted_signing_key_b64(),
                SIGNAL_PROFILE_TEST_NOW,
            )
            .unwrap_err(),
            ProfileError::BadSignature
        );

        let mut wrong_key = doc;
        wrong_key.signing_key_b64 = STANDARD.encode([7u8; 32]);
        assert_eq!(
            verify_profile_doc(
                &wrong_key,
                whatsapp_default_trusted_signing_key_b64(),
                SIGNAL_PROFILE_TEST_NOW,
            )
            .unwrap_err(),
            ProfileError::SigningKeyMismatch
        );
    }
}
