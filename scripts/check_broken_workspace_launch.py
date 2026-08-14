#!/usr/bin/env python3
"""Verify TASK 1608's fail-closed launch fixture.

The fixture models the observable result of starting OSL after its required
app-local data folder has become unavailable.  It is intentionally a
small, machine-checkable observation: one plain failure screen and no opened
workspace.  The checker refuses incomplete fixtures rather than assuming a
missing local-data declaration means the same thing.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


FAILURE_CAUSE = "Required local data folder is unavailable."


class FixtureError(ValueError):
    """The fixture does not prove the required failure mode."""


def require(value: bool, message: str) -> None:
    if not value:
        raise FixtureError(message)


def verify(fixture: object) -> dict[str, object]:
    require(isinstance(fixture, dict), "fixture must be a JSON object")
    require(fixture.get("schemaVersion") == 1, "unsupported fixture schema")

    local_data = fixture.get("requiredLocalDataFolder")
    require(isinstance(local_data, dict), "required local data folder record is missing")
    require(local_data.get("available") is False, "required local data folder is not unavailable")
    require(isinstance(local_data.get("path"), str) and local_data["path"],
            "required local data folder path is missing")

    screens = fixture.get("screens")
    require(isinstance(screens, list), "failure screens record is missing")
    require(len(screens) == 1, f"expected exactly 1 plain failure screen, found {len(screens)}")
    screen = screens[0]
    require(isinstance(screen, dict), "failure screen is not an object")
    require(screen.get("kind") == "plain", "failure screen is not plain")
    require(screen.get("text") == FAILURE_CAUSE, "plain failure screen does not name the local data folder cause")

    workspaces = fixture.get("openWorkspaces")
    require(isinstance(workspaces, list), "open workspaces record is missing")
    require(len(workspaces) == 0, f"expected 0 broken workspaces open, found {len(workspaces)}")
    return fixture


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        fixture = json.loads(args.fixture.read_text(encoding="utf-8"))
        verified = verify(fixture)
    except (OSError, json.JSONDecodeError, FixtureError) as error:
        print(f"TASK1608_FAIL {error}", file=sys.stderr)
        return 1
    local_data = verified["requiredLocalDataFolder"]
    print(
        "TASK1608_PASS "
        f"failure_screens=1 cause={FAILURE_CAUSE!r} "
        f"broken_workspaces_open=0 local_data_available={local_data['available']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
