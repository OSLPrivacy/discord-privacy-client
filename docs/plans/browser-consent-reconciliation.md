# Browser Profile Consent Reconciliation

Date: 2026-07-29

Current tree inspected: `unit-f14` at `16778b297d3e`.

Comparison line inspected for analysis only: `codex/f1-browser-footprint` at
`61933d3a4b50`, worktree `/home/<user>/discord-privacy-client-f1-footprint`.

This is a reconciliation specification, not a merge. Do not copy code between
lines. The future implementation must be a semantic port into the current tree.

## Current Tree Model

`unit-f14` does not contain `apps/osl-hub/src/browser_footprint.rs` or
`apps/osl-hub/src/browser_profile_scan.rs`. The browser-related model in this
tree is split between browser import/launch DTOs and the optional browser
companion:

- `apps/osl-hub/src/native_apps.rs` defines `BrowserImportId` as the fixed
  browser enum: Chrome, Edge, Firefox, Brave, Opera, DuckDuckGo.
- `BrowserImportStatus` reports `{ id, display_name, installed }`.
- `BrowserImportResult` reports `{ id, opened }`.
- `BrowserAccountImportResult` reports `{ preferred_source, detected_sources,
  opened, mode, manual_export_required }`.
- `ProtectedBrowserImportResult` reports `{ selected_sources, started, mode,
  source_selected, manual_fallback }`.
- `apps/osl-hub/src/browser_companion.rs` defines `BrowserAccountMode` as
  `ExistingBrowser` or `IsolatedOsl`.
- `BrowserCompanionStatus` reports `{ status, browser_id, display_name, reason,
  capture_protected, containment }`.
- `BrowserCompanionAction` reports `{ status, browser_id, reason, mode,
  capture_protected, containment }`.

The current tree has no production browser-profile content consent model. Its
profile-related browser operations are launch/import shell operations:

- `list_browser_imports` detects fixed browser executables in fixed locations.
- `open_browser_import` opens a fixed browser import path.
- `begin_browser_account_import`, `begin_protected_browser_import`, and
  `finish_protected_browser_import` drive Firefox migration/import flows for the
  active unlocked owner, but they do not scan arbitrary browser profile content
  or mint profile-content consent receipts.
- `host_default_browser_companion` can host an existing browser session or an
  OSL-created isolated browser companion profile. That profile is owner-scoped
  and constructed internally; no renderer-supplied profile path crosses the
  boundary.

The native Discord QA driver still recognizes browser-profile verbs, but in
this tree they are explicit refusals:

- `ListBrowserProfiles` returns `list-browser-profiles-unavailable`.
- `GrantBrowserProfile` returns `grant-browser-profile-unavailable`.
- `RevokeBrowserProfile` returns `revoke-browser-profile-unavailable`.
- `RunBrowserImport` returns `run-browser-import-unavailable`.

Therefore, in `unit-f14`, absence of browser-profile consent means refusal by
construction: no production command exists to read profile content, and the QA
verbs are hard fail-closed.

## F1 Footprint Model

The f1 line adds two modules and production commands for browser footprint
discovery. The model has two distinct consent gates.

### One-Shot Profile Scan Grant

`apps/osl-hub/src/browser_profile_scan.rs` defines the scan-time consent model:

- `BrowserProfileDescriptor` reports `{ browser_id, profile, display_name }`.
  It is inventory metadata only. Inventory reads directory labels; it must not
  open History, `places.sqlite`, Login Data, Web Data, login stores, cookies, or
  any profile content.
- `BrowserProfileConsentGrant` reports `{ grant_id, browser_id, profile,
  expires_at_unix_ms }`.
- `PendingConsentGrant` stores `{ owner, browser_id, profile, grant_id,
  expires_at_unix_ms }` in memory.
- `BrowserProfileScanState` owns `{ snapshot_root, listed_profiles, grants,
  scan }`.

The flow is:

1. `list_profiles` builds a bounded inventory of allowed profile directory
   labels for supported browsers.
2. `grant_profile_consent` requires an active owner, a valid browser/profile,
   a fresh inventory match, and a supported browser. It mints a short-lived
   random grant.
3. `scan_consented_profile` consumes the exact owner/browser/profile/grant
   before resolving or opening the profile.
