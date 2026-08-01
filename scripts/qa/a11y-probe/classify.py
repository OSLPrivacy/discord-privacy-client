"""Classify metadata-only UI Automation observations from the Discord probe."""

from __future__ import annotations

from collections.abc import Iterable, Mapping
from typing import Any

MINIMUM_POPULATED_DESCENDANTS = 20


def classify(observations: Iterable[Mapping[str, Any]]) -> dict[str, object]:
    """Return a conservative verdict without accepting UIA content as input.

    Chromium can initially expose a small, flat shell.  A tree is useful only
    after a bounded sample has more than that shell-sized descendant count.
    """
    counts: list[int] = []
    for observation in observations:
        count = observation.get("DescendantCount")
        if not isinstance(count, int) or isinstance(count, bool) or count < 0:
            raise ValueError("each observation needs a non-negative DescendantCount")
        counts.append(count)
    if not counts:
        raise ValueError("at least one observation is required")

    maximum = max(counts)
    return {
        "Schema": "discord-uia-probe/v1",
        "Samples": len(counts),
        "MaximumDescendantCount": maximum,
        "Verdict": "populated" if maximum > MINIMUM_POPULATED_DESCENDANTS else "emptyOrShellOnly",
    }
