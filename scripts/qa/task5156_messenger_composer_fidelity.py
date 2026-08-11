#!/usr/bin/env python3
"""TASK 5156 fail-closed, nonshipping Messenger composer qualification gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
import sys
import zlib
from pathlib import Path
from typing import Any

SCHEMA = "osl.task5156.messenger-composer-fidelity.v1"
ORIGIN = "https://www.messenger.com"
STATES = {
    "direct-message/ordinary-unprotected",
    "group/ordinary-unprotected",
    "community/ordinary-unprotected",
}
REFERENCE_KEYS = {
    state: f"{ORIGIN}/{state.split('/')[0]}/ordinary-unprotected-probe"
    for state in STATES
}
INVENTORIES = {"release", "installed_process"}
ENVELOPE = {
    "boundaryDisplacementPhysicalPxMax": 0,
    "baselineDisplacementPhysicalPxMax": 0,
    "flatFillMedianDeltaE00ExclusiveMax": 1.0,
    "flatFillP99DeltaE00ExclusiveMax": 2.3,
    "blurredStructureMismatchPercentMax": 0.5,
    "tile16MismatchPercentMax": 5.0,
    "exactRawMismatchPercentMax": 0.5,
}
ZERO_FIELDS = (
    "messengerProviderCount", "composerPainterCount", "protectedPixelCount",
    "composerActionCount", "shippingClaimCount",
)
RELEASE_FILES = (
    "scripts/build-release-installer.mjs", "apps/osl-hub/tauri.conf.json",
    "data/public-surface-manifest.json", "data/pricing.json",
)
INSTALLED_FILES = (
    "apps/osl-hub/capabilities/hub.json", "apps/osl-hub/permissions/hub.toml",
    "src-tauri/capabilities/main.json", "src-tauri/permissions/osl-encrypt-message.toml",
)
NEEDLES = {
    "messengerProviderCount": ("MessengerProtectedComposerProvider", "messenger-protected-composer-provider"),
    "composerPainterCount": ("MessengerProtectedComposerPainter", "renderMessengerProtectedComposer"),
    "protectedPixelCount": ("messengerProtectedComposerPixels", "messenger-protected-composer-pixels"),
    "composerActionCount": ("openMessengerProtectedComposer", "open-messenger-protected-composer"),
    "shippingClaimCount": ("Messenger protected composer supported", '"messenger-protected-composer"'),
}


class GateError(ValueError):
    pass


def require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise GateError(f"{label} must be an object")
    return value


def require_exact(value: dict[str, Any], keys: set[str], label: str) -> None:
    if set(value) != keys:
        raise GateError(f"{label} inventory starved or extended")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_pixels(path: Path) -> tuple[tuple[int, int], int, int]:
    """Return dimensions, nontransparent pixels, and distinct RGB colours."""
    try:
        blob = path.read_bytes()
    except OSError as error:
        raise GateError(f"starved pixels: cannot read {path}: {error}") from error
    if not blob.startswith(b"\x89PNG\r\n\x1a\n"):
        raise GateError(f"starved pixels: {path} is not PNG")
    offset, chunks, width, height, colour, depth = 8, [], 0, 0, -1, -1
    while offset + 12 <= len(blob):
        size = struct.unpack(">I", blob[offset:offset + 4])[0]
        kind, data = blob[offset + 4:offset + 8], blob[offset + 8:offset + 8 + size]
        offset += 12 + size
        if kind == b"IHDR":
            width, height, depth, colour, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            if depth != 8 or colour not in (2, 6) or compression or filtering or interlace:
                raise GateError(f"unsupported PNG encoding: {path}")
        elif kind == b"IDAT":
            chunks.append(data)
        elif kind == b"IEND":
            break
    channels = 4 if colour == 6 else 3
    stride = width * channels
    try:
        raw = zlib.decompress(b"".join(chunks))
    except zlib.error as error:
        raise GateError(f"starved pixels: corrupt PNG {path}: {error}") from error
    previous = bytearray(stride)
    cursor, nonempty, colours = 0, 0, set()
    for _ in range(height):
        if cursor + stride + 1 > len(raw):
            raise GateError(f"starved pixels: truncated PNG {path}")
        filter_kind = raw[cursor]
        scan = bytearray(raw[cursor + 1:cursor + 1 + stride])
        cursor += stride + 1
        for index in range(stride):
            left = scan[index - channels] if index >= channels else 0
            up = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_kind == 1:
                scan[index] = (scan[index] + left) & 255
            elif filter_kind == 2:
                scan[index] = (scan[index] + up) & 255
            elif filter_kind == 3:
                scan[index] = (scan[index] + ((left + up) // 2)) & 255
            elif filter_kind == 4:
                estimate = left + up - upper_left
                nearest = min((left, up, upper_left), key=lambda x: abs(estimate - x))
                scan[index] = (scan[index] + nearest) & 255
            elif filter_kind != 0:
                raise GateError(f"unsupported PNG filter in {path}")
        for index in range(0, stride, channels):
            rgb = tuple(scan[index:index + 3])
            alpha = scan[index + 3] if channels == 4 else 255
            if alpha:
                nonempty += 1
                colours.add(rgb)
        previous = scan
    return (width, height), nonempty, len(colours)


def scan_inventory(root: Path, kind: str) -> dict[str, int]:
    relatives = RELEASE_FILES if kind == "release" else INSTALLED_FILES
    texts = []
    for relative in relatives:
        path = root / relative
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            raise GateError(f"starved inventory {kind}: missing {relative}: {error}") from error
        if not text.strip():
            raise GateError(f"starved inventory {kind}: empty {relative}")
        texts.append(text.lower())
    joined = "\n".join(texts)
    return {
        field: sum(joined.count(needle.lower()) for needle in needles)
        for field, needles in NEEDLES.items()
    }


def validate_metrics(metrics: dict[str, Any], state: str) -> None:
    required = {
        "boundaryDisplacementPhysicalPx", "baselineDisplacementPhysicalPx",
        "flatFillMedianDeltaE00", "flatFillP99DeltaE00",
        "blurredStructureMismatchPercent", "tile16MismatchPercentMax",
        "exactRawMismatchPercent",
    }
    require_exact(metrics, required, f"state {state} metrics")
    values = list(metrics.values())
    if any(isinstance(value, bool) or not isinstance(value, (int, float)) for value in values):
        raise GateError(f"state {state}: malformed 5103 metrics")
    if (metrics["boundaryDisplacementPhysicalPx"] != 0
            or metrics["baselineDisplacementPhysicalPx"] != 0
            or metrics["flatFillMedianDeltaE00"] >= 1.0
            or metrics["flatFillP99DeltaE00"] >= 2.3
            or metrics["blurredStructureMismatchPercent"] > 0.5
            or metrics["tile16MismatchPercentMax"] > 5.0
            or metrics["exactRawMismatchPercent"] > 0.5):
        raise GateError(f"state {state}: exceeds unchanged 5103/5135 envelope")


def validate(manifest: dict[str, Any], root: Path, *, allow_test_pixels: bool = False) -> dict[str, int]:
    require_exact(manifest, {
        "schema", "carrier", "origin", "evidenceMode", "gateReceipts",
        "effectiveEnvelope", "contractStates", "harness", "states", "inventories",
    }, "manifest")
    if manifest["schema"] != SCHEMA or manifest["carrier"] != "Messenger" or manifest["origin"] != ORIGIN:
        raise GateError("manifest identity or Messenger origin changed")
    if manifest["evidenceMode"] != "reviewed_real_5130":
        if not allow_test_pixels:
            raise GateError("catalogue fixture substituted for reviewed real 5130 reference")
    receipts = require_object(manifest["gateReceipts"], "gateReceipts")
    require_exact(receipts, {"5103", "5130", "5131", "5135"}, "gate receipts")
    if any(not isinstance(value, str) or len(value) != 64 for value in receipts.values()):
        raise GateError("gate receipt starved")
    if manifest["effectiveEnvelope"] != ENVELOPE:
        raise GateError("5103/5135 envelope changed")
    contract = manifest["contractStates"]
    if not isinstance(contract, list) or len(contract) != len(set(contract)) or set(contract) != STATES:
        raise GateError("starved state: 5131 origin-bound contract inventory")
    harness = require_object(manifest["harness"], "harness")
    require_exact(harness, {"mode", "originBound", "rendererId", "rendererInRelease", "rendererInstalled"}, "harness")
    if (harness["mode"] != "isolated_nonshipping" or harness["originBound"] is not True
            or harness["rendererInRelease"] is not False or harness["rendererInstalled"] is not False):
        raise GateError("promoted renderer: Messenger qualification renderer escaped isolation")
    states = manifest["states"]
    if not isinstance(states, list):
        raise GateError("starved state: states must be an array")
    by_id = {item.get("stateId"): item for item in states if isinstance(item, dict)}
    if len(states) != len(by_id) or set(by_id) != STATES:
        raise GateError("starved state: candidate/reference state inventory")
    total_reference = total_candidate = 0
    for state_id in sorted(STATES):
        state = by_id[state_id]
        require_exact(state, {"stateId", "origin", "manifestId", "reference5130", "candidate", "metrics"}, f"state {state_id}")
        if state["origin"] != ORIGIN:
            raise GateError(f"state {state_id}: origin binding changed")
        reference = require_object(state["reference5130"], f"state {state_id} reference")
        candidate = require_object(state["candidate"], f"state {state_id} candidate")
        fields = {"manifestId", "key", "path", "sha256", "dimensionsPhysicalPx"}
        require_exact(reference, fields, f"state {state_id} reference")
        require_exact(candidate, fields | {"rendererId", "captureKind"}, f"state {state_id} candidate")
        if (reference["manifestId"] != state["manifestId"] or candidate["manifestId"] != state["manifestId"]
                or reference["key"] != REFERENCE_KEYS[state_id] or candidate["key"] != REFERENCE_KEYS[state_id]):
            raise GateError(f"state {state_id}: candidate/reference not same-manifest 5130")
        if candidate["rendererId"] != harness["rendererId"] or candidate["captureKind"] != "isolated_nonshipping_render":
            raise GateError(f"state {state_id}: promoted or foreign renderer")
        reference_path, candidate_path = root / reference["path"], root / candidate["path"]
        if reference_path == candidate_path:
            raise GateError(f"state {state_id}: reference and candidate pixels are not independent")
        ref_dimensions, ref_pixels, ref_colours = png_pixels(reference_path)
        can_dimensions, can_pixels, can_colours = png_pixels(candidate_path)
        if sha256(reference_path) != reference["sha256"] or sha256(candidate_path) != candidate["sha256"]:
            raise GateError(f"state {state_id}: pixel hash changed")
        if (list(ref_dimensions) != reference["dimensionsPhysicalPx"]
                or list(can_dimensions) != candidate["dimensionsPhysicalPx"]
                or ref_dimensions != can_dimensions):
            raise GateError(f"state {state_id}: dimensions differ")
        if ref_pixels == 0 or ref_colours <= 2:
            raise GateError(f"state {state_id}: starved reference pixels")
        if can_pixels == 0 or can_colours <= 2:
            raise GateError(f"state {state_id}: starved candidate pixels")
        validate_metrics(require_object(state["metrics"], "metrics"), state_id)
        total_reference += ref_pixels
        total_candidate += can_pixels
    inventories = manifest["inventories"]
    if not isinstance(inventories, list):
        raise GateError("starved inventory")
    declared = {item.get("kind"): item for item in inventories if isinstance(item, dict)}
    if len(inventories) != len(declared) or set(declared) != INVENTORIES:
        raise GateError("starved inventory: release and installed_process required separately")
    for kind in sorted(INVENTORIES):
        item = declared[kind]
        require_exact(item, {"kind", "scannedFiles", *ZERO_FIELDS}, f"inventory {kind}")
        expected_files = len(RELEASE_FILES if kind == "release" else INSTALLED_FILES)
        if item["scannedFiles"] != expected_files:
            raise GateError(f"starved inventory {kind}: scanned file count changed")
        actual = scan_inventory(root, kind)
        for field in ZERO_FIELDS:
            if item[field] != 0 or actual[field] != 0:
                raise GateError(f"inventory {kind}: {field}={max(item[field], actual[field])}, required 0")
    return {
        "origin_bound_states": len(states), "reference_pixels": total_reference,
        "candidate_pixels": total_candidate, "shipping_inventories": len(inventories),
        "messenger_providers": 0, "composer_painters": 0, "shipping_pixels": 0,
        "composer_actions": 0, "shipping_claims": 0,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args(argv)
    try:
        try:
            manifest_text = args.manifest.read_text(encoding="utf-8")
        except OSError as error:
            raise GateError(f"reviewed real 5130 reference manifest missing: {error}") from error
        manifest = json.loads(manifest_text)
        result = validate(require_object(manifest, "manifest"), args.root)
    except (OSError, json.JSONDecodeError, GateError) as error:
        print(f"TASK5156_FAIL={error}", file=sys.stderr)
        return 1
    print("TASK5156_PASS " + " ".join(f"{key}={value}" for key, value in result.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