4. After consent is consumed, the scanner resolves only the allowed profile
   directory under the installed browser root, snapshots the browser history
   database into an OSL-owned temporary directory, queries only bounded URL host
   observations, deletes and verifies the snapshot, then commits the footprint.

The supported history scan account is fixed as a history footprint account, not
renderer supplied. DuckDuckGo is intentionally refused for history scanning.

### Footprint Persistence And Hydration Consent

`apps/osl-hub/src/browser_footprint.rs` defines the persisted footprint model:

- `FootprintObservation` carries `{ service, site, observed_handle,
  source_browser, source_profile, source_account, confidence }`.
- `NativeBrowserImportBinding` carries `{ browser_id, profile, account, scope,
  run_id }`.
- `BrowserFootprintConsent` carries `{ browser_id, profile, account, run_id,
  consent }`.
- `NativeBrowserImportReceipt` carries `{ schema_version, authority,
  owner_binding_sha256, browser_id, profile, account, scope, run_id,
  build_sha256, generation, persisted_count, immediate_reread_count,
  observations_sha256, sealed_sha256, rollback_status }`.
- `BrowserFootprintHydration` carries `{ schema_version, owner_binding_sha256,
  generation, imports, observations, rollback_status }`.
- `BrowserFootprintRevokeReceipt` carries `{ schema_version, authority,
  owner_binding_sha256, browser_id, profile, account, scope, generation,
  removed_count, immediate_reread_count, sealed_sha256, rollback_status }`.
- The sealed document contains owner rows and stored imports. Stored imports are
  keyed by the exact native import binding and contain observation digests.

The flow is:

1. Native scan commits only a nonempty, bounded observation batch whose
   observation fields match the native binding.
2. Commit writes the encrypted owner-bound document, rereads the exact committed
   bytes, verifies generation/count/digests/build hash, then returns a receipt.
3. Hydration is not automatic. `hydrate_consented_for_owner` requires a nonempty
   bounded list of `BrowserFootprintConsent` entries, each with `consent == true`
   and an exact browser/profile/account/run match.
4. Empty consent, `consent == false`, duplicate consent, malformed fields, wrong
   run, wrong account, wrong profile, wrong scope, absent owner, missing sealed
   document, or invalid digest all refuse.
5. Revocation removes the exact owner/browser/profile/account/scope import and
   proves removal with a fresh reread.

The f1 model is default-deny in both gates: no scan without a fresh consumed
profile grant, and no persisted footprint hydration without explicit affirmative
per-import consent.

## Conflicts

The two lines diverge in these load-bearing places:

| Area | Current `unit-f14` | F1 footprint line | Reconciliation |
| --- | --- | --- | --- |
| Production browser-profile consent | Absent. QA browser-profile verbs are hard refusals. | Present as one-shot profile grants plus explicit footprint hydration consent. | F1's explicit consent gates are authoritative for any profile-content read or footprint hydration. |
| Browser identifier type | `BrowserImportId` enum at IPC and UI boundaries. | Scan descriptors use `BrowserImportId`; footprint bindings/receipts store lower-case strings. | Keep the enum at public command/UI boundaries. Convert to canonical lower-case tokens only inside sealed records/receipts. |
| Meaning of `profile` | OSL-created isolated companion profile path is internal and never returned. Current import DTOs do not expose native profile labels. | Native browser profile label, for example a directory label under a fixed browser root. | Keep these domains separate. Browser companion profiles are OSL-owned runtime profiles; footprint profiles are native inventory labels. Do not let either field satisfy the other. |
| Account binding | Current browser import/companion DTOs do not bind an account/run. | Footprint binding carries `account`, fixed `scope`, and `run_id`; history scan uses a fixed history account. | For history footprint, keep the fixed internal account and exact run binding. Do not invent renderer-supplied account authority. Future login/autofill import needs a separate consent scope. |
| Consent representation | No production consent object. Command descriptions mention explicit local consent for some launch flows but no native grant or hydration contract exists. | `BrowserProfileConsentGrant` authorizes one scan; `BrowserFootprintConsent` authorizes one persisted hydration item. | Use both f1 consent objects. Absence, empty arrays, expired grants, false consent, and mismatches must refuse. |
| Success claim | Current `opened`/`started` booleans prove launch/import wizard actions only. | Native import receipt proves encrypted persistence and immediate reread of nonempty observations. | Never treat `opened` or `started` as footprint success. Profile-content scan success requires `NativeBrowserImportReceipt`. |
| Display labels | Current `display_name` means browser app display name. | `display_name` means profile label display text. | Preserve exact DTO names only with context-specific TypeScript validators; do not merge these into a shared display-name model. |
| Persistence | Current browser import/companion path has no footprint store. | Encrypted owner-bound `browser-footprint.json` plus integrity companion. | Add footprint persistence only under the active owner and current file-storage key; no renderer owner field. |
| Module registration | Current `lib.rs` exposes `browser_companion`, not footprint modules. | F1 registers `browser_footprint` and `browser_profile_scan` behind `core`. | Add the modules behind `core` in the current tree during implementation. |
| Debug/Display leakage | Current browser DTO derives do not contain account identifiers or handles. | Several f1 structs derive `Debug` while carrying profile, account, run, site, or observed handle values. | The unified implementation must not derive `Debug` or implement `Display` in a way that emits account identifiers, handles, credentials, run IDs, profile labels, sites, or owner material. Use hand-written redacted impls or omit `Debug`. |

