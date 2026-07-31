from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

RESOURCE_GROUP = "OSL-WHATSAPP-TWO-CLIENT-LAB"
ALLOWED_VMS = ("OSL-WhatsApp-Client-1", "OSL-WhatsApp-Client-2")
ARTIFACT_HOST = "osltestartifactsa7d5.blob.core.windows.net"
ROOT = Path(__file__).resolve().parent
DISCOVER = ROOT / "discover-whatsapp-session.ps1"
DEPLOY = ROOT / "deploy-whatsapp-preserve.ps1"
RUNTIME_AUDIT = ROOT / "audit-whatsapp-qa-runtime.ps1"
SUCCESS = {"alreadyInstalledPreserved", "installedPreservedAndLaunched"}
VM_LOCATIONS = {
    "OSL-WhatsApp-Client-1": "centralus",
    "OSL-WhatsApp-Client-2": "northcentralus",
}
COMMAND_NAMES = {
    DISCOVER: "whatsapp-session-discovery",
    DEPLOY: "whatsapp-build-deploy",
    RUNTIME_AUDIT: "whatsapp-runtime-audit",
}


class DeploymentError(RuntimeError):
    pass


def _validate_hash(value: str) -> str:
    if not re.fullmatch(r"[0-9a-fA-F]{64}", value):
        raise ValueError("artifact hash must be exactly 64 hexadecimal characters")
    return value.lower()


def _validate_uri(value: str, filename: str) -> str:
    parsed = urlsplit(value)
    if parsed.scheme != "https" or parsed.hostname != ARTIFACT_HOST or parsed.query or parsed.fragment:
        raise ValueError("artifact URI must be query-free HTTPS on the fixed managed-identity blob host")
    if not parsed.path.lower().endswith("/" + filename.lower()):
        raise ValueError("artifact URI does not match the fixed immutable build layout")
    return value


def _parse_receipt(stdout: str) -> dict[str, Any]:
    for line in reversed([line.strip() for line in stdout.splitlines() if line.strip()]):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise DeploymentError("Azure RunCommand returned no bounded JSON receipt")


def _run_command(vm: str, script: Path, parameters: dict[str, str]) -> dict[str, Any]:
    if vm not in ALLOWED_VMS:
        raise ValueError("VM is outside the exact dedicated WhatsApp pair")
    if script not in COMMAND_NAMES:
        raise ValueError("script is outside the fixed WhatsApp QA command allowlist")
    run_command_name = COMMAND_NAMES[script]
    common = [
        "--resource-group", RESOURCE_GROUP,
        "--vm-name", vm,
        "--run-command-name", run_command_name,
    ]
    listed = subprocess.run([
        "az", "vm", "run-command", "list",
        "--resource-group", RESOURCE_GROUP,
        "--vm-name", vm,
        "--query", f"[?name=='{run_command_name}'].name | [0]",
        "--output", "tsv", "--only-show-errors",
    ], check=True, text=True, capture_output=True)
    operation = "update" if listed.stdout.strip() == run_command_name else "create"
    command = [
        "az", "vm", "run-command", operation,
        *common,
        "--location", VM_LOCATIONS[vm],
        "--script", f"@{script}",
        "--async-execution", "false",
        "--timeout-in-seconds", "240",
    ]
    if parameters:
        command.extend(["--parameters", *[f"{key}={value}" for key, value in parameters.items()]])
    command.extend(["--output", "none", "--only-show-errors"])
    subprocess.run(command, check=True, text=True, capture_output=True)
    shown = subprocess.run([
        "az", "vm", "run-command", "show", *common,
        "--expand", "instanceView",
        "--query", "instanceView", "--output", "json", "--only-show-errors",
    ], check=True, text=True, capture_output=True)
    instance = json.loads(shown.stdout)
    if not isinstance(instance, dict) or instance.get("executionState") != "Succeeded" or instance.get("exitCode") != 0:
        raise DeploymentError(f"{vm}: persistent Azure RunCommand failed closed")
    output = instance.get("output")
    if not isinstance(output, str):
        raise DeploymentError(f"{vm}: persistent Azure RunCommand returned no bounded output")
    return _parse_receipt(output)


