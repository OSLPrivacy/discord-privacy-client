# Group sender keys: shipping state

Verified against the working tree on 2026-07-31.  This is a record of what
the binaries can actually do, not a promise that the product has shipped a
group-chat surface.

The old premise, “sender keys exist but are switched off,” is false in both
directions.  Sender keys are enabled in the IPC core, while the shipping app
does not construct a group conversation that could select that path.

| Layer | What actually ships | Live evidence |
| --- | --- | --- |
| Sender-key construction | The complete sender-key implementation is present in the shared crypto crate (1,465 lines at verification). | `crates/crypto/src/sender_keys.rs:1-1465` |
| IPC owner switch | **Enabled by default.** The field documentation says it defaults enabled, and `AppState::new` initializes it to `true`. | `crates/ipc/src/state.rs:285-288`, `crates/ipc/src/state.rs:413` |
| Send routing | A `Gc`, `ServerChannel`, or `ServerFull` scope with at least one non-self peer selects `SenderKeysV5` when that enabled switch is read by the send path. | `crates/ipc/src/commands.rs:4587-4593`, `crates/ipc/src/commands.rs:5422-5433` |
| Sender-key distribution | SKDMs are bundled through the live v=3 path and are re-emitted at most every five minutes while a chain remains active. | `crates/ipc/src/commands.rs:5583`, `crates/ipc/src/commands.rs:5802-5829`, `crates/ipc/src/commands.rs:5901-5919` |
| SKDM receive | Both the v=3 control-message dispatch and the legacy v=4 dispatch call `apply_skdm_recv`. | `crates/ipc/src/commands.rs:7331-7335`, `crates/ipc/src/commands.rs:8188-8190`, `crates/ipc/src/commands.rs:8631-8638` |
| v=5 receive | v=5 decrypt is live and checks that the carrier-supplied Discord sender is bound to the wire sender identity key. | `crates/ipc/src/commands.rs:9189-9202`, `crates/ipc/src/commands.rs:9305-9323` |
| Shipping app (`apps/osl-hub`) | Production conversation construction is DM-only: its live construction points use `HubConversationKind::Dm`; the manual-peer route explicitly creates `ScopeKind::Dm`. Therefore this app cannot reach a group sender-key route. | `apps/osl-hub/src/broker.rs:300-305`, `apps/osl-hub/src/broker.rs:328-350`, `apps/osl-hub/src/broker.rs:1948-1966` |
| Live group caller | The legacy `src-tauri` shell emits `kind: "gc"`; it is excluded from the root workspace, unlike the shipping app. | `src-tauri/src/injection/boot.js:1974-1978`, `src-tauri/src/injection/boot.js:5322-5328`, `Cargo.toml:25-31` |
| Hub status field | The shipping app currently reports `group_sender_keys_enabled: false` as a stale literal. This is display state, not the IPC gate. | `apps/osl-hub/src/core_bridge.rs:269-276` |

## Corrected product premise

Sender keys are **on in the core and unreachable in the shipping app because
the app has no group conversations**.  Enabling group sender keys in the
product is consequently a conversation-surface build, not a flag flip.  The
legacy `src-tauri` shell is the only binary that currently supplies group
scopes, despite being excluded from the root workspace.

The public statement that group chats are currently no safer than direct
messages remains true because there are no product group chats, not because a
sender-key switch is off.

## Stale citations for follow-up work

These are documentation corrections for the owners of A3 and A4; this survey
does not edit either document.

| Document | Stale citation / claim | Verified replacement |
| --- | --- | --- |
| `docs/THREAT_MODEL.md` | Group row says the v=5 router is `commands.rs:3268-3282` and the switch defaults false at `state.rs:279-284`. | The router decision is `crates/ipc/src/commands.rs:5422-5433`; the switch documentation and true initializer are `crates/ipc/src/state.rs:285-288` and `:413`. The same stale positions recur in its post-compromise-security group row. |
| `docs/design/osl-public-claim-allowlist.md` | Group sender-key row says it is gated off at `commands.rs:2938` and defaults false at `state.rs:280`. | The row must instead describe sender keys as implemented and enabled in core but unreachable in the shipping app; see the routing and app evidence in the table above. |
