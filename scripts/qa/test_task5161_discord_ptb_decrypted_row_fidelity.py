#!/usr/bin/env python3
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import struct
import subprocess
import tempfile
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "task5161_discord_ptb_decrypted_row_fidelity.py"
SPEC = importlib.util.spec_from_file_location("task5161", SCRIPT)
assert SPEC and SPEC.loader
GATE = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(GATE)
H = "a" * 64


def png(path: Path, seed: int) -> str:
    width, height = 20, 16
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width): row.extend(((seed + x * 17) % 256, (seed + y * 31) % 256, (seed + x + y * 7) % 256, 255))
        rows.append(bytes(row))
    def chunk(kind: bytes, body: bytes) -> bytes: return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xffffffff)
    blob = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
    path.write_bytes(blob); return hashlib.sha256(blob).hexdigest()


class Fixture:
    def __init__(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="task5161-"); self.root = Path(self.temp.name)
        states = []
        for index, state in enumerate(sorted(GATE.STATES)):
            reference, candidate = self.root / f"ordinary-{index}.png", self.root / f"decrypted-{index}.png"
            ref_hash, candidate_hash = png(reference, index), png(candidate, index)
            metrics = {"boundaryDisplacementPhysicalPx": 0, "baselineDisplacementPhysicalPx": 0,
                       "flatFillMedianDeltaE00ExclusiveMax": 0.0, "flatFillP99DeltaE00ExclusiveMax": 0.0,
                       "blurredStructureMismatchPercentMax": 0.0, "tile16MismatchPercentMax": 0.0,
                       "exactRawMismatchPercentMax": 0.0}
            states.append({"stateId": state, "manifestId": str(index + 1) * 64,
                           "reference5159": {"manifestId": str(index + 1) * 64, "path": reference.name, "sha256": ref_hash},
                           "candidate5134": {"manifestId": str(index + 1) * 64, "path": candidate.name, "sha256": candidate_hash,
                                             "rendererId": "discord_ptb_production_decrypted_row_renderer", "captureKind": "native_5134_decrypted_row"},
                           "journeys": {"unmaskedExactText": {"maskPixels": 0, "maskRects": [], "metrics": metrics},
                                         "glyphOnlyMask": {"maskPixels": 4, "maskRects": [[8, 6, 10, 8]], "metrics": metrics}}})
        self.manifest = {"schema": GATE.SCHEMA, "carrier": GATE.PTB, "evidenceMode": "test_only_synthetic_pixels",
                         "gateReceipts": {gate: H for gate in GATE.GATES}, "effectiveEnvelope": copy.deepcopy(GATE.ENVELOPE),
                         "states": states, "shipping5158": {"gate": "5158", "passed": True, "rendererInShipping": False, "capturedOslPixels": 0}}

    def close(self) -> None: self.temp.cleanup()


class Task5161(unittest.TestCase):
    def setUp(self) -> None: self.fixture = Fixture()
    def tearDown(self) -> None: self.fixture.close()
    def red(self, mutate, state: str) -> None:
        manifest = copy.deepcopy(self.fixture.manifest); mutate(manifest)
        with self.assertRaisesRegex(GATE.GateError, f"state {state}"): GATE.validate(manifest, self.fixture.root, allow_test_pixels=True)

    def test_every_ptb_state_passes_each_journey_and_5158_separately(self) -> None:
        result = GATE.validate(self.fixture.manifest, self.fixture.root, allow_test_pixels=True)
        self.assertEqual(result, {"states": 3, "unmasked_exact_pixels": 960, "glyph_masked_pixels": 12, "shipping5158": 1})
        print("TASK5161_PASS states=3 unmasked_exact_pixels=960 glyph_masked_pixels=12 shipping5158=1")

    def test_starving_candidate_widening_mask_and_fixture_renderer_exit_one_naming_state(self) -> None:
        for index, state in enumerate(sorted(GATE.STATES)):
            self.red(lambda m, i=index: m["states"][i]["candidate5134"].__setitem__("path", "missing.png"), state)
            self.red(lambda m, i=index: m["states"][i]["journeys"]["glyphOnlyMask"].__setitem__("maskRects", [[1, 6, 10, 8]]), state)
            self.red(lambda m, i=index: m["states"][i]["candidate5134"].__setitem__("rendererId", "fixture_renderer"), state)
        print("TASK5161_RED states=3 mutants=9 exit=1 named_state=true")

    def test_real_checker_rejects_synthetic_fixture_and_5158_remains_separate(self) -> None:
        manifest_path = self.fixture.root / "manifest.json"; manifest_path.write_text(json.dumps(self.fixture.manifest), encoding="utf-8")
        result = subprocess.run(["python3", str(SCRIPT), str(manifest_path), "--root", str(self.fixture.root)], text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 1); self.assertIn("fixture renderer substituted", result.stderr)
        bad = copy.deepcopy(self.fixture.manifest); bad["shipping5158"]["capturedOslPixels"] = 1
        with self.assertRaisesRegex(GATE.GateError, "5158 shipping exclusion failed"): GATE.validate(bad, self.fixture.root, allow_test_pixels=True)
        print("TASK5161_RED synthetic_fixture exit=1 shipping5158=separate")


if __name__ == "__main__": unittest.main()
