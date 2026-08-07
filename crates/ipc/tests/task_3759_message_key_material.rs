use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::cmd_osl_encrypt_message_v2_wire;
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity};

const ALICE_DID: &str = "900000000003759001";
const BOB_DID: &str = "900000000003759002";
const CAROL_DID: &str = "900000000003759003";
const GROUP_ID: &str = "task-3759-group-room";
const SERVER_ID: &str = "task-3759-server";
const CHANNEL_ID: &str = "task-3759-server-channel";

#[derive(Clone)]
struct MessageCase {
    name: &'static str,
    path: &'static str,
    scope: Scope,
    members: Vec<String>,
    plaintext: String,
}

struct WireMaterial {
    version: u8,
    msg_type: u8,
    transport: MaterialTransport,
}

enum MaterialTransport {
    V3 {
        recipient_slots: usize,
        x25519_ephemeral_slots: usize,
        mlkem768_ciphertext_slots: usize,
    },
    V5 {
        sender_mlkem_ad_bound: bool,
        wrong_sender_mlkem_rejected: bool,
    },
}

fn install_identity(state: &AppState, did: &str, name: &str) -> Identity {
    let mut identity = generate_identity(name.to_owned());
    identity.discord_snowflake = Some(did.to_owned());
    state.install_identity(identity.clone());
    identity
}

fn install_peer(state: &AppState, did: &str, identity: &Identity, grant: WhitelistEntry) {
    state.peer_map.lock().expect("peer map").insert(
        did.to_owned(),
        PeerEntry {
            osl_user_id: Some(identity.user_id.clone()),
            discord_id: Some(did.to_owned()),
            pubkey: Some(STANDARD.encode(identity.x25519_public.as_bytes())),
            ik_mlkem768_pub: Some(STANDARD.encode(identity.mlkem_public_bytes)),
            outgoing_whitelists: vec![grant],
            ..PeerEntry::default()
        },
    );
}

fn enable_scope(state: &AppState, scope: &Scope) {
    state.whitelist_state.lock().expect("whitelist").insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
}

fn inspect_v3_material(wire: &str) -> WireMaterial {
    let body = wire.strip_prefix("DPC0::").expect("wire uses DPC0 prefix");
    let raw = STANDARD.decode(body).expect("wire is base64");
    assert!(
        raw.len() >= 35 + ipc::wire_v2::SLOT_V3_BYTES + 12 + 16,
        "v3 wire has complete global header, slot and body"
    );
    let version = raw[0];
    let msg_type = raw[1];
    let recipient_slots = raw[34] as usize;
    let slots_start = 35usize;
    let slots_end = slots_start + recipient_slots * ipc::wire_v2::SLOT_V3_BYTES;
    assert!(
        raw.len() >= slots_end + 12 + 16,
        "v3 wire has complete slots and body"
    );

    let mut x25519_ephemeral_slots = 0usize;
    let mut mlkem768_ciphertext_slots = 0usize;
    for slot in 0..recipient_slots {
        let base = slots_start + slot * ipc::wire_v2::SLOT_V3_BYTES;
        let eph_start = base + ipc::wire_v2::RECIPIENT_HASH_PREFIX_LEN;
        let eph_end = eph_start + 32;
        if raw[eph_start..eph_end].iter().any(|byte| *byte != 0) {
            x25519_ephemeral_slots += 1;
        }
        let ct_len = u16::from_le_bytes([raw[eph_end], raw[eph_end + 1]]) as usize;
        let ct_start = eph_end + 2;
        let ct_end = ct_start + crypto::ml_kem_768::CIPHERTEXT_SIZE;
        assert!(ct_end <= raw.len(), "ML-KEM ciphertext is within the slot");
        if ct_len == crypto::ml_kem_768::CIPHERTEXT_SIZE
            && raw[ct_start..ct_end].iter().any(|byte| *byte != 0)
        {
            mlkem768_ciphertext_slots += 1;
        }
    }

    WireMaterial {
        version,
        msg_type,
        transport: MaterialTransport::V3 {
            recipient_slots,
            x25519_ephemeral_slots,
            mlkem768_ciphertext_slots,
        },
    }
}

