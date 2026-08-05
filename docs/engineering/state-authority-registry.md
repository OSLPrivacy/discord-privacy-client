# State-authority registry

This registry is a change-time contract. Before adding a persisted datum, flag,
or worker, add one record below and have its owner review it. A record has one
writer: other components request mutations through that authority. Readers and
every lifecycle reset are recorded so review can find stale copies and cleanup
obligations.

`scripts/check-state-authority.py` validates this document in CI. Its JSON fence
is deliberately machine-readable without a third-party parser. Do not replace a
required field with `TBD`, an issue link, or a generic statement: describe the
actual serialization, migration, downgrade, and lifecycle behavior. `reuse`
must explicitly say which existing ledger/contract was considered and why it is
not the authority; `conflicts` names competing sources of truth or says why none
exists. `mirrors` is required whenever a datum is represented on multiple
native/renderer/website surfaces, and every mirror needs a concrete drift test.

`paths` lists all files that implement the datum. On pull requests, CI compares
added lines for storage writes and worker creation against those paths; adding a
new one without a record fails the gate.

The gate is **file-granular, not line-granular**: listing a large file in `paths`
stops the gate flagging *every* stateful line in that file, not just the lines
belonging to the declared datum. `apps/osl-hub/src/main.rs` is the live example
— it carries the onboarding-preferences command at line 646 and nineteen other
unrelated `spawn`/`write_all` sites. Declaring it below is honest (it does
implement the datum) but it costs coverage of the other nineteen, which are
listed as open items. Do not use this as a shortcut: adding a big file to a
record to make the count go down is exactly the failure this registry exists to
prevent.

