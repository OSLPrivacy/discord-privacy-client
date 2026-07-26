//! Local-first encrypted collaboration protocol and product-access contract.

use crypto::aes_gcm::{self, Key, Nonce};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use zeroize::Zeroize;

const ROOM_SECRET_BYTES: usize = 32;
const ROOM_ID_BYTES: usize = 16;
const MAX_FRAME_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslLanInvitation {
    pub code: String,
    pub address: String,
    pub room_id: String,
    pub encrypted: bool,
    pub requires_cloud: bool,
    pub requires_pro: bool,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslCollaborationAccess {
    pub lan_available: bool,
    pub lan_price: &'static str,
    pub hosted_available: bool,
    pub hosted_price: &'static str,
    pub automatic_cloud_upload: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslCollaborationFrame {
    pub version: u8,
    pub room_id: String,
    pub sequence: u64,
    pub nonce: String,
    pub ciphertext: String,
}

pub fn access(pro: bool) -> OslCollaborationAccess {
    OslCollaborationAccess {
        lan_available: true,
        lan_price: "free",
        hosted_available: pro,
        hosted_price: "pro",
        automatic_cloud_upload: false,
    }
}

pub fn create_invitation(
    address: SocketAddr,
) -> Result<(OslLanInvitation, [u8; ROOM_SECRET_BYTES]), String> {
    validate_lan_address(address)?;
    let secret = random_array::<ROOM_SECRET_BYTES>();
    let room = random_array::<ROOM_ID_BYTES>();
    let room_id = hex(&room);
    let code = format!("osl-lan-v1|{address}|{room_id}|{}", hex(&secret));
    Ok((
        OslLanInvitation {
            code,
            address: address.to_string(),
            room_id,
            encrypted: true,
            requires_cloud: false,
            requires_pro: false,
        },
        secret,
    ))
}

pub fn parse_invitation(
    code: &str,
) -> Result<(SocketAddr, String, [u8; ROOM_SECRET_BYTES]), String> {
    if code.len() > 256 {
        return Err("That LAN invitation is too long".into());
    }
    let mut fields = code.split('|');
    if fields.next() != Some("osl-lan-v1") {
        return Err("That is not an OSL LAN invitation".into());
    }
    let address = fields
        .next()
        .ok_or_else(|| "That LAN invitation is incomplete".to_owned())?
        .parse::<SocketAddr>()
        .map_err(|_| "That LAN invitation address is invalid".to_owned())?;
    validate_lan_address(address)?;
    let room_id = fields
        .next()
        .ok_or_else(|| "That LAN invitation is incomplete".to_owned())?;
    if room_id.len() != ROOM_ID_BYTES * 2 || !room_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("That LAN room identifier is invalid".into());
    }
    let secret_text = fields
        .next()
        .ok_or_else(|| "That LAN invitation is incomplete".to_owned())?;
    if fields.next().is_some() {
        return Err("That LAN invitation has unexpected fields".into());
    }
    let secret = decode_array::<ROOM_SECRET_BYTES>(secret_text)?;
    Ok((address, room_id.to_ascii_lowercase(), secret))
}

pub fn seal_frame(
    room_id: &str,
    sequence: u64,
    secret: &[u8; ROOM_SECRET_BYTES],
    plaintext: &[u8],
) -> Result<OslCollaborationFrame, String> {
    if plaintext.is_empty() || plaintext.len() > MAX_FRAME_BYTES {
        return Err("That collaboration update exceeds the encrypted frame limit".into());
    }
    let key = Key::from_bytes(*secret);
    let ad = frame_ad(room_id, sequence)?;
    let (nonce, mut ciphertext) = aes_gcm::seal(&key, ad.as_bytes(), plaintext)
        .map_err(|_| "The LAN update could not be encrypted".to_owned())?;
    let result = OslCollaborationFrame {
        version: 1,
        room_id: room_id.to_owned(),
        sequence,
        nonce: hex(nonce.as_bytes()),
        ciphertext: hex(&ciphertext),
    };
    ciphertext.zeroize();
    Ok(result)
}

pub fn open_frame(
    frame: &OslCollaborationFrame,
    secret: &[u8; ROOM_SECRET_BYTES],
    expected_sequence: u64,
) -> Result<Vec<u8>, String> {
    if frame.version != 1
        || frame.sequence != expected_sequence
        || frame.ciphertext.len() > (MAX_FRAME_BYTES + aes_gcm::TAG_SIZE) * 2
    {
        return Err("That collaboration update is stale, out of order, or oversized".into());
    }
    let nonce = Nonce::from_bytes(decode_array::<{ aes_gcm::NONCE_SIZE }>(&frame.nonce)?);
    let mut ciphertext = decode_vec(&frame.ciphertext)?;
    let key = Key::from_bytes(*secret);
    let ad = frame_ad(&frame.room_id, frame.sequence)?;
    let result = aes_gcm::open(&key, &nonce, ad.as_bytes(), &ciphertext)
        .map_err(|_| "The LAN update failed authentication".to_owned());
    ciphertext.zeroize();
    result
}

fn frame_ad(room_id: &str, sequence: u64) -> Result<String, String> {
    if room_id.len() != ROOM_ID_BYTES * 2 || !room_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("That LAN room identifier is invalid".into());
    }
    Ok(format!(
        "osl-notes-lan-v1|{}|{sequence}",
        room_id.to_ascii_lowercase()
    ))
}

fn validate_lan_address(address: SocketAddr) -> Result<(), String> {
    if address.port() == 0 {
        return Err("The LAN room port is invalid".into());
    }
    let allowed = match address.ip() {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_loopback(),
    };
    if !allowed {
        return Err("OSL free collaboration invitations must use a local-network address".into());
    }
    Ok(())
}

fn random_array<const N: usize>() -> [u8; N] {
    let mut bytes = crypto::random::random_bytes(N);
    let mut result = [0u8; N];
    result.copy_from_slice(&bytes);
    bytes.zeroize();
    result
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn decode_vec(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("That encrypted LAN field is invalid".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "That encrypted LAN field is invalid".to_owned())
        })
        .collect()
}
fn decode_array<const N: usize>(value: &str) -> Result<[u8; N], String> {
    let mut bytes = decode_vec(value)?;
    if bytes.len() != N {
        bytes.zeroize();
        return Err("That encrypted LAN field has the wrong size".into());
    }
    let mut result = [0u8; N];
    result.copy_from_slice(&bytes);
    bytes.zeroize();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_is_free_and_hosted_service_requires_pro_without_implicit_uploads() {
        assert_eq!(
            access(false),
            OslCollaborationAccess {
                lan_available: true,
                lan_price: "free",
                hosted_available: false,
                hosted_price: "pro",
                automatic_cloud_upload: false
            }
        );
        assert!(access(true).hosted_available);
    }

    #[test]
    fn invitations_are_local_only_and_frames_are_authenticated_and_ordered() {
        let address = "192.168.1.12:48190".parse().unwrap();
        let (invitation, secret) = create_invitation(address).unwrap();
        let (parsed_address, room, parsed_secret) = parse_invitation(&invitation.code).unwrap();
        assert_eq!(parsed_address, address);
        assert_eq!(room, invitation.room_id);
        assert_eq!(parsed_secret, secret);
        let frame = seal_frame(&room, 1, &secret, b"private update").unwrap();
        assert_eq!(open_frame(&frame, &secret, 1).unwrap(), b"private update");
        assert!(open_frame(&frame, &secret, 2).is_err());
        assert!(parse_invitation(&invitation.code.replace("192.168.1.12", "8.8.8.8")).is_err());
    }
}
