//! HTTP client for the OSL key server.
//!
//! Built on `reqwest` 0.12 blocking + rustls-tls (webpki-roots).
//! Both `http://` and `https://` `base_url`s are accepted —
//! Railway-deployed Phase B keyservers force-redirect to HTTPS at
//! the edge, so the client must do TLS; localhost dev keyservers
//! over plain HTTP still work for the inner-loop workflow.
//!
//! Endpoints exposed here mirror [`keyserver/src/server.js`]:
//!
//! - [`KeyServerClient::register`] → `POST /v1/register`
//! - [`KeyServerClient::fetch_pubkeys`] → `GET /v1/pubkeys/:user_id`
//! - [`KeyServerClient::fetch_prekey_bundle`] → `GET /v1/prekey-bundle/:user_id`
//! - [`KeyServerClient::replenish_prekeys`] → `POST /v1/prekey-bundle/replenish`
//! - [`KeyServerClient::burn`] → `DELETE /v1/wrapped-keys`
//!
//! All calls block on I/O. Tauri command handlers (Layer 8) drive
//! these through `tokio::task::spawn_blocking` to avoid stalling
//! the async runtime. reqwest's blocking client has its own
//! Tokio runtime under the hood; that's fine — the outer Tauri
//! runtime stays unblocked.

