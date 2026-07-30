from __future__ import annotations

import copy
import os
import re
import subprocess
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github" / "workflows" / "ts-test.yml"
REQUIRED_SUCCESS_LANES = {
    "test": ("TEST_RESULT", "TypeScript workflow"),
    "selector-check": ("SELECTOR_CHECK_RESULT", "Selector check"),
    "telegram-reporting-bot": (
        "TELEGRAM_REPORTING_BOT_RESULT",
        "Telegram reporting bot script check",
    ),
    "public-audit": ("PUBLIC_AUDIT_RESULT", "Public audit"),
}


def _yaml_scalar(value: str) -> str:
    raw = value.strip()
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in {"'", '"'}:
        return raw[1:-1]
    return raw


def _workflow() -> dict[str, Any]:
    jobs: dict[str, dict[str, Any]] = {}
    current_job: dict[str, Any] | None = None
    in_needs = False
    in_steps = False
    in_step_env = False
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
            in_step_env = False
            current_step = None
            continue

        if current_job is None:
            continue

        if raw_line == "    needs:":
            current_job["needs"] = []
            in_needs = True
            in_steps = False
            in_step_env = False
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
            in_step_env = False
            current_step = None
            continue

        job_field = re.fullmatch(r"    ([A-Za-z_-]+): (.+)", raw_line)
        if job_field and not in_steps:
            current_job[job_field.group(1)] = _yaml_scalar(job_field.group(2))
            continue

        if not in_steps:
            continue

        step_start = re.fullmatch(r"      - ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_start:
            current_step = {step_start.group(1): _yaml_scalar(step_start.group(2))}
            current_job["steps"].append(current_step)
            in_step_env = False
            continue
        if in_step_env and current_step is not None:
            env_field = re.fullmatch(r"          ([A-Za-z0-9_]+): (.+)", raw_line)
            if env_field:
                env = current_step.setdefault("env", {})
                assert isinstance(env, dict)
                env[env_field.group(1)] = _yaml_scalar(env_field.group(2))
                continue
            in_step_env = False
        step_field = re.fullmatch(r"        ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_field and current_step is not None:
            key, value = step_field.group(1), step_field.group(2)
            if key == "run" and value == "|":
                run_indent = 10
                run_lines = []
                in_step_env = False
            elif key == "env" and value == "":
                current_step["env"] = {}
                in_step_env = True
            else:
                current_step[key] = _yaml_scalar(value)
                in_step_env = False

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


def _success_step(workflow: dict[str, Any]) -> dict[str, Any]:
    steps = _job(workflow, "success").get("steps", [])
    matches = [
        step
        for step in steps
        if isinstance(step, dict) and step.get("name") == "success'"
    ]
    if len(matches) != 1:
        raise AssertionError("success job must have exactly one success' step")
    return matches[0]


def _run_success_step(
    step: dict[str, Any],
    results: dict[str, str],
) -> subprocess.CompletedProcess[str]:
    run = step.get("run")
    if not isinstance(run, str) or not run.strip():
        raise AssertionError("success' step must have a shell run block")
    env = os.environ.copy()
    for job_id, result in results.items():
        env_name, _label = REQUIRED_SUCCESS_LANES[job_id]
        env[env_name] = result
    return subprocess.run(
        ["bash", "-e", "-c", run],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def _audit_success_step_behavior(workflow: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    step = _success_step(workflow)
    green_results = {job_id: "success" for job_id in REQUIRED_SUCCESS_LANES}
    green = _run_success_step(step, green_results)
    if green.returncode != 0:
        errors.append(
            "success job must accept an all-green required-check set"
            f" (stdout={green.stdout!r}, stderr={green.stderr!r})"
        )

    failing_cases = {
        "test": "failure",
        "selector-check": "cancelled",
        "telegram-reporting-bot": "skipped",
        "public-audit": "failure",
    }
    for job_id, result in failing_cases.items():
        results = dict(green_results)
        results[job_id] = result
        completed = _run_success_step(step, results)
        _env_name, label = REQUIRED_SUCCESS_LANES[job_id]
        expected = f"{label} failed with result: {result}"
        if completed.returncode == 0:
            errors.append(f"success job must fail closed when {job_id} is {result}")
        if completed.stdout.strip() != expected:
            errors.append(
                f"success job must report the failed {job_id} result"
                f" (expected={expected!r}, stdout={completed.stdout!r})"
            )
    return errors


def _audit_success_gate(workflow: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    success = _job(workflow, "success")
    required = set(REQUIRED_SUCCESS_LANES)

    if success.get("name") != "success'":
        errors.append("success job must keep the exact aggregate check name")
    if success.get("if") != "${{ always() }}":
        errors.append("success job must run even after a dependency fails")
    if not required.issubset(_needs(success)):
        errors.append("success job must depend on every TypeScript, selector, and audit lane")
    try:
        step = _success_step(workflow)
    except AssertionError as exc:
        errors.append(str(exc))
        return errors

    env = step.get("env", {})
    if not isinstance(env, dict):
        env = {}
    expected_env = {
        env_name: f"${{{{ needs.{job_id}.result }}}}"
        for job_id, (env_name, _label) in REQUIRED_SUCCESS_LANES.items()
    }
    for env_name, expression in expected_env.items():
        if env.get(env_name) != expression:
            errors.append("success job must bind every required lane result into the aggregate gate")
            break

    errors.extend(_audit_success_step_behavior(workflow))

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

    missing_public_audit_binding = copy.deepcopy(workflow)
    env = missing_public_audit_binding["jobs"]["success"]["steps"][0]["env"]
    del env["PUBLIC_AUDIT_RESULT"]
    testcase.assertIn(
        "success job must bind every required lane result into the aggregate gate",
        _audit_success_gate(missing_public_audit_binding),
    )

    permissive_selector_check = copy.deepcopy(workflow)
    run = permissive_selector_check["jobs"]["success"]["steps"][0]["run"]
    permissive_selector_check["jobs"]["success"]["steps"][0]["run"] = run.replace(
        '"Selector check=$SELECTOR_CHECK_RESULT"',
        '"Selector check=success"',
    )
    testcase.assertIn(
        "success job must fail closed when selector-check is cancelled",
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
