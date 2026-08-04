use cover_ai::pool::{CoverContextVersion, PoolEntry};
use cover_ai::pool_at_rest::{open, seal, PoolAead, SealedPoolEntry};

struct Xor;
impl PoolAead for Xor {
    type Error = ();
    fn seal(&self, _aad: &[u8], plaintext: &[u8]) -> Result<SealedPoolEntry, ()> {
        Ok(SealedPoolEntry {
            nonce: vec![1],
            ciphertext: plaintext.iter().map(|v| v ^ 0xa5).collect(),
        })
    }
    fn open(&self, _aad: &[u8], sealed: &SealedPoolEntry) -> Result<Vec<u8>, ()> {
        Ok(sealed.ciphertext.iter().map(|v| v ^ 0xa5).collect())
    }
}

#[test]
fn t13_tl4_pool_capabilities_are_aead_sealed_not_plaintext() {
    let secret = b"unused-bearer-capability".to_vec();
    let entry = PoolEntry::new(
        secret.clone(),
        "ordinary cover".into(),
        CoverContextVersion([3; 32]),
        9,
    )
    .unwrap();
    let sealed = seal(&Xor, &entry).unwrap();
    assert!(!sealed
        .ciphertext
        .windows(secret.len())
        .any(|part| part == secret.as_slice()));
    assert_eq!(open(&Xor, &sealed).unwrap().into_capability(), secret);
}
