"""D-281 / D-194 — the contract on ts-test.yml's cipher-store lane.

WHAT THIS FILE IS FOR.

`.github/workflows/ts-test.yml` now carries the terminal anchor for
cipher-store-cf's release contract: two steps that run the seven named
security-property suites BY EXPLICIT PATH, against a second copy of the names
held in `scripts/ci/d2-property-spec-anchor.mjs`. D-281's whole point is that no
in-suite gate survives deletion of the gate itself, so the anchor lives outside
the suite -- and an anchor that can be deleted with one line of YAML is an anchor
that lives outside the suite and inside nothing.

So the steps are asserted, and the ASSERTION RUNS FROM A DIFFERENT WORKFLOW:
`quality-checks` in rust-test.yml executes this file, for the same reason it
executes the Rust and integration-branch contracts. A contract enforced only by
the thing it constrains is not enforced.

AND IT ASSERTS THE ORDERING, WHICH IS NOT A STYLE POINT.

Actions aborts a job at its first failing step. A gate placed ABOVE a test step
that carries no `if:` does not merely fail -- it SKIPS that test, and a skipped
test is reported as neither red nor green. This repository has now paid for that
three times: `cargo fmt` at position 5 of the `test` job hid clippy, the whole
workspace suite, nextest and both hub steps (D-172); hoisting it promoted
`cargo test --workspace` into the same position and hid four more (D-194); and
the hub lint ratchet, as originally recommended, would have been inserted in
front of `Test the OSL Privacy desktop binary` and silently skipped it (D-268).

The anchor is therefore BELOW `Test cipher-store worker`, and every step in the
lane below the first carries `!cancelled()` so a red one cannot blank a later
one. Both halves are asserted here, positionally, not trusted.

Zero third-party dependencies on purpose. `quality-checks` runs this under
`if: ${{ !cancelled() }}`, so it must not need a `pip install` that a red step
above it might never have reached.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TS_TEST = ROOT / ".github" / "workflows" / "ts-test.yml"
RUST_TEST = ROOT / ".github" / "workflows" / "rust-test.yml"
ANCHOR = ROOT / "scripts" / "ci" / "d2-property-spec-anchor.mjs"

ANCHOR_COMMAND = "node scripts/ci/d2-property-spec-anchor.mjs"
ANCHOR_SELF_TEST = f"{ANCHOR_COMMAND} --self-test"
CIPHER_SUITE_STEP = "Test cipher-store worker"


def _yaml_scalar(value: str) -> str:
    raw = value.strip()
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in {"'", '"'}:
        return raw[1:-1]
    return raw


def _job_block(path: Path, job_name: str) -> list[str]:
    lines = path.read_text(encoding="utf-8").splitlines()
    in_jobs = False
    in_job = False
    collected: list[str] = []
    for line in lines:
        if re.match(r"^jobs:\s*$", line):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        if re.match(r"^\S", line):
            if in_job:
                break
            in_jobs = False
            continue
        job_match = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if job_match:
            if in_job:
                break
            in_job = job_match.group(1) == job_name
            continue
        if in_job:
            collected.append(line)
    assert collected, f"{path} declares no job named {job_name}"
    return collected


def _steps(path: Path, job_name: str) -> list[dict[str, str]]:
    """Every step of a job, in order, as {name, run, uses, if}.

    A hand-rolled reader rather than PyYAML: this file is run under
    `!cancelled()` from a job whose `pip install` step may itself be red, and a
    contract that cannot execute when something above it fails is the masking
    pattern it exists to forbid.
    """
    steps: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    key: str | None = None
    for line in _job_block(path, job_name):
        start = re.match(r"^      - (\S+):\s*(.*)$", line)
        if start:
            if current is not None:
                steps.append(current)
            current = {start.group(1): _yaml_scalar(start.group(2))}
            key = start.group(1)
            continue
        if current is None:
            continue
        field = re.match(r"^        ([A-Za-z0-9_-]+):\s*(.*)$", line)
        if field:
            key = field.group(1)
            current[key] = _yaml_scalar(field.group(2))
            continue
        continuation = re.match(r"^\s{10,}(\S.*)$", line)
        if continuation and key is not None:
            current[key] = (current[key] + " " + continuation.group(1).strip()).strip()
    if current is not None:
        steps.append(current)
    assert steps, f"{path} job {job_name} has no steps"
    return steps


def _index_of_run(steps: list[dict[str, str]], command: str) -> int:
    for index, step in enumerate(steps):
        if step.get("run", "").strip() == command:
            return index
    raise AssertionError(f"no step runs `{command}`")


def _index_of_name(steps: list[dict[str, str]], name: str) -> int:
    for index, step in enumerate(steps):
        if step.get("name", "") == name:
            return index
    raise AssertionError(f"no step named `{name}`")


class TerminalAnchorIsWiredTest(unittest.TestCase):
    """D-281: the anchor exists, runs, and is proved able to fail first."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.steps = _steps(TS_TEST, "ts-test")

    def test_the_anchor_script_exists(self) -> None:
        self.assertTrue(
            ANCHOR.is_file(),
            "scripts/ci/d2-property-spec-anchor.mjs is gone; D-281's terminal anchor "
            "has no grader",
        )

    def test_some_step_runs_the_anchor(self) -> None:
        self.assertGreaterEqual(
            _index_of_run(self.steps, ANCHOR_COMMAND),
            0,
            "no step runs the D-281 terminal anchor",
        )

    def test_the_grader_is_proved_able_to_fail_before_it_grades(self) -> None:
        # D-195. A grader nobody has seen refuse is decoration. Its self-test is
        # a step of its own so its exit code is a step result, and it runs first
        # so a broken grader is reported as a broken grader.
        self.assertLess(
            _index_of_run(self.steps, ANCHOR_SELF_TEST),
            _index_of_run(self.steps, ANCHOR_COMMAND),
            "the anchor's self-test must run before the anchor grades anything",
        )

    def test_both_anchor_steps_run_in_the_cipher_store_lane(self) -> None:
        for command in (ANCHOR_SELF_TEST, ANCHOR_COMMAND):
            condition = self.steps[_index_of_run(self.steps, command)].get("if", "")
            self.assertIn(
                "matrix.lane == 'cipher-store-worker'",
                condition,
                f"`{command}` is not confined to the lane whose deps it needs",
            )


