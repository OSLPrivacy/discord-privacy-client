//! Fixed-path, public-only identity pairing for the dedicated two-VM WhatsApp QA lane.
//!
//! The controller may exchange only these signed public offers. Private keys,
//! storage keys, recovery phrases, provider sessions, and renderer input never
//! cross this boundary.

use crate::core_bridge::HubCoreState;
use crate::security::{self, HubSecurityState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const PUBLIC_OFFER_FILENAME: &str = "whatsapp-qa-offer.v1.json";
pub const PEER_OFFER_FILENAME: &str = "whatsapp-qa-peer-offer.v1.json";
pub const PAIRING_STATUS_FILENAME: &str = "whatsapp-qa-pairing-status.v1.json";
const VERSION: u32 = 1;
const MAX_OFFER_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicOffer {
    version: u32,
    friend_code: String,
    osl_user_id: String,
    safety_number: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairingStatus {
    version: u32,
    peer_person_id: String,
    peer_osl_user_id: String,
    peer_safety_number: String,
    peer_offer_sha256: String,
    verified: bool,
}

pub fn publish_and_consume(
    account_dir: &Path,
    core: &HubCoreState,
    security_state: &HubSecurityState,
) -> Result<(), String> {
    let exported = security::export_friend_code(core)?;
    let offer = PublicOffer {
        version: VERSION,
        friend_code: exported.friend_code,
        osl_user_id: exported.osl_user_id,
        safety_number: exported.safety_number,
    };
    let encoded = encode_offer(&offer)?;
    crate::atomic_file::write_recoverable(
        &account_dir.join(PUBLIC_OFFER_FILENAME),
        &encoded,
        "WhatsApp QA public offer",
    )?;

    let Some(peer_bytes) = crate::atomic_file::read_recoverable_bounded(
        &account_dir.join(PEER_OFFER_FILENAME),
        MAX_OFFER_BYTES,
        "WhatsApp QA peer offer",
    )?
    else {
        return Ok(());
    };
    let peer = decode_offer(&peer_bytes)?;
    let added = security::add_friend_code(
        core,
        security_state,
        peer.friend_code,
        Some("WhatsApp QA peer".to_owned()),
    )?;
    if added.osl_user_id != peer.osl_user_id || added.safety_number != peer.safety_number {
        return Err("WhatsApp QA signed peer offer metadata mismatch".to_owned());
    }
    let verified = security::verify_friend_safety_number(
        core,
        security_state,
        added.person_id,
        added.safety_number,
    )?;
    if !verified.safety_number_verified || verified.pending_key_change {
        return Err("WhatsApp QA peer keys were not stably verified".to_owned());
    }
    let status = PairingStatus {
        version: VERSION,
        peer_person_id: verified.person_id,
        peer_osl_user_id: verified.osl_user_id,
        peer_safety_number: verified.safety_number,
        peer_offer_sha256: Sha256::digest(&peer_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        verified: true,
    };
    let status_bytes = serde_json::to_vec(&status)
        .map_err(|_| "WhatsApp QA pairing status could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(
        &account_dir.join(PAIRING_STATUS_FILENAME),
        &status_bytes,
        "WhatsApp QA pairing status",
    )
}

fn encode_offer(offer: &PublicOffer) -> Result<Vec<u8>, String> {
    validate_offer(offer)?;
    let bytes = serde_json::to_vec(offer)
        .map_err(|_| "WhatsApp QA public offer could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_OFFER_BYTES {
        return Err("WhatsApp QA public offer exceeds its bound".to_owned());
    }
    Ok(bytes)
}

fn decode_offer(bytes: &[u8]) -> Result<PublicOffer, String> {
    if bytes.len() as u64 > MAX_OFFER_BYTES {
        return Err("WhatsApp QA peer offer exceeds its bound".to_owned());
    }
    let offer = serde_json::from_slice(bytes)
        .map_err(|_| "WhatsApp QA peer offer is invalid".to_owned())?;
    validate_offer(&offer)?;
    Ok(offer)
}

fn validate_offer(offer: &PublicOffer) -> Result<(), String> {
    if offer.version != VERSION
        || !(16..=8 * 1024).contains(&offer.friend_code.len())
        || !offer.friend_code.starts_with("OSLFR1.")
        || offer.osl_user_id.is_empty()
        || offer.osl_user_id.len() > 160
        || offer.safety_number.is_empty()
        || offer.safety_number.len() > 160
        || offer
            .friend_code
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')))
        || offer.osl_user_id.chars().any(char::is_control)
        || offer.safety_number.chars().any(char::is_control)
    {
        return Err("WhatsApp QA public offer is invalid".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> PublicOffer {
        PublicOffer {
            version: 1,
            friend_code: format!("OSLFR1.{}", "a".repeat(32)),
            osl_user_id: "osl:test-public-id".to_owned(),
            safety_number: "1234 5678".to_owned(),
        }
    }

    #[test]
    fn public_offer_is_bounded_strict_and_round_trips() {
        let bytes = encode_offer(&offer()).unwrap();
        assert_eq!(decode_offer(&bytes).unwrap(), offer());
        assert!(decode_offer(br#"{"version":1,"friend_code":"OSLFR1.aaaaaaaaaaaaaaaa","osl_user_id":"x","safety_number":"y","extra":true}"#).is_err());
        assert!(decode_offer(&vec![b'x'; MAX_OFFER_BYTES as usize + 1]).is_err());
    }
}
