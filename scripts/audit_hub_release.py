#!/usr/bin/env python3
"""Fail closed when the OSL Privacy updater supply-chain policy drifts."""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "osl-hub-release.yml"
PROMOTION_WORKFLOW = ROOT / ".github" / "workflows" / "osl-hub-promote.yml"
HUB_CONFIG = ROOT / "apps" / "osl-hub" / "tauri.conf.json"
ORIGINAL_CONFIG = ROOT / "src-tauri" / "tauri.conf.json"
PINNED_ACTION = re.compile(r"^\s*-?\s*uses:\s*[^\s@]+@([0-9a-f]{40})\s*$", re.MULTILINE)
ANY_ACTION = re.compile(r"^\s*-?\s*uses:\s*[^\s@]+@([^\s#]+)", re.MULTILINE)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def main() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    promotion = PROMOTION_WORKFLOW.read_text(encoding="utf-8")
    hub = json.loads(HUB_CONFIG.read_text(encoding="utf-8"))
    original = json.loads(ORIGINAL_CONFIG.read_text(encoding="utf-8"))

    refs = ANY_ACTION.findall(workflow + "\n" + promotion)
    pins = PINNED_ACTION.findall(workflow + "\n" + promotion)
    require(refs and len(refs) == len(pins), "OSL Privacy release actions must use full commit SHAs")
    require("environment: hub-release" in workflow, "OSL Privacy signing requires hub-release environment approval")
    require("if: startsWith(github.ref, 'refs/tags/hub-v')" in workflow,
            "OSL Privacy release job must reject branch and unscoped manual dispatches")
    require('if ("${{ github.ref_name }}" -ne $tag)' in workflow,
            "OSL Privacy release tag must exactly match the configured application version")
    require("HUB_TAURI_SIGNING_PRIVATE_KEY" in workflow, "OSL Privacy release must use its dedicated signing secret")
    require("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}" not in workflow,
            "OSL Privacy release must not use the original client's signing secret")
    require("releaseDraft: true" in workflow,
            "Signed OSL Privacy candidates must remain draft until clean-VM QA")
    require("releaseDraft: false" not in workflow,
            "Candidate workflow must never publish a release directly")
    require("gh release upload hub-latest" not in workflow,
            "Candidate workflow must never move the stable updater feed")

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

    updater = hub.get("plugins", {}).get("updater", {})
    endpoints = updater.get("endpoints")
    require(endpoints == [
        "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-latest/latest.json"
    ], "OSL Privacy updater endpoint must be the product-specific hub-latest feed")
    require(bool(updater.get("pubkey")), "OSL Privacy updater public key is missing")
    original_key = original.get("plugins", {}).get("updater", {}).get("pubkey")
    require(updater["pubkey"] != original_key, "OSL Privacy and original client must not share an updater signing key")

    forbidden = [
        path for path in ROOT.rglob("*")
        if path.is_file()
        and ".git" not in path.parts
        and (path.name.endswith(".key") or "signing-private" in path.name.lower())
    ]
    require(not forbidden, f"Private updater key material is present in the repository: {forbidden}")


if __name__ == "__main__":
    main()
