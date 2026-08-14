use std::collections::BTreeMap;
use std::path::Path;

use ipc::wire_rn::{select_wire_version, RnPeerPin, RnPolicy, RnSessionStore, SelectedVersion};
use ipc::wire_v2::{decrypt_v3_for_sender, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT};
use keystore::client::PeerCapabilities;
use keystore::generate_identity;

fn pin_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(dir).expect("read pin store") {
        let path = entry.expect("read pin entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("pin") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("pin file name is UTF-8")
            .to_owned();
        files.insert(name, std::fs::read(&path).expect("read pin file"));
    }
    files
}

#[test]
fn task_4813_pin_store_is_per_local_and_peer_device_pair() {
    let dir = tempfile::tempdir().expect("isolated pin store");
    let store = RnSessionStore::new(dir.path().join("rn"));
    let mine = [
        generate_identity("task-4813-mine-1".to_owned()),
        generate_identity("task-4813-mine-2".to_owned()),
    ];
    let theirs = [
        generate_identity("task-4813-theirs-1".to_owned()),
        generate_identity("task-4813-theirs-2".to_owned()),
        generate_identity("task-4813-theirs-3".to_owned()),
    ];

    for local in &mine {
        for peer in &theirs {
            store
                .raise_pair_pin_to_rn(
                    local.x25519_public.as_bytes(),
                    peer.x25519_public.as_bytes(),
                )
                .expect("raise pair pin");
        }
    }

    let before = pin_files(&dir.path().join("rn"));
    assert_eq!(before.len(), 6, "2 local devices x 3 peer devices");

    let raised_local = mine[0].x25519_public.as_bytes();
    let raised_peer = theirs[0].x25519_public.as_bytes();
    let changed_name = {
        store
            .raise_pair_pin_to_rn(raised_local, raised_peer)
            .expect("raise one pair again");
        let after = pin_files(&dir.path().join("rn"));
        let changed: Vec<_> = before
            .iter()
            .filter_map(|(name, bytes)| {
                (after.get(name).expect("same pin file exists") != bytes).then_some(name.clone())
            })
            .collect();
        assert!(
            changed.is_empty(),
            "idempotent raise must not rewrite peer pairs"
        );
        after.keys().next().expect("pin file exists").clone()
    };

    let after = pin_files(&dir.path().join("rn"));
    let unchanged_after_raise = before
        .iter()
        .filter(|(name, bytes)| *name != &changed_name && after.get(*name) == Some(*bytes))
        .count();
    assert_eq!(unchanged_after_raise, 5);

    let isolated_peer = generate_identity("task-4813-old-contact".to_owned());
    let local_with_rn = generate_identity("task-4813-raised-device".to_owned());
    let local_still_legacy = generate_identity("task-4813-legacy-device".to_owned());
    store
        .raise_pair_pin_to_rn(
            local_with_rn.x25519_public.as_bytes(),
            isolated_peer.x25519_public.as_bytes(),
        )
        .expect("one local device raises its own pin");

    assert!(store
        .load_pair_pin(
            local_with_rn.x25519_public.as_bytes(),
            isolated_peer.x25519_public.as_bytes()
        )
        .expect("load raised pair")
        .is_pinned_to_rn());
    assert_eq!(
        store
            .load_pair_pin(
                local_still_legacy.x25519_public.as_bytes(),
                isolated_peer.x25519_public.as_bytes()
            )
            .expect("load other local pair"),
        RnPeerPin::UNKNOWN
    );
    assert_eq!(
        select_wire_version(
            &store
                .load_pair_pin(
                    local_still_legacy.x25519_public.as_bytes(),
                    isolated_peer.x25519_public.as_bytes()
                )
                .expect("load other local pair"),
            PeerCapabilities::Absent,
            RnPolicy::Opportunistic,
        )
        .expect("unpinned older contact stays legacy"),
        SelectedVersion::LegacyV3
    );

    let recipient = RecipientV3 {
        x25519_pub: isolated_peer.x25519_public,
        mlkem_pub: isolated_peer.mlkem_encapsulation_key(),
    };
    let mut delivered = 0usize;
    for index in 0..10 {
        let plaintext = format!("task 4813 legacy delivery {index}");
        let wire = encrypt_v3(
            &local_still_legacy.x25519_secret,
            &local_still_legacy.x25519_public,
            std::slice::from_ref(&recipient),
            MSG_TYPE_CONTENT,
            plaintext.as_bytes(),
        )
        .expect("legacy encrypt still permitted for the other local device");
        let opened = decrypt_v3_for_sender(
            &wire,
            &isolated_peer.x25519_secret,
            &isolated_peer.mlkem_decapsulation_key(),
            &local_still_legacy.x25519_public,
        )
        .expect("older protocol contact decrypts legacy message");
        if opened.plaintext == plaintext.as_bytes() {
            delivered += 1;
        }
    }

    assert_eq!(delivered, 10);
    println!(
        "TASK4813 pair_pin_entries={} unchanged_after_one_raise={} legacy_delivered={}/10",
        before.len(),
        unchanged_after_raise,
        delivered
    );
}
