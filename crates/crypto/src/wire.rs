//! Outer wire-format serialization for the crypto types that travel
//! between peers.
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` (the as-yet-unwritten
//! "wire-format companion doc" called out in "Remaining open items").
//! This module is the on-the-wire byte-level encoding for the three
//! Layer-3 envelope types:
//!
//! - [`encode_ratchet_message`] / [`decode_ratchet_message`] — the
//!   pairwise Double Ratchet [`crate::ratchet::EncryptedMessage`].
//! - [`encode_sender_keys_message`] / [`decode_sender_keys_message`] —
//!   the group [`crate::sender_keys::EncryptedMessage`].
//! - [`encode_initiator_handshake`] / [`decode_initiator_handshake`] —
//!   the PQXDH [`crate::pqxdh::InitiatorHandshake`].
//!
//! Each format is tagged with a 7-byte magic + 1-byte version prefix
//! so that mis-typed inputs fail fast (and so the wire format can
//! evolve). Variable-length fields are length-prefixed with `u32` BE;
//! receivers reject if the declared length runs past the end of the
//! input or if trailing bytes remain after the last field.
//!
//! ## Inner-header byte formats
//!
//! The plaintext ratchet / sender-keys headers (`Header::to_bytes` /
//! `Header::from_bytes`) live INSIDE the AEAD `enc_header` field of
//! the outer envelope. They are therefore a fixed, magic-less byte
//! layout (44 bytes for the ratchet header, 16 bytes for sender
//! keys); see [`crate::ratchet::Header`] / [`crate::sender_keys::Header`]
//! and their `HEADER_BYTES` constants. The outer envelope here only
//! carries them in opaque encrypted form.
//!
//! ## Compatibility
//!
//! The magic byte is part of the magic prefix (`...\x01`) and bumps
//! with every format-breaking change. Receivers MUST reject unknown
//! magic / version bytes — never silently fall back. This is mirrored
//! by the attachment stream header in [`crate::attachment`].

use crate::aead;
use crate::error::{Error, Result};
use crate::ml_kem_768;
use crate::pqxdh::InitiatorHandshake;
use crate::ratchet;
use crate::sender_keys;
use crate::x25519;

/// Wire-format protocol version. Embedded as the trailing byte of
/// every envelope's magic prefix. Bumps require receivers to reject
/// the older format explicitly.
pub const WIRE_VERSION_V1: u8 = 0x01;

/// Magic prefix for the pairwise ratchet message envelope.
pub const RATCHET_MAGIC: [u8; 7] = *b"DPCRDM\x01";

/// Magic prefix for the sender-keys (group) message envelope.
pub const SENDER_KEYS_MAGIC: [u8; 7] = *b"DPCSKG\x01";

/// Magic prefix for the PQXDH initiator-handshake envelope.
pub const HANDSHAKE_MAGIC: [u8; 7] = *b"DPCPQX\x01";

// ---------- shared helpers ----------

fn check_magic(bytes: &[u8], magic: &[u8; 7], what: &str) -> Result<usize> {
    if bytes.len() < magic.len() {
        return Err(Error::Internal(format!("{what}: too short for magic")));
    }
    if &bytes[..magic.len()] != magic {
        return Err(Error::Internal(format!(
            "{what}: bad magic / version byte (expected {:?}, got {:?})",
            magic,
            &bytes[..magic.len()]
        )));
    }
    Ok(magic.len())
}

fn read_u32(bytes: &[u8], off: usize, what: &str) -> Result<(u32, usize)> {
    if bytes.len() < off + 4 {
        return Err(Error::Internal(format!(
            "{what}: truncated u32 at offset {off}"
        )));
    }
    let v = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap());
    Ok((v, off + 4))
}

fn read_lp<'a>(bytes: &'a [u8], off: usize, what: &str) -> Result<(&'a [u8], usize)> {
    let (len, mut off) = read_u32(bytes, off, what)?;
    let len = len as usize;
    if bytes.len() < off + len {
        return Err(Error::Internal(format!(
            "{what}: truncated length-prefixed payload (declared {len} bytes from offset {off})"
        )));
    }
    let slice = &bytes[off..off + len];
    off += len;
    Ok((slice, off))
}

fn read_fixed<'a>(bytes: &'a [u8], off: usize, n: usize, what: &str) -> Result<(&'a [u8], usize)> {
    if bytes.len() < off + n {
        return Err(Error::Internal(format!(
            "{what}: truncated fixed {n}-byte field at offset {off}"
        )));
    }
    Ok((&bytes[off..off + n], off + n))
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

// ---------- ratchet::EncryptedMessage ----------

/// Encode a pairwise-ratchet [`ratchet::EncryptedMessage`] to wire bytes.
///
/// Layout:
///
/// ```text
/// magic                  : 7 B  ("DPCRDM\x01")
/// header_nonce           : 24 B
/// enc_header_len         : u32 BE
/// enc_header             : `enc_header_len` bytes
/// message_nonce          : 24 B
/// ciphertext_len         : u32 BE
/// ciphertext             : `ciphertext_len` bytes
/// ```
pub fn encode_ratchet_message(msg: &ratchet::EncryptedMessage) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        RATCHET_MAGIC.len()
            + aead::NONCE_SIZE
            + 4
            + msg.enc_header.len()
            + aead::NONCE_SIZE
            + 4
            + msg.ciphertext.len(),
    );
    buf.extend_from_slice(&RATCHET_MAGIC);
    buf.extend_from_slice(msg.header_nonce.as_bytes());
    write_lp(&mut buf, &msg.enc_header);
    buf.extend_from_slice(msg.message_nonce.as_bytes());
    write_lp(&mut buf, &msg.ciphertext);
    buf
}