fn inspect_v5_material(
    state: &AppState,
    case: &MessageCase,
    wire: &str,
    sender_mlkem_pub: &[u8],
) -> WireMaterial {
    let body = wire.strip_prefix("DPC0::").expect("wire uses DPC0 prefix");
    let raw = STANDARD.decode(body).expect("wire is base64");
    assert!(
        raw.len() >= ipc::wire_v2::V5_GLOBAL_HEADER_BYTES,
        "v5 wire has a complete global header"
    );
    let version = raw[0];
    let parsed = ipc::wire_v2::decrypt_v5(wire).expect("wire is parseable v5 content");
    assert_eq!(parsed.msg_type, ipc::wire_v2::MSG_TYPE_CONTENT);

    let (chain_id, rotation_root, physical_device_id) = {
        let sender_keys = state.sender_key_state.lock().expect("sender keys");
        let scope_state = sender_keys
            .get(&case.scope.storage_key())
            .unwrap_or_else(|| panic!("{} sender-key state exists", case.name));
        let sender_chain = scope_state
            .sender_chain()
            .unwrap_or_else(|| panic!("{} sender chain exists", case.name));
        (
            sender_chain.current_chain_id(),
            sender_chain.rotation_root_bytes(),
            sender_chain.physical_device_id(),
        )
    };

    let encrypted = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    let correct_ctx = crypto::sender_keys::SenderContext {
        sender_ik_x25519_pub: parsed.sender_ik_pub,
        sender_ik_mlkem_pub: sender_mlkem_pub.to_vec(),
        group_id: case.scope.storage_key().into_bytes(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };
    let mut correct_state = crypto::sender_keys::SenderKeyState::new();
    correct_state
        .install_receiver(
            ALICE_DID.as_bytes().to_vec(),
            chain_id,
            &rotation_root,
            physical_device_id,
        )
        .expect("install correct receiver");
    let opened = correct_state
        .decrypt_from(ALICE_DID.as_bytes(), &encrypted, &correct_ctx)
        .unwrap_or_else(|error| panic!("{} correct ML-KEM AD rejected: {error}", case.name));
    let sender_mlkem_ad_bound = opened == case.plaintext.as_bytes();

    let mut wrong_mlkem_pub = sender_mlkem_pub.to_vec();
    wrong_mlkem_pub.fill(0);
    assert_ne!(
        wrong_mlkem_pub, sender_mlkem_pub,
        "generated sender ML-KEM pubkey should not be all zeroes"
    );
    let wrong_ctx = crypto::sender_keys::SenderContext {
        sender_ik_x25519_pub: correct_ctx.sender_ik_x25519_pub,
        sender_ik_mlkem_pub: wrong_mlkem_pub,
        group_id: correct_ctx.group_id.clone(),
        session_version: correct_ctx.session_version,
    };
    let mut wrong_state = crypto::sender_keys::SenderKeyState::new();
    wrong_state
        .install_receiver(
            ALICE_DID.as_bytes().to_vec(),
            chain_id,
            &rotation_root,
            physical_device_id,
        )
        .expect("install wrong receiver");
    let wrong_sender_mlkem_rejected = wrong_state
        .decrypt_from(ALICE_DID.as_bytes(), &encrypted, &wrong_ctx)
        .is_err();

    WireMaterial {
        version,
        msg_type: parsed.msg_type,
        transport: MaterialTransport::V5 {
            sender_mlkem_ad_bound,
            wrong_sender_mlkem_rejected,
        },
    }
}

fn inspect_material(
    state: &AppState,
    case: &MessageCase,
    wire: &str,
    sender_mlkem_pub: &[u8],
) -> WireMaterial {
    let body = wire.strip_prefix("DPC0::").expect("wire uses DPC0 prefix");
    let raw = STANDARD.decode(body).expect("wire is base64");
    match raw.first().copied() {
        Some(ipc::wire_v2::WIRE_VERSION_V3) => inspect_v3_material(wire),
        Some(ipc::wire_v2::WIRE_VERSION_V5) => {
            inspect_v5_material(state, case, wire, sender_mlkem_pub)
        }
        Some(version) => panic!("{} unexpected wire version 0x{version:02x}", case.name),
        None => panic!("{} empty wire", case.name),
    }
}

fn send_case(state: &AppState, case: &MessageCase, sender_mlkem_pub: &[u8]) -> WireMaterial {
    let output = cmd_osl_encrypt_message_v2_wire(
        state,
        case.plaintext.clone(),
        ScopeInput::from(&case.scope),
        case.members.clone(),
        ALICE_DID.to_owned(),
    )
    .unwrap_or_else(|error| panic!("{} send failed: {error}", case.name));
    inspect_material(state, case, &output.content, sender_mlkem_pub)
}

#[test]
fn task_3759_all_ten_actual_messages_carry_stronger_key_material() {
    let state = AppState::new();
    let alice = install_identity(&state, ALICE_DID, "task-3759-alice");
    let bob = generate_identity("task-3759-bob".to_owned());
    let carol = generate_identity("task-3759-carol".to_owned());

    let dm_scope = Scope::dm(BOB_DID);
    let group_scope = Scope::gc(GROUP_ID);
    let server_scope = Scope::server_channel(SERVER_ID, CHANNEL_ID);

    enable_scope(&state, &dm_scope);
    enable_scope(&state, &group_scope);
    enable_scope(&state, &server_scope);

    install_peer(
        &state,
        BOB_DID,
        &bob,
        WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        },
    );
    install_peer(
        &state,
        CAROL_DID,
        &carol,
        WhitelistEntry::Gc {
            id: GROUP_ID.to_owned(),
            user_specific: false,
        },
    );
    state
        .peer_map
        .lock()
        .expect("peer map")
        .get_mut(BOB_DID)
        .expect("bob peer")
        .outgoing_whitelists
        .push(WhitelistEntry::ServerChannel {
            server_id: SERVER_ID.to_owned(),
            channel_id: CHANNEL_ID.to_owned(),
            user_specific: true,
        });

    let cases = vec![
        MessageCase {
            name: "task3759-dm-01",
            path: "dm",
            scope: dm_scope.clone(),
            members: vec![BOB_DID.to_owned()],
            plaintext: "task 3759 direct message 01".to_owned(),
        },
        MessageCase {
            name: "task3759-dm-02",
            path: "dm",
            scope: dm_scope.clone(),
            members: vec![BOB_DID.to_owned()],
            plaintext: "task 3759 direct message 02".to_owned(),
        },
        MessageCase {
            name: "task3759-dm-03",
            path: "dm",
            scope: dm_scope.clone(),
            members: vec![BOB_DID.to_owned()],
            plaintext: "task 3759 direct message 03".to_owned(),
        },
        MessageCase {
            name: "task3759-dm-04",
            path: "dm",
            scope: dm_scope.clone(),
            members: vec![BOB_DID.to_owned()],
            plaintext: "task 3759 direct message 04".to_owned(),
        },
        MessageCase {
            name: "task3759-group-01",
            path: "group",
            scope: group_scope.clone(),
            members: vec![ALICE_DID.to_owned(), CAROL_DID.to_owned()],
            plaintext: "task 3759 group message 01".to_owned(),
        },
        MessageCase {
            name: "task3759-group-02",
            path: "group",
            scope: group_scope.clone(),
            members: vec![ALICE_DID.to_owned(), CAROL_DID.to_owned()],
            plaintext: "task 3759 group message 02".to_owned(),
        },
        MessageCase {
            name: "task3759-group-03",
            path: "group",
            scope: group_scope.clone(),
            members: vec![ALICE_DID.to_owned(), CAROL_DID.to_owned()],
            plaintext: "task 3759 group message 03".to_owned(),
        },
        MessageCase {
            name: "task3759-server-01",
            path: "server",
            scope: server_scope.clone(),
            members: vec![ALICE_DID.to_owned(), BOB_DID.to_owned()],
            plaintext: "task 3759 server message 01".to_owned(),
        },
        MessageCase {
            name: "task3759-server-02",
            path: "server",
            scope: server_scope.clone(),
            members: vec![ALICE_DID.to_owned(), BOB_DID.to_owned()],
            plaintext: "task 3759 server message 02".to_owned(),
        },
        MessageCase {
            name: "task3759-server-03",
            path: "server",
            scope: server_scope,
            members: vec![ALICE_DID.to_owned(), BOB_DID.to_owned()],
            plaintext: "task 3759 server message 03".to_owned(),
        },
    ];

    let mut missing_stronger = 0usize;
    let mut group_seen = false;
    let mut server_seen = false;

    for case in &cases {
        let material = send_case(&state, case, &alice.mlkem_public_bytes);
        let carries_stronger = match material.transport {
            MaterialTransport::V3 {
                recipient_slots,
                x25519_ephemeral_slots,
                mlkem768_ciphertext_slots,
            } => {
                material.version == ipc::wire_v2::WIRE_VERSION_V3
                    && material.msg_type == ipc::wire_v2::MSG_TYPE_CONTENT
                    && recipient_slots >= 2
                    && x25519_ephemeral_slots == recipient_slots
                    && mlkem768_ciphertext_slots == recipient_slots
            }
            MaterialTransport::V5 {
                sender_mlkem_ad_bound,
                wrong_sender_mlkem_rejected,
            } => {
                material.version == ipc::wire_v2::WIRE_VERSION_V5
                    && material.msg_type == ipc::wire_v2::MSG_TYPE_CONTENT
                    && sender_mlkem_ad_bound
                    && wrong_sender_mlkem_rejected
            }
        };
        if !carries_stronger {
            missing_stronger += 1;
        }
        group_seen |= case.path == "group";
        server_seen |= case.path == "server";
        match material.transport {
            MaterialTransport::V3 {
                recipient_slots,
                x25519_ephemeral_slots,
                mlkem768_ciphertext_slots,
            } => println!(
                "TASK3759 message_name={} path={} transport=v3 version=0x{:02x} msg_type=0x{:02x} recipient_slots={} x25519_ephemeral_slots={} mlkem768_ciphertext_slots={} carries_stronger_key_material={}",
                case.name,
                case.path,
                material.version,
                material.msg_type,
                recipient_slots,
                x25519_ephemeral_slots,
                mlkem768_ciphertext_slots,
                carries_stronger
            ),
            MaterialTransport::V5 {
                sender_mlkem_ad_bound,
                wrong_sender_mlkem_rejected,
            } => println!(
                "TASK3759 message_name={} path={} transport=v5 version=0x{:02x} msg_type=0x{:02x} sender_mlkem_ad_bound={} wrong_sender_mlkem_rejected={} carries_stronger_key_material={}",
                case.name,
                case.path,
                material.version,
                material.msg_type,
                sender_mlkem_ad_bound,
                wrong_sender_mlkem_rejected,
                carries_stronger
            ),
        }
    }

    println!(
        "TASK3759 total_messages={} missing_stronger_key_material={} group_message_name=task3759-group-01 server_message_name=task3759-server-01 group_seen={} server_seen={}",
        cases.len(),
        missing_stronger,
        group_seen,
        server_seen
    );

    assert_eq!(cases.len(), 10);
    assert_eq!(missing_stronger, 0);
    assert!(group_seen, "a group message must be among the ten by name");
    assert!(
        server_seen,
        "a server message must be among the ten by name"
    );
}
