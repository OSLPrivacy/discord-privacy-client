# T1-70 — Tor integration decision

**Status:** decided 2026-08-01
**Authority:** `09-DECISIONS.md` D8, D10, and D36 override earlier track notes.

## Decision

OSL will ship Tor routing by bundling and launching Arti's `arti` CLI as a
**child SOCKS5 proxy process**.  The application routes every Tor-selected
store request through `socks5h://127.0.0.1:9150`, using reqwest
`Proxy::all` (not `Proxy::http`, which would leave HTTPS outside the proxy).
The `h` is required: hostname resolution happens at the proxy, so the local
resolver does not receive the destination lookup.

`crates/transport` owns the process boundary and the Tor-aware client
configuration.  The later implementation task must fail closed: when the user
has selected Tor and the proxy is not ready, no request may fall back to the
direct client.

## Why this is the only viable v1 shape

Embedding `arti-client 0.44` is rejected.  It requires Rust 1.91 and edition
2024, while this workspace is pinned to Rust 1.88.0 and its manifests declare
edition 2021 / rust-version 1.88.  An embed would therefore turn a Tor feature
into a workspace-wide toolchain migration.  It also lets `arti-client` call
`exit(1)` in the host process when consensus declares the client obsolete.

The subprocess boundary avoids both hazards.  It is also Arti's documented
recommendation for applications that need to connect via a non-Rust/FFI-shaped
interface.  The bundled Arti executable is built and signed by OSL's release
pipeline; do not introduce a separately shipped `tor.exe`.

Do not use `arti-hyper`: upstream marks it obsolete and unmaintained.

## Explicit non-decisions and limits

- No Vanguards preference or client-facing toggle.  Tor's Vanguards
  specification says neither system applies to exit activity; OSL presently
  connects to an HTTPS origin, not an onion service.  D36 withdraws D9.
- Cloudflare Onion Routing is a separate deployment task (T1-74), not a
  prerequisite for this client routing decision.  When enabled on a custom
  domain, it removes the exit hop; `*.workers.dev` cannot provide it.
- Tor hides the user's IP from OSL, but does not hide the existence of the
  persistent connection.  The public honesty copy must retain this limit.
- Default-on versus onboarding opt-in remains the owner's OQ-8 decision.  This
  record only fixes the implementation shape required by D8.

## Evidence checked

- Workspace pin: `rust-toolchain.toml` is `1.88.0`; the root transport comment
  confirms Arti is not currently a workspace dependency.
- Arti 0.44 documentation: it recommends spawning the `arti` CLI SOCKS proxy
  for non-FFI integration and warns that `arti-client` can terminate its host
  with `exit(1)` on an obsolete consensus.
- Tor Vanguards specification: Vanguards-Lite is for onion activity and
  neither Vanguards system applies to exit activity.

Primary references: <https://docs.rs/arti-client/0.44.0/arti_client/>,
<https://spec.torproject.org/vanguards-spec/>, and
<https://developers.cloudflare.com/network/onion-routing/>.

## Consequences for dependent tasks

T1-05 may claim `crates/transport` for this boundary and replace its stale
dependency-conflict note.  T1-71 implements process lifecycle, bootstrap
readiness, crash/orphan cleanup, and the all-route proxy invariant.  T1-72
stores the onboarding choice and proves fail-closed behavior.  T1-73 records
the no-toggle finding; T1-74 performs Cloudflare configuration.