## Authoritative Model

For browser-profile footprint data, the f1 consent model is authoritative:

- A profile-content read requires a fresh, exact, one-shot native
  `BrowserProfileConsentGrant`.
- Persisted footprint hydration requires a nonempty explicit list of exact
  `BrowserFootprintConsent` entries with `consent == true`.
- The active owner is always derived from the unlocked OSL identity in native
  code. It is never accepted from the renderer.
- The profile path is always resolved from an installed browser root and an
  inventory-listed profile label. A renderer may never supply a path.
- The browser account/scope/run binding is load-bearing. Missing binding means
  refusal, not permission.
- The current browser companion and Firefox migration/import surfaces remain
  separate launch surfaces. They do not grant profile-content scan authority and
  their `opened`/`started` booleans cannot be upgraded into footprint receipts.

Consent remains default-deny. The unified code must treat absence of consent,
binding, owner, active identity, fresh inventory, exact grant, exact run,
supported browser, safe snapshot cleanup, encrypted storage key, or immediate
reread proof as refusal.

## Implementation Steps

1. Add `apps/osl-hub/src/browser_footprint.rs` and
   `apps/osl-hub/src/browser_profile_scan.rs` as semantic ports. Do not copy
   code from the f1 line.
2. Register both modules from `apps/osl-hub/src/lib.rs` behind the existing
   `core` feature, matching the current module style.
3. Reuse `native_apps::BrowserImportId` at public IPC boundaries. Add a local
   canonical token conversion helper for sealed bindings and receipts.
4. Implement the scan-time model in `browser_profile_scan.rs`: descriptor,
   one-shot grant, pending grant ledger, fresh inventory requirement, exact
   grant consumption before profile resolution, fixed profile-root resolution,
   bounded history snapshot, snapshot deletion proof, and native commit call.
5. Implement the persistence/hydration model in `browser_footprint.rs`: native
   binding, observations, receipt, hydration, revocation, encrypted owner-bound
   document, immediate reread, digest checks, and refusal on missing or false
   consent.
6. Do not derive `Debug` on any type containing owner material, profile labels,
   account identifiers, handles, sites, run IDs, or credentials. Hand-write
   redacted `Debug` only where tests or errors require it. Do not implement
   `Display` for these values unless it is also redacted.
7. Manage `BrowserFootprintState` and `BrowserProfileScanState` during Tauri
   startup. Use the current tree's `app_local_data_dir` and config directory
   conventions; fail startup rather than creating a scanner over uncleared
   snapshot bytes.
8. Add Tauri commands:
   - `list_browser_profiles_for_consent`
   - `grant_browser_profile_consent`
   - `scan_consented_browser_profile`
   - `load_detected_browser_footprint`
   - `revoke_detected_browser_footprint`
9. Every command must lock the existing account session transition where current
   neighboring commands do, derive the active unlocked owner natively, and reject
   without touching profile content when the owner is absent.
10. Add the matching permissions to `apps/osl-hub/permissions/hub.toml` with
    descriptions that state no owner, path, URL, source bytes, login store, or
    credential authority crosses IPC.
