#!/usr/bin/env python3
"""Fail a public release when tracked files violate OSL's trust boundary."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


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
                if not re.fullmatch(r'''(?:"[^"\r\n]*"|'[^'\r\n]*')''', argument):
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

    if errors:
        print("Public-release audit failed:", file=sys.stderr)
        for error in sorted(set(errors)):
            print(f"  - {error}", file=sys.stderr)
        return 1
    print(f"OK: public-release boundary checked across {len(paths)} publishable files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
