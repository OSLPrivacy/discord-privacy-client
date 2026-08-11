#!/usr/bin/env python3
"""Fail-closed Task 5106 Discord composer reference-set verifier."""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import zlib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONTRACT = REPO_ROOT / "apps/osl-hub/src/discord_composer_surface_contract.json"
DEFAULT_REFERENCES = REPO_ROOT / "evidence/task-5106-discord-composer"
DEFAULT_CENSUS = DEFAULT_REFERENCES / "live-discord-census-2026-08-11.json"
DEFAULT_INDEX = DEFAULT_REFERENCES / "discord-composer-reference-set.json"
DEFAULT_SHIPPING_SOURCE = REPO_ROOT / "apps/osl-hub/src/native_window_host.rs"


class Refusal(Exception):
    pass


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise Refusal(f"{path.name}: unreadable JSON: {error}") from error
    if not isinstance(value, dict):
        raise Refusal(f"{path.name}: expected a JSON object")
    return value


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def expected_surfaces(contract: dict) -> dict[str, dict]:
    if contract.get("schema") != "osl-discord-composer-surface-contract-v1":
        raise Refusal("shipped composer contract: wrong schema")
    channels = contract.get("channels")
    states = contract.get("states")
    surfaces = contract.get("surfaces")
    if not isinstance(channels, list) or not channels or len(set(channels)) != len(channels):
        raise Refusal("shipped composer contract: channel inventory is empty or duplicated")
    if not isinstance(states, list) or not states or not isinstance(surfaces, list):
        raise Refusal("shipped composer contract: states and surfaces are required")
    state_map = {state.get("state"): state for state in states if isinstance(state, dict)}
    if len(state_map) != len(states):
        raise Refusal("shipped composer contract: state inventory is malformed or duplicated")
    for state_name, state in state_map.items():
        if not isinstance(state_name, str) or state.get("content") != "exact_probe" or type(state.get("has_keyboard_focus")) is not bool:
            raise Refusal(f"shipped composer contract: invalid exact-probe state {state_name!r}")
    result: dict[str, dict] = {}
    for surface in surfaces:
        if not isinstance(surface, dict):
            raise Refusal("shipped composer contract: surface entry must be an object")
        key = surface.get("key")
        channel = surface.get("channel")
        state = surface.get("state")
        if key != f"{channel}/{state}" or channel not in channels or state not in state_map:
            raise Refusal(f"shipped composer contract: invalid surface key {key!r}")
        if key in result:
            raise Refusal(f"shipped composer contract: duplicate surface key {key}")
        result[key] = surface
    return result


