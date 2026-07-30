from __future__ import annotations

import ast
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent


class WhatsAppBootstrapStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.arm = (ROOT / "arm-whatsapp-bootstrap.ps1").read_text(encoding="utf-8")
        cls.poll = (ROOT / "poll-whatsapp-bootstrap.ps1").read_text(encoding="utf-8")
        cls.discover = (ROOT / "discover-whatsapp-session.ps1").read_text(encoding="utf-8")
        cls.live_audit = (ROOT / "audit-whatsapp-live-session.ps1").read_text(encoding="utf-8")
        cls.orchestrator = (ROOT / "whatsapp-bootstrap-orchestrator.py").read_text(encoding="utf-8")

    def test_only_exact_store_product_and_package_identity_are_allowed(self) -> None:
        self.assertIn("$productId = '9NKSQGP7F2NH'", self.arm)
        self.assertIn("$packageFamily = '5319275A.WhatsAppDesktop_cv1g1gvanyjgm'", self.arm)
        self.assertRegex(self.arm, r"--id',\s*\$productId")
        self.assertRegex(self.arm, r"--source',\s*'msstore'")
        self.assertIn("--exact", self.arm)
        self.assertIn("PackageTypeFilter Main", self.arm)
        self.assertIn("unexpected WhatsApp package registration exists", self.arm)
        self.assertIn("package registration is ambiguous", self.arm)

    def test_arm_requires_exact_osltest_interactive_limited_session(self) -> None:
        self.assertRegex(self.arm, r"candidateOwner[.]User\s+-ceq\s+'osltest'")
        self.assertRegex(self.arm, r"osltestExplorers[.]Count\s+-ne\s+1")
        self.assertRegex(self.arm, r"Process[.]SessionId\s+-ne\s+\$SessionId")
        self.assertRegex(self.arm, r"LogonType\s+Interactive")
        self.assertRegex(self.arm, r"RunLevel\s+Limited")
        self.assertNotRegex(self.arm, r"RunLevel\s+Highest")
        self.assertIn("$existing.WrapperSha256 -cne $existingWrapperSha256", self.arm)
        self.assertIn("$currentIdentity -cne $request.InteractiveUser", self.arm)
        self.assertIn("$currentSessionId -ne [int]$request.SessionId", self.arm)
        self.assertIn("$actualWrapperSha256 -cne $request.WrapperSha256", self.arm)

    def test_install_is_idempotent_and_never_launches_or_uninstalls(self) -> None:
        self.assertRegex(self.arm, r"if \(-not \$package\) \{")
        self.assertIn("InstalledBefore = $installedBefore", self.arm)
        self.assertIn("InstallAttempted = $installAttempted", self.arm)
        forbidden = (
            r"Start-Process[^\n]+WhatsApp",
            r"explorer[.]exe[^\n]+shell:AppsFolder",
            r"Remove-AppxPackage",
            r"Reset-AppxPackage",
            r"Stop-Process",
            r"taskkill",
            r"Get-ChildItem[^\n]+WhatsApp",
            r"AppData[^\n]+WhatsApp",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.arm, rf"(?i){pattern}")

    def test_result_is_atomic_bounded_and_truthful(self) -> None:
        for field in ("AppLaunched = $false", "ProfileRead = $false", "LinkStateTouched = $false", "ProcessesTerminated = 0"):
            self.assertIn(field, self.arm)
        self.assertRegex(self.arm, r"WriteAllText\(\$resultTemporary")
        self.assertRegex(self.arm, r"Move\(\$resultTemporary,\s*\$request[.]ResultPath\)")
        self.assertIn("Get-FileHash -LiteralPath $wrapperPath -Algorithm SHA256", self.arm)
        self.assertIn("WrapperSha256 = $request.WrapperSha256", self.arm)
        self.assertNotIn("ExceptionMessage", self.arm)
        self.assertNotRegex(self.arm, r"(?i)(phone|message content|database|token|credential)")

    def test_poll_is_read_only_and_revalidates_terminal_contract(self) -> None:
        for fixed in ("9NKSQGP7F2NH", "5319275A.WhatsAppDesktop_cv1g1gvanyjgm", "AppLaunched", "ProfileRead", "LinkStateTouched", "ProcessesTerminated"):
            self.assertIn(fixed, self.poll)
        self.assertIn("runnerExitedWithoutResult", self.poll)
        self.assertIn("$result.WrapperSha256 -cne $request.WrapperSha256", self.poll)
        for mutation in ("Start-ScheduledTask", "Register-ScheduledTask", "Unregister-ScheduledTask", "Remove-Item", "WriteAllText", "Set-Content"):
            self.assertNotIn(mutation, self.poll)

    def test_orchestrator_is_pair_scoped_and_runcommand_only(self) -> None:
        ast.parse(self.orchestrator)
        self.assertIn('"OSL-WhatsApp-Client-1"', self.orchestrator)
        self.assertIn('"OSL-WhatsApp-Client-2"', self.orchestrator)
        self.assertIn('"vm", "run-command", "invoke"', self.orchestrator)
        for forbidden in ('"vm", "start"', '"vm", "restart"', '"vm", "deallocate"', "keyvault secret show"):
            self.assertNotIn(forbidden, self.orchestrator)
        self.assertIn('receipt.get("Status") == "verified"', self.orchestrator)
        self.assertIn("return 3", self.orchestrator)

    def test_session_discovery_is_exact_and_read_only(self) -> None:
        self.assertIn("$sessionIds.Count -ne 1", self.discover)
        self.assertIn("$owner.User -ceq 'osltest'", self.discover)
        self.assertIn("WhatsApp.Root.exe", self.discover)
        self.assertIn("OSL Privacy.exe", self.discover)
        self.assertIn("ProviderStorageRead = $false", self.discover)
        self.assertIn("DISCOVER", self.orchestrator)
        for mutation in ("Start-Process", "Stop-Process", "Register-ScheduledTask", "Set-Content", "Remove-Item"):
            self.assertNotIn(mutation, self.discover)

    def test_WhatsAppTwoClientQualification(self) -> None:
        self.assertIn("function WhatsAppTwoClientQualification", self.live_audit)
        self.assertRegex(self.live_audit, r"\[ValidateSet\(1,\s*2\)\]\[int\]\$ClientNumber")
        self.assertIn("'OSL-WhatsApp-Client-1', 'OSL-WhatsApp-Client-2'", self.live_audit)
        self.assertIn("whatsapp-two-client-qualification/v1", self.live_audit)
        self.assertIn("$explorerSessions.Count -eq 1", self.live_audit)
        self.assertIn("$oslSessions.Count -ge 1", self.live_audit)
        self.assertIn("$whatsAppSessions.Count -ge 1", self.live_audit)
        self.assertIn("$sharedSessions.Count -eq 1", self.live_audit)
        for field in ("ProfileRead = $false", "ProviderStorageRead = $false", "ContentRead = $false", "WindowForegrounded = $false", "ProcessesTerminated = $false"):
            self.assertIn(field, self.live_audit)
        for forbidden in ("Get-Content", "LocalState", "SetForegroundWindow", "Start-Process", "Stop-Process", "Set-Content", "Remove-Item"):
            self.assertNotIn(forbidden, self.live_audit)


if __name__ == "__main__":
    unittest.main()