use crate::burn::{sign_burn, BurnScope};
use crate::control_inbox::{
    sign_control_inbox_delete, sign_control_inbox_get, sign_control_inbox_post,
};
use crate::identity::Identity;
use crate::prekeys::{
    sign_replenish_batch, OpkEntry, PrekeyState, ReplenishOpk, ReplenishSpk, SpkEntry,
};
use crate::signed_get::{sign_prekey_bundle_get, sign_wrapped_key_get};
use crate::unregister::sign_unregister;
use crate::wrapped_key::{sign_wrapped_key_post, WrappedKeyUpload};
use crate::{Error, Result};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Rotation proof for an authenticated key change (register
/// state-machine Case C). Present only when rotating an existing
/// `user_id` onto a new Ed25519 identity key.
#[derive(Serialize)]
pub struct RotationProof {
    /// The CURRENTLY-registered Ed25519 pub (base64). Must byte-equal
    /// the server's stored key or the rotation is rejected (replay
    /// defence — see `ROT_MSG`).
    pub prev_ik_ed25519_pub: String,
    /// `Ed25519(ROT_MSG, OLD identity secret)` (base64, 64 bytes) —
    /// authorises the change with the key being replaced.
    pub prev_sig: String,
}

/// Body of `POST /v1/register`.
///
/// REGISTER-FIX: `/v1/register` is now open + Ed25519-self-signed.
/// `registration_sig` is load-bearing: the server verifies it over
/// the reconstructed `REG_MSG` against `ik_ed25519_pub` (Case A/B)
/// or against the NEW key during a rotation (Case C). The legacy
/// `ik_x25519_signature` placeholder field is gone.
#[derive(Serialize)]
pub struct RegisterRequest {
    pub user_id: String,
    pub ik_x25519_pub: String,
    /// Ed25519 identity-signing key. The server verifies
    /// `registration_sig` against this (and reuses it for
    /// `prekey-bundle/replenish` batch signatures).
    pub ik_ed25519_pub: String,
    pub ik_mlkem768_pub: String,
    /// `Ed25519(REG_MSG, identity.ed25519_secret)` (base64, 64 bytes).
    /// In a rotation this is signed by the NEW key (proof of
    /// possession of the key being rotated to).
    pub registration_sig: String,
    /// Present only for an authenticated key rotation (Case C).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationProof>,
    /// Phase 9-A2: base64-encoded X25519 public key used by peers
    /// as the initial Double Ratchet bootstrap pub. Skipped on the
    /// wire when the local build hasn't generated one (legacy
    /// upgrades); the keyserver column is nullable so old servers
    /// receive `null` and reject unmatched columns just as they did
    /// before. Senders treat a missing column as "peer not v=4
    /// eligible" and fall through to v=3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ik_ratchet_initial_pub: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub registered_at: Option<String>,
    pub key_rotation_recorded: Option<bool>,
    pub last_rotated_at: Option<String>,
    /// REGISTER-FIX: `"noop"` (Case B — already ours, write-free) or
    /// `"rotated"` (Case C — authenticated rotation applied). Absent
    /// on a Case A 201 (which carries `registered_at` instead).
    #[serde(default)]
    pub status: Option<String>,
}

/// Domain-separation + version tag for first/refresh registration.
/// MUST byte-match `keyserver-cf/src/lib/signed-request.ts`
/// `REG_DOMAIN` (a mirrored vector covers this in tests).
pub const REG_DOMAIN: &str = "OSL-REGISTER-v1";
/// Domain-separation + version tag for authenticated rotation.
pub const ROT_DOMAIN: &str = "OSL-ROTATE-v1";

/// REG_MSG bytes — byte-identical to the server's `buildRegMsg`:
///
///   "OSL-REGISTER-v1\n" || user_id "\n" || ik_x25519_pub_b64 "\n"
///   || ik_ed25519_pub_b64 "\n" || ik_mlkem768_pub_b64 "\n"
///   || ik_ratchet_initial_pub_b64_or_empty
///
/// The b64 args are the EXACT strings placed in the JSON body
/// (never re-encoded). A `None` ratchet contributes the empty
/// string with no trailing newline.
pub fn reg_msg(
    user_id: &str,
    ik_x25519_pub_b64: &str,
    ik_ed25519_pub_b64: &str,
    ik_mlkem768_pub_b64: &str,
    ik_ratchet_initial_pub_b64: Option<&str>,
) -> Vec<u8> {
    format!(
        "{REG_DOMAIN}\n{user_id}\n{ik_x25519_pub_b64}\n{ik_ed25519_pub_b64}\n{ik_mlkem768_pub_b64}\n{}",
        ik_ratchet_initial_pub_b64.unwrap_or("")
    )
    .into_bytes()
}

/// ROT_MSG bytes — byte-identical to the server's `buildRotMsg`:
///
///   "OSL-ROTATE-v1\n" || user_id "\n" || prev_ik_ed25519_pub_b64
///   "\n" || new_ik_x25519_pub_b64 "\n" || new_ik_ed25519_pub_b64
///   "\n" || new_ik_mlkem768_pub_b64 "\n"
///   || new_ik_ratchet_initial_pub_b64_or_empty
pub fn rot_msg(
    user_id: &str,
    prev_ik_ed25519_pub_b64: &str,
    new_ik_x25519_pub_b64: &str,
    new_ik_ed25519_pub_b64: &str,
    new_ik_mlkem768_pub_b64: &str,
    new_ik_ratchet_initial_pub_b64: Option<&str>,
) -> Vec<u8> {
    format!(
        "{ROT_DOMAIN}\n{user_id}\n{prev_ik_ed25519_pub_b64}\n{new_ik_x25519_pub_b64}\n{new_ik_ed25519_pub_b64}\n{new_ik_mlkem768_pub_b64}\n{}",
        new_ik_ratchet_initial_pub_b64.unwrap_or("")
    )
    .into_bytes()
}

#[derive(Debug, Deserialize)]
pub struct PubkeysResponse {
    pub user_id: String,
    pub ik_x25519_pub: String,
    pub ik_ed25519_pub: String,
    pub ik_mlkem768_pub: String,
    pub registered_at: String,
    pub last_rotated_at: Option<String>,
    /// Phase 9-A2: peer's published ratchet bootstrap pub. `None`
    /// when the server hasn't been migrated yet OR the peer
    /// registered before A2 rolled out OR the peer's build doesn't
    /// support v=4 sends. Old servers return responses without
    /// this field at all; `#[serde(default)]` lets them parse.
    #[serde(default)]
    pub ik_ratchet_initial_pub: Option<String>,
}

/// One-time prekey returned by `/v1/prekey-bundle/:user_id`. `None`
/// when the server's pool is exhausted (the design's "OPK
/// exhaustion fallback" — sender's PQXDH proceeds without DH4).
#[derive(Debug, Deserialize)]
pub struct PrekeyBundleOpk {
    pub id: u32,
    pub pub_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct PrekeyBundleResponse {
    pub user_id: String,
    pub ik_x25519_pub: String,
    pub ik_ed25519_pub: String,
    pub ik_mlkem768_pub: String,
    pub spk_pub: String,
    pub spk_signature: String,
    pub spk_rotated_at: String,
    pub opk: Option<PrekeyBundleOpk>,
    pub remaining_opk_count: u32,
    /// Phase 9-A2: peer's ratchet bootstrap pub, surfaced on the
    /// prekey-bundle endpoint so callers fetching a bundle for a
    /// fresh session immediately know the v=4 eligibility.
    #[serde(default)]
    pub ik_ratchet_initial_pub: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplenishResponse {
    pub user_id: String,
    pub opks_added: u32,
}

/// Recipient-authorized response from `GET /v1/wrapped-keys/:content_id`.
#[derive(Debug, Deserialize)]
pub struct WrappedKeyResponse {
    pub content_id: String,
    pub content_type: String,
    pub system_message_kind: Option<String>,
    pub sender_id: String,
    pub recipient_id: String,
    pub session_version: u32,
    pub share_index: u32,
    pub wrapped_share_blob: String,
    pub blob_version: u32,
    pub single_use: bool,
    pub display_duration_seconds: Option<u32>,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct WrappedKeyPostResponse {
    pub content_id: String,
}

/// Response body for `DELETE /v1/wrapped-keys`.
#[derive(Debug, Deserialize)]
pub struct BurnResponse {
    pub scope: String,
    pub deleted_count: u32,
}

/// Body of `POST /v1/license/validate`.
#[derive(Serialize)]
struct LicenseValidateRequest<'a> {
    license_key: &'a str,
}

/// Response body for `POST /v1/license/validate`.
///
/// The endpoint always returns HTTP 200 on a parseable request,
/// even when the license is unknown / revoked / malformed —
/// the *meaning* lives in `status`. F2.4 distinguishes a
/// `keyserver-unreachable` outcome (mapped to `Error::Transport`
/// by the client) from a `keyserver-rejected` outcome (mapped to
/// `Error::HttpStatus` or a deserialised `UNKNOWN` / `REVOKED`
/// response) when deciding whether to honour the offline-grace
/// window.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LicenseValidateResponse {
    /// One of: `"ACTIVE"`, `"GRACE"`, `"CANCELLED"`, `"EXPIRED"`,
    /// `"REVOKED"`, `"UNKNOWN"`, `"PENDING"`. Kept as a free-form
    /// string here so the keystore crate doesn't have to evolve
    /// alongside the keyserver's state machine; the consuming
    /// layer (F2.2's `LicenseState`) does the mapping.
    pub status: String,
    /// Unix seconds. `None` when the subscription is `PENDING` /
    /// `UNKNOWN`, or (legacy, pre-F2.0) when the keyserver hadn't
    /// stamped a period yet under the old Stripe API shape.
    #[serde(default)]
    pub current_period_end: Option<i64>,
    /// `true` iff the HMAC checksum on the 14-char license body
    /// matched. `false` is the cheap "user mistyped" gate — the
    /// client SHOULD distinguish this from a legitimately unknown
    /// key when surfacing UI text.
    pub checksum_ok: bool,
}

#[derive(Serialize)]
struct BurnRequest<'a> {
    scope: &'a str,
    user_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_content_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_user_id: Option<&'a str>,
    timestamp_ms: i64,
    request_id: String,
    burn_signature_b64: String,
}

#[derive(Serialize)]
struct ReplenishRequest {
    user_id: String,
    timestamp_ms: i64,
    request_id: String,
    batch_signature_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    spk: Option<ReplenishSpkWire>,
    opks: Vec<ReplenishOpkWire>,
}

#[derive(Serialize)]
struct ReplenishSpkWire {
    pub_b64: String,
    signature_b64: String,
    rotated_at: String,
}

#[derive(Serialize)]
struct ReplenishOpkWire {
    id: u32,
    pub_b64: String,
}

#[derive(Serialize)]
struct WrappedKeyPostRequest<'a> {
    #[serde(flatten)]
    upload: &'a WrappedKeyUpload,
    sender_id: &'a str,
    timestamp_ms: i64,
    sender_signature_b64: String,
}

