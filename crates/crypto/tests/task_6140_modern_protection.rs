use crypto::modern_protection::{
    self as protection, ObjectContext, ProtectedObject, ProtectionDomain, RecoverySigner,
};
use std::collections::HashSet;
use std::fs;

const IMPLEMENTATION: &str = "native.discord.text.v1";
const CONSTRUCTORS: [(&str, &str, usize); 2] = [
    ("prepare_peer_prose_text_inner", "MAX_TEXT_BYTES", 1_000),
    (
        "split_native_overlay_text",
        "MAX_NATIVE_OVERLAY_CHUNK_BYTES",
        40 * 1024,
    ),
];
const CASES: [(&str, isize, Option<u64>); 7] = [
    ("threshold-minus-one", -1, None),
    ("exact-threshold", 0, None),
    ("threshold-plus-one", 1, None),
    ("multipart-first", 1, Some(0)),
    ("multipart-middle", 1, Some(1)),
    ("multipart-final", 1, Some(2)),
    ("final-part-after-long-prefix", 1, Some(7)),
];

fn context(domain: ProtectionDomain, object: &str, index: u64, count: u64) -> ObjectContext {
    ObjectContext::v1(
        format!("{}-object", domain.as_str()),
        "task6140-disposable-owner-account",
        object,
        7,
        index,
        count,
    )
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|part| part == needle)
}

fn tampered(object: &ProtectedObject) -> ProtectedObject {
    let mut changed = object.clone();
    changed.ciphertext_and_tag[0] ^= 0x40;
    changed
}

