use std::cell::RefCell;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use osl_privacy_hub::hub_command_surface::{
    direct_photo_post_after_image_quality_check, PhotoPostImageInput,
};
use sha2::{Digest, Sha256};

const POINTER: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES] = [
    0x06, 0x71, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0,
    0xf0, 0x0f, 0x1e, 0x2d,
];
const CHECK_MARK: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES] = [0xca, 0xfe, 0x67, 0x10];

#[test]
fn uploaded_hash_is_the_prepared_copy_not_the_private_original() {
    let original = fixture_png();
    let original_hash = sha256_hex(&original);
    let quality_checked_copy = RefCell::new(None::<Vec<u8>>);
    let provider_upload = RefCell::new(None::<Vec<u8>>);

    let result = direct_photo_post_after_image_quality_check(
        vec![PhotoPostImageInput {
            image_id: "task-0671-private-original".to_owned(),
            png_bytes: original.clone(),
        }],
        POINTER,
        CHECK_MARK,
        |copies| {
            assert_eq!(copies.image_copies.len(), 1);
            let prepared = copies.image_copies[0].png_bytes.clone();
            let decoded = stego::decode_png_hidden_pointer_bytes(&prepared)
                .expect("prepared PNG decodes")
                .expect("quality check reads the hidden pointer from the prepared copy");
            assert_eq!(decoded.pointer, POINTER);
            assert_eq!(decoded.check_mark, CHECK_MARK);
            quality_checked_copy.replace(Some(prepared));
            Ok(())
        },
        |copies| {
            assert_eq!(copies.image_copies.len(), 1);
            provider_upload.replace(Some(copies.image_copies[0].png_bytes.clone()));
            Ok(1)
        },
    )
    .expect("quality-approved image-hidden post reaches the provider test boundary");

    assert_eq!(result.provider_post_count, 1);
    let uploaded = provider_upload
        .into_inner()
        .expect("provider test boundary recorded one upload");
    let mut prepared = quality_checked_copy
        .into_inner()
        .expect("quality path recorded the prepared copy");

    if std::env::var_os("TASK0671_TAMPER_PREPARED").is_some() {
        prepared[0] ^= 0x01;
        println!("TASK0671_TAMPERED_PREPARED_BYTE_INDEX=0");
    }

    let prepared_hash = sha256_hex(&prepared);
    let uploaded_hash = sha256_hex(&uploaded);
    println!("TASK0671_ORIGINAL_SHA256={original_hash}");
    println!("TASK0671_PREPARED_SHA256={prepared_hash}");
    println!("TASK0671_UPLOADED_SHA256={uploaded_hash}");
    println!(
        "TASK0671_PROVIDER_POST_COUNT={}",
        result.provider_post_count
    );

    require_uploaded_prepared_hash(&uploaded, &prepared)
        .expect("uploaded image hash must equal the prepared-copy hash");
    assert_ne!(
        uploaded_hash, original_hash,
        "private original must not be uploaded"
    );

    prepared[0] ^= 0x01;
    let changed_prepared_hash = sha256_hex(&prepared);
    let mismatch = require_uploaded_prepared_hash(&uploaded, &prepared)
        .expect_err("a one-byte prepared-copy change must fail the uploaded-hash check");
    println!("TASK0671_ONE_BYTE_CHANGE_INDEX=0");
    println!("TASK0671_CHANGED_PREPARED_SHA256={changed_prepared_hash}");
    println!("TASK0671_ONE_BYTE_CHECK=FAIL:{mismatch}");
}

fn require_uploaded_prepared_hash(uploaded: &[u8], prepared: &[u8]) -> Result<(), String> {
    let uploaded_hash = sha256_hex(uploaded);
    let prepared_hash = sha256_hex(prepared);
    if uploaded_hash == prepared_hash {
        Ok(())
    } else {
        Err(format!(
            "uploaded_sha256={uploaded_hash} prepared_sha256={prepared_hash}"
        ))
    }
}

fn fixture_png() -> Vec<u8> {
    STANDARD
        .decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAwAAAAICAIAAABChommAAABM0lEQVR42gEoAdf+ABFbyxxgxCdlvTJqtj1vr0h0qFN5oV5+mmmDk3SIjH+NhYqSfgAUaMIfbbsqcrQ1d61AfKZLgZ9Whphhi5FskIp3lYOCmnyNn3UAF3W5InqyLX+rOISkQ4mdTo6WWZOPZJiIb52BeqJ6hadzkKxsABqCsCWHqTCMojuRm0aWlFGbjVyghmelf3KqeH2vcYi0apO5YwAdj6colKAzmZk+npJJo4tUqIRfrX1qsnZ1t2+AvGiLwWGWxloAIJyeK6GXNqaQQauJTLCCV7V7Yrp0bb9teMRmg8lfjs5YmdNRACOplS6ujjmzh0S4gE+9eVrCcmXHa3DMZHvRXYbWVpHbT5zgSAAmtowxu4U8wH5HxXdSynBdz2lo1GJz2Vt+3lSJ402U6Eaf7T/16JBhoW65AQAAAABJRU5ErkJggg==",
        )
        .expect("fixture PNG base64 decodes")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