def decode_png(path: Path) -> tuple[int, int, str, list[tuple[int, int, int]], int]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise Refusal(f"{path.name}: PNG unavailable: {error}") from error
    if not raw.startswith(b"\x89PNG\r\n\x1a\n"):
        raise Refusal(f"{path.name}: not a lossless PNG")
    position = 8
    width = height = 0
    colour_type = -1
    compressed = bytearray()
    while position + 12 <= len(raw):
        length = struct.unpack(">I", raw[position : position + 4])[0]
        kind = raw[position + 4 : position + 8]
        data = raw[position + 8 : position + 8 + length]
        if position + 12 + length > len(raw):
            raise Refusal(f"{path.name}: truncated PNG chunk")
        expected_crc = struct.unpack(">I", raw[position + 8 + length : position + 12 + length])[0]
        if (zlib.crc32(kind + data) & 0xFFFFFFFF) != expected_crc:
            raise Refusal(f"{path.name}: corrupt PNG chunk")
        position += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour_type, compression, filtering, interlace = struct.unpack(">IIBBBBB", data)
            if depth != 8 or colour_type not in (2, 6) or compression or filtering or interlace:
                raise Refusal(f"{path.name}: PNG must be non-interlaced 8-bit RGB/RGBA")
        elif kind == b"IDAT":
            compressed.extend(data)
        elif kind == b"IEND":
            break
    channels = 3 if colour_type == 2 else 4 if colour_type == 6 else 0
    if width < 1 or height < 1 or channels == 0:
        raise Refusal(f"{path.name}: invalid PNG geometry")
    try:
        decoded = zlib.decompress(compressed)
    except zlib.error as error:
        raise Refusal(f"{path.name}: corrupt PNG data") from error
    stride = width * channels
    if len(decoded) != height * (stride + 1):
        raise Refusal(f"{path.name}: unexpected PNG payload length")
    prior = bytearray(stride)
    pixels: list[tuple[int, int, int]] = []
    visible_alpha = 0
    offset = 0
    for _ in range(height):
        filter_kind = decoded[offset]
        scan = bytearray(decoded[offset + 1 : offset + stride + 1])
        offset += stride + 1
        for index in range(stride):
            left = scan[index - channels] if index >= channels else 0
            above = prior[index]
            upper_left = prior[index - channels] if index >= channels else 0
            if filter_kind == 1:
                scan[index] = (scan[index] + left) & 255
            elif filter_kind == 2:
                scan[index] = (scan[index] + above) & 255
            elif filter_kind == 3:
                scan[index] = (scan[index] + ((left + above) // 2)) & 255
            elif filter_kind == 4:
                estimate = left + above - upper_left
                distances = (abs(estimate - left), abs(estimate - above), abs(estimate - upper_left))
                predictor = left if distances[0] <= distances[1] and distances[0] <= distances[2] else above if distances[1] <= distances[2] else upper_left
                scan[index] = (scan[index] + predictor) & 255
            elif filter_kind != 0:
                raise Refusal(f"{path.name}: unsupported PNG filter {filter_kind}")
        for index in range(0, stride, channels):
            alpha = scan[index + 3] if channels == 4 else 255
            visible_alpha += int(alpha > 0)
            pixels.append((scan[index], scan[index + 1], scan[index + 2]))
        prior = scan
    if visible_alpha == 0:
        raise Refusal(f"{path.name}: all-transparent capture")
    return width, height, "RGB" if channels == 3 else "RGBA", pixels, visible_alpha


def seam_sha256(pixels: list[tuple[int, int, int]], width: int, height: int, seam: int) -> str:
    digest = hashlib.sha256()
    for y in range(height):
        for x in range(width):
            if x < seam or y < seam or x >= width - seam or y >= height - seam:
                digest.update(bytes(pixels[y * width + x]))
    return digest.hexdigest()


def census_surfaces(census: dict, expected: dict[str, dict], probe_sha256: str) -> tuple[set[str], list[str]]:
    errors: list[str] = []
    if census.get("schema") != "osl-live-carrier-census-v1" or not census.get("independent_of_contract_and_manifest"):
        errors.append("live carrier census: dated independent completeness oracle is missing")
    marker = census.get("marker")
    marker_hash = census.get("marker_sha256")
    if not isinstance(marker, str) or not marker or marker_hash != sha256(marker.encode()) or marker_hash != probe_sha256:
        errors.append("missing carrier state: live exact-text marker")
    keys: set[str] = set()
    channel_count: dict[str, int] = {}
    for channel in census.get("channels", []):
        if not isinstance(channel, dict):
            continue
        channel_key = channel.get("channel")
        channel_count[channel_key] = channel_count.get(channel_key, 0) + 1
        if channel.get("signature_status") != "valid" or "Discord Inc" not in str(channel.get("signer")):
            errors.append(f"missing carrier state: {channel_key} valid Discord signature")
        if channel.get("original_value_sha256") != channel.get("restored_value_sha256"):
            errors.append(f"missing carrier state: {channel_key} before/after restoration")
        for state in channel.get("states", []):
            if not isinstance(state, dict):
                continue
            key = state.get("key")
            if isinstance(key, str):
                keys.add(key)
            if state.get("marker_readback_sha256") != marker_hash:
                errors.append(f"missing carrier state: {key} marker read-back")
            if not isinstance(state.get("whole_window_distinct_rgb"), int) or state["whole_window_distinct_rgb"] < 256:
                errors.append(f"{key}: whole Discord window below 256 distinct RGB colours")
    for channel in ("stable", "ptb", "canary"):
        if channel_count.get(channel) != 1:
            errors.append(f"missing carrier state: installed channel {channel}")
    missing_from_contract = keys - set(expected)
    missing_from_census = set(expected) - keys
    for key in sorted(missing_from_contract):
        errors.append(f"{key}: live census key missing from shipped contract")
    for key in sorted(missing_from_census):
        errors.append(f"{key}: shipped composer state missing from live census")
    if census.get("shipping_integration", {}).get("observed") is not True:
        errors.append("missing carrier state: shipping Windows integration disabled")
    return keys, errors


def check(
    reference_dir: Path,
    contract_path: Path = DEFAULT_CONTRACT,
    census_path: Path = DEFAULT_CENSUS,
    index_path: Path = DEFAULT_INDEX,
    shipping_source: Path = DEFAULT_SHIPPING_SOURCE,
) -> str:
    contract = load_json(contract_path)
    expected = expected_surfaces(contract)
    errors: list[str] = []
    try:
        source = shipping_source.read_text(encoding="utf-8")
        if 'include_str!("discord_composer_surface_contract.json")' not in source:
            errors.append("missing carrier state: shipping composer contract integration disabled")
    except OSError:
        errors.append("missing carrier state: shipping composer contract integration disabled")

    census = load_json(census_path)
    _, census_errors = census_surfaces(census, expected, contract.get("probe_sha256"))
    errors.extend(census_errors)
    try:
        index = load_json(index_path)
        indexed = index.get("references", [])
        if not isinstance(indexed, list):
            raise Refusal("reference index: references must be a list")
    except Refusal as error:
        errors.append(str(error))
        indexed = []

    seen: dict[str, dict] = {}
    for entry in indexed:
        if not isinstance(entry, dict) or not isinstance(entry.get("key"), str):
            errors.append("reference index: malformed entry")
            continue
        key = entry["key"]
        if key in seen:
            errors.append(f"{key}: duplicate composer reference")
            continue
        seen[key] = entry
        if key not in expected:
            errors.append(f"{key}: unexpected composer reference")
            continue
        try:
            manifest_path = reference_dir / entry.get("manifest", "")
            png_path = reference_dir / entry.get("png", "")
            manifest = load_json(manifest_path)
            if manifest.get("schema") != "osl-discord-composer-reference-v1" or manifest.get("key") != key:
                raise Refusal(f"{key}: manifest schema/key mismatch")
            surface = expected[key]
            for field in ("channel", "state"):
                if manifest.get(field) != surface[field]:
                    raise Refusal(f"{key}: manifest {field} disagrees with shipped contract")
            if manifest.get("carrier") != "Discord" or manifest.get("surface_kind") != "composer":
                raise Refusal(f"{key}: wrong carrier surface")
            if manifest.get("source") != "real_windows_carrier" or manifest.get("synthetic_test_account") is not True:
                raise Refusal(f"{key}: source is not a real Windows synthetic-account carrier")
            if manifest.get("exact_probe_sha256") != contract.get("probe_sha256"):
                raise Refusal(f"{key}: exact-text probe mismatch")
            identity = manifest.get("identity", {})
            if identity.get("signature_status") != "valid" or "Discord Inc" not in str(identity.get("signer")):
                raise Refusal(f"{key}: signed Discord identity missing")
            capture = manifest.get("capture", {})
            if capture.get("seam_ring_physical_px") != 4:
                raise Refusal(f"{key}: untouched 4-physical-pixel seam ring missing")
            counts = capture.get("known_good_distinct_rgb")
            floor = capture.get("fixture_specific_distinct_rgb_floor")
            if not isinstance(counts, list) or len(counts) < 5 or any(type(value) is not int or value < 32 for value in counts):
                raise Refusal(f"{key}: five-known-good distinct-colour fixture missing")
            if floor != min(counts) or floor < 32:
                raise Refusal(f"{key}: fixture-specific distinct-colour floor invalid")
            if not isinstance(capture.get("whole_window_distinct_rgb"), int) or capture["whole_window_distinct_rgb"] < 256:
                raise Refusal(f"{key}: whole Discord window below 256 distinct RGB colours")
            width, height, mode, pixels, _ = decode_png(png_path)
            bounds = manifest.get("uia", {}).get("bounds", {})
            expected_width = bounds.get("right", 0) - bounds.get("left", 0) + 8
            expected_height = bounds.get("bottom", 0) - bounds.get("top", 0) + 8
            if width != expected_width or height != expected_height or mode != capture.get("png_mode"):
                raise Refusal(f"{key}: exact UIA ROI plus seam geometry/mode mismatch")
            colours = len(set(pixels))
            if colours <= 2 or colours < 32 or colours < floor:
                raise Refusal(f"{key}: degenerate ROI has {colours} distinct RGB colours below floor {floor}")
            if colours != capture.get("persisted_distinct_rgb"):
                raise Refusal(f"{key}: persisted distinct RGB count disagrees ({colours})")
            if sha256(png_path.read_bytes()) != capture.get("png_sha256"):
                raise Refusal(f"{key}: PNG SHA-256 mismatch")
            if seam_sha256(pixels, width, height, 4) != capture.get("seam_ring_rgb_sha256"):
                raise Refusal(f"{key}: untouched seam ring SHA-256 mismatch")

            review = manifest.get("baseline_review", {})
            candidate_name = review.get("candidate_baseline_record")
            if not candidate_name:
                raise Refusal(f"{key}: content-addressed candidate baseline record missing")
            candidate = load_json(reference_dir / candidate_name)
            if (
                candidate.get("schema") != "osl-carrier-baseline-candidate-v1"
                or candidate.get("key") != key
                or candidate.get("png_sha256") != capture.get("png_sha256")
                or candidate.get("manifest_sha256") != sha256(manifest_path.read_bytes())
                or candidate.get("capture_author") != review.get("capture_author")
            ):
                raise Refusal(f"{key}: candidate baseline does not bind manifest and PNG")
            if review.get("status") != "accepted" or not review.get("reviewed_baseline_record"):
                raise Refusal(f"{key}: distinct authorized reviewed baseline record missing")
            if not review.get("reviewer") or review.get("reviewer") == review.get("capture_author"):
                raise Refusal(f"{key}: distinct authorized reviewer missing")
            review_path = reference_dir / review["reviewed_baseline_record"]
            record = load_json(review_path)
            if record.get("key") != key or record.get("png_sha256") != capture.get("png_sha256"):
                raise Refusal(f"{key}: reviewed baseline record does not bind PNG")
            if record.get("capture_author") != review.get("capture_author") or record.get("reviewer") != review.get("reviewer"):
                raise Refusal(f"{key}: reviewed baseline actors disagree")
            if not isinstance(record.get("reviewer_signature"), str) or len(record["reviewer_signature"]) < 64:
                raise Refusal(f"{key}: authorized reviewer signature missing")
        except Refusal as error:
            errors.append(str(error))

    for key in sorted(set(expected) - set(seen)):
        errors.append(f"{key}: required composer reference is missing")
    for channel in ("stable", "ptb", "canary"):
        count = sum(key.startswith(f"{channel}/") for key in seen)
        if count == 0:
            errors.append(f"channel {channel}: has 0 composer references")
    if errors:
        raise Refusal("\n".join(errors))
    return (
        f"TASK5106_PASS contract_keys={len(expected)} manifest_keys={len(seen)} "
        "channels=3 composers=6 known_good_captures=30 min_whole_window_distinct_rgb=256 "
        "min_persisted_roi_distinct_rgb=32 reviewed_baselines=6"
    )


def require_mutation_coverage(observed: set[str]) -> None:
    required = {"missing_canary", "missing_focused_state", "one_colour_stable"}
    missing = required - observed
    if missing:
        raise Refusal(f"TASK5106b missing mutation coverage: {', '.join(sorted(missing))}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--references", type=Path, default=DEFAULT_REFERENCES)
    parser.add_argument("--contract", type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument("--census", type=Path, default=DEFAULT_CENSUS)
    parser.add_argument("--index", type=Path, default=DEFAULT_INDEX)
    parser.add_argument("--shipping-source", type=Path, default=DEFAULT_SHIPPING_SOURCE)
    args = parser.parse_args()
    try:
        print(check(args.references, args.contract, args.census, args.index, args.shipping_source))
    except Refusal as error:
        print(f"TASK5106_REFUSED {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
