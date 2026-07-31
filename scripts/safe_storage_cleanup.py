#!/usr/bin/env python3
"""Recoverable storage cleanup gated by archived worktree proof and consent."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
DEFAULT_MIN_PATCHES = 95
CONSENT_TEXT = "I AUTHORIZE SAFE STORAGE CLEANUP"
PLAN_KEYS = {"schemaVersion", "allowedRoot", "actions"}
ACTION_KEYS = {"action", "path", "reason"}
CONSENT_KEYS = {
    "schemaVersion",
    "operatorConsent",
    "preservedDir",
    "preservedPatchCount",
    "preservedManifestSha256",
    "cleanupPlanSha256",
    "authorizedAtUtc",
}


class CleanupRefusal(ValueError):
    pass


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise CleanupRefusal(f"{label} is not valid UTF-8 JSON: {exc}") from exc


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CleanupRefusal(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise CleanupRefusal(
            f"{label} fields are not exact; missing={sorted(keys - actual)} "
            f"unknown={sorted(actual - keys)}"
        )
    return value


def archive_proof(preserved_dir: Path, min_patches: int = DEFAULT_MIN_PATCHES) -> dict[str, Any]:
    root = preserved_dir.resolve()
    if not root.is_dir():
        raise CleanupRefusal("Preserved archive directory is absent")
    patches = sorted(path for path in root.rglob("*.patch") if path.is_file() and not path.is_symlink())
    if len(patches) < min_patches:
        raise CleanupRefusal(
            f"Preserved archive proof has {len(patches)} patch files; need at least {min_patches}"
        )
    manifest = []
    for path in patches:
        relative = path.relative_to(root).as_posix()
        manifest.append(f"{relative}\0{sha256_bytes(path.read_bytes())}")
    digest = sha256_bytes("\n".join(manifest).encode("utf-8"))
    return {
        "preservedDir": str(root),
        "preservedPatchCount": len(patches),
        "preservedManifestSha256": digest,
    }


def load_plan(plan_path: Path) -> tuple[dict[str, Any], str]:
    try:
        payload = plan_path.read_bytes()
        plan = exact_object(json.loads(payload.decode("utf-8")), PLAN_KEYS, "Cleanup plan")
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise CleanupRefusal(f"Cleanup plan is not valid UTF-8 JSON: {exc}") from exc
    if plan["schemaVersion"] != SCHEMA_VERSION:
        raise CleanupRefusal("Cleanup plan schema version is unsupported")
    allowed_root = plan["allowedRoot"]
    if not isinstance(allowed_root, str) or not allowed_root.strip():
        raise CleanupRefusal("Cleanup plan allowed root is invalid")
    actions = plan["actions"]
    if not isinstance(actions, list) or not actions:
        raise CleanupRefusal("Cleanup plan must contain at least one action")
    for index, action in enumerate(actions):
        exact_object(action, ACTION_KEYS, f"Cleanup action {index}")
        if action["action"] not in {"removeFile", "removeEmptyDirectory"}:
            raise CleanupRefusal(f"Cleanup action {index} has an unsupported verb")
        for field in ("path", "reason"):
            if not isinstance(action[field], str) or not action[field].strip():
                raise CleanupRefusal(f"Cleanup action {index} has an invalid {field}")
        if any(char in action["path"] for char in "*?[]{}"):
            raise CleanupRefusal(f"Cleanup action {index} path contains glob syntax")
    return plan, sha256_bytes(payload)


def validate_consent(consent_path: Path, proof: dict[str, Any], plan_sha256: str) -> dict[str, Any]:
    consent = exact_object(load_json(consent_path, "Cleanup consent"), CONSENT_KEYS, "Cleanup consent")
    if consent["schemaVersion"] != SCHEMA_VERSION:
        raise CleanupRefusal("Cleanup consent schema version is unsupported")
    if consent["operatorConsent"] != CONSENT_TEXT:
        raise CleanupRefusal("Cleanup consent phrase is absent")
    for key in ("preservedDir", "preservedPatchCount", "preservedManifestSha256"):
        if consent[key] != proof[key]:
            raise CleanupRefusal(f"Cleanup consent does not match archive proof field {key}")
    if consent["cleanupPlanSha256"] != plan_sha256:
        raise CleanupRefusal("Cleanup consent does not match cleanup plan")
    if not isinstance(consent["authorizedAtUtc"], str) or not consent["authorizedAtUtc"].endswith("Z"):
        raise CleanupRefusal("Cleanup consent timestamp is invalid")
    return consent


def safe_target(allowed_root: Path, relative_path: str) -> Path:
    parsed = Path(relative_path)
    if parsed.is_absolute():
        raise CleanupRefusal("Cleanup target path must be relative")
    root = allowed_root.resolve()
    if any(part in {"", ".", ".."} for part in parsed.parts):
        raise CleanupRefusal("Cleanup target path must be normalized")
    target = root.joinpath(*parsed.parts)
    if target == root:
        raise CleanupRefusal("Cleanup target may not be the allowed root")
    return target


def refuse_symlink_chain(allowed_root: Path, target: Path, index: int) -> None:
    current = allowed_root.resolve()
    for part in target.relative_to(current).parts:
        current = current / part
        if current.is_symlink():
            raise CleanupRefusal(f"Cleanup target {index} crosses a symlink")


def execute_cleanup(
    *,
    preserved_dir: Path,
    plan_path: Path,
    consent_path: Path,
    quarantine_dir: Path,
    receipt_path: Path,
    min_patches: int = DEFAULT_MIN_PATCHES,
) -> dict[str, Any]:
    proof = archive_proof(preserved_dir, min_patches)
    plan, plan_sha256 = load_plan(plan_path)
    validate_consent(consent_path, proof, plan_sha256)
    allowed_root = Path(plan["allowedRoot"]).resolve()
    if not allowed_root.is_dir():
        raise CleanupRefusal("Cleanup allowed root is absent")
    quarantine = quarantine_dir.resolve()
    if allowed_root == quarantine or allowed_root in quarantine.parents:
        raise CleanupRefusal("Cleanup quarantine must not live inside the allowed root")

    planned_moves = []
    moves = []
    for index, action in enumerate(plan["actions"]):
        target = safe_target(allowed_root, action["path"])
        refuse_symlink_chain(allowed_root, target, index)
        if not target.exists():
            moves.append({"path": action["path"], "status": "alreadyAbsent"})
            continue
        if action["action"] == "removeFile" and not target.is_file():
            raise CleanupRefusal(f"Cleanup target {index} is not a file")
        if action["action"] == "removeEmptyDirectory":
            if not target.is_dir():
                raise CleanupRefusal(f"Cleanup target {index} is not a directory")
            if any(target.iterdir()):
                raise CleanupRefusal(f"Cleanup target {index} directory is not empty")
        destination = quarantine / target.relative_to(allowed_root)
        if destination.exists():
            raise CleanupRefusal(f"Cleanup quarantine destination already exists for target {index}")
        planned_moves.append((action, target, destination))

    quarantine.mkdir(parents=True, exist_ok=True)
    for action, target, destination in planned_moves:
        destination.parent.mkdir(parents=True, exist_ok=True)
        os.replace(target, destination)
        moves.append(
            {
                "path": action["path"],
                "status": "quarantined",
                "quarantinePath": str(destination),
            }
        )

    receipt = {
        "schemaVersion": SCHEMA_VERSION,
        "preservedDir": proof["preservedDir"],
        "preservedPatchCount": proof["preservedPatchCount"],
        "preservedManifestSha256": proof["preservedManifestSha256"],
        "cleanupPlanSha256": plan_sha256,
        "allowedRoot": str(allowed_root),
        "quarantineDir": str(quarantine),
        "moves": moves,
    }
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return receipt


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preserved-dir", required=True, type=Path)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--consent", required=True, type=Path)
    parser.add_argument("--quarantine", required=True, type=Path)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--min-patches", default=DEFAULT_MIN_PATCHES, type=int)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        execute_cleanup(
            preserved_dir=args.preserved_dir,
            plan_path=args.plan,
            consent_path=args.consent,
            quarantine_dir=args.quarantine,
            receipt_path=args.receipt,
            min_patches=args.min_patches,
        )
    except CleanupRefusal as exc:
        print(f"REFUSED: {exc}", file=sys.stderr)
        return 2
    return 0


class SafeStorageCleanupTests(unittest.TestCase):
    def write_archive(self, root: Path, count: int = 3) -> Path:
        preserved = root / "preserved"
        preserved.mkdir()
        for index in range(count):
            (preserved / f"worktree-{index}.patch").write_text(f"diff {index}\n", encoding="utf-8")
        return preserved

    def write_plan(self, root: Path, allowed_root: Path, actions: list[dict[str, str]]) -> Path:
        plan = root / "plan.json"
        plan.write_text(
            json.dumps(
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "allowedRoot": str(allowed_root),
                    "actions": actions,
                }
            ),
            encoding="utf-8",
        )
        return plan

    def write_consent(self, root: Path, preserved: Path, plan: Path, min_patches: int = 3) -> Path:
        consent = root / "consent.json"
        proof = archive_proof(preserved, min_patches)
        consent.write_text(
            json.dumps(
                {
                    "schemaVersion": SCHEMA_VERSION,
                    "operatorConsent": CONSENT_TEXT,
                    "preservedDir": proof["preservedDir"],
                    "preservedPatchCount": proof["preservedPatchCount"],
                    "preservedManifestSha256": proof["preservedManifestSha256"],
                    "cleanupPlanSha256": sha256_bytes(plan.read_bytes()),
                    "authorizedAtUtc": "2026-07-30T00:00:00Z",
                }
            ),
            encoding="utf-8",
        )
        return consent

    def test_executes_recoverable_file_cleanup_after_matching_archive_and_consent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root)
            allowed = root / "scratch"
            allowed.mkdir()
            target = allowed / "old.log"
            target.write_text("reproducible\n", encoding="utf-8")
            plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": "old.log", "reason": "reproducible artifact"}],
            )
            consent = self.write_consent(root, preserved, plan)
            receipt = execute_cleanup(
                preserved_dir=preserved,
                plan_path=plan,
                consent_path=consent,
                quarantine_dir=root / "quarantine",
                receipt_path=root / "receipt.json",
                min_patches=3,
            )
            self.assertFalse(target.exists())
            self.assertEqual((root / "quarantine" / "old.log").read_text(encoding="utf-8"), "reproducible\n")
            self.assertEqual(receipt["moves"][0]["status"], "quarantined")

    def test_missing_consent_refuses_before_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root)
            allowed = root / "scratch"
            allowed.mkdir()
            target = allowed / "old.log"
            target.write_text("keep\n", encoding="utf-8")
            plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": "old.log", "reason": "reproducible artifact"}],
            )
            with self.assertRaises(CleanupRefusal):
                execute_cleanup(
                    preserved_dir=preserved,
                    plan_path=plan,
                    consent_path=root / "missing-consent.json",
                    quarantine_dir=root / "quarantine",
                    receipt_path=root / "receipt.json",
                    min_patches=3,
                )
            self.assertEqual(target.read_text(encoding="utf-8"), "keep\n")

    def test_bad_archive_proof_refuses_before_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root, count=2)
            allowed = root / "scratch"
            allowed.mkdir()
            target = allowed / "old.log"
            target.write_text("keep\n", encoding="utf-8")
            plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": "old.log", "reason": "reproducible artifact"}],
            )
            consent = root / "consent.json"
            consent.write_text("{}", encoding="utf-8")
            with self.assertRaises(CleanupRefusal):
                execute_cleanup(
                    preserved_dir=preserved,
                    plan_path=plan,
                    consent_path=consent,
                    quarantine_dir=root / "quarantine",
                    receipt_path=root / "receipt.json",
                    min_patches=3,
                )
            self.assertTrue(target.exists())

    def test_symlink_target_refuses_before_following_link(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root)
            allowed = root / "scratch"
            allowed.mkdir()
            outside = root / "outside.txt"
            outside.write_text("outside\n", encoding="utf-8")
            link = allowed / "link.txt"
            link.symlink_to(outside)
            plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": "link.txt", "reason": "stale link"}],
            )
            consent = self.write_consent(root, preserved, plan)
            with self.assertRaises(CleanupRefusal):
                execute_cleanup(
                    preserved_dir=preserved,
                    plan_path=plan,
                    consent_path=consent,
                    quarantine_dir=root / "quarantine",
                    receipt_path=root / "receipt.json",
                    min_patches=3,
                )
            self.assertEqual(outside.read_text(encoding="utf-8"), "outside\n")
            self.assertTrue(link.is_symlink())

    def test_plan_paths_must_be_relative_and_not_globs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            allowed = root / "scratch"
            allowed.mkdir()
            absolute = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": str(allowed / "old.log"), "reason": "bad"}],
            )
            with self.assertRaises(CleanupRefusal):
                safe_target(allowed, json.loads(absolute.read_text(encoding="utf-8"))["actions"][0]["path"])
            glob_plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeFile", "path": "*.log", "reason": "bad"}],
            )
            with self.assertRaises(CleanupRefusal):
                load_plan(glob_plan)

    def test_non_empty_directory_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root)
            allowed = root / "scratch"
            nested = allowed / "cache"
            nested.mkdir(parents=True)
            (nested / "file.txt").write_text("keep\n", encoding="utf-8")
            plan = self.write_plan(
                root,
                allowed,
                [{"action": "removeEmptyDirectory", "path": "cache", "reason": "empty cache dir"}],
            )
            consent = self.write_consent(root, preserved, plan)
            with self.assertRaises(CleanupRefusal):
                execute_cleanup(
                    preserved_dir=preserved,
                    plan_path=plan,
                    consent_path=consent,
                    quarantine_dir=root / "quarantine",
                    receipt_path=root / "receipt.json",
                    min_patches=3,
                )
            self.assertTrue((nested / "file.txt").exists())

    def test_later_bad_action_refuses_without_partial_move(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            preserved = self.write_archive(root)
            allowed = root / "scratch"
            bad_dir = allowed / "cache"
            bad_dir.mkdir(parents=True)
            target = allowed / "old.log"
            target.write_text("keep\n", encoding="utf-8")
            (bad_dir / "file.txt").write_text("still here\n", encoding="utf-8")
            plan = self.write_plan(
                root,
                allowed,
                [
                    {"action": "removeFile", "path": "old.log", "reason": "reproducible artifact"},
                    {"action": "removeEmptyDirectory", "path": "cache", "reason": "empty cache dir"},
                ],
            )
            consent = self.write_consent(root, preserved, plan)
            with self.assertRaises(CleanupRefusal):
                execute_cleanup(
                    preserved_dir=preserved,
                    plan_path=plan,
                    consent_path=consent,
                    quarantine_dir=root / "quarantine",
                    receipt_path=root / "receipt.json",
                    min_patches=3,
                )
            self.assertEqual(target.read_text(encoding="utf-8"), "keep\n")
            self.assertFalse((root / "quarantine").exists())


if __name__ == "__main__":
    raise SystemExit(main())
