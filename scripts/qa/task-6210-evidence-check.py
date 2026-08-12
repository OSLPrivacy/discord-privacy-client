#!/usr/bin/env python3
"""Fail-closed checker and starvation campaign for TASK 6210 output."""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path


CARRIERS = ["discord", "telegram", "whatsapp", "x", "instagram", "messenger", "signal"]
MAIL = [
    "gmail",
    "outlook-web",
    "outlook-desktop",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
]
MODES = ["discovery", "scheduled_find_only"]
ACCOUNT_A = "provider-stable-account-A-6210"
ACCOUNT_B = "provider-stable-account-B-6210"


def refuse(name: str) -> None:
    print(f"TASK6210_STARVATION missing={name}", file=sys.stderr)
    raise SystemExit(1)


def fields(line: str) -> dict[str, str]:
    answer: dict[str, str] = {}
    for token in line.split()[1:]:
        if "=" in token:
            key, value = token.split("=", 1)
            answer[key] = value
    return answer


def marked_lines(report: str, marker: str) -> list[str]:
    return [line[line.index(marker) :] for line in report.splitlines() if marker in line]


def require_field(row: dict[str, str], key: str, value: str, label: str) -> None:
    if row.get(key) != value:
        refuse(label)


def check(report: str) -> None:
    cell_lines = marked_lines(report, "TASK6210_CELL ")
    cells: dict[tuple[str, str], dict[str, str]] = {}
    for line in cell_lines:
        row = fields(line)
        key = (row.get("source", ""), row.get("mode", ""))
        if key in cells:
            refuse(f"duplicate-cell:{key[0]}/{key[1]}")
        cells[key] = row

    expected_cells = {(source, mode) for source in CARRIERS + MAIL for mode in MODES}
    for source, mode in sorted(expected_cells):
        if (source, mode) not in cells:
            refuse(f"provider-mode:{source}/{mode}")
    if set(cells) != expected_cells:
        refuse("unexpected-provider-mode-cell")

    exact = {
        "account_A": ACCOUNT_A,
        "account_B": ACCOUNT_B,
        "A_consent": "true",
        "B_consent_before_refusal": "false",
        "selected_approved_before": "true",
        "selected_approved_after": "false",
        "A_actions": "1",
        "first_B_action": "list",
        "B_actions_before_fresh_approval": "0",
        "multi_page_run": "true",
        "mid_page_barrier": "1",
        "in_place_switch": "1",
        "same_live_session": "true",
        "signed_out_events": "0",
        "independent_identity_read": "true",
        "independent_network_action_read": "true",
        "independent_osl_state_read": "true",
        "refusal": "account_changed",
        "stale_state_cleared": "true",
        "fresh_approval_required": "true",
        "fresh_B_approval": "true",
        "control_runs": "1",
        "control_pages": "2",
        "control_actions": "9",
        "ui_label_unchanged": "true",
        "expected_set_unchanged": "true",
        "requested_id_mismatch": "true",
        "self_declared_name_stale": "true",
        "cooperative_ui_stop": "0",
        "fixture_provider": "false",
    }
    labels = {
        "account_A": "account-A-identity",
        "account_B": "account-B-identity",
        "A_consent": "A-consent-state",
        "B_consent_before_refusal": "B-unapproved-state",
        "selected_approved_before": "selected-approved-before",
        "selected_approved_after": "stale-state-clear",
        "A_actions": "A-action",
        "first_B_action": "first-B-action",
        "B_actions_before_fresh_approval": "B-action-refusal",
        "multi_page_run": "multi-page-run",
        "mid_page_barrier": "mid-page-barrier",
        "in_place_switch": "in-place-switch",
        "same_live_session": "same-live-session",
        "signed_out_events": "no-logout",
        "independent_identity_read": "independent-identity-read",
        "independent_network_action_read": "independent-action-read",
        "independent_osl_state_read": "independent-OSL-state-read",
        "refusal": "account-changed-refusal",
        "stale_state_cleared": "stale-state-clear",
        "fresh_approval_required": "fresh-approval-required",
        "fresh_B_approval": "fresh-B-approval",
        "control_runs": "control-run",
        "control_pages": "control-pages",
        "control_actions": "control-actions",
        "ui_label_unchanged": "unchanged-UI-label",
        "expected_set_unchanged": "unchanged-expected-set",
        "requested_id_mismatch": "requested-id-mismatch",
        "self_declared_name_stale": "self-declared-name",
        "cooperative_ui_stop": "cooperative-UI-stop",
        "fixture_provider": "non-fixture-shipping-port",
    }
    for (source, mode), row in cells.items():
        for key, value in exact.items():
            require_field(row, key, value, f"{labels[key]}:{source}/{mode}")
        require_field(
            row,
            "find_only",
            "true" if mode == "scheduled_find_only" else "false",
            f"find-only-mode:{source}/{mode}",
        )

    aggregate_lines = marked_lines(report, "TASK6210 shipping_carriers=")
    aggregate = fields(aggregate_lines[0]) if aggregate_lines else None
    if aggregate is None:
        refuse("aggregate")
    for key, value in {
        "shipping_carriers": "7",
        "mailbox_sources": "10",
        "modes": "2",
        "cells": "34",
        "A_read_actions": "34",
        "B_actions_before_fresh_approval": "0",
        "account_changed_refusals": "34",
        "stale_selected_approved_cleared": "34",
        "mid_page_barriers": "34",
        "signed_out_events": "0",
        "independent_identity_reads": "493",
        "fresh_B_approvals": "34",
        "control_runs_per_cell": "1",
        "control_runs_total": "34",
        "control_pages": "68",
        "control_actions": "306",
        "scheduled_find_only_delete_refusals": "17",
    }.items():
        require_field(aggregate, key, value, f"aggregate-{key}")

    action_lines = marked_lines(report, "TASK6210_ACTION_MATRIX ")
    action = fields(action_lines[0]) if action_lines else None
    if action is None:
        refuse("action-matrix")
    for key, value in {
        "cells": "34",
        "actions": "list,open,fetch,search,scroll,delete",
        "attacks": "204",
        "first_B_provider_actions": "0",
        "refusals": "204",
        "cleared": "204",
        "independent_identity_reads": "408",
        "requested_ui_self_declared_hints_authoritative": "0",
    }.items():
        require_field(action, key, value, f"action-matrix-{key}")

    required_fragments = {
        "production-mutant": "TASK6210_MUTANT_GATE exit=1 cargo_child_exit=101 cells=34",
        "production-mutant-first-action": f"first_B_bound_action=discord/Discovery/list/{ACCOUNT_B}",
        "production-mutant-all-cells": "mutant_B_actions=34",
        "production-mutant-output-independent": "local_output_suppressed=true",
        "production-mutant-discarded": "deployment_discarded=1",
        "restored-run": "TASK6210_RESTORED exit=0 tests=6/6 B_bound_actions=0",
        "receipt-binding": "TASK6210_RECEIPT_MUTANTS mismatched_account_receipts_refused=1",
        "inventory-starvation": "TASK6210_INVENTORY carrier_routes=7 mail_routes=10 starvation_refusals=17 all_named=true",
        "mutant-observer": "TASK6210_MUTANT_OBSERVER cells=34 B_bound_actions=0",
        "fixture-refusal": "TASK6210_FIXTURE_PROVIDER shipping_consent_minted=0 identity_reads=0 network_actions=0 stale_state=0",
    }
    for label, fragment in required_fragments.items():
        if fragment not in report:
            refuse(label)


