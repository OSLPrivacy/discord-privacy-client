#!/usr/bin/env python3
"""Fail-closed verifier for TASK 1607's clean-Windows install receipt."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


SHA256 = re.compile(r"^[0-9a-f]{64}$")


class ReceiptError(ValueError):
    """The receipt does not prove the requested clean install."""


def require(value: bool, message: str) -> None:
    if not value:
        raise ReceiptError(message)


def verify(receipt: object, expected_sha256: str) -> dict[str, object]:
    require(isinstance(receipt, dict), "receipt must be a JSON object")
    require(receipt.get("schemaVersion") == 1, "unsupported receipt schema")
    require(expected_sha256 == expected_sha256.lower() and bool(SHA256.fullmatch(expected_sha256)),
            "expected installer SHA-256 must be lowercase hex")
    require(receipt.get("installerSha256") == expected_sha256,
            "downloaded installer SHA-256 does not match the published checksum")

    installer = receipt.get("installer")
    require(isinstance(installer, dict), "installer record is missing")
    require(installer.get("name") == "osl-hub-0.1.0-x64-nsis.exe",
            "receipt names an unexpected installer")
    require(installer.get("sizeBytes") == 7126171,
            "receipt names an unexpected installer size")

    installed_apps = receipt.get("installedApps")
    require(isinstance(installed_apps, list), "Installed Apps record is missing")
    require(any(isinstance(app, dict) and app.get("displayName") == "OSL Privacy" for app in installed_apps),
            "OSL Privacy is absent from Installed Apps")

    launch = receipt.get("welcomeLaunch")
    require(isinstance(launch, dict), "Welcome launch record is missing")
    require(launch.get("processAlive") is True, "OSL process was not alive after launch")
    require(launch.get("visibleText") == "Welcome", "OSL did not launch to Welcome")
    require(isinstance(launch.get("processId"), int) and launch["processId"] > 0,
            "Welcome launch lacks a process id")
    return receipt


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True)
    args = parser.parse_args(argv)
    try:
        receipt = json.loads(args.receipt.read_text(encoding="utf-8"))
        verified = verify(receipt, args.expected_sha256)
    except (OSError, json.JSONDecodeError, ReceiptError) as error:
        print(f"TASK1607_FAIL {error}", file=sys.stderr)
        return 1
    print(
        "TASK1607_PASS "
        f"installer_sha256={verified['installerSha256']} "
        "installed_app=OSL Privacy welcome=Welcome"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
