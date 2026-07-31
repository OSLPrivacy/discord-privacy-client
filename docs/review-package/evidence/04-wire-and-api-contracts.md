# Evidence 04 — wire and API contracts

Supports README §8 (`wire_api_contract_evidence`).

All citations are `path:line` against the working tree pinned by `reviewed-file-hashes.txt`.
**This is source review.** No wire byte was produced or parsed by a running product in the course
of assembling this package; the only execution was the three test suites in `01-test-runs.md`.

---

## 1. `crates/ipc/src/wire_v2.rs` — the wire that actually ships

### Version and type namespace

| Constant | Value | Line | Meaning |
|---|---|---|---|
| `WIRE_VERSION_V2` | `0x02` | `:85` | legacy |
| `WIRE_VERSION_V3` | `0x03` | `:91` | **PQ-hybrid wrap — what ships** |
| `WIRE_VERSION_V4` | `0x04` | `:99` | ratcheted single-recipient (adds an 84-byte ratchet header) |
| `WIRE_VERSION_V5` | `0x05` | `:107` | sender-keys multi-recipient |

Message types (`:111-214`): `CONTENT 0x00`, `BURN 0x01`, `ATTACHMENT 0x04`,
`SENDER_KEY_DISTRIBUTION 0x05`, `SKDM_REQUEST 0x06`, `SESSION_RESET 0x07`,
`NATIVE_OVERLAY_RELAY 0x08`, `NATIVE_OVERLAY_ACK 0x09`, `REVOCATION 0x0A`, `REVOCATION_ACK 0x0B`.

Domain separators (`:273-275`): `AD_WRAP_V3 = b"OSL/A1/wrap/v3"`,
`AD_BODY_V3 = b"OSL/A1/body/v3"`, `HKDF_INFO_WRAP_V3 = b"OSL/A1/wrap-key/v3"`.
Layout constants: `RECIPIENT_HASH_PREFIX_LEN = 8` (`:250`), `SLOT_V3_BYTES` (`:331`),
`V4_GLOBAL_HEADER_BYTES = 1+1+1+32+1` (`:308`), `V5_GLOBAL_HEADER_BYTES = 1+1+32+1` (`:295`).

### `encrypt_v3` — `:685-770`

```rust
encrypt_v3(
    sender_ik_sk: &x25519::SecretKey,
    sender_ik_pub: &x25519::PublicKey,
    recipients: &[RecipientV3],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, V2Error>            // :685-691
```

- Fresh random 32-byte body key `K` (`:704-708`); body sealed **once** with **AES-256-GCM**
  (12-byte nonce, 16-byte tag) under AD `AD_BODY_V3` (`:710`).
- Per recipient: `pqxdh::initiate(sender_ik_sk, recip.x25519_pub /*as ik*/,
  recip.x25519_pub /*as spk*/, None /*opk*/, recip.mlkem_pub)` — **X25519 + ML-KEM-768 → HKDF
  combiner** (`:729-736`); `wrap_key = hkdf::derive_32([], session_key, HKDF_INFO_WRAP_V3)`
  (`:738`); `K` sealed under `wrap_key` with AES-256-GCM, AD `AD_WRAP_V3` (`:741`).
- Slot layout (`:752-760`):
  `pubkey_hash_prefix(8) ‖ ek_x25519_pub(32) ‖ ct_len(u16 LE) ‖ mlkem_ct ‖ wrap_nonce ‖ wrap_ct`.
- Global header (`:721-724`): `version(0x03) ‖ msg_type ‖ sender_ik_x25519_pub(32) ‖ N`.
  Trailer (`:762-763`): `body_nonce(12) ‖ body_ct+tag`. Output `"DPC0::" + base64` (`:765-770`).
- Max 255 recipients (`:697-702`). `RecipientV3 { x25519_pub, mlkem_pub }` (`:668-672`).
- `decrypt_v3` `:778`; sender self-decrypt `decrypt_v3_for_sender` `:790`.
- Cipher choice rationale (AES-256-GCM over XChaCha20-Poly1305 for both legs) at `:74-78`.

