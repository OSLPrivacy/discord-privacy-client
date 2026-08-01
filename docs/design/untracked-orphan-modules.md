# Notes/Creative orphan cluster — track assignment required

## Status

This is an **untracked feature candidate**, not a shipped OSL capability and not work assigned to T14. Owner decision D40 requires missed product areas to be raised for planning; it names Notes/Creative as a candidate because `osl_notes.rs` exists but is not compiled into the binary.

The source below is therefore preserved as an evidenced intake for a future track. Assigning a track must decide product scope, integration order, security review, test coverage, and truthful release claims before any of it is wired into the desktop application.

## Evidence collected 2026-07-31

All six Rust modules are present under `apps/osl-hub/src/`, but none is declared by `apps/osl-hub/src/lib.rs`. Consequently they are not compiled through the library's module tree. Their UI adapters also have no production command registration or renderer call path, as covered by the Notes and LAN reachability tests.

| Module | Lines | Candidate capability | Current integration status |
|---|---:|---|---|
| `osl_notes.rs` | 673 | encrypted notes, revisions, saved searches, and trash | untracked; no module declaration or desktop command path |
| `osl_assets.rs` | 720 | encrypted asset vault and project asset references | untracked; no module declaration or desktop command path |
| `osl_lan.rs` | 588 | local-network rooms for shared Notes documents | untracked; no module declaration or desktop command path |
| `osl_formats.rs` | 698 | bounded Office-format import helpers | untracked; no module declaration or desktop command path |
| `osl_collab.rs` | 241 | sealed collaboration frames and invitations | untracked; no module declaration or desktop command path |
| `osl_plugins.rs` | 247 | bounded Wasm command-pack runtime | untracked; no module declaration or desktop command path |

The cluster is 3,167 lines in the current worktree. The earlier T14 survey reported 2,167+ lines; the per-file count above is the verified current total and takes precedence.

## Existing safeguards and next owner

`apps/osl-hub-ui/src/osl-notes-reachability.test.ts` proves that Notes is not production-reachable. `apps/osl-hub-ui/src/osl-lan-reachability.test.ts` does the same for LAN collaboration and the plugin runtime. These checks prevent present-tense availability claims while the modules remain unwired; they are not evidence that the feature is ready to ship.

The future track owner should start with a dedicated Notes/Creative product decision and then supply an explicit module declaration, Tauri command boundary, production UI route, encrypted-state tests, input/fuzzing limits for imported formats, and release proof. No capability claim may be added until that integrated path exists.
