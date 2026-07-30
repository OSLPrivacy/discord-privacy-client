#!/usr/bin/env python3
"""Fail a public release when tracked files violate OSL's trust boundary."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

FORBIDDEN_TRACKED = {
    ".claude/settings.local.json",
    "KEYVAULT.md",
}
FORBIDDEN_SUFFIXES = {
    ".db",
    ".env",
    ".key",
    ".log",
    ".p12",
    ".pem",
    ".pfx",
    ".sqlite",
}
SECRET_PATTERNS = {
    "AWS access key": re.compile(rb"AKIA[0-9A-Z]{16}"),
    "GitHub token": re.compile(rb"gh[pousr]_[A-Za-z0-9_]{20,}"),
    "Google API key": re.compile(rb"AIza[0-9A-Za-z_-]{30,}"),
    "private key": re.compile(rb"-----BEGIN (?:(?:RSA|EC|OPENSSH|DSA|ENCRYPTED) )?PRIVATE KEY-----"),
    "Slack token": re.compile(rb"xox[baprs]-[A-Za-z0-9-]{10,}"),
    "Stripe secret": re.compile(rb"(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}"),
    "Stripe webhook secret": re.compile(rb"whsec_[A-Za-z0-9]{16,}"),
    "Telegram bot token": re.compile(rb"\b[0-9]{8,10}:[A-Za-z0-9_-]{35,}\b"),
    "Cloudflare API token": re.compile(
        rb"(?i)\b(?:CLOUDFLARE_API_TOKEN|CF_API_TOKEN)\b\s*[:=]\s*['\"]?(?!test|example|your)[A-Za-z0-9_-]{30,}"
    ),
    "Azure client secret": re.compile(
        rb"(?i)\bAZURE_(?:CLIENT_SECRET|STORAGE_KEY)\b\s*[:=]\s*['\"]?(?!test|example|your)[A-Za-z0-9_~.\-/+=]{24,}"
    ),
    "Discord token": re.compile(
        rb"(?:\bmfa\.[A-Za-z0-9_-]{70,}|\b[MN][A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{6}\.[A-Za-z0-9_-]{25,})"
    ),
    "long bearer credential": re.compile(
        rb"(?i)\bBearer\s+(?!test|wrong|expected|example)[A-Za-z0-9_~.\-/+=]{48,}"
    ),
    "Bitcoin private key": re.compile(rb"\b[5KL][1-9A-HJ-NP-Za-km-z]{50,51}\b"),
    "Monero private spend key": re.compile(
        rb"(?i)\b(?:MONERO_(?:PRIVATE_)?SPEND_KEY|XMR_SPEND_KEY)\b\s*[:=]\s*['\"]?[0-9a-f]{64}\b"
    ),
}
PERSONAL_PATH_PATTERNS = {
    "personal Windows path": re.compile(rb"(?i)[A-Z]:\\Users\\liamw(?:\\|/|\b)"),
    "personal WSL path": re.compile(rb"/(?:home|mnt/c/Users)/liamw(?:/|\b)"),
}
RELEASE_IDENTITY_PATH = ROOT / "docs/evidence/public-release/released-binary.json"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_OBJECT_RE = re.compile(r"^[0-9a-f]{40}$")
RELEASE_CONTEXT_RE = re.compile(
    r"\b(?:release|released|shipping|production|binary|build|artifact|candidate|executable|app)\b",
    re.I,
)
RELEASE_TAG_RE = re.compile(r"\bv[0-9][0-9A-Za-z._-]*\b")
HEX64_RE = re.compile(r"\b[0-9a-fA-F]{64}\b")
GIT_OBJECT_CLAIM_RE = re.compile(
    r"(?<![0-9a-fA-F])[0-9a-fA-F]{40}(?![0-9a-fA-F])"
)
BINARY_DIGEST_CONTEXT_RE = re.compile(
    r"\b(?:binary|executable|artifact|installer|app)\b.{0,60}\b(?:sha-?256|digest|hash)\b"
    r"|\b(?:sha-?256|digest|hash)\b.{0,60}\b(?:binary|executable|artifact|installer|app)\b",
    re.I | re.S,
)
SOURCE_COMMIT_CONTEXT_RE = re.compile(
    r"\bsource\s+commit\b.{0,60}\b(?:sha-?1|git|object|hash)?\b"
    r"|\b(?:sha-?1|git|object|hash)\b.{0,60}\bsource\s+commit\b",
    re.I | re.S,
)
SOURCE_TREE_CONTEXT_RE = re.compile(
    r"\bsource\s+tree\b.{0,60}\b(?:sha-?1|git|object|hash)?\b"
    r"|\b(?:sha-?1|git|object|hash)\b.{0,60}\bsource\s+tree\b",
    re.I | re.S,
)
PUBLIC_CLAIM_SURFACES = {
    "README.md",
    "apps/osl-hub-ui/README.md",
    "docs/prototypes/osl-hub/README.md",
    "docs/prototypes/osl-hub/index.html",
    "docs/prototypes/osl-hub/app.js",
    "docs/prototypes/osl-chats-lab/README.md",
    "docs/prototypes/osl-chats-lab/index.html",
    "docs/prototypes/osl-chats-lab/app.js",
}
RELEASE_BINARY_REQUIRED_CLAIMS = {
    "release proof without retained binary identity": re.compile(
        r"\b(?:released?|shipping|production|named\s+release|exact\s+release)\s+"
        r"(?:build|binary|app|artifact|candidate)?\b.{0,100}\b"
        r"(?:proves?|verified|validated|demonstrated|works?|supports?|protects?|encrypts?|decrypts?|sends?|deletes?)\b",
        re.I | re.S,
    ),
    "unqualified shipped channel/image support": re.compile(
        r"\bit\s+works\s+for\s+direct\s+messages,\s+group\s+chats,\s+and\s+server\s+channels,\s+including\s+images\s+and\s+edits\b",
        re.I,
    ),
    "unqualified cover transport claim": re.compile(
        r"\bdiscord\s+stores\s+and\s+forwards\s+an\s+unreadable\s+cover\s+instead\s+of\s+your\s+text\b"
        r"|\bthe\s+encrypted\s+bytes\s+do\s+not\s+get\s+posted\s+to\s+discord\s+as\s+an\s+obvious\s+blob\b"
        r"|\bordinary\s+looking\s+chat\s+text\b",
        re.I,
    ),
    "unqualified ratchet/group sender-key claim": re.compile(
        r"\bdirect\s+messages\s+then\s+ride\s+a\s+double\s+ratchet\b"
        r"|\bgroup\s+chats\s+and\s+server\s+channels\s+use\s+sender\s+keys\b",
        re.I,
    ),
    "unqualified private-key storage claim": re.compile(
        r"\bprivate\s+halves\s+never\s+leave\s+your\s+machine\s+and\s+are\s+sealed\s+at\s+rest\b",
        re.I,
    ),
}
HONEST_RELEASE_LIMIT_RE = re.compile(
    r"\b(?:not\s+yet|no|without|unproved|unproven|unknown|planned|qa\s+builds?|must\s+recheck|must\s+be\s+aligned|before\s+(?:every|exposing|claiming))\b.{0,100}"
    r"\b(?:release|released|shipping|production|named\s+release|exact\s+release)\b"
    r"|\b(?:release|released|shipping|production|named\s+release|exact\s+release)\b.{0,100}"
    r"\b(?:not\s+yet|no|without|unproved|unproven|unknown|planned|qa\s+builds?|must\s+recheck|must\s+be\s+aligned|before\s+(?:every|exposing|claiming))\b",
    re.I | re.S,
)


def publishable_files() -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    entries = [entry.decode("utf-8") for entry in result.stdout.split(b"\0") if entry]
    # A tracked file deleted in the working tree will not be part of the next
    # release even though `git ls-files --cached` still names it until staging.
    return [entry for entry in entries if (ROOT / entry).is_file()]


def audit_actions(paths: list[str], errors: list[str]) -> None:
    immutable = re.compile(r"^\s*-?\s*uses:\s*[^#\s]+@[0-9a-f]{40}\s*(?:#.*)?$")
    for relative in paths:
        if not relative.startswith(".github/workflows/") or not relative.endswith((".yml", ".yaml")):
            continue
        for number, line in enumerate((ROOT / relative).read_text(encoding="utf-8").splitlines(), 1):
            if "uses:" in line and not immutable.match(line):
                errors.append(f"{relative}:{number}: GitHub Action is not pinned to a commit")


def audit_worker_privacy(errors: list[str]) -> None:
    keyserver_config = (ROOT / "keyserver-cf/wrangler.toml").read_text(encoding="utf-8")
    if re.search(r"(?ms)^\[observability\]\s*^enabled\s*=\s*true\s*$", keyserver_config):
        errors.append("keyserver-cf/wrangler.toml: request observability must be disabled")

    cipher_store_config = (ROOT / "cipher-store-cf/wrangler.toml").read_text(encoding="utf-8")
    required_disabled = {
        "observability": ("enabled", "head_sampling_rate"),
        "observability.logs": ("enabled", "head_sampling_rate", "invocation_logs", "persist"),
        "observability.traces": ("enabled", "head_sampling_rate", "persist"),
    }
    for section, fields in required_disabled.items():
        match = re.search(
            rf"(?ms)^\[{re.escape(section)}\]\s*(.*?)(?=^\[|\Z)",
            cipher_store_config,
        )
        body = match.group(1) if match else ""
        for field in fields:
            expected = "0" if field == "head_sampling_rate" else "false"
            if not re.search(rf"(?m)^{re.escape(field)}\s*=\s*{expected}\s*$", body):
                errors.append(
                    f"cipher-store-cf/wrangler.toml: [{section}] must set {field} = {expected}"
                )

    worker_sources = list((ROOT / "keyserver-cf/src").rglob("*.ts")) + list(
        (ROOT / "cipher-store-cf/src").rglob("*.ts")
    )
    for path in worker_sources:
        text = path.read_text(encoding="utf-8")
        relative = path.relative_to(ROOT)
        if re.search(r"console\.(?:log|warn|error)\([^\n]*request\.url", text):
            errors.append(f"{relative}: request URL reaches Worker logging")
        if re.search(
            r"console\.(?:log|warn|error)\([^\n]*\$\{[^}]*(?:email|address|token|signature|user_?id|content_?id)",
            text,
            re.I,
        ):
            errors.append(f"{relative}: potentially identifying value reaches Worker logging")
        if relative.parts[0] == "cipher-store-cf":
            for call in re.finditer(r"console\.(?:log|warn|error)\(\s*([^)]*?)\s*\)", text, re.S):
                argument = call.group(1).strip()
                fixed_literal = re.fullmatch(r'''(?:"[^"\r\n]*"|'[^'\r\n]*')''', argument)
                fixed_contract_marker = argument == "CYCLE_MARKER" and "import { CYCLE_MARKER }" in text
                if not fixed_literal and not fixed_contract_marker:
                    errors.append(f"{relative}: cipher-store logging must use one fixed literal event name")


def audit_local_scanner(errors: list[str]) -> None:
    scanner_path = ROOT / "apps/osl-hub/src/privacy_scan.rs"
    if not scanner_path.exists():
        return
    scanner = scanner_path.read_text(encoding="utf-8")
    forbidden = {
        "filesystem API": r"\b(?:std::fs|tokio::fs|File::|OpenOptions::)",
        "network API": r"\b(?:reqwest|hyper|ureq|std::net|tokio::net|TcpStream|UdpSocket)\b",
        "process execution": r"\b(?:std::process|Command::new)\b",
        "runtime IPC": r"\btauri\b",
    }
    for label, pattern in forbidden.items():
        if re.search(pattern, scanner):
            errors.append(f"{scanner_path.relative_to(ROOT)}: local scanner gained {label}")
    if "pub text: String" not in scanner or "persisted: false" not in scanner:
        errors.append("apps/osl-hub/src/privacy_scan.rs: scanner contract changed; review egress/persistence")


def require_exact_keys(value: Any, keys: set[str], label: str, errors: list[str]) -> bool:
    if not isinstance(value, dict):
        errors.append(f"{label}: must be an object")
        return False
    actual = set(value.keys())
    missing = sorted(keys - actual)
    unknown = sorted(actual - keys)
    if missing or unknown:
        errors.append(
            f"{label}: fields are not exact; missing={','.join(missing)} unknown={','.join(unknown)}"
        )
        return False
    return True


def load_release_binary_identity(errors: list[str]) -> dict[str, Any] | None:
    if not RELEASE_IDENTITY_PATH.exists():
        return None
    try:
        value = json.loads(RELEASE_IDENTITY_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"{RELEASE_IDENTITY_PATH.relative_to(ROOT)}: invalid release identity: {error}")
        return None

    label = str(RELEASE_IDENTITY_PATH.relative_to(ROOT))
    if not require_exact_keys(
        value,
        {"schemaVersion", "releaseTag", "sourceCommit", "sourceTree", "binarySha256", "binarySizeBytes", "claimProfile"},
        label,
        errors,
    ):
        return None
    if value["schemaVersion"] != 1:
        errors.append(f"{label}: schemaVersion must be 1")
    if not isinstance(value["releaseTag"], str) or not re.fullmatch(r"v[0-9][0-9A-Za-z._-]*", value["releaseTag"]):
        errors.append(f"{label}: releaseTag must be a nonempty public v-prefixed tag")
    if not isinstance(value["sourceCommit"], str) or not GIT_OBJECT_RE.fullmatch(value["sourceCommit"]):
        errors.append(f"{label}: sourceCommit must be a full lowercase Git object id")
    if not isinstance(value["sourceTree"], str) or not GIT_OBJECT_RE.fullmatch(value["sourceTree"]):
        errors.append(f"{label}: sourceTree must be a full lowercase Git object id")
    if not isinstance(value["binarySha256"], str) or not SHA256_RE.fullmatch(value["binarySha256"]):
        errors.append(f"{label}: binarySha256 must be lowercase SHA-256")
    if not isinstance(value["binarySizeBytes"], int) or value["binarySizeBytes"] <= 0:
        errors.append(f"{label}: binarySizeBytes must be a positive integer")
    if value["claimProfile"] not in {"none", "qa-only", "release-proven"}:
        errors.append(f"{label}: claimProfile must be none, qa-only, or release-proven")
    return value if not any(error.startswith(f"{label}:") for error in errors) else None


def line_at(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def claim_sentences(text: str) -> list[tuple[int, str]]:
    spans: list[tuple[int, str]] = []
    cursor = 0
    for line in text.splitlines(keepends=True):
        if line.strip():
            spans.append((cursor, line))
        cursor += len(line)
    if not text.endswith(("\n", "\r")):
        tail_start = cursor
        if tail_start < len(text) and text[tail_start:].strip():
            spans.append((tail_start, text[tail_start:]))
    return spans


def release_identity_mismatch_violations(
    text: str, release_identity: dict[str, Any] | None
) -> list[tuple[int, str]]:
    if release_identity is None or release_identity.get("claimProfile") != "release-proven":
        return []

    release_tag = release_identity["releaseTag"]
    binary_sha = release_identity["binarySha256"].lower()
    source_commit = release_identity["sourceCommit"].lower()
    source_tree = release_identity["sourceTree"].lower()
    allowed_identity_hashes = {binary_sha, source_commit, source_tree}
    violations: list[tuple[int, str]] = []

    for offset, sentence in claim_sentences(text):
        if not RELEASE_CONTEXT_RE.search(sentence):
            continue
        for match in RELEASE_TAG_RE.finditer(sentence):
            if match.group(0) != release_tag:
                violations.append(
                    (
                        line_at(text, offset + match.start()),
                        "release claim references a different released binary identity",
                    )
                )
        for match in HEX64_RE.finditer(sentence):
            digest = match.group(0).lower()
            context = sentence[
                max(0, match.start() - 80) : min(len(sentence), match.end() + 80)
            ]
            expected_hashes: set[str] = set()
            if BINARY_DIGEST_CONTEXT_RE.search(context):
                expected_hashes.add(binary_sha)
            if SOURCE_COMMIT_CONTEXT_RE.search(context):
                expected_hashes.add(source_commit)
            if SOURCE_TREE_CONTEXT_RE.search(context):
                expected_hashes.add(source_tree)
            if not expected_hashes:
                expected_hashes = allowed_identity_hashes
            if digest not in expected_hashes:
                violations.append(
                    (
                        line_at(text, offset + match.start()),
                        "release claim references a different released binary identity",
                    )
                )
        for match in GIT_OBJECT_CLAIM_RE.finditer(sentence):
            digest = match.group(0).lower()
            context = sentence[
                max(0, match.start() - 80) : min(len(sentence), match.end() + 80)
            ]
            expected_hashes: set[str] = set()
            if SOURCE_COMMIT_CONTEXT_RE.search(context):
                expected_hashes.add(source_commit)
            if SOURCE_TREE_CONTEXT_RE.search(context):
                expected_hashes.add(source_tree)
            if not expected_hashes:
                expected_hashes = {source_commit, source_tree}
            if digest not in expected_hashes:
                violations.append(
                    (
                        line_at(text, offset + match.start()),
                        "release claim references a different released binary identity",
                    )
                )
    return violations


def release_claim_violations(text: str, release_identity: dict[str, Any] | None) -> list[tuple[int, str]]:
    violations: list[tuple[int, str]] = []
    release_claims_allowed = release_identity is not None and release_identity.get("claimProfile") == "release-proven"
    violations.extend(release_identity_mismatch_violations(text, release_identity))
    for label, pattern in RELEASE_BINARY_REQUIRED_CLAIMS.items():
        for match in pattern.finditer(text):
            start = match.start()
            sentence_start = max(
                text.rfind(".", 0, start),
                text.rfind("!", 0, start),
                text.rfind("?", 0, start),
                text.rfind("\n", 0, start),
            ) + 1
            sentence_end_candidates = [
                index for index in (
                    text.find(".", match.end()),
                    text.find("!", match.end()),
                    text.find("?", match.end()),
                    text.find("\n", match.end()),
                )
                if index != -1
            ]
            sentence_end = min(sentence_end_candidates) if sentence_end_candidates else len(text)
            sentence = text[sentence_start:sentence_end]
            if release_claims_allowed or HONEST_RELEASE_LIMIT_RE.search(sentence):
                continue
            violations.append((line_at(text, start), label))
    return violations


def audit_public_release_claims(paths: list[str], errors: list[str]) -> None:
    release_identity = load_release_binary_identity(errors)
    scanned = 0
    for relative in paths:
        if relative not in PUBLIC_CLAIM_SURFACES:
            continue
        path = ROOT / relative
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        scanned += 1
        for line, label in release_claim_violations(text, release_identity):
            errors.append(
                f"{relative}:{line}: {label}; public claim needs {RELEASE_IDENTITY_PATH.relative_to(ROOT)} with claimProfile=release-proven"
            )

    if scanned < 2:
        errors.append("public release claim audit scanned too few public surfaces")


def audit_public_release_claims_self_test(errors: list[str]) -> None:
    bad_release = "This release build proves encrypted messages send through Discord."
    bad_stale = "It works for direct messages, group chats, and server channels, including images and edits."
    honest_limit = "Encrypted send is verified on QA builds, not yet on the release build."
    exact_release_test = "Reconcile public docs and site claims against the exact released binary."
    release_identity = {
        "schemaVersion": 1,
        "releaseTag": "v1.2.3",
        "sourceCommit": "c" * 40,
        "sourceTree": "d" * 40,
        "binarySha256": "a" * 64,
        "binarySizeBytes": 123,
        "claimProfile": "release-proven",
    }
    good_exact = (
        f"Release v1.2.3 binary SHA-256 {'a' * 64} is the release-proven binary."
    )
    bad_tag = (
        f"Release v9.9.9 binary SHA-256 {'a' * 64} is the release-proven binary."
    )
    bad_hash = (
        f"Release v1.2.3 binary SHA-256 {'b' * 64} is the release-proven binary."
    )
    bad_source_commit = (
        f"Release v1.2.3 source commit {'e' * 40} is the release-proven source."
    )
    bad_source_tree = (
        f"Release v1.2.3 source tree {'f' * 40} is the release-proven source."
    )
    release_identity = None
    if not release_claim_violations(bad_release, release_identity):
        errors.append("internal release-claim test did not catch unbound release proof")
    if not release_claim_violations(bad_stale, release_identity):
        errors.append("internal release-claim test did not catch stale shipped-feature wording")
    if release_claim_violations(honest_limit, release_identity):
        errors.append("internal release-claim test rejected honest release limitation")
    release_identity = {
        "schemaVersion": 1,
        "releaseTag": "v1.2.3",
        "sourceCommit": "c" * 40,
        "sourceTree": "d" * 40,
        "binarySha256": "a" * 64,
        "binarySizeBytes": 123,
        "claimProfile": "release-proven",
    }
    if release_claim_violations(good_exact, release_identity):
        errors.append(f"{exact_release_test}: exact release identity was rejected")
    for bad in (bad_tag, bad_hash, bad_source_commit, bad_source_tree):
        if not release_identity_mismatch_violations(bad, release_identity):
            errors.append(f"{exact_release_test}: stale release identity was not caught")


class PublicReleaseAuditBehaviourTests(unittest.TestCase):
    def release_identity(self) -> dict[str, Any]:
        return {
            "schemaVersion": 1,
            "releaseTag": "v1.2.3",
            "sourceCommit": "c" * 40,
            "sourceTree": "d" * 40,
            "binarySha256": "a" * 64,
            "binarySizeBytes": 123,
            "claimProfile": "release-proven",
        }

    def test_reconcile_public_docs_and_site_claims_against_the_exact_released_binary(self) -> None:
        identity = self.release_identity()

        self.assertEqual(
            release_claim_violations(
                (
                    f"Release v1.2.3 binary SHA-256 {'a' * 64} is release-proven. "
                    f"The source commit {'c' * 40} and source tree {'d' * 40} match the app."
                ),
                identity,
            ),
            [],
        )
        self.assertEqual(
            release_identity_mismatch_violations(
                f"Release v9.9.9 binary SHA-256 {'a' * 64} is release-proven.",
                identity,
            ),
            [(1, "release claim references a different released binary identity")],
        )
        self.assertEqual(
            release_identity_mismatch_violations(
                f"Release v1.2.3 binary SHA-256 {'b' * 64} is release-proven.",
                identity,
            ),
            [(1, "release claim references a different released binary identity")],
        )
        self.assertEqual(
            release_identity_mismatch_violations(
                f"Release v1.2.3 source tree {'f' * 40} is release-proven.",
                identity,
            ),
            [(1, "release claim references a different released binary identity")],
        )
        self.assertTrue(
            release_claim_violations(
                "This release build proves encrypted messages send through Discord.",
                None,
            )
        )
        self.assertTrue(
            release_claim_violations(
                "This release build proves encrypted messages send through Discord.",
                {**identity, "claimProfile": "qa-only"},
            )
        )


def main() -> int:
    errors: list[str] = []
    paths = publishable_files()
    for relative in paths:
        lower = relative.lower()
        path = ROOT / relative
        if relative in FORBIDDEN_TRACKED or Path(lower).suffix in FORBIDDEN_SUFFIXES:
            errors.append(f"{relative}: local/private artifact is tracked")
            continue
        try:
            data = path.read_bytes()
        except OSError as error:
            errors.append(f"{relative}: could not be read: {error}")
            continue
        if b"\0" in data[:8192]:
            continue
        for label, pattern in SECRET_PATTERNS.items():
            if pattern.search(data):
                errors.append(f"{relative}: possible {label}")
        for label, pattern in PERSONAL_PATH_PATTERNS.items():
            if pattern.search(data):
                errors.append(f"{relative}: {label}")

    audit_actions(paths, errors)
    audit_worker_privacy(errors)
    audit_local_scanner(errors)
    audit_public_release_claims(paths, errors)
    audit_public_release_claims_self_test(errors)

    if errors:
        print("Public-release audit failed:", file=sys.stderr)
        for error in sorted(set(errors)):
            print(f"  - {error}", file=sys.stderr)
        return 1
    print(f"OK: public-release boundary checked across {len(paths)} publishable files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