def starve(report_path: Path) -> None:
    canonical = report_path.read_text(encoding="utf-8")
    mutations: list[tuple[str, str]] = []
    for source in CARRIERS + MAIL:
        lines = [
            line
            for line in canonical.splitlines()
            if not ("TASK6210_CELL " in line and f"source={source} " in line)
        ]
        mutations.append((f"provider-mode:{source}/discovery", "\n".join(lines) + "\n"))
    for mode in MODES:
        lines = [
            line
            for line in canonical.splitlines()
            if not ("TASK6210_CELL " in line and f"mode={mode} " in line)
        ]
        first_source = sorted(CARRIERS + MAIL)[0]
        mutations.append((f"provider-mode:{first_source}/{mode}", "\n".join(lines) + "\n"))

    field_starvations = [
        ("account-A-identity", f"account_A={ACCOUNT_A}"),
        ("account-B-identity", f"account_B={ACCOUNT_B}"),
        ("A-consent-state", "A_consent=true"),
        ("B-unapproved-state", "B_consent_before_refusal=false"),
        ("stale-state-clear", "selected_approved_after=false"),
        ("mid-page-barrier", "mid_page_barrier=1"),
        ("in-place-switch", "in_place_switch=1"),
        ("same-live-session", "same_live_session=true"),
        ("independent-identity-read", "independent_identity_read=true"),
        ("independent-action-read", "independent_network_action_read=true"),
        ("independent-OSL-state-read", "independent_osl_state_read=true"),
        ("account-changed-refusal", "refusal=account_changed"),
        ("fresh-approval-required", "fresh_approval_required=true"),
        ("fresh-B-approval", "fresh_B_approval=true"),
        ("control-run", "control_runs=1"),
        ("multi-page-run", "multi_page_run=true"),
        ("non-fixture-shipping-port", "fixture_provider=false"),
    ]
    for expected, token in field_starvations:
        mutations.append((f"{expected}:discord/discovery", canonical.replace(token, f"{token}.starved", 1)))

    for expected, fragment in [
        ("production-mutant", "TASK6210_MUTANT_GATE"),
        ("restored-run", "TASK6210_RESTORED"),
        ("action-matrix", "TASK6210_ACTION_MATRIX"),
    ]:
        mutations.append(
            (
                expected,
                "\n".join(line for line in canonical.splitlines() if fragment not in line) + "\n",
            )
        )

    script = Path(__file__).resolve()
    with tempfile.TemporaryDirectory(prefix="task-6210-starvation-") as directory:
        root = Path(directory)
        for index, (expected, candidate) in enumerate(mutations):
            candidate_path = root / f"candidate-{index:02}.log"
            candidate_path.write_text(candidate, encoding="utf-8")
            child = subprocess.run(
                [sys.executable, str(script), "--check", str(candidate_path)],
                capture_output=True,
                text=True,
                check=False,
            )
            if child.returncode != 1 or f"missing={expected}" not in child.stderr:
                print(child.stdout, end="", file=sys.stderr)
                print(child.stderr, end="", file=sys.stderr)
                raise SystemExit(
                    f"TASK6210 starvation child failed expected={expected} exit={child.returncode}"
                )
    print(
        f"TASK6210_STARVATION actual_child_exits_1={len(mutations)} named_missing_classes={len(mutations)} providers=17 modes=2 axes={len(field_starvations)} release_markers=3"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", type=Path)
    group.add_argument("--self-test-starvation", type=Path)
    args = parser.parse_args()
    if args.check:
        check(args.check.read_text(encoding="utf-8"))
        print("TASK6210_EVIDENCE_CHECK ok=true cells=34 actions=204")
    else:
        check(args.self_test_starvation.read_text(encoding="utf-8"))
        starve(args.self_test_starvation)


if __name__ == "__main__":
    main()
