# OSL-RN Discord Wire-In Costing

Date: 2026-07-26.

Scope: read-only audit of the eventual Discord-path wire-in for OSL-RN wire `0x10`.

No source change is applied here. No Cargo command was run. No network command was run.

The requested `crates/ipc/src/broker.rs` does not exist in this checkout. The current broker path is `apps/osl-hub/src/broker.rs`, and `MIGRATION.md` itself cites `apps/osl-hub/src/broker.rs` for the send helper shape at `crates/osl-ratchet-next/MIGRATION.md:142`.

## 1. Call-Site Inventory

### Existing OSL-RN integration boundary

`crates/ipc/src/wire_rn.rs:4` says this module is the only sanctioned entry point to `osl_ratchet_next` from application code. `crates/ipc/src/wire_rn.rs:15` states that nothing in the module is wired into a send path yet. `crates/ipc/src/lib.rs:75` exposes `wire_rn`, and `crates/ipc/src/lib.rs:78` makes it public. `crates/ipc/src/wire_rn.rs:281` defines `select_wire_version(pin, peer_supports_rn, policy)`. `crates/ipc/src/wire_rn.rs:286` checks the sticky pin first. `crates/ipc/src/wire_rn.rs:298` returns `Rn` only when the peer support bit is true, except required mode errors rather than falling back.

`osl_ratchet_next::encrypt_rn` has the v3-like shape at `crates/osl-ratchet-next/src/api.rs:147`. It mutates a session at `crates/osl-ratchet-next/src/api.rs:149`. `osl_ratchet_next::decrypt_rn` mutates a session at `crates/osl-ratchet-next/src/api.rs:153`.

`crates/ipc/src/wire_rn.rs` does not currently provide a sanctioned send/open wrapper equivalent to `encrypt_v3` / `decrypt_v3_for_sender`. Its production helpers are version selection at `crates/ipc/src/wire_rn.rs:281`, binding construction at `crates/ipc/src/wire_rn.rs:315` and `crates/ipc/src/wire_rn.rs:342`, session storage at `crates/ipc/src/wire_rn.rs:422` and `crates/ipc/src/wire_rn.rs:492`, pin raise/load at `crates/ipc/src/wire_rn.rs:562` and `crates/ipc/src/wire_rn.rs:593`, initiate at `crates/ipc/src/wire_rn.rs:615`, and accept at `crates/ipc/src/wire_rn.rs:645`. The broker therefore has no allowed RN encrypt/decrypt function to call today.

### Send sites

#### `apps/osl-hub/src/broker.rs:1443`

What it does now: `prepare_peer_prose_text_inner_with_chunk` serializes a `PeerProtectedPayload` and calls `encrypt_direct_manual_v3_payload` with `MSG_TYPE_CONTENT`.

What it would have to do: call a version-selecting helper. This helper must choose v3 or RN using a verified capability and the sticky pin. It must persist the RN session before the carrier send leaves the machine. It must not try local self-readback for RN, because the pairwise ratchet cannot decrypt its own ciphertext; `MIGRATION.md` calls this out at `crates/osl-ratchet-next/MIGRATION.md:287`.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    encrypt_direct_manual_v3_payload(core, peer, ipc::wire_v2::MSG_TYPE_CONTENT, &payload)
+    encrypt_direct_manual_payload(core, peer, ipc::wire_v2::MSG_TYPE_CONTENT, &payload)
```

#### `apps/osl-hub/src/broker.rs:1446`

What it does now: `encrypt_direct_manual_v3_payload` clones the local identity at `apps/osl-hub/src/broker.rs:1452`, refuses self-key sends at `apps/osl-hub/src/broker.rs:1459`, constructs a two-recipient v3 slot array at `apps/osl-hub/src/broker.rs:1462`, and calls `ipc::wire_v2::encrypt_v3` at `apps/osl-hub/src/broker.rs:1472`.

What it would have to do: become a version-selecting helper, or be wrapped by one. For RN it must load or initiate an RN session, call `osl_ratchet_next::encrypt_rn`, save the session before the row is POSTed, and raise the pin on successful initiation. `wire_rn` already provides `initiate_and_persist` at `crates/ipc/src/wire_rn.rs:615` and saves before returning at `crates/ipc/src/wire_rn.rs:631`.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-fn encrypt_direct_manual_v3_payload(
+fn encrypt_direct_manual_payload(
     core: &HubCoreState,
     peer: &ManualPeerBinding,
     message_type: u8,
     payload: &[u8],
 ) -> Result<String, String> {
@@
-    let recipients = [
+    if osl_rn_discord_wire_in_enabled() {
+        return encrypt_direct_manual_rn_payload(core, peer, message_type, payload);
+    }
+    let recipients = [
@@
-    ipc::wire_v2::encrypt_v3(
+    ipc::wire_v2::encrypt_v3(
         &identity.x25519_secret,
         &identity.x25519_public,
         &recipients,
         message_type,
         payload,
     )
     .map_err(|_| "OSL could not prepare a single manual peer message".to_owned())
 }
+
+fn encrypt_direct_manual_v3_payload(
+    core: &HubCoreState,
+    peer: &ManualPeerBinding,
+    message_type: u8,
+    payload: &[u8],
+) -> Result<String, String> {
+    encrypt_direct_manual_payload(core, peer, message_type, payload)
+}
```

