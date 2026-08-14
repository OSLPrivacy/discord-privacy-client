#!/usr/bin/env python3
"""Focused Task 6586 source/runtime oracle with product-named failures."""

from __future__ import annotations

import argparse
import re
import shlex
import subprocess
import sys
from pathlib import Path


EXPECTED_TARGET = "/mnt/d/osl-lane-targets/c"
GROUP_ERROR = "Group chats hold at most 20 people"
WARNING = "Removal takes time and is not immediate."


class CheckFailure(RuntimeError):
    def __init__(self, product: str, starvation: str, detail: str = "") -> None:
        super().__init__(
            f"product={product} starvation={starvation}" + (f" {detail}" if detail else "")
        )


def fields(transcript: str, prefix: str, product: str) -> dict[str, str]:
    matches = [line[line.index(prefix) :] for line in transcript.splitlines() if prefix in line]
    if len(matches) != 1:
        raise CheckFailure(product, "runtime_line", f"prefix={prefix} count={len(matches)}")
    result: dict[str, str] = {}
    for token in shlex.split(matches[0])[1:]:
        if "=" in token:
            key, value = token.split("=", 1)
            result[key] = value
    return result


def require(value: bool, product: str, starvation: str, detail: str = "") -> None:
    if not value:
        raise CheckFailure(product, starvation, detail)


def verify_source(root: Path) -> None:
    rules = (root / "crates/ipc/src/membership_size_rules.rs").read_text(encoding="utf-8")
    membership = (root / "crates/ipc/src/membership.rs").read_text(encoding="utf-8")
    service = (root / "crates/ipc/src/membership_service.rs").read_text(encoding="utf-8")
    commands = (root / "crates/ipc/src/commands.rs").read_text(encoding="utf-8")

    require(
        re.search(r"GROUP_CHAT_MAX_PEOPLE:\s*usize\s*=\s*20\s*;", rules) is not None,
        "group_chat",
        "authoritative_cap_20",
    )
    require(
        "maximum_people: None" in rules,
        "enclave",
        "no_maximum_rule",
    )
    enclave_body = re.search(
        r"pub fn enforce_enclave_candidate_size\([^}]+\}\n", rules, re.DOTALL
    )
    require(enclave_body is not None, "enclave", "admission_function")
    require(
        "enforce_group_chat_candidate_size" not in enclave_body.group(0),
        "enclave",
        "independent_limiter",
        "shared_group_limiter=true",
    )
    require(
        not re.search(r"candidate_people\s*>\s*[1-9]", enclave_body.group(0)),
        "enclave",
        "no_maximum_rule",
        "finite_cap=true",
    )
    require(
        membership.count("enforce_group_chat_candidate_size") == 2,
        "group_chat",
        "scope_consumers",
    )
    require(
        service.count("enforce_group_chat_candidate_size") == 2,
        "group_chat",
        "service_consumers",
    )
    require(
        service.count("enforce_enclave_candidate_size") == 2,
        "enclave",
        "service_consumers",
    )
    require(
        commands.count("enforce_group_chat_candidate_size") == 2,
        "group_chat",
        "command_consumers",
    )

    for consumer in (
        "schema.group_chat",
        "command.membership_size_rules.group_chat",
        "settings.group_chat",
        "help.group_chat",
        "membership.scope.admit",
        "membership.scope.replace",
        "membership_service.join",
        "membership_service.reopen",
        "command.create_group_conversation",
        "command.membership_update_and_send_seed",
    ):
        require(consumer in rules, "group_chat", "runtime_inventory", f"absent={consumer}")
    for consumer in (
        "schema.enclave",
        "command.membership_size_rules.enclave",
        "settings.enclave",
        "help.enclave",
        "membership_service.join",
        "hub.named_enclave_registry",
        "hub.enclave_conversation_context",
        "enclave_removal.begin",
    ):
        require(consumer in rules, "enclave", "runtime_inventory", f"absent={consumer}")

    shipping_sources = []
    for base in (root / "crates", root / "apps"):
        if base.exists():
            for path in base.rglob("*.rs"):
                if "tests" in path.parts or path.name.startswith("task_"):
                    continue
                shipping_sources.append(path.read_text(encoding="utf-8", errors="replace"))
    shipping = "\n".join(shipping_sources)
    finite_enclave = re.search(
        r"(?:MAX_ENCLAVE[^\n]*(?:MEMBER|PEOPLE)|MAX_MEMBERS_PER_ENCLAVE)",
        shipping,
        re.IGNORECASE,
    )
    require(finite_enclave is None, "enclave", "shipping_inventory", "finite_cap_source=true")


