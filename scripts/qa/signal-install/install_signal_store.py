#!/usr/bin/env python3
"""Install and audit the fixed Microsoft Store Signal product on one Signal QA VM."""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import time
import uuid
from pathlib import Path
from typing import Any

PRODUCT_ID = "XP89119P9F2PCQ"
STORE_SOURCE = "msstore"
HERE = Path(__file__).resolve().parent
ARM = HERE / "arm-signal-store-install.ps1"
POLL = HERE / "poll-signal-store-install.ps1"
SAFE_NAME = re.compile(r"^[A-Za-z0-9._()-]{1,90}$")


class InstallError(RuntimeError):
    pass


def _validate_target(resource_group: str, vm_name: str, session_id: int, windows_user: str) -> None:
    if not SAFE_NAME.fullmatch(resource_group) or not SAFE_NAME.fullmatch(vm_name):
        raise InstallError("resource group and VM name must use bounded safe characters")
    combined = f"{resource_group}/{vm_name}".lower()
    if "signal" not in resource_group.lower() or "signal" not in vm_name.lower():
        raise InstallError("resource group and VM name must both identify the dedicated Signal lane")
    if any(name in combined for name in ("discord", "telegram", "whatsapp")):
        raise InstallError("another provider lane is forbidden")
    if not 1 <= session_id <= 128:
        raise InstallError("session ID must be from 1 through 128")
    if windows_user != "osltest":
        raise InstallError("Windows user must be exactly osltest")
    for leaf in (ARM, POLL):
        if not leaf.is_file():
            raise InstallError(f"missing leaf: {leaf.name}")


def _extract(stdout: str) -> dict[str, Any]:
    try:
        envelope = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise InstallError("Azure RunCommand returned invalid JSON") from exc
    messages = []
    if isinstance(envelope, dict):
        for value in envelope.get("value", []):
            if isinstance(value, dict) and isinstance(value.get("message"), str):
                messages.extend(line.strip() for line in value["message"].splitlines() if line.strip())
    for line in reversed(messages):
        try:
            parsed = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(parsed, dict) and "Status" in parsed and "InvocationId" in parsed:
            return parsed
    raise InstallError("Azure RunCommand did not return a semantic receipt")


def _invoke(resource_group: str, vm_name: str, leaf: Path, parameters: dict[str, Any]) -> dict[str, Any]:
    command = [
        "az", "vm", "run-command", "invoke", "--only-show-errors",
        "--resource-group", resource_group, "--name", vm_name,
        "--command-id", "RunPowerShellScript", "--scripts", f"@{leaf}",
        "--output", "json", "--parameters",
    ]
    command.extend(f"{key}={value}" for key, value in parameters.items())
    completed = subprocess.run(command, text=True, capture_output=True, timeout=300, check=False)
    if completed.returncode:
        raise InstallError("Azure RunCommand failed; details intentionally redacted")
    return _extract(completed.stdout)


def run(resource_group: str, vm_name: str, session_id: int, windows_user: str, receipt_dir: Path, poll_seconds: int = 5, timeout_seconds: int = 900, audit_only: bool = False) -> Path:
    _validate_target(resource_group, vm_name, session_id, windows_user)
    invocation_id = f"sigstore-{uuid.uuid4().hex[:20]}"
    mode = "audit" if audit_only else "install"
    armed = _invoke(resource_group, vm_name, ARM, {
        "InvocationId": invocation_id,
        "SessionId": session_id,
        "WindowsUser": windows_user,
        "Mode": mode,
    })
    if armed.get("Status") != "armed" or armed.get("InvocationId") != invocation_id or armed.get("ProductId") != PRODUCT_ID or armed.get("Source") != STORE_SOURCE or armed.get("Mode") != mode:
        raise InstallError("arm receipt failed identity validation")
    deadline = time.monotonic() + timeout_seconds
    terminal: dict[str, Any] | None = None
    while time.monotonic() < deadline:
        result = _invoke(resource_group, vm_name, POLL, {"InvocationId": invocation_id})
        if result.get("InvocationId") != invocation_id:
            raise InstallError("poll receipt identity mismatch")
        if result.get("Terminal") is True:
            terminal = result
            break
        if result.get("Status") not in ("pending", "runner-missing"):
            raise InstallError("unexpected nonterminal receipt")
        time.sleep(poll_seconds)
    if terminal is None:
        raise InstallError("bounded install/audit did not reach a terminal receipt")
    if terminal.get("Status") != "completed":
        code = terminal.get("Detail", {}).get("FailureCode", "unknown")
        raise InstallError(f"Signal Store install/audit failed: {code}")
    detail = terminal.get("Detail")
    if not isinstance(detail, dict) or set(detail) != {"Version", "Sha256", "Publisher", "PathClass", "ProcessState"}:
        raise InstallError("terminal audit fields are not exactly allowlisted")
    if not re.fullmatch(r"[0-9a-f]{64}", str(detail["Sha256"])) or detail["PathClass"] != "LocalAppDataProgramsSignalDesktop":
        raise InstallError("terminal executable evidence is invalid")
    if detail["ProcessState"] not in ("closed", "installer-auto-launched-exact-candidate", "running-exact-candidate"):
        raise InstallError("terminal process evidence is invalid")

    receipt = {
        "schemaVersion": 1,
        "status": "completed",
        "terminal": True,
        "invocationId": invocation_id,
        "resourceGroup": resource_group,
        "vmName": vm_name,
        "sessionId": session_id,
        "windowsUser": windows_user,
        "productId": PRODUCT_ID,
        "source": STORE_SOURCE,
        "mode": mode,
        "signalExecutable": detail,
    }
    receipt_dir.mkdir(parents=True, exist_ok=True)
    path = receipt_dir / f"{invocation_id}.json"
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)
    return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resource-group", required=True)
    parser.add_argument("--vm-name", required=True)
    parser.add_argument("--session-id", required=True, type=int)
    parser.add_argument("--windows-user", default="osltest")
    parser.add_argument("--receipt-dir", type=Path, required=True)
    parser.add_argument("--audit-only", action="store_true", help="audit an existing exact Store installation without reinstalling or closing Signal")
    args = parser.parse_args()
    try:
        path = run(args.resource_group, args.vm_name, args.session_id, args.windows_user, args.receipt_dir, audit_only=args.audit_only)
    except InstallError as exc:
        print(json.dumps({"status": "failed", "detail": str(exc)}))
        return 1
    print(json.dumps({"status": "completed", "receipt": str(path)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
