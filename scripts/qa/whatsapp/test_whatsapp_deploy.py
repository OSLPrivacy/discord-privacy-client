from __future__ import annotations

import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent
POWERSHELL = (ROOT / "deploy-whatsapp-preserve.ps1").read_text(encoding="utf-8")
DISCOVER = (ROOT / "discover-whatsapp-session.ps1").read_text(encoding="utf-8")
RUNTIME_AUDIT = (ROOT / "audit-whatsapp-qa-runtime.ps1").read_text(encoding="utf-8")
MAIN_RS = (ROOT.parents[2] / "apps" / "osl-hub" / "src" / "main.rs").read_text(encoding="utf-8")
PYTHON_PATH = ROOT / "whatsapp-deploy-orchestrator.py"
PYTHON = PYTHON_PATH.read_text(encoding="utf-8")
SPEC = importlib.util.spec_from_file_location("whatsapp_deploy", PYTHON_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class WhatsAppDeployPowerShellTests(unittest.TestCase):
    def test_session_discovery_is_metadata_only_and_fails_closed_on_ambiguity(self) -> None:
        self.assertIn("$sessionIds.Count -ne 1", DISCOVER)
        self.assertIn("ProcessesTerminated = $false", DISCOVER)
        self.assertIn("ProfileRead = $false", DISCOVER)
        self.assertIn("ProviderStorageRead = $false", DISCOVER)
        self.assertNotRegex(DISCOVER, r"Get-Content|LocalState|SetForegroundWindow|Stop-Process")

    def test_runtime_receipt_is_fixed_content_free_and_fail_closed(self) -> None:
        self.assertIn('"protectedControlsEnabled": false', MAIN_RS)
        self.assertIn('"browserFallbackUsed": false', MAIN_RS)
        self.assertIn('"providerContentRead": false', MAIN_RS)
        self.assertIn('"providerPrivateStorageRead": false', MAIN_RS)
        self.assertIn('"nativeWindowClaimed"', MAIN_RS)
        self.assertIn('state::<WhatsAppQaHostState>().detach()', MAIN_RS)
        self.assertIn("$receipt.phase -notin @('coreReady','nativeWindowClaimed','failedClosed')", RUNTIME_AUDIT)
        self.assertIn("WhatsAppPrivateStorageRead=$false", RUNTIME_AUDIT)
        self.assertIn("metadata/identity/oauth2/token", RUNTIME_AUDIT)
        self.assertNotRegex(RUNTIME_AUDIT, r"SetForegroundWindow|SendKeys|LocalState|Packages\\5319275A")

    def test_exact_scope_and_managed_identity_only(self) -> None:
        self.assertIn("C:\\Users\\osltest\\Desktop\\OSL Privacy\\OSL Privacy.exe", POWERSHELL)
        self.assertIn("osltestartifactsa7d5.blob.core.windows.net", POWERSHELL)
        self.assertIn("metadata/identity/oauth2/token", POWERSHELL)
        self.assertIn("AuthenticationHeaderValue]::new('Bearer'", POWERSHELL)
        self.assertIn("5319275A.WhatsAppDesktop_cv1g1gvanyjgm", POWERSHELL)

    def test_hashes_before_atomic_replace_and_rolls_back(self) -> None:
        first_replace = POWERSHELL.index("[IO.File]::Replace($exeStage")
        self.assertLess(POWERSHELL.index("Save-ManagedIdentityArtifact $ExeUri"), first_replace)
        self.assertLess(POWERSHELL.index("Save-ManagedIdentityArtifact $WebView2LoaderUri"), first_replace)
        self.assertIn("downloaded artifact hash mismatch", POWERSHELL)
        self.assertIn("[IO.File]::Replace($loaderBackup,$loaderPath", POWERSHELL)
        self.assertIn("[IO.File]::Replace($exeBackup,$oslPath", POWERSHELL)
        self.assertIn("Status='failedClosed'", POWERSHELL)
        self.assertIn("FailureStage=$failureStage", POWERSHELL)
        self.assertIn("RollbackVerified=$true", POWERSHELL)
        catch = POWERSHELL[POWERSHELL.index("} catch {") :]
        self.assertLess(catch.index("Stop-Process"), catch.index("[IO.File]::Replace($exeBackup"))

    def test_never_touches_whatsapp_private_state_or_unrelated_processes(self) -> None:
        stop_lines = "\n".join(line for line in POWERSHELL.splitlines() if "Stop-Process" in line)
        self.assertIn("$primaryBefore[0].ProcessId", stop_lines)
        self.assertNotRegex(stop_lines, r"WhatsApp|explorer|taskkill")
        for forbidden in ("AppData", "LocalState", "Packages\\5319275A", "Get-Content", "Remove-AppxPackage"):
            self.assertNotIn(forbidden, POWERSHELL)
        self.assertIn("WhatsAppPrivateStorageRead=$false", POWERSHELL)
        self.assertIn("WhatsAppProcessTerminated=$false", POWERSHELL)

    def test_preserves_exact_process_and_window_state_without_foregrounding(self) -> None:
        self.assertNotRegex(POWERSHELL, r"(?i)\[(?:u?int32)\]\$pid\b")
        self.assertIn("Get-WhatsAppProcessSnapshot", POWERSHELL)
        self.assertIn("Get-WhatsAppWindowSnapshot", POWERSHELL)
        self.assertIn("Test-ExactSnapshot $whatsAppBefore $whatsAppAfter", POWERSHELL)
        self.assertIn("Test-ExactSnapshot $windowsBefore $windowsAfter", POWERSHELL)
        self.assertNotRegex(POWERSHELL, r"SetForegroundWindow|ShowWindow|SetWindowPos")

    def test_limited_interactive_launch_and_repeatability(self) -> None:
        self.assertIn("LogonType Interactive", POWERSHELL)
        self.assertIn("RunLevel Limited", POWERSHELL)
        self.assertIn("alreadyInstalledPreserved", POWERSHELL)
        self.assertIn("-not $initialInstall -and $primaryBefore.Count -gt 1", POWERSHELL)
        self.assertIn("if($primaryBefore.Count -eq 0) { [void](Start-ExactOsl", POWERSHELL)
        self.assertIn("Remove-Item -LiteralPath $exeBackup,$loaderBackup", POWERSHELL)
        self.assertIn("Unregister-ScheduledTask", POWERSHELL)

    def test_disconnected_rdp_fallback_uses_exact_session_process_owners(self) -> None:
        self.assertIn("$explorers.Count -gt 1", POWERSHELL)
        self.assertIn("Name = 'WhatsApp.Root.exe'", POWERSHELL)
        self.assertIn("Get-ExactOslPrimary", POWERSHELL)
        self.assertIn("$ownerIdentities.Count -ne 1", POWERSHELL)
        self.assertIn("$candidateOwner.User -cne 'osltest'", POWERSHELL)


class WhatsAppDeployControllerTests(unittest.TestCase):
    def test_pair_allowlist_and_runcommand_only(self) -> None:
        self.assertIn('("OSL-WhatsApp-Client-1", "OSL-WhatsApp-Client-2")', PYTHON)
        for operation in ('"list"', '"create"', '"update"', '"show"'):
            self.assertIn(operation, PYTHON)
        self.assertNotIn('"invoke"', PYTHON)
        self.assertIn('"--expand", "instanceView"', PYTHON)
        self.assertNotRegex(PYTHON, r'"az",\s*"(ssh|vm restart|vm start|storage)"')

    def test_one_command_requires_the_content_free_runtime_audit(self) -> None:
        self.assertIn("RUNTIME_AUDIT", PYTHON)
        self.assertIn('audit.get("Phase") != "nativeWindowClaimed"', PYTHON)
        self.assertIn('audit.get("NativeWindowClaimed") is not True', PYTHON)
        for field in ("ProtectedControlsEnabled", "BrowserFallbackUsed", "ProviderContentRead", "WhatsAppPrivateStorageRead"):
            self.assertIn(field, PYTHON)

    def test_rejects_sas_or_wrong_host(self) -> None:
        with self.assertRaises(ValueError):
            MODULE._validate_uri("https://evil.example/qa/OSL%20Privacy.exe", "OSL%20Privacy.exe")
        with self.assertRaises(ValueError):
            MODULE._validate_uri(
                "https://osltestartifactsa7d5.blob.core.windows.net/qa/OSL%20Privacy.exe?sig=secret",
                "OSL%20Privacy.exe",
            )

    @mock.patch.object(subprocess, "run")
    def test_command_cannot_target_another_vm(self, run: mock.Mock) -> None:
        with self.assertRaises(ValueError):
            MODULE._run_command("OSL-Azure-Client-1", PYTHON_PATH, {})
        run.assert_not_called()

    def test_semantic_receipt_omits_urls_and_secrets(self) -> None:
        self.assertIn('"artifactHashes"', PYTHON)
        self.assertNotIn('"artifactUris"', PYTHON)
        self.assertIn('"failureCode": "pair-deployment-failed"', PYTHON)
        self.assertIn("details redacted", PYTHON)


if __name__ == "__main__":
    unittest.main()