def _discover(vm: str) -> int:
    result = _run_command(vm, DISCOVER, {})
    if (
        result.get("Status") != "ready"
        or result.get("ProfileRead") is not False
        or result.get("ProviderStorageRead") is not False
        or result.get("ProcessesTerminated") is not False
    ):
        raise DeploymentError(f"{vm}: exact secret-safe osltest session discovery failed")
    session_id = result.get("SessionId")
    if not isinstance(session_id, int) or not 1 <= session_id <= 128:
        raise DeploymentError(f"{vm}: invalid interactive session receipt")
    return session_id


def _deploy_one(
    vm: str,
    invocation: str,
    artifacts: dict[str, str],
    expect_probe_dispatch: bool = False,
) -> dict[str, Any]:
    session_id = _discover(vm)
    suffix = vm[-1]
    result = _run_command(vm, DEPLOY, {
        "InvocationId": f"{invocation}-c{suffix}",
        "ExeUri": artifacts["exe_uri"],
        "ExeSha256": artifacts["exe_sha256"],
        "WebView2LoaderUri": artifacts["loader_uri"],
        "WebView2LoaderSha256": artifacts["loader_sha256"],
        "SessionId": str(session_id),
    })
    required_true = (
        "Terminal", "ExactOfficialWhatsAppPackageVerified", "WhatsAppProcessSetUnchanged",
        "WhatsAppWindowStateUnchanged", "ForegroundWindowUnchanged",
    )
    required_false = (
        "OslProfileTouched", "WhatsAppPrivateStorageRead", "WhatsAppProfileTouched",
        "WhatsAppProcessTerminated", "WhatsAppWindowForegrounded", "BrowserFallbackUsed",
    )
    if (
        result.get("Schema") != "whatsapp-deploy-preserve/v1"
        or result.get("Status") not in SUCCESS
        or result.get("ExeSha256") != artifacts["exe_sha256"]
        or result.get("WebView2LoaderSha256") != artifacts["loader_sha256"]
        or any(result.get(key) is not True for key in required_true)
        or any(result.get(key) is not False for key in required_false)
    ):
        raise DeploymentError(f"{vm}: deployment receipt failed semantic validation")
    audit = _run_command(vm, RUNTIME_AUDIT, {
        "ClientNumber": suffix,
        "InvocationId": f"{invocation}-audit-c{suffix}",
        "OslExeSha256": artifacts["exe_sha256"],
    })
    audit_required_false = (
        "BrowserFallbackUsed", "ProviderContentRead",
        "WhatsAppPrivateStorageRead", "ProfileRead", "WindowForegrounded",
    )
    allowed_phases = (
        {"protectedProbeDispatched", "protectedProbeDispatchFailed", "protectedProbePreparationFailed"}
        if expect_probe_dispatch
        else {"nativeWindowClaimed", "protectedControlsReady", "visualBindingFailed"}
    )
    expected_input = audit.get("Phase") in {"protectedProbeDispatched", "protectedProbeDispatchFailed"}
    if (
        audit.get("Schema") != "whatsapp-qa-runtime-audit/v1"
        or audit.get("Status") != "audited"
        or audit.get("ExeSha256") != artifacts["exe_sha256"]
        or audit.get("Phase") not in allowed_phases
        or audit.get("NativeWindowClaimed") is not True
        or not isinstance(audit.get("ProtectedControlsEnabled"), bool)
        or audit.get("InputSent") is not expected_input
        or any(audit.get(key) is not False for key in audit_required_false)
    ):
        raise DeploymentError(f"{vm}: runtime audit receipt failed semantic validation")
    return {
        "vmName": vm,
        "status": result["Status"],
        "sessionId": session_id,
        "oslProcessCount": result.get("OslProcessCount"),
        "whatsAppProcessCount": result.get("WhatsAppProcessCount"),
        "whatsAppWindowCount": result.get("WhatsAppWindowCount"),
        "runtimeAudit": {
            "phase": audit["Phase"],
            "nativeWindowClaimed": True,
            "protectedControlsEnabled": audit["ProtectedControlsEnabled"],
            "failureCode": audit.get("FailureCode"),
            "inputSent": audit["InputSent"],
        },
        "preservation": {
            "officialPackageVerified": True,
            "processSetUnchanged": True,
            "windowStateUnchanged": True,
            "privateStorageRead": False,
            "profileTouched": False,
            "processTerminated": False,
            "windowForegrounded": False,
            "browserFallbackUsed": False,
        },
    }


