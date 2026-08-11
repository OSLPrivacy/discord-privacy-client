#!/usr/bin/env python3
"""Run TASK 5156b's four isolated break mutations and restored control."""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import os
import shutil
import struct
import tempfile
import zlib
from pathlib import Path

from test_task5156_messenger_composer_fidelity import Fixture

HERE = Path(__file__).resolve().parent
DEFAULT_CHECKER = HERE / "task5156_messenger_composer_fidelity.py"
DEFAULT_EXPECTED = HERE / "test_task5156_messenger_composer_fidelity.py"
CHECKER_SHA256 = "06f0ea3c149de0bee51508a7c92edb42fc6a4ac49e04017e21dc95b2d97aff97"
EXPECTED_IMAGE_SHA256 = "43e60698b6d4891de2d9ef1acba9f81aa97ab1db280afc30ff7d9ff2ad1ffa64"


class ProofError(AssertionError):
    pass


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_checker(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    if not spec or not spec.loader:
        raise ProofError("unchanged TASK 5156 checker could not be loaded")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_rgba_png(path: Path, width: int, height: int, pixels: tuple[tuple[int, int, int, int], ...]) -> None:
    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xffffffff)

    rows = []
    for y in range(height):
        row = bytearray([0])
        for pixel in pixels[y * width:(y + 1) * width]:
            row.extend(pixel)
        rows.append(bytes(row))
    body = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows)))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(body)


def shift_right_one_pixel(checker, path: Path) -> None:
    (width, height), _, _, pixels = checker.png_pixels(path)
    shifted = []
    for y in range(height):
        row = pixels[y * width:(y + 1) * width]
        shifted.extend(((0, 0, 0, 0), *row[:-1]))
    write_rgba_png(path, width, height, tuple(shifted))


def gate_exit(checker, manifest: dict, root: Path) -> tuple[int, str]:
    try:
        checker.validate(manifest, root, allow_test_pixels=True)
    except checker.GateError as error:
        return 1, str(error)
    return 0, "TASK5156_PASS"


def fail(message: str) -> int:
    print(f"TASK5156B_FAIL={message}", file=os.sys.stderr)
    return 1