```json
{
  "records": [
    {
      "id": "hub-onboarding-preferences",
      "datum": "The OSL hub's install-scoped onboarding/preview preferences (onboarding_complete, send_mode, placement_mode, show_plaintext_preview, window_capture_enabled, acknowledge_experimental_send_risk, forward_secrecy_mode), persisted as app_config_dir/preview-preferences.json.",
      "kind": "persisted-state",
      "authority": {
        "owner": "native preferences module (PreviewState)",
        "path": "apps/osl-hub/src/preferences.rs",
        "reason": "write_preferences (preferences.rs:84-95) is the only function that writes preview-preferences.json, and PreviewState::save (preferences.rs:51-65) is its only caller. Every mutation reaches it through the save_onboarding_preferences Tauri command (main.rs:646); nothing else opens the file."
      },
      "readers": [
        {
          "actor": "startup load (PreviewState::load, called once from the Tauri setup closure at main.rs:9629)",
          "path": "apps/osl-hub/src/preferences.rs"
        },
        {
          "actor": "Tauri command get_onboarding_preferences (main.rs:603-606)",
          "path": "apps/osl-hub/src/main.rs"
        },
        {
          "actor": "capture-protection gate in prepare_peer_prose_text, reads window_capture_enabled (main.rs:5735)",
          "path": "apps/osl-hub/src/main.rs"
        },
        {
          "actor": "renderer preferences client, reads over IPC via invoke(get_onboarding_preferences) (preferences.ts:36-40)",
          "path": "apps/osl-hub-ui/src/preferences.ts"
        }
      ],
      "writers": [
        {
          "actor": "PreviewState::save -> write_preferences -> atomic_file::write_recoverable",
          "path": "apps/osl-hub/src/preferences.rs"
        }
      ],
      "resets": [
        {
          "actor": "execute_full_hub_cleanup deletes preview-preferences.json and its .tmp/.bak siblings (cleanup.rs:383-397, driven by main.rs:6656-6688)",
          "path": "apps/osl-hub/src/cleanup.rs"
        },
        {
          "actor": "execute_verified_gate_burn and its crash-resume path resume_interrupted_gate_burn (cleanup.rs:169-217, main.rs:1416-1449 and main.rs:9624) delete the same targets",
          "path": "apps/osl-hub/src/cleanup.rs"
        },
        {
          "actor": "load-time reset to OnboardingPreferences::default() whenever read_preferences returns None -- file missing, over 16 KiB, malformed, unknown field, or version != 1 (preferences.rs:33-36, 73-81)",
          "path": "apps/osl-hub/src/preferences.rs"
        }
      ],
      "graphs": {
        "dependency": "No dependency-graph document exists in this repository (docs/engineering/ contains only this registry), so the placement is stated here instead of linked: apps/osl-hub/src/main.rs (Tauri commands + setup closure) -> apps/osl-hub/src/preferences.rs -> apps/osl-hub/src/atomic_file.rs -> app_config_dir/preview-preferences.json, with apps/osl-hub/src/models.rs supplying the serde shape. apps/osl-hub-ui/src/preferences.ts depends on it only through the two Tauri commands. Nothing depends on it in the other direction.",
        "interaction": "No interaction-graph document exists in this repository, so the placement is stated here: (1) boot -- setup step 14 (main.rs:9629-9631) loads the file after gate-burn resume (main.rs:9624) and identity selection, and before HubCoreState::bootstrap_from_disk; (2) renderer read -- preferences.ts:38 invoke(get_onboarding_preferences) -> main.rs:603-606 -> PreviewState::get; (3) renderer write -- preferences.ts:55 invoke(save_onboarding_preferences) -> main.rs:646 -> PreviewState::save; (4) send-path read -- main.rs:5735 consults window_capture_enabled while preparing peer prose; (5) destruction -- full cleanup or verified gate burn removes the file (cleanup.rs:383-397)."
      },
      "lifecycle": {
        "serialization": "serde_json, pretty-printed on write (to_vec_pretty, preferences.rs:89) and from_slice on read (preferences.rs:80). The document is PreferencesDocument { version: u8, onboarding: OnboardingPreferences } with #[serde(rename_all = \"camelCase\", deny_unknown_fields)] (preferences.rs:12-16). PREVIEW_STATE_VERSION = 1 (preferences.rs:8). Bounded at MAX_PREFERENCES_BYTES = 16 KiB, enforced on both read (preferences.rs:73-77) and write (preferences.rs:91-93).",
        "migration": "There is no forward migration code. read_preferences accepts the document only when document.version == PREVIEW_STATE_VERSION (preferences.rs:81); any other version is discarded and PreviewState::load falls back to unwrap_or_default() (preferences.rs:33-36). Two individual fields carry serde defaults so a newer build tolerates an older file that lacks them: window_capture_enabled has #[serde(default = \"default_true\")] and forward_secrecy_mode has #[serde(default)] (models.rs:47, 50, 84, 87).",
        "default": "OnboardingPreferences::default() (models.rs:58-70): onboarding_complete=false, send_mode=Manual, placement_mode=Atomic, show_plaintext_preview=true, window_capture_enabled=true, acknowledge_experimental_send_risk=false, forward_secrecy_mode=KeepGroupDelivery (models.rs:36-37). This is the value used whenever the file is absent, oversized, malformed, version-mismatched, or contains an unknown field.",
        "downgrade": "Asymmetric and lossy by construction. deny_unknown_fields is set on both PreferencesDocument (preferences.rs:12) and the WirePreferences deserialization shim (models.rs:78), so an older build reading a file written by a newer build that added any field fails to deserialize; read_preferences returns None (preferences.rs:80) and the ENTIRE document is discarded to defaults, not just the unknown field. A downgrade therefore silently resets every preference, including window_capture_enabled. The next save by the older build overwrites the newer file.",
        "startup": "Loaded exactly once per process, in the Tauri setup closure at main.rs:9629-9631 (breadcrumb setup_step_14_preview_state_managed): after config_dir/local_data_dir resolution, after keystore::set_active_account_dir(None), after peer_attachment_io::scavenge_staging_on_startup, after cleanup::resume_interrupted_gate_burn (main.rs:9624), and after identity_registry::select_active_identity_before_bootstrap; before TorPreferenceState/ServiceRegistryState are managed and before HubCoreState::bootstrap_from_disk.",
        "restart": "Re-read from disk on every process start by the same setup step; there is no in-memory carry-over and no code path distinguishes a restart from a first launch. A restart after a successful save observes the saved value; a restart after a version-mismatched or corrupt write observes defaults.",
        "crash": "Write is atomic via atomic_file::write_recoverable (preferences.rs:94, atomic_file.rs:57-91): write preview-preferences.tmp, sync_all, rename any existing file to preview-preferences.bak, rename .tmp over the real path, restore .bak if that final rename fails, then delete the stale .bak. Read recovers from the .bak sibling when the primary is missing (atomic_file.rs:10-32). A crash at any point therefore leaves either the prior value or the new one, never a truncated file.",
        "account_switch": "Not account-scoped and deliberately untouched: switch_hub_identity (main.rs:6603-6624), identity_registry::switch_identity_slot (identity_registry.rs:211-266) and identity_registry::reset_account_scoped_state (identity_registry.rs:944-970) contain no reference to PreviewState or preview-preferences.json. The file lives in app_config_dir, not the per-account directory, so a switched account inherits the same preferences. Verified by absence: no code in apps/osl-hub/src reads or writes it during a switch.",
        "teardown": "In-app teardown deletes it. cleanup_targets() lists preview_preferences -> preview-preferences.json plus its .tmp/.bak/.tmp staging siblings (cleanup.rs:383-397), removed by purge_fixed_targets via remove_target_without_following_links (cleanup.rs:307-319), reached from execute_full_hub_cleanup (cleanup.rs:113-164) and execute_verified_gate_burn (cleanup.rs:169-217). There is no OS-uninstall hook in apps/osl-hub/src; uninstall-time removal is not implemented here."
      },
      "reuse": "Considered apps/osl-hub/src/tor_pref.rs (TorPreferenceState / tor-preference.json, tor_pref.rs:133-144, 327-337), which is the nearest existing preference ledger and uses the same atomic_file::write_recoverable primitive. It is not the authority for this datum: it is a separate file with its own PersistedPreference shape covering only the network route choice, and folding onboarding state into it would make a network-route rollback also roll back onboarding. Also considered the account-scoped state under identity_registry (identity_registry.rs:944-970); rejected because these preferences are install-scoped and must survive an account switch.",
      "conflicts": "One competing store exists and is named deliberately rather than hidden. apps/osl-hub-ui/src/preferences.ts branches on isTauriRuntime() (preferences.ts:32-34): inside the desktop app both load and save go through the native commands, but in a plain browser saveOnboardingPreferences writes localStorage keys 'osl-preview-setup' and 'osl-preview-onboarded' instead (preferences.ts:59-62) and loadOnboardingPreferences reads them back (preferences.ts:44-45). The two stores are mutually exclusive within a process, the browser store holds only 2 of the 7 fields (the rest are hardcoded at preferences.ts:46-48), and nothing native ever reads localStorage. The native file is the authority for every shipping desktop build; the localStorage path is reachable only when __TAURI_INTERNALS__ is absent.",
      "paths": [
        "apps/osl-hub/src/preferences.rs",
        "apps/osl-hub/src/models.rs",
        "apps/osl-hub/src/atomic_file.rs",
        "apps/osl-hub/src/main.rs",
        "apps/osl-hub/src/cleanup.rs",
        "apps/osl-hub-ui/src/preferences.ts",
        "apps/osl-hub-ui/src/state.ts"
      ],
      "mirrors": [
        {
          "surface": "native",
          "path": "apps/osl-hub/src/models.rs",
          "drift_assertion": "models.rs:289 send_modes_match_the_frontend_contract asserts the exact camelCase wire JSON of OnboardingPreferences::default() and round-trips it back, and pins all four SendMode discriminants."
        },
        {
          "surface": "renderer",
          "path": "apps/osl-hub-ui/src/preferences.ts",
          "drift_assertion": "apps/osl-hub-ui/src/state.test.ts:81 'strictly accepts the Rust camelCase contract', plus state.test.ts:115 'rejects unknown Rust response fields' and preferences.test.ts:94 'sends the resolved first-run preference contract through the native boundary', which pins the payload the renderer hands to save_onboarding_preferences."
        }
      ]
    },
    {
      "id": "keyserver-space-event-queue",
      "datum": "The opaque Space membership-event relay queue (keyserver D1 table space_event_queue): one row per undelivered ciphertext envelope, addressed only by a rotating 32-byte recipient delivery tag.",
      "kind": "persisted-state",
      "authority": {
        "owner": "keyserver space-events endpoint",
        "path": "keyserver-cf/src/endpoints/space-events.ts",
        "reason": "The only file in keyserver-cf/ that issues INSERT/SELECT/DELETE against space_event_queue (space-events.ts:48, 59, 62). A repository-wide grep for space_event_queue returns this endpoint, migration 0041, the step-order gate list, and the t21 source-text test -- no second module and no KV/Durable Object holds the same datum."
      },
      "readers": [
        {
          "actor": "handleSpaceEventDrain, the only reader: SELECT id, ciphertext WHERE recipient_tag = ? AND expires_at > ? ORDER BY created_at LIMIT 64 (space-events.ts:59)",
          "path": "keyserver-cf/src/endpoints/space-events.ts"
        },
        {
          "actor": "request router, dispatches GET drain and POST enqueue (index.ts:288-289)",
          "path": "keyserver-cf/src/index.ts"
        }
      ],
      "writers": [
        {
          "actor": "handleSpaceEventPost, the single INSERT (space-events.ts:48), reached only from index.ts:289",
          "path": "keyserver-cf/src/endpoints/space-events.ts"
        }
      ],
      "resets": [
        {
          "actor": "drain deletes each returned row in the same request: DELETE FROM space_event_queue WHERE id = ? AND recipient_tag = ? (space-events.ts:62)",
          "path": "keyserver-cf/src/endpoints/space-events.ts"
        },
        {
          "actor": "recipient-tag rotation, which resets the addressable row set without deleting anything: the table has no account/Space/roster column (migrations/0041 header), so a rotated tag simply addresses a disjoint set",
          "path": "keyserver-cf/migrations/0041_space_event_queue_reserved.sql"
        }
      ],
      "graphs": {
        "dependency": "No dependency-graph document exists in this repository (docs/engineering/ contains only this registry), so the placement is stated here: keyserver-cf/src/index.ts (router, lines 93 and 288-289) -> keyserver-cf/src/endpoints/space-events.ts -> env.DB (D1) -> table created by keyserver-cf/migrations/0041_space_event_queue_reserved.sql. The endpoint depends on nothing else in keyserver-cf/src -- no users table join, no identity lookup, no auth middleware.",
        "interaction": "No interaction-graph document exists in this repository, so the placement is stated here: (1) enqueue -- POST /v1/space-events with {recipient_tag, ciphertext, expires_at}; the envelope is validated (32-byte tag, 1..65536-byte ciphertext, safe-integer future expiry, space-events.ts:44-46) and inserted with a fresh 16 random bytes as id; answers 202. (2) drain -- GET with a tag returns up to 64 unexpired ciphertexts oldest-first and deletes each one it returned. (3) There is no third interaction: no push, no notification, no expiry sweep."
      },
      "lifecycle": {
        "serialization": "A D1/SQLite row, not a serialized document: id BLOB PRIMARY KEY (16 bytes from crypto.getRandomValues, space-events.ts:26-30), recipient_tag BLOB (exactly 32 bytes, TAG_BYTES), ciphertext BLOB (1..65536 bytes, MAX_CIPHERTEXT_BYTES), expires_at INTEGER (client-supplied unix seconds), created_at INTEGER (server unix seconds). On the wire recipient_tag and ciphertext are base64 (atob/btoa, space-events.ts:13-24); they are decoded to bytes before insert and re-encoded on drain, so the stored form is raw bytes. The relay never parses the ciphertext.",
        "migration": "Created by keyserver-cf/migrations/0041_space_event_queue_reserved.sql with CREATE TABLE IF NOT EXISTS plus two indexes (recipient_tag, created_at) and (expires_at). No later migration in keyserver-cf/migrations/ or keyserver-cf/migrations-contract/ alters it -- 0041 is the only file that names the table. The migration is pinned in the expand/contract deploy step order at keyserver-cf/scripts/expand-contract-step-order.mjs:177.",
        "default": "Absent. No row exists until a POST is accepted, and a drain for an unknown or empty tag returns HTTP 200 {\"events\": []} (space-events.ts:63-65) -- indistinguishable from a tag whose rows have all been drained.",
        "downgrade": "0041 is expand-only (CREATE TABLE IF NOT EXISTS, additive indexes) and has no contract half in keyserver-cf/migrations-contract/. A Worker generation older than the routes at index.ts:288-289 never reaches the table and is unaffected by its presence; a newer schema cannot break it because the schema has not changed since creation. This is the safe side of the rule in keyserver-cf/migrations-contract/README.md:63-65 -- 'Contracting is one-way for the Worker' -- which applies to tables that HAVE a contract migration; this one does not, so rolling the Worker back is not an outage for this datum.",
        "startup": "There is no startup step. Cloudflare Workers hold no process state, and both handlers read and write env.DB directly on each request (space-events.ts:48, 59); there is no in-memory queue, cache, or warm-up to sequence. The table's availability is a function of migration 0041 having been applied, not of any boot ordering.",
        "restart": "Unaffected. Because nothing is held in the isolate, a Worker restart or eviction loses no queued event -- every row is already durable in D1 at the moment the 202 is returned. A restart mid-drain can lose only the in-flight HTTP response; rows already DELETEd stay deleted and rows not yet DELETEd stay drainable.",
        "crash": "Each statement is an independent D1 prepare().run() -- the enqueue is a single INSERT and the drain is a SELECT followed by a per-row DELETE loop (space-events.ts:62), not a batch or transaction. A crash between the INSERT and the 202 leaves a stored, drainable row while the client sees a failure; because id() is fresh random bytes on every call and there is no idempotency key, a client retry inserts a SECOND row and the recipient drains the same envelope twice. A crash partway through a drain leaves the already-DELETEd rows gone and the rest drainable on the next call, so a partial drain never loses an envelope.",
        "account_switch": "Invisible to the server, by design. migrations/0041's header states 'no account, roster, Space id, or plaintext event is stored here' and the schema has no such column; routing is exclusively by recipient_tag (space-events.ts module doc). An account switch is therefore not an event the keyserver can observe -- the switched client presents a different tag and drains a disjoint row set, while rows addressed to the previous tag remain until drained or expired. Nothing server-side is cleared, because nothing server-side links a tag to an account.",
        "teardown": "Incomplete, and stated as measured rather than as intent. Delivery deletes rows (space-events.ts:62). Expired rows are excluded from every drain by 'expires_at > ?' (space-events.ts:59) but are NEVER deleted: the cron privacy sweep sweepExpiredPrivacyRows (keyserver-cf/src/lib/db.ts:390-457, invoked from index.ts:223) enumerates six tables and space_event_queue is not one of them, and account deletion (unregisterUserIfCurrent, lib/db.ts:219-304) cannot reach the table because there is no account column to match on. An undrained envelope therefore persists past its expires_at indefinitely. See the open items below -- this is recorded, not resolved."
      },
      "reuse": "Considered the keyserver's existing per-account durable stores -- control_inbox and wrapped_keys, both swept by sweepExpiredPrivacyRows (lib/db.ts:390-457) -- as the home for this datum. Neither can hold it: every one of them is keyed by user_id, and the stated purpose of 0041 (its header comment, and the module doc at space-events.ts:1-5) is that the relay must not learn which account a Space event belongs to. Storing Space events in a user_id-keyed table would let the relay enumerate a Space, which is exactly the property the table exists to deny. A separate, account-free store is therefore deliberate rather than duplicative.",
      "conflicts": "No competing server-side source of truth: a full grep of keyserver-cf/ shows space_event_queue is touched only by endpoints/space-events.ts, migration 0041, the step-order gate list (expand-contract-step-order.mjs:177), and a source-text assertion (scripts/t21-space-events.test.mjs:9). No KV namespace or Durable Object mirrors it. The client-side half of the contract -- what rotates the recipient tag and how a client decides a tag is stale -- was not located in this repository and is listed as an open item; until it is, the client's view of 'which tag am I draining' is an undeclared counterpart, not a declared mirror.",
      "paths": [
        "keyserver-cf/src/endpoints/space-events.ts",
        "keyserver-cf/src/index.ts",
        "keyserver-cf/migrations/0041_space_event_queue_reserved.sql"
      ]
    }
  ]
}
```

