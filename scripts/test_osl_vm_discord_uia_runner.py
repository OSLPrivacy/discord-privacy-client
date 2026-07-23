from __future__ import annotations

import re
import unittest
from pathlib import Path


QA = Path(__file__).parent / "qa"
ARM = QA / "osl-vm-discord-uia-arm.ps1"
POLL = QA / "osl-vm-discord-uia-poll.ps1"


class OslVmDiscordUiaRunnerStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.arm = ARM.read_text(encoding="utf-8")
        cls.poll = POLL.read_text(encoding="utf-8")

    def test_arm_uses_exact_bounded_invocation_identity(self) -> None:
        self.assertRegex(self.arm, r"\$InvocationId\s+-cnotmatch")
        self.assertRegex(self.arm, r"Join-Path\s+\$root\s+\$InvocationId")
        self.assertRegex(self.arm, r"invocation ID already exists")
        self.assertRegex(self.arm, r"\$HarnessSha256\s+-cnotmatch")
        self.assertRegex(self.arm, r"Get-FileHash[^\r\n]+SHA256")
        self.assertRegex(
            self.arm,
            r"Get-FileHash[^\r\n]+\.Hash\.ToLowerInvariant\(\)",
        )
        self.assertRegex(self.arm, r"\$HarnessSha256\.ToLowerInvariant\(\)")
        self.assertRegex(self.arm, r"allowedHarnessHost")

    def test_arm_returns_after_scheduling_not_after_harness_completion(self) -> None:
        scheduling = self.arm[self.arm.index("Register-ScheduledTask") :]
        self.assertRegex(scheduling, r"Start-ScheduledTask")
        self.assertRegex(scheduling, r"Status\s*=\s*'armed'")
        self.assertNotRegex(scheduling, r"(?i)while\s*\(|do\s*\{|Start-Sleep|Wait-")
        self.assertNotRegex(scheduling, r"Get-Content[^\r\n]+result")

    def test_scheduled_task_is_exact_interactive_limited_user(self) -> None:
        self.assertRegex(self.arm, r"owner\.User\s+-cne\s+'osltest'")
        self.assertRegex(self.arm, r"LogonType\s+Interactive")
        self.assertNotRegex(self.arm, r"LogonType\s+InteractiveToken")
        self.assertRegex(self.arm, r"RunLevel\s+Limited")
        self.assertRegex(self.arm, r"SessionId\s+-eq\s+\$SessionId")

    def test_interactive_wrapper_writes_temp_then_atomic_final(self) -> None:
        self.assertRegex(self.arm, r"resultTemporary")
        self.assertRegex(self.arm, r"WriteAllText\(`\$resultTemporary")
        self.assertRegex(self.arm, r"\[IO\.File\]::Move\(`\$resultTemporary,\s*`\$request\.ResultPath\)")
        self.assertNotRegex(self.arm, r"(?i)Remove-Item[^\r\n]*(?:result|invocationRoot)")

    def test_success_does_not_read_an_unset_last_exit_code_in_strict_mode(self) -> None:
        self.assertRegex(self.arm, r"Test-Path\s+Variable:LASTEXITCODE")
        self.assertNotRegex(self.arm, r"\$null\s+-eq\s+`\$LASTEXITCODE")

    def test_poll_reads_only_exact_request_task_and_result(self) -> None:
        self.assertRegex(self.poll, r"\$InvocationId\s+-cnotmatch")
        self.assertRegex(self.poll, r"Import-Clixml\s+-LiteralPath\s+\$requestPath")
        self.assertRegex(self.poll, r"Get-ScheduledTask\s+-TaskName\s+\$taskName")
        self.assertRegex(self.poll, r"Get-ScheduledTaskInfo\s+-TaskName\s+\$taskName")
        self.assertRegex(self.poll, r"Get-Content\s+-LiteralPath\s+\$resultPath\s+-Raw")
        self.assertRegex(self.poll, r"result identity mismatch")

    def test_poll_is_strictly_non_mutating(self) -> None:
        forbidden = (
            r"Remove-Item",
            r"Unregister-ScheduledTask",
            r"Start-ScheduledTask",
            r"Stop-ScheduledTask",
            r"Set-Content",
            r"WriteAllText",
            r"\.Move\(",
            r"\.Delete\(",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.poll, rf"(?i){pattern}")

    def test_poll_does_not_treat_temporary_result_as_success(self) -> None:
        terminal_branch = self.poll[
            self.poll.index("if (Test-Path -LiteralPath $resultPath") :
        ]
        self.assertRegex(terminal_branch, r"-not\s+\$result\.Terminal")
        self.assertRegex(terminal_branch, r"ResultTemporaryPresent")
        self.assertNotRegex(
            self.poll,
            r"if\s*\(Test-Path[^\r\n]+\$resultTemporary[^)]*\)\s*\{[^}]*Terminal\s*=\s*\$true",
        )


if __name__ == "__main__":
    unittest.main()
