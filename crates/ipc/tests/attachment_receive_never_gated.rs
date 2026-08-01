//! T16-T22: receiving encrypted attachments is free on every IPC open path.
//!
//! A paid sender seals each supported attachment wire format. A fresh Free
//! recipient then opens it through each receiver entry point. This is an
//! end-to-end policy test: adding the attachment send-tier gate to any open
//! path makes the corresponding assertion fail while the sender remains paid.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::attachment_wire::{
    cmd_osl_open_attachment, cmd_osl_open_attachment_b64, cmd_osl_seal_attachment,
};
use ipc::commands::{
    cmd_osl_open_attachment_v2, cmd_osl_seal_attachment_with_cover_v2,
    cmd_osl_seal_attachment_with_cover_v3,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use keystore::{generate_identity, LicenseState, LicenseStateDto};

const PRO_SENDER_DID: &str = "900000000000000003";
const FREE_RECIPIENT_DID: &str = "900000000000000001";

fn paid_sender() -> AppState {
    let state = AppState::new();
    *state.identity.lock().expect("identity mutex poisoned") =
        Some(generate_identity("pro-sender".to_owned()));
    *state
        .license_state
        .lock()
        .expect("license state mutex poisoned") = LicenseStateDto {
        state: LicenseState::Paid,
        raw_status: "ACTIVE".to_owned(),
        current_period_end: None,
        last_validated_at: None,
    };
    state
}

fn free_recipient(
    sender: &AppState,
    secret: &x25519::SecretKey,
    public: x25519::PublicKey,
) -> AppState {
    let state = AppState::new();
    let mut identity = generate_identity("free-recipient".to_owned());
    identity.x25519_secret = secret.clone();
    identity.x25519_public = public;
    *state.identity.lock().expect("identity mutex poisoned") = Some(identity);

    let sender_public = sender
        .identity
        .lock()
        .expect("identity mutex poisoned")
        .as_ref()
        .expect("sender identity present")
        .x25519_public;
    let mut peers = state.peer_map.lock().expect("peer map mutex poisoned");
    let sender = peers.entry(PRO_SENDER_DID.to_owned()).or_default();
    sender.pubkey = Some(STANDARD.encode(sender_public.as_bytes()));
    sender.discord_id = Some(PRO_SENDER_DID.to_owned());
    state
}

fn allow_pro_sender_to_send_to(sender: &AppState, recipient_public: x25519::PublicKey) {
    let mut peers = sender.peer_map.lock().expect("peer map mutex poisoned");
    let recipient = peers.entry(FREE_RECIPIENT_DID.to_owned()).or_default();
    recipient.pubkey = Some(STANDARD.encode(recipient_public.as_bytes()));
    recipient.discord_id = Some(FREE_RECIPIENT_DID.to_owned());
    recipient.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn dm_scope() -> Scope {
    Scope::dm(FREE_RECIPIENT_DID)
}

fn scope_input(scope: &Scope) -> ScopeInput {
    ScopeInput::from(scope)
}

fn assert_opened_plaintext(opened: ipc::attachment_wire::OpenedAttachment, expected: &[u8]) {
    assert_eq!(
        STANDARD
            .decode(opened.plaintext_b64)
            .expect("opened plaintext is base64"),
        expected
    );
}

#[test]
fn free_recipient_opens_a_pro_attachment_on_every_ipc_open_path() {
    let sender = paid_sender();
    let (recipient_secret, recipient_public) = x25519::generate_keypair();
    allow_pro_sender_to_send_to(&sender, recipient_public);
    let recipient = free_recipient(&sender, &recipient_secret, recipient_public);
    assert_eq!(
        recipient
            .license_state
            .lock()
            .expect("license state mutex poisoned")
            .state,
        LicenseState::Free,
        "the receiver must be a Free install"
    );

    let scope = dm_scope();
    let plaintext = b"attachment delivery must not require a subscription".to_vec();

    // Legacy V1 has three supported receiver entry points: raw bytes, base64,
    // and the V1 fallback in the current open command.
    let v1 = cmd_osl_seal_attachment(&sender, plaintext.clone(), "legacy.png".to_owned())
        .expect("paid sender seals V1 attachment");
    let v1_bytes = STANDARD
        .decode(&v1.file_blob_b64)
        .expect("V1 sealed bytes are base64");
    assert_opened_plaintext(
        cmd_osl_open_attachment(&recipient, v1.att_key_b64.clone(), v1_bytes)
            .expect("Free recipient opens raw V1 attachment"),
        &plaintext,
    );
    assert_opened_plaintext(
        cmd_osl_open_attachment_b64(&recipient, v1.att_key_b64.clone(), &v1.file_blob_b64)
            .expect("Free recipient opens base64 V1 attachment"),
        &plaintext,
    );
    assert_opened_plaintext(
        cmd_osl_open_attachment_v2(
            &recipient,
            PRO_SENDER_DID.to_owned(),
            None,
            v1.file_blob_b64,
            Some(v1.att_key_b64),
            None,
        )
        .expect("Free recipient opens V1 fallback attachment"),
        &plaintext,
    );

    let v2 = cmd_osl_seal_attachment_with_cover_v2(
        &sender,
        scope_input(&scope),
        vec![FREE_RECIPIENT_DID.to_owned()],
        PRO_SENDER_DID.to_owned(),
        STANDARD.encode(&plaintext),
        "current.png".to_owned(),
        "current.bin".to_owned(),
    )
    .expect("paid sender seals V2 attachment");
    assert_opened_plaintext(
        cmd_osl_open_attachment_v2(
            &recipient,
            PRO_SENDER_DID.to_owned(),
            Some(scope_input(&scope)),
            v2.sealed_b64,
            None,
            None,
        )
        .expect("Free recipient opens V2 attachment"),
        &plaintext,
    );

    let v3 = cmd_osl_seal_attachment_with_cover_v3(
        &sender,
        scope_input(&scope),
        vec![FREE_RECIPIENT_DID.to_owned()],
        PRO_SENDER_DID.to_owned(),
        STANDARD.encode(&plaintext),
        "current.mp4".to_owned(),
        "current.mp4".to_owned(),
    )
    .expect("paid sender seals V3 attachment");
    assert_opened_plaintext(
        cmd_osl_open_attachment_v2(
            &recipient,
            PRO_SENDER_DID.to_owned(),
            Some(scope_input(&scope)),
            v3.sealed_b64,
            None,
            None,
        )
        .expect("Free recipient opens V3 attachment"),
        &plaintext,
    );
}
