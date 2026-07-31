from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "qa" / "osl-instance-b-build.ps1"
TAURI_CONFIG = ROOT / "apps" / "osl-hub" / "tauri.conf.json"
BUILD_ORDER = ROOT / "docs" / "design" / "build-order.md"


class OslInstanceBBuildStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.script = SCRIPT.read_text(encoding="utf-8")
        cls.config = TAURI_CONFIG.read_text(encoding="utf-8")

    def test_b22_script_exists_for_documented_instance_b_build(self) -> None:
        self.assertTrue(SCRIPT.is_file())
        self.assertIn("tool = 'osl-instance-b-build'", self.script)
        self.assertIn("org.oslprivacy.hubqab", self.script)
        self.assertIn("C:\\OSL-QA-B", self.script)

    def test_b22_refuses_same_or_unsafe_identifier_before_build(self) -> None:
        same_id_gate = self.script.index("if ($Identifier -eq $BundleA)")
        shape_gate = self.script.index("if ($Identifier -cnotmatch")
        build_call = self.script.index("& cargo @buildArgs")
        self.assertLess(same_id_gate, build_call)
        self.assertLess(shape_gate, build_call)
        self.assertRegex(self.script, r"Write-Result 'blocked'[\s\S]+equals instance A")
        self.assertRegex(self.script, r"\^\[A-Za-z0-9\]\[A-Za-z0-9\.\-\]\{2,95\}\$")

    def test_b22_uses_tauri_build_config_overlay_without_editing_tracked_config(self) -> None:
        self.assertIn("tauri', 'build'", self.script)
        self.assertIn("'--features', 'desktop,discord-qa-shell'", self.script)
        self.assertIn("'--config', $overlayPath", self.script)
        self.assertIn("[ordered]@{ identifier = $Identifier }", self.script)
        tauri_conf_writes = re.findall(
            r"(?:Out-File|Set-Content|Add-Content)\s+-LiteralPath\s+\$ConfigPath",
            self.script,
        )
        self.assertEqual(tauri_conf_writes, [])
        self.assertIn('"identifier": "org.oslprivacy.hub"', self.config)

    def test_b22_bad_repo_root_is_a_json_refusal_not_an_uncaught_resolve(self) -> None:
        repo_gate = self.script.index("if (-not (Test-Path -LiteralPath $RepoRoot")
        resolve = self.script.index("$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path")
        self.assertLess(repo_gate, resolve)
        self.assertRegex(self.script, r"Write-Result 'blocked' 'RepoRoot was not found.'")

    def test_b22_requires_fresh_dist_and_never_builds_renderer_or_launches_b(self) -> None:
        self.assertIn("apps\\osl-hub-ui\\dist", self.script)
        self.assertIn("A non-test frontend source is newer than dist", self.script)
        self.assertNotRegex(self.script, r"npm\s+(?:run\s+)?build")
        self.assertNotRegex(self.script, r"Start-Process\s+-FilePath\s+\$stagedExe")
        self.assertNotRegex(self.script, r"Start-Process\s+-FilePath\s+\$builtExe")

    def test_b22_proves_embedded_identifier_and_stages_loader(self) -> None:
        self.assertIn("function Test-BinaryContainsAscii", self.script)
        self.assertIn("Test-BinaryContainsAscii -Path $builtExe -Needle $Identifier", self.script)
        self.assertIn("WebView2Loader.dll", self.script)
        self.assertIn("Copy-Item -LiteralPath $builtExe -Destination $stagedExe -Force", self.script)
        self.assertIn("Copy-Item -LiteralPath $LoaderDll -Destination $stagedLoader -Force", self.script)

    def test_b22_json_verdict_has_acceptance_contract_fields(self) -> None:
        for expected in (
            "schemaVersion = 1",
            "overall = [ordered]@",
            "exeSha256 = $exeSha",
            "webview2LoaderSha256 = $loaderSha",
            "distinctFromA = [bool]($aSha -and $aSha -ne $exeSha)",
            "osl-launch-instance-b.ps1",
        ):
            self.assertIn(expected, self.script)

    def test_b22_does_not_surface_credentials_or_account_fields(self) -> None:
        forbidden = (
            "accountId",
            "accountIdentifier",
            "authorization",
            "credential",
            "password",
            "secret",
            "token",
        )
        lowered = self.script.lower()
        for word in forbidden:
            self.assertNotIn(word.lower(), lowered)


def _bash_blocks(markdown: str) -> list[str]:
    return re.findall(r"```bash\n(.*?)\n```", markdown, flags=re.DOTALL)


def frontend_dist_is_embedded_after_frontend_build() -> None:
    markdown = BUILD_ORDER.read_text(encoding="utf-8")
    blocks = _bash_blocks(markdown)
    contract = next(
        (
            block
            for block in blocks
            if re.search(
                r"^frontend_dist_is_embedded_after_frontend_build\(\)",
                block,
                flags=re.MULTILINE,
            )
        ),
        None,
    )
    testcase = unittest.TestCase()
    testcase.assertIsNotNone(contract, "build-order doc has no executable contract")
    assert contract is not None
    testcase.assertIn('local repo_root="${1:-$(git rev-parse --show-toplevel)}"', contract)
    testcase.assertIn('cd "$repo_root"', contract)
    testcase.assertIn("bash scripts/qa/osl-instance-b-build-wsl.sh --self-test", contract)
    testcase.assertIn('frontend_dist_is_embedded_after_frontend_build "$@"', contract)
    testcase.assertNotRegex(contract, r"\b(?:cargo|npm|pnpm|yarn)\b")


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del tests
    del pattern
    suite = unittest.TestSuite()
    suite.addTests(loader.loadTestsFromTestCase(OslInstanceBBuildStaticTests))
    suite.addTest(unittest.FunctionTestCase(frontend_dist_is_embedded_after_frontend_build))
    return suite


if __name__ == "__main__":
    unittest.main()