## Record shape

```text
{
  "id": "example-sync-enabled",
  "datum": "Whether account sync is enabled",
  "kind": "flag",
  "authority": {"owner": "native settings service", "path": "apps/example/src/settings.rs", "reason": "only durable settings writer"},
  "readers": [{"actor": "renderer preferences", "path": "apps/example-ui/src/preferences.ts"}],
  "writers": [{"actor": "native settings service", "path": "apps/example/src/settings.rs"}],
  "resets": [{"actor": "account switch", "path": "apps/example/src/session.rs"}],
  "graphs": {"dependency": "docs/engineering/dependency-graph.md#settings", "interaction": "docs/engineering/interaction-graph.md#account-switch"},
  "lifecycle": {"serialization": "versioned settings JSON", "migration": "v1 missing key defaults false", "default": "false for new accounts", "downgrade": "older clients ignore the additive key", "startup": "native service loads before renderer boot", "restart": "reload from settings JSON", "crash": "atomic write preserves prior value", "account_switch": "discard account-scoped cache then reload", "teardown": "remove account settings on account deletion"},
  "reuse": "Reviewed existing account settings ledger; this is a field in that ledger, not a new store.",
  "conflicts": "No second truth: renderer cache is read-only and receives snapshots from native.",
  "paths": ["apps/example/src/settings.rs", "apps/example-ui/src/preferences.ts"],
  "mirrors": [{"surface": "native", "path": "apps/example/src/settings.rs", "drift_assertion": "settings integration test compares serialized value"}, {"surface": "renderer", "path": "apps/example-ui/src/preferences.ts", "drift_assertion": "preferences test compares IPC snapshot"}]
}
```

