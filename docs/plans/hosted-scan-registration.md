# Hosted Session Scan Registration Spec

Status: prerequisite-gated. Do not edit `apps/osl-hub/src/main.rs`,
`apps/osl-hub/capabilities/hub.json`, or `apps/osl-hub/permissions/hub.toml`
from this packet while those central serialized files are owned elsewhere.

## Current Source Facts

The requested Tauri command symbols are absent in this commit:

- `open_hosted_session_scan`
- `request_hosted_session_scan`

Both `rg` and `git grep` over `HEAD` returned no hits for those symbols. Because
they do not exist, their command argument lists cannot honestly be "read from
the real source" yet. Do not fabricate them.

The scan-only engine does exist, but it is not a Tauri command and has no caller:

```rust
// apps/osl-hub/src/native_discord_adapter.rs
pub fn scan_own_messages_for_deletion(
    host: &crate::native_window_host::NativeWindowHostState,
    owner_osl_user_id: &str,
    scope_binding: &str,
    generation: u64,
    operator_names: &[String],
) -> Result<guided_deletion::DeletionScan, String>
```

It delegates to the same bounded transcript reader already used for rehydrate,
then returns a content-free `guided_deletion::DeletionScan`:

```rust
// apps/osl-hub/src/native_discord_adapter/guided_deletion.rs
pub struct DeletionScan {
    pub scope_binding_hash: String,
    pub generation: u64,
    pub rows_seen: usize,
    pub rows_unreadable: usize,
    pub walk: WalkCompleteness,
    pub candidates: Vec<ScannedRow>,
}

pub struct ScannedRow {
    pub scan_ordinal: usize,
    pub shape_ordinal: usize,
    pub shape: RowShape,
    pub text_len: usize,
    pub authored_by_operator: bool,
}

pub struct RowShape {
    pub height_px: i32,
    pub children: u16,
}

pub enum WalkCompleteness {
    Complete,
    Truncated,
}
```

The scan engine's inputs are all native-held authority inputs except
`operator_names`. Current source has tests with fixed operator names, but no
reviewed production binding that derives those names from native trusted state
inside the command layer. Absence of that binding must mean refusal, never
permission.

## Registration Only

When the two missing command wrappers exist and their exact source signatures
are available, register exactly these command names and no deletion command:

```rust
// apps/osl-hub/src/main.rs, inside tauri::generate_handler![ ... ]
open_hosted_session_scan,
request_hosted_session_scan,
```

Place them near the existing local scan and Scrub entries, after
`scan_local_privacy` and before `initialize_scrub_index`, unless the eventual
command wrappers are defined beside a more specific hosted-session block.

Do not register or route through any generic delete-capable port as part of this
change. In particular, this scan registration must not add or require:

- `execute_mass_cleanup_batch`
- `preview_discord_guided_deletion`
- `execute_discord_guided_deletion`
- any command or verb named `DeleteOwnItem`

## Required ACL Entries

Add these exact permission identifiers to
`apps/osl-hub/capabilities/hub.json`:

```json
"allow-open-hosted-session-scan",
"allow-request-hosted-session-scan",
```

Add these exact TOML entries to `apps/osl-hub/permissions/hub.toml`:

```toml
[[permission]]
identifier = "allow-open-hosted-session-scan"
description = "Open the trusted hosted-session scan surface for the exact active native-hosted context; reads only bounded visible rows already available on this device and grants no preview, confirmation, deletion, navigation, credential, profile, process, path, or network authority."
commands.allow = ["open_hosted_session_scan"]

[[permission]]
identifier = "allow-request-hosted-session-scan"
description = "Request one read-only scan of the exact active native-hosted context using only native-held owner, scope, generation, and attended binding state; returns content-free row shape metadata and grants no preview, confirmation, deletion, navigation, credential, profile, process, path, or network authority."
commands.allow = ["request_hosted_session_scan"]
```

The descriptions are intentionally explicit that the ACL grants only scan
reachability. It does not grant a plan builder or executor.

## Command Contract That Must Exist Before Registration

The command wrappers must refuse unless all of these are true:

- An OSL identity is unlocked via `active_unlocked_osl_user_id`.
- The active hosted/native context is revalidated against `HubBrokerState` and
  the current `ActiveServiceHost` generation.
- The scope binding is derived by native/trusted code, not supplied by the
  renderer.
- The operator-name binding is derived from reviewed native trusted state. The
  renderer must not supply `operator_names`.
- The scan calls `native_discord_adapter::scan_own_messages_for_deletion`.
- The return type is `guided_deletion::DeletionScan`, or a narrower DTO with the
  same content-free fields.
- The command has no argument that can name an account, handle, credential,
  process id, window handle, profile path, service URL, conversation id,
  deletion verb, plan id, plan digest, or row selection.
- Any missing consent, binding, owner proof, active host, scope, generation, or
  operator-name source returns `Err(String)`.

Do not add `Debug` or `Display` derives to any new request or result wrapper
that can contain account identifiers, handles, paths, credentials, or raw
service labels. If an implementation needs `Debug` or `Display`, write it by
hand and emit only fixed refusal codes or opaque hashes.

## Safety Argument