**It is a key wrap, not a ratchet. No forward secrecy, no post-compromise security.**
Confirmed in-tree by `crates/ipc/src/wire_rn.rs:394` ("The existing PQ-hybrid wrap. No forward
secrecy.").

**This is the highest-value target for the reviewing firm**, because it is the only wire format
carrying real user data (see `02-reachability-and-caller-search.md`).

---

## 2. `crates/osl-ratchet-next` — OSL-RN, wire `0x10` (unwired)

### Companion documents

`crates/osl-ratchet-next/DESIGN.md` (607 lines) — §1 problem, §2 design in one page (incl. "Prior
art, stated plainly" `:82`), §3 key schedule (`:97`), §4 the PQ epoch ratchet (`:169`), §5 wire
format `0x10` (`:236`, version coexistence `:278`), §6 ciphertext budget measured (`:294`),
§6a carrier cost in Discord messages (`:327`), §7 state machine (`:378`), §8 bounds (`:444`),
§9 two real bugs the tests caught (`:472`), §10 claims table (`:500`), §11 bounds stated
explicitly (`:552`), §12 deliberately not done (`:570`), §13 test inventory (`:590`).

`THREAT-MODEL.md` (233 lines) — §1 what it defends (`:13`); §2 assumptions the crate does **not**
enforce (`:31`: peer key authenticity `:33`, one-time prekey lifecycle `:47`, randomness `:58`,
state-at-rest `:68`); §3 endpoint/platform (`:78`); §4 traffic analysis (`:101`); §5 cryptographic
non-defences (`:122`); §6 availability (`:187`); §7 out of scope (`:211`); §8 the dominating
residual risk (`:224`).

`MIGRATION.md` (452 lines) — **§1 is stale, see README §5.** §0 exec summary (`:13`), §1 version
coexistence (`:24`, incl. "Capability advertisement is absent end-to-end" `:136` — **false in this
tree**), §2 call sites that change (`:166`), §3 state persistence (`:235`), §4 what changes on the
Discord path (`:262`, incl. "4.1 Re-decryption. **This is the blocker.**" `:284`), §5 ciphertext
budget (`:376`), §6 suggested order of work (`:406`), §7 the interface (`:426`).

### Framing constants (`src/session.rs`)

```
WIRE_VERSION_RN        = 0x10        :148
RN_FLAG_BOOTSTRAP      = 0x01        :151
RN_FLAG_RESERVED_MASK  = 0xFE        :154
WIRE_PREFIX            = "DPC0::"    :157
MAX_WIRE_BYTES         = 64 KiB      :160
MAX_HEADER_CT_BYTES    = 1024        :162
STATE_FORMAT_VERSION   = 1           :888
```

Version-byte namespace reasoning (why `0x06` was rejected — it collides with
`wire_v2::MSG_TYPE_SKDM_REQUEST`) at `src/session.rs:87-148` and DESIGN.md `:278-292`.

### Public API (`src/lib.rs`)

Modules (`:53-63`): `api, codec, error, handshake, kdf, negotiate, pq, primitives, session,
skipped, test_support`.

Re-exports: `accept_rn, accept_rn_bound, decrypt_rn, encrypt_rn, initiate_rn_bound,
peek_wire_version, SecureSession` (`:65-68`); `Error, Result` (`:69`); `LocalPrekeys, PeerBundle`
(`:70`); `Negotiation` (`:71`); `PqParams, Role` (`:72`); `KemPublic, KemSecret, XPublic, XSecret,
MLKEM_CT, MLKEM_DK, MLKEM_EK` (`:73`); `peek_bootstrap_initiator_identity, Opened, Session,
SessionParams, WIRE_VERSION_RN` (`:74-76`); `SkipParams` (`:77`).

Crate lints (`:48-51`): `#![forbid(unsafe_code)]`,
`deny(clippy::indexing_slicing / unwrap_used / panic)`.
Banner (`:3`): **"UNREVIEWED. NOT WIRED IN. DO NOT CARRY REAL TRAFFIC."**

### Primitives (`src/primitives.rs`)

- X25519 — `x25519-dalek` 2.0, RFC 7748 (`:3`); `X25519_PUB/SEC = 32` (`:42-43`);
  **contributory-behaviour check**: all-zero shared secret rejected in constant time via
  `subtle::ConstantTimeEq` (`:15-20`), `dh()` at `:142`
- ML-KEM-768 — FIPS 203, RustCrypto `ml-kem` 0.2 (`:4`); `MLKEM_EK = 1184` (`:53`),
  `MLKEM_DK = 2400` (`:54`), `MLKEM_CT = 1088` (`:55`); implicit-rejection note (`:22-28`);
  `kem_keypair` `:233`, `kem_encapsulate` `:239`, `kem_decapsulate` `:257`
- AEAD — XChaCha20-Poly1305; `aead_seal` `:293`, `aead_open` `:313`; `AEAD_KEY = 32` (`:44`),
  `AEAD_NONCE = 24` (`:49`), `AEAD_TAG = 16` (`:50`); `HEADER_NONCE_WIRE = 12` (`:51`)
- HKDF-SHA256 — `hkdf` + `sha2`, RFC 5869 (`:5`), generic `hkdf::<N>` (`:278`), RFC 5869 A.1
  vector at `:334+`

### Key schedule (`src/kdf.rs`)

Labels (`:54-61`): `LABEL_HANDSHAKE = b"OSL-RN/v1/pqxdh-root"`, `LABEL_ROOT = b"OSL-RN/v1/root"`,
`LABEL_MK = b"OSL-RN/v1/message-key"`, `LABEL_CK = b"OSL-RN/v1/chain-key"`,
`LABEL_BODY_NONCE = b"OSL-RN/v1/body-nonce"`, `LABEL_HEADER_INIT = b"OSL-RN/v1/header-key-init"`,
`LABEL_SESSION_ID = b"OSL-RN/v1/session-id"`, `LABEL_STATE_KEY = b"OSL-RN/v1/state-export"`.
IKM constants (`:63-65`): `IKM_MK = b"mk"`, `IKM_CK = b"ck"`, `IKM_NONCE = b"nonce"`.

- `root_step` (`:87-121`) — HKDF-SHA256 producing 96 bytes split `root_key ‖ chain_key ‖ header_key`
  (`:104-118`); info = `LABEL_ROOT ‖ mix_epoch.to_le_bytes()` (`:101-103`)
- `chain_step` (`:123-127`)
- `expand()` (`src/handshake.rs:134-151`) derives root, two header keys, session id, and
  `pq_epoch0` (`b"OSL-RN/v1/pq-epoch-0"`)

### Handshake (`src/handshake.rs`)

PQXDH-shaped. Initiator (`:207-214`): `dh1 = DH(IK_a, SPK_b)`, `dh2 = DH(EK_a, IK_b)`,
`dh3 = DH(EK_a, SPK_b)`, optional `dh4 = DH(EK_a, OPK_b)`, plus
`ss = ML-KEM-768.Encaps(PQ_prekey_b)`.
`SK = HKDF(salt = [], ikm = dh1‖dh2‖dh3‖dh4?‖ss, info = handshake_info(binding))` (`:216-226`).
Responder mirror at `:258-266`. OPK id `0` means "none" (`:250-252`).
`PREAMBLE_MIN_BYTES = 32 + 32 + 1 + MLKEM_CT` (`:95`).

### Header encryption

- Per-chain header keys `hks` / `hkr` from the root step (above).
- Header nonce: 12 random bytes on the wire, expanded with the constant prefix
  `HEADER_NONCE_PREFIX = *b"OSL-RN/hdr\x00\x01"` (`src/kdf.rs:73`, expansion `:135-139`).
- AD chaining: header AD = `AD_HEADER(b"OSL-RN/v1/header") ‖ version ‖ flags ‖ preamble?`
  (`src/session.rs:541-546`); body AD = `AD_BODY(b"OSL-RN/v1/body") ‖ <everything written so far>`
  (`:552-554`).
- Encrypted header plaintext: `msg_type(1) dh_pub(32) pn n pq_have mix_epoch (varints)
  has_fragment(1) [fragment]` — `RatchetHeader::encode` (`src/session.rs:192-211`),
  DESIGN.md `:253-257`.
- Trial header decryption bounded at `2 + max_chains` = **7** AEAD opens (DESIGN.md `:451`).

### Body nonce — a deliberate design choice with a stated consequence

`body_nonce = HKDF(salt = MK, ikm = "nonce", info = LABEL_BODY_NONCE)[0..24]`
(`src/kdf.rs:130-133`), used at `src/session.rs:555`.

**It is fully deterministic in the message key.** Therefore *any* message-key reuse is a nonce
reuse. The mitigation in the integration layer is that `send_rn` persists advanced state **before**
returning the wire. The restore-from-backup / VM-snapshot case is **not closed**. See README §9.3.

### Bounds

| Struct | Field | Default | Line |
|---|---|---|---|
| `SkipParams` (`src/skipped.rs:66-73`) | `max_skip_per_message` | 512 | `:75-85` |
| | `max_keys_per_chain` | 512 | |
| | `max_total_keys` | 2048 | |
| | `max_chains` | 5 | |
| | `max_age` | 100_000 | |
| `PqParams` (`src/pq.rs:115-121`) | `fragment_bytes` | 128 | `:123-130` |
| | `rekey_interval` | 64 | |

`SkipParams::validated()` rejects zeros and `max_keys_per_chain > max_total_keys` (`:87-99`);
`PqParams::validated()` at `src/pq.rs:132-143`.
`SessionParams { pq, skip }` derives `Default` (`src/session.rs:173-178`).

Further PQ bounds (`src/pq.rs`): `FRAG_KIND_EK = 1` (`:97`), `FRAG_KIND_CT = 2` (`:99`),
`MAX_FRAGMENTS = 64` (`:104`), `MAX_FRAGMENT_BYTES = 512` (`:106`),
`MAX_EPOCH_LOOKAHEAD = 2` (`:108`), `MAX_RETAINED_EPOCHS = 8` (`:111`).
Whole-session memory bound ≈300 KiB (DESIGN.md `:461-462`).

### Session state export / import

- `Session::export_state` (`src/session.rs:781`), `Session::import_state` (`:825`).
- **Hand-rolled binary codec** (`src/codec.rs` `Writer`/`Reader`, varints). **There is no serde
  anywhere on this crate's session state** — verified: no `Serialize`/`Deserialize` derives under
  `crates/osl-ratchet-next/src/`.
- Field order (`:782-822`): `STATE_FORMAT_VERSION(u8)`, role (`0 = Initiator`, `1 = Responder`),
  `session_id[16]`, `root[32]`, `dhs[32]`, opt `dhr[32]`, opt `cks`, opt `hks`, `nhks[32]`,
  opt `ckr`, opt `hkr`, `nhkr[32]`, varints `ns, nr, pn, send_mix_epoch`, `pq.export`,
  `skipped.export`, opt bootstrap `Preamble`.
- Any other `STATE_FORMAT_VERSION` → `Error::BadStateFormat` (`:827-829`); trailing bytes rejected
  by `r.finish()` (`:854`).
- Trait form: `SecureSession::export` / `::import` (`src/api.rs:98`, `:101`, impl `:134-140`).
- Doc (`:779-780`): **"The output contains every secret the session holds."** Sealing is the
  caller's responsibility — done in `crates/ipc/src/wire_rn.rs:645-672` (see `03-at-rest-and-sealer.md`).

### Negotiation binding (`src/negotiate.rs`)

- `NEGOTIATION_DOMAIN = b"OSL-RN/v1/negotiation/v1"` (`:87`), `MAX_CONTEXT_BYTES = 256` (`:91`).
- `Negotiation::digest()` (`:160-192`) = SHA-256 over length-prefixed (`u32_be(len) ‖ bytes`,
  framing helper `:198`) fields: domain, selected version, floor, responder identity, responder
  ML-KEM ek, initiator identity, context (`:183-191`).
- Refuses with `Error::PolicyBound` when: selected ≠ `0x10`, floor < `0x10`, floor > selected,
  wrong ML-KEM ek length, or empty/oversize context (`:160-192`).
- `:21` documents the "L2 sticky monotone version pin" layer as living in `ipc::wire_rn`.

---

## 3. `crates/ipc/src/wire_rn.rs` — negotiation, pinning, downgrade refusal

| Item | Line | Note |
|---|---|---|
| `LEGACY_WIRE_VERSION_V3 = 0x03` | `:76` | deliberately mirrors `wire_v2::WIRE_VERSION_V3` without depending on it (`:73-75`), so an edit over there cannot silently break the guard |
| `RN_WIRE_IN_ENABLED = false` | `:83` | the review fuse (`:78-83`) |
| `RN_CONTEXT_DISCORD_MANUAL = b"osl-hub/discord-manual-peer/v1"` | `:89` | fixed handshake context; an input to `SK`, so it must never vary per install or per build |
| `enum RatchetPolicyDecision { LegacyV3, Rn }` | `:392-397` | `pub type SelectedVersion = RatchetPolicyDecision;` (`:401`) |
| `enum RnPolicy { Opportunistic (default), Required }` | `:405-416` | |
| `struct RnPeerPin { min_wire_version: u8 }` | `:422-426` | serde `Serialize`/`Deserialize` |
| `RnPeerPin::UNKNOWN` | `:430-432` | `min_wire_version = 0x03` |
| `raise_to_rn` | `:435-439` | **the only mutator; raises only** |
| `validated()` | `:452-462` | accepts only `0x03` or `0x10`, else `RnError::Storage(…)` |

### `select_wire_version` — `:472-495`

Pin is checked **first** (`:477`):

- pinned + `peer_capabilities.supports_rn()` → `Ok(Rn)`
- pinned + not supported → `Err(RnError::PinnedToRn)` (`:479-486`) — **structurally incapable of
  returning `LegacyV3` for a pinned peer**
- unpinned + supported → `Ok(Rn)`
- unpinned + `Required` + unsupported → `Err(RnRequiredButUnsupported)`
- unpinned + `Opportunistic` + unsupported → `Ok(LegacyV3)` (`:490-494`)

`RnError` (`:143-204`) has **no** "fall back to v=3" variant, by design.

### Pin persistence

`load_pin` (`:808-819`): absent → `UNKNOWN`; present-but-unparseable → error; pin below the
persisted `.pin-floor` → error. `raise_pin_to_rn` (`:826-841`);
`persist_pin_floor_to_rn` (`:882-892`). `delete_session` never touches the pin (`:791-802`).
Rationale for separate files (`:41-55`): sharing one file would make deleting it a complete
downgrade attack.

Negotiation digest builders: `initiator_binding` (`:503`), `responder_binding` (`:530`),
`rn_negotiation` forcing `min_acceptable_version = WIRE_VERSION_RN` (`:549-563`).

### Known defect, disclosed and not fixed

`initiate_and_persist_with_sealer` **raises the peer's pin before the peer has confirmed
anything.** Verbatim tail of the function (`crates/ipc/src/wire_rn.rs`, raise at **`:970`**):

```rust
    let session = osl_ratchet_next::initiate_rn_bound(own_identity_secret, peer, &binding, params)
        .map_err(RnError::from)?;
    let peer_id = *peer.identity.as_bytes();
    store.save_session_with_sealer(&peer_id, &session, sealer)?;
    if caps.supports_rn() {
        store.raise_pin_to_rn(&peer_id)?;     // :970
    }
    Ok(session)
```

A local initiation the peer never completes therefore pins them permanently → `PinnedToRn` on
every later send, with no recovery short of a new identity key. No attacker is required.
Identified internally 2026-07-26; deliberately not fixed because changing it changes
downgrade-protection behaviour.

**Stated accurately:** the raise is guarded by `caps.supports_rn()`, i.e. it requires
`PeerCapabilities::Verified` — a bitmap whose Ed25519 REG_MSG signature has already been checked
(`crates/keystore/src/client.rs:432-471`). That is a real narrowing of the trigger, and the
earlier internal write-up did not mention it. It does not remove the defect: the peer still never
confirmed *this* session.

The same "raise on local action, not on confirmed peer traffic" pattern appears at
`wire_rn.rs:1026` and `:1102`. `raise_pin_to_rn` itself is `:826`, writing the pin floor via
`persist_pin_floor_to_rn` (`:882`) at `:829` and `:833`.

**Live item for the reviewing firm.**

---

## 4. Keyserver API

### Cloudflare Worker routes — `keyserver-cf/src/index.ts`

GET (`:328-375`): `/v1/healthz` (`:329`), `/v1/mail/capabilities` (`:330`),
`/v1/download/windows` (`:331`), `/v1/selector-manifest` (`:332`), `/v1/pubkeys/:user_id`
(`:333-334`), `/v1/wrapped-keys/:content_id` (`:335-338`), `/v1/prekey-bundle/:user_id`
(`:339-342`), `/v1/usernames/:username` (`:343-346`), `/v1/control-inbox/:user_id` (`:347-348`),
`/v1/sender-filter-capability-floor/:user_id` (`:349-358`), `/v1/update-manifest/:a/:b/:c`
(`:360-374`).

POST (`:377-463`): `/v1/account-ownership/challenge` (`:378`), **`/v1/register`** (`:381`),
`/v1/mail/{address,consent,send/osl,send/external,list,fetch,ack,delete,burn}` (`:382-390`),
`/v1/internal/sender-filter-rollout-root/{provision,advance}` (`:391`, `:394`),
`/v1/control-inbox` (`:397`), `/v1/usernames/claim` (`:398`), `/v1/wrapped-keys` (`:399`),
`/v1/prekey-bundle/replenish` (`:400`), `/v1/proof-challenge` (`:403`), `/v1/link-grant` (`:406`),
plus commerce routes (`:424-459`).

DELETE (`:465-474`): `/v1/internal/comp/batches/:id`, `/v1/wrapped-keys`,
`/v1/pubkeys/:user_id` (unregister), `/v1/control-inbox/:id`.
CORS allowlist `:308-327`; anything else `405` (`:476`).

### Legacy Node/Fastify keyserver — `keyserver/src/server.js`

`GET /v1/healthz` (`:227`), `POST /v1/register` (`:230`), `GET /v1/pubkeys/:user_id` (`:306`),
`POST /v1/wrapped-keys` (`:313`), `GET /v1/wrapped-keys/:content_id` (`:404`),
`GET /v1/prekey-bundle/:user_id` (`:416`), `POST /v1/prekey-bundle/replenish` (`:439`),
`DELETE /v1/wrapped-keys` (`:517`), `GET /v1/selector-manifest` (`:587`).

**No `rn_capabilities` anywhere in `keyserver/src/*.js`.** RN capability advertisement exists only
on the Worker path. Whether the legacy server is still deployed is **not verified**.

### RN capability advertisement — the only defence against first-contact downgrade

Server side (`keyserver-cf/src/lib/signed-request.ts`): `RN_CAP_WIRE_RN = 1` (`:46`),
`RN_CAP_MAX = 0xffff` (`:57`), `parseRnCapabilities` range check (`:78`), REG_MSG extension
`"\n" + String(rn_capabilities)` (`:146-147`), ROT_MSG extension (`:197-198`).

`keyserver-cf/src/endpoints/register.ts`: parse (`:181`), range error (`:184`), no-op path when
`caps.value <= stored.rn_capabilities` (`:269`), *"changing keys and rn_capabilities together
requires an authenticated rotation"* (`:292`), and rotation refused when
`caps.value < priorRecord.rn_capabilities` — *"capability advertisements never lower"*
(`:384-388`). **Server-side monotonicity.**

`keyserver-cf/src/endpoints/pubkeys.ts`: returns `rn_capabilities` (`:51`) **and**
`registration_sig` (`:52`) so the reader can verify; doc (`:16-19`): *"always present and defaults
to 0 … no encoding of 'unknown, assume capable'"*.

Client side (`crates/keystore/src/client.rs`):

- `RN_CAP_WIRE_RN = 1` (`:190`), `RN_CAP_MAX = 0xffff` (`:195`),
  `CLIENT_RN_CAPABILITY_FLOOR = RN_CAP_WIRE_RN` (`:202`),
  `CLIENT_RN_CAPABILITIES = CLIENT_RN_CAPABILITY_FLOOR` (`:205`)
- `reg_msg_with_capabilities` = legacy `reg_msg` + `b'\n'` + decimal bitmap (`:212-231`);
  `rot_msg_with_capabilities` (`:500-518`)
- `enum PeerCapabilities { Absent, Unverified, Verified(u32) }` (`:241-256`) — **no "unknown,
  assume capable" variant** (`:237-240`); `bitmap()` returns 0 for everything but `Verified`
  (`:262-267`); `supports_rn()` (`:270-272`)
- `verify_peer_capabilities` (`:432-471`): no bitmap → `Absent`; `bits == 0` → `Absent`;
  `bits > RN_CAP_MAX` → `Unverified`; no `registration_sig` → `Unverified`; rebuild extended
  REG_MSG (`:445-453`); base64/length failure → `Unverified` (`:454-463`); only
  `ed25519::verify == Ok(true)` → `Verified(bits)` (`:466-470`)
- `validate_peer_bundle`: capability range check (`:354`), extended-REG_MSG verify (`:358-370`),
  **refuses to fall back to the legacy REG_MSG when `capabilities != 0`** (`:371-373`), legacy
  fallback only for 0 (`:374-388`)
- `PubkeysResponse.rn_capabilities: Option<u32>` (`:545`); `registration_sig` doc warning:
  *"Never trust `rn_capabilities` without checking the signature"* (`:550`)

**The client does advertise, in this tree** — `RegisterRequest.rn_capabilities` (`:96`);
`build_register_request` sets `CLIENT_RN_CAPABILITY_FLOOR` and signs the extended REG_MSG
(`:875-902`); `build_rotation_request` signs both REG_MSG and ROT_MSG with the floor (`:934-969`);
used by `register()` (`:973`) and `register_with_rotation()` (`:1003`).

Tests asserting non-zero advertisement, stripping invalidating the signature, and rotation never
lowering: `crates/keystore/src/client.rs:2374-2386`, `:2416`, `:2421-2430`, `:2768-2782`.
These are in-crate unit tests inside `-p keystore`, which **was** run (289 passed).

**Caveat:** `crates/keystore/src/client.rs` is an **uncommitted** file in this tree (README §1).

### Migrations — `keyserver-cf/migrations/`, 39 `.sql` files

`0026_rn_capability_advertisement.sql` **exists**. Single DDL:
`ALTER TABLE users ADD COLUMN rn_capabilities INTEGER NOT NULL DEFAULT 0;` (file lines 35-36).
Its header states integrity comes from the REG_MSG/ROT_MSG Ed25519 signatures rather than the
table (lines 11-18), that `GET /v1/pubkeys/:user_id` returns `registration_sig` so readers can
verify (19-25), that the default 0 is fail-closed (27-32), and — **line 33 — "NOT DEPLOYED"**,
with the deploy sequence `wrangler d1 migrations apply osl-keyserver-prod --remote` then
`wrangler deploy`, migration before worker (38-50).

`0027_control_inbox_revocation_lane.sql` **exists** (unrelated to RN). Adds a non-evictable,
collapsible priority lane for bilateral-burn revocation notices: never evicted, **507-refused
rather than dropped** when full, collapsible via an opaque client-computed MAC over
(scope, epoch); `kind` is a signed component of the POST canonical bytes. Also **"NOT DEPLOYED"**
(lines 3-5).

Later references to the column: `0033_canonical_identity_rollout_authority.sql:51`,
`0034_scheme1_prekey_owner_proofs.sql:44-45` (`CHECK (rn_capabilities BETWEEN 0 AND 65535)`),
`:83`, `:106`, `:191-193`.

**Structural hazard, found and not assessed.** Duplicate numeric prefixes exist:
`0025_payment_alert_outbox.sql` / `0025_username_directory.sql`;
`0026_osl_mail.sql` / `0026_rn_capability_advertisement.sql`;
`0036_account_ownership_challenges.sql` / `0036_account_ownership_proof_required.sql`.
Whether `wrangler d1 migrations apply` orders these deterministically **was not verified**.

Other migration directories in the repo (`cipher-store-cf/migrations/`, `0001`–`0010`) are
unrelated to the keyserver and were not reviewed.

---

## 5. Not determined

- **Deployment state of `0026` / `0027`, and of either keyserver.** Both files self-declare "NOT
  DEPLOYED"; that is a statement in a file, not an observation of production. No production
  endpoint was queried. **This package refuses to infer it.**
- Whether duplicate migration prefixes are safely ordered by `wrangler`.
- Whether any wire byte produced by the current build actually matches these documented layouts —
  no runtime capture was taken.
- Whether two current builds, both advertising `RN_CAP_WIRE_RN`, drive each other into the
  "refusing to send v=3 (no downgrade)" state in practice.