11. Keep the native Discord QA browser-profile pseudo-verbs fail-closed unless a
    separate F1 QA runtime admission path explicitly owns and proves them. The
    production Tauri commands are the source of truth, not QA trigger verbs.
12. Update `apps/osl-hub-ui/src/services.ts` with exact validators for the new
    DTOs. Reject unknown fields, duplicate profiles/receipts, empty consent
    arrays, false consent, invalid browser IDs, overlong display strings, and
    malformed hex digests.
13. Update onboarding/browser-footprint UI only to call hydration with receipts
    from the current user action. Do not hydrate browser footprints during
    bootstrap, identity refresh, or app selection without an explicit consent
    set.
14. Preserve the current browser companion/import commands as separate features.
    Do not route their `BrowserAccountMode`, `opened`, `started`, or
    `manual_fallback` fields into the footprint consent model.

## Unit Test Requirements

Write focused unit tests inline in the implementation files they exercise.
Do not add broad integration tests until the semantic port is accepted.

`browser_profile_scan.rs` tests:

- Inventory returns only allowed profile labels and does not require history or
  login database files.
- Grant refuses without fresh inventory.
- Grant refuses unsupported browsers and invalid profile labels.
- Scan refuses missing, malformed, expired, wrong-owner, wrong-browser,
  wrong-profile, and replayed grants.
- Grant is consumed before profile resolution; a failed profile read does not
  leave reusable consent.
- Profile resolution refuses path traversal, symlinks, escaped canonical roots,
  unsupported profile names, and DuckDuckGo history.
- Snapshot creation refuses oversized or symlinked history databases and bounded
  WAL mutations.
- Success removes the snapshot before committing observations.
- A nonempty history snapshot commits exactly one native receipt with matching
  browser/profile/account/scope/run binding.

`browser_footprint.rs` tests:

- Native commit refuses empty observations, malformed owner, malformed binding,
  mismatched observation source fields, unsupported confidence, and duplicate
  import scopes.
- Commit writes, rereads, and returns counts/digests only after the reopened
  sealed bytes match.
- Hydration refuses empty consent, `consent == false`, duplicate consent,
  malformed fields, wrong run, wrong browser, wrong profile, wrong account,
  wrong scope, absent owner, missing document with pin, invalid digest, and
  rollback/integrity companion mismatch.
- Hydration returns only consented exact imports and never all owner imports by
  default.
- Revocation refuses absent scopes, removes the exact scope, advances
  generation, and proves zero reread matches.
- Redacted `Debug`/`Display` tests prove no owner, profile label, account
  identifier, run ID, site, handle, or credential-shaped string appears in debug
  output.

`main.rs` and permission tests:

- `lib.rs` registers both modules behind `core`.
- Tauri command registration includes the five production footprint commands.
- The main-window capability grants only the five fixed permission identifiers.
- Browser-profile QA pseudo-verbs remain explicit refusals unless the separate
  F1 QA admission path is present and active.
- The current browser import/companion DTOs remain accepted by their existing
  parsers and do not gain footprint authority fields.

UI tests:

- `services.ts` validators reject unknown fields and invalid enum/hex/display
  values.
- `loadDetectedBrowserFootprint([])` rejects client-side before IPC.
- A scan receipt is converted to affirmative hydration consent only in the same
  explicit user flow.
- Bootstrap and identity refresh do not auto-hydrate detected browser
  footprints.
- Revoke calls remove only the selected receipt scope and reload with the
  remaining explicit consents.

## Stop Conditions

Stop instead of fabricating behavior if any of these prerequisites are absent in
the implementation parent:

- `native_apps::BrowserImportId` is missing or no longer has fixed browser enum
  semantics.
- The active unlocked owner cannot be derived in native code.
- The file-storage key/encrypted-at-rest helpers required for footprint
  persistence are unavailable.
- The available atomic file substrate cannot provide recoverable bounded reads
  and writes for the footprint document and pin.
- Startup cannot create and clean a fixed OSL-owned snapshot directory.
- The current module/permission/command registration style has changed enough
  that the five commands cannot be registered without inventing a new authority
  surface.

## Verification Policy

This reconciliation spec was produced without running Cargo, `osl-cargo`,
Nextest, or `cargo check`. Future implementation must also respect the current
batch-build policy: write focused code and inline tests, but leave compilation
to the central batched gate unless that policy is changed explicitly.
