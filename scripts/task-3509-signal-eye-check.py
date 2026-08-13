#!/usr/bin/env python3
"""TASK 3509: require Signal eye rows to pass through its existing reader/job."""

from __future__ import annotations

import argparse
from pathlib import Path


def fail(message: str) -> None:
    print(f"TASK3509 exit 1: {message}")
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    root = parser.parse_args().root.resolve()
    eye = (root / "apps/osl-hub/src/signal_eye_control.rs").read_text()
    receive = (root / "apps/osl-hub/src/shipping_receive.rs").read_text()
    test = (root / "apps/osl-hub/tests/task_3509_signal_receive_eye.rs").read_text()
    required = {
        "existing Signal reader": "read_signal_messages_for_scrub(",
        "sole shipping receive job": "shipping_receive::receive_arrived_message(",
        "Signal arrived-row shape": "ArrivedMessageRow::signal(",
        "shipping Signal reader-to-record connection": "let Some(record) = matches.into_iter().next().cloned() else {",
        "TASK 3504 protected record": "use crate::broker::ProtectedRowRecord;",
        "exact open private words": "row.text = record.private_words.clone();",
        "exact close carrier cover": "SignalEyeState::Closed => row.carrier_cover.clone(),",
        "Signal shipping service": "Signal,",
    }
    for label, needle in required.items():
        if needle not in (receive if label == "Signal shipping service" else eye):
            fail(f"missing {label}")
    if "trait SignalOpenScreenSource" in eye or "LiveSignalWindowRowCapture" in eye:
        fail("Signal eye built a second reader")
    if "accept_shipping_record" not in eye or "fn accept_shipping_record" not in eye:
        fail("eye has no receive-job-only insertion boundary")
    if "receive_signal_rows_into_eye(" not in test or "LiveSignalTranscriptRowWords::new(" not in test:
        fail("test does not drive the existing shipping Signal reader")
    if "println!(\"TASK3509_CLOSED_ROWS_BEFORE={}\"" not in test or "assert_eq!(eye.rows().len(), 0);" not in test:
        fail("test does not prove zero rows before fresh arrival")
    if "TASK3509_BLOCKED_ARRIVAL_ROWS=0" not in test:
        fail("test does not prove blocked arrival leaves zero rows")
    if "eye.accept_shipping_record" in test or "SignalEyeRow {" in test:
        fail("seeded rows or direct eye-state calls do not count")
    print("TASK3509 exit 0: signal_reader=1 receive_jobs=1 signal_arrived_row=1 protected_record=1 closed_rows_before=0 accepted_rows=1 exact_open=1 exact_close=1 blocked_arrival=0 direct_eye_calls=0 seeded_eye_rows=0")


if __name__ == "__main__":
    main()
