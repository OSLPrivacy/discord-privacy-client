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
DISCOVER = ROOT / "discover-whatsapp-session.ps1"
ARM = ROOT / "arm-whatsapp-uia-probe.ps1"
POLL = ROOT / "poll-whatsapp-uia-probe.ps1"
MSAA = ROOT / "probe-whatsapp-msaa-metadata.ps1"


def run_command(vm: str, script: Path, parameters: list[str]) -> dict:
    command = [
        "az", "vm", "run-command", "invoke",
        "--resource-group", RESOURCE_GROUP,
        "--name", vm,
        "--command-id", "RunPowerShellScript",
        "--scripts", f"@{script}",
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


def _validate_discovery(discovery: dict) -> int:
    if discovery.get("Status") != "ready" or discovery.get("ProfileRead") is not False or discovery.get("ProviderStorageRead") is not False:
        raise RuntimeError("exact WhatsApp QA interactive session is unavailable")
    session_id = discovery.get("SessionId")
    if not isinstance(session_id, int) or not 1 <= session_id <= 65535:
        raise ValueError("invalid interactive session ID")
    return session_id


def run_uia_bench(vm: str, invocation: str, session_id: int, mode: str, timeout: int) -> tuple[int, dict]:
    armed = run_command(vm, ARM, [f"InvocationId={invocation}", f"SessionId={session_id}", f"Mode={mode}"])
    if armed.get("Status") not in {"armed", "alreadyArmed", "alreadyCompleted"}:
        raise RuntimeError("UIA probe did not arm safely")

    deadline = time.monotonic() + max(10, min(timeout, 180))
    while time.monotonic() < deadline:
        response = run_command(vm, POLL, [f"InvocationId={invocation}"])
        if response.get("Terminal") is True:
            result = response.get("Result")
            return (
                0 if isinstance(result, dict) and result.get("Status") == "capturedStructure" else 3,
                response,
            )
        time.sleep(5)
    response = {"InvocationId": invocation, "Terminal": False, "Status": "pollTimeout"}
    return 2, response


def run_msaa_bench(vm: str, session_id: int, max_nodes: int) -> tuple[int, dict]:
    response = run_command(vm, MSAA, [f"MaxNodes={max_nodes}", f"SessionId={session_id}"])
    safe = (
        response.get("Schema") == "whatsapp-msaa-metadata-probe/v1"
        and response.get("NamesReturned") is False
        and response.get("ValuesReturned") is False
        and response.get("ProviderContentReturned") is False
        and response.get("InputSent") is False
        and response.get("WindowForegrounded") is False
        and response.get("WhatsAppPrivateStorageRead") is False
        and isinstance(response.get("TotalNodes", 0), int)
        and 0 <= response.get("TotalNodes", 0) <= max_nodes
    )
    return (0 if safe and response.get("Status") != "timedOut" else 4), response


def main() -> int:
    parser = argparse.ArgumentParser(description="Run metadata-only WhatsApp UIA and MSAA structure probes")
    parser.add_argument("--vm", required=True, choices=sorted(ALLOWED_VMS))
    parser.add_argument("--invocation", required=True)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--bench", choices=("uia", "msaa", "both"), default="both")
    parser.add_argument("--msaa-max-nodes", type=int, default=2048)
    parser.add_argument("--graceful-relaunch", action="store_true")
    parser.add_argument("--verified-relaunch", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", args.invocation):
        raise ValueError("invalid invocation ID")
    if not 64 <= args.msaa_max_nodes <= 4096:
        raise ValueError("invalid MSAA node cap")

    discovery = run_command(args.vm, DISCOVER, [])
    session_id = _validate_discovery(discovery)
    if args.graceful_relaunch and args.verified_relaunch:
        raise ValueError("select only one relaunch mode")
    mode = "verifiedRelaunch" if args.verified_relaunch else ("gracefulRelaunch" if args.graceful_relaunch else "observe")
    receipt: dict[str, object] = {"InvocationId": args.invocation}
    status = 0
    if args.bench in {"uia", "both"}:
        uia_status, uia_receipt = run_uia_bench(args.vm, args.invocation, session_id, mode, args.timeout)
        receipt["Uia"] = uia_receipt
        status = max(status, uia_status)
    if args.bench in {"msaa", "both"}:
        msaa_status, msaa_receipt = run_msaa_bench(args.vm, session_id, args.msaa_max_nodes)
        receipt["Msaa"] = msaa_receipt
        status = max(status, msaa_status)
    print(json.dumps(receipt, sort_keys=True, separators=(",", ":")))
    return status


if __name__ == "__main__":
    sys.exit(main())
