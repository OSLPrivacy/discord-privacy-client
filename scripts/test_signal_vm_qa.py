from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

PATH = Path(__file__).parent / "qa" / "signal_vm_qa.py"
SPEC = importlib.util.spec_from_file_location("signal_vm_qa", PATH)
qa = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(qa)


def manifest() -> dict:
    zeros = "0" * 64
    def vm(number: int) -> dict:
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
        "leafScriptDirectory": "/signal/leaves",
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


class ManifestTests(unittest.TestCase):
    def test_example_is_valid(self) -> None:
        qa.validate_manifest(json.loads((PATH.parent / "signal-vm-qa.example.json").read_text()))

    def test_only_signal_desktop_adapter_is_accepted(self) -> None:
        value = manifest()
        value["adapter"] = "signal-web"
        with self.assertRaisesRegex(qa.QaError, "browser routes fail closed"):
            qa.validate_manifest(value)

    def test_only_dedicated_signal_aliases_are_accepted(self) -> None:
        value = manifest()
        value["machines"]["discord-qa-1"] = value["machines"].pop("signal-qa-1")
        with self.assertRaisesRegex(qa.QaError, "exactly signal-qa-1 and signal-qa-2"):
            qa.validate_manifest(value)

    def test_credential_like_fields_are_rejected(self) -> None:
        for field in ("password", "accessToken", "recoveryPhrase", "phoneNumber", "linkingQr"):
            with self.subTest(field=field):
                value = manifest()
                value["machines"]["signal-qa-1"][field] = "forbidden"
                with self.assertRaisesRegex(qa.QaError, "credential-like"):
                    qa.validate_manifest(value)

    def test_key_vault_reference_is_not_accepted(self) -> None:
        value = manifest()
        value["machines"]["signal-qa-1"]["keyVaultSecretRef"] = {"vaultName": "old", "secretName": "old"}
        with self.assertRaisesRegex(qa.QaError, "device-sealed"):
            qa.validate_manifest(value)

    def test_signal_path_and_hash_are_explicit(self) -> None:
        value = manifest()
        value["machines"]["signal-qa-1"]["signalExePath"] = r"C:\Browser\chrome.exe"
        with self.assertRaisesRegex(qa.QaError, "Signal.exe"):
            qa.validate_manifest(value)
        value = manifest()
        value["machines"]["signal-qa-1"]["signalExeSha256"] = "latest"
        with self.assertRaisesRegex(qa.QaError, "SHA-256"):
            qa.validate_manifest(value)

    def test_artifact_uris_are_query_free_https(self) -> None:
        for uri in ("http://example.invalid/osl.exe", "https://example.invalid/osl.exe?sas=x"):
            with self.subTest(uri=uri):
                value = manifest()
                value["build"]["exeUri"] = uri
                with self.assertRaisesRegex(qa.QaError, "query-free HTTPS"):
                    qa.validate_manifest(value)

    @mock.patch.object(qa.shutil, "which", return_value="/usr/bin/az")
    def test_preflight_accepts_current_signal_leaves(self, _which: mock.Mock) -> None:
        value = manifest()
        value["leafScriptDirectory"] = str(PATH.parent)
        leaves = qa.preflight(value)
        self.assertEqual(set(leaves), set(qa.LEAVES))

    @mock.patch.object(qa.shutil, "which", return_value="/usr/bin/az")
    def test_preflight_reports_every_missing_leaf(self, _which: mock.Mock) -> None:
        with tempfile.TemporaryDirectory() as directory:
            value = manifest()
            value["leafScriptDirectory"] = directory
            with self.assertRaises(qa.QaError) as error:
                qa.preflight(value)
        for filename in qa.LEAVES.values():
            self.assertIn(filename, str(error.exception))

    def test_common_parameters_pin_signal_identity(self) -> None:
        value = manifest()
        common = qa._common(value["machines"]["signal-qa-1"], value["build"])
        self.assertEqual(common["SignalExeSha256"], "0" * 64)
        self.assertEqual(common["SignalPublisherSubject"], "CN=Signal Publisher")
        self.assertNotIn("SecretName", common)

    def test_parallel_uses_both_dedicated_aliases(self) -> None:
        seen = set()
        qa._parallel("test", manifest(), lambda alias, _vm: seen.add(alias))
        self.assertEqual(seen, set(qa.ALIASES))

    @mock.patch.object(qa.subprocess, "run")
    def test_az_rejects_guest_powershell_stderr_even_when_cli_succeeds(self, run: mock.Mock) -> None:
        run.return_value = mock.Mock(
            returncode=0,
            stdout=json.dumps({"value": [
                {"code": "ComponentStatus/StdOut/succeeded", "message": ""},
                {"code": "ComponentStatus/StdErr/succeeded", "message": "sensitive guest detail"},
            ]}),
            stderr="",
        )
        with self.assertRaisesRegex(qa.QaError, "guest script reported an error") as error:
            qa._az(manifest()["machines"]["signal-qa-1"], Path("leaf.ps1"), {})
        self.assertNotIn("sensitive guest detail", str(error.exception))

    @mock.patch.object(qa.subprocess, "run")
    def test_az_accepts_clean_guest_status(self, run: mock.Mock) -> None:
        expected = {"value": [
            {"code": "ComponentStatus/StdOut/succeeded", "message": '{"Ok":true}'},
            {"code": "ComponentStatus/StdErr/succeeded", "message": ""},
        ]}
        run.return_value = mock.Mock(returncode=0, stdout=json.dumps(expected), stderr="")
        self.assertEqual(
            qa._az(manifest()["machines"]["signal-qa-1"], Path("leaf.ps1"), {}),
            expected,
        )

    def test_stage_failure_writes_terminal_redacted_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(manifest()), encoding="utf-8")
            leaves = {key: root / filename for key, filename in qa.LEAVES.items()}
            with mock.patch.object(qa, "preflight", return_value=leaves), mock.patch.object(
                qa, "_parallel", side_effect=qa.QaError("secret-value")
            ):
                with self.assertRaisesRegex(qa.QaError, "details redacted"):
                    qa.run(manifest_path, root / "receipts")
            receipts = list((root / "receipts").glob("*.json"))
            self.assertEqual(len(receipts), 1)
            receipt_text = receipts[0].read_text(encoding="utf-8")
            receipt = json.loads(receipt_text)
            self.assertEqual(receipt["status"], "failed")
            self.assertTrue(receipt["terminal"])
            self.assertEqual(receipt["failedStage"], "parallel-deploy-1-preserve")
            self.assertEqual(receipt["failureCode"], "stage-failed")
            self.assertNotIn("secret-value", receipt_text)

    def test_successful_run_performs_two_pinned_deploy_audit_cycles(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(manifest()), encoding="utf-8")
            leaves = {key: root / filename for key, filename in qa.LEAVES.items()}
            calls = []
            with mock.patch.object(qa, "preflight", return_value=leaves), mock.patch.object(
                qa, "_az", side_effect=lambda vm, script, parameters: calls.append((script.name, parameters)) or {"ok": True}
            ), mock.patch.object(qa, "arm_and_poll", return_value=None) as arm_and_poll:
                receipt_path = qa.run(manifest_path, root / "receipts")
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"], "ready")
            self.assertEqual(receipt["liveMatrix"]["caseCount"], 28)
            names = [name for name, _ in calls]
            self.assertEqual(names.count(qa.LEAVES["deploy"]), 4)
            self.assertEqual(names.count(qa.LEAVES["audit"]), 4)
            self.assertEqual(arm_and_poll.call_count, 8)
            bench_calls = [call for call in arm_and_poll.call_args_list if call.args[2] == "AlreadyRunningAccessibilityBench"]
            self.assertEqual(len(bench_calls), 2)
            self.assertTrue(all(call.args[3] == "already-running-a11y-bench" for call in bench_calls))
            screenshot_calls = [call for call in arm_and_poll.call_args_list if call.args[2] == "CaptureSafeChrome"]
            self.assertEqual(len(screenshot_calls), 2)
            self.assertTrue(all(call.args[4].startswith("https://example.invalid/screenshots/") for call in screenshot_calls))
            invocations = [parameters["InvocationId"] for name, parameters in calls if name == qa.LEAVES["deploy"]]
            self.assertEqual(len(invocations), len(set(invocations)))
            stage_names = [stage["name"] for stage in receipt["stages"]]
            self.assertLess(stage_names.index("parallel-exact-build-audit-1"), stage_names.index("parallel-deploy-2-preserve"))
            self.assertLess(stage_names.index("parallel-exact-build-audit-2"), stage_names.index("parallel-claim-exact-window"))
            self.assertLess(stage_names.index("parallel-claim-exact-window"), stage_names.index("parallel-already-running-accessibility-bench"))
            self.assertLess(stage_names.index("parallel-already-running-accessibility-bench"), stage_names.index("parallel-safe-chrome-screenshot"))

    def test_run_signal_already_running_accessibility_bench(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "manifest.json"
            manifest_path.write_text(json.dumps(manifest()), encoding="utf-8")
            leaves = {key: root / filename for key, filename in qa.LEAVES.items()}
            with mock.patch.object(qa, "preflight", return_value=leaves), mock.patch.object(
                qa, "_az", return_value={"ok": True}
            ), mock.patch.object(qa, "arm_and_poll", return_value=None) as arm_and_poll:
                receipt_path = qa.run(manifest_path, root / "receipts")
            receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
            stage_names = [stage["name"] for stage in receipt["stages"]]
            self.assertIn("parallel-already-running-accessibility-bench", stage_names)
            bench_calls = [
                call for call in arm_and_poll.call_args_list
                if call.args[2] == "AlreadyRunningAccessibilityBench"
            ]
            self.assertEqual(len(bench_calls), 2)
            self.assertEqual({call.args[1] for call in bench_calls}, set(qa.ALIASES))
            self.assertTrue(all(call.args[3] == "already-running-a11y-bench" for call in bench_calls))
            self.assertLess(
                stage_names.index("parallel-claim-exact-window"),
                stage_names.index("parallel-already-running-accessibility-bench"),
            )

    def test_bootstrap_rejects_obsolete_identity_import_reference(self) -> None:
        value = manifest()
        value["machines"]["signal-qa-1"]["identityImportSecretRef"] = {
            "vaultName": "signal-vault",
            "secretName": "must-not-be-used",
        }
        with self.assertRaisesRegex(qa.QaError, "identityImportSecretRef is forbidden"):
            qa.validate_manifest(value)

    def test_safe_screenshot_upload_is_query_free_https(self) -> None:
        value = manifest()
        value["machines"]["signal-qa-1"]["safeScreenshotUploadBaseUri"] = "https://example.invalid/capture?sas=secret"
        with self.assertRaisesRegex(qa.QaError, "query-free HTTPS"):
            qa.validate_manifest(value)

    def test_plan_covers_every_case_in_both_directions(self) -> None:
        plan = qa.stage_plan()
        self.assertEqual(len(plan), 36)
        live = [stage for stage in plan if stage["support"] == "blockedUntilLiveAdapterReviewed"]
        self.assertEqual(len(live), len(qa.QA_CASES) * 2)
        self.assertEqual({(stage["sender"], stage["receiver"]) for stage in live}, set(qa.DIRECTIONS))
        for case_id, action in qa.QA_CASES:
            self.assertEqual(sum(stage.get("action") == action for stage in live), 2, case_id)
        self.assertTrue(all(stage["support"] == "blockedUntilLiveAdapterReviewed" for stage in live))

    def test_complete_live_matrix(self) -> None:
        plan = qa.stage_plan()
        live = [stage for stage in plan if stage["support"] == "blockedUntilLiveAdapterReviewed"]
        expected_pairs = {
            (sender, receiver, action)
            for sender, receiver in qa.DIRECTIONS
            for _case_id, action in qa.QA_CASES
        }
        actual_pairs = {
            (stage.get("sender"), stage.get("receiver"), stage.get("action"))
            for stage in live
        }

        self.assertEqual(tuple(sorted(qa.ALIASES)), ("signal-qa-1", "signal-qa-2"))
        self.assertEqual(
            set(qa.DIRECTIONS),
            {("signal-qa-1", "signal-qa-2"), ("signal-qa-2", "signal-qa-1")},
        )
        self.assertEqual(len(qa.QA_CASES), 14)
        self.assertEqual(len(live), 28)
        self.assertEqual(actual_pairs, expected_pairs)
        self.assertTrue(all(stage["mode"] == "ordered" for stage in live))
        self.assertTrue(all("caseId" not in stage for stage in live))

    def test_plan_has_two_deploy_audit_cycles_and_safe_screenshot(self) -> None:
        ids = [stage["id"] for stage in qa.stage_plan()]
        self.assertLess(ids.index("deploy-1"), ids.index("audit-1"))
        self.assertLess(ids.index("audit-1"), ids.index("deploy-2-preserve"))
        self.assertLess(ids.index("deploy-2-preserve"), ids.index("audit-2"))
        self.assertLess(ids.index("claim-exact-window"), ids.index("already-running-accessibility-bench"))
        self.assertLess(ids.index("already-running-accessibility-bench"), ids.index("safe-chrome-screenshots"))
        self.assertIn("safe-chrome-screenshots", ids)


class LeafStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sources = {
            key: (PATH.parent / filename).read_text(encoding="utf-8-sig")
            for key, filename in qa.LEAVES.items()
        }

    def test_deploy_stops_only_exact_osl_and_preserves_signal_pid_set(self) -> None:
        source = self.sources["deploy"]
        self.assertIn("Get-SignalPids", source)
        self.assertIn("Signal PID set changed", source)
        self.assertIn("$primaryBefore|ForEach-Object{Stop-Process", source)
        self.assertNotRegex(source, r"Stop-Process[^\n]*(?:Signal|Discord|Telegram)")
        self.assertNotIn("Remove-Item -Recurse", source)

    def test_guardian_marker_is_classified_consistently(self) -> None:
        marker = "--osl-signal-window-guardian-v1"
        for key in ("deploy", "audit", "arm"):
            self.assertIn(marker, self.sources[key], key)
        self.assertNotIn("--osl-borrowed-window-guardian-v1", "\n".join(self.sources.values()))

    def test_deploy_allows_guardian_restore_before_binary_replacement(self) -> None:
        source = self.sources["deploy"]
        stop = source.index("$primaryBefore|ForEach-Object{Stop-Process")
        wait = source.index("Signal guardian did not restore and exit before replacement")
        replace = source.index("[IO.File]::Replace($exeStage")
        self.assertLess(stop, wait)
        self.assertLess(wait, replace)
        self.assertNotIn("Get-ExactSignalGuardians)|ForEach-Object{Stop-Process", source)

    def test_deploy_supports_empty_install_without_replacing_a_profile(self) -> None:
        source = self.sources["deploy"]
        self.assertIn("$initialInstall=-not $exePresent", source)
        self.assertIn("if($exePresent -xor $loaderPresent)", source)
        self.assertIn("if($initialInstall -and -not(Test-Path -LiteralPath $install -PathType Container))", source)
        self.assertIn("configured installation parent is missing", source)
        self.assertIn("New-Item -ItemType Directory -Path $install", source)
        self.assertIn("[IO.File]::Move($exeStage,$path);$installedExe=$true", source)
        self.assertIn("Get-ExactPrimaryProcesses)|ForEach-Object{Stop-Process", source)
        self.assertNotIn("Remove-Item -LiteralPath $profile", source)

    def test_signal_identity_is_hash_and_publisher_pinned(self) -> None:
        for key in ("deploy", "audit", "arm"):
            source = self.sources[key]
            self.assertIn("$SignalExeSha256", source)
            self.assertIn("$SignalPublisherSubject", source)
            self.assertIn("Get-AuthenticodeSignature", source)

    def test_signal_roots_and_tasks_do_not_collide_with_other_lanes(self) -> None:
        combined = "\n".join(self.sources.values())
        self.assertIn(r"C:\ProgramData\OSL-QA\signal", combined)
        self.assertIn("OSL-QA-Signal-", combined)
        self.assertNotIn("OSL-QA-Discord-", combined)
        self.assertNotIn(r"\discord-uia", combined)

    def test_arm_is_scaffolding_not_message_automation(self) -> None:
        source = self.sources["arm"]
        self.assertIn("ValidateSet('Inventory','ClaimExactWindow','AlreadyRunningAccessibilityBench','CaptureSafeChrome')", source)
        for action in ("Send", "InspectInbound", "OpenVerifiedFriend", "PrepareOverlay"):
            self.assertNotIn(f"'{action}'", source)

    def test_scripts_do_not_name_signal_private_storage(self) -> None:
        combined = "\n".join(self.sources.values()).lower()
        for forbidden in ("signal.sqlite", "sqlcipher", "config.json", "databases", "local storage"):
            self.assertNotIn(forbidden, combined)

    def test_screenshot_is_exact_osl_titlebar_only_and_uploaded_by_identity(self) -> None:
        source = (PATH.parent / "osl-vm-signal-uia-harness.ps1").read_text(encoding="utf-8-sig")
        self.assertIn("PrintWindow", source)
        self.assertIn("$safeHeight=48", source)
        self.assertIn("'Tauri Window'", source)
        self.assertIn("'OSL Signal QA'", source)
        self.assertNotRegex(source, r"(?i)\$pid\s*=")
        self.assertIn("Region='osl-titlebar-only'", source)
        self.assertIn("SignalPixelsIncluded=$false", source)
        self.assertIn("MessagePixelsIncluded=$false", source)
        self.assertIn("metadata/identity/oauth2/token", source)
        self.assertNotIn("CopyFromScreen", source)

    def test_interactive_harness_exposes_only_inventory_and_exact_claim(self) -> None:
        source = (PATH.parent / "osl-vm-signal-uia-harness.ps1").read_text(encoding="utf-8-sig")
        self.assertIn("ValidateSet('Inventory','ClaimExactWindow','AlreadyRunningAccessibilityBench','CaptureSafeChrome')", source)
        self.assertIn("Chrome_WidgetWin_1", source)
        self.assertIn("Title -ceq 'Signal'", source)
        self.assertIn("TitleIsExactSignal", source)
        self.assertIn("TitleIsExactQa", source)
        self.assertIn("Title -ceq 'OSL Signal QA'", source)
        self.assertIn("$candidatePid", source)
        self.assertNotRegex(source, r"(?i)\$pid\s*=")
        self.assertIn("ContentInspected=$false", source)
        self.assertIn("ForegroundChanged=$false", source)
        self.assertIn("SignalAlreadyRunning=$true", source)
        self.assertIn("ControlType.ProgrammaticName", source)
        for forbidden in ("SendKeys", "SetForegroundWindow", "ValuePattern", "message text", "ConversationList"):
            self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
