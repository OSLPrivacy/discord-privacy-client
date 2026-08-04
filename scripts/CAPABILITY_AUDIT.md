# Capability audit — SUPERSEDED by ledger 3 (D-159, 2026-08-04)

`scripts/audit_capabilities.py` was deleted and its CI step in
`.github/workflows/rust-test.yml` removed. **The rule it enforced is not
retracted** — it moved to the tree that ships. This file is kept as the record
of the discipline and of where each artifact is checked now.

## Why it was deleted rather than repointed

The script audited `src-tauri/**`. That is the **legacy shell**: excluded from the
cargo workspace (`Cargo.toml` `exclude`), built by no release or promote workflow,
and not the app the user installs. The shipping app is `apps/osl-hub`, with its
own `tauri.conf.json`, `permissions/hub.toml` and `capabilities/*.json`.

On every push CI printed:

```
OK: 126 commands audited, all 5 artifacts present, cross-window grants consistent.
```

while `apps/osl-hub` held three commands registered on its IPC surface with **no
grant anywhere** (D-158) and a permission block allowing a **deleted** command
(the D-146 class, which took the shipping app from compiling to not compiling
twice in one day). A capability auditor aimed at code that does not ship does not
merely fail to help; its green line is read as covering the app, and in D-158 it
was.

Repointing it at `apps/osl-hub` was the other option and was rejected: the two
trees do not share a shape (one TOML per command with a `windows` list, versus a
single 168-block `hub.toml` with `webviews`), so "repoint" means rewriting the
script into a **second** parser of the same ACL. D-137 was caused by exactly that
— two checkers disagreeing about the same set of commands. The ledger already
holds the parser, and now holds the checks.

## The five artifacts, and what enforces each one today

For every `#[tauri::command]` in `apps/osl-hub`:

| # | Artifact | Enforced by |
|---|----------|-------------|
| 1 | The `#[tauri::command]` fn exists | `scripts/ledger/acl-diff.mjs` — `registeredCommands()` intersects the handler registry with the names carrying the attribute; if either side extracts nothing, the ledger fails rather than passing vacuously |
| 2 | The name is listed in a `tauri::generate_handler![...]` / `hub_tauri_commands!` list | `acl-diff.mjs` — `defined-command-not-registered` |
| 3 | A `[[permission]]` block allows the command | `acl-diff.mjs` — `registered-but-granted-to-no-webview` (a command with no block and no grant is reported), and `granted-command-does-not-exist` in the other direction |
| 4 | Some capability JSON grants that permission | `acl-diff.mjs` — `registered-but-granted-to-no-webview`, evaluated against **every** capability file, not just the one covering `main` |
| 5 | Every name in the handler list resolves to a real command | `acl-diff.mjs` — `granted-command-does-not-exist`; and from the frontend side, `scripts/ledger/commands.mjs` — `frontend-invoke-not-registered` |

The old script's `ALLOWLIST_NO_PERMISSION` has no successor **by design**: the
ledger's ratchet (`scripts/ledger/exceptions/acl.json`) requires a reason and a
`file:line` citation per entry and its high-water mark may only move down, so a
name cannot be added quietly to make a check pass.

## The per-window cross-check, honestly stated

The script's TD2.1 cross-window validation relied on a hand-written
file → window map for `src-tauri`. `apps/osl-hub` has five webview labels and
decides at runtime which page loads under which; ledger 3 scopes its
issued-vs-granted direction to `main` and **declares the other four a blind
spot** in its header and in `exceptions/acl.json`. That blind spot predates this
change and is not widened by it: the direction added in D-159 is deliberately
webview-agnostic ("granted to no webview at all"), because scoring overlay
commands against `main` produces false positives for every command the overlay
legitimately holds and `main` must never have — measured: 14 on this tree.

## Open item for the conductor

**The ledgers are not wired into CI.** They are run by the plan's ODC harness
(`node scripts/ledger/all.mjs`, or one ledger at a time). Deleting the Python
step therefore does not reduce CI's coverage of the shipping app — that coverage
was zero either way — but it does mean no CI job checks the ACL of the app that
ships. Wiring the ledgers into CI is a larger decision than this lane owns
(they need the frontend bundle for reachability), and it is recorded here rather
than left implied.
