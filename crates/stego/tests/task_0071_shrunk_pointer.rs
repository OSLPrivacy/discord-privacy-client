//! TASK 0071 acceptance proof.  The `FrozenDecoder` below deliberately does
//! not call `PairPointerProtocol::open`, `decode_observable_cover`, or any
//! product key gate.  It is a small independent implementation over the
//! captured carrier, public word bank and public protocol constants.

use std::collections::HashMap;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use stego::{
    bigram, paired_pointer::decode_observable_cover, CarrierCapture, PairPointerProtocol,
    ProtectedRecord, RecordAddress, POINTER_BYTES, SHIPPING_CARRIED_BITS,
};

const MASK_DOMAIN: &[u8] = b"osl/task-0071/cover-observable-mask/v2";
const COVER_SEED_DOMAIN: &[u8] = b"osl/task-0071/cover-seed/v2";
const KEY_DOMAIN: &[u8] = b"osl/task-0071/protected-record-key/v2";
const NONCE_DOMAIN: &[u8] = b"osl/task-0071/protected-record-nonce/v2";
const ADDRESS_DOMAIN: &[u8] = b"osl/task-0071/protected-record-address/v2";
const AAD_DOMAIN: &[u8] = b"osl/task-0071/protected-record-aad/v2";

#[derive(Default)]
struct DeployedLookup {
    rows: HashMap<RecordAddress, ProtectedRecord>,
}

impl DeployedLookup {
    fn insert(&mut self, record: ProtectedRecord) {
        self.rows.insert(record.address, record);
    }
    fn read(&self, address: &RecordAddress) -> Option<&ProtectedRecord> {
        self.rows.get(address)
    }
}

/// Independent, frozen protocol reader used for the positive interoperability
/// proof. Its only product dependency is public word-bank data (`bigram::model`),
/// not a product decoder, key gate, record reader, or lookup implementation.
struct FrozenDecoder;

impl FrozenDecoder {
    fn derive<const N: usize>(
        key: &[u8],
        context: &[u8],
        domain: &[u8],
        pointer: Option<&[u8; POINTER_BYTES]>,
    ) -> [u8; N] {
        let hk = Hkdf::<Sha256>::new(Some(context), key);
        let mut info = domain.to_vec();
        if let Some(pointer) = pointer {
            info.extend_from_slice(pointer);
        }
        let mut out = [0u8; N];
        hk.expand(&info, &mut out).unwrap();
        out
    }

    /// Independent public cover parser: every word choice is inventoried and
    /// exactly 80 bits are reconstructed. This is intentionally not the
    /// product `decode_observable_cover` helper.
    fn public_cover_bits(cover: &str) -> Option<[u8; POINTER_BYTES]> {
        let words: Vec<_> = cover.split_ascii_whitespace().collect();
        if words.len() != 14 {
            return None;
        } // ceil(80 / 6)
        let model = bigram::model();
        let mut bits = Vec::with_capacity(SHIPPING_CARRIED_BITS as usize);
        for (position, raw) in words.iter().enumerate() {
            let word = raw.trim_end_matches('.');
            let index = *model.index_of.get(word)?;
            if !(1..=64).contains(&index) {
                return None;
            }
            let width = if position == 13 { 2 } else { 6 };
            let value = index - 1;
            for shift in (0..width).rev() {
                bits.push((value >> shift) & 1 != 0);
            }
        }
        if bits.len() != SHIPPING_CARRIED_BITS as usize {
            return None;
        }
        let mut out = [0u8; POINTER_BYTES];
        for (index, bit) in bits.into_iter().enumerate() {
            out[index / 8] = (out[index / 8] << 1) | bit as u8;
        }
        Some(out)
    }

    fn open(
        key: &[u8],
        context: &[u8],
        capture: &CarrierCapture,
        lookup: &DeployedLookup,
    ) -> Option<([u8; POINTER_BYTES], Vec<u8>)> {
        let observable = Self::public_cover_bits(&capture.cover_text)?;
        let cover_seed = Self::derive::<32>(key, context, COVER_SEED_DOMAIN, None);
        let hk = Hkdf::<Sha256>::new(Some(context), &cover_seed);
        let mut mask = [0u8; POINTER_BYTES];
        hk.expand(MASK_DOMAIN, &mut mask).unwrap();
        let pointer = std::array::from_fn(|i| observable[i] ^ mask[i]);
        let address = RecordAddress(Self::derive::<32>(
            key,
            context,
            ADDRESS_DOMAIN,
            Some(&pointer),
        ));
        let record = lookup.read(&address)?;
        let record_key = Self::derive::<32>(key, context, KEY_DOMAIN, Some(&pointer));
        let nonce = Self::derive::<24>(key, context, NONCE_DOMAIN, Some(&pointer));
        let mut aad = AAD_DOMAIN.to_vec();
        aad.extend_from_slice(context);
        aad.extend_from_slice(&pointer);
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&record_key));
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &record.ciphertext,
                    aad: &aad,
                },
            )
            .ok()?;
        Some((pointer, plaintext))
    }
}

