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
ARM = ROOT / "arm-whatsapp-bootstrap.ps1"
POLL = ROOT / "poll-whatsapp-bootstrap.ps1"
DISCOVER = ROOT / "discover-whatsapp-session.ps1"


def run_command(vm: str, script: Path, parameters: list[str]) -> dict:
    command = [
        "az", "vm", "run-command", "invoke",
        "--resource-group", RESOURCE_GROUP,
        "--name", vm,
        "--command-id", "RunPowerShellScript",
        "--scripts", f"@{script}",
        "--parameters", *parameters,
        "--query", "value[0].message",
        "--output", "tsv",
    ]
    completed = subprocess.run(command, check=True, text=True, capture_output=True)
    lines = [line.strip() for line in completed.stdout.splitlines() if line.strip()]
    for line in reversed(lines):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise RuntimeError("Azure RunCommand returned no bounded JSON object")


def validate(vm: str, invocation: str) -> None:
    if vm not in ALLOWED_VMS:
        raise ValueError("VM is outside the dedicated WhatsApp pair")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", invocation):
        raise ValueError("invalid invocation ID")


def main() -> int:
    parser = argparse.ArgumentParser(description="Arm/poll the fixed WhatsApp Store bootstrap through Azure RunCommand")
    parser.add_argument("--vm", required=True, choices=sorted(ALLOWED_VMS))
    parser.add_argument("--invocation", required=True)
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    validate(args.vm, args.invocation)
    discovery = run_command(args.vm, DISCOVER, [])
    if discovery.get("Status") != "ready" or discovery.get("ProfileRead") is not False or discovery.get("ProviderStorageRead") is not False:
        raise RuntimeError("exact WhatsApp QA interactive session is unavailable")
    session_id = discovery.get("SessionId")
    if not isinstance(session_id, int) or not 1 <= session_id <= 65535:
        raise ValueError("invalid interactive session ID")
    armed = run_command(args.vm, ARM, [f"InvocationId={args.invocation}", f"SessionId={session_id}"])
    if armed.get("Status") not in {"armed", "alreadyArmed", "alreadyCompleted"}:
        raise RuntimeError("bootstrap did not arm safely")
    deadline = time.monotonic() + max(10, min(args.timeout, 600))
    while time.monotonic() < deadline:
        result = run_command(args.vm, POLL, [f"InvocationId={args.invocation}"])
        if result.get("Terminal") is True:
            print(json.dumps(result, sort_keys=True, separators=(",", ":")))
            receipt = result.get("Result")
            if (
                isinstance(receipt, dict)
                and receipt.get("Status") == "verified"
                and receipt.get("OfficialPackageVerified") is True
            ):
                return 0
            return 3
        time.sleep(5)
    print(json.dumps({"InvocationId": args.invocation, "Terminal": False, "Status": "pollTimeout"}, separators=(",", ":")))
    return 2


if __name__ == "__main__":
    sys.exit(main())