def run(checker_path: Path, expected_image_path: Path) -> int:
    try:
        if sha256(checker_path) != CHECKER_SHA256:
            return fail("checker edit refused: unchanged TASK 5156 checker digest required")
        if sha256(expected_image_path) != EXPECTED_IMAGE_SHA256:
            return fail("expected-image edit refused: unchanged TASK 5156 fixture pixels digest required")
    except OSError as error:
        return fail(f"protected proof input unavailable: {error}")

    starved = os.environ.get("TASK5156B_STARVE_MUTATION", "")
    mutation_names = {
        "hidden_candidate": "qualification",
        "catalogue_fixture": "qualification",
        "edge_shift_1px": "qualification",
        "packaged_shipping_renderer": "shipping",
    }
    if starved and starved not in mutation_names:
        return fail(f"unknown requested starvation: {starved}")

    fixture = Fixture()
    observed: set[str] = set()
    try:
        pristine = copy.deepcopy(fixture.manifest)
        reference_hashes = {
            state["stateId"]: sha256(fixture.root / state["reference5130"]["path"])
            for state in pristine["states"]
        }
        mutations = (
            ("hidden_candidate", "qualification", "hidden candidate"),
            ("catalogue_fixture", "qualification", "catalogue fixture"),
            ("edge_shift_1px", "qualification", "edge defect"),
            ("packaged_shipping_renderer", "shipping", "illegal shipping promotion"),
        )
        with tempfile.TemporaryDirectory(prefix="task5156b-copies-") as copies:
            copies_root = Path(copies)
            for ordinal, (name, scope, expected) in enumerate(mutations):
                if name == starved:
                    continue
                root = copies_root / name
                shutil.copytree(fixture.root, root)
                copied_checker = root / "task5156_messenger_composer_fidelity.py"
                shutil.copy2(checker_path, copied_checker)
                checker = load_checker(copied_checker, f"task5156_copy_{ordinal}")
                manifest = copy.deepcopy(pristine)
                first = manifest["states"][0]
                candidate = root / first["candidate"]["path"]

                if name == "hidden_candidate":
                    candidate.unlink()
                    if candidate.exists():
                        return fail("starved qualification mutation: hidden_candidate")
                elif name == "catalogue_fixture":
                    manifest["evidenceMode"] = "catalogue_fixture"
                    dimensions, _, _, source_pixels = checker.png_pixels(candidate)
                    catalogue_candidate = root / "catalogue-fixture.png"
                    write_rgba_png(catalogue_candidate, *dimensions, tuple(reversed(source_pixels)))
                    first["candidate"]["path"] = catalogue_candidate.name
                    first["candidate"]["sha256"] = sha256(catalogue_candidate)
                    if (manifest["evidenceMode"] != "catalogue_fixture"
                            or first["candidate"]["path"] != "catalogue-fixture.png"
                            or sha256(catalogue_candidate) == pristine["states"][0]["candidate"]["sha256"]):
                        return fail("starved qualification mutation: catalogue_fixture")
                elif name == "edge_shift_1px":
                    shift_right_one_pixel(checker, candidate)
                    first["candidate"]["sha256"] = sha256(candidate)
                    dimensions, _, _, shifted = checker.png_pixels(candidate)
                    _, _, _, reference = checker.png_pixels(root / first["reference5130"]["path"])
                    width, height = dimensions
                    if any(
                        shifted[y * width] != (0, 0, 0, 0)
                        or shifted[y * width + 1:(y + 1) * width] != reference[y * width:(y + 1) * width - 1]
                        for y in range(height)
                    ):
                        return fail("starved qualification mutation: edge_shift_1px")
                else:
                    restored_before_packaging = checker.validate(manifest, root, allow_test_pixels=True)
                    if restored_before_packaging["origin_bound_states"] != 3:
                        return fail("shipping mutation was not made from a qualified renderer")
                    shipping_file = root / checker.RELEASE_FILES[0]
                    shipping_file.write_text(
                        shipping_file.read_text(encoding="utf-8") + "\nrenderMessengerProtectedComposer\n",
                        encoding="utf-8",
                    )
                    if "renderMessengerProtectedComposer" not in shipping_file.read_text(encoding="utf-8"):
                        return fail("starved shipping mutation: packaged_shipping_renderer")

                if sha256(copied_checker) != CHECKER_SHA256:
                    return fail("checker edit escaped digest protection")
                for state in manifest["states"]:
                    reference_path = root / state["reference5130"]["path"]
                    if sha256(reference_path) != reference_hashes[state["stateId"]]:
                        return fail(f"expected-image edit escaped digest protection: {state['stateId']}")

                exit_code, diagnostic = gate_exit(checker, manifest, root)
                if exit_code != 1 or expected not in diagnostic:
                    return fail(
                        f"mutant {name} did not exit 1 naming {expected}: "
                        f"exit={exit_code} diagnostic={diagnostic}"
                    )
                observed.add(name)
                print(
                    f"TASK5156B_MUTANT name={name} scope={scope} exit=1 "
                    f"defect={expected} gate={diagnostic}"
                )

            missing = next((name for name in mutation_names if name not in observed), None)
            if missing:
                return fail(f"starved {mutation_names[missing]} mutation: {missing}")

            restored_root = copies_root / "restored"
            shutil.copytree(fixture.root, restored_root)
            restored_checker_path = restored_root / "task5156_messenger_composer_fidelity.py"
            shutil.copy2(checker_path, restored_checker_path)
            restored_checker = load_checker(restored_checker_path, "task5156_restored")
            restored = restored_checker.validate(copy.deepcopy(pristine), restored_root, allow_test_pixels=True)
            if restored != {
                "origin_bound_states": 3,
                "reference_pixels": 288,
                "candidate_pixels": 288,
                "shipping_inventories": 2,
                "messenger_providers": 0,
                "composer_painters": 0,
                "shipping_pixels": 0,
                "composer_actions": 0,
                "shipping_claims": 0,
            }:
                return fail(f"restoration did not pass unchanged TASK 5156: {restored}")
            print("TASK5156B_RESTORED exit=0 state_passes=3 reference_pixels=288 candidate_pixels=288")
            print("TASK5156B_PASS mutants=4 qualification_mutants=3 shipping_mutants=1 restoration=1")
    finally:
        fixture.close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checker", type=Path, default=DEFAULT_CHECKER)
    parser.add_argument("--expected-image", type=Path, default=DEFAULT_EXPECTED)
    args = parser.parse_args()
    return run(args.checker.resolve(), args.expected_image.resolve())


if __name__ == "__main__":
    raise SystemExit(main())
