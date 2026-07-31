from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

RESOURCE_GROUP = "OSL-WHATSAPP-TWO-CLIENT-LAB"
ALLOWED_VMS = {"OSL-WhatsApp-Client-1", "OSL-WhatsApp-Client-2"}
ROOT = Path(__file__).resolve().parent


def run_command(vm: str, script: Path, parameters: list[str]) -> dict:
    command = [
        "az", "vm", "run-command", "invoke", "--resource-group", RESOURCE_GROUP,
        "--name", vm, "--command-id", "RunPowerShellScript", "--scripts", f"@{script}",
    ]
    if parameters:
        command.extend(["--parameters", *parameters])
    command.extend(["--query", "value[0].message", "--output", "tsv"])
    completed = subprocess.run(command, check=True, text=True, capture_output=True)
    for line in reversed([line.strip() for line in completed.stdout.splitlines() if line.strip()]):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise RuntimeError("Azure RunCommand returned no bounded JSON object")


def main() -> int:
    parser = argparse.ArgumentParser(description="Open exact official WhatsApp Desktop for user login")
    parser.add_argument("--vm", required=True, choices=sorted(ALLOWED_VMS))
    parser.add_argument("--invocation", required=True)
    parser.add_argument("--timeout", type=int, default=180)
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", args.invocation):
        raise ValueError("invalid invocation ID")
    discovery = run_command(args.vm, ROOT / "discover-whatsapp-session.ps1", [])
    if discovery.get("Status") != "ready" or discovery.get("ProfileRead") is not False:
        raise RuntimeError("exact WhatsApp QA interactive session is unavailable")
    session_id = discovery.get("SessionId")
    if not isinstance(session_id, int) or not 1 <= session_id <= 65535:
        raise RuntimeError("invalid WhatsApp QA session")
    armed = run_command(
        args.vm, ROOT / "arm-whatsapp-login.ps1",
        [f"InvocationId={args.invocation}", f"SessionId={session_id}"],
    )
    if armed.get("Status") not in {"armed", "alreadyArmed", "alreadyCompleted"}:
        raise RuntimeError("WhatsApp login task did not arm safely")
    deadline = time.monotonic() + max(15, min(args.timeout, 300))
    while time.monotonic() < deadline:
        result = run_command(
            args.vm, ROOT / "poll-whatsapp-login.ps1", [f"InvocationId={args.invocation}"],
        )
        if result.get("Terminal") is True:
            print(json.dumps(result, sort_keys=True, separators=(",", ":")))
            return 0 if result.get("Status") == "readyForUserLogin" and result.get("ExactWindowVerified") is True else 3
        time.sleep(3)
    print(json.dumps({"InvocationId": args.invocation, "Terminal": False, "Status": "pollTimeout"}, separators=(",", ":")))
    return 2


if __name__ == "__main__":
    sys.exit(main())