def _write_receipt(path: Path, receipt: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    data = json.dumps(receipt, sort_keys=True, separators=(",", ":")) + "\n"
    temporary.write_text(data, encoding="utf-8")
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser(description="Deploy one immutable OSL WhatsApp QA build to the exact dedicated pair")
    parser.add_argument("--invocation", required=True)
    parser.add_argument("--exe-uri", required=True)
    parser.add_argument("--exe-sha256", required=True)
    parser.add_argument("--webview2-loader-uri", required=True)
    parser.add_argument("--webview2-loader-sha256", required=True)
    parser.add_argument("--expect-probe-dispatch", action="store_true")
    parser.add_argument("--receipt-dir", type=Path, default=ROOT / "receipts")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,52}", args.invocation):
        raise ValueError("invalid invocation ID")
    artifacts = {
        "exe_uri": _validate_uri(args.exe_uri, "OSL%20Privacy.exe"),
        "exe_sha256": _validate_hash(args.exe_sha256),
        "loader_uri": _validate_uri(args.webview2_loader_uri, "WebView2Loader.dll"),
        "loader_sha256": _validate_hash(args.webview2_loader_sha256),
    }
    if args.exe_uri.rsplit("/", 1)[0] != args.webview2_loader_uri.rsplit("/", 1)[0]:
        raise ValueError("both artifacts must come from the same immutable build directory")

    started = dt.datetime.now(dt.timezone.utc)
    receipt_path = args.receipt_dir / f"{args.invocation}.json"
    receipt: dict[str, Any] = {
        "schema": "whatsapp-pair-deploy/v1",
        "invocationId": args.invocation,
        "resourceGroup": RESOURCE_GROUP,
        "allowedPair": list(ALLOWED_VMS),
        "artifactHashes": {
            "exeSha256": artifacts["exe_sha256"],
            "webView2LoaderSha256": artifacts["loader_sha256"],
        },
        "transport": "AzureRunCommandOnly",
        "artifactAuthorization": "systemAssignedManagedIdentityOnly",
        "startedAt": started.isoformat(),
        "status": "running",
        "terminal": False,
        "expectedProbeDispatch": args.expect_probe_dispatch,
    }
    _write_receipt(receipt_path, receipt)
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            futures = {
                executor.submit(
                    _deploy_one,
                    vm,
                    args.invocation,
                    artifacts,
                    args.expect_probe_dispatch,
                ): vm
                for vm in ALLOWED_VMS
            }
            machines = [future.result() for future in concurrent.futures.as_completed(futures)]
        receipt.update({
            "status": "deployedPreservedAndAudited",
            "terminal": True,
            "finishedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
            "machines": sorted(machines, key=lambda item: item["vmName"]),
        })
        _write_receipt(receipt_path, receipt)
        print(json.dumps({"status": receipt["status"], "receipt": str(receipt_path)}, separators=(",", ":")))
        return 0
    except Exception:
        receipt.update({
            "status": "failedClosed",
            "terminal": True,
            "finishedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
            "failureCode": "pair-deployment-failed",
        })
        _write_receipt(receipt_path, receipt)
        raise DeploymentError(f"pair deployment failed closed; details redacted; receipt: {receipt_path}") from None


if __name__ == "__main__":
    sys.exit(main())
