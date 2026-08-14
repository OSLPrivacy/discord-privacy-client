#!/usr/bin/env python3
"""TASK 5161: fail-closed Discord PTB decrypted-row fidelity qualification.

The production invocation accepts only the reviewed 5134/5159 capture manifest.
The synthetic mode exists exclusively for this checker's own unit tests.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import zlib
from pathlib import Path
from typing import Any

SCHEMA = "osl.task5161.discord-ptb-decrypted-row-fidelity.v1"
PTB = "DiscordPTB"
STATES = frozenset({
    "direct-message/decrypted-row",
    "group-message/decrypted-row",
    "thread-reply/decrypted-row",
})
GATES = frozenset({"5103", "5108", "5134", "5135", "5158", "5159"})
ENVELOPE = {
    "boundaryDisplacementPhysicalPxMax": 0,
    "baselineDisplacementPhysicalPxMax": 0,
    "flatFillMedianDeltaE00ExclusiveMax": 1.0,
    "flatFillP99DeltaE00ExclusiveMax": 2.3,
    "blurredStructureMismatchPercentMax": 0.5,
    "tile16MismatchPercentMax": 5.0,
    "exactRawMismatchPercentMax": 0.5,
}
SEAM_RING_PX = 4


class GateError(ValueError):
    pass


def exact(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        raise GateError(f"{label} inventory starved or extended")
    return value


def read_png(path: Path) -> tuple[int, int, tuple[tuple[int, int, int, int], ...]]:
    try:
        blob = path.read_bytes()
    except OSError as error:
        raise GateError(f"starved pixels: cannot read {path}: {error}") from error
    if not blob.startswith(b"\x89PNG\r\n\x1a\n"):
        raise GateError(f"starved pixels: {path} is not PNG")
    offset, width, height, colour, depth, chunks = 8, 0, 0, -1, -1, []
    while offset + 12 <= len(blob):
        length = struct.unpack(">I", blob[offset:offset + 4])[0]
        kind = blob[offset + 4:offset + 8]
        data = blob[offset + 8:offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            if depth != 8 or colour not in (2, 6) or compression or filtering or interlace:
                raise GateError(f"unsupported PNG encoding: {path}")
        elif kind == b"IDAT":
            chunks.append(data)
        elif kind == b"IEND":
            break
    channels, stride = (4 if colour == 6 else 3), width * (4 if colour == 6 else 3)
    try:
        raw = zlib.decompress(b"".join(chunks))
    except zlib.error as error:
        raise GateError(f"starved pixels: corrupt PNG {path}: {error}") from error
    prior, cursor, pixels = bytearray(stride), 0, []
    for _ in range(height):
        if cursor + stride + 1 > len(raw):
            raise GateError(f"starved pixels: truncated PNG {path}")
        kind, scan = raw[cursor], bytearray(raw[cursor + 1:cursor + stride + 1])
        cursor += stride + 1
        for i in range(stride):
            left, up, upper_left = (scan[i - channels] if i >= channels else 0), prior[i], (prior[i - channels] if i >= channels else 0)
            if kind == 1: scan[i] = (scan[i] + left) & 255
            elif kind == 2: scan[i] = (scan[i] + up) & 255
            elif kind == 3: scan[i] = (scan[i] + ((left + up) // 2)) & 255
            elif kind == 4:
                estimate = left + up - upper_left
                scan[i] = (scan[i] + min((left, up, upper_left), key=lambda p: abs(estimate - p))) & 255
            elif kind != 0: raise GateError(f"unsupported PNG filter in {path}")
        for i in range(0, stride, channels):
            pixels.append((*scan[i:i + 3], scan[i + 3] if channels == 4 else 255))
        prior = scan
    return width, height, tuple(pixels)


def validate_metrics(state: str, metrics: Any) -> None:
    required = set(ENVELOPE).difference({"boundaryDisplacementPhysicalPxMax", "baselineDisplacementPhysicalPxMax"}) | {"boundaryDisplacementPhysicalPx", "baselineDisplacementPhysicalPx"}
    values = exact(metrics, required, f"state {state} metrics")
    if any(isinstance(value, bool) or not isinstance(value, (int, float)) for value in values.values()):
        raise GateError(f"state {state}: malformed 5103 metrics")
    if (values["boundaryDisplacementPhysicalPx"] != 0 or values["baselineDisplacementPhysicalPx"] != 0
            or values["flatFillMedianDeltaE00ExclusiveMax"] >= 1.0 or values["flatFillP99DeltaE00ExclusiveMax"] >= 2.3
            or values["blurredStructureMismatchPercentMax"] > .5 or values["tile16MismatchPercentMax"] > 5.0
            or values["exactRawMismatchPercentMax"] > .5):
        raise GateError(f"state {state}: exceeds fixed 5135 envelope")


def validate(manifest: Any, root: Path, *, allow_test_pixels: bool = False) -> dict[str, int]:
    m = exact(manifest, {"schema", "carrier", "evidenceMode", "gateReceipts", "effectiveEnvelope", "states", "shipping5158"}, "manifest")
    if m["schema"] != SCHEMA or m["carrier"] != PTB: raise GateError("manifest is not Discord PTB decrypted-row evidence")
    if m["evidenceMode"] != "reviewed_real_5134_5159":
        if not (allow_test_pixels and m["evidenceMode"] == "test_only_synthetic_pixels"):
            raise GateError("fixture renderer substituted for reviewed real capture")
    receipts = exact(m["gateReceipts"], set(GATES), "gate receipts")
    if any(not isinstance(v, str) or len(v) != 64 for v in receipts.values()): raise GateError("gate receipt starved")
    if m["effectiveEnvelope"] != ENVELOPE: raise GateError("fixed 5135 envelope changed")
    shipping = exact(m["shipping5158"], {"gate", "passed", "rendererInShipping", "capturedOslPixels"}, "5158 receipt")
    if shipping["gate"] != "5158" or shipping["passed"] is not True or shipping["rendererInShipping"] is not False or shipping["capturedOslPixels"] != 0:
        raise GateError("5158 shipping exclusion failed")
    if not isinstance(m["states"], list): raise GateError("starved state: states must be an array")
    states = {entry.get("stateId"): entry for entry in m["states"] if isinstance(entry, dict)}
    if len(states) != len(m["states"]) or set(states) != STATES: raise GateError("starved state: 5134 PTB decrypted-row inventory")
    exact_pixels = protected_pixels = 0
    for state_id in sorted(STATES):
        row = exact(states[state_id], {"stateId", "manifestId", "reference5159", "candidate5134", "journeys"}, f"state {state_id}")
        ref = exact(row["reference5159"], {"manifestId", "path", "sha256"}, f"state {state_id} ordinary row")
        candidate = exact(row["candidate5134"], {"manifestId", "path", "sha256", "rendererId", "captureKind"}, f"state {state_id} decrypted row")
        if row["manifestId"] != ref["manifestId"] or row["manifestId"] != candidate["manifestId"]: raise GateError(f"state {state_id}: candidate/reference not same-manifest 5159")
        if candidate["rendererId"] != "discord_ptb_production_decrypted_row_renderer" or candidate["captureKind"] != "native_5134_decrypted_row": raise GateError(f"state {state_id}: fixture renderer substituted")
        ref_path, can_path = root / ref["path"], root / candidate["path"]
        if ref_path.resolve() == can_path.resolve(): raise GateError(f"state {state_id}: reference and candidate pixels are not independent")
        try:
            width, height, reference = read_png(ref_path)
            cwidth, cheight, candidate_pixels = read_png(can_path)
        except GateError as error:
            raise GateError(f"state {state_id}: {error}") from error
        if (width, height) != (cwidth, cheight) or width <= SEAM_RING_PX * 2 or height <= SEAM_RING_PX * 2: raise GateError(f"state {state_id}: dimensions cannot retain untouched seam ring")
        if hashlib.sha256(ref_path.read_bytes()).hexdigest() != ref["sha256"] or hashlib.sha256(can_path.read_bytes()).hexdigest() != candidate["sha256"]: raise GateError(f"state {state_id}: capture hash changed")
        journeys = exact(row["journeys"], {"unmaskedExactText", "glyphOnlyMask"}, f"state {state_id} journeys")
        for journey, protected in (("unmaskedExactText", False), ("glyphOnlyMask", True)):
            proof = exact(journeys[journey], {"maskPixels", "maskRects", "metrics"}, f"state {state_id} {journey}")
            masks = proof["maskRects"]
            if not isinstance(masks, list) or any(not isinstance(rect, list) or len(rect) != 4 for rect in masks): raise GateError(f"state {state_id}: malformed {journey} mask")
            covered = set()
            for left, top, right, bottom in masks:
                if not all(isinstance(n, int) for n in (left, top, right, bottom)) or left < SEAM_RING_PX or top < SEAM_RING_PX or right > width - SEAM_RING_PX or bottom > height - SEAM_RING_PX or left >= right or top >= bottom:
                    raise GateError(f"state {state_id}: {journey} mask touches untouched seam ring")
                covered.update((x, y) for y in range(top, bottom) for x in range(left, right))
            if proof["maskPixels"] != len(covered) or (not protected and (proof["maskPixels"] != 0 or masks)) or (protected and not covered): raise GateError(f"state {state_id}: {journey} mask policy failed")
            for y in range(height):
                for x in range(width):
                    if (x, y) not in covered and reference[y * width + x] != candidate_pixels[y * width + x]: raise GateError(f"state {state_id}: {journey} exact-text mismatch")
            validate_metrics(state_id, proof["metrics"])
            if protected: protected_pixels += len(covered)
            else: exact_pixels += width * height
    return {"states": len(states), "unmasked_exact_pixels": exact_pixels, "glyph_masked_pixels": protected_pixels, "shipping5158": 1}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path); parser.add_argument("--root", type=Path, default=Path.cwd()); args = parser.parse_args(argv)
    try:
        result = validate(json.loads(args.manifest.read_text(encoding="utf-8")), args.root)
    except (OSError, json.JSONDecodeError, GateError) as error:
        print(f"TASK5161_FAIL={error}", file=sys.stderr); return 1
    print("TASK5161_PASS " + " ".join(f"{k}={v}" for k, v in result.items())); return 0


if __name__ == "__main__": raise SystemExit(main())