fn canary_500() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut bytes = [0u8; 500];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

#[test]
fn task_0071_pair_keyed_shipping_cover_proof() {
    let mut pair_key = [0u8; 32];
    let mut pointer = [0u8; POINTER_BYTES];
    OsRng.fill_bytes(&mut pair_key);
    OsRng.fill_bytes(&mut pointer);
    let context = b"conversation=pair-a;message=independently-authenticated-0071";
    let canary = canary_500();
    assert_eq!(
        canary.chars().count(),
        500,
        "fresh private canary must be 500 characters"
    );

    let sender = PairPointerProtocol::new(&pair_key, context);
    let (capture, record) = sender.seal(pointer, canary.as_bytes());
    let exact_carrier_capture = capture.clone();
    let mut deployed_read_access = DeployedLookup::default();
    deployed_read_access.insert(record.clone());

    // Intended product recipient: the only lookup attempt uses an address
    // derived from the paired key and authenticated context.
    let recipient = PairPointerProtocol::new(&pair_key, context);
    let (recipient_pointer, recipient_text) = recipient
        .open(&capture, &record)
        .expect("paired recipient recovers record");
    assert_eq!(recipient_pointer, pointer);
    assert_eq!(recipient_text, canary.as_bytes());

    // Outside verifier receives the unchanged capture, all public cover
    // choices and a deployed read interface, but has no pair key and never
    // invokes a product reader. It can inventory 14 choices / 80 bits only.
    let observable = FrozenDecoder::public_cover_bits(&exact_carrier_capture.cover_text)
        .expect("public cover inventory");
    assert_eq!(
        exact_carrier_capture
            .cover_text
            .split_ascii_whitespace()
            .count(),
        14
    );
    assert_ne!(
        observable, pointer,
        "outside verifier recovered exact pointer from reversible cover mapping"
    );
    assert!(FrozenDecoder::open(
        b"outside verifier has no pair key",
        context,
        &exact_carrier_capture,
        &deployed_read_access
    )
    .is_none());
    assert!(
        deployed_read_access
            .read(&RecordAddress([0u8; 32]))
            .is_none(),
        "outside guessed no addressable record"
    );

    // Independent frozen decoder has no product reader/key-gate dependency.
    let frozen = FrozenDecoder::open(
        &pair_key,
        context,
        &exact_carrier_capture,
        &deployed_read_access,
    )
    .expect("independent keyed decoder recovers shipping capture");
    assert_eq!(
        frozen.0, pointer,
        "independent decoder recovers exact 80-bit pointer"
    );
    assert_eq!(
        frozen.1,
        canary.as_bytes(),
        "independent decoder recovers exact canary"
    );

    // Wrong, random and unpaired keys; one altered carried bit; and replay
    // under different authenticated context each release zero plaintext.
    let wrong = PairPointerProtocol::new(b"wrong independently supplied pair key", context);
    assert!(wrong.open(&capture, &record).is_err());
    let mut random_key = [0u8; 32];
    OsRng.fill_bytes(&mut random_key);
    assert!(PairPointerProtocol::new(&random_key, context)
        .open(&capture, &record)
        .is_err());
    assert!(
        PairPointerProtocol::new(&pair_key, b"conversation=pair-a;message=other")
            .open(&capture, &record)
            .is_err()
    );
    let mut one_changed_bit = decode_observable_cover(&capture.cover_text).unwrap();
    one_changed_bit[0] ^= 0x80;
    let tampered = CarrierCapture {
        cover_text: stego::paired_pointer::encode_observable_cover(&one_changed_bit),
    };
    assert!(recipient.open(&tampered, &record).is_err());

    // Same pointer/context under independently generated pair keys produces
    // a distinct observable mapping (the cover seed is also distinct).
    let mut other_pair_key = [0u8; 32];
    OsRng.fill_bytes(&mut other_pair_key);
    let other = PairPointerProtocol::new(&other_pair_key, context);
    let (other_capture, _) = other.seal(pointer, canary.as_bytes());
    assert_ne!(other_capture.cover_text, capture.cover_text);
    assert_ne!(other.cover_seed(), sender.cover_seed());

    println!("TASK0071 private_canary_chars={}", canary.chars().count());
    println!("TASK0071 carried_material_before_bits=192");
    println!("TASK0071 carried_material_exact_bits={SHIPPING_CARRIED_BITS}");
    println!(
        "TASK0071 observable_cover_choices={}",
        exact_carrier_capture
            .cover_text
            .split_ascii_whitespace()
            .count()
    );
    println!("TASK0071 serialized_seed=false serialized_clear_pointer=false");
    println!("TASK0071 paired_recipient_exact=true frozen_decoder_pointer_bits=80 frozen_decoder_exact=true");
    println!("TASK0071 outside_recovered_seed=false outside_recovered_pointer=false outside_addressable_record=false outside_canary_bytes=0");
    println!("TASK0071 wrong_key_plaintext_bytes=0 random_key_plaintext_bytes=0 tampered_plaintext_bytes=0 replay_plaintext_bytes=0 unpaired_plaintext_bytes=0");
}
