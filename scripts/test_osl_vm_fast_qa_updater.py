from __future__ import annotations

import re
import unittest
from pathlib import Path


QA = Path(__file__).parent / "qa"
UPDATER = QA / "osl-vm-fast-qa-updater.ps1"
BOOTSTRAP = QA / "osl-vm-fast-qa-updater-bootstrap.ps1"
MANUAL = QA / "osl-vm-fast-qa-manual-install.ps1"


def mapping_block(script: str, following: str) -> str:
    start = script.index("function Get-ClosedClientMapping")
    end = script.index(following, start)
    return script[start:end]


class OslVmFastQaUpdaterStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.updater = UPDATER.read_text(encoding="utf-8")
        cls.bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
        cls.manual = MANUAL.read_text(encoding="utf-8")
        cls.all_code = "\n".join((cls.updater, cls.bootstrap, cls.manual))

    def test_one_byte_identical_implementation_has_no_client_variants(self) -> None:
        self.assertTrue(UPDATER.is_file())
        self.assertTrue(BOOTSTRAP.is_file())
        self.assertTrue(MANUAL.is_file())
        for forbidden in (
            QA / "osl-vm-fast-qa-updater-c1.ps1",
            QA / "osl-vm-fast-qa-updater-c2.ps1",
            QA / "osl-vm-fast-qa-updater-bootstrap-c1.ps1",
            QA / "osl-vm-fast-qa-updater-bootstrap-c2.ps1",
        ):
            self.assertFalse(forbidden.exists())
        updater_param = self.updater[: self.updater.index(")\n\n$ErrorActionPreference")]
        bootstrap_param = self.bootstrap[
            : self.bootstrap.index(")\n\n$ErrorActionPreference")
        ]
        self.assertNotRegex(updater_param, r"(?i)ClientId")
        self.assertNotRegex(bootstrap_param, r"(?i)ClientId")
        self.assertRegex(self.bootstrap, r"\[IO\.File\]::Copy\(.+UpdaterSourcePath")
        self.assertNotRegex(self.bootstrap, r"(?i)(?:replace|rewrite|substitut).+updater")

    def test_all_three_scripts_have_only_two_exact_imds_mappings(self) -> None:
        blocks = (
            mapping_block(self.updater, "\nfunction Get-ValidatedClientConfig"),
            mapping_block(self.bootstrap, "\n$localVmName ="),
            mapping_block(self.manual, "\nfunction Get-Sha256"),
        )
        for block in blocks:
            self.assertEqual(block.count("'OSL-Azure-Client-1'"), 2)
            self.assertEqual(block.count("'OSL-Azure-Client-2'"), 2)
            self.assertIn("osl-discord-qa-client1", block)
            self.assertIn("osl-discord-qa-client2", block)
            self.assertRegex(block, r"default\s*\{\s*throw")
        for script in (self.updater, self.bootstrap, self.manual):
            self.assertRegex(script, r"metadata/instance/compute/name")
            self.assertRegex(script, r"Get-ClosedClientMapping")

    def test_updater_mapping_selects_all_private_runtime_names(self) -> None:
        block = mapping_block(
            self.updater, "\nfunction Get-ValidatedClientConfig"
        )
        for expected in (
            "manifests/c1-fast-qa.json",
            "manifests/c2-fast-qa.json",
            "OSL-QA-FastUpdater-Launch-C1",
            "OSL-QA-FastUpdater-Launch-C2",
            "OSL-QA-FastUpdater-Poller-C1",
            "OSL-QA-FastUpdater-Poller-C2",
            "results/fast-updater/c1/",
            "results/fast-updater/c2/",
        ):
            self.assertIn(expected, block)
        outside = self.updater.replace(block, "")
        for stale in (
            "OSL-Azure-Client-1",
            "OSL-Azure-Client-2",
            "osl-discord-qa-client1",
            "osl-discord-qa-client2",
            "manifests/c1-fast-qa.json",
            "manifests/c2-fast-qa.json",
            "results/fast-updater/c1/",
            "results/fast-updater/c2/",
            "Launch-C1",
            "Launch-C2",
        ):
            self.assertNotIn(stale, outside)

    def test_bootstrap_checks_imds_then_writes_exact_protected_config(self) -> None:
        self.assertLess(
            self.bootstrap.index("$localVmName = Get-LocalVmName"),
            self.bootstrap.index("[void](New-Item -ItemType Directory -Path $root"),
        )
        self.assertRegex(self.bootstrap, r"\$configJson\s*=\s*\$clientConfig")
        self.assertRegex(self.bootstrap, r"WriteAllText\(\$configTemporary")
        self.assertRegex(self.bootstrap, r"\[IO\.File\]::Move\(\$configTemporary,\s*\$configPath\)")
        self.assertIn("S-1-5-18", self.bootstrap)
        self.assertIn("S-1-5-32-544", self.bootstrap)
        self.assertRegex(self.bootstrap, r"SetAccessRuleProtection\(\$true,\s*\$false\)")
        self.assertLess(
            self.bootstrap.index("Set-Acl -LiteralPath $root"),
            self.bootstrap.index("WriteAllText($configTemporary"),
        )

    def test_updater_strictly_validates_config_against_closed_mapping(self) -> None:
        self.assertRegex(self.updater, r"protected updater config shape is invalid")
        self.assertRegex(self.updater, r"Compare-Object")
        self.assertRegex(
            self.updater,
            r"Get-ClosedClientMapping \(\[string\]\$config\.targetMachine\)",
        )
        self.assertRegex(self.updater, r"config mapping mismatch")
        self.assertRegex(self.updater, r"Get-LocalVmName\) -cne \$targetMachine")
        for fixed in (
            "$storageAccount = 'osltestartifactsa7d5'",
            "$targetSessionId = 2",
            "$interactiveUserName = 'osltest'",
            "$oslExePath = 'C:\\Users\\osltest\\Desktop\\OSL Privacy\\OSL Privacy.exe'",
        ):
            self.assertIn(fixed, self.updater)

    def test_manifest_contract_remains_exact_and_monotonic(self) -> None:
        for field in (
            "schemaVersion",
            "targetMachine",
            "sessionId",
            "generation",
            "invocationId",
            "expectedCurrent.exeSha256",
            "expectedCurrent.loaderSha256",
            "desired.exe.sha256",
            "desired.loader.sha256",
            "desired.exe.size",
            "desired.loader.size",
        ):
            self.assertIn(field, self.updater)
        self.assertRegex(self.updater, r"manifest generation is not monotonic")
        self.assertRegex(self.updater, r"\^\[0-9a-f\]\{64\}\$")
        self.assertRegex(
            self.updater,
            r"installed bytes do not match manifest expected-current hashes",
        )

    def test_zero_or_one_old_osl_and_empty_or_exact_discord_are_allowed(self) -> None:
        self.assertRegex(self.updater, r"oldProcesses\.Count\s+-gt\s+1")
        self.assertNotRegex(self.updater, r"oldProcesses\.Count\s+-ne\s+1")
        self.assertRegex(
            self.updater, r"if \(\$oldProcesses\.Count -eq 1\) \{ \$oldOsl"
        )
        self.assertRegex(
            self.updater, r"if \(\$null -ne \$oldOsl\) \{ Stop-ExactOsl"
        )
        self.assertRegex(self.updater, r"Fingerprint\.Count -eq 0\) \{ return '\[\]'")
        self.assertNotRegex(
            self.updater,
            r"discordBefore\.Count\s*-(?:eq|le|lt)\s*[01]",
        )
        self.assertNotIn(
            "at least one exact Discord Stable, PTB, or Canary process is required",
            self.updater,
        )
        self.assertRegex(self.updater, r"Discord process fingerprint is incomplete")
        self.assertIn(
            "@('Discord.exe', 'DiscordPTB.exe', 'DiscordCanary.exe')",
            self.updater,
        )
        for field in ("Name", "Pid", "Path", "SessionId", "Start"):
            self.assertRegex(self.updater, rf"\b{field}\s*=")
        self.assertRegex(self.updater, r"Discord fingerprint changed during OSL-only update")

    def test_stage_journal_swap_rollback_relaunch_and_terminal_are_preserved(self) -> None:
        self.assertLess(
            self.updater.index("Save-Artifact ([string]$Manifest.desired.exe.blobName"),
            self.updater.index("if ($null -ne $oldOsl) { Stop-ExactOsl"),
        )
        self.assertLess(
            self.updater.index("[IO.File]::Replace($loaderStage"),
            self.updater.index("[IO.File]::Replace($exeStage"),
        )
        for phase in (
            "staged",
            "stopped",
            "loaderReplaced",
            "exeReplaced",
            "launched",
            "committed",
        ):
            self.assertIn(f"'{phase}'", self.updater)
        rollback = self.updater[
            self.updater.index("function Invoke-Rollback") :
            self.updater.index("function Get-ResultIdentity")
        ]
        self.assertRegex(rollback, r"Stop-ExactOsl")
        self.assertRegex(rollback, r"Restore-Backup")
        self.assertRegex(rollback, r"Start-ExactOsl")
        self.assertRegex(self.updater, r"function Recover-IncompleteTransaction")
        self.assertRegex(self.updater, r"lastTerminalEtag")
        self.assertRegex(self.updater, r"lastResultPath")

    def test_launcher_is_fixed_dynamic_domain_interactive_limited(self) -> None:
        self.assertRegex(self.bootstrap, r"Invoke-CimMethod.+GetOwner")
        self.assertRegex(self.bootstrap, r'"\$\(\$owner\.Domain\)\\\$\(\$owner\.User\)"')
        self.assertRegex(self.bootstrap, r"LogonType Interactive")
        self.assertRegex(self.bootstrap, r"RunLevel Limited")
        self.assertRegex(self.bootstrap, r"ExecutionTimeLimit \(\[TimeSpan\]::Zero\)")
        self.assertRegex(self.updater, r"function Assert-LauncherTask")
        self.assertRegex(self.updater, r"Start-ScheduledTask -TaskName \$launcherTaskName")

    def test_manual_installer_is_universal_hash_pinned_and_proves_build(self) -> None:
        self.assertIn("$updaterBlobName = 'tools/fast-updater/osl-vm-fast-qa-updater.ps1'", self.manual)
        self.assertIn(
            "$bootstrapBlobName = 'tools/fast-updater/osl-vm-fast-qa-updater-bootstrap.ps1'",
            self.manual,
        )
        self.assertRegex(self.manual, r"managed identity storage token unavailable")
        self.assertRegex(self.manual, r"universal script response size mismatch")
        self.assertRegex(self.manual, r"universal script hash mismatch")
        self.assertRegex(self.manual, r"-DeferPollerStart")
        self.assertIn(
            "$qaExeSha256 = '2065d3329f9f6999c3e51e149634c6dab660a03a5cda86cfa146465ead28410f'",
            self.manual,
        )
        self.assertIn("$qaExeSize = 43865117", self.manual)
        self.assertIn(
            "$qaLoaderSha256 = '8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c'",
            self.manual,
        )
        self.assertIn("$qaLoaderSize = 160320", self.manual)
        self.assertRegex(self.manual, r"expectedCurrent")
        self.assertRegex(self.manual, r"generation = 1")
        self.assertRegex(
            self.manual,
            r"& \$residentUpdater -RunOnce -LocalBootstrapManifestPath \$bootstrapManifestPath",
        )
        self.assertRegex(self.manual, r"installedPreservedAndLaunched")
        self.assertRegex(self.manual, r"discordUnchanged")
        self.assertRegex(self.manual, r"Start-ScheduledTask -TaskName")
        self.assertRegex(self.manual, r"Status = 'installed-and-proven'")

    def test_local_bootstrap_manifest_is_exact_run_once_only_and_terminal_consumed(self) -> None:
        self.assertIn(
            "$bootstrapManifestPath = Join-Path $root 'bootstrap-manifest.json'",
            self.updater,
        )
        self.assertRegex(self.updater, r"accepted only with RunOnce")
        self.assertRegex(self.updater, r"exact protected canonical path")
        self.assertRegex(self.updater, r"ReparsePoint")
        self.assertRegex(self.updater, r"Get-BytesSha256 \$bootstrapBytes")
        self.assertRegex(
            self.updater,
            r'\$bootstrapIdentity = "local-bootstrap-\$bootstrapSha256"',
        )
        self.assertRegex(
            self.updater,
            r"ConvertTo-ValidatedManifest \$bootstrapBytes",
        )
        terminal_index = self.updater.index(
            "$terminalState = Get-State"
        )
        delete_index = self.updater.index(
            "Remove-Item -LiteralPath $bootstrapManifestPath",
            terminal_index,
        )
        self.assertLess(terminal_index, delete_index)

    def test_no_provider_profile_restart_or_foreground_surface(self) -> None:
        forbidden = (
            r"UIAutomation",
            r"SendInput",
            r"mouse_event",
            r"keybd_event",
            r"SetForegroundWindow",
            r"ShowWindow",
            r"AppData",
            r"discord-qa-.*receipt",
            r"Stop-Process[^\r\n]+Discord",
            r"Remove-Item[^\r\n]+Discord",
            r"taskkill",
            r"shutdown",
            r"Restart-",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.all_code, rf"(?i){pattern}")


if __name__ == "__main__":
    unittest.main()
