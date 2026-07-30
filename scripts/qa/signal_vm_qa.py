#!/usr/bin/env python3
"""Fail-closed controller for the dedicated two-VM Signal Desktop QA lane."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from pathlib import Path
from typing import Any
from unittest import mock
from urllib.parse import urlsplit


HERE = Path(__file__).resolve().parent
ALIASES = ("signal-qa-1", "signal-qa-2")
LEAVES = {
    "deploy": "osl-vm-signal-deploy-preserve.ps1",
    "audit": "osl-vm-signal-exact-build-audit.ps1",
    "arm": "osl-vm-signal-uia-arm.ps1",
    "poll": "osl-vm-signal-uia-poll.ps1",
}
CAPABILITIES = {
    "deploy": (
        "signal-deploy-preserve/v1",
        (
            "InvocationId",
            "ExeUri",
            "ExeSha256",
            "WebView2LoaderUri",
            "WebView2LoaderSha256",
            "SessionId",
            "WindowsUser",
            "ProfileRoot",
            "OslExePath",
            "SignalExePath",
            "SignalExeSha256",
            "SignalPublisherSubject",
        ),
    ),
    "audit": (
        "signal-exact-build-audit/v1",
        (
            "SessionId",
            "WindowsUser",
            "ProfileRoot",
            "OslExePath",
            "OslExeSha256",
            "WebView2LoaderSha256",
            "SignalExePath",
            "SignalExeSha256",
            "SignalPublisherSubject",
        ),
    ),
    "arm": (
        "signal-uia-arm/v1",
        (
            "InvocationId",
            "HarnessUri",
            "HarnessSha256",
            "Action",
            "SessionId",
            "WindowsUser",
            "ProfileRoot",
            "OslExePath",
            "OslExeSha256",
            "SignalExePath",
            "SignalExeSha256",
            "SignalPublisherSubject",
            "CaseId",
            "UploadUri",
        ),
    ),
    "poll": ("signal-uia-poll/v1", ("InvocationId",)),
}
FORBIDDEN_KEYS = re.compile(
    r"(?:password|passphrase|credential|private.?key|recovery|seed|mnemonic|totp|otp|"
    r"token|cookie|session.?key|auth(?:orization)?.?header|sas|link(?:ing)?.?(?:code|secret)|"
    r"qr|phone|message.?content)",
    re.I,
)
HEX_256 = re.compile(r"[0-9a-fA-F]{64}")
CASE_ID = re.compile(r"[A-Za-z0-9._-]{1,48}")
AUTOMATION_ACTIONS = (
    "Inventory",
    "ClaimExactWindow",
    "AlreadyRunningAccessibilityBench",
    "CaptureSafeChrome",
)

# Code-owned complete matrix. Manifests cannot omit an identity, direction, or
# security case. All stages remain blocked until reviewed live selectors exist.
QA_CASES = (
    ("text", "protectedText"),
    ("multilineUtf8", "protectedMultilineUtf8"),
    ("encryption", "encryptionRoundTrip"),
    ("protectedComposer", "protectedComposerBinding"),
    ("transcriptOverlay", "transcriptOverlay"),
    ("burn", "burn"),
    ("covertext", "covertext"),
    ("attachmentsImages", "attachmentImage"),
    ("receipts", "deliveryReadReceipts"),
    ("reconnect", "reconnect"),
    ("replayRejection", "replayRejection"),
    ("malformedRejection", "malformedDataRejection"),
    ("expiry", "expiry"),
    ("windowLifecycle", "windowLifecycle"),
)
DIRECTIONS = (("signal-qa-1", "signal-qa-2"), ("signal-qa-2", "signal-qa-1"))


class QaError(RuntimeError):
    pass


def _load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise QaError(f"cannot read manifest {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise QaError("manifest root must be an object")
    return value


def _reject_secrets(value: Any, at: str = "manifest") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            name = str(key)
            if FORBIDDEN_KEYS.search(name):
                raise QaError(f"credential-like manifest field is forbidden: {at}.{name}")
            _reject_secrets(child, f"{at}.{name}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            _reject_secrets(child, f"{at}[{index}]")


def _query_free_https(value: Any, field: str) -> None:
    parsed = urlsplit(str(value))
    if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
        raise QaError(f"{field} must be a query-free HTTPS URI")


def validate_manifest(manifest: dict[str, Any]) -> None:
    _reject_secrets(manifest)
    if manifest.get("adapter") != "signal-desktop":
        raise QaError("adapter must be exactly 'signal-desktop'; browser routes fail closed")

    machines = manifest.get("machines")
    if not isinstance(machines, dict) or tuple(sorted(machines)) != ALIASES:
        raise QaError("machines must contain exactly signal-qa-1 and signal-qa-2")

    required_vm = (
        "resourceGroup",
        "vmName",
        "sessionId",
        "windowsUser",
        "profileRoot",
        "oslExePath",
        "signalExePath",
        "signalExeSha256",
        "signalPublisherSubject",
        "safeScreenshotUploadBaseUri",
    )
    for alias in ALIASES:
        vm = machines[alias]
        if not isinstance(vm, dict):
            raise QaError(f"{alias} must be an object")
        missing = [key for key in required_vm if not vm.get(key)]
        if missing:
            raise QaError(f"{alias} is missing explicit fields: {', '.join(missing)}")
        if not isinstance(vm["sessionId"], int) or isinstance(vm["sessionId"], bool) or not 1 <= vm["sessionId"] <= 128:
            raise QaError(f"{alias}.sessionId must be an integer from 1 through 128")
        if Path(str(vm["signalExePath"]).replace("\\", "/")).name.lower() != "signal.exe":
            raise QaError(f"{alias}.signalExePath must identify Signal.exe")
        if not HEX_256.fullmatch(str(vm["signalExeSha256"])):
            raise QaError(f"{alias}.signalExeSha256 must be a SHA-256 digest")
        if "identityImportSecretRef" in vm:
            raise QaError(f"{alias}.identityImportSecretRef is forbidden; fresh QA profiles create disposable identities")
        if "keyVaultSecretRef" in vm:
            raise QaError(f"{alias}.keyVaultSecretRef is forbidden; the QA identity is device-sealed")
        _query_free_https(vm["safeScreenshotUploadBaseUri"], f"{alias}.safeScreenshotUploadBaseUri")

    leaf_dir = manifest.get("leafScriptDirectory")
    if not isinstance(leaf_dir, str) or not leaf_dir.strip():
        raise QaError("leafScriptDirectory must locate the versioned Signal QA leaves")

    build = manifest.get("build")
    required_build = (
        "exeUri",
        "exeSha256",
        "webView2LoaderUri",
        "webView2LoaderSha256",
        "harnessUri",
        "harnessSha256",
    )
    if not isinstance(build, dict):
        raise QaError("build must be an object")
    missing = [key for key in required_build if not build.get(key)]
    if missing:
        raise QaError(f"build is missing fields: {', '.join(missing)}")
    for key in ("exeUri", "webView2LoaderUri", "harnessUri"):
        _query_free_https(build[key], f"build.{key}")
    for key in ("exeSha256", "webView2LoaderSha256", "harnessSha256"):
        if not HEX_256.fullmatch(str(build[key])):
            raise QaError(f"build.{key} must be a SHA-256 digest")


def preflight(manifest: dict[str, Any]) -> dict[str, Path]:
    if not shutil.which("az"):
        raise QaError("missing tool: az (Azure CLI)")
    leaf_dir = Path(manifest["leafScriptDirectory"]).expanduser().resolve()
    missing = [name for name in LEAVES.values() if not (leaf_dir / name).is_file()]
    if missing:
        raise QaError(f"missing exact Signal QA leaves in {leaf_dir}: {', '.join(missing)}")

    leaves = {key: leaf_dir / name for key, name in LEAVES.items()}
    incompatible: list[str] = []
    for key, path in leaves.items():
        source = path.read_text(encoding="utf-8-sig")
        capability, parameters = CAPABILITIES[key]
        if f"# OSL-QA-Capability: {capability}" not in source:
            incompatible.append(f"{path.name} lacks capability {capability}")
            continue
        absent = [
            parameter
            for parameter in parameters
            if not re.search(rf"[$]{re.escape(parameter)}\b", source, re.I)
        ]
        if absent:
            incompatible.append(f"{path.name} lacks parameters {','.join(absent)}")
    if incompatible:
        raise QaError("incompatible Signal QA leaves: " + "; ".join(incompatible))
    return leaves


def _az(vm: dict[str, Any], script: Path, parameters: dict[str, Any]) -> dict[str, Any]:
    command = [
        "az",
        "vm",
        "run-command",
        "invoke",
        "--only-show-errors",
        "-g",
        vm["resourceGroup"],
        "-n",
        vm["vmName"],
        "--command-id",
        "RunPowerShellScript",
        "--scripts",
        f"@{script}",
        "-o",
        "json",
        "--parameters",
    ]
    command.extend(f"{key}={value}" for key, value in parameters.items())
    result = subprocess.run(command, text=True, capture_output=True, timeout=300)
    if result.returncode:
        detail = (result.stderr or result.stdout).strip().replace("\n", " ")[:240]
        raise QaError(f"Azure run-command failed: {detail}")
    try:
        response = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise QaError("Azure run-command returned invalid JSON") from exc
    records = response.get("value") if isinstance(response, dict) else None
    if not isinstance(records, list) or not records:
        raise QaError("Azure run-command returned no guest status")
    for record in records:
        if not isinstance(record, dict):
            raise QaError("Azure run-command returned malformed guest status")
        code = str(record.get("code", ""))
        message = str(record.get("message", ""))
        if not code.endswith("/succeeded") or ("/StdErr/" in code and message.strip()):
            raise QaError("Azure guest script reported an error")
    return response


def _parallel(stage: str, manifest: dict[str, Any], operation: Any) -> dict[str, Any]:
    output: dict[str, Any] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        futures = {
            pool.submit(operation, alias, manifest["machines"][alias]): alias
            for alias in ALIASES
        }
        for future in concurrent.futures.as_completed(futures):
            alias = futures[future]
            try:
                output[alias] = future.result()
            except Exception as exc:
                raise QaError(f"{stage} failed on {alias}: {exc}") from exc
    return output


def _common(vm: dict[str, Any], build: dict[str, Any]) -> dict[str, Any]:
    return {
        "SessionId": vm["sessionId"],
        "WindowsUser": vm["windowsUser"],
        "ProfileRoot": vm["profileRoot"],
        "OslExePath": vm["oslExePath"],
        "OslExeSha256": build["exeSha256"],
        "SignalExePath": vm["signalExePath"],
        "SignalExeSha256": vm["signalExeSha256"],
        "SignalPublisherSubject": vm["signalPublisherSubject"],
    }


def _write_receipt(path: Path, receipt: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(receipt, ensure_ascii=True, indent=2, sort_keys=True) + "\n",
        encoding="ascii",
    )
    temporary.replace(path)


def stage_plan() -> list[dict[str, Any]]:
    stages: list[dict[str, Any]] = [
        {"id": "deploy-1", "mode": "parallel", "support": "implemented"},
        {"id": "audit-1", "mode": "parallel", "support": "implemented"},
        {"id": "deploy-2-preserve", "mode": "parallel", "support": "implemented"},
        {"id": "audit-2", "mode": "parallel", "support": "implemented"},
        {"id": "inventory", "mode": "parallel", "support": "scaffold"},
        {"id": "claim-exact-window", "mode": "parallel", "support": "scaffold"},
        {"id": "already-running-accessibility-bench", "mode": "parallel", "support": "implemented"},
    ]
    for sender, receiver in DIRECTIONS:
        for case_id, action in QA_CASES:
            stages.append(
                {
                    "id": f"{sender}-to-{receiver}-{case_id}",
                    "mode": "ordered",
                    "sender": sender,
                    "receiver": receiver,
                    "action": action,
                    "support": "blockedUntilLiveAdapterReviewed",
                }
            )
    stages.append({"id": "safe-chrome-screenshots", "mode": "parallel", "support": "implemented"})
    return stages


def run(manifest_path: Path, receipt_dir: Path) -> Path:
    manifest = _load(manifest_path)
    validate_manifest(manifest)
    leaves = preflight(manifest)
    build = manifest["build"]
    run_id = f"signal-qa-{dt.datetime.now(dt.timezone.utc):%Y%m%d%H%M%S}-{uuid.uuid4().hex[:8]}"
    receipt_path = receipt_dir / f"{run_id}.json"
    receipt: dict[str, Any] = {
        "schemaVersion": 1,
        "runId": run_id,
        "adapter": "signal-desktop",
        "status": "running",
        "startedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
        "machines": {
            alias: {"vmName": manifest["machines"][alias]["vmName"]}
            for alias in ALIASES
        },
        "stages": [],
    }

    def execute_stage(name: str, operation: Any) -> None:
        try:
            _parallel(name, manifest, operation)
        except Exception:
            receipt.update(
                {
                    "status": "failed",
                    "terminal": True,
                    "failedStage": name,
                    "failureCode": "stage-failed",
                    "finishedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
                }
            )
            _write_receipt(receipt_path, receipt)
            raise QaError(f"{name} failed; details redacted; receipt: {receipt_path}") from None
        receipt["stages"].append({"name": name, "status": "ok"})

    def deploy(alias: str, vm: dict[str, Any], suffix: str) -> dict[str, Any]:
        return _az(
            vm,
            leaves["deploy"],
            {
                "SessionId": vm["sessionId"],
                "WindowsUser": vm["windowsUser"],
                "ProfileRoot": vm["profileRoot"],
                "OslExePath": vm["oslExePath"],
                "SignalExePath": vm["signalExePath"],
                "SignalExeSha256": vm["signalExeSha256"],
                "SignalPublisherSubject": vm["signalPublisherSubject"],
                "InvocationId": f"{run_id}-{alias[-1]}-{suffix}",
                "ExeUri": build["exeUri"],
                "ExeSha256": build["exeSha256"],
                "WebView2LoaderUri": build["webView2LoaderUri"],
                "WebView2LoaderSha256": build["webView2LoaderSha256"],
            },
        )

    def audit(_alias: str, vm: dict[str, Any]) -> dict[str, Any]:
        return _az(vm, leaves["audit"], {**_common(vm, build), "WebView2LoaderSha256": build["webView2LoaderSha256"]})

    execute_stage("parallel-deploy-1-preserve", lambda alias, vm: deploy(alias, vm, "deploy1"))
    execute_stage("parallel-exact-build-audit-1", audit)
    execute_stage("parallel-deploy-2-preserve", lambda alias, vm: deploy(alias, vm, "deploy2"))
    execute_stage("parallel-exact-build-audit-2", audit)
    execute_stage("parallel-semantic-inventory", lambda alias, _vm: arm_and_poll(manifest_path, alias, "Inventory", "post-redeploy-inventory"))
    execute_stage("parallel-claim-exact-window", lambda alias, _vm: arm_and_poll(manifest_path, alias, "ClaimExactWindow", "post-redeploy-claim"))
    execute_stage(
        "parallel-already-running-accessibility-bench",
        lambda alias, _vm: arm_and_poll(manifest_path, alias, "AlreadyRunningAccessibilityBench", "already-running-a11y-bench"),
    )
    execute_stage(
        "parallel-safe-chrome-screenshot",
        lambda alias, vm: arm_and_poll(
            manifest_path,
            alias,
            "CaptureSafeChrome",
            f"{run_id}-{alias[-1]}-chrome",
            f"{str(vm['safeScreenshotUploadBaseUri']).rstrip('/')}/{run_id}-chrome.png",
        ),
    )
    receipt.update(
        {
            "status": "ready",
            "terminal": True,
            "finishedAt": dt.datetime.now(dt.timezone.utc).isoformat(),
            "liveMatrix": {
                "status": "blockedUntilLiveAdapterReviewed",
                "caseCount": len(QA_CASES) * len(DIRECTIONS),
            },
            "next": "review exact Signal destination/composer selectors on the signed live build",
        }
    )
    _write_receipt(receipt_path, receipt)
    return receipt_path


def arm_and_poll(
    manifest_path: Path,
    alias: str,
    action: str,
    case_id: str,
    upload_uri: str | None = None,
) -> None:
    manifest = _load(manifest_path)
    validate_manifest(manifest)
    leaves = preflight(manifest)
    vm, build = manifest["machines"][alias], manifest["build"]
    invocation = f"signal-{alias[-1]}-{uuid.uuid4().hex[:12]}"
    if action == "CaptureSafeChrome" and upload_uri is None:
        upload_uri = f"{str(vm['safeScreenshotUploadBaseUri']).rstrip('/')}/{case_id}.png"
    parameters = {
        **_common(vm, build),
        "InvocationId": invocation,
        "HarnessUri": build["harnessUri"],
        "HarnessSha256": build["harnessSha256"],
        "Action": action,
        "CaseId": case_id,
    }
    if upload_uri is not None:
        _query_free_https(upload_uri, "safe screenshot upload URI")
        parameters["UploadUri"] = upload_uri
    _az(vm, leaves["arm"], parameters)
    deadline = time.monotonic() + 190
    while True:
        rendered = json.dumps(_az(vm, leaves["poll"], {"InvocationId": invocation}))
        if re.search(r'completed|"Ok"\s*:\s*true', rendered, re.I):
            return
        if re.search(r"harnessFailed|runnerMissingWithoutResult", rendered, re.I):
            raise QaError(f"Signal scaffold action {action} failed on {alias}")
        if time.monotonic() >= deadline:
            raise QaError(f"Signal scaffold action {action} timed out on {alias}")
        time.sleep(2)


def _test_manifest() -> dict[str, Any]:
    zeros = "0" * 64

    def vm(number: int) -> dict[str, Any]:
        return {
            "resourceGroup": "signal-rg",
            "vmName": f"osl-signal-qa-{number}",
            "sessionId": 2,
            "windowsUser": "osltest",
            "profileRoot": r"C:\Users\osltest\AppData\Roaming",
            "oslExePath": r"C:\QA\OSL Privacy.exe",
            "signalExePath": r"C:\Users\osltest\AppData\Local\Programs\signal-desktop\Signal.exe",
            "signalExeSha256": zeros,
            "signalPublisherSubject": "CN=Signal Publisher",
            "safeScreenshotUploadBaseUri": f"https://example.invalid/screenshots/client-{number}",
        }

    return {
        "adapter": "signal-desktop",
        "leafScriptDirectory": str(HERE),
        "machines": {"signal-qa-1": vm(1), "signal-qa-2": vm(2)},
        "build": {
            "exeUri": "https://example.invalid/osl.exe",
            "exeSha256": zeros,
            "webView2LoaderUri": "https://example.invalid/WebView2Loader.dll",
            "webView2LoaderSha256": zeros,
            "harnessUri": "https://example.invalid/signal-harness.ps1",
            "harnessSha256": zeros,
        },
    }


class SignalVmQaSelfTests(unittest.TestCase):
    def test_example_manifest_is_valid(self) -> None:
        validate_manifest(json.loads((HERE / "signal-vm-qa.example.json").read_text(encoding="utf-8")))

    def test_manifest_rejects_browser_route_alias_drift_and_secrets(self) -> None:
        candidate = _test_manifest()
        candidate["adapter"] = "signal-web"
        with self.assertRaisesRegex(QaError, "browser routes fail closed"):
            validate_manifest(candidate)

        candidate = _test_manifest()
        candidate["machines"]["discord-qa-1"] = candidate["machines"].pop("signal-qa-1")
        with self.assertRaisesRegex(QaError, "exactly signal-qa-1 and signal-qa-2"):
            validate_manifest(candidate)

        for field in ("password", "accessToken", "recoveryPhrase", "phoneNumber", "linkingQr"):
            candidate = _test_manifest()
            candidate["machines"]["signal-qa-1"][field] = "forbidden"
            with self.subTest(field=field), self.assertRaisesRegex(QaError, "credential-like"):
                validate_manifest(candidate)

    def test_manifest_rejects_absent_authority_and_unpinned_identity(self) -> None:
        candidate = _test_manifest()
        candidate["machines"]["signal-qa-1"]["keyVaultSecretRef"] = {"vaultName": "old", "secretName": "old"}
        with self.assertRaisesRegex(QaError, "device-sealed"):
            validate_manifest(candidate)

        candidate = _test_manifest()
        candidate["machines"]["signal-qa-1"]["signalExePath"] = r"C:\Browser\chrome.exe"
        with self.assertRaisesRegex(QaError, "Signal.exe"):
            validate_manifest(candidate)

        candidate = _test_manifest()
        candidate["machines"]["signal-qa-1"]["signalExeSha256"] = "latest"
        with self.assertRaisesRegex(QaError, "SHA-256"):
            validate_manifest(candidate)

    def test_query_strings_are_refused_for_artifacts_and_screenshots(self) -> None:
        for field in ("exeUri", "webView2LoaderUri", "harnessUri"):
            candidate = _test_manifest()
            candidate["build"][field] = f"https://example.invalid/{field}?sas=secret"
            with self.subTest(field=field), self.assertRaisesRegex(QaError, "query-free HTTPS"):
                validate_manifest(candidate)

        candidate = _test_manifest()
        candidate["machines"]["signal-qa-1"]["safeScreenshotUploadBaseUri"] = "https://example.invalid/capture?sas=secret"
        with self.assertRaisesRegex(QaError, "query-free HTTPS"):
            validate_manifest(candidate)

    @mock.patch.object(shutil, "which", return_value="/usr/bin/az")
    def test_preflight_accepts_current_signal_leaves(self, _which: mock.Mock) -> None:
        self.assertEqual(set(preflight(_test_manifest())), set(LEAVES))

    @mock.patch.object(shutil, "which", return_value="/usr/bin/az")
    def test_preflight_reports_every_missing_leaf(self, _which: mock.Mock) -> None:
        with tempfile.TemporaryDirectory() as directory:
            candidate = _test_manifest()
            candidate["leafScriptDirectory"] = directory
            with self.assertRaises(QaError) as error:
                preflight(candidate)
        for filename in LEAVES.values():
            self.assertIn(filename, str(error.exception))

    def test_common_parameters_pin_signal_identity_without_secret_references(self) -> None:
        candidate = _test_manifest()
        common = _common(candidate["machines"]["signal-qa-1"], candidate["build"])
        self.assertEqual(common["SignalExeSha256"], "0" * 64)
        self.assertEqual(common["SignalPublisherSubject"], "CN=Signal Publisher")
        self.assertNotIn("SecretName", common)

    def test_parallel_uses_both_dedicated_aliases(self) -> None:
        seen: set[str] = set()
        _parallel("test", _test_manifest(), lambda alias, _vm: seen.add(alias))
        self.assertEqual(seen, set(ALIASES))

    @mock.patch.object(subprocess, "run")
    def test_az_rejects_guest_stderr_without_echoing_it(self, run_command: mock.Mock) -> None:
        run_command.return_value = mock.Mock(
            returncode=0,
            stdout=json.dumps(
                {
                    "value": [
                        {"code": "ComponentStatus/StdOut/succeeded", "message": ""},
                        {"code": "ComponentStatus/StdErr/succeeded", "message": "sensitive guest detail"},
                    ]
                }
            ),
            stderr="",
        )
        with self.assertRaisesRegex(QaError, "guest script reported an error") as error:
            _az(_test_manifest()["machines"]["signal-qa-1"], Path("leaf.ps1"), {})
        self.assertNotIn("sensitive guest detail", str(error.exception))

    def test_complete_live_matrix(self) -> None:
        plan = stage_plan()
        live = [stage for stage in plan if stage["support"] == "blockedUntilLiveAdapterReviewed"]
        expected = {
            (sender, receiver, action)
            for sender, receiver in DIRECTIONS
            for _case_id, action in QA_CASES
        }
        actual = {
            (stage.get("sender"), stage.get("receiver"), stage.get("action"))
            for stage in live
        }
        self.assertEqual(tuple(sorted(ALIASES)), ("signal-qa-1", "signal-qa-2"))
        self.assertEqual(set(DIRECTIONS), {("signal-qa-1", "signal-qa-2"), ("signal-qa-2", "signal-qa-1")})
        self.assertEqual(len(QA_CASES), 14)
        self.assertEqual(len(live), 28)
        self.assertEqual(actual, expected)
        self.assertTrue(all(stage["mode"] == "ordered" for stage in live))

    def test_successful_run_records_preflight_only_then_blocked_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(_test_manifest()), encoding="utf-8")
            calls: list[tuple[str, dict[str, Any]]] = []
            leaves = {key: root / filename for key, filename in LEAVES.items()}
            with (
                mock.patch.object(sys.modules[__name__], "preflight", return_value=leaves),
                mock.patch.object(sys.modules[__name__], "_az", side_effect=lambda _vm, script, parameters: calls.append((script.name, parameters)) or {"ok": True}),
                mock.patch.object(sys.modules[__name__], "arm_and_poll", return_value=None) as arm,
            ):
                receipt_path = run(manifest_path, root / "receipts")
            receipt = json.loads(receipt_path.read_text(encoding="ascii"))
            self.assertEqual(receipt["status"], "ready")
            self.assertEqual(receipt["liveMatrix"], {"caseCount": 28, "status": "blockedUntilLiveAdapterReviewed"})
            names = [name for name, _parameters in calls]
            self.assertEqual(names.count(LEAVES["deploy"]), 4)
            self.assertEqual(names.count(LEAVES["audit"]), 4)
            self.assertEqual(arm.call_count, 8)
            invocations = [parameters["InvocationId"] for _name, parameters in calls if "InvocationId" in parameters]
            self.assertEqual(len(invocations), len(set(invocations)))

    def test_run_signal_already_running_accessibility_bench(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(_test_manifest()), encoding="utf-8")
            leaves = {key: root / filename for key, filename in LEAVES.items()}
            with (
                mock.patch.object(sys.modules[__name__], "preflight", return_value=leaves),
                mock.patch.object(sys.modules[__name__], "_az", return_value={"ok": True}),
                mock.patch.object(sys.modules[__name__], "arm_and_poll", return_value=None) as arm,
            ):
                receipt_path = run(manifest_path, root / "receipts")

            receipt = json.loads(receipt_path.read_text(encoding="ascii"))
            stage_names = [stage["name"] for stage in receipt["stages"]]
            self.assertIn("parallel-already-running-accessibility-bench", stage_names)
            bench_calls = [
                call
                for call in arm.call_args_list
                if call.args[2] == "AlreadyRunningAccessibilityBench"
            ]
            self.assertEqual(len(bench_calls), 2)
            self.assertEqual({call.args[1] for call in bench_calls}, set(ALIASES))
            self.assertTrue(all(call.args[3] == "already-running-a11y-bench" for call in bench_calls))
            self.assertLess(
                stage_names.index("parallel-claim-exact-window"),
                stage_names.index("parallel-already-running-accessibility-bench"),
            )
            self.assertLess(
                stage_names.index("parallel-already-running-accessibility-bench"),
                stage_names.index("parallel-safe-chrome-screenshot"),
            )

    def test_leaf_sources_expose_only_allowed_interactive_actions(self) -> None:
        arm_source = (HERE / LEAVES["arm"]).read_text(encoding="utf-8-sig")
        harness_source = (HERE / "osl-vm-signal-uia-harness.ps1").read_text(encoding="utf-8-sig")
        self.assertIn("ValidateSet('Inventory','ClaimExactWindow','AlreadyRunningAccessibilityBench','CaptureSafeChrome')", arm_source)
        self.assertIn("$safeHeight=48", harness_source)
        self.assertIn("PrintWindow", harness_source)
        self.assertIn("SignalPixelsIncluded=$false", harness_source)
        self.assertIn("MessagePixelsIncluded=$false", harness_source)
        for forbidden in ("SendKeys", "SetForegroundWindow", "ValuePattern", "ConversationList"):
            self.assertNotIn(forbidden, harness_source)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SignalVmQaSelfTests)
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate")
    validate.add_argument("--manifest", required=True, type=Path)
    execute = commands.add_parser("run")
    execute.add_argument("--manifest", required=True, type=Path)
    execute.add_argument("--receipts", type=Path, default=HERE / "receipts")
    plan = commands.add_parser("plan")
    plan.add_argument("--manifest", required=True, type=Path)
    arm = commands.add_parser("arm")
    arm.add_argument("--manifest", required=True, type=Path)
    arm.add_argument("--alias", required=True, choices=ALIASES)
    arm.add_argument("--action", required=True, choices=AUTOMATION_ACTIONS)
    arm.add_argument("--case-id", required=True)
    commands.add_parser("self-test")
    args = parser.parse_args(argv)
    try:
        if args.command == "validate":
            validate_manifest(_load(args.manifest))
            print("manifest valid")
        elif args.command == "plan":
            validate_manifest(_load(args.manifest))
            print(json.dumps({"schemaVersion": 1, "adapter": "signal-desktop", "stages": stage_plan()}, indent=2))
        elif args.command == "run":
            print(run(args.manifest, args.receipts))
        elif args.command == "self-test":
            return run_self_tests()
        else:
            if not CASE_ID.fullmatch(args.case_id):
                raise QaError("case ID is invalid")
            arm_and_poll(args.manifest, args.alias, args.action, args.case_id)
            print("Signal scaffold action completed")
        return 0
    except QaError as exc:
        print(f"Signal QA controller stopped: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
