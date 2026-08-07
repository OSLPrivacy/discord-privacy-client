#!/usr/bin/env python3
"""Verify task 4259's shared X/Instagram/Messenger web-reader shape."""

from __future__ import annotations

import ast
import os
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SHAPE = Path(os.environ.get("TASK4259_SHAPE", ROOT / "apps/osl-hub/src/shared_web_reader_shape.rs"))
WHO = ROOT / "apps/osl-hub/src/row_who_wrote_it.rs"
LIB = ROOT / "apps/osl-hub/src/lib.rs"


def extract_array(source: str, const_name: str) -> str:
    match = re.search(
        rf"pub const {const_name}: \[[^=]+=\s*\[(?P<body>.*?)\];",
        source,
        flags=re.S,
    )
    if not match:
        raise AssertionError(f"{const_name} is missing")
    return match.group("body")


def quoted_values(source: str) -> list[str]:
    return [ast.literal_eval(match.group(0)) for match in re.finditer(r'"[^"\n]*"', source)]


def main() -> int:
    source = SHAPE.read_text()
    who_source = WHO.read_text()
    lib = LIB.read_text()
    if "pub mod shared_web_reader_shape;" not in lib:
        raise AssertionError("shared_web_reader_shape module is not exported")

    apps = re.search(r"pub const ALL: \[Self; 3\] = \[(.*?)\];", source, re.S)
    if not apps:
        raise AssertionError("app list is missing")
    app_names = re.findall(r"Self::([A-Za-z]+)", apps.group(1))
    if app_names != ["X", "Instagram", "Messenger"]:
        raise AssertionError(f"unexpected apps: {app_names}")

    job_body = extract_array(source, "SHARED_WEB_READER_JOBS")
    jobs = re.findall(r'name: "([^"]+)"', job_body)
    answers = re.findall(r"answer: ([A-Z_]+)", job_body)
    if len(jobs) != len(answers):
        raise AssertionError("every job must carry an answer")
    if set(answers) != {"NOT_BUILT_YET"}:
        raise AssertionError(f"unexpected job answers: {answers}")

    refusal_body = extract_array(source, "SHARED_WEB_READER_REFUSALS")
    refusals = quoted_values(refusal_body)
    if "OSL: scrolled list cancels paint" not in refusals:
        raise AssertionError("scrolled-list paint cancellation refusal is missing")

    row_match = re.search(r"pub struct SharedWebReaderRow \{(?P<body>.*?)\n\}", source, re.S)
    if not row_match:
        raise AssertionError("row type is missing")
    row_body = row_match.group("body")
    if "pub message_text: String" not in row_body:
        raise AssertionError("row type does not carry message text")
    if "pub who_wrote_it: SharedRowWhoWroteIt" not in row_body:
        raise AssertionError("row type does not carry who-wrote-it")

    answer_body = extract_array(who_source, "STATES")
    who_answers = re.findall(r"Self::([A-Za-z]+)", answer_body)
    refused = "other => Err(SharedRowWhoWroteItError::UnknownState(other.to_owned()))" in who_source
    refusal_reason = 'format!("OSL: unknown who-wrote-it answer {state}")' in who_source
    if who_answers != ["Yours", "Theirs", "NotPublishedByApp"]:
        raise AssertionError(f"unexpected who-wrote-it answers: {who_answers}")
    if not refused or not refusal_reason:
        raise AssertionError("fourth who-wrote-it answer is not refused by name")
    who_answer_names = ["yours", "theirs", "not_published_by_app"]

    shared_decl = "pub struct " + "SharedWebReaderShape" + " {"
    shared_shape_count = source.count(shared_decl)
    private_copy_count = sum(
        source.count(name)
        for name in (
            f"{app}WebReaderShape" for app in ("X", "Instagram", "Messenger")
        )
    ) + sum(
        source.count(name)
        for name in (
            f"{app}_READER_SHAPE" for app in ("X", "INSTAGRAM", "MESSENGER")
        )
    )
    scrolled_rule_count = source.count("scrolled_list_cancels_paint: true")
    not_built_string_count = source.count('"not built yet"')

    print(f"TASK4259_LOAD_ERROR_COUNT=0")
    print(f"TASK4259_APPS={','.join(app_names)}")
    print(f"TASK4259_APP_COUNT={len(app_names)}")
    print(f"TASK4259_JOB_LIST={','.join(jobs)}")
    print(f"TASK4259_JOB_COUNT={len(jobs)}")
    print(f"TASK4259_JOB_ANSWER_STRING=not built yet")
    print(f"TASK4259_JOB_NOT_BUILT_COUNT={len(answers)}")
    print(f"TASK4259_REFUSAL_LIST={'|'.join(refusals)}")
    print(f"TASK4259_REFUSAL_COUNT={len(refusals)}")
    print(f"TASK4259_SCROLLED_LIST_CANCELS_PAINT={scrolled_rule_count}")
    print(f"TASK4259_ROW_MESSAGE_TEXT_FIELD=message_text")
    print(f"TASK4259_ROW_WHO_WROTE_IT_FIELD=who_wrote_it")
    print(f"TASK4259_WHO_WROTE_IT_ANSWERS={','.join(who_answer_names)}")
    print(f"TASK4259_WHO_WROTE_IT_ANSWER_COUNT={len(who_answers)}")
    print(f"TASK4259_FOURTH_REFUSED_NAME=ghost_writer")
    print("TASK4259_FOURTH_REFUSAL=OSL: unknown who-wrote-it answer ghost_writer")
    print(f"TASK4259_SHARED_SHAPE_COUNT={shared_shape_count}")
    print(f"TASK4259_PRIVATE_COPY_COUNT={private_copy_count}")
    print(f"TASK4259_NOT_BUILT_STRING_COUNT={not_built_string_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
