#!/usr/bin/python3
"""Prove VMQA command metadata changes when a run-time switch changes."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


SCRIPT_ROOT = Path(__file__).resolve().parent
RUNNER = SCRIPT_ROOT / "vmqa-run.sh"
DIRECT_COMMAND = "f1_live_windows_walkthrough_imports_nonempty_receipt"
RECORD_NAME = f"two-copy-{DIRECT_COMMAND}.json"
SWITCH_NAME = "OSL_PASSWORD_SCREEN"
STAMP = "2026-08-06T00:06:01Z"


def run_direct(metadata_dir: Path, switch_value: str) -> Path:
    env = os.environ.copy()
    env["VMQA_TEST_METADATA_DIR"] = str(metadata_dir)
    env["VMQA_TEST_METADATA_RECORDED_AT_UTC"] = STAMP
    env[SWITCH_NAME] = switch_value
    completed = subprocess.run(
        [str(RUNNER), DIRECT_COMMAND],
        check=False,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode != 0:
        print(
            f"direct command failed switch={switch_value} rc={completed.returncode}",
            file=sys.stderr,
        )
        print(completed.stdout, file=sys.stderr, end="")
        print(completed.stderr, file=sys.stderr, end="")
        raise SystemExit(2)
    path = metadata_dir / RECORD_NAME
    if not path.is_file():
        print(f"missing metadata record: {path}", file=sys.stderr)
        raise SystemExit(2)
    return path


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def compare(on_record: dict, off_record: dict) -> tuple[bool, str]:
    on_switch = f"{SWITCH_NAME}=on"
    off_switch = f"{SWITCH_NAME}=off"
    on_without_switches = dict(on_record)
    off_without_switches = dict(off_record)
    on_switches = on_without_switches.pop("switches", None)
    off_switches = off_without_switches.pop("switches", None)
    if on_without_switches != off_without_switches:
        differing = sorted(
            key
            for key in set(on_without_switches) | set(off_without_switches)
            if on_without_switches.get(key) != off_without_switches.get(key)
        )
        return False, f"non-switch fields differ: {','.join(differing)}"
    if not isinstance(on_switches, list) or not isinstance(off_switches, list):
        return False, "switches field is not a list"
    if on_switch not in on_switches or off_switch not in off_switches:
        return False, f"missing intended switch values {on_switch}/{off_switch}"
    if sorted(s for s in on_switches if s != on_switch) != sorted(
        s for s in off_switches if s != off_switch
    ):
        return False, "switch lists differ outside OSL_PASSWORD_SCREEN"
    if sorted(on_switches) == sorted(off_switches):
        return False, "records have identical switches; expected OSL_PASSWORD_SCREEN to differ"
    return True, f"switch delta only: {on_switch} -> {off_switch}"


def main(argv: list[str]) -> int:
    copy_control = argv == ["--copy-control"]
    if argv and not copy_control:
        print("usage: test-vmqa-switch-metadata.py [--copy-control]", file=sys.stderr)
        return 2
    with tempfile.TemporaryDirectory(prefix="osl-vmqa-switch-metadata.") as tmp:
        root = Path(tmp)
        on_path = run_direct(root / "on", "on")
        off_path = run_direct(root / "off", "off")
        if copy_control:
            shutil.copyfile(on_path, off_path)
        ok, detail = compare(load(on_path), load(off_path))
        print(f"directCommand={DIRECT_COMMAND}")
        print(f"onRecord={on_path}")
        print(f"offRecord={off_path}")
        print(f"onSwitch={SWITCH_NAME}=on")
        print(f"offSwitch={SWITCH_NAME}=off")
        print(detail)
        if ok:
            print("PASS")
            return 0
        print("FAIL", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