Registering scan-only does not widen the delete path because the scan engine
only returns `DeletionScan`: scope hash, generation, row counts, row shape,
text length, ownership booleans, and walk completeness. It returns no message
text, no credential, no service account id, no process/window handle, no row
locator capable of acting on the platform, and no plan digest.

Deletion remains a separate workflow in current source:

- `guided_deletion::build_preview` constructs `DeletionPreview` from a scan and
  a selected row set.
- `guided_deletion::confirm_preview` requires an echoed digest and unchanged
  scope/generation.
- `native_discord_adapter::execute_guided_deletion` requires a
  `ConfirmedPlan`.
- The host-backed execution surface currently refuses the input rungs, so even
  that path is not a scan-only registration prerequisite.

Therefore adding only `open_hosted_session_scan` and
`request_hosted_session_scan` to `generate_handler!`, `hub.json`, and
`hub.toml` exposes a read-only observation result. It does not expose row
selection, preview, confirmation, platform input, or execution authority.

## Security-Review Checklist for Isolation Boundary

This checklist is deliberately written as a decision table: every row names the
trusted boundary, the authority source, and the fail-closed result when that
source is absent. It is meant to be parsed by the independent review test rather
than treated as prose.

| check_id | boundary | authority_source | renderer_supplied | deletion_authority | refusal_without_authority |
| --- | --- | --- | --- | --- | --- |
| hosted_identity_unlock | unlocked OSL owner identity | native_state | no | no | refuse |
| hosted_active_context | current service host generation | native_state | no | no | refuse |
| hosted_scope_binding | active hosted scope binding | native_state | no | no | refuse |
| hosted_operator_binding | attended operator-name binding | native_state | no | no | refuse |
| hosted_credential_handling | hosted credential and profile material | unavailable_to_command | no | no | refuse |
| hosted_delete_boundary | delete-own-item authority | not_present_in_scan_port | no | no | refuse |

Required review verdict: every hosted scan command remains read-only, every
binding comes from trusted native state, the renderer supplies no account,
handle, credential, profile, path, URL, conversation, plan, digest, row, or
deletion verb, and absence of any required binding refuses instead of issuing an
empty or permissive scan.

## Unit Tests To Add With The Registration

Add a static reachability test under the UI test suite, for example
`apps/osl-hub-ui/src/hosted-scan-registration.test.ts`:

```ts
import { readFileSync } from "node:fs";

const nativeMain = readFileSync(
  new URL("../../osl-hub/src/main.rs", import.meta.url),
  "utf8",
);
const permissions = readFileSync(
  new URL("../../osl-hub/permissions/hub.toml", import.meta.url),
  "utf8",
);
const capability = readFileSync(
  new URL("../../osl-hub/capabilities/hub.json", import.meta.url),
  "utf8",
);

function handlerBlock(source: string): string {
  const start = source.indexOf("tauri::generate_handler![");
  if (start < 0) throw new Error("generate_handler missing");
  const end = source.indexOf("\n    ]);", start);
  if (end < 0) throw new Error("generate_handler terminator missing");
  return source.slice(start, end);
}

describe("hosted-session scan registration", () => {
  it("registers exactly the scan-only commands in Tauri", () => {
    const handler = handlerBlock(nativeMain);
    expect(handler).toContain("open_hosted_session_scan,");
    expect(handler).toContain("request_hosted_session_scan,");
    expect(handler).not.toContain("execute_discord_guided_deletion");
    expect(handler).not.toContain("preview_discord_guided_deletion");
    expect(handler).not.toContain("DeleteOwnItem");
  });

  it("grants only scan ACL entries", () => {
    expect(capability).toContain('"allow-open-hosted-session-scan"');
    expect(capability).toContain('"allow-request-hosted-session-scan"');
    expect(permissions).toContain('commands.allow = ["open_hosted_session_scan"]');
    expect(permissions).toContain('commands.allow = ["request_hosted_session_scan"]');
    expect(permissions).not.toContain('commands.allow = ["execute_discord_guided_deletion"]');
    expect(permissions).not.toContain("DeleteOwnItem");
  });

  it("keeps the command wrappers renderer-argument-free", () => {
    for (const command of ["open_hosted_session_scan", "request_hosted_session_scan"]) {
      const signatureStart = nativeMain.indexOf(`fn ${command}(`);
      expect(signatureStart).toBeGreaterThan(-1);
      const bodyStart = nativeMain.indexOf("{", signatureStart);
      const signature = nativeMain.slice(signatureStart, bodyStart);
      expect(signature).not.toMatch(/\bString\b|Vec<|account|handle|credential|profile|path|url|conversation|plan|digest|row/i);
    }
  });
});
```

Add or extend a Rust unit test in the module that owns the command wrapper only
after the wrapper exists. The test must prove absence means refusal:

```rust
#[test]
fn hosted_session_scan_refuses_without_attended_operator_binding() {
    // Construct the smallest command-helper state with an unlocked owner and
    // active host but no reviewed operator-name binding.
    //
    // Expected result: Err(...), not an empty scan and not permission.
}
```

Do not use `cargo`, `osl-cargo`, `nextest`, or `cargo check` for this packet.
Compilation is centrally batched.