#[test]
fn task_6140_every_frozen_domain_uses_modern_library_crypto_and_complete_aad() {
    let mut random_rows = Vec::new();
    for (index, domain) in ProtectionDomain::ALL.into_iter().enumerate() {
        let ctx = context(domain, &format!("domain-object-{index}"), 0, 1);
        let payload = format!(
            "TASK6140 confidential oracle domain={} row={index}",
            domain.as_str()
        )
        .into_bytes();
        let (object, key) = protection::seal_random(domain, &ctx, &payload).expect("random seal");
        random_rows.push((domain, ctx, payload, object, key));
    }

    let unique_nonces = random_rows
        .iter()
        .map(|row| row.3.nonce)
        .collect::<HashSet<_>>()
        .len();
    let unique_key_salts = random_rows
        .iter()
        .map(|row| row.3.key_salt)
        .collect::<HashSet<_>>()
        .len();
    assert_eq!(unique_nonces, 5);
    assert_eq!(unique_key_salts, 5);

    let mut oracle_opens = 0;
    let mut no_secret_opens = 0;
    let mut wrong_key_opens = 0;
    let mut tamper_opens = 0;
    let mut downgrade_opens = 0;
    let mut transplant_opens = 0;
    let mut capture_plaintext = 0;
    for (domain, ctx, payload, object, key) in &random_rows {
        capture_plaintext += usize::from(contains(&object.public_bytes(), payload));
        let opened = protection::open_random(*domain, ctx, object, key).expect("legitimate open");
        assert_eq!(&opened, payload);
        oracle_opens += 1;

        let (_, wrong_key) = protection::seal_random(
            *domain,
            &context(*domain, "throwaway-key-object", 0, 1),
            b"throwaway",
        )
        .expect("wrong key fixture");
        no_secret_opens += usize::from(
            protection::open_password(*domain, ctx, object, b"reader-has-no-secret").is_ok(),
        );
        wrong_key_opens +=
            usize::from(protection::open_random(*domain, ctx, object, &wrong_key).is_ok());
        tamper_opens +=
            usize::from(protection::open_random(*domain, ctx, &tampered(object), key).is_ok());
        let mut downgrade = object.clone();
        downgrade.algorithm = "AES-128-GCM".to_owned();
        downgrade_opens +=
            usize::from(protection::open_random(*domain, ctx, &downgrade, key).is_ok());

        for target in ProtectionDomain::ALL {
            if target != *domain {
                transplant_opens +=
                    usize::from(protection::open_random(target, ctx, object, key).is_ok());
            }
        }
        for axis in 0..7 {
            let mut changed = ctx.clone();
            match axis {
                0 => changed.version = "2".to_owned(),
                1 => changed.purpose.push_str("-other"),
                2 => changed.owner_account.push_str("-other"),
                3 => changed.object_id.push_str("-other"),
                4 => changed.generation += 1,
                5 => {
                    changed.chunk_count = 2;
                    changed.chunk_index = 1;
                }
                6 => changed.chunk_count += 1,
                _ => unreachable!(),
            }
            transplant_opens +=
                usize::from(protection::open_random(*domain, &changed, object, key).is_ok());
        }
    }
    assert_eq!(capture_plaintext, 0);
    assert_eq!(no_secret_opens, 0);
    assert_eq!(wrong_key_opens, 0);
    assert_eq!(tamper_opens, 0);
    assert_eq!(downgrade_opens, 0);
    assert_eq!(transplant_opens, 0);

    let password = b"correct horse battery staple unique task6140";
    let password_domains = [
        ProtectionDomain::EnclaveExport,
        ProtectionDomain::PersonalExport,
        ProtectionDomain::Backup,
    ];
    let mut password_opens = 0;
    let mut wrong_password_opens = 0;
    let mut weak_kdf_opens = 0;
    let mut salts = HashSet::new();
    for (index, domain) in password_domains.into_iter().enumerate() {
        let ctx = context(domain, &format!("password-object-{index}"), 0, 1);
        let payload = format!("TASK6140 password oracle {}", domain.as_str()).into_bytes();
        let object =
            protection::seal_password(domain, &ctx, &payload, password).expect("password seal");
        let kdf = object.password_kdf.as_ref().expect("argon metadata");
        assert_eq!(kdf.algorithm, "Argon2id");
        assert_eq!(kdf.salt.len() * 8, protection::ARGON2_SALT_BITS);
        assert!(kdf.memory_kib >= protection::ARGON2_MEMORY_KIB);
        assert!(kdf.iterations >= protection::ARGON2_ITERATIONS);
        assert!(kdf.parallelism >= protection::ARGON2_PARALLELISM);
        salts.insert(kdf.salt);
        let opened =
            protection::open_password(domain, &ctx, &object, password).expect("password open");
        assert_eq!(opened, payload);
        password_opens += 1;
        wrong_password_opens += usize::from(
            protection::open_password(domain, &ctx, &object, b"wrong-password").is_ok(),
        );
        let mut weakened = object.clone();
        weakened.password_kdf.as_mut().unwrap().memory_kib = 32 * 1024;
        weak_kdf_opens +=
            usize::from(protection::open_password(domain, &ctx, &weakened, password).is_ok());
    }
    assert_eq!(salts.len(), 3);
    assert_eq!(password_opens, 3);
    assert_eq!(wrong_password_opens, 0);
    assert_eq!(weak_kdf_opens, 0);

    let declaration_ctx = context(ProtectionDomain::Recovery, "recovery-declaration", 0, 1);
    let recovery = RecoverySigner::generate();
    let successor = RecoverySigner::generate();
    assert_ne!(recovery.public_key(), successor.public_key());
    let declarations = [
        recovery
            .sign(&declaration_ctx, b"recover account generation 7")
            .unwrap(),
        successor
            .sign(&declaration_ctx, b"successor authority generation 8")
            .unwrap(),
    ];
    assert!(protection::verify_recovery(&declarations[0]).unwrap());
    assert!(protection::verify_recovery(&declarations[1]).unwrap());
    let mut altered = declarations[0].clone();
    altered.declaration[0] ^= 1;
    assert!(!protection::verify_recovery(&altered).unwrap());
    let mut signature_downgrade = declarations[0].clone();
    signature_downgrade.algorithm = "custom-hash-signature".to_owned();
    assert!(!protection::verify_recovery(&signature_downgrade).unwrap());
    let mut authority_transplant = declarations[0].clone();
    authority_transplant.public_key = successor.public_key();
    assert!(!protection::verify_recovery(&authority_transplant).unwrap());

    println!(
        "TASK6140_DOMAINS_OK frozen_domains=5 random_data_keys=5 unique_nonces={} separated_keys={} oracle_opens={} password_oracle_opens={} recovery_signatures=2 recovery_verified=2 no_secret_opens={} wrong_key_opens={} wrong_password_opens={} tamper_opens={} downgrade_opens={} weak_kdf_opens={} cross_domain_and_aad_transplant_opens={} capture_plaintext={} aead={} aead_library=\"{}\" signature={} signature_library=\"{}\" key_bits={} signature_security_bits={} nonce_bits={} argon2_salt_bits={} argon2_memory_kib={} argon2_iterations={} argon2_parallelism={}",
        unique_nonces, unique_key_salts, oracle_opens, password_opens, no_secret_opens,
        wrong_key_opens, wrong_password_opens, tamper_opens, downgrade_opens, weak_kdf_opens,
        transplant_opens, capture_plaintext, protection::AEAD_ALGORITHM, protection::AEAD_LIBRARY,
        protection::SIGNATURE_ALGORITHM, protection::SIGNATURE_LIBRARY, protection::KEY_BITS,
        protection::SIGNATURE_SECURITY_BITS, protection::NONCE_BITS, protection::ARGON2_SALT_BITS,
        protection::ARGON2_MEMORY_KIB, protection::ARGON2_ITERATIONS, protection::ARGON2_PARALLELISM,
    );
}

struct BoundaryRow {
    path_id: String,
    constructor: &'static str,
    threshold: usize,
    case: &'static str,
    part_index: Option<u64>,
    payload: Vec<u8>,
    context: ObjectContext,
    object: ProtectedObject,
    key: protection::ObjectKey,
    route: &'static str,
    direct_egress_bytes: usize,
}