def verify_transcript(transcript: str) -> None:
    rules = fields(transcript, "TASK6586_RULES ", "group_chat")
    require(rules.get("schema") == "osl.membership-size-rules.v1", "group_chat", "schema")
    require(rules.get("command") == "cmd_osl_membership_size_rules", "group_chat", "command")
    require(rules.get("group_chat_max") == "20", "group_chat", "cap_20")
    require(rules.get("enclave_max") == "null", "enclave", "no_maximum")
    require(rules.get("measured_N") == "3", "enclave", "measured_N")
    require(rules.get("group_chat_consumers") == "10", "group_chat", "runtime_inventory")
    require(rules.get("enclave_consumers") == "8", "enclave", "runtime_inventory")
    require(
        rules.get("group_settings") == "Group chats can have up to 20 people, including the creator.",
        "group_chat",
        "settings_copy",
    )
    require(
        rules.get("group_help") == "A group chat admits 20 total people. Person 21 is not added.",
        "group_chat",
        "help_copy",
    )
    require(rules.get("enclave_settings") == "Enclaves have no member limit.", "enclave", "settings_copy")
    require("threshold never blocks a member" in rules.get("enclave_help", ""), "enclave", "help_copy")

    group = fields(transcript, "TASK6586_GROUP_CHAT ", "group_chat")
    require(group.get("admitted") == "1-20", "group_chat", "person_20")
    require(group.get("refused") == "21", "group_chat", "person_21")
    require(group.get("prior_members") == "20" and group.get("final_members") == "20", "group_chat", "refusal_membership_atomicity")
    require(group.get("key_hash_unchanged") == "true" and group.get("key_epoch") == "20", "group_chat", "refusal_key_atomicity")
    require(group.get("delivery_state") == "20" and group.get("command_delivery_unchanged") == "true", "group_chat", "refusal_delivery_atomicity")
    require(group.get("allowance_state") == "20", "group_chat", "refusal_allowance_atomicity")
    require(group.get("durable_bytes_unchanged") == "true", "group_chat", "refusal_durable_atomicity")
    require(group.get("restart_members") == "20", "group_chat", "restart")
    require(group.get("error") == GROUP_ERROR, "group_chat", "refusal_copy")

    enclave = fields(transcript, "TASK6586_ENCLAVE ", "enclave")
    require(enclave.get("maximum") == "null", "enclave", "no_maximum")
    require(enclave.get("N") == "3", "enclave", "measured_N")
    require(enclave.get("generated_larger") == "103", "enclave", "generated_larger")
    require(enclave.get("admitted_sizes") == "3,20,21,103", "enclave", "size_inventory")
    require(enclave.get("removal_size") == "103", "enclave", "removal_above_N")
    require(enclave.get("warning") == WARNING, "enclave", "warning")
    require(int(enclave.get("progress_samples", "0")) > 2, "enclave", "progress")
    require(enclave.get("progress_completed") == "102" and enclave.get("progress_remaining") == "0", "enclave", "rekey_completion")
    require(enclave.get("fresh_epoch") == "42", "enclave", "fresh_epoch")
    require(enclave.get("removed_packaged_reads") == "0" and enclave.get("removed_direct_reads") == "0", "enclave", "removed_reader")
    require(enclave.get("remaining_fresh_reads") == "1", "enclave", "remaining_reader")
    require(enclave.get("restart_members") == "103", "enclave", "restart")

    require(
        re.search(r"test result: ok\. 3 passed; 0 failed", transcript) is not None,
        "group_chat+enclave",
        "focused_test_result",
    )


def run_test(root: Path) -> str:
    command = [
        "cargo",
        "test",
        "-p",
        "ipc",
        "--test",
        "task_6586_membership_size_rules",
        "--",
        "--nocapture",
        "--test-threads=1",
    ]
    completed = subprocess.run(command, cwd=root, text=True, capture_output=True)
    transcript = completed.stdout + completed.stderr
    print(transcript, end="")
    if completed.returncode != 0:
        raise CheckFailure("group_chat+enclave", "focused_test", f"cargo_exit={completed.returncode}")
    return transcript


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--transcript", type=Path)
    parser.add_argument("--skip-source", action="store_true")
    args = parser.parse_args()
    try:
        if not args.skip_source:
            verify_source(args.source_root)
        if args.transcript:
            transcript = args.transcript.read_text(encoding="utf-8", errors="replace")
        else:
            import os

            target = os.environ.get("CARGO_TARGET_DIR")
            if target != EXPECTED_TARGET:
                raise CheckFailure("group_chat+enclave", "cargo_target", f"actual={target}")
            transcript = run_test(args.source_root)
        verify_transcript(transcript)
        print(
            "TASK6586_CHECK_EXIT=0 group_chat_cap=20 admitted=1-20 refused=21 atomic=membership,key,delivery,allowance enclave_max=null enclave_sizes=3,20,21,103 N=3 progress_samples=11 rekey_completed=102 fresh_epoch=42"
        )
        return 0
    except (CheckFailure, OSError, ValueError) as error:
        print(f"TASK6586_CHECK_EXIT=1 {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
