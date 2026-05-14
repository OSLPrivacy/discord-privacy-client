# Capability audit (`scripts/audit_capabilities.py`)

Tauri 2 will silently refuse to invoke any `#[tauri::command]` that
is missing its `allow-*` permission grant. Pre-9-C1 we lost several
debug days to that failure mode: the command exists, the JS calls
it, the build is green, but every `oslInvoke` returns
`{ok: false, error: "Command not found"}`. The five-artifact
discipline (the rule the audit enforces) avoids that whole class
of bug.

## What the audit checks

For each `#[tauri::command]` declared in `src-tauri/src/main.rs`:

1. The function definition exists (the `#[tauri::command]` annotation
   precedes a `fn` or `async fn`).
2. The function name is listed inside `tauri::generate_handler![ ... ]`.
3. A permission TOML file exists at
   `src-tauri/permissions/<kebab>.toml` (so `osl_tour_get_state` →
   `osl-tour-get-state.toml`) and its `commands.allow` array names
   the function.
4. The permission id (`allow-<kebab>`) is listed in **at least one**
   capability JSON under `src-tauri/capabilities/`.
5. Cross-check: every name inside `generate_handler![ ... ]` resolves
   to a real `#[tauri::command]` fn. Catches typo'd entries that
   would `panic!` at build time.

Internal-only commands (Rust-side helpers never wired to a webview)
go in `ALLOWLIST_NO_PERMISSION` inside the script with a one-line
justification. The audit warns on them but treats them as passing.

## How to run

```bash
python3 scripts/audit_capabilities.py
```

Exit code `0` = clean, `1` = at least one missing artifact, `2` = the
script couldn't find any `#[tauri::command]` at all (probably wrong
path / corrupted repo).

CI runs this on every push. See `.github/workflows/ci.yml`.

## Opting into a local pre-commit hook

Pre-commit is opt-in because some workflows prefer fast unchecked
commits + heavier checks in CI. To enable, drop this into
`.git/hooks/pre-commit` (or wire it via your `husky` / `pre-commit`
framework of choice):

```bash
#!/usr/bin/env bash
set -e
python3 scripts/audit_capabilities.py
```

Make it executable: `chmod +x .git/hooks/pre-commit`.

## When the audit fails on a new command

Either:

1. The command genuinely needs the 5 artifacts. Standard pattern,
   modelled on any recent `osl_*` command in `commands.rs` +
   `main.rs` + `src-tauri/permissions/*.toml` +
   `src-tauri/capabilities/{main,settings-window}.json`. The audit's
   "missing artifacts" list tells you which artifact is absent.

2. The command is internal and shouldn't be reachable from JS. Add
   it to `ALLOWLIST_NO_PERMISSION` with a one-line justification.
   If a future developer wires it to a webview, the JS-string-literal
   check (run manually before adding) prevents accidental skipped
   audits, and removing the allowlist entry triggers the standard
   pattern requirement.
