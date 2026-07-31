#!/usr/bin/env python3
"""Static contract audit for the OSL Privacy reproducible-build workflow."""

from __future__ import annotations

import argparse
import copy
import re
import sys
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "reproducible-build.yml"


def require(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def _workflow_model(text: str) -> dict[str, Any]:
    jobs: dict[str, dict[str, Any]] = {}
    current_job: dict[str, Any] | None = None
    in_steps = False
    current_step: dict[str, Any] | None = None
    in_env = False
    run_indent: int | None = None
    run_lines: list[str] = []

    def unquote(value: str) -> str:
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
            return value[1:-1]
        return value

    def finish_run() -> None:
        nonlocal run_indent, run_lines
        if current_step is not None and run_indent is not None:
            current_step["run"] = "\n".join(run_lines)
        run_indent = None
        run_lines = []

    for raw_line in text.splitlines():
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
            current_step = None
            in_steps = False
            in_env = False
            continue

        if current_job is None:
            continue

        if raw_line == "    steps:":
            in_steps = True
            current_step = None
            in_env = False
            continue

        job_field = re.fullmatch(r"    ([A-Za-z_-]+): (.+)", raw_line)
        if job_field and not in_steps:
            current_job[job_field.group(1)] = unquote(job_field.group(2))
            continue

        if not in_steps:
            continue

        step_start = re.fullmatch(r"      - ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_start:
            current_step = {step_start.group(1): unquote(step_start.group(2))}
            current_job["steps"].append(current_step)
            in_env = False
            continue

        if current_step is None:
            continue

        if raw_line == "        env:":
            current_step["env"] = {}
            in_env = True
            continue

        if in_env:
            env_field = re.fullmatch(r"          ([A-Za-z0-9_]+): (.+)", raw_line)
            if env_field:
                current_step["env"][env_field.group(1)] = unquote(env_field.group(2))
                continue
            in_env = False

        step_field = re.fullmatch(r"        ([A-Za-z_-]+): ?(.*)", raw_line)
        if step_field:
            key, value = step_field.group(1), step_field.group(2)
            if key == "run" and value == "|":
                run_indent = 10
                run_lines = []
            else:
                current_step[key] = unquote(value)

    finish_run()
    return {"jobs": jobs}


def _workflow() -> dict[str, Any]:
    return _workflow_model(WORKFLOW.read_text(encoding="utf-8"))


def _job(workflow: dict[str, Any], job_id: str) -> dict[str, Any]:
    jobs = workflow.get("jobs")
    if not isinstance(jobs, dict) or not isinstance(jobs.get(job_id), dict):
        raise AssertionError(f"workflow job {job_id!r} is missing")
    return jobs[job_id]


def _step(job: dict[str, Any], name: str) -> dict[str, Any]:
    matches = [
        step
        for step in job.get("steps", [])
        if isinstance(step, dict) and step.get("name") == name
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected exactly one step named {name!r}")
    return matches[0]


def _audit_success_step(workflow: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    job = _job(workflow, "reproduce-hub-release")
    success = _step(job, "success'")
    run = success.get("run")
    env = success.get("env")
    if success.get("shell") != "pwsh":
        errors.append("success step must run under PowerShell")
    if not isinstance(env, dict) or set(env) != {
        "CANDIDATE_TAG",
        "SOURCE_COMMIT",
        "SOURCE_TREE",
    }:
        errors.append("success step must bind candidate tag, source commit, and source tree")
    if not isinstance(run, str):
        return errors + ["success step must execute proof verification commands"]

    required = {
        "proof": (
            "$proof = Get-Content reproducible-build-proof.json -Raw | ConvertFrom-Json",
            "success step must read the retained reproducibility proof",
        ),
        "released-hash": (
            "$releasedHash = (Get-FileHash $env:RELEASED_EXE -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success step must recompute the released executable hash",
        ),
        "rebuilt-hash": (
            "$rebuiltHash = (Get-FileHash $env:REBUILT_EXE -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success step must recompute the rebuilt executable hash",
        ),
        "installer-hash": (
            "$installerHash = (Get-FileHash $env:RELEASE_INSTALLER -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success step must recompute the released installer hash",
        ),
        "actual-commit": (
            "$actualCommit = (git rev-parse HEAD).Trim()",
            "success step must read the checked-out commit",
        ),
        "actual-tree": (
            "$actualTree = (git rev-parse HEAD^{tree}).Trim()",
            "success step must read the checked-out source tree",
        ),
    }
    for needle, message in required.values():
        if needle not in run:
            errors.append(message)

    def has_closed_guard(pattern: str) -> bool:
        return re.search(pattern, run, re.DOTALL) is not None

    if not has_closed_guard(
        r"\$actualCommit -cne \$env:SOURCE_COMMIT -or "
        r"\$actualTree -cne \$env:SOURCE_TREE.*?"
        r"reproducibility proof was not produced from the resolved exact source"
    ):
        errors.append("success step must fail unless the proof was produced from the exact source")
    if not has_closed_guard(
        r"\$proof\.candidateTag -cne \$env:CANDIDATE_TAG -or\s+"
        r"\$proof\.sourceCommit -cne \$env:SOURCE_COMMIT -or\s+"
        r"\$proof\.sourceTree -cne \$env:SOURCE_TREE.*?"
        r"reproducibility proof is not bound to the resolved exact source"
    ):
        errors.append("success step must bind the proof to the resolved source")
    if not has_closed_guard(
        r"\$proof\.installer\.name -cne \$env:RELEASE_INSTALLER_NAME -or\s+"
        r"\$proof\.installer\.sha256 -cne \$installerHash.*?"
        r"reproducibility proof is not bound to the released installer bytes"
    ):
        errors.append("success step must bind the proof to the released installer")
    if not has_closed_guard(
        r"\$proof\.releasedExecutable\.sha256 -cne \$releasedHash -or\s+"
        r"\$proof\.rebuiltExecutable\.sha256 -cne \$rebuiltHash -or\s+"
        r"\$releasedHash -cne \$rebuiltHash.*?"
        r"released executable bytes do not reproduce from exact source"
    ):
        errors.append("success step must fail unless released and rebuilt bytes match")
    return errors


def audit_text(text: str) -> list[str]:
    errors: list[str] = []
    try:
        errors.extend(_audit_success_step(_workflow_model(text)))
    except AssertionError as exc:
        errors.append(str(exc))
    require(
        "push:\n    branches:\n      - main\n    tags:\n      - \"hub-v*\"" in text,
        "workflow must run on main pushes and hub-v* tags",
        errors,
    )
    require("candidate_tag:" in text, "manual dispatch must accept an explicit candidate tag", errors)
    require("permissions:\n  contents: read" in text, "workflow permissions must be read-only", errors)
    require("fetch-depth: 0" in text, "workflow must fetch the full tag graph", errors)
    require(
        "candidate_tag must be a bounded hub-v* tag" in text
        and "^hub-v[0-9A-Za-z.+-]{1,64}$" in text,
        "candidate tags must be bounded and product-scoped",
        errors,
    )
    require(
        "git rev-list -n 1 \"$candidateTag^{commit}\"" in text
        and "git checkout --detach $tagCommit" in text,
        "workflow must checkout the exact release tag commit before building",
        errors,
    )
    require(
        "hub-v$($config.version)" in text
        and "does not match source version" in text,
        "release tag must match apps/osl-hub/tauri.conf.json",
        errors,
    )
    require(
        "gh release view $candidateTag --json tagName --jq .tagName" in text,
        "workflow must verify the GitHub release tag binding",
        errors,
    )
    require(
        "gh release download $env:CANDIDATE_TAG --pattern \"*.exe\" --dir candidate" in text
        and "release must contain exactly one Windows installer" in text,
        "workflow must download exactly one released Windows installer",
        errors,
    )
    require(
        "7z x \"$env:RELEASE_INSTALLER\"" in text
        and "-Filter \"osl-privacy-hub.exe\"" in text
        and "must contain exactly one OSL Privacy Hub executable" in text,
        "workflow must extract exactly one released Hub executable",
        errors,
    )
    require(
        "working-directory: apps/osl-hub-ui" in text
        and "npm ci" in text
        and "npm run build" in text,
        "workflow must rebuild the committed frontend first",
        errors,
    )
    require(
        "--manifest-path apps/osl-hub/Cargo.toml" in text
        and "--release --features desktop --bin osl-privacy-hub" in text,
        "workflow must rebuild the Hub executable with the release feature set",
        errors,
    )
    require(
        "RUNNER_TEMP" in text
        and "reproducible build target directory already exists" in text
        and "CARGO_TARGET_DIR" in text,
        "workflow must use a born-empty runner-owned target directory",
        errors,
    )
    require(
        "$env:RELEASED_EXE_SHA256 -cne $env:REBUILT_EXE_SHA256" in text
        and "released executable bytes do not reproduce from exact source" in text
        and "released sha256=$env:RELEASED_EXE_SHA256 rebuilt sha256=$env:REBUILT_EXE_SHA256" in text,
        "workflow must compare released and rebuilt bytes by digest and report both digests",
        errors,
    )
    require(
        "reproducible-build-proof.json" in text
        and "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02" in text,
        "workflow must retain a pinned proof artifact",
        errors,
    )
    require(
        "discord-privacy-client.exe" not in text
        and "release-deterministic" not in text
        and "Hash artifact" not in text,
        "workflow must not use the stale legacy executable proof",
        errors,
    )

    unpinned_actions = []
    for number, line in enumerate(text.splitlines(), 1):
        if "uses:" not in line:
            continue
        if not re.match(r"^\s*-?\s*uses:\s*[^#\s]+@[0-9a-f]{40}\s*(?:#.*)?$", line):
            unpinned_actions.append(number)
    require(not unpinned_actions, f"GitHub Actions must be pinned: {unpinned_actions}", errors)
    return errors


def audit_workflow(path: Path = WORKFLOW) -> list[str]:
    return audit_text(path.read_text(encoding="utf-8"))


class ReproducibleBuildWorkflowTests(unittest.TestCase):
    def workflow(self) -> str:
        return WORKFLOW.read_text(encoding="utf-8")

    def test_current_workflow_satisfies_contract(self) -> None:
        self.assertEqual(audit_workflow(), [])

    def test_refuses_branch_head_build_as_release_source(self) -> None:
        mutant = self.workflow().replace("git checkout --detach $tagCommit", "# checkout omitted")
        self.assertIn(
            "workflow must checkout the exact release tag commit before building",
            audit_text(mutant),
        )

    def test_refuses_legacy_double_build_hash_workflow(self) -> None:
        legacy = """
name: Reproducible Build
jobs:
  build:
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
      - name: Build deterministic
        run: cargo build --profile release-deterministic
      - name: Hash artifact
        run: Get-FileHash target\\release-deterministic\\discord-privacy-client.exe
"""
        errors = audit_text(legacy)
        self.assertIn("workflow must not use the stale legacy executable proof", errors)
        self.assertIn(
            "workflow must compare released and rebuilt bytes by digest and report both digests",
            errors,
        )

    def test_refuses_missing_release_download(self) -> None:
        mutant = self.workflow().replace(
            "gh release download $env:CANDIDATE_TAG --pattern \"*.exe\" --dir candidate",
            "# release download omitted",
        )
        self.assertIn(
            "workflow must download exactly one released Windows installer",
            audit_text(mutant),
        )


def reproducible_success_contract() -> None:
    workflow = _workflow()
    testcase = unittest.TestCase()
    testcase.assertEqual(_audit_success_step(workflow), [])

    missing_source_tree = copy.deepcopy(workflow)
    missing_source_tree_step = _step(
        _job(missing_source_tree, "reproduce-hub-release"), "success'"
    )
    del missing_source_tree_step["env"]["SOURCE_TREE"]
    testcase.assertIn(
        "success step must bind candidate tag, source commit, and source tree",
        _audit_success_step(missing_source_tree),
    )

    branch_head_proof = copy.deepcopy(workflow)
    branch_head_step = _step(
        _job(branch_head_proof, "reproduce-hub-release"), "success'"
    )
    branch_head_step["run"] = branch_head_step["run"].replace(
        "$actualTree = (git rev-parse HEAD^{tree}).Trim()",
        "$actualTree = $env:SOURCE_TREE",
    )
    testcase.assertIn(
        "success step must read the checked-out source tree",
        _audit_success_step(branch_head_proof),
    )

    permissive_byte_compare = copy.deepcopy(workflow)
    permissive_step = _step(
        _job(permissive_byte_compare, "reproduce-hub-release"), "success'"
    )
    permissive_step["run"] = permissive_step["run"].replace(
        "$releasedHash -cne $rebuiltHash",
        "$releasedHash -ceq $rebuiltHash",
    )
    testcase.assertIn(
        "success step must fail unless released and rebuilt bytes match",
        _audit_success_step(permissive_byte_compare),
    )


reproducible_success_contract.__name__ = "success'"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, pattern
    tests.addTest(unittest.FunctionTestCase(reproducible_success_contract))
    return tests


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ReproducibleBuildWorkflowTests)
        suite.addTest(unittest.FunctionTestCase(reproducible_success_contract))
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1

    errors = audit_workflow()
    if errors:
        print("Reproducible-build workflow audit failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("OK: reproducible-build workflow binds released bytes to the exact Hub source tag")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