## Open items — files the gate flags that are deliberately NOT declared

Measured 2026-08-04 on `fix/state-authority-registry`. Of the 23 files the gate
flags across the ranges CI actually uses, **2 are now declared** and **21 are
deliberately left undeclared**. Every one below is left undeclared because the
source does not answer a specific question, and that question is written out
here. A guessed record is worse than none: a wrong record is a *declared*
authority, and review would then trust it.

### A. A real datum, blocked on a question only the owner can answer

1. **`apps/osl-hub/src/eager_fetch.rs`** — the encrypted offline burn outbox
   (`EncryptedBurnQueue`, `BurnQueueDocument`). Its serialization, bounds,
   crash-safety and idempotency are all fully answered in the file itself.
   **Unanswered: `EncryptedBurnQueue::new` has no production caller.** The only
   four constructions in the tree are in `apps/osl-hub/tests/` with literal keys
   (`[7; 32]`, `[3; 32]`) and temp paths; `apps/osl-hub/src/osl_chat_delivery.rs`
   takes an already-built `EagerFetchDriver` and never builds a queue.
   *Question for the owner:* where does the shipping burn outbox live on disk,
   and where does its 32-byte at-rest key come from? Until that exists there is
   no honest answer for `authority.path`, `startup`, `account_switch`, or
   `teardown`, and `resets` cannot be inventoried.

