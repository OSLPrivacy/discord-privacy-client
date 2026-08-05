#!/usr/bin/env python3
"""T8-T20: state-authority registry fixtures and sabotage proofs."""
from __future__ import annotations

import importlib.util
import io
import contextlib
import shutil
import subprocess
import tempfile
import unittest
import unittest.mock
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts/fixtures/state-authority/valid-registry.md"
SPEC = importlib.util.spec_from_file_location("state_authority", ROOT / "scripts/check-state-authority.py")
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class StateAuthorityT8T20(unittest.TestCase):
    def copied_registry(self) -> Path:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        target = directory / "registry.md"
        shutil.copy(FIXTURE, target)
        return target

    def test_fixture_passes_complete_native_renderer_website_contract(self) -> None:
        records = CHECKER.load_records(FIXTURE)
        self.assertEqual(CHECKER.validate(records), [])
        self.assertEqual({record["kind"] for record in records}, {"flag", "worker"})

    def test_second_writer_is_refused(self) -> None:
        registry = self.copied_registry()
        text = registry.read_text()
        text = text.replace('"writers": [{"actor": "native fixture service", "path": "native/settings.rs"}]', '"writers": [{"actor": "native fixture service", "path": "native/settings.rs"}, {"actor": "renderer", "path": "renderer/settings.ts"}]')
        registry.write_text(text)
        self.assertTrue(any("exactly one writer" in error for error in CHECKER.validate(CHECKER.load_records(registry))))

    def test_renderer_mirror_without_drift_assertion_is_refused(self) -> None:
        registry = self.copied_registry()
        text = registry.read_text().replace('"drift_assertion": "renderer fixture test compares IPC snapshot"', '"drift_assertion": ""')
        registry.write_text(text)
        self.assertTrue(any("every mirror requires" in error for error in CHECKER.validate(CHECKER.load_records(registry))))


class StateAuthorityCannotFailPath(unittest.TestCase):
    """D-247: the gate must never report success on a run that graded nothing."""

    def sandbox(self, *, registry_text: str, stateful: bool) -> Path:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        (directory / "docs").mkdir()
        (directory / "docs/registry.md").write_text(registry_text, encoding="utf-8")
        (directory / "src").mkdir()
        body = 'localStorage.setItem("k", "v");\n' if stateful else "export const noop = 1;\n"
        (directory / "src/thing.ts").write_text(body, encoding="utf-8")
        for command in (["init", "-q", "."], ["add", "-A"]):
            subprocess.run(["git", *command], cwd=directory, check=True)
        subprocess.run(
            ["git", "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "base"],
            cwd=directory, check=True,
        )
        return directory

    def run_main(self, root: Path, *extra: str) -> tuple[int, str]:
        argv = ["check-state-authority.py", "--registry", str(root / "docs/registry.md"), "--root", str(root), *extra]
        err = io.StringIO()
        with unittest.mock.patch("sys.argv", argv), contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
            code = CHECKER.main()
        return code, err.getvalue()

    EMPTY = '# r\n\n```json\n{"records": []}\n```\n'

    def test_empty_registry_with_stateful_tree_and_no_range_refuses(self) -> None:
        root = self.sandbox(registry_text=self.EMPTY, stateful=True)
        code, err = self.run_main(root)
        self.assertEqual(code, 1)
        self.assertIn("REFUSING", err)
        self.assertIn("src/thing.ts", err)
        self.assertIn("no --git-range was supplied", err)

    def test_all_zero_base_sha_range_refuses(self) -> None:
        root = self.sandbox(registry_text=self.EMPTY, stateful=True)
        code, err = self.run_main(root, "--git-range", "0" * 40 + "...HEAD")
        self.assertEqual(code, 1)
        self.assertIn("all-zero sha", err)

    def test_empty_range_string_refuses(self) -> None:
        root = self.sandbox(registry_text=self.EMPTY, stateful=True)
        code, _ = self.run_main(root, "--git-range", "")
        self.assertEqual(code, 1)

    def test_no_range_passes_only_when_the_tree_has_no_stateful_construct(self) -> None:
        root = self.sandbox(registry_text=self.EMPTY, stateful=False)
        code, _ = self.run_main(root)
        self.assertEqual(code, 0)

    def test_fixture_registry_declares_paths(self) -> None:
        self.assertTrue(CHECKER.declared_paths(CHECKER.load_records(FIXTURE)))
        self.assertEqual(CHECKER.declared_paths([]), set())

    def test_usable_range_is_not_treated_as_degenerate(self) -> None:
        self.assertEqual(CHECKER.degenerate_range("abc123...def456"), "")
        self.assertTrue(CHECKER.degenerate_range(None))
        self.assertTrue(CHECKER.degenerate_range("   "))


if __name__ == "__main__":
    unittest.main()
