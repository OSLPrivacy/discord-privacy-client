#!/usr/bin/env python3
"""T8-T20: state-authority registry fixtures and sabotage proofs."""
from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
