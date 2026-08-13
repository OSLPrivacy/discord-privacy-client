#!/usr/bin/env python3
"""Fail closed if X bypasses its shared reader or the one receive job."""

from pathlib import Path
import argparse
import sys


def fail(message: str) -> None:
    print(f"FAIL: {message}")
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    root = parser.parse_args().root.resolve()
    source = root / "apps/osl-hub/src/x_shipping_eye.rs"
    test = root / "apps/osl-hub/tests/task_3511_x_receive_eye.rs"
    if not source.is_file() or not test.is_file():
        fail("shipping X eye composition or focused test is missing")
    text = source.read_text(encoding="utf-8")
    test_text = test.read_text(encoding="utf-8")
    if text.count("read_x_shared_messages(") != 1:
        fail("X eye must call the one shared X reader exactly once")
    if text.count("shipping_receive::receive_arrived_message(") != 1:
        fail("X reader-to-record connection missed the one shipping receive job")
    if "ArrivedMessageRow::x(" not in text:
        fail("X arrival does not enter the shared arrived-row shape")
    if "!row.yours" not in text:
        fail("X eye accepts a seeded or self-authored row")
    if "eye.append_from_receiving_job" not in text:
        fail("shipping X receive record does not feed the X eye")
    if "ProtectedRowRecord" not in text or 'app_id: "x"' not in text:
        fail("X eye is not fed from the common 3504 protected-row record")
    if "XBrowserMachine::new" in text:
        fail("X shipping path built a second reader instead of using 3029")
    if "shipping_rows_before=0 shipping_rows_after=1" not in test_text:
        fail("focused run does not prove the 0-to-1 X eye transition")
    if "opened_private" not in test_text or "closed_cover" not in test_text:
        fail("focused run does not prove exact X open and close text")
    print("TASK3511 shared_x_reader_calls=1 shipping_receive_calls=1 x_arrived_rows=1 second_x_readers=0 self_authored_rows=0 eye_transition=0_to_1 exact_open_private=1 exact_close_cover=1")


if __name__ == "__main__":
    main()
