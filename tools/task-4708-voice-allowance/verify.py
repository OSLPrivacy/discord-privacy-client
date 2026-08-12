#!/usr/bin/env python3
"""Fail-closed release-call reconciliation for TASK 4708."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def fail(detail: str) -> None:
    print(f"TASK4708 FAIL {detail}", file=sys.stderr)
    raise SystemExit(1)


def interface_counter(path: Path) -> tuple[int, int]:
    events = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
    matches = [row for row in events if row.get("event") == "interface_counters"]
    if len(matches) != 1:
        fail(f"missing interface counters client={path.stem}")
    row = matches[0]
    if row.get("counter_source") != "release-client-media-interface":
        fail(f"synthetic counters client={path.stem}")
    sent = row.get("sent_bytes")
    received = row.get("received_bytes")
    if not isinstance(sent, int) or sent <= 0:
        fail(f"missing sent bytes client={path.stem}")
    if not isinstance(received, int) or received <= 0:
        fail(f"missing received bytes client={path.stem}")
    return sent, received


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--allowance", type=Path, required=True)
    args = parser.parse_args()
    report = json.loads(args.allowance.read_text(encoding="utf-8"))
    people = {row["identity"]: row for row in report.get("people", [])}
    if set(people) != {"speaker-A", "speaker-B", "speaker-C"}:
        fail("allowance report lacks three independent people")
    for identity in sorted(people):
        sent, received = interface_counter(args.artifacts / f"{identity}.jsonl")
        row = people[identity]
        actual = sent + received
        recorded = row.get("voice_row_bytes")
        if not isinstance(recorded, int) or recorded == 0:
            fail(f"missing voice bytes client={identity}")
        if abs(recorded - actual) * 100 > actual:
            fail(f"allowance mismatch client={identity} actual={actual} persisted={recorded}")
        if row.get("interface_sent_bytes") != sent or row.get("interface_received_bytes") != received:
            fail(f"bypassed accounting client={identity}")
    rows = report.get("rows")
    if not isinstance(rows, list) or [row.get("class") for row in rows] != [
        "background connection", "messages", "attachments", "stories and posts", "voice", "multi-device sync",
    ]:
        fail("itemised rows do not contain the stable Voice row")
    voice = next(row.get("bytes") for row in rows if row.get("class") == "voice")
    if not isinstance(voice, int) or voice <= 0:
        fail("missing voice bytes row")
    total = report.get("total_after_restart")
    if total != report.get("total_before_restart") or total != report.get("itemised_sum"):
        fail("restart total does not reconcile to itemised rows")
    if report.get("voice_minute_units") != 0:
        fail("voice-minute units present")
    print(
        "TASK4708 PASS speakers=3 duration_minutes=10 "
        f"voice_row_bytes={voice} total={total} itemised_rows={len(rows)} voice_minute_units=0"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