The exact RN helper body cannot be written honestly from the current source alone. The broker has no sanctioned `ipc::wire_rn` encrypt/decrypt wrapper, no sealer argument, no RN store directory, no verified `PeerCapabilities`, and no accepted answer to the re-decryption blocker. Those are not local substitutions.

#### `apps/osl-hub/src/broker.rs:2436`

What it does now: native overlay relay notices are sealed as `MSG_TYPE_NATIVE_OVERLAY_RELAY` with v3, verified by `verify_manual_v3_type` at `apps/osl-hub/src/broker.rs:2442`, decoded as a v3 relay bundle at `apps/osl-hub/src/broker.rs:2453`, and posted to control inbox at `apps/osl-hub/src/broker.rs:2460`.

What it would have to do: call the version-selecting helper, stop requiring `verify_manual_v3_type` for RN, and use a version-aware bundle decoder. The current v3 decoder requires cleartext byte 1 to be the message type; RN encrypts the real message type, as stated at `crates/osl-ratchet-next/MIGRATION.md:42`.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-        let wire = encrypt_direct_manual_v3_payload(
+        let wire = encrypt_direct_manual_payload(
             core,
             &verified,
             ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
             &encoded,
         )?;
-        verify_manual_v3_type(
-            core,
-            &verified,
-            &wire,
-            ManualWireSender::SelfIdentity,
-            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
-        )
-        .map_err(|_| {
-            qa_encrypt_refusal_site("verify_manual_v3_type_failed");
-            "OSL could not deliver the protected message".to_owned()
-        })?;
-        let bundle = decode_overlay_relay_wire(&wire)?;
+        let bundle = decode_overlay_relay_wire_versioned(&wire)?;
```

#### `apps/osl-hub/src/broker.rs:3486`

What it does now: native overlay attachment notices are sealed with v3 `MSG_TYPE_ATTACHMENT`, self-verified at `apps/osl-hub/src/broker.rs:3494`, decoded with `decode_typed_manual_wire` at `apps/osl-hub/src/broker.rs:3502`, and posted at `apps/osl-hub/src/broker.rs:3508`.

What it would have to do: use the version-selecting helper and a version-aware bundle decoder. The self-verification readback cannot apply to RN.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    let wire = encrypt_direct_manual_v3_payload(
+    let wire = encrypt_direct_manual_payload(
         core,
         &verified,
         ipc::wire_v2::MSG_TYPE_ATTACHMENT,
         &encoded,
     )
     .map_err(|_| ERROR.to_owned())?;
     encoded.fill(0);
-    verify_manual_v3_type(
-        core,
-        &verified,
-        &wire,
-        ManualWireSender::SelfIdentity,
-        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
-    )
-    .map_err(|_| ERROR.to_owned())?;
-    let bundle = decode_typed_manual_wire(&wire, ipc::wire_v2::MSG_TYPE_ATTACHMENT)
+    let bundle = decode_typed_manual_wire_versioned(&wire, ipc::wire_v2::MSG_TYPE_ATTACHMENT)
         .map_err(|_| ERROR.to_owned())?;
```

#### `apps/osl-hub/src/broker.rs:3888`

What it does now: native overlay acknowledgements are sealed with v3 `MSG_TYPE_NATIVE_OVERLAY_ACK`, self-verified at `apps/osl-hub/src/broker.rs:3894`, decoded as a v3 ack bundle at `apps/osl-hub/src/broker.rs:3902`, and posted at `apps/osl-hub/src/broker.rs:3904`.

What it would have to do: use the version-selecting helper and a version-aware ack decoder. This path also has retry/replay semantics: the receiver records acknowledgements idempotently and deletes the row only after recording at `apps/osl-hub/src/broker.rs:2788` and `apps/osl-hub/src/broker.rs:2806`. With RN, a failed delete followed by a re-drain cannot re-decrypt.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    let wire = encrypt_direct_manual_v3_payload(
+    let wire = encrypt_direct_manual_payload(
         core,
         verified,
         ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
         &encoded,
     )?;
-    verify_manual_v3_type(
-        core,
-        verified,
-        &wire,
-        ManualWireSender::SelfIdentity,
-        ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
-    )
-    .map_err(|_| "OSL could not acknowledge the protected message".to_owned())?;
-    let bundle = decode_native_overlay_ack_wire(&wire)?;
+    let bundle = decode_native_overlay_ack_wire_versioned(&wire)?;
```

#### `apps/osl-hub/src/broker.rs:5001`

What it does now: bilateral revocation notices and acknowledgements are sealed with v3, self-verified at `apps/osl-hub/src/broker.rs:5006`, decoded by v3 framing at `apps/osl-hub/src/broker.rs:5014`, and posted to a revocation lane at `apps/osl-hub/src/broker.rs:5015`.

What it would have to do: either remain v3 or accept the one-decrypt cost. There is no honest one-line RN substitution because the revocation lane intentionally survives retry and collapse-key behavior. `apply_inbound_revocation_row` can return `Deferred` at `apps/osl-hub/src/broker.rs:4839`; an RN decrypt that consumed the message before a deferred storage write would destroy the only decrypt opportunity.

This cannot be done by replacing `encrypt_v3` with RN at this site. It needs a decision about whether revocation control rows stay v3, are plaintext-cached after one decrypt, or get their own non-ratcheted recovery channel.

Proposed diff if this site is kept v3, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    let wire = encrypt_direct_manual_v3_payload(core, verified, message_type, &payload)?;
+    // Deliberately v3: revocation must tolerate retry/deferred apply.
+    let wire = encrypt_direct_manual_v3_payload(core, verified, message_type, &payload)?;
```

#### `apps/osl-hub/src/broker.rs:5392`

What it does now: peer attachment envelopes are sealed with v3 `MSG_TYPE_ATTACHMENT`, self-verified at `apps/osl-hub/src/broker.rs:5400`, and returned as `envelope_wire` at `apps/osl-hub/src/broker.rs:5410`.

What it would have to do: use the version-selecting helper and remove v3 self-readback for RN. This also inherits the re-decryption blocker because `open_peer_attachment` later decrypts the same envelope supplied by the UI at `apps/osl-hub/src/broker.rs:5466`.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    let envelope_wire = encrypt_direct_manual_v3_payload(
+    let envelope_wire = encrypt_direct_manual_payload(
         core,
         &verified,
         ipc::wire_v2::MSG_TYPE_ATTACHMENT,
         &payload_bytes,
     )
     .map_err(|_| PREPARE_ERROR.to_owned())?;
     payload_bytes.fill(0);
-    verify_manual_v3_type(
-        core,
-        &verified,
-        &envelope_wire,
-        ManualWireSender::SelfIdentity,
-        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
-    )
-    .map_err(|_| PREPARE_ERROR.to_owned())?;
```

#### `crates/ipc/src/commands.rs:2965`

What it does now: the generic v3 content send path builds a `Vec<RecipientV3>` at `crates/ipc/src/commands.rs:2963` and calls `crate::wire_v2::encrypt_v3` at `crates/ipc/src/commands.rs:2965`.

What it would have to do: only a DM with exactly one non-self peer can be routed to a pairwise RN session. Multi-recipient scope sends cannot be represented as one RN blob. `DESIGN.md` states this limitation at `crates/osl-ratchet-next/DESIGN.md:527`, and `MIGRATION.md` repeats it at `crates/osl-ratchet-next/MIGRATION.md:318`.

This cannot be converted globally. The only honest diff is a narrow DM-only branch before the v3 call.

Proposed diff, not applied:

```diff
diff --git a/crates/ipc/src/commands.rs b/crates/ipc/src/commands.rs
@@
     let key_recipients: Vec<crate::wire_v2::RecipientV3> =
         recipients.iter().map(|(_, r)| r.clone()).collect();
+    if rn_send_gate_enabled()
+        && !scope_is_group_or_server(&scope)
+        && non_self_peers.len() == 1
+    {
+        return encrypt_rn_dm_send(
+            state,
+            &sender_sk,
+            &self_pk,
+            &scope,
+            &non_self_peers[0],
+            crate::wire_v2::MSG_TYPE_CONTENT,
+            plaintext.as_bytes(),
+        );
+    }
     crate::wire_v2::encrypt_v3(
```

#### `crates/ipc/src/commands.rs:3320`

What it does now: `send_skdm_via_v3_bundle` sends `MSG_TYPE_SENDER_KEY_DISTRIBUTION` with v3.

What it would have to do: stay v3 unless the group sender-key design is replaced. OSL-RN is pairwise and does not replace sender-key distribution. This is not a valid RN wire-in site.

Proposed diff, not applied:

```diff
diff --git a/crates/ipc/src/commands.rs b/crates/ipc/src/commands.rs
@@
-    crate::wire_v2::encrypt_v3(
+    // Deliberately v3: OSL-RN is pairwise and does not carry SKDM bundles.
+    crate::wire_v2::encrypt_v3(
```

### Receive sites

#### `crates/ipc/src/commands.rs:4368`

What it does now: `cmd_osl_decrypt_message_v2` peeks the version at `crates/ipc/src/commands.rs:4352`, routes v2 at `crates/ipc/src/commands.rs:4369`, v3 at `crates/ipc/src/commands.rs:4385`, v4 at `crates/ipc/src/commands.rs:4407`, v5 at `crates/ipc/src/commands.rs:4435`, and sends unknown versions to the legacy v1 path at `crates/ipc/src/commands.rs:4449`.

What it would have to do: add an explicit `0x10` arm. When the compile feature or runtime switch is off, the arm must return an error before any RN state load or mutation. When on, it must load or accept a session, decrypt once, persist the advanced session, and then route by `opened.msg_type` exactly like v2/v3.

Proposed diff, not applied:

```diff
diff --git a/crates/ipc/src/commands.rs b/crates/ipc/src/commands.rs
@@
         Some(crate::wire_v2::WIRE_VERSION_V5) => {
             // Phase 9-A3: v=5 sender-keys group decode.
             tracing::debug!(wire_version = "v5", "v=5 decode dispatched");
             let sender_did_for_persist = sender_discord_id.clone();
             let result = decrypt_v5_recv(state, sender_discord_id, content, scope_opt)?;
@@
             return Ok(result);
         }
+        Some(osl_ratchet_next::WIRE_VERSION_RN) => {
+            if !rn_recv_gate_enabled() {
+                return Err("OSL: OSL-RN wire 0x10 is disabled".to_string());
+            }
+            tracing::debug!(wire_version = "rn", "v=0x10 decode dispatched");
+            decrypt_rn_recv(
+                state,
+                sender_discord_id,
+                content,
+                scope_opt,
+                config_dir.as_deref(),
+            )?
+        }
         _ => {
```

#### `apps/osl-hub/src/broker.rs:2752` and `apps/osl-hub/src/broker.rs:2813`

What it does now: the native overlay text drain classifies ack and relay rows by v3 fixed bytes using `is_native_overlay_ack_bundle` and `is_native_overlay_relay_bundle`. Those helpers require `bundle[0] == WIRE_VERSION_V3` and `bundle[1] == MSG_TYPE_*` at `crates/ipc/src/wire_v2.rs:236` and `crates/ipc/src/wire_v2.rs:231`.

What it would have to do: use version-aware classification. For RN, classification is successful decryption, because byte 1 is flags and the real type is encrypted.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-        if ipc::wire_v2::is_native_overlay_ack_bundle(&bundle) {
+        if manual_wire_maybe_type(&bundle, ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK) {
@@
-        if !allow_messages
+        if !allow_messages
             || messages.len().saturating_add(pending_view_once.len())
                 >= MAX_NATIVE_OVERLAY_OPEN_BATCH
-            || !ipc::wire_v2::is_native_overlay_relay_bundle(&bundle)
+            || !manual_wire_maybe_type(&bundle, ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY)
         {
             continue;
         }
```

#### `apps/osl-hub/src/broker.rs:2757`, `apps/osl-hub/src/broker.rs:2768`, `apps/osl-hub/src/broker.rs:2818`, `apps/osl-hub/src/broker.rs:2829`

What they do now: the text drain first checks `verify_manual_v3_type`, then calls `decrypt_direct_manual_v3_payload` for ack and relay rows.

What they would have to do: replace pre-decrypt v3 inspection with a version-aware decrypt helper. For RN, authentication is the decrypt success. This is a large behavior change because failed delete/retry will no longer be able to re-open the same row.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-            if verify_manual_v3_type(
-                core,
-                &verified,
-                &wire,
-                ManualWireSender::Peer,
-                ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
-            )
-            .is_err()
-            {
-                continue;
-            }
-            let Ok(plaintext) = decrypt_direct_manual_v3_payload(
+            let Ok(plaintext) = decrypt_direct_manual_payload(
                 core,
                 &verified,
                 ManualWireSender::Peer,
                 &wire,
                 ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_ACK,
@@
-        if verify_manual_v3_type(
-            core,
-            &verified,
-            &wire,
-            ManualWireSender::Peer,
-            ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
-        )
-        .is_err()
-        {
-            continue;
-        }
-        let Ok(plaintext) = decrypt_direct_manual_v3_payload(
+        let Ok(plaintext) = decrypt_direct_manual_payload(
             core,
             &verified,
             ManualWireSender::Peer,
             &wire,
             ipc::wire_v2::MSG_TYPE_NATIVE_OVERLAY_RELAY,
```

#### `apps/osl-hub/src/broker.rs:3638`, `apps/osl-hub/src/broker.rs:3642`, `apps/osl-hub/src/broker.rs:3653`

What they do now: attachment listing filters control-inbox rows with the v3 attachment fixed-byte probe, verifies v3 type, and decrypts v3.

What they would have to do: classify/decrypt version-aware. This is directly affected by the re-decryption blocker because listing attachments can inspect rows without consuming the final open plan.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-        if !ipc::wire_v2::is_attachment_bundle(&bundle) {
+        if !manual_wire_maybe_type(&bundle, ipc::wire_v2::MSG_TYPE_ATTACHMENT) {
             return None;
         }
         let wire = format!("DPC0::{}", STANDARD.encode(&bundle));
-        if verify_manual_v3_type(
-            core,
-            &verified,
-            &wire,
-            ManualWireSender::Peer,
-            ipc::wire_v2::MSG_TYPE_ATTACHMENT,
-        )
-        .is_err()
-        {
-            return None;
-        }
-        let Ok(mut plaintext) = decrypt_direct_manual_v3_payload(
+        let Ok(mut plaintext) = decrypt_direct_manual_payload(
             core,
             &verified,
             ManualWireSender::Peer,
             &wire,
             ipc::wire_v2::MSG_TYPE_ATTACHMENT,
```

#### `apps/osl-hub/src/broker.rs:4179` and `apps/osl-hub/src/broker.rs:4334`

What they do now: `decrypt_direct_manual_v3` is a content wrapper over `decrypt_direct_manual_v3_payload`; `decrypt_direct_manual_v3_payload` calls `ipc::wire_v2::decrypt_v3_for_sender` at `apps/osl-hub/src/broker.rs:4353` and then checks the clear message type at `apps/osl-hub/src/broker.rs:4360`.

What they would have to do: become version-aware and call RN open for `0x10`. This cannot be a pure function any more, because RN decrypt mutates and must persist session state on success.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-fn decrypt_direct_manual_v3(
+fn decrypt_direct_manual(
@@
-    let plaintext =
-        decrypt_direct_manual_v3_payload(core, peer, sender, wire, ipc::wire_v2::MSG_TYPE_CONTENT)
+    let plaintext =
+        decrypt_direct_manual_payload(core, peer, sender, wire, ipc::wire_v2::MSG_TYPE_CONTENT)
             .map_err(str::to_owned)?;
@@
-fn decrypt_direct_manual_v3_payload(
+fn decrypt_direct_manual_payload(
     core: &HubCoreState,
     peer: &ManualPeerBinding,
     sender: ManualWireSender,
     wire: &str,
     expected_message_type: u8,
 ) -> Result<Vec<u8>, &'static str> {
@@
+    if peek_manual_wire_version(wire) == Some(osl_ratchet_next::WIRE_VERSION_RN) {
+        return decrypt_direct_manual_rn_payload(core, peer, sender, wire, expected_message_type);
+    }
     let opened = ipc::wire_v2::decrypt_v3_for_sender(
```

#### `apps/osl-hub/src/broker.rs:4899` and `apps/osl-hub/src/broker.rs:4902`

What they do now: inbound revocation rows are v3-verified and then v3-decrypted before applying or deferring.

What they would have to do: this cannot be blindly switched to RN because `RevocationRowOutcome::Deferred` exists at `apps/osl-hub/src/broker.rs:4839`. A deferred apply after RN decrypt would consume the message key and leave the row undecryptable on retry.

Proposed diff if kept v3, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    if verify_manual_v3_type(core, verified, &wire, ManualWireSender::Peer, message_type).is_err() {
+    // Deliberately v3: deferred revocation apply must be retryable.
+    if verify_manual_v3_type(core, verified, &wire, ManualWireSender::Peer, message_type).is_err() {
```

#### `apps/osl-hub/src/broker.rs:5458` and `apps/osl-hub/src/broker.rs:5466`

What they do now: `open_peer_attachment` verifies the v3 envelope and decrypts it.

What it would have to do: use the version-aware decrypt helper. This path cannot share the existing attachment-list decrypt unless plaintext metadata is cached, because RN decrypt is one-use.

Proposed diff, not applied:

```diff
diff --git a/apps/osl-hub/src/broker.rs b/apps/osl-hub/src/broker.rs
@@
-    verify_manual_v3_type(
-        core,
-        &verified,
-        &envelope_wire,
-        ManualWireSender::Peer,
-        ipc::wire_v2::MSG_TYPE_ATTACHMENT,
-    )
-    .map_err(|_| OPEN_ERROR.to_owned())?;
-    let mut payload_bytes = decrypt_direct_manual_v3_payload(
+    let mut payload_bytes = decrypt_direct_manual_payload(
         core,
         &verified,
         ManualWireSender::Peer,
         &envelope_wire,
         ipc::wire_v2::MSG_TYPE_ATTACHMENT,
```

#### `crates/ipc/src/commands.rs:5383` and `crates/ipc/src/commands.rs:5400`

What they do now: the generic control-inbox drain skips native-overlay relay bundles by v3 fixed-byte classification at `crates/ipc/src/commands.rs:5383`, then dispatches all other rows through `cmd_osl_decrypt_message_v2` at `crates/ipc/src/commands.rs:5400`.

What it would have to do: understand that `0x10` relay rows cannot be identified before decrypt. If the separate trusted overlay drain owns RN relay rows, the generic drain must not try to decrypt and delete them as control messages. A v3 byte-probe is insufficient.

Proposed diff, not applied:

```diff
diff --git a/crates/ipc/src/commands.rs b/crates/ipc/src/commands.rs
@@
-        if crate::wire_v2::is_native_overlay_relay_bundle(&bundle) {
+        if crate::wire_v2::is_native_overlay_relay_bundle(&bundle)
+            || rn_wire_may_be_native_overlay_relay(&bundle)
+        {
             continue;
         }
```

The helper cannot be exact without decrypting or adding an authenticated outer hint. The honest choices are: leave RN relay rows to the overlay drain by lane/kind, add a signed server lane for RN relay, or accept that the generic drain will sometimes have to attempt RN decrypt and risk consuming a message.

## 2. Feature Gate

Feature name: `danger-osl-rn-wire-in`.

Runtime switch: `OSL_RN_WIRE_IN=reviewed-0x10`. Any missing or different value means off. This is an enable token, but operationally it is the kill-switch: deleting or changing the environment variable turns RN off without rebuilding.

Manifest changes that would be needed, not applied:

`crates/ipc/Cargo.toml` currently has no `[features]` section before `[dependencies]` at `crates/ipc/Cargo.toml:7`. Add this after line 6:

```diff
diff --git a/crates/ipc/Cargo.toml b/crates/ipc/Cargo.toml
@@
 edition.workspace = true
 license.workspace = true
 
+[features]
+default = []
+danger-osl-rn-wire-in = []
+
 [dependencies]
```

`apps/osl-hub/Cargo.toml:44` has `[features]`, `apps/osl-hub/Cargo.toml:45` has `default = []`, and `apps/osl-hub/Cargo.toml:52` has `discord-qa-shell = []`. Add the forwarding feature after line 52:

```diff
diff --git a/apps/osl-hub/Cargo.toml b/apps/osl-hub/Cargo.toml
@@
 discord-qa-shell = []
+danger-osl-rn-wire-in = ["ipc/danger-osl-rn-wire-in"]
```

The runtime check should live in both receive and send selectors, not only the UI. Send-side check belongs inside the eventual `encrypt_direct_manual_payload` helper in `apps/osl-hub/src/broker.rs`, before it can select RN. Receive-side check belongs in the new `Some(osl_ratchet_next::WIRE_VERSION_RN)` arm in `crates/ipc/src/commands.rs:4368`.

When the gate is off and a `0x10` blob arrives, the safe behavior is an explicit error from the `0x10` arm before loading or mutating RN state. It must not fall through to v1. The current router sends unknown versions to the legacy path at `crates/ipc/src/commands.rs:4449`, so the gated arm is not optional. In the native overlay drain, current v3 fixed probes would just skip a `0x10` blob because `is_native_overlay_relay_bundle` requires v3 at `crates/ipc/src/wire_v2.rs:231`; that is safe for confidentiality and state, but silent for delivery.

## 3. Re-Decryption Blocker

The blocker is still true today.

`MIGRATION.md` says the same stored ciphertext is decrypted repeatedly at `crates/osl-ratchet-next/MIGRATION.md:258`. It says a ratchet can decrypt each message exactly once at `crates/osl-ratchet-next/MIGRATION.md:266`.

The current history path still re-opens rows. `rehydrate_native_discord_overlay_history` takes Discord visible rows at `apps/osl-hub/src/broker.rs:1745`. It iterates rows at `apps/osl-hub/src/broker.rs:1772`, tries candidates for each row at `apps/osl-hub/src/broker.rs:1817`, and calls `authenticate_oriented_prose_pointer` at `apps/osl-hub/src/broker.rs:1822`. That path explicitly says it consumes no message at `apps/osl-hub/src/broker.rs:1737`. Re-running history therefore repeats the decrypt attempt.

The control-inbox text drain also decrypts fetched stored rows repeatedly until deletion succeeds. It fetches `items` at `apps/osl-hub/src/broker.rs:2666`, loops at `apps/osl-hub/src/broker.rs:2678`, decodes each stored `bundle_b64` at `apps/osl-hub/src/broker.rs:2687`, decrypts ack rows at `apps/osl-hub/src/broker.rs:2768`, decrypts relay rows at `apps/osl-hub/src/broker.rs:2829`, and deletes only after successful processing at `apps/osl-hub/src/broker.rs:2806`. If the delete fails, the same row remains and the later drain re-decrypts it.

The attachment listing path has the same shape. It fetches control-inbox rows at `apps/osl-hub/src/broker.rs:3622`, runs a bounded collection over them at `apps/osl-hub/src/broker.rs:3631`, decodes `bundle_b64` at `apps/osl-hub/src/broker.rs:3635`, decrypts the attachment notice at `apps/osl-hub/src/broker.rs:3653`, and returns a plan without deleting the row unless it was already consumed at `apps/osl-hub/src/broker.rs:3680`.

The generic control-inbox drain does the same for core control rows. It fetches rows at `crates/ipc/src/commands.rs:5306`, loops at `crates/ipc/src/commands.rs:5317`, decodes `bundle_b64` at `crates/ipc/src/commands.rs:5367`, calls `cmd_osl_decrypt_message_v2` at `crates/ipc/src/commands.rs:5400`, and deletes only after success at `crates/ipc/src/commands.rs:5420`.

Real options and costs:

1. Cache plaintext or parsed application payload after the first successful decrypt. Cost: the at-rest boundary changes from Discord ciphertext to local encrypted store plaintext. This is how the OSL Chats history path already works after decrypt: v4/v5 and v2/v3 content call `persist_user_plaintext` at `crates/ipc/src/commands.rs:4426` and `crates/ipc/src/commands.rs:4475`. It requires product decisions for view-once, expiry, burn, attachment metadata, acknowledgements, and revocation rows.

2. Retain RN message keys or enough skipped-key state to reopen a message. Cost: this destroys the forward-secrecy benefit that motivated the ratchet. It also turns skipped-key retention into a durable secret cache. This is the wrong security trade unless the product explicitly chooses replayable ciphertext over forward secrecy.

3. Leave Discord overlay v3 and use OSL-RN only for paths that already cache plaintext or hold ratchet state. Cost: no forward secrecy improvement for the Discord overlay. `MIGRATION.md` already suggests migrating OSL Chats first at `crates/osl-ratchet-next/MIGRATION.md:391`.

4. Redesign the carrier contract so an RN decrypt happens exactly once on arrival, before rows are exposed to history/listing flows. Cost: a new ingestion ledger, durable idempotence, failure recovery, and UI behavior for rows whose first decrypt succeeded but downstream persistence failed.

There is no cheap local workaround.

## 4. Capability Plumbing

`keystore::client::verify_peer_capabilities` exists at `crates/keystore/src/client.rs:301`. It returns `Absent` when no bitmap exists at `crates/keystore/src/client.rs:302`, returns `Unverified` for out-of-range bits at `crates/keystore/src/client.rs:308`, verifies the full peer bundle at `crates/keystore/src/client.rs:311`, and returns `Verified(bits)` at `crates/keystore/src/client.rs:314`. `PeerCapabilities::supports_rn` tests bit 0 at `crates/keystore/src/client.rs:207`.

I found no production call to `verify_peer_capabilities`. Direct calls found by grep are test-only: `crates/keystore/tests/client_test.rs:747`, `crates/keystore/tests/client_test.rs:762`, `crates/keystore/tests/client_test.rs:770`, `crates/keystore/tests/client_test.rs:783`, `crates/keystore/tests/client_test.rs:801`, `crates/keystore/tests/client_test.rs:809`, `crates/keystore/tests/client_test.rs:817`, `crates/keystore/tests/client_test.rs:826`, `crates/keystore/tests/client_test.rs:835`, `crates/keystore/tests/client_test.rs:843`, and `crates/keystore/tests/client_test.rs:849`.

Production fetch currently verifies only the bundle. `KeyServerClient::fetch_pubkeys` says it returns only after the Ed25519 registration signature verifies at `crates/keystore/src/client.rs:739`; it calls `verify_peer_bundle` at `crates/keystore/src/client.rs:746`. It does not call `verify_peer_capabilities`.

Production fetch sites include `cmd_fetch_pubkeys` at `crates/ipc/src/commands.rs:1027`, scope recipient fetch at `crates/ipc/src/commands.rs:1457`, sender resolution fetch at `crates/ipc/src/commands.rs:1851`, cache fill fetch at `crates/ipc/src/commands.rs:5812`, peer refresh fetch at `crates/ipc/src/commands.rs:6343`, and recovery verification fetch at `crates/ipc/src/commands.rs:8501`. The Discord manual broker has a QA-only registration check fetch at `apps/osl-hub/src/broker.rs:2318`.

Current state has nowhere to store a verified capability. `PeerEntry` holds OSL user id at `crates/ipc/src/peer_map.rs:67`, v3/v4 public keys at `crates/ipc/src/peer_map.rs:75`, ML-KEM at `crates/ipc/src/peer_map.rs:84`, v4 initial ratchet key at `crates/ipc/src/peer_map.rs:135`, and v4 ratchet state at `crates/ipc/src/peer_map.rs:144`. There is no capability field. `ManualPeerBinding` contains only person id, OSL user id, X25519 public key, and ML-KEM public key at `apps/osl-hub/src/security.rs:165`.

Exact plumbing needed:

1. Add a persisted capability record to `PeerEntry`, not only to `ManualPeerBinding`. Send-time selection needs the value after bootstrap and across process restarts. The record must be the verified result, not raw `rn_capabilities`. A minimal shape is `rn_capabilities_verified: Option<u32>`, plus `rn_capabilities_checked_at: Option<i64>` and a binding to the exact trusted key bundle.

2. In `populate_peer_from_fetch_response`, call `keystore::client::verify_peer_capabilities(resp)` immediately after `verify_peer_bundle` at `crates/ipc/src/commands.rs:5872`. Only persist `Verified(bits)` when `live_writable` is true. If the TOFU outcome is `Changed`, the existing code skips live key writes at `crates/ipc/src/commands.rs:5922`; it must also skip capability writes, or it would attach a new signed capability to an old trusted key.

3. `manual_peer_binding` must copy the verified capability from `PeerEntry` into `ManualPeerBinding`. It currently reads the peer entry at `apps/osl-hub/src/security.rs:1326`, validates the trusted bundle at `apps/osl-hub/src/security.rs:1344`, and returns only keys at `apps/osl-hub/src/security.rs:1358`.

4. The send helper must load the RN pin from `RnSessionStore::load_pin`, pass `binding.peer_capabilities.supports_rn()` into `ipc::wire_rn::select_wire_version`, and refuse on `PinnedToRn` rather than falling back. The pin path exists at `crates/ipc/src/wire_rn.rs:562`. The selector behavior exists at `crates/ipc/src/wire_rn.rs:286`.

5. Lifetime: the capability is valid only for the exact signed key bundle it was verified with. It may persist until the trusted bundle changes, the keyserver fetch reports a changed bundle, or a local freshness policy expires it. Stale must mean "not capable" for unpinned peers and "refuse" for pinned peers. It must never mean "assume RN".

6. Freshness: unverified. The current code has no general keyserver freshness TTL for peer capabilities. A real wire-in needs either fetch-on-send for stale records or a documented local TTL. Fetch-on-send costs network latency and failure on the send path. TTL caching costs delayed recognition of newly capable peers and delayed detection of capability stripping.

## 5. Manifest Falsehood

Root manifest quote:

`Cargo.toml:25`:

> `    # Isolated, UNWIRED research crate. Reachable only from its own`

`Cargo.toml:26`:

> `    # tests; no other crate or app depends on it. See`

`Cargo.toml:27`:

> `    # crates/osl-ratchet-next/DESIGN.md — unreviewed, must not carry`

`Cargo.toml:28`:

> `    # real traffic before external cryptographic review.`

IPC manifest quote:

`crates/ipc/Cargo.toml:71`:

> `# OSL-RN (wire 0x10): hybrid PQ epoch ratchet with header encryption.`

`crates/ipc/Cargo.toml:72`:

> `# Consumed only by `wire_rn`; the v=2..v=5 paths do not reference it.`

`crates/ipc/Cargo.toml:73`:

> `osl-ratchet-next = { path = "../osl-ratchet-next" }`

Corrected root wording should be:

```toml
    # Isolated, UNWIRED research protocol. The ipc crate depends on it only
    # through ipc::wire_rn; no send path routes real traffic through it. See
    # crates/osl-ratchet-next/DESIGN.md — unreviewed, must not carry
    # real traffic before external cryptographic review.
```

This is not applied. Editing the root manifest would force a full workspace rebuild while other agents are building.

## 6. Residual Risk

External cryptographic review would still be missing unless it happens first. `MIGRATION.md` says external review is step 1 and that nothing below matters until then at `crates/osl-ratchet-next/MIGRATION.md:380`. `DESIGN.md` classifies formal analysis as none at `crates/osl-ratchet-next/DESIGN.md:532` and implementation maturity as unreviewed and zero deployment at `crates/osl-ratchet-next/DESIGN.md:533`.

The re-decryption blocker would still be unresolved unless the product chooses plaintext cache, message-key retention, or a new once-only ingestion layer.

The recovery/reset channel would still have no forward secrecy. `MIGRATION.md` says SKDM request and session reset stay on `encrypt_v3` at `crates/osl-ratchet-next/MIGRATION.md:105`, and `DESIGN.md` records that weakness at `crates/osl-ratchet-next/DESIGN.md:536`.

Multi-recipient Discord sends would still not be covered. OSL-RN is pairwise only, as stated at `crates/osl-ratchet-next/DESIGN.md:527`.

Multi-device/session sync would still not be solved. `DESIGN.md` marks it weaker at `crates/osl-ratchet-next/DESIGN.md:528`.

Capability authentication would still depend on the identity/TOFU layer. `verify_peer_capabilities` explicitly does not close whole-record identity substitution at `crates/keystore/src/client.rs:295`.

Stale capability policy would still be unproven. The current code has no production call to `verify_peer_capabilities` and no capability freshness state.

Session persistence rollback would still be a high-value failure mode. `wire_rn` writes atomically at `crates/ipc/src/wire_rn.rs:726`, but rollback resistance is a system property, not just an atomic-write property.

Skipped-key availability would still be a real regression under large gaps. Defaults cap one-message skips at 512 at `crates/osl-ratchet-next/src/skipped.rs:78`. `MIGRATION.md` says a larger burst with only the last message arriving is refused at `crates/osl-ratchet-next/MIGRATION.md:335`.

Bootstrap without one-time prekeys would still have the first-message cost described in `MIGRATION.md` at `crates/osl-ratchet-next/MIGRATION.md:197`.

Older builds would still silently drop RN overlay rows. `MIGRATION.md` records this at `crates/osl-ratchet-next/MIGRATION.md:79`; `session.rs` records the same failure at `crates/osl-ratchet-next/src/session.rs:143`.

There is a stale internal doc header in `crates/osl-ratchet-next/src/session.rs:4` saying wire format version `0x06`, while the authoritative constant is `0x10` at `crates/osl-ratchet-next/src/session.rs:148`. This report does not fix it.

No build or test was run for this audit, by instruction.
