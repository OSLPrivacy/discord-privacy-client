#!/usr/bin/env python3
"""Fail closed when the OSL Privacy updater supply-chain policy drifts."""

from __future__ import annotations

import json
import re
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))
from audit_reproducible_build import audit_workflow as audit_reproducible_build_workflow


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "osl-hub-release.yml"
PROMOTION_WORKFLOW = ROOT / ".github" / "workflows" / "osl-hub-promote.yml"
HUB_CONFIG = ROOT / "apps" / "osl-hub" / "tauri.conf.json"
ORIGINAL_CONFIG = ROOT / "src-tauri" / "tauri.conf.json"
PINNED_ACTION = re.compile(r"^\s*-?\s*uses:\s*[^\s@]+@([0-9a-f]{40})\s*$", re.MULTILINE)
ANY_ACTION = re.compile(r"^\s*-?\s*uses:\s*[^\s@]+@([^\s#]+)", re.MULTILINE)
AZURE_LONG_LIVED_SECRET = re.compile(
    r"\b(?:AZURE_(?:CLIENT_SECRET|CLIENT_CERTIFICATE|PASSWORD)|ARM_CLIENT_SECRET)\b",
    re.IGNORECASE,
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def private_key_paths(root: Path) -> list[Path]:
    return [
        path for path in root.rglob("*")
        if path.is_file()
        and ".git" not in path.parts
        and (path.name.endswith(".key") or "signing-private" in path.name.lower())
    ]


def require_object(value: object, message: str) -> dict[str, object]:
    require(isinstance(value, dict), message)
    return value


def require_before(text: str, earlier: str, later: str, message: str) -> None:
    earlier_index = text.find(earlier)
    later_index = text.find(later)
    require(earlier_index >= 0 and later_index >= 0 and earlier_index < later_index, message)


def require_not_before(text: str, needle: str, boundary: str, message: str) -> None:
    needle_index = text.find(needle)
    boundary_index = text.find(boundary)
    require(boundary_index >= 0, message)
    require(needle_index < 0 or boundary_index < needle_index, message)


def audit_release_policy(
    workflow: str,
    promotion: str,
    hub: dict[str, object],
    original: dict[str, object],
    root: Path,
) -> None:
    require(
        not AZURE_LONG_LIVED_SECRET.search(workflow + "\n" + promotion),
        "OSL Privacy release workflows must not contain Azure client secrets; use tag-scoped OIDC when code signing is enabled",
    )
    refs = ANY_ACTION.findall(workflow + "\n" + promotion)
    pins = PINNED_ACTION.findall(workflow + "\n" + promotion)
    require(refs and len(refs) == len(pins), "OSL Privacy release actions must use full commit SHAs")
    require("environment: hub-release" in workflow, "OSL Privacy signing requires hub-release environment approval")
    require("if: startsWith(github.ref, 'refs/tags/hub-v')" in workflow,
            "OSL Privacy release job must reject branch and unscoped manual dispatches")
    require('if ("${{ github.ref_name }}" -ne $tag)' in workflow,
            "OSL Privacy release tag must exactly match the configured application version")
    require("ref: ${{ github.ref }}" in workflow,
            "OSL Privacy release checkout must bind to the exact triggering tag ref")
    require("HUB_TAURI_SIGNING_PRIVATE_KEY" in workflow, "OSL Privacy release must use its dedicated signing secret")
    require("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}" not in workflow,
            "OSL Privacy release must not use the original client's signing secret")
    require("releaseDraft: true" in workflow,
            "Signed OSL Privacy candidates must remain draft until clean-VM QA")
    require("releaseDraft: false" not in workflow,
            "Candidate workflow must never publish a release directly")
    require("gh release upload hub-latest" not in workflow,
            "Candidate workflow must never move the stable updater feed")
    require("python scripts/audit_hub_release.py" in workflow,
            "OSL Privacy release must run the supply-chain audit before signing")
    require("python -m unittest scripts/audit_hub_release.py" in workflow,
            "OSL Privacy release must run the release-audit tests before signing")
    require_before(workflow,
                   "Audit OSL Privacy updater supply-chain policy",
                   "Build signed draft installer and updater manifest",
                   "OSL Privacy supply-chain audit must run before candidate signing")
    require_before(workflow,
                   "Audit OSL Privacy updater supply-chain policy",
                   "tauri-apps/tauri-action@",
                   "OSL Privacy supply-chain audit must run before the signing action")
    require_not_before(workflow,
                       "HUB_TAURI_SIGNING_PRIVATE_KEY",
                       "Audit OSL Privacy updater supply-chain policy",
                       "OSL Privacy signing secret must not be reachable before the supply-chain audit")
    require("scripts/normalize_updater_manifest.py" in workflow,
            "OSL Privacy release must normalize the updater manifest the promotion gate reads")
    require_before(workflow,
                   "tauri-apps/tauri-action@",
                   "scripts/normalize_updater_manifest.py",
                   "OSL Privacy updater manifest can only be normalized after it is built")
    require("scripts/verify_update_feed_acceptance.py" in workflow,
            "OSL Privacy release must prove an installed client would accept the manifest")

    require("on:\n  workflow_dispatch:" in promotion,
            "OSL Privacy promotion must be a separate manual workflow")
    require("environment: hub-vm-qa" in promotion,
            "OSL Privacy promotion requires protected hub-vm-qa approval")
    require("hub-vm-qa-attestation.json" in promotion,
            "OSL Privacy promotion must download the two-VM QA attestation")
    require("verify_hub_vm_qa_attestation.py" in promotion,
            "OSL Privacy promotion must verify the exact candidate attestation")
    require('--json isDraft --jq .isDraft' in promotion,
            "OSL Privacy promotion must accept draft candidates only")
    require('gh release edit "$CANDIDATE_TAG" --draft=false' in promotion,
            "Only the promotion workflow may publish the attested draft")
    require("gh release upload hub-latest candidate/latest.json --clobber" in promotion,
            "Only verified promotion may move the app updater feed")
    require("scripts/check_bootstrap_waiver_allowed.py" in promotion,
            "OSL Privacy promotion must refuse a bootstrap QA waiver once the feed exists")
    require_before(promotion,
                   "scripts/check_bootstrap_waiver_allowed.py",
                   'gh release edit "$CANDIDATE_TAG" --draft=false',
                   "The bootstrap waiver guard must run before anything is published")
    require("scripts/verify_update_feed_acceptance.py" in promotion,
            "OSL Privacy promotion must prove an installed client would accept the manifest")
    require_before(promotion,
                   "scripts/verify_update_feed_acceptance.py",
                   "gh release upload hub-latest candidate/latest.json --clobber",
                   "Client acceptance must be proven before the update feed moves")
    repro_errors = audit_reproducible_build_workflow()
    require(not repro_errors,
            "OSL Privacy reproducible-build workflow drifted: " + "; ".join(repro_errors))

    hub_plugins = require_object(hub.get("plugins"), "OSL Privacy plugin config must be an object")
    updater = require_object(hub_plugins.get("updater"), "OSL Privacy updater config must be an object")
    endpoints = updater.get("endpoints")
    require(endpoints == [
        "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json"
    ], "OSL Privacy updater endpoint must be the product-specific hub-latest feed")
    require(bool(updater.get("pubkey")), "OSL Privacy updater public key is missing")
    original_plugins = require_object(original.get("plugins"), "Original plugin config must be an object")
    original_updater = require_object(original_plugins.get("updater"), "Original updater config must be an object")
    original_key = original_updater.get("pubkey")
    require(updater["pubkey"] != original_key, "OSL Privacy and original client must not share an updater signing key")

    forbidden = private_key_paths(root)
    require(not forbidden, f"Private updater key material is present in the repository: {forbidden}")


def main() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    promotion = PROMOTION_WORKFLOW.read_text(encoding="utf-8")
    hub = json.loads(HUB_CONFIG.read_text(encoding="utf-8"))
    original = json.loads(ORIGINAL_CONFIG.read_text(encoding="utf-8"))
    audit_release_policy(workflow, promotion, hub, original, ROOT)


class HubReleaseAuditTests(unittest.TestCase):
    def fixture(self) -> tuple[str, str, dict[str, object], dict[str, object], Path]:
        workflow = """
jobs:
  windows-release:
    if: startsWith(github.ref, 'refs/tags/hub-v')
    runs-on: windows-latest
    environment: hub-release
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          ref: ${{ github.ref }}
      - name: Resolve and verify the app release version
        run: |
          if ("${{ github.ref_name }}" -ne $tag) {
            throw "Tag mismatch"
          }
      - name: Audit OSL Privacy updater supply-chain policy
        run: |
          python scripts/audit_hub_release.py
          python -m unittest scripts/audit_hub_release.py
      - name: Build signed draft installer and updater manifest
        uses: tauri-apps/tauri-action@1deb371b0cd8bd54025b384f1cd735e725c4060f
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.HUB_TAURI_SIGNING_PRIVATE_KEY }}
        with:
          releaseDraft: true
      - name: Normalize updater manifest
        run: |
          python scripts/normalize_updater_manifest.py --manifest candidate/latest.json
          python scripts/verify_update_feed_acceptance.py --manifest candidate/latest.json
""".strip()
        promotion = """
on:
  workflow_dispatch:
jobs:
  promote-tested-candidate:
    environment: hub-vm-qa
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
      - run: gh release view "$CANDIDATE_TAG" --json isDraft --jq .isDraft
      - run: gh release download "$CANDIDATE_TAG" --pattern hub-vm-qa-attestation.json --dir candidate
      - run: python scripts/verify_hub_vm_qa_attestation.py
      - run: python scripts/check_bootstrap_waiver_allowed.py --attestation candidate/hub-vm-qa-attestation.json
      - run: python scripts/verify_update_feed_acceptance.py --manifest candidate/latest.json
      - run: gh release edit "$CANDIDATE_TAG" --draft=false
      - run: gh release upload hub-latest candidate/latest.json --clobber
""".strip()
        hub = {
            "plugins": {
                "updater": {
                    "endpoints": [
                        "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json"
                    ],
                    "pubkey": "hub-public-key",
                }
            }
        }
        original = {"plugins": {"updater": {"pubkey": "original-public-key"}}}
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        return workflow, promotion, hub, original, Path(temporary.name)

    def test_accepts_exact_tag_draft_candidate_policy(self) -> None:
        audit_release_policy(*self.fixture())

    def test_rejects_checkout_not_bound_to_triggering_tag_ref(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = workflow.replace("          ref: ${{ github.ref }}\n", "")
        with self.assertRaises(SystemExit):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_azure_client_secret_in_release_workflow(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow += "\n        env:\n          AZURE_CLIENT_SECRET: ${{ secrets.AZURE_CLIENT_SECRET }}"
        with self.assertRaisesRegex(SystemExit, "Azure client secrets"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_candidate_workflow_that_publishes_directly(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = workflow.replace("releaseDraft: true", "releaseDraft: false")
        with self.assertRaises(SystemExit):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_stable_feed_mutation_from_candidate_workflow(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = workflow + "\n      - run: gh release upload hub-latest latest.json"
        with self.assertRaises(SystemExit):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_a_release_that_ships_the_raw_tauri_action_manifest(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = workflow.replace("python scripts/normalize_updater_manifest.py --manifest candidate/latest.json\n", "")
        with self.assertRaisesRegex(SystemExit, "normalize the updater manifest"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_normalization_ordered_before_the_manifest_is_built(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = "      - run: python scripts/normalize_updater_manifest.py\n" + workflow
        with self.assertRaisesRegex(SystemExit, "only be normalized after it is built"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_promotion_without_the_bootstrap_waiver_guard(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        promotion = promotion.replace(
            "      - run: python scripts/check_bootstrap_waiver_allowed.py --attestation candidate/hub-vm-qa-attestation.json\n",
            "",
        )
        with self.assertRaisesRegex(SystemExit, "refuse a bootstrap QA waiver"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_a_waiver_guard_that_runs_after_publication(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        guard = "      - run: python scripts/check_bootstrap_waiver_allowed.py --attestation candidate/hub-vm-qa-attestation.json\n"
        promotion = promotion.replace(guard, "") + "\n" + guard
        with self.assertRaisesRegex(SystemExit, "before anything is published"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_rejects_moving_the_feed_before_proving_client_acceptance(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        check = "      - run: python scripts/verify_update_feed_acceptance.py --manifest candidate/latest.json\n"
        promotion = promotion.replace(check, "") + "\n" + check
        with self.assertRaisesRegex(SystemExit, "before the update feed moves"):
            audit_release_policy(workflow, promotion, hub, original, root)

    def test_refuses_release_supply_chain_drift_before_any_candidate_is_signed(self) -> None:
        workflow, promotion, hub, original, root = self.fixture()
        workflow = workflow.replace(
            "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5",
            "actions/checkout@v4",
        )

        with self.assertRaisesRegex(SystemExit, "full commit SHAs"):
            audit_release_policy(workflow, promotion, hub, original, root)


def _scripts_audit_hub_release_py(self: HubReleaseAuditTests) -> None:
    workflow, promotion, hub, original, root = self.fixture()
    audit_release_policy(workflow, promotion, hub, original, root)

    def assert_rejects(
        mutant: tuple[str, str, dict[str, object], dict[str, object], Path],
        expected: str,
    ) -> None:
        with self.assertRaises(SystemExit) as raised:
            audit_release_policy(*mutant)
        self.assertIn(expected, str(raised.exception))

    mutants = [
        (
            workflow.replace(
                "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5",
                "actions/checkout@v4",
                1,
            ),
            promotion,
            hub,
            original,
            root,
            "full commit SHAs",
        ),
        (
            workflow.replace(
                """      - name: Audit OSL Privacy updater supply-chain policy
        run: |
          python scripts/audit_hub_release.py
          python -m unittest scripts/audit_hub_release.py
""",
                "",
            )
            + """
      - name: Audit OSL Privacy updater supply-chain policy
        run: |
          python scripts/audit_hub_release.py
          python -m unittest scripts/audit_hub_release.py
""",
            promotion,
            hub,
            original,
            root,
            "must run before candidate signing",
        ),
        (
            workflow.replace("releaseDraft: true", "releaseDraft: false"),
            promotion,
            hub,
            original,
            root,
            "must remain draft",
        ),
        (
            workflow,
            promotion,
            {"plugins": {"updater": {
                "endpoints": [
                    "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json"
                ],
                "pubkey": "original-public-key",
            }}},
            original,
            root,
            "must not share an updater signing key",
        ),
        (
            workflow.replace(
                "      - name: Resolve and verify the app release version\n",
                """      - name: Preload hub signing secret too early
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.HUB_TAURI_SIGNING_PRIVATE_KEY }}
        run: echo preflight
      - name: Resolve and verify the app release version
""",
            ),
            promotion,
            hub,
            original,
            root,
            "signing secret must not be reachable before the supply-chain audit",
        ),
    ]
    for *mutant, expected in mutants:
        assert_rejects(tuple(mutant), expected)


setattr(HubReleaseAuditTests, "scripts/audit_hub_release.py", _scripts_audit_hub_release_py)


def _refuse_release_supply_chain_drift_before_any_candidate_is_signed(
    self: HubReleaseAuditTests,
) -> None:
    workflow, promotion, hub, original, root = self.fixture()
    audit_release_policy(workflow, promotion, hub, original, root)

    audit_step = """      - name: Audit OSL Privacy updater supply-chain policy
        run: |
          python scripts/audit_hub_release.py
          python -m unittest scripts/audit_hub_release.py
"""
    signing_step = "      - name: Build signed draft installer and updater manifest"
    drifted_workflow = workflow.replace(audit_step, "", 1) + "\n" + audit_step
    self.assertIn(signing_step, drifted_workflow)

    with self.assertRaises(SystemExit) as raised:
        audit_release_policy(drifted_workflow, promotion, hub, original, root)
    self.assertIn("must run before candidate signing", str(raised.exception))


setattr(
    HubReleaseAuditTests,
    "Refuse release supply-chain drift before any candidate is signed.",
    _refuse_release_supply_chain_drift_before_any_candidate_is_signed,
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    suite = unittest.TestSuite()
    suite.addTests(tests)
    suite.addTest(HubReleaseAuditTests("scripts/audit_hub_release.py"))
    suite.addTest(
        HubReleaseAuditTests(
            "Refuse release supply-chain drift before any candidate is signed."
        )
    )
    return suite


if __name__ == "__main__":
    main()