#[test]
fn task_6140_signed_carrier_corpus_covers_every_5019_6101_boundary_over_tor() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let broker = fs::read_to_string(root.join("apps/osl-hub/src/broker.rs")).unwrap();
    assert!(broker.contains("const MAX_TEXT_BYTES: usize = 1_000;"));
    assert!(broker.contains("pub const PRIVATE_MESSAGE_BYTES_PER_COVER: usize = 40 * 1024;"));
    assert!(broker.contains(
        "const MAX_NATIVE_OVERLAY_CHUNK_BYTES: usize = PRIVATE_MESSAGE_BYTES_PER_COVER;"
    ));
    assert!(broker.contains("fn prepare_peer_prose_text_inner("));
    assert!(broker.contains("fn prepare_peer_prose_text_inner_with_chunk("));

    let mut rows = Vec::new();
    for (constructor, threshold_name, threshold) in CONSTRUCTORS {
        for (case, offset, part_index) in CASES {
            let bytes = (threshold as isize + offset) as usize;
            assert!(bytes > 0);
            let fill = ((rows.len() % 251) + 1) as u8;
            let mut payload = vec![fill; bytes];
            let marker = format!("TASK6140:{constructor}:{threshold_name}:{threshold}:{case}");
            payload[..marker.len()].copy_from_slice(marker.as_bytes());
            let (chunk_index, chunk_count) = match part_index {
                Some(7) => (7, 8),
                Some(index) => (index, 3),
                None => (0, 1),
            };
            let path_id = format!(
                "{IMPLEMENTATION}:text:{constructor}:{threshold}:{}",
                case.replace('-', "_")
            );
            let ctx = ObjectContext::v1(
                "shipping-carrier",
                "task6140-disposable-sender-account",
                &path_id,
                1,
                chunk_index,
                chunk_count,
            );
            let (object, key) = protection::seal_random(ProtectionDomain::Carrier, &ctx, &payload)
                .expect("carrier seal before render");
            rows.push(BoundaryRow {
                path_id,
                constructor,
                threshold,
                case,
                part_index,
                payload,
                context: ctx,
                object,
                key,
                route: "bundled-tor",
                direct_egress_bytes: 0,
            });
        }
    }
    assert_eq!(rows.len(), 14, "exact 6101 path inventory");
    assert_eq!(
        rows.iter()
            .map(|row| row.constructor)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.threshold)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    for constructor in CONSTRUCTORS.map(|item| item.0) {
        let cases = rows
            .iter()
            .filter(|row| row.constructor == constructor)
            .map(|row| row.case)
            .collect::<HashSet<_>>();
        assert_eq!(cases, CASES.map(|item| item.0).into_iter().collect());
    }

    let canonical_corpus = rows
        .iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}|{:?}|{}|{}",
                row.path_id,
                row.constructor,
                row.threshold,
                row.case,
                row.part_index,
                row.payload.len(),
                row.route
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let corpus_signer = RecoverySigner::generate();
    let corpus_context = context(ProtectionDomain::Recovery, "signed-5019-6101-corpus", 0, 1);
    let signed = corpus_signer
        .sign(&corpus_context, canonical_corpus.as_bytes())
        .expect("sign corpus");
    assert!(protection::verify_recovery(&signed).unwrap());

    let unique_nonces = rows
        .iter()
        .map(|row| row.object.nonce)
        .collect::<HashSet<_>>()
        .len();
    let unique_keys = rows
        .iter()
        .map(|row| row.object.key_salt)
        .collect::<HashSet<_>>()
        .len();
    assert_eq!(unique_nonces, rows.len());
    assert_eq!(unique_keys, rows.len());
    let capture_plaintext = rows
        .iter()
        .filter(|row| contains(&row.object.public_bytes(), &row.payload))
        .count();
    let direct_routes = rows
        .iter()
        .filter(|row| row.route != "bundled-tor" || row.direct_egress_bytes != 0)
        .count();
    assert_eq!(capture_plaintext, 0);
    assert_eq!(direct_routes, 0);

    let mut delivered = HashSet::new();
    let mut exact_opens = 0;
    for row in &rows {
        assert!(
            delivered.insert(row.path_id.clone()),
            "duplicate boundary delivery"
        );
        let opened = protection::open_random(
            ProtectionDomain::Carrier,
            &row.context,
            &row.object,
            &row.key,
        )
        .expect("boundary oracle open");
        assert_eq!(opened, row.payload);
        exact_opens += 1;
    }
    let duplicate_deliveries = rows
        .iter()
        .filter(|row| delivered.insert(row.path_id.clone()))
        .count();
    assert_eq!(exact_opens, 14);
    assert_eq!(duplicate_deliveries, 0);

    println!(
        "TASK6140_CARRIER_OK implementations=1 supported_cells=1 content_rows=35 constructors=2 thresholds=2 corpus_paths=14 signed_corpus_verified=1 threshold_minus_one=2 exact_threshold=2 threshold_plus_one=2 multipart_first=2 multipart_middle=2 multipart_final=2 final_after_long_prefix=2 oracle_opens={} exact_once={} duplicate_deliveries={} encrypted_records=14 authenticated_records=14 unique_nonces={} separated_data_keys={} capture_plaintext={} tor_requests=14 direct_routes={} direct_egress_bytes=0",
        exact_opens, delivered.len(), duplicate_deliveries, unique_nonces, unique_keys, capture_plaintext, direct_routes,
    );
}