2. **`keyserver-cf/src/endpoints/usernames.ts`**,
   **`keyserver-cf/migrations-contract/0100_username_identity_contract.sql`**,
   **`keyserver-cf/migrations-contract/0101_account_ownership_binding_contract.sql`**,
   **`keyserver-cf/test/integration/mail.test.ts`** — the username directory
   (`username_directory`, `username_tombstones`, `username_claim_receipts`).
   Serialization, migration order, rotation-to-tombstone, expand/contract
   downgrade doctrine and account-deletion teardown are all answered by the
   source. **Unanswered: this datum has three production writers, and the
   registry contract permits exactly one.** They are
   `endpoints/usernames.ts:136-152` (claim/rename), `lib/db.ts:161-176`
   (`rotateUserKeys`, from `endpoints/register.ts:391`) and `lib/db.ts:262-273`
   (`unregisterUserIfCurrent`, from `endpoints/unregister.ts:108`), plus the
   DB-level trigger `username_directory_retire_before_delete` from migration
   0038. *Question for the owner:* which of the three is the authority, and do
   the other two route their mutations through it — or is the single-authority
   rule not the right shape for a table with a claim path and two retirement
   paths? Writing a record today would mean naming one writer and silently
   demoting two real ones to "readers", which is a fabrication.

   *Separately noticed while reading, and flagged rather than fixed (not this
   lane's file):* the production writer stores the **raw username** into both
   `username_skeleton` and `display_username`
   (`endpoints/usernames.ts:145-147`, `SELECT ?1, ?1, ?1`). A real UTS-39
   confusable `skeleton()` exists at
   `keyserver-cf/src/lib/unicode-identifier/runtime.ts:11`, and nothing under
   `keyserver-cf/src/endpoints/` or `lib/db.ts` imports it. If that is
   intentional the comment should say so; if not, the confusable-name defence
   that migration 0038 and contract 0100 were built for is not actually running.

3. **`keyserver-cf/test/integration/devices.test.ts`** — writes
   `device_roster`. *Question:* the device roster is a real persisted datum but
   was not traced in this lane. Who is its single writer, and what happens to a
   roster row on device revocation and on account deletion?

4. **`keyserver-cf/test/integration/redemption-privacy.test.ts`** — writes
   `subscriptions` and `licenses`. *Question:* these are billing-lifetime
   records with an external source of truth (the payment processor). Which side
   is the authority, and what is the reconciliation contract when they disagree?

5. **`cipher-store-cf/test/blob-bounds.test.ts`**,
   **`cipher-store-cf/test/d81-fetch-carries-no-identity.test.ts`**,
   **`cipher-store-cf/test/decoy-indistinguishable.test.ts`**,
   **`cipher-store-cf/test/rate-limit-atomic.test.ts`** — write
   `blob_capability_index` and `blobs`. *Question:* the cipher store is a
   separate lane's subsystem and was not traced here. What resets a capability
   index row on burn, and does an expired blob's index row survive it?

6. **`keyserver-cf/scripts/keyserver-red-mutants.mjs`** — contains the
   `mail_address_epochs` INSERT/UPDATE statements as *string literals it patches
   into other files* to prove tests go red. It writes nothing itself, but the
   datum it names (mail address epochs and their tombstoning) is real and
   undeclared. *Question:* who is the single writer of `mail_address_epochs`,
   and what is the tombstone retention?

### B. Flagged by the regex but not a state authority at all

These matched `STATEFUL_CHANGE` inside test modules, harnesses, or build
tooling. **The regex is not being narrowed to silence them** — narrowing it
would be the "shrink what it inspects" move this registry forbids, and the same
broad pattern is what catches a genuine new store. They are recorded here so a
future lane does not re-derive the classification:

* `apps/osl-hub/src/browser_profile_scan.rs:1079,1164` — `INSERT INTO urls`
  inside `#[cfg(test)] mod tests` (starts line 685). Builds a throwaway SQLite
  `History` fixture; the production scanner only reads.
* `apps/osl-hub/src/diagnostics.rs:212` — `write_all` inside `#[cfg(test)]`
  (starts line 181).
* `crates/ipc/src/cipher_store_client.rs:1224-1486` and
  `crates/ipc/tests/rn_shipping_default_gate.rs:88,106` — `thread::spawn` and
  `stream.write_all` in mock HTTP servers. Nothing is persisted.
* `keyserver-cf/scripts/expand-contract-step-order.mjs` — extracts SQL text from
  Worker bundles and replays it against an in-memory `node:sqlite` clone. Never
  touches a live D1.
* `keyserver-cf/scripts/t21-space-events.test.mjs:9` — asserts the *source text*
  of `space-events.ts` still contains the INSERT. A drift assertion, not a write.
* `scripts/build-download-meta.mjs:40`, `scripts/publish-installer.mjs:35,71`,
  `scripts/fleet/model-routing-benchmark.py:137` — build/release tooling writing
  its own output artifacts.

*Question this class raises for the owner:* should the registry gain a way to
declare "this file matched, and here is why it is not a state authority" — an
explicit classification with a reviewed reason — rather than either declaring a
fake datum or leaving the gate red? That is a contract change, not a triage
call, so it is not made here.

### C. Coverage lost to file granularity

`apps/osl-hub/src/main.rs` is declared for onboarding preferences, which stops
the gate flagging its other nineteen stateful sites: the QA-shell timing/stage
writers (388, 488, 489), sealed/byte writes (1619, 1715), the OS
`Command::spawn` release-page openers (1929, 1955), and the eleven thread /
async-runtime spawns (1130, 5267, 8193, 8318, 8724, 8836, 9247, 9523, 9531,
9685, 9696, 9716). None of these are declared. *Question:* several are long-
lived workers (`spawn_trigger_watcher`, `spawn_lifecycle_tick`,
`entitlement_refresh::spawn`, `revocation_drain_timer::spawn`) and the registry
has a `worker` kind for exactly this — each needs its own record with a
supervisor, a restart policy and a teardown join.
