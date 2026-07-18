#!/usr/bin/env python3
"""TD1.2: Tauri 2 capability artifact audit.

For every `#[tauri::command]` in `src-tauri/src/main.rs`, verify the
five artifacts required for the command to be reachable from a
webview:

  1. Rust fn definition (the `#[tauri::command]` itself).
  2. Listed in the `tauri::generate_handler![ ... ]` macro.
  3. Permission TOML at `src-tauri/permissions/<kebab>.toml`
     that allows the command.
  4. Granted in at least one capability JSON
     (`src-tauri/capabilities/*.json`).
  5. Internal-only commands MAY skip permission/capability if they
     are deliberately not exposed to webviews — the script flags
     them so the developer can confirm.

Exit 0 if every command has all four "must-have" artifacts. Exit 1
otherwise. Designed to run in CI and locally as a pre-commit step.

Kebab-case mapping for permission TOMLs: each `_` in the Rust fn
name becomes `-` in the TOML filename. So `cmd osl_tour_get_state`
expects `osl-tour-get-state.toml` (the `cmd_` prefix is conventionally
stripped from Tauri wrappers in this project — the wrappers are
named `osl_*`, matching the TOML).
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
MAIN_RS = REPO_ROOT / "src-tauri" / "src" / "main.rs"
PERMISSIONS_DIR = REPO_ROOT / "src-tauri" / "permissions"
CAPABILITIES_DIR = REPO_ROOT / "src-tauri" / "capabilities"

# TD2.1 cross-window validation: map every JS/HTML file that calls
# Tauri commands to the window it actually runs in. A capability JSON
# only grants permissions to its `windows: [...]` list, so a command
# invoked from window X needs a capability JSON whose `windows`
# includes X. The bug class this guards against: command lives in
# main.rs + has a permission TOML + is granted in *some* capability
# JSON (so the simple 5-artifact audit passes), but that JSON targets
# the wrong window and the runtime returns "not allowed by ACL".
#
# boot.js is injected into the main Discord window (label "main").
# password_gate.html is loaded in the main window before the gate
# resolves. settings_window.html runs in the "settings" window.
JS_INVOKER_CAPABILITIES: dict[str, str] = {
    "src-tauri/src/injection/boot.js": "main-capability",
    "src-tauri/assets/password_gate.html": "password-gate-capability",
    "src-tauri/assets/account_burn.html": "account-burn-capability",
    "src-tauri/assets/settings_window.html": "settings-window-capability",
}

# Commands that must never be callable by remote Discord content. These
# mutate device credentials or irreversibly destroy local state and must
# remain on a bundled `osl-gate://` page.
REMOTE_MAIN_FORBIDDEN_PERMISSIONS = {
    "allow-osl-burn-engage",
    "allow-osl-set-main-password",
}

# Commands that intentionally exist without an `allow-*` permission
# because they're internal / core-only / sandbox-only. Add command
# names here (no kebab transform) with a one-line justification.
#
# Most entries here are pre-Tauri-2 v1-era commands that never got
# JS callers but were never removed. They are reachable in theory
# only from a webview that explicitly invokes them, which neither
# `boot.js` nor any settings page does (zero string-literal refs
# checked at allowlist time). If you wire one up to a webview, you
# MUST move it out of this list and add the standard 5 artifacts.
ALLOWLIST_NO_PERMISSION: dict[str, str] = {
    "aead_seal": "v1 dev primitive; no JS callers",
    "aead_open": "v1 dev primitive; no JS callers",
    "fetch_pubkeys": "v1 keyserver helper; superseded by bootstrap auto-fetch",
    "generate_identity": "bootstrap-only; never invoked from JS",
    "load_identity": "bootstrap-only; never invoked from JS",
    "save_identity": "bootstrap-only; never invoked from JS",
    "init_keyserver": "bootstrap-only; never invoked from JS",
    "register": "bootstrap-only; never invoked from JS",
    "status": "v1 dev probe; no JS callers",
    "stego_encode": "v1 dev primitive; no JS callers",
    "stego_decode": "v1 dev primitive; no JS callers",
    "x25519_diffie_hellman": "v1 dev primitive; no JS callers",
    "set_screenshot_protection": "main.rs setup-only path; not exposed to webviews",
}


def extract_tauri_commands() -> list[tuple[str, int]]:
    """Return [(fn_name, line_no)] for every `#[tauri::command]` fn
    in main.rs. Catches both `async fn` and `fn`."""
    text = MAIN_RS.read_text()
    out: list[tuple[str, int]] = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if "#[tauri::command]" not in line:
            continue
        # Walk forward to the fn declaration (allow blank lines /
        # attribute stacks in between).
        for j in range(i + 1, min(i + 8, len(lines))):
            m = re.match(r"\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)", lines[j])
            if m:
                out.append((m.group(1), j + 1))
                break
    return out


def extract_generate_handler_entries() -> set[str]:
    """Return the set of fn names listed inside
    `tauri::generate_handler![ ... ]`."""
    text = MAIN_RS.read_text()
    # Find the macro invocation. Multi-line; bounded by closing `]`.
    m = re.search(r"tauri::generate_handler!\s*\[(.*?)\]", text, re.DOTALL)
    if not m:
        return set()
    body = m.group(1)
    # Strip line comments — the macro body has historical commentary
    # like `// 9-C1: invitation/... all retired.` that the bare split
    # would misread as identifiers.
    body = re.sub(r"//[^\n]*", "", body)
    entries: set[str] = set()
    for tok in re.split(r"[,\s]+", body):
        tok = tok.strip()
        if not tok:
            continue
        # Strip module paths (e.g. `crate::commands::fn` -> `fn`).
        bare = tok.rsplit("::", 1)[-1]
        # Only accept valid Rust identifiers; anything else is
        # comment shrapnel.
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", bare):
            entries.add(bare)
    return entries


def expected_permission_toml(fn_name: str) -> Path:
    """Map a Rust fn name to its permission TOML path.

    Project convention: `osl_foo_bar` → `osl-foo-bar.toml`."""
    return PERMISSIONS_DIR / f"{fn_name.replace('_', '-')}.toml"


def permission_toml_grants_command(toml_path: Path, fn_name: str) -> bool:
    """Return True iff the permission TOML at `toml_path` lists
    `fn_name` in any `commands.allow` array."""
    if not toml_path.is_file():
        return False
    text = toml_path.read_text()
    # The TOMLs in this repo use a simple shape; rather than pulling
    # a TOML library, check that `commands.allow` lists the bare fn
    # name (matches the pattern used by every existing osl-*.toml).
    return bool(re.search(rf'commands\.allow\s*=\s*\[[^\]]*"{re.escape(fn_name)}"', text))


def capability_jsons_grant_permission(permission_id: str) -> list[str]:
    """Return the basenames of capability JSONs that list the
    permission id (e.g. `allow-osl-tour-get-state`)."""
    granters: list[str] = []
    for cap in CAPABILITIES_DIR.glob("*.json"):
        try:
            data = json.loads(cap.read_text())
        except json.JSONDecodeError:
            continue
        perms = data.get("permissions") or []
        if permission_id in perms:
            granters.append(cap.name)
    return granters


def windows_granting_permission(permission_id: str) -> set[str]:
    """Return the union of `windows` across every capability JSON
    that grants `permission_id`. e.g. permission granted in
    main.json (windows=["main"]) and settings-window.json
    (windows=["settings"]) returns {"main", "settings"}."""
    windows: set[str] = set()
    for cap in CAPABILITIES_DIR.glob("*.json"):
        try:
            data = json.loads(cap.read_text())
        except json.JSONDecodeError:
            continue
        if permission_id in (data.get("permissions") or []):
            for w in data.get("windows") or []:
                windows.add(w)
    return windows


_INVOKE_PATTERNS = (
    re.compile(r'oslInvoke\(\s*"(osl_\w+)"'),
    re.compile(r'invoke\(\s*"(osl_\w+)"'),
)


def extract_invoke_calls(path: Path) -> set[str]:
    """Return the set of `osl_*` command names invoked from a JS/HTML
    file. Matches both `oslInvoke("name", ...)` (boot.js's wrapper) and
    raw `invoke("name", ...)` (password_gate.html, settings_window.html
    inline scripts)."""
    if not path.is_file():
        return set()
    text = path.read_text()
    cmds: set[str] = set()
    for pat in _INVOKE_PATTERNS:
        cmds.update(pat.findall(text))
    return cmds


def cross_window_check(commands: list[tuple[str, int]]) -> list[str]:
    """Validate callers against their exact origin capability.

    Window-label unions are insufficient because the main window hosts
    both remote Discord and bundled local gate pages. A local grant must
    never satisfy a command invoked by remote boot.js.
    """
    capabilities: dict[str, set[str]] = {}
    for path in CAPABILITIES_DIR.glob("*.json"):
        try:
            data = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        identifier = data.get("identifier")
        if isinstance(identifier, str):
            capabilities[identifier] = set(data.get("permissions") or [])

    declared = {name for name, _ in commands}
    errors: list[str] = []

    for rel, capability_id in JS_INVOKER_CAPABILITIES.items():
        granted = capabilities.get(capability_id)
        if granted is None:
            errors.append(f"{rel}: missing capability {capability_id}")
            continue
        for cmd in sorted(extract_invoke_calls(REPO_ROOT / rel)):
        # Skip invokes of names that aren't actually declared as
        # tauri::command in main.rs — those are caught by the
        # generate_handler cross-check elsewhere or are typos. Surface
        # as a separate error so the audit fails loudly either way.
            if cmd not in declared:
                errors.append(
                    f"{cmd}: invoked from {rel} but no #[tauri::command] fn in main.rs"
                )
                continue
            permission_id = f"allow-{cmd.replace('_', '-')}"
            if permission_id not in granted:
                errors.append(
                    f"{cmd}: invoked from {rel}, but {capability_id} does not grant "
                    f"{permission_id}"
                )

    return errors


def audit() -> int:
    commands = extract_tauri_commands()
    handler_entries = extract_generate_handler_entries()

    if not commands:
        print("error: no #[tauri::command] declarations found in main.rs", file=sys.stderr)
        return 2

    missing: dict[str, list[str]] = {}
    warnings: list[str] = []

    for fn_name, line in commands:
        issues: list[str] = []

        # Artifact 2: listed in generate_handler!
        if fn_name not in handler_entries:
            issues.append("not registered in tauri::generate_handler![...]")

        # Artifact 3: permission TOML.
        toml_path = expected_permission_toml(fn_name)
        permission_id = f"allow-{fn_name.replace('_', '-')}"
        toml_ok = toml_path.is_file() and permission_toml_grants_command(toml_path, fn_name)

        if not toml_ok:
            if fn_name in ALLOWLIST_NO_PERMISSION:
                warnings.append(
                    f"  {fn_name}: skipped permission check (allowlist: "
                    f"{ALLOWLIST_NO_PERMISSION[fn_name]})"
                )
            else:
                if not toml_path.is_file():
                    issues.append(f"missing permission TOML: {toml_path.relative_to(REPO_ROOT)}")
                else:
                    issues.append(
                        f"permission TOML exists but doesn't list "
                        f"{fn_name} in commands.allow: "
                        f"{toml_path.relative_to(REPO_ROOT)}"
                    )

        # Artifact 5: granted in a capability JSON (only checked when
        # a permission TOML exists or is expected).
        if toml_ok:
            granters = capability_jsons_grant_permission(permission_id)
            if not granters:
                issues.append(
                    f"permission {permission_id} not granted in any capability JSON "
                    f"({CAPABILITIES_DIR.relative_to(REPO_ROOT)})"
                )

        if issues:
            missing[fn_name] = issues

    # Cross-check: every entry in generate_handler! resolves to a
    # real #[tauri::command]. Catches dead handler entries that
    # would break the build silently.
    declared = {name for name, _ in commands}
    orphaned = handler_entries - declared
    for name in sorted(orphaned):
        missing.setdefault(name, []).append(
            "listed in generate_handler! but no #[tauri::command] fn found"
        )

    # TD2.1: cross-window capability validation.
    xwindow_errors = cross_window_check(commands)

    for capability_path in CAPABILITIES_DIR.glob("*.json"):
        try:
            capability = json.loads(capability_path.read_text())
        except (OSError, json.JSONDecodeError) as exc:
            xwindow_errors.append(f"cannot read {capability_path.name}: {exc}")
            continue
        remote_urls = (capability.get("remote") or {}).get("urls") or []
        if not any("discord.com" in str(url) for url in remote_urls):
            continue
        if capability.get("local", True) is not False:
            xwindow_errors.append(
                f"{capability_path.name}: Discord remote capability must set local=false"
            )
        permissions = set(capability.get("permissions") or [])
        for permission in sorted(permissions & REMOTE_MAIN_FORBIDDEN_PERMISSIONS):
            xwindow_errors.append(
                f"{capability_path.name}: Discord remote capability grants forbidden {permission}"
            )

    if warnings:
        print("=== Warnings ===")
        for w in warnings:
            print(w)
        print()

    if xwindow_errors:
        print("=== Cross-window capability gaps (TD2.1) ===\n")
        for e in xwindow_errors:
            print(f"  - {e}")
        print()

    if not missing and not xwindow_errors:
        total = len(commands)
        print(
            f"OK: {total} commands audited, all 5 artifacts present, "
            f"cross-window grants consistent."
        )
        return 0

    if missing:
        print("=== Missing capability artifacts ===\n")
        for fn_name in sorted(missing):
            print(f"{fn_name}:")
            for issue in missing[fn_name]:
                print(f"  - {issue}")
            print()

    total_missing = len(missing)
    total_xwindow = len(xwindow_errors)
    print(
        f"FAIL: {total_missing} command(s) with missing artifacts, "
        f"{total_xwindow} cross-window gap(s) (of {len(commands)} total commands)."
    )
    print(
        "\nIf a command is deliberately internal-only and should NOT have a\n"
        "permission TOML / capability grant, add it to ALLOWLIST_NO_PERMISSION\n"
        "in this script with a one-line justification."
    )
    print(
        "Cross-window gaps mean the command is invoked from a window whose\n"
        "capability JSON does not grant the matching `allow-*` permission.\n"
        "Fix by adding the permission to the capability JSON whose `windows`\n"
        "list includes the calling window."
    )
    return 1


if __name__ == "__main__":
    sys.exit(audit())
