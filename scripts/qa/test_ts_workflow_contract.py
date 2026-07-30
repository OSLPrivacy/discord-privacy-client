from __future__ import annotations

import copy
import re
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github" / "workflows" / "ts-test.yml"


def _workflow() -> dict[str, Any]:
    jobs: dict[str, dict[str, Any]] = {}
    current_job: dict[str, Any] | None = None
    in_needs = False
    in_steps = False
    current_step: dict[str, Any] | None = None
    run_indent: int | None = None
    run_lines: list[str] = []

    def finish_run() -> None:
        nonlocal run_indent, run_lines
        if current_step is not None and run_indent is not None:
            current_step["run"] = "\n".join(run_lines)
        run_indent = None
        run_lines = []

    for raw_line in WORKFLOW.read_text(encoding="utf-8").splitlines():
        if run_indent is not None:
            indent = len(raw_line) - len(raw_line.lstrip(" "))
            if raw_line.strip() == "" or indent >= run_indent:
                run_lines.append(raw_line[run_indent:] if len(raw_line) >= run_indent else "")
                continue
            finish_run()

        job_match = re.fullmatch(r"  ([A-Za-z0-9_-]+):", raw_line)
        if job_match:
            current_job = {"steps": []}
            jobs[job_match.group(1)] = current_job
            in_needs = False
            in_steps = False
            current_step = None
            continue

        if current_job is None:
            continue

        if raw_line == "    needs:":
            current_job["needs"] = []
            in_needs = True
            in_steps = False
            current_step = None
            continue
        if in_needs:
            need_match = re.fullmatch(r"      - ([A-Za-z0-9_-]+)", raw_line)
            if need_match:
                current_job["needs"].append(need_match.group(1))
                continue
            in_needs = False

        if raw_line == "    steps:":
            in_steps = True
            current_step = None
            continue

        job_field = re.fullmatch(r"    ([A-Za-z_-]+): (.+)", raw_line)
        if job_field and not in_steps:
            current_job[job_field.group(1)] = job_field.group(2).strip('"')
            continue

        if not in_steps:
            continue

        step_start = re.fullmatch(r"      - ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_start:
            current_step = {step_start.group(1): step_start.group(2).strip("'\"")}
            current_job["steps"].append(current_step)
            continue
        step_field = re.fullmatch(r"        ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_field and current_step is not None:
            key, value = step_field.group(1), step_field.group(2)
            if key == "run" and value == "|":
                run_indent = 10
                run_lines = []
            else:
                current_step[key] = value.strip("'\"")

    finish_run()
    return {"jobs": jobs}


def _job(workflow: dict[str, Any], job_id: str) -> dict[str, Any]:
    jobs = workflow.get("jobs")
    if not isinstance(jobs, dict) or not isinstance(jobs.get(job_id), dict):
        raise AssertionError(f"workflow job {job_id!r} is missing")
    return jobs[job_id]


def _needs(job: dict[str, Any]) -> set[str]:
    needs = job.get("needs", [])
    if isinstance(needs, str):
        return {needs}
    if isinstance(needs, list):
        return {need for need in needs if isinstance(need, str)}
    return set()


def _step_commands(step: dict[str, Any]) -> list[str]:
    run = step.get("run")
    if not isinstance(run, str):
        return []
    return [line.strip() for line in run.splitlines() if line.strip()]


def _guarded_needs(success_job: dict[str, Any]) -> set[str]:
    guarded: set[str] = set()
    for step in success_job.get("steps", []):
        if not isinstance(step, dict):
            continue
        for command in _step_commands(step):
            match = re.fullmatch(
                r"if \[ '\$\{\{ needs\.([A-Za-z0-9_-]+)\.result \}\}' != 'success' \]; then",
                command,
            )
            if match:
                guarded.add(match.group(1))
    return guarded


def _audit_success_gate(workflow: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    success = _job(workflow, "success")
    required = {"test", "selector-check", "telegram-reporting-bot", "public-audit"}

    if success.get("name") != "success'":
        errors.append("success job must keep the exact aggregate check name")
    if success.get("if") != "${{ always() }}":
        errors.append("success job must run even after a dependency fails")
    if not required.issubset(_needs(success)):
        errors.append("success job must depend on every TypeScript, selector, and audit lane")
    if not required.issubset(_guarded_needs(success)):
        errors.append("success job must fail closed on every required lane result")

    steps = success.get("steps", [])
    exit_commands = [
        command
        for step in steps
        if isinstance(step, dict)
        for command in _step_commands(step)
        if command.startswith("exit ")
    ]
    if exit_commands != ['exit "$failed"']:
        errors.append("success job must exit with the accumulated failure state")

    return errors


def _audit_claim_gate(workflow: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    test_job = _job(workflow, "test")
    steps = test_job.get("steps", [])
    if not isinstance(steps, list):
        return ["test job must have steps"]

    install_indexes = [
        index
        for index, step in enumerate(steps)
        if isinstance(step, dict)
        and isinstance(step.get("name"), str)
        and step["name"].startswith("Install ")
    ]
    gate_indexes = [
        index
        for index, step in enumerate(steps)
        if isinstance(step, dict)
        and step.get("name") == "Claim-gate-in-app-copy-and-README"
    ]
    if len(gate_indexes) != 1:
        errors.append("test job must have exactly one app-claim gate step")
        return errors
    if install_indexes and gate_indexes[0] > min(install_indexes):
        errors.append("app-claim gate must run before dependency installation")

    commands = _step_commands(steps[gate_indexes[0]])
    if commands != [
        "node scripts/check-app-claims.mjs --self-test",
        "node scripts/check-app-claims.mjs",
    ]:
        errors.append("app-claim gate must run self-test and repository scan in order")
    if steps[gate_indexes[0]].get("working-directory") is not None:
        errors.append("app-claim gate must run from the repository root")

    return errors


def _claim_gate_step_index(workflow: dict[str, Any]) -> int:
    steps = _job(workflow, "test")["steps"]
    return next(
        index
        for index, step in enumerate(steps)
        if isinstance(step, dict)
        and step.get("name") == "Claim-gate-in-app-copy-and-README"
    )


def ts_success_gate_contract() -> None:
    workflow = _workflow()
    testcase = unittest.TestCase()
    testcase.assertEqual(_audit_success_gate(workflow), [])

    missing_public_audit = copy.deepcopy(workflow)
    missing_public_audit["jobs"]["success"]["needs"].remove("public-audit")
    testcase.assertIn(
        "success job must depend on every TypeScript, selector, and audit lane",
        _audit_success_gate(missing_public_audit),
    )

    permissive_selector_check = copy.deepcopy(workflow)
    run = permissive_selector_check["jobs"]["success"]["steps"][0]["run"]
    permissive_selector_check["jobs"]["success"]["steps"][0]["run"] = run.replace(
        "needs.selector-check.result }}' != 'success'",
        "needs.selector-check.result }}' == 'success'",
    )
    testcase.assertIn(
        "success job must fail closed on every required lane result",
        _audit_success_gate(permissive_selector_check),
    )


ts_success_gate_contract.__name__ = "success'"


def app_claim_gate_workflow_contract() -> None:
    workflow = _workflow()
    testcase = unittest.TestCase()
    testcase.assertEqual(_audit_claim_gate(workflow), [])

    no_self_test = copy.deepcopy(workflow)
    step = no_self_test["jobs"]["test"]["steps"][_claim_gate_step_index(no_self_test)]
    step["run"] = "\n".join(
        command
        for command in _step_commands(step)
        if command != "node scripts/check-app-claims.mjs --self-test"
    )
    testcase.assertIn(
        "app-claim gate must run self-test and repository scan in order",
        _audit_claim_gate(no_self_test),
    )

    delayed_gate = copy.deepcopy(workflow)
    steps = delayed_gate["jobs"]["test"]["steps"]
    gate_step = steps.pop(_claim_gate_step_index(delayed_gate))
    steps.insert(4, gate_step)
    testcase.assertIn(
        "app-claim gate must run before dependency installation",
        _audit_claim_gate(delayed_gate),
    )


app_claim_gate_workflow_contract.__name__ = "scripts/check-app-claims.mjs'"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(ts_success_gate_contract))
    suite.addTest(unittest.FunctionTestCase(app_claim_gate_workflow_contract))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
