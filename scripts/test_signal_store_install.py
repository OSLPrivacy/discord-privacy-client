from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).parent
LANE = ROOT / "qa" / "signal-install"
MODULE_PATH = LANE / "install_signal_store.py"
SPEC = importlib.util.spec_from_file_location("install_signal_store", MODULE_PATH)
install = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(install)


class StaticLeafTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.arm = (LANE / "arm-signal-store-install.ps1").read_text(encoding="utf-8-sig")
        cls.poll = (LANE / "poll-signal-store-install.ps1").read_text(encoding="utf-8-sig")
        cls.combined = cls.arm + "\n" + cls.poll

    def test_store_identity_is_fixed_and_exact(self) -> None:
        self.assertIn("XP89119P9F2PCQ", self.arm)
        self.assertIn("'--exact'", self.arm)
        self.assertIn("'--source', 'msstore'", self.arm)
        self.assertNotRegex(self.arm, r"https?://|Invoke-WebRequest|Start-BitsTransfer")

    def test_runs_in_exact_limited_interactive_session(self) -> None:
        self.assertIn("Name = 'explorer.exe'", self.arm)
        self.assertIn("$owner.User -cne $WindowsUser", self.arm)
        self.assertIn("-LogonType Interactive -RunLevel Limited", self.arm)
        self.assertIn("FromMinutes(10)", self.arm)

    def test_does_not_launch_or_inspect_signal_private_state(self) -> None:
        self.assertNotRegex(self.arm, r"Start-Process[^\n]+Signal(?:\.exe)?")
        self.assertNotRegex(self.combined, r"Cookies|Local State|sql(?:ite)?|leveldb|messages|attachments|config\.json")
        self.assertNotIn("Get-ChildItem", self.combined)
        self.assertIn("running Signal process identity mismatch", self.arm)
        self.assertIn("Signal must remain closed before installation", self.arm)

    def test_discovery_is_one_documented_candidate_and_fails_closed(self) -> None:
        self.assertIn("Programs\\signal-desktop\\Signal.exe", self.arm)
        self.assertIn("documented Signal executable candidate was not found", self.arm)
        self.assertIn("Get-AuthenticodeSignature", self.arm)
        self.assertIn("LocalAppDataProgramsSignalDesktop", self.arm)

    def test_winget_is_resolved_from_one_signed_store_package(self) -> None:
        self.assertIn("Get-AppxPackage -Name 'Microsoft.DesktopAppInstaller'", self.arm)
        self.assertIn("$appInstallers.Count -ne 1", self.arm)
        self.assertIn("$appInstaller.SignatureKind -cne 'Store'", self.arm)
        self.assertIn("Join-Path ([string]$appInstaller.InstallLocation) 'winget.exe'", self.arm)
        self.assertIn("$wingetSignature.Status -cne 'Valid'", self.arm)
        self.assertNotIn("Get-Command winget.exe", self.arm)

    def test_audit_mode_never_reinstalls_or_closes_signal(self) -> None:
        self.assertIn("ValidateSet('install','audit')", self.arm)
        self.assertIn("if ($r.Mode -ceq 'install')", self.arm)
        self.assertIn("running-exact-candidate", self.arm)
        self.assertNotRegex(self.arm, r"Stop-Process[^\n]+Signal")

    def test_receipt_is_atomic_and_metadata_only(self) -> None:
        self.assertIn("[IO.File]::Move($temporary", self.arm)
        for key in ("Version", "Sha256", "Publisher", "PathClass"):
            self.assertIn(key, self.arm)
        self.assertNotRegex(self.combined, r"password|token|cookie|phone|qr|message.?content")


class ControllerTests(unittest.TestCase):
    def test_rejects_other_provider_and_nondedicated_targets(self) -> None:
        with self.assertRaisesRegex(install.InstallError, "dedicated Signal lane"):
            install._validate_target("qa-rg", "qa-vm", 2, "osltest")
        with self.assertRaisesRegex(install.InstallError, "another provider"):
            install._validate_target("signal-rg", "signal-discord-vm", 2, "osltest")
        with self.assertRaisesRegex(install.InstallError, "exactly osltest"):
            install._validate_target("signal-rg", "signal-vm", 2, "administrator")

    def test_extracts_only_semantic_runcommand_receipt(self) -> None:
        receipt = {"Status": "armed", "InvocationId": "sigstore-12345678"}
        envelope = {"value": [{"message": "noise\n" + json.dumps(receipt)}]}
        self.assertEqual(install._extract(json.dumps(envelope)), receipt)
        with self.assertRaisesRegex(install.InstallError, "semantic receipt"):
            install._extract(json.dumps({"value": [{"message": "noise"}]}))

    def test_completed_run_writes_only_allowlisted_executable_evidence(self) -> None:
        invocation = "sigstore-12345678901234567890"
        results = iter([
            {"Status": "armed", "Terminal": False, "InvocationId": invocation, "ProductId": install.PRODUCT_ID, "Source": install.STORE_SOURCE, "Mode": "install"},
            {"Status": "completed", "Terminal": True, "InvocationId": invocation, "ProductId": install.PRODUCT_ID, "Source": install.STORE_SOURCE,
             "Detail": {"Version": "7.99.0", "Sha256": "a" * 64, "Publisher": "CN=Signal Messenger, LLC", "PathClass": "LocalAppDataProgramsSignalDesktop", "ProcessState": "installer-auto-launched-exact-candidate"}},
        ])
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(install.uuid, "uuid4", return_value=mock.Mock(hex="12345678901234567890abcdef")), mock.patch.object(install, "_invoke", side_effect=lambda *_args, **_kwargs: next(results)):
            path = install.run("signal-rg", "signal-vm-1", 2, "osltest", Path(directory), poll_seconds=0)
            receipt = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(receipt["signalExecutable"]["Sha256"], "a" * 64)
        self.assertEqual(set(receipt["signalExecutable"]), {"Version", "Sha256", "Publisher", "PathClass", "ProcessState"})
        self.assertNotRegex(json.dumps(receipt), r"(?i)password|token|cookie|qr|phone|message")

    def test_failed_terminal_receipt_does_not_write_success_receipt(self) -> None:
        invocation = "sigstore-12345678901234567890"
        results = iter([
            {"Status": "armed", "Terminal": False, "InvocationId": invocation, "ProductId": install.PRODUCT_ID, "Source": install.STORE_SOURCE, "Mode": "install"},
            {"Status": "failed", "Terminal": True, "InvocationId": invocation, "ProductId": install.PRODUCT_ID, "Source": install.STORE_SOURCE, "Detail": {"FailureCode": "signature-validation-failed"}},
        ])
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(install.uuid, "uuid4", return_value=mock.Mock(hex="12345678901234567890abcdef")), mock.patch.object(install, "_invoke", side_effect=lambda *_args, **_kwargs: next(results)):
            with self.assertRaisesRegex(install.InstallError, "signature-validation-failed"):
                install.run("signal-rg", "signal-vm-1", 2, "osltest", Path(directory), poll_seconds=0)
            self.assertEqual(list(Path(directory).glob("*.json")), [])


if __name__ == "__main__":
    unittest.main()