/// Decode a pairwise-ratchet wire bundle. Errors on bad magic,
/// truncated fields, or trailing garbage.
pub fn decode_ratchet_message(bytes: &[u8]) -> Result<ratchet::EncryptedMessage> {
    let what = "wire::ratchet_message";
    let mut off = check_magic(bytes, &RATCHET_MAGIC, what)?;
    let (header_nonce_bytes, next) = read_fixed(bytes, off, aead::NONCE_SIZE, what)?;
    off = next;
    let mut header_nonce_arr = [0u8; aead::NONCE_SIZE];
    header_nonce_arr.copy_from_slice(header_nonce_bytes);

    let (enc_header_slice, next) = read_lp(bytes, off, what)?;
    off = next;
    let enc_header = enc_header_slice.to_vec();

    let (msg_nonce_bytes, next) = read_fixed(bytes, off, aead::NONCE_SIZE, what)?;
    off = next;
    let mut msg_nonce_arr = [0u8; aead::NONCE_SIZE];
    msg_nonce_arr.copy_from_slice(msg_nonce_bytes);

    let (ct_slice, next) = read_lp(bytes, off, what)?;
    off = next;
    let ciphertext = ct_slice.to_vec();

    if off != bytes.len() {
        return Err(Error::Internal(format!(
            "{what}: trailing {} bytes after last field",
            bytes.len() - off
        )));
    }

    Ok(ratchet::EncryptedMessage {
        header_nonce: aead::Nonce::from_bytes(header_nonce_arr),
        enc_header,
        message_nonce: aead::Nonce::from_bytes(msg_nonce_arr),
        ciphertext,
    })
}

// ---------- sender_keys::EncryptedMessage ----------

/// Encode a group-sender-keys [`sender_keys::EncryptedMessage`] to wire
/// bytes. Same shape as the ratchet envelope but with a distinct
/// magic so receivers cannot conflate them.
pub fn encode_sender_keys_message(msg: &sender_keys::EncryptedMessage) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        SENDER_KEYS_MAGIC.len()
            + aead::NONCE_SIZE
            + 4
            + msg.enc_header.len()
            + aead::NONCE_SIZE
            + 4
            + msg.ciphertext.len(),
    );
    buf.extend_from_slice(&SENDER_KEYS_MAGIC);
    buf.extend_from_slice(msg.header_nonce.as_bytes());
    write_lp(&mut buf, &msg.enc_header);
    buf.extend_from_slice(msg.message_nonce.as_bytes());
    write_lp(&mut buf, &msg.ciphertext);
    buf
}

/// Decode a sender-keys wire bundle.
pub fn decode_sender_keys_message(bytes: &[u8]) -> Result<sender_keys::EncryptedMessage> {
    let what = "wire::sender_keys_message";
    let mut off = check_magic(bytes, &SENDER_KEYS_MAGIC, what)?;

    let (header_nonce_bytes, next) = read_fixed(bytes, off, aead::NONCE_SIZE, what)?;
    off = next;
    let mut header_nonce_arr = [0u8; aead::NONCE_SIZE];
    header_nonce_arr.copy_from_slice(header_nonce_bytes);

    let (enc_header_slice, next) = read_lp(bytes, off, what)?;
    off = next;
    let enc_header = enc_header_slice.to_vec();

    let (msg_nonce_bytes, next) = read_fixed(bytes, off, aead::NONCE_SIZE, what)?;
    off = next;
    let mut msg_nonce_arr = [0u8; aead::NONCE_SIZE];
    msg_nonce_arr.copy_from_slice(msg_nonce_bytes);

    let (ct_slice, next) = read_lp(bytes, off, what)?;
    off = next;
    let ciphertext = ct_slice.to_vec();

    if off != bytes.len() {
        return Err(Error::Internal(format!(
            "{what}: trailing {} bytes after last field",
            bytes.len() - off
        )));
    }

    Ok(sender_keys::EncryptedMessage {
        header_nonce: aead::Nonce::from_bytes(header_nonce_arr),
        enc_header,
        message_nonce: aead::Nonce::from_bytes(msg_nonce_arr),
        ciphertext,
    })
}

// ---------- pqxdh::InitiatorHandshake ----------

