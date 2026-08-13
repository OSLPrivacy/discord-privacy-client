#!/usr/bin/env python3
"""Reject extra shipping receive jobs or an OSL Chats routing bypass."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


def fail(message: str) -> None:
    print(f"FAIL: {message}")
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    source_root = root / "apps/osl-hub/src"
    shipping = source_root / "shipping_receive.rs"
    chats = source_root / "osl_chat_delivery.rs"
    test = root / "apps/osl-hub/tests/task_3505a_one_shipping_receive_job.rs"
    files = list(source_root.rglob("*.rs"))
    job_matches = []
    per_app_readers = []
    for path in files:
        text = path.read_text()
        job_matches.extend((path, match.start()) for match in re.finditer(r"\bfn\s+receive_arrived_message(?:<[^>]+>)?\s*\(", text))
        per_app_readers.extend(
            (path, match.group(1))
            for match in re.finditer(
                r"\bfn\s+((?:receive|read)_(?:osl_chat|discord)[A-Za-z0-9_]*(?:arriv|message)[A-Za-z0-9_]*)\s*\(",
                text,
            )
        )
    if len(job_matches) != 1 or job_matches[0][0] != shipping:
        fail(f"expected exactly 1 shipping receive job, found {len(job_matches)}")
    if per_app_readers:
        fail("extra per-app reader: " + ", ".join(name for _, name in per_app_readers))
    shipping_text = shipping.read_text()
    chat_text = chats.read_text()
    test_text = test.read_text()
    if "journal.services.push(row.service);" not in shipping_text:
        fail("shipping receive job does not record the calling service on every row")
    if "ArrivedMessageRow::osl_chats" not in chat_text:
        fail("OSL Chats missed the shared arrived-row shape")
    if "shipping_receive::receive_arrived_message" not in chat_text:
        fail("OSL Chats missed the one shipping entry point")
    if "route_osl_chat_arrival" not in test_text or "LiveOslChatsSession" not in test_text:
        fail("saved conversation or direct entry call does not prove a live OSL Chats session")
    if "receive_arrived_message(" in test_text:
        fail("direct entry-point call does not count as OSL Chats reaching it")
    print("TASK3505A receive_jobs=1 per_app_readers=0 osl_chats_shared_row=1 entry_point=1 service_attribution=1 live_session_route=1 saved_conversations=0 direct_entry_calls=0")


if __name__ == "__main__":
    main()