class TheAnchorCannotMaskTheSuiteTest(unittest.TestCase):
    """D-194, positionally. This is the assertion the brief asked for."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.steps = _steps(TS_TEST, "ts-test")

    def test_the_anchor_runs_after_the_suite_it_backstops(self) -> None:
        suite = _index_of_name(self.steps, CIPHER_SUITE_STEP)
        for command in (ANCHOR_SELF_TEST, ANCHOR_COMMAND):
            self.assertGreater(
                _index_of_run(self.steps, command),
                suite,
                "the anchor is ABOVE `Test cipher-store worker`. Actions aborts a "
                "job at its first failing step, so a red anchor there SKIPS the "
                "suite -- D-194, verbatim, for the third time in this repository.",
            )

    def test_a_red_suite_cannot_skip_the_anchor(self) -> None:
        for command in (ANCHOR_SELF_TEST, ANCHOR_COMMAND):
            condition = self.steps[_index_of_run(self.steps, command)].get("if", "")
            self.assertIn(
                "!cancelled()",
                condition,
                f"`{command}` has no `!cancelled()`, so `npm test` going red above "
                "it reports it as `skipped` -- a result nobody has",
            )

    def test_the_suite_step_itself_still_carries_the_condition(self) -> None:
        # Symmetry: moving the anchor above the suite is caught by the test
        # above, and dropping the suite's own condition would let a red anchor
        # matter again if anybody ever did reorder.
        condition = self.steps[_index_of_name(self.steps, CIPHER_SUITE_STEP)].get("if", "")
        self.assertIn("!cancelled()", condition)

    def test_no_step_in_the_lane_below_the_first_can_be_masked(self) -> None:
        # Measured from the first `run:` step, not from the top of the job: the
        # `checkout`/`setup-node` prologue is `uses:` only and a failure there is
        # a runner that never became usable, not one gate hiding another.
        first_run = next(index for index, step in enumerate(self.steps) if "run" in step)
        missing = [
            step.get("name", step.get("run", "?"))
            for step in self.steps[first_run + 1 :]
            if "run" in step
            and "!cancelled()" not in step.get("if", "")
            and step.get("run", "").strip() != "npm ci"
        ]
        # The per-lane `npm ci` steps are the deliberate exception: each is the
        # FIRST step of its own lane, nothing precedes it within that lane, and a
        # lane whose install failed has nothing to run anyway.
        self.assertEqual(
            missing,
            [],
            f"steps that a red step above them can silently skip: {missing}",
        )


class TheAnchorIsNotSatisfiedByTheSuiteTest(unittest.TestCase):
    """D-285 / D-281: it must not take its demands from what it is guarding."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.source = ANCHOR.read_text(encoding="utf-8")

    def test_it_carries_its_own_list_of_names(self) -> None:
        self.assertIn(
            "export const ANCHORED_SPECS",
            self.source,
            "the anchor no longer holds its own copy of the named suites",
        )
        suites = re.findall(r"^\s+suite: '((?:\\.|[^'])*)',$", self.source, re.M)
        self.assertGreaterEqual(
            len(suites),
            7,
            f"the anchor names {len(suites)} suites; D-281 names 7. A short list is "
            "the omission-shaped failure the floor inside the suite exists for, and "
            "the floor is in-suite.",
        )

    def test_it_does_not_run_the_suites_own_test_script(self) -> None:
        # `npm test` is a string in package.json. Running it would make the
        # anchor a second consumer of the configuration D-281 says cannot anchor
        # itself.
        #
        # Graded on the SPAWNED COMMANDS, not on the file text. The first draft
        # of this assertion did a substring search and went red on the word
        # "npm test" inside the script's own header comment -- a checker reading
        # prose as if it were code, which is D-285's family and would have been a
        # permanently red job for a sentence.
        spawned = re.findall(r"spawnSync\(\s*([^,]+),", self.source)
        self.assertTrue(spawned, "the anchor spawns nothing; it cannot be running any spec")
        for command in spawned:
            self.assertNotIn(
                "npm",
                command,
                f"the anchor spawns {command.strip()}; it must not go through "
                "package.json's scripts",
            )

    def test_it_runs_the_named_files_by_explicit_path(self) -> None:
        self.assertIn("ANCHORED_SPECS.map((spec) => spec.file)", self.source)
        self.assertIn("'vitest', 'run', ...files", self.source)

    def test_it_grades_the_report_rather_than_the_exit_code(self) -> None:
        # `describe.skip` leaves vitest at exit 0. A gate reading only the exit
        # code is green on a suite that ran nothing.
        self.assertIn("--reporter=json", self.source)
        self.assertIn("status !== 'passed'", self.source)

    def test_an_empty_report_is_not_a_pass(self) -> None:
        self.assertIn("ZERO tests", self.source)


class ThisContractRunsFromAnotherWorkflowTest(unittest.TestCase):
    """The lesson of D-283, applied to this file on the day it was written."""

    def test_a_job_outside_this_workflow_executes_this_file(self) -> None:
        # D-283 is the defect where `scripts/test_root_cargo_ci_contract.py` was
        # run by nothing at all. Writing a second contract file and not wiring it
        # would be that defect, committed by the lane fixing it.
        rust = RUST_TEST.read_text(encoding="utf-8")
        self.assertIn(
            "python3 .github/workflows/ts-test.workflow-test.py",
            rust,
            "this contract is executed by no workflow -- D-283, again",
        )

    def test_it_runs_from_a_job_that_is_not_the_one_it_grades(self) -> None:
        quality = _steps(RUST_TEST, "quality-checks")
        index = _index_of_run(quality, "python3 .github/workflows/ts-test.workflow-test.py")
        self.assertIn("!cancelled()", quality[index].get("if", ""))


if __name__ == "__main__":
    unittest.main(verbosity=2)
