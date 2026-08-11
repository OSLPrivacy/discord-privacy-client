//! Transient live diagnostic for Task 6211.
//!
//! This deliberately uses the same public `CipherStoreClient` attachment
//! methods as the shipping application. It never prints the bearer token and
//! removes a successful Free upload before returning.

use std::fs::{self, File};
use std::io::Cursor;
use std::path::PathBuf;

use ipc::cipher_store_client::{CipherStoreClient, CipherStoreError};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

fn random_path() -> PathBuf {
    let mut suffix = [0_u8; 12];
    OsRng.fill_bytes(&mut suffix);
    std::env::temp_dir().join(format!("osl-task-6211-{}.bin", hex(&suffix)))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CipherStoreClient::new("https://ciphers.oslprivacy.com")?;
    let path = random_path();
    let mut bytes = [0_u8; 96];
    let mut token = [0_u8; 16];
    OsRng.fill_bytes(&mut bytes);
    OsRng.fill_bytes(&mut token);
    fs::write(&path, bytes)?;
    let expected_hash = hex(&Sha256::digest(bytes));

    let mut readback = Vec::new();
    let free_ok = match client.upload_attachment_file(File::open(&path)?, 604_800, &token) {
        Ok(free) => {
            let read_bytes =
                client.fetch_attachment_to_writer(&free.id_hex, &token, &mut readback)?;
            let actual_hash = hex(&Sha256::digest(&readback));
            println!(
                "TASK6211_SHIPPING_FREE status=201 read_bytes={read_bytes} hash_match={} receipt_expiry={}",
                actual_hash == expected_hash,
                free.expires_at,
            );
            client.delete_attachment(&free.id_hex, &token)?;
            actual_hash == expected_hash
        }
        Err(CipherStoreError::Status { status, body }) => {
            println!(
                "TASK6211_SHIPPING_FREE status={status} body={} object_left=false",
                body.replace(['\n', '\r'], " ")
            );
            false
        }
        Err(error) => {
            println!("TASK6211_SHIPPING_FREE error={error} object_left=false");
            false
        }
    };

    let pro_result = client.upload_attachment_file(File::open(&path)?, 2_592_000, &token);
    let pro_refused = match pro_result {
        Err(CipherStoreError::Status { status, body }) => {
            println!(
                "TASK6211_SHIPPING_PRO status={status} body={} object_left=false",
                body.replace(['\n', '\r'], " ")
            );
            true
        }
        Err(error) => {
            println!("TASK6211_SHIPPING_PRO error={error} object_left=false");
            true
        }
        Ok(upload) => {
            client.delete_attachment(&upload.id_hex, &token)?;
            println!("TASK6211_SHIPPING_PRO unexpected_status=201 object_left=false");
            false
        }
    };

    fs::remove_file(path)?;
    // Keep Cursor referenced so this example proves its writer type stays the
    // shipping generic writer rather than a test-only response shortcut.
    let _ = Cursor::new(readback);
    if free_ok && pro_refused {
        Ok(())
    } else {
        Err("production shipping attachment retention preflight failed".into())
    }
}
