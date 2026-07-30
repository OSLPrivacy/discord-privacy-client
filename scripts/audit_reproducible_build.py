#!/usr/bin/env python3
"""Static contract audit for the OSL Privacy reproducible-build workflow."""

from __future__ import annotations

import argparse
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "reproducible-build.yml"


def require(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def audit_text(text: str) -> list[str]:
    errors: list[str] = []
    require(
        "push:\n    branches:\n      - main\n    tags:\n      - \"hub-v*\"" in text,
        "workflow must run on main pushes and hub-v* tags",
        errors,
    )
    require("candidate_tag:" in text, "manual dispatch must accept an explicit candidate tag", errors)
    require("permissions:\n  contents: read" in text, "workflow permissions must be read-only", errors)
    require("fetch-depth: 0" in text, "workflow must fetch the full tag graph", errors)
    require(
        "candidate_tag must be a bounded hub-v* tag" in text
        and "^hub-v[0-9A-Za-z.+-]{1,64}$" in text,
        "candidate tags must be bounded and product-scoped",
        errors,
    )
    require(
        "git rev-list -n 1 \"$candidateTag^{commit}\"" in text
        and "git checkout --detach $tagCommit" in text,
        "workflow must checkout the exact release tag commit before building",
        errors,
    )
    require(
        "hub-v$($config.version)" in text
        and "does not match source version" in text,
        "release tag must match apps/osl-hub/tauri.conf.json",
        errors,
    )
    require(
        "gh release view $candidateTag --json tagName --jq .tagName" in text,
        "workflow must verify the GitHub release tag binding",
        errors,
    )
    require(
        "gh release download $env:CANDIDATE_TAG --pattern \"*.exe\" --dir candidate" in text
        and "release must contain exactly one Windows installer" in text,
        "workflow must download exactly one released Windows installer",
        errors,
    )
    require(
        "7z x \"$env:RELEASE_INSTALLER\"" in text
        and "-Filter \"osl-privacy-hub.exe\"" in text
        and "must contain exactly one OSL Privacy Hub executable" in text,
        "workflow must extract exactly one released Hub executable",
        errors,
    )
    require(
        "working-directory: apps/osl-hub-ui" in text
        and "npm ci" in text
        and "npm run build" in text,
        "workflow must rebuild the committed frontend first",
        errors,
    )
    require(
        "--manifest-path apps/osl-hub/Cargo.toml" in text
        and "--release --features desktop --bin osl-privacy-hub" in text,
        "workflow must rebuild the Hub executable with the release feature set",
        errors,
    )
    require(
        "RUNNER_TEMP" in text
        and "reproducible build target directory already exists" in text
        and "CARGO_TARGET_DIR" in text,
        "workflow must use a born-empty runner-owned target directory",
        errors,
    )
    require(
        "$env:RELEASED_EXE_SHA256 -cne $env:REBUILT_EXE_SHA256" in text
        and "released executable bytes do not reproduce from exact source" in text
        and "released sha256=$env:RELEASED_EXE_SHA256 rebuilt sha256=$env:REBUILT_EXE_SHA256" in text,
        "workflow must compare released and rebuilt bytes by digest and report both digests",
        errors,
    )
    require(
        "reproducible-build-proof.json" in text
        and "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02" in text,
        "workflow must retain a pinned proof artifact",
        errors,
    )
    require(
        "discord-privacy-client.exe" not in text
        and "release-deterministic" not in text
        and "Hash artifact" not in text,
        "workflow must not use the stale legacy executable proof",
        errors,
    )

    unpinned_actions = []
    for number, line in enumerate(text.splitlines(), 1):
        if "uses:" not in line:
            continue
        if not re.match(r"^\s*-?\s*uses:\s*[^#\s]+@[0-9a-f]{40}\s*(?:#.*)?$", line):
            unpinned_actions.append(number)
    require(not unpinned_actions, f"GitHub Actions must be pinned: {unpinned_actions}", errors)
    return errors


def audit_workflow(path: Path = WORKFLOW) -> list[str]:
    return audit_text(path.read_text(encoding="utf-8"))


class ReproducibleBuildWorkflowTests(unittest.TestCase):
    def workflow(self) -> str:
        return WORKFLOW.read_text(encoding="utf-8")

    def test_current_workflow_satisfies_contract(self) -> None:
        self.assertEqual(audit_workflow(), [])

    def test_refuses_branch_head_build_as_release_source(self) -> None:
        mutant = self.workflow().replace("git checkout --detach $tagCommit", "# checkout omitted")
        self.assertIn(
            "workflow must checkout the exact release tag commit before building",
            audit_text(mutant),
        )

    def test_refuses_legacy_double_build_hash_workflow(self) -> None:
        legacy = """
name: Reproducible Build
jobs:
  build:
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
      - name: Build deterministic
        run: cargo build --profile release-deterministic
      - name: Hash artifact
        run: Get-FileHash target\\release-deterministic\\discord-privacy-client.exe
"""
        errors = audit_text(legacy)
        self.assertIn("workflow must not use the stale legacy executable proof", errors)
        self.assertIn(
            "workflow must compare released and rebuilt bytes by digest and report both digests",
            errors,
        )

    def test_refuses_missing_release_download(self) -> None:
        mutant = self.workflow().replace(
            "gh release download $env:CANDIDATE_TAG --pattern \"*.exe\" --dir candidate",
            "# release download omitted",
        )
        self.assertIn(
            "workflow must download exactly one released Windows installer",
            audit_text(mutant),
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ReproducibleBuildWorkflowTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1

    errors = audit_workflow()
    if errors:
        print("Reproducible-build workflow audit failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("OK: reproducible-build workflow binds released bytes to the exact Hub source tag")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
