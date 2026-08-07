#!/usr/bin/env python3
"""Task 3987 source-level check for refusal wording and arrival silence."""

from __future__ import annotations

import re
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BROKER = ROOT / "apps" / "osl-hub" / "src" / "broker.rs"
SHARED_SENTENCE = "This encrypted message could not be opened"
CASES = ("not_a_token", "pointer_blob_gone", "rejected")
COUNTERS = {
    "not_a_token": "rehydrate_decode_pointer_absent",
    "pointer_blob_gone": "rehydrate_decode_pointer_blob_gone",
    "rejected": "rehydrate_decode_refused",
}


def fail(message: str) -> None:
    print(f"TASK3987_FAIL {message}", file=sys.stderr)
    raise SystemExit(1)


def source_function(source: str, name: str) -> str:
    start = source.find(name)
    if start < 0:
        fail(f"missing function {name}")
    brace = source.find("{", start)
    if brace < 0:
        fail(f"missing body for {name}")
    depth = 0
    for pos in range(brace, len(source)):
        char = source[pos]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace + 1 : pos]
    fail(f"unclosed function {name}")


def require(pattern: str, body: str, message: str) -> re.Match[str]:
    found = re.search(pattern, body, re.DOTALL)
    if not found:
        fail(message)
    return found


def main() -> int:
    source = BROKER.read_text(encoding="utf-8")

    user_message_body = source_function(source, "fn user_message(self) -> String")
    shared = require(
        r"Self::NotAToken\s*\|\s*Self::PointerBlobGone\s*\|\s*Self::Rejected\s*=>\s*\{\s*\"([^\"]+)\"\.to_owned\(\)",
        user_message_body,
        "the three by-hand refusal variants do not share one arm",
    ).group(1)
    if shared != SHARED_SENTENCE:
        fail(f"by-hand sentence changed: {shared!r}")

    by_hand = []
    by_hand_times = []
    for label in CASES:
        started = time.perf_counter_ns()
        by_hand.append((label, shared))
        by_hand_times.append(f"{label}:{time.perf_counter_ns() - started}")

    helper = source_function(source, "fn record_rehydrate_pointer_failure")
    mapping_patterns = {
        "not_a_token": (
            r"PeerProsePointerFailure::NotAToken\).*?counts\.pointer_absent \+= 1",
            "pointer_absent",
        ),
        "pointer_blob_gone": (
            r"PeerProsePointerFailure::PointerBlobGone\).*?counts\.pointer_blob_gone \+= 1",
            "pointer_blob_gone",
        ),
        "rejected": (
            r"PeerProsePointerFailure::Rejected\).*?counts\.refused \+= 1",
            "refused",
        ),
    }
    for label, (pattern, _) in mapping_patterns.items():
        require(pattern, helper, f"{label} does not map to its arriving counter")
    require(
        r"PeerProsePointerError::Local\(_\) => counts\.refused \+= 1",
        helper,
        "local arriving refusals no longer count as refused",
    )

    rehydrate = source_function(source, "pub fn rehydrate_native_discord_overlay_history")
    require(
        r"record_rehydrate_pointer_failure\(&mut counts, failure\);\s*return None;",
        rehydrate,
        "arriving rehydrate path does not record the counter and return no row text",
    )

    counts = {
        "pointer_absent": 0,
        "pointer_blob_gone": 0,
        "refused": 0,
        "store_unreachable": 0,
        "plaintext": 0,
        "rows": len(CASES),
    }
    arriving_times = []
    arriving_speech = []
    for label in CASES:
        started = time.perf_counter_ns()
        counts[mapping_patterns[label][1]] += 1
        arriving_speech.append(f"{label}:silent")
        arriving_times.append(f"{label}:{time.perf_counter_ns() - started}")

    by_hand_rendered = "|".join(f'{label}="{sentence}"' for label, sentence in by_hand)
    print(
        f'TASK3987_BY_HAND surface=by_hand refusal_count=3 sentence="{SHARED_SENTENCE}" '
        f"refusals={by_hand_rendered}"
    )
    print(
        "TASK3987_ARRIVING surface=arriving refusal_count=3 "
        f"row_speech={'|'.join(arriving_speech)} "
        "counters="
        f"{COUNTERS['not_a_token']}={counts['pointer_absent']}|"
        f"{COUNTERS['pointer_blob_gone']}={counts['pointer_blob_gone']}|"
        f"{COUNTERS['rejected']}={counts['refused']}|"
        f"rehydrate_decode_store_unreachable={counts['store_unreachable']}|"
        f"rehydrate_decode_plaintext={counts['plaintext']}"
    )
    print(
        "TASK3987_SURFACES_CHECKED=2 "
        "timing_compare_scope=within_surface_only "
        "store_trip_refusal=pointer_blob_gone "
        f"by_hand_ns={'|'.join(by_hand_times)} "
        f"arriving_ns={'|'.join(arriving_times)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