const HANDSHAKE_OPK_FLAG_PRESENT: u8 = 0x01;
const HANDSHAKE_OPK_FLAG_ABSENT: u8 = 0x00;

/// Encode a PQXDH [`InitiatorHandshake`] to wire bytes.
///
/// Layout:
///
/// ```text
/// magic                : 7 B  ("DPCPQX\x01")
/// ek_x25519_pub        : 32 B  (X25519 public key, fixed)
/// mlkem_ct_len         : u32 BE  (always 1088 for ML-KEM-768)
/// mlkem_ciphertext     : `mlkem_ct_len` bytes
/// no_opk               : u8  (1 = no OPK, 0 = OPK consumed)
/// opk_id_present       : u8  (0 = absent, 1 = present)
/// opk_id               : u32 BE  (only if opk_id_present == 1)
/// ```
///
/// `no_opk` and `opk_id_present` MUST be consistent: `no_opk = 1`
/// implies `opk_id_present = 0`, and vice versa. The decoder enforces
/// this and rejects mismatches.
pub fn encode_initiator_handshake(handshake: &InitiatorHandshake) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        HANDSHAKE_MAGIC.len() + 32 + 4 + ml_kem_768::CIPHERTEXT_SIZE + 1 + 1 + 4,
    );
    buf.extend_from_slice(&HANDSHAKE_MAGIC);
    buf.extend_from_slice(handshake.ek_x25519_pub.as_bytes());
    let ct_bytes = handshake.mlkem_ciphertext.to_bytes();
    write_lp(&mut buf, &ct_bytes);
    buf.push(if handshake.no_opk { 1 } else { 0 });
    match handshake.opk_id {
        Some(id) => {
            buf.push(HANDSHAKE_OPK_FLAG_PRESENT);
            buf.extend_from_slice(&id.to_be_bytes());
        }
        None => {
            buf.push(HANDSHAKE_OPK_FLAG_ABSENT);
        }
    }
    buf
}

/// Decode a PQXDH initiator-handshake wire bundle. Validates the
/// `no_opk` / `opk_id_present` consistency, the ML-KEM ciphertext
/// length, and rejects trailing garbage.
pub fn decode_initiator_handshake(bytes: &[u8]) -> Result<InitiatorHandshake> {
    let what = "wire::initiator_handshake";
    let mut off = check_magic(bytes, &HANDSHAKE_MAGIC, what)?;

    let (ek_bytes, next) = read_fixed(bytes, off, x25519::PUBLIC_KEY_SIZE, what)?;
    off = next;
    let mut ek_arr = [0u8; x25519::PUBLIC_KEY_SIZE];
    ek_arr.copy_from_slice(ek_bytes);
    let ek_x25519_pub = x25519::PublicKey::from_bytes(ek_arr);

    let (mlkem_ct_slice, next) = read_lp(bytes, off, what)?;
    off = next;
    if mlkem_ct_slice.len() != ml_kem_768::CIPHERTEXT_SIZE {
        return Err(Error::Internal(format!(
            "{what}: mlkem_ciphertext length {} != expected {}",
            mlkem_ct_slice.len(),
            ml_kem_768::CIPHERTEXT_SIZE
        )));
    }
    let mut mlkem_ct_arr = [0u8; ml_kem_768::CIPHERTEXT_SIZE];
    mlkem_ct_arr.copy_from_slice(mlkem_ct_slice);
    let mlkem_ciphertext = ml_kem_768::Ciphertext::from_bytes(&mlkem_ct_arr);

    let (no_opk_slice, next) = read_fixed(bytes, off, 1, what)?;
    off = next;
    let no_opk = match no_opk_slice[0] {
        0 => false,
        1 => true,
        v => {
            return Err(Error::Internal(format!(
                "{what}: invalid no_opk byte {v} (must be 0 or 1)"
            )));
        }
    };

    let (opk_flag_slice, next) = read_fixed(bytes, off, 1, what)?;
    off = next;
    let opk_id = match opk_flag_slice[0] {
        HANDSHAKE_OPK_FLAG_ABSENT => None,
        HANDSHAKE_OPK_FLAG_PRESENT => {
            let (id_val, next) = read_u32(bytes, off, what)?;
            off = next;
            Some(id_val)
        }
        v => {
            return Err(Error::Internal(format!(
                "{what}: invalid opk_id flag {v} (must be 0 or 1)"
            )));
        }
    };

    if no_opk && opk_id.is_some() {
        return Err(Error::Internal(format!(
            "{what}: no_opk=true but opk_id present"
        )));
    }
    if !no_opk && opk_id.is_none() {
        return Err(Error::Internal(format!(
            "{what}: no_opk=false but opk_id absent"
        )));
    }

    if off != bytes.len() {
        return Err(Error::Internal(format!(
            "{what}: trailing {} bytes after last field",
            bytes.len() - off
        )));
    }

    Ok(InitiatorHandshake {
        ek_x25519_pub,
        mlkem_ciphertext,
        no_opk,
        opk_id,
    })
}
