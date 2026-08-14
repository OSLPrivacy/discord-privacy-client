#!/usr/bin/env python3
"""Fail closed if TASK 7211 can turn its supplied mark into a fake read-back."""

from __future__ import annotations

import argparse
from pathlib import Path


def check(module: str, job: str, adapter: str) -> None:
    required_module = (
        "Get-Process -Name 'Discord'",
        "Resolve-OnePersonDiscordQaPlacementJob",
        "--send-message",
        "discord_process_id",
        "conversation_tree_reported",
        "accessibility_wait_ms",
        "sent_message_readback",
        "ReadBack = $readBack",
        "PlacedText = $readBack",
        "Discord process is absent.",
    )
    required_job = (
        "const SENT_MESSAGE_WAIT_MS",
        "place_send_and_read_back_from_discord_tree",
        "send_enter()",
        "wait_for_sent_message",
        "sent_message_in_tree",
        "if element_is_composer(&element, None)",
        "sent_message_readback={read_back}",
        "accessibility_wait_ms={wait_ms}",
        "conversation_tree_reported={conversation}",
    )
    absent = (
        "ReadBack = $Message",
        "PlacedText = $Message",
        "read_back: message.to_owned()",
        "placed_text: message.to_owned()",
    )
    missing = [needle for needle in required_module if needle not in module]
    missing += [needle for needle in required_job if needle not in job]
    present = [needle for needle in absent if needle in module or needle in job or needle in adapter]
    if missing or present:
        raise SystemExit(
            "TASK7211 failed: "
            + (f"missing={missing} " if missing else "")
            + (f"forbidden_echo={present}" if present else "")
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--module",
        type=Path,
        default=Path("scripts/qa/discord-one-person-test-target.psm1"),
    )
    parser.add_argument(
        "--job",
        type=Path,
        default=Path("apps/osl-hub/examples/task_3406_place_text.rs"),
    )
    parser.add_argument(
        "--adapter",
        type=Path,
        default=Path("apps/osl-hub/src/native_discord_adapter.rs"),
    )
    args = parser.parse_args()
    check(args.module.read_text(), args.job.read_text(), args.adapter.read_text())
    print("TASK7211_NO_INPUT_ECHO_PATHS=0")
    print("TASK7211_DISCORD_TREE_READBACK_REQUIRED=true")
    print("TASK7211_ASYNC_WAIT_REQUIRED=true")


if __name__ == "__main__":
    main()