/// HTTP client for the OSL key server.
///
/// Holds the canonicalised base URL, a `reqwest::blocking::Client`
/// with rustls TLS configured. Public-client mutations are authorized
/// by fresh registered-identity Ed25519 signatures; the open-source
/// desktop client never embeds or transmits a shared bearer secret.
/// `Clone` is cheap: `reqwest::blocking::Client` is `Arc`-backed, so
/// a clone shares the same connection pool. Callers clone the client
/// out from under the `AppState` keyserver mutex so network calls
/// don't hold that lock (see the Phase 6.4 control-inbox drain).
#[derive(Clone)]
pub struct KeyServerClient {
    /// Canonicalised base URL. Includes scheme + host + optional
    /// port + optional base path. Trailing `/` stripped so callers
    /// can `format!("{}{}", base_url, "/v1/foo")` without
    /// double-slash hazards.
    base_url: String,
    client: reqwest::blocking::Client,
}

impl KeyServerClient {
    /// Release builds accept only the exact production HTTPS origin. Debug and
    /// test builds additionally accept numeric loopback HTTP(S) endpoints.
    ///
    /// The underlying `reqwest::blocking::Client` is built with a
    /// 30-second timeout (matching the prior hand-rolled
    /// transport's behaviour) and rustls-tls + webpki-roots
    /// (Mozilla CA bundle). No certificate pinning — that's a
    /// v1-stable feature.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let url = base_url.as_ref();
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| Error::Transport(format!("invalid base_url {url:?}: {e}")))?;

        let no_ambient_authority = parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none();
        let production = no_ambient_authority
            && parsed.scheme() == "https"
            && parsed.host_str() == Some("keyserver.oslprivacy.com")
            && parsed.port_or_known_default() == Some(443)
            && parsed.path() == "/";
        let debug_loopback = cfg!(debug_assertions)
            && no_ambient_authority
            && matches!(parsed.scheme(), "http" | "https")
            && parsed
                .host_str()
                .and_then(|host| {
                    host.trim_start_matches('[')
                        .trim_end_matches(']')
                        .parse::<std::net::IpAddr>()
                        .ok()
                })
                .is_some_and(|ip| ip.is_loopback());
        if !production && !debug_loopback {
            return Err(Error::Transport(
                "keyserver origin is not trusted for this build".to_string(),
            ));
        }

        let base_url = parsed.as_str().trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            // Never follow an origin-changing redirect with signed protocol
            // bodies. Production is already HTTPS and local tests do not need
            // an upgrade redirect.
            .redirect(reqwest::redirect::Policy::none())
            // User-Agent string mirrors the prior hand-rolled
            // value so server-side log greps continue working.
            .user_agent("discord-privacy-client/0.0.1")
            .build()
            .map_err(|e| Error::Transport(format!("reqwest client build: {e}")))?;
        Ok(KeyServerClient { base_url, client })
    }

    /// Compatibility shim for pre-signed-mutation callers. The retired
    /// client bearer is deliberately discarded and never placed on the wire.
    pub fn with_client_token(self, _token: Option<String>) -> Self {
        self
    }

    /// Build the registration request body for `identity`, signed
    /// with `identity.ed25519_secret` over `REG_MSG`.
    pub fn build_register_request(identity: &Identity) -> RegisterRequest {
        let ik_x25519_pub = STANDARD.encode(identity.x25519_public.as_bytes());
        let ik_ed25519_pub = STANDARD.encode(identity.ed25519_public.as_bytes());
        let ik_mlkem768_pub = STANDARD.encode(identity.mlkem_public_bytes);
        let ik_ratchet_initial_pub = identity
            .ratchet_initial_pub
            .as_ref()
            .map(|p| STANDARD.encode(p.as_bytes()));
        let msg = reg_msg(
            &identity.user_id,
            &ik_x25519_pub,
            &ik_ed25519_pub,
            &ik_mlkem768_pub,
            ik_ratchet_initial_pub.as_deref(),
        );
        let sig = crypto::ed25519::sign(&identity.ed25519_secret, &msg);
        RegisterRequest {
            user_id: identity.user_id.clone(),
            ik_x25519_pub,
            ik_ed25519_pub,
            ik_mlkem768_pub,
            registration_sig: STANDARD.encode(sig.as_bytes()),
            rotation: None,
            ik_ratchet_initial_pub,
        }
    }

    /// Build a Case-C rotation request: move `user_id` from
    /// `old_identity`'s Ed25519 key onto `new_identity`'s keys.
    /// `registration_sig` is signed by the NEW key (proof of
    /// possession); `rotation.prev_sig` is signed by the OLD key
    /// (authorises the change). `user_id` is taken from
    /// `old_identity` (the registered identifier is immutable across
    /// a key rotation).
    ///
    /// PRESENT-BUT-UNEXERCISED: no current caller. The beta
    /// first-launch flow only ever produces Case A/B requests; this
    /// exists so the wire shape is ready and tested before any
    /// rotation UX lands. (It also requires the caller to still hold
    /// the OLD identity secret, which the current app does not
    /// retain after a key change — wiring that is out of scope here.)
    pub fn build_rotation_request(
        old_identity: &Identity,
        new_identity: &Identity,
    ) -> RegisterRequest {
        let user_id = old_identity.user_id.clone();
        let new_x = STANDARD.encode(new_identity.x25519_public.as_bytes());
        let new_ed = STANDARD.encode(new_identity.ed25519_public.as_bytes());
        let new_mlkem = STANDARD.encode(new_identity.mlkem_public_bytes);
        let new_ratchet = new_identity
            .ratchet_initial_pub
            .as_ref()
            .map(|p| STANDARD.encode(p.as_bytes()));
        let prev_ed = STANDARD.encode(old_identity.ed25519_public.as_bytes());

        // new key proves possession over REG_MSG (Case-A/B shape).
        let reg = reg_msg(
            &user_id,
            &new_x,
            &new_ed,
            &new_mlkem,
            new_ratchet.as_deref(),
        );
        let reg_sig = crypto::ed25519::sign(&new_identity.ed25519_secret, &reg);
        // old key authorises the change over ROT_MSG.
        let rot = rot_msg(
            &user_id,
            &prev_ed,
            &new_x,
            &new_ed,
            &new_mlkem,
            new_ratchet.as_deref(),
        );
        let prev_sig = crypto::ed25519::sign(&old_identity.ed25519_secret, &rot);

        RegisterRequest {
            user_id,
            ik_x25519_pub: new_x,
            ik_ed25519_pub: new_ed,
            ik_mlkem768_pub: new_mlkem,
            registration_sig: STANDARD.encode(reg_sig.as_bytes()),
            rotation: Some(RotationProof {
                prev_ik_ed25519_pub: prev_ed,
                prev_sig: STANDARD.encode(prev_sig.as_bytes()),
            }),
            ik_ratchet_initial_pub: new_ratchet,
        }
    }

    /// `POST /v1/register`.
    pub fn register(&self, identity: &Identity) -> Result<RegisterResponse> {
        let body = Self::build_register_request(identity);
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/register",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// `POST /v1/register` presenting a pre-signed Case-C rotation
    /// proof.
    ///
    /// The body is the Case-A/B request for the CURRENT identity
    /// (`registration_sig` = current key over `REG_MSG`, proving
    /// possession of the key being rotated to), with `rotation` set
    /// to the persisted [`crate::PendingRotation`] (`prev_sig` =
    /// the OLD key's authorization over `ROT_MSG`, minted at burn
    /// time while the old identity still existed). This is what lets
    /// a burn re-publish onto a `user_id` the server already holds
    /// under the destroyed old key.
    ///
    /// `register` is intentionally left unchanged; callers without a
    /// pending proof keep using it.
    pub fn register_with_rotation(
        &self,
        identity: &Identity,
        proof: &crate::pending_rotation::PendingRotation,
    ) -> Result<RegisterResponse> {
        let mut body = Self::build_register_request(identity);
        body.rotation = Some(RotationProof {
            prev_ik_ed25519_pub: proof.prev_ik_ed25519_pub.clone(),
            prev_sig: proof.prev_sig.clone(),
        });
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/register",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// `GET /v1/pubkeys/:user_id`.
    pub fn fetch_pubkeys(&self, user_id: &str) -> Result<PubkeysResponse> {
        let path = format!("/v1/pubkeys/{}", urlencode_segment(user_id));
        let resp = self.send_request("GET", &path, None)?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// `GET /v1/prekey-bundle/:user_id`. Atomically pops one OPK
    /// server-side; the popped value rides in the response. Pool
    /// remaining count surfaces via `remaining_opk_count` so the
    /// caller can decide whether to replenish. The requester signs
    /// actor + recipient/target + timestamp; the client-wide bearer
    /// is deliberately not used as identity authority.
    pub fn fetch_prekey_bundle(
        &self,
        requester: &Identity,
        recipient_user_id: &str,
    ) -> Result<PrekeyBundleResponse> {
        let timestamp_ms = unix_timestamp_ms();
        let signature = sign_prekey_bundle_get(requester, recipient_user_id, timestamp_ms);
        let sig_b64 = STANDARD.encode(signature.as_bytes());
        let path = format!(
            "/v1/prekey-bundle/{}?requester_id={}&recipient_id={}&ts={}&sig={}",
            urlencode_segment(recipient_user_id),
            urlencode_query_value(&requester.user_id),
            urlencode_query_value(recipient_user_id),
            timestamp_ms,
            urlencode_query_value(&sig_b64),
        );
        let resp = self.send_request("GET", &path, None)?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// Authenticated wrapped-key fetch. Only the intended recipient can
    /// sign this request; a single-use row is atomically deleted by the
    /// server only after verification succeeds.
    pub fn fetch_wrapped_key(
        &self,
        recipient: &Identity,
        content_id: &str,
    ) -> Result<WrappedKeyResponse> {
        let timestamp_ms = unix_timestamp_ms();
        let signature = sign_wrapped_key_get(recipient, content_id, timestamp_ms);
        let sig_b64 = STANDARD.encode(signature.as_bytes());
        let path = format!(
            "/v1/wrapped-keys/{}?requester_id={}&recipient_id={}&ts={}&sig={}",
            urlencode_segment(content_id),
            urlencode_query_value(&recipient.user_id),
            urlencode_query_value(&recipient.user_id),
            timestamp_ms,
            urlencode_query_value(&sig_b64),
        );
        let resp = self.send_request("GET", &path, None)?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// Upload an opaque wrapped share authorized by the registered sender's
    /// Ed25519 identity. No client-wide bearer is required or trusted.
    pub fn post_wrapped_key(
        &self,
        sender: &Identity,
        upload: &WrappedKeyUpload,
    ) -> Result<WrappedKeyPostResponse> {
        let timestamp_ms = unix_timestamp_ms();
        let signature = sign_wrapped_key_post(sender, upload, timestamp_ms);
        let body = WrappedKeyPostRequest {
            upload,
            sender_id: &sender.user_id,
            timestamp_ms,
            sender_signature_b64: STANDARD.encode(signature.as_bytes()),
        };
        let body_json = serde_json::to_vec(&body)?;
        let response = self.send_request(
            "POST",
            "/v1/wrapped-keys",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&response)?;
        Ok(serde_json::from_slice(&response.body)?)
    }

    /// `POST /v1/prekey-bundle/replenish`. Signs the canonical batch
    /// bytes with `identity.ed25519_secret` and ships them along
    /// with the new SPK (if any) and the OPK batch.
    pub fn replenish_prekeys(
        &self,
        identity: &Identity,
        spk: Option<&SpkEntry>,
        opks: &[OpkEntry],
    ) -> Result<ReplenishResponse> {
        let replenish_spk = spk.map(|s| ReplenishSpk {
            pub_b64: STANDARD.encode(s.public),
            signature_b64: STANDARD.encode(s.signature),
            rotated_at: crate::prekeys::iso_8601_from_unix_seconds(s.rotated_at_unix_seconds),
        });
        let replenish_opks: Vec<ReplenishOpk> = opks
            .iter()
            .map(|o| ReplenishOpk {
                id: o.id,
                pub_b64: STANDARD.encode(o.public),
            })
            .collect();
        let timestamp_ms = unix_timestamp_ms();
        let request_id = fresh_request_id();
        let sig = sign_replenish_batch(
            identity,
            &identity.user_id,
            timestamp_ms,
            &request_id,
            replenish_spk.as_ref(),
            &replenish_opks,
        );

        // Build the wire body. Mirrors `keyserver/src/server.js`'s
        // /v1/prekey-bundle/replenish handler.
        let body = ReplenishRequest {
            user_id: identity.user_id.clone(),
            timestamp_ms,
            request_id,
            batch_signature_b64: STANDARD.encode(sig.as_bytes()),
            spk: replenish_spk.map(|r| ReplenishSpkWire {
                pub_b64: r.pub_b64,
                signature_b64: r.signature_b64,
                rotated_at: r.rotated_at,
            }),
            opks: replenish_opks
                .into_iter()
                .map(|o| ReplenishOpkWire {
                    id: o.id,
                    pub_b64: o.pub_b64,
                })
                .collect(),
        };
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/prekey-bundle/replenish",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// Convenience: full client-side replenish flow given a
    /// [`PrekeyState`]. Generates fresh OPKs to top up to the
    /// configured target, optionally rotates the SPK if due, signs
    /// + uploads, and updates the local state.
    pub fn replenish_using_state(
        &self,
        identity: &Identity,
        state: &mut PrekeyState,
        server_remaining: u32,
        now_unix_seconds: u64,
    ) -> Result<ReplenishResponse> {
        let spk_to_send = if state.should_rotate_spk(now_unix_seconds) {
            Some(state.rotate_spk(identity, now_unix_seconds).clone())
        } else {
            None
        };
        let to_add = state.replenish_count_to_target(server_remaining);
        let new_opks_owned: Vec<OpkEntry> = if to_add > 0 {
            state.add_opk_batch(to_add).to_vec()
        } else {
            Vec::new()
        };
        self.replenish_prekeys(identity, spk_to_send.as_ref(), &new_opks_owned)
    }

    /// `DELETE /v1/wrapped-keys`. Signs the canonical burn bytes
    /// with the identity's Ed25519 key. The server filters
    /// `sender_id = identity.user_id` so this only ever deletes the
    /// caller's own rows. Returns `(scope, deleted_count)`.
    pub fn burn(&self, identity: &Identity, scope: &BurnScope) -> Result<BurnResponse> {
        let timestamp_ms = unix_timestamp_ms();
        let request_id = fresh_request_id();
        let sig = sign_burn(identity, timestamp_ms, &request_id, scope);
        let sig_b64 = STANDARD.encode(sig.as_bytes());
        let (target_content_id, target_user_id) = match scope {
            BurnScope::Single { content_id } => (Some(content_id.as_str()), None),
            BurnScope::ToUser { user_id } => (None, Some(user_id.as_str())),
            BurnScope::All => (None, None),
        };
        let body = BurnRequest {
            scope: scope.label(),
            user_id: identity.user_id.as_str(),
            target_content_id,
            target_user_id,
            timestamp_ms,
            request_id,
            burn_signature_b64: sig_b64,
        };
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "DELETE",
            "/v1/wrapped-keys",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// `DELETE /v1/pubkeys/:user_id` — account-burn unregister.
    ///
    /// Signs canonical `(domain || user_id || timestamp_ms)` with
    /// the OLD identity's Ed25519 secret and POSTs the body. The
    /// server cascades the delete across users / wrapped_keys /
    /// prekey_bundles / opk_pool. Idempotent on the server side
    /// (returns `noop` if the user was never registered or already
    /// deleted), so re-running the burn after a partial failure is
    /// safe.
    ///
    /// MUST be called BEFORE the local identity files are wiped —
    /// the signing requires the OLD ed25519_secret, which is gone
    /// after `cmd_osl_fresh_start`.
    pub fn unregister(&self, identity: &Identity) -> Result<()> {
        let timestamp_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let sig = sign_unregister(identity, timestamp_ms);
        let sig_b64 = STANDARD.encode(sig.as_bytes());
        self.unregister_signed(&identity.user_id, &sig_b64, timestamp_ms)
    }

    /// Pre-signed variant of [`Self::unregister`]. Used when the
    /// identity lives behind a Mutex — caller signs inside the lock
    /// guard (sign returns `Vec<u8>` which is Send) then drops the
    /// lock before issuing the blocking HTTP request.
    pub fn unregister_signed(
        &self,
        user_id: &str,
        signature_b64: &str,
        timestamp_ms: i64,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct UnregisterBody<'a> {
            signature_b64: &'a str,
            timestamp_ms: i64,
        }
        let body = UnregisterBody {
            signature_b64,
            timestamp_ms,
        };
        let body_json = serde_json::to_vec(&body)?;
        let path = format!("/v1/pubkeys/{}", urlencode_segment(user_id));
        let resp = self.send_request("DELETE", &path, Some(("application/json", &body_json)))?;
        check_2xx(&resp)?;
        Ok(())
    }

    /// Phase 6.4: enqueue a control-message wire into the
    /// recipient's keyserver inbox. The bundle is the same opaque
    /// v=3/v=4 wire we used to POST to Discord channels via
    /// oslSendControlMessage; the keyserver server-side stores it
    /// and the recipient drains via [`Self::get_control_inbox`].
    /// Signed by the sender's identity ed25519.
    pub fn post_control_inbox(
        &self,
        sender: &Identity,
        recipient_id: &str,
        scope_id: &str,
        bundle: &[u8],
    ) -> Result<ControlInboxPostResponse> {
        let timestamp_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let sig = sign_control_inbox_post(sender, recipient_id, scope_id, timestamp_ms, bundle);
        let body = ControlInboxPostBody {
            sender_id: &sender.user_id,
            recipient_id,
            scope_id,
            bundle_b64: STANDARD.encode(bundle),
            timestamp_ms,
            signature_b64: STANDARD.encode(sig.as_bytes()),
        };
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/control-inbox",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// Phase 6.4: drain the caller's own control-message inbox.
    /// Returns up to MAX_DRAIN_ROWS items in FIFO order. Each
    /// returned item carries an inbox `id` the caller MUST
    /// [`Self::delete_control_inbox`] after applying so the row
    /// doesn't re-appear on the next poll.
    pub fn get_control_inbox(&self, identity: &Identity) -> Result<Vec<ControlInboxItem>> {
        let timestamp_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let sig = sign_control_inbox_get(identity, timestamp_ms);
        let sig_b64 = STANDARD.encode(sig.as_bytes());
        // ed25519 sigs are 64 bytes -> 88 base64 chars; URL-encode
        // them since `+` / `/` would otherwise corrupt the query.
        let sig_q = urlencode_query_value(&sig_b64);
        let path = format!(
            "/v1/control-inbox/{}?ts={}&sig={}",
            urlencode_segment(&identity.user_id),
            timestamp_ms,
            sig_q,
        );
        let resp = self.send_request("GET", &path, None)?;
        check_2xx(&resp)?;
        let parsed: ControlInboxGetResponse = serde_json::from_slice(&resp.body)?;
        Ok(parsed.items)
    }

    /// Phase 6.4: delete a specific inbox row after the caller has
    /// successfully applied the bundle. Idempotent.
    pub fn delete_control_inbox(&self, identity: &Identity, inbox_id_hex: &str) -> Result<()> {
        let timestamp_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let sig = sign_control_inbox_delete(identity, inbox_id_hex, timestamp_ms);
        let body = ControlInboxDeleteBody {
            user_id: &identity.user_id,
            timestamp_ms,
            signature_b64: STANDARD.encode(sig.as_bytes()),
        };
        let body_json = serde_json::to_vec(&body)?;
        let path = format!("/v1/control-inbox/{}", urlencode_segment(inbox_id_hex));
        let resp = self.send_request("DELETE", &path, Some(("application/json", &body_json)))?;
        check_2xx(&resp)?;
        Ok(())
    }

    /// `POST /v1/license/validate`. Public endpoint — no admin
    /// token attached even when one is configured (the keyserver
    /// rate-limits per IP for this route). Returns a
    /// [`LicenseValidateResponse`] on HTTP 200 regardless of the
    /// license's actual `status`; the meaning lives in the body.
    ///
    /// Error mapping (load-bearing for F2.4 offline grace):
    ///   - network / TLS / DNS failure → [`Error::Transport`]
    ///   - rate limit (429) / unexpected non-2xx → [`Error::HttpStatus`]
    ///   - 200 with unparseable JSON → [`Error::Json`]
    ///
    /// `Error::Transport` is the "keyserver unreachable" case
    /// F2.4 honours via the cached state + 7-day grace. The
    /// other variants mean the keyserver answered — the cached
    /// state should be considered stale (or in the case of
    /// `HttpStatus 429`, retried later with backoff).
    pub fn validate_license(&self, license_plaintext: &str) -> Result<LicenseValidateResponse> {
        let body = LicenseValidateRequest {
            license_key: license_plaintext,
        };
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/license/validate",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// Issue an HTTP request via the underlying reqwest client.
    /// Wraps reqwest's typed errors in [`Error::Transport`] (low-level
    /// transport / TLS / serialization issues) and
    /// [`Error::HttpStatus`] (server returned non-2xx).
    ///
    /// `path` MUST start with `/` and SHOULD be URL-encoded by the
    /// caller for any `:user_id`-shaped segments (see
    /// [`urlencode_segment`]). reqwest re-parses the resulting
    /// `base_url + path` string into a [`reqwest::Url`] without
    /// double-encoding pre-encoded sequences.
    ///
    /// Authorization is operation-specific. Identity-sensitive mutations and
    /// consuming reads carry canonical Ed25519 signatures in their payloads;
    /// this generic transport never adds ambient authorization.
    fn send_request(
        &self,
        method: &str,
        path: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<HttpResponse> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = match method {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            other => {
                return Err(Error::Transport(format!(
                    "unsupported HTTP method: {other:?}"
                )));
            }
        };
        req = req.header("Accept", "application/json");
        if let Some((ctype, payload)) = body {
            req = req.header("Content-Type", ctype).body(payload.to_vec());
        }

        let response = req
            .send()
            .map_err(|e| Error::Transport(format!("send {method} {url}: {e}")))?;
        let status = response.status().as_u16();
        let body_bytes = response
            .bytes()
            .map_err(|e| Error::Transport(format!("read response body: {e}")))?
            .to_vec();
        Ok(HttpResponse {
            status,
            body: body_bytes,
        })
    }
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn check_2xx(resp: &HttpResponse) -> Result<()> {
    if (200..300).contains(&resp.status) {
        Ok(())
    } else {
        Err(Error::HttpStatus {
            status: resp.status,
            body: String::from_utf8_lossy(&resp.body).to_string(),
        })
    }
}

/// Encode each byte that isn't an unreserved URL character as %XX.
/// Used for the `:user_id` path segment.
///
/// We pre-encode rather than relying on reqwest's URL parser
/// because callers compose the path string before calling
/// [`KeyServerClient::send_request`]; the encoded result round-
/// trips through `Url::parse` unchanged (parser preserves existing
/// `%XX` escapes, doesn't re-encode them).
fn urlencode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b;
        let unreserved =
            c.is_ascii_alphanumeric() || c == b'-' || c == b'.' || c == b'_' || c == b'~';
        if unreserved {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

/// URL-encode a query-string value. Same unreserved-set as
/// `urlencode_segment`. Used by the control-inbox GET which
/// passes the ed25519 signature as `?sig=...` (raw base64 has `+`
/// and `/` which would corrupt the query parser).
fn urlencode_query_value(s: &str) -> String {
    urlencode_segment(s)
}

fn unix_timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn fresh_request_id() -> String {
    URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(32))
}

// ---- Phase 6.4 control-inbox payload shapes (used by post_control_inbox /
// get_control_inbox / delete_control_inbox above). Shapes mirror
// keyserver-cf/src/endpoints/control-inbox.ts. ----

#[derive(Serialize)]
struct ControlInboxPostBody<'a> {
    sender_id: &'a str,
    recipient_id: &'a str,
    scope_id: &'a str,
    bundle_b64: String,
    timestamp_ms: i64,
    signature_b64: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ControlInboxPostResponse {
    pub id: String,
    pub expires_at: i64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ControlInboxItem {
    pub id: String,
    pub sender_id: String,
    pub scope_id: String,
    pub bundle_b64: String,
    pub created_at: i64,
}

#[derive(Deserialize)]
struct ControlInboxGetResponse {
    items: Vec<ControlInboxItem>,
}

#[derive(Serialize)]
struct ControlInboxDeleteBody<'a> {
    user_id: &'a str,
    timestamp_ms: i64,
    signature_b64: String,
}
