#!/usr/bin/env python3
"""TASK 6818 — prove the departure check can actually fail.

Each mutation below starves exactly one thing the finish line names, runs the
shipped check unchanged, and records the exit code and the first assertion that
fired. Every mutation is applied to a tracked file and reverted with
`git checkout --` before the next one, so nothing is left behind.

Usage:
    python3 scripts/task-6818/starve-check.py [name ...]

With no arguments it runs the green baseline and then every mutation.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECK = [
    "cargo",
    "test",
    "-p",
    "place-departure",
    "--test",
    "task_6818_place_departure",
    "--",
    "--test-threads=1",
]

SCENARIO = "crates/place-departure/scenario/task-6818-places.json"
WORLD = "crates/place-departure/src/world.rs"
RUNNER = "crates/place-departure/src/run.rs"
AUTHORITY = "crates/place-departure/src/authority.rs"
MENU_TS = "apps/osl-hub-ui/src/place-departure-6818.ts"


def sub(path: str, old: str, new: str) -> None:
    """Exact, unique text replacement. A miss is a hard error, not a no-op."""
    target = ROOT / path
    text = target.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"mutation anchor matched {count} times in {path}:\n{old}")
    target.write_text(text.replace(old, new))


def drop_place(handle: str, step: str) -> None:
    """Starve one real place out of the scenario, and the step that leaves it."""
    target = ROOT / SCENARIO
    data = json.loads(target.read_text())
    data["places"] = [p for p in data["places"] if p["handle"] != handle]
    data["steps"] = [s for s in data["steps"] if s.get("step") != step]
    target.write_text(json.dumps(data, indent=2) + "\n")


# --------------------------------------------------------------------------
# The mutations, one per thing the finish line says may not be starved.
# --------------------------------------------------------------------------

def starve_place_group() -> None:
    drop_place("atlas-group", "leave-atlas")


def starve_place_enclave() -> None:
    drop_place("harbor-enclave", "leave-harbor")


def starve_menu() -> None:
    sub(
        MENU_TS,
        ')}">Mark as read</button><button ${leaveAttributes}>${escapeHtml(\n    label,\n  )}</button>${deleteCopy}</div>',
        ')}">Mark as read</button>${deleteCopy}</div>',
    )


def starve_history() -> None:
    # Leaving silently deletes the member's own copy, with no separate choice.
    sub(
        WORLD,
        "            if activation.delete_local_history {\n"
        "                client.history.retain(|record| record.place != place_handle);\n"
        "            }\n",
        "            client.history.retain(|record| record.place != place_handle);\n",
    )


def starve_roster() -> None:
    # Only the leaver's own client applies the signed self-removal.
    sub(
        WORLD,
        "        for client in self.clients.values_mut() {\n"
        "            if let Some(live) = client.places.get_mut(&place_handle) {\n"
        "                live.membership.apply(&wire)?;\n"
        "            }\n"
        "        }\n",
        "        for client in self.clients.values_mut() {\n"
        "            if client.handle != leaver_handle {\n"
        "                continue;\n"
        "            }\n"
        "            if let Some(live) = client.places.get_mut(&place_handle) {\n"
        "                live.membership.apply(&wire)?;\n"
        "            }\n"
        "        }\n",
    )


def starve_restart() -> None:
    # The "restart" hands back the live in-memory clients instead of rebuilding
    # them from their sealed files.
    sub(
        WORLD,
        "        let mut rebuilt = BTreeMap::new();\n"
        "        for (handle, client) in std::mem::take(&mut self.clients) {\n"
        "            let path = client.path.clone();\n"
        "            let key = client.key.clone();\n"
        "            drop(client);\n"
        "            rebuilt.insert(handle, Client::restart(path, key)?);\n"
        "        }\n"
        "        self.clients = rebuilt;\n"
        "        Ok(self.roster_views())\n",
        "        Ok(self.roster_views())\n",
    )


def starve_new_content() -> None:
    sub(
        RUNNER,
        "    for (place, kind, channels) in place_order {",
        "    for (place, kind, channels) in place_order.into_iter().take(0) {",
    )


def starve_rekey() -> None:
    # Groups do not rotate, and no enclave channel is ever re-keyed.
    sub(
        WORLD,
        "rotations.push(self.distribute_from(&activation.place, &sender, true)?);",
        "rotations.push(self.distribute_from(&activation.place, &sender, false)?);",
    )
    sub(
        WORLD,
        "                            channel.includes(&roster_before, leaver_id),",
        "                            false && channel.includes(&roster_before, leaver_id),",
    )


def starve_ownership() -> None:
    # The gate stops looking at who holds the required authority.
    sub(
        AUTHORITY,
        "    if held_role_ids.is_empty() {\n"
        "        return DepartureAuthority::Clear;\n"
        "    }\n",
        "    let _ = &held_role_ids;\n"
        "    return DepartureAuthority::Clear;\n"
        "    #[allow(unreachable_code)]\n",
    )


def local_list_only_leave() -> None:
    # Leaving is reduced to dropping the row from this device's own list: no
    # client applies the self-removal, and no signed event is reported.
    sub(
        WORLD,
        "        for client in self.clients.values_mut() {\n"
        "            if let Some(live) = client.places.get_mut(&place_handle) {\n"
        "                live.membership.apply(&wire)?;\n"
        "            }\n"
        "        }\n",
        "        // starved: nothing but this device's own list is touched\n",
    )
    sub(WORLD, "            signed_leave_event: Some(wire),", "            signed_leave_event: None,")


MUTATIONS = [
    ("starve-place-group", starve_place_group, [SCENARIO]),
    ("starve-place-enclave", starve_place_enclave, [SCENARIO]),
    ("starve-menu", starve_menu, [MENU_TS]),
    ("starve-history-state", starve_history, [WORLD]),
    ("starve-roster", starve_roster, [WORLD]),
    ("starve-restart", starve_restart, [WORLD]),
    ("starve-new-content", starve_new_content, [RUNNER]),
    ("starve-rekey", starve_rekey, [WORLD]),
    ("starve-ownership-boundary", starve_ownership, [AUTHORITY]),
    ("local-list-only-leave", local_list_only_leave, [WORLD]),
]


def run_check() -> tuple[int, str]:
    proc = subprocess.run(
        CHECK, cwd=ROOT, capture_output=True, text=True, env={**os.environ}
    )
    return proc.returncode, proc.stdout + proc.stderr


def first_failure(output: str) -> str:
    lines = []
    for line in output.splitlines():
        stripped = line.strip()
        if stripped.startswith("TASK6818 ") or stripped.startswith("assertion"):
            lines.append(stripped)
        if stripped.startswith("left:") or stripped.startswith("right:"):
            lines.append(stripped)
        if len(lines) >= 4:
            break
    if not lines:
        for line in output.splitlines():
            if "panicked at" in line or line.startswith("error"):
                lines.append(line.strip())
                break
    return "\n".join(lines) if lines else "(no assertion text captured)"


def summary(output: str) -> str:
    for line in output.splitlines():
        if line.startswith("test result:"):
            return line.strip()
    return "(no test result line)"


def revert(paths: list[str]) -> None:
    subprocess.run(["git", "checkout", "--", *paths], cwd=ROOT, check=True)


def main() -> int:
    wanted = set(sys.argv[1:])
    dirty = subprocess.run(
        ["git", "status", "--porcelain", "--", SCENARIO, WORLD, RUNNER, AUTHORITY, MENU_TS],
        cwd=ROOT,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if dirty:
        print("refusing to mutate: those files already differ from HEAD\n" + dirty)
        return 2

    failures = 0
    if not wanted or "baseline" in wanted:
        code, output = run_check()
        print(f"=== baseline (no mutation) -> exit {code} ===")
        print(summary(output))
        if code != 0:
            print(first_failure(output))
            failures += 1

    for name, apply, paths in MUTATIONS:
        if wanted and name not in wanted:
            continue
        apply()
        try:
            code, output = run_check()
        finally:
            revert(paths)
        print(f"\n=== {name} -> exit {code} ===")
        print(summary(output))
        print(first_failure(output))
        if code == 0:
            print(f"!!! {name} did NOT make the check fail")
            failures += 1

    # Restored: the check has to be green again.
    code, output = run_check()
    print(f"\n=== restored -> exit {code} ===")
    print(summary(output))
    if code != 0:
        print(first_failure(output))
        failures += 1
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
