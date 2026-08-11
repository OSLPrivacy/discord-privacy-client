from __future__ import annotations

import binascii
import copy
import hashlib
import json
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

import check as subject


def png(width: int, height: int, solid: bool = False) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        crc = binascii.crc32(kind + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", crc)
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            rows.extend((49, 51, 56, 255) if solid else ((x * 17) & 255, (y * 31) & 255, ((x + y) * 13) & 255, 255))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")


def fixture(root: Path) -> dict[str, Path]:
    contract = subject.load_json(subject.DEFAULT_CONTRACT)
    contract_path = root / "contract.json"
    census_path = root / "census.json"
    index_path = root / "index.json"
    source_path = root / "native_window_host.rs"
    references = root / "references"
    references.mkdir()
    write_json(contract_path, contract)
    source_path.write_text('pub const C: &str = include_str!("discord_composer_surface_contract.json");', encoding="utf-8")
    marker = "OSL5106-FIXTURE-EXACT-PROBE"
    marker_hash = hashlib.sha256(marker.encode()).hexdigest()
    contract["probe_sha256"] = marker_hash
    write_json(contract_path, contract)
    census_channels = []
    entries = []
    for channel in contract["channels"]:
        states = []
        for surface in (item for item in contract["surfaces"] if item["channel"] == channel):
            states.append({"key": surface["key"], "marker_readback_sha256": marker_hash, "whole_window_distinct_rgb": 1024})
            key = surface["key"]
            stem = key.replace("/", "--")
            image = png(32, 20)
            png_path = references / f"{stem}.png"
            manifest_path = references / f"{stem}.manifest.json"
            review_path = references / f"{stem}.review.json"
            candidate_path = references / f"{stem}.candidate-baseline.json"
            png_path.write_bytes(image)
            png_hash = hashlib.sha256(image).hexdigest()
            _, _, _, pixels, _ = subject.decode_png(png_path)
            count = len(set(pixels))
            manifest = {
                "schema": "osl-discord-composer-reference-v1",
                "key": key,
                "carrier": "Discord",
                "channel": channel,
                "state": surface["state"],
                "surface_kind": "composer",
                "source": "real_windows_carrier",
                "synthetic_test_account": True,
                "exact_probe_sha256": marker_hash,
                "identity": {"signature_status": "valid", "signer": "CN=Discord Inc."},
                "uia": {"bounds": {"left": 100, "top": 200, "right": 124, "bottom": 212}},
                "capture": {
                    "seam_ring_physical_px": 4,
                    "png_mode": "RGBA",
                    "png_sha256": png_hash,
                    "seam_ring_rgb_sha256": subject.seam_sha256(pixels, 32, 20, 4),
                    "persisted_distinct_rgb": count,
                    "whole_window_distinct_rgb": 1024,
                    "known_good_distinct_rgb": [count] * 5,
                    "fixture_specific_distinct_rgb_floor": count,
                },
                "baseline_review": {
                    "status": "accepted",
                    "capture_author": "capture-author",
                    "candidate_baseline_record": candidate_path.name,
                    "reviewer": "release-reviewer",
                    "reviewed_baseline_record": review_path.name,
                },
            }
            write_json(manifest_path, manifest)
            write_json(candidate_path, {
                "schema": "osl-carrier-baseline-candidate-v1",
                "key": key,
                "png_sha256": png_hash,
                "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
                "capture_author": "capture-author",
            })
            write_json(review_path, {
                "key": key,
                "png_sha256": png_hash,
                "capture_author": "capture-author",
                "reviewer": "release-reviewer",
                "reviewer_signature": "ab" * 64,
            })
            entries.append({"key": key, "png": png_path.name, "manifest": manifest_path.name})
        census_channels.append({
            "channel": channel,
            "signature_status": "valid",
            "signer": "CN=Discord Inc.",
            "original_value_sha256": "00" * 32,
            "restored_value_sha256": "00" * 32,
            "states": states,
        })
    write_json(census_path, {
        "schema": "osl-live-carrier-census-v1",
        "independent_of_contract_and_manifest": True,
        "marker": marker,
        "marker_sha256": marker_hash,
        "shipping_integration": {"observed": True},
        "channels": census_channels,
    })
    write_json(index_path, {"references": entries})
    return {"contract": contract_path, "census": census_path, "index": index_path, "source": source_path, "references": references}


def run(paths: dict[str, Path]) -> str:
    return subject.check(paths["references"], paths["contract"], paths["census"], paths["index"], paths["source"])


class Task5106CheckerTest(unittest.TestCase):
    def test_complete_six_key_fixture_passes_without_vacuity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = fixture(Path(directory))
            result = run(paths)
            self.assertIn("contract_keys=6 manifest_keys=6 channels=3 composers=6 known_good_captures=30", result)
            print(result)

    def test_coordinated_contract_and_manifest_shrink_still_fails_against_census(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = fixture(Path(directory))
            missing = "ptb/composer-unfocused-probe"
            contract = subject.load_json(paths["contract"])
            contract["surfaces"] = [item for item in contract["surfaces"] if item["key"] != missing]
            write_json(paths["contract"], contract)
            index = subject.load_json(paths["index"])
            index["references"] = [item for item in index["references"] if item["key"] != missing]
            write_json(paths["index"], index)
            with self.assertRaisesRegex(subject.Refusal, f"{missing}: live census key missing from shipped contract"):
                run(paths)
            print(f"TASK5106_BREAK coordinated_shrink={missing} exit=1 named=true")

    def test_live_marker_and_shipping_integration_are_independent_required_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = fixture(Path(directory))
            census = subject.load_json(paths["census"])
            census["marker"] = ""
            census["shipping_integration"]["observed"] = False
            write_json(paths["census"], census)
            with self.assertRaises(subject.Refusal) as raised:
                run(paths)
            message = str(raised.exception)
            self.assertIn("missing carrier state: live exact-text marker", message)
            self.assertIn("missing carrier state: shipping Windows integration disabled", message)
            print("TASK5106_BREAK starved_live_marker exit=1 named=true disabled_shipping_integration exit=1 named=true")

    def test_5106b_missing_canary_missing_focused_and_one_colour_are_all_named(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = fixture(Path(directory))
            index = subject.load_json(paths["index"])
            missing_canary = "canary/composer-unfocused-probe"
            missing_focused = "stable/composer-focused-probe"
            one_colour = "stable/composer-unfocused-probe"
            index["references"] = [item for item in index["references"] if item["key"] not in {missing_canary, missing_focused}]
            target = next(item for item in index["references"] if item["key"] == one_colour)
            image = png(32, 20, solid=True)
            png_path = paths["references"] / target["png"]
            png_path.write_bytes(image)
            image_hash = hashlib.sha256(image).hexdigest()
            manifest_path = paths["references"] / target["manifest"]
            manifest = subject.load_json(manifest_path)
            manifest["capture"]["png_sha256"] = image_hash
            manifest["capture"]["persisted_distinct_rgb"] = 1
            write_json(manifest_path, manifest)
            review_path = paths["references"] / manifest["baseline_review"]["reviewed_baseline_record"]
            review = subject.load_json(review_path)
            review["png_sha256"] = image_hash
            write_json(review_path, review)
            write_json(paths["index"], index)
            with self.assertRaises(subject.Refusal) as raised:
                run(paths)
            message = str(raised.exception)
            self.assertIn(f"{missing_canary}: required composer reference is missing", message)
            self.assertIn(f"{missing_focused}: required composer reference is missing", message)
            self.assertIn(f"{one_colour}: degenerate ROI has 1 distinct RGB colours", message)
            subject.require_mutation_coverage({"missing_canary", "missing_focused_state", "one_colour_stable"})
            print(f"TASK5106b_RED missing={missing_canary} missing={missing_focused} degenerate={one_colour} colours=1 exit=1 all_named=true")

    def test_5106b_starved_mutation_inventory_fails_by_name(self) -> None:
        for starved in ("missing_canary", "missing_focused_state", "one_colour_stable"):
            observed = {"missing_canary", "missing_focused_state", "one_colour_stable"} - {starved}
            with self.assertRaisesRegex(subject.Refusal, f"missing mutation coverage: {starved}"):
                subject.require_mutation_coverage(observed)
            print(f"TASK5106b_STARVED mutation={starved} exit=1 named=true")


if __name__ == "__main__":
    unittest.main()
