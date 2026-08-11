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
SCRIPT = HERE / "task5156_messenger_composer_fidelity.py"
SPEC = importlib.util.spec_from_file_location("task5156", SCRIPT)
assert SPEC and SPEC.loader
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)
H = "a" * 64


def png(path: Path, seed: int) -> tuple[str, int]:
    width, height = 12, 8
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            row.extend(((x * 29 + seed) % 256, (y * 47 + seed) % 256, ((x + y) * 17 + seed) % 256, 255))
        rows.append(bytes(row))
    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xffffffff)
    body = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b""))
    path.write_bytes(body)
    return hashlib.sha256(body).hexdigest(), width * height


class Fixture:
    def __init__(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="task5156-")
        self.root = Path(self.temp.name)
        for relative in (*GATE.RELEASE_FILES, *GATE.INSTALLED_FILES):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("positive-control=true\n", encoding="utf-8")
        states = []
        self.pixel_paths = []
        for index, state_id in enumerate(sorted(GATE.STATES)):
            manifest_id = str(index + 1) * 64
            reference_path = self.root / f"reference-{index}.png"
            candidate_path = self.root / f"candidate-{index}.png"
            ref_hash, _ = png(reference_path, index + 1)
            can_hash, _ = png(candidate_path, index + 1)
            self.pixel_paths.extend((reference_path, candidate_path))
            common = {"manifestId": manifest_id, "key": GATE.REFERENCE_KEYS[state_id], "dimensionsPhysicalPx": [12, 8]}
            states.append({
                "stateId": state_id, "origin": GATE.ORIGIN, "manifestId": manifest_id,
                "reference5130": {**common, "path": reference_path.name, "sha256": ref_hash},
                "candidate": {**common, "path": candidate_path.name, "sha256": can_hash,
                              "rendererId": "task5156-isolated-renderer", "captureKind": "isolated_nonshipping_render"},
                "metrics": {
                    "boundaryDisplacementPhysicalPx": 0, "baselineDisplacementPhysicalPx": 0,
                    "flatFillMedianDeltaE00": 0.0, "flatFillP99DeltaE00": 0.0,
                    "blurredStructureMismatchPercent": 0.0, "tile16MismatchPercentMax": 0.0,
                    "exactRawMismatchPercent": 0.0,
                },
            })
        self.manifest = {
            "schema": GATE.SCHEMA, "carrier": "Messenger", "origin": GATE.ORIGIN,
            "evidenceMode": "test_only_synthetic_pixels",
            "gateReceipts": {gate: H for gate in ("5103", "5130", "5131", "5135")},
            "effectiveEnvelope": copy.deepcopy(GATE.ENVELOPE),
            "contractStates": sorted(GATE.STATES),
            "harness": {"mode": "isolated_nonshipping", "originBound": True,
                        "rendererId": "task5156-isolated-renderer", "rendererInRelease": False, "rendererInstalled": False},
            "states": states,
            "inventories": [{"kind": kind,
                              "scannedFiles": len(GATE.RELEASE_FILES if kind == "release" else GATE.INSTALLED_FILES),
                              **{field: 0 for field in GATE.ZERO_FIELDS}}
                             for kind in sorted(GATE.INVENTORIES)],
        }

    def close(self) -> None:
        self.temp.cleanup()


class Task5156Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = Fixture()

    def tearDown(self) -> None:
        self.fixture.close()

    def assert_red(self, mutate, phrase: str) -> None:
        manifest = copy.deepcopy(self.fixture.manifest)
        mutate(manifest)
        with self.assertRaisesRegex(GATE.GateError, phrase):
            GATE.validate(manifest, self.fixture.root, allow_test_pixels=True)

    def test_contract_mechanics_score_every_nonempty_state_independently(self) -> None:
        result = GATE.validate(self.fixture.manifest, self.fixture.root, allow_test_pixels=True)
        self.assertEqual(result["origin_bound_states"], 3)
        self.assertGreater(result["reference_pixels"], 0)
        self.assertGreater(result["candidate_pixels"], 0)
        print("TASK5156_TEST_CONTRACT states=3 reference_pixels=288 candidate_pixels=288")

    def test_catalogue_pixels_cannot_pose_as_real_5130_evidence(self) -> None:
        with self.assertRaisesRegex(GATE.GateError, "catalogue fixture substituted"):
            GATE.validate(self.fixture.manifest, self.fixture.root)
        print("TASK5156_RED catalogue_fixture exit=1 named=catalogue_fixture_substituted")

    def test_every_state_reference_candidate_and_inventory_starvation_is_named(self) -> None:
        for state_id in sorted(GATE.STATES):
            self.assert_red(lambda m, s=state_id: m["contractStates"].remove(s), "starved state")
            self.assert_red(lambda m, s=state_id: m["states"].__setitem__(slice(None), [x for x in m["states"] if x["stateId"] != s]), "starved state")
        for index in range(3):
            self.assert_red(lambda m, i=index: m["states"][i]["reference5130"].__setitem__("path", "missing.png"), "starved pixels")
            self.assert_red(lambda m, i=index: m["states"][i]["candidate"].__setitem__("path", "missing.png"), "starved pixels")
        for kind in sorted(GATE.INVENTORIES):
            self.assert_red(lambda m, k=kind: m["inventories"].__setitem__(slice(None), [x for x in m["inventories"] if x["kind"] != k]), "starved inventory")
        print("TASK5156_RED states=3 references=3 candidates=3 inventories=2 exit=1 named=true")

    def test_promoted_renderer_and_each_shipping_category_are_named(self) -> None:
        self.assert_red(lambda m: m["harness"].__setitem__("rendererInstalled", True), "promoted renderer")
        for field in GATE.ZERO_FIELDS:
            self.assert_red(lambda m, f=field: m["inventories"][0].__setitem__(f, 1), field)
        print("TASK5156_RED promoted_renderer=1 shipping_categories=5 exit=1 named=true")

    def test_cli_without_real_manifest_exits_one_naming_missing_reference(self) -> None:
        missing = self.fixture.root / "reviewed-real-5130-manifest.json"
        result = subprocess.run(["python3", str(SCRIPT), str(missing), "--root", str(self.fixture.root)],
                                text=True, capture_output=True, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("reviewed real 5130 reference manifest missing", result.stderr)
        print(f"TASK5156_REAL_GATE_BLOCKED exit=1 error={result.stderr.strip()}")


if __name__ == "__main__":
    unittest.main()
