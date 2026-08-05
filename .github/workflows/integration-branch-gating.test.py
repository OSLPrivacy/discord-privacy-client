"""D-172 policy test: the integration branch must stay gated.

Run with: python3 .github/workflows/integration-branch-gating.test.py

WHY
---
`gh run list --branch integrate/first-usable` returned NOTHING until 2026-08-04.
Every workflow triggered on `push: branches: [main]` plus `pull_request`, and the
integration branch is pushed to neither -- so every gate in this directory was a
statement about `main`, not about the branch we integrate into and cut builds
from. D-162 (an import with no matching export, undetected for two days) and
D-146 (an integration head that could not build the shipping app at all while
three suites were green) both came straight out of that.

A trigger is one line and one line is easy to lose in a merge. This file is what
notices. It is deliberately two-sided: it fails if a gate stops covering the
integration branch, AND it fails if a workflow that deploys or publishes starts
covering it.

D-195 -- WHAT WAS WRONG WITH THIS FILE
--------------------------------------
`test_publishing_workflows_never_fire_on_an_integration_branch` executed ZERO
assertions. It read `push.branches` for each of the five workflows that must
never fire on a WIP branch, and every one of them returned `[]` -- they are
tags-only, `schedule`, or `workflow_dispatch` -- so all five inner loops were
empty. Worse, a BARE `push:`, which fires on every branch in the repository
including `integrate/**`, ALSO returns `[]`. Wiring the signed-release workflow
onto every integration push would have passed this test green. It was the only
thing asserting that release and promote cannot fire on a work-in-progress
branch, and it was a gate that could not fail.

The replacement asserts POSITIVELY: for each workflow it determines WHY the
integration branch cannot reach it, requires that reason to be one of an
explicit recorded set, and requires the recorded reason to be the one that is
actually true today. An empty `push:` has no such reason and fails.

`test_the_glob_actually_matches_the_branch_we_ship_from` was a
constant-vs-constant tautology using Python `pathlib` glob semantics, which are
not Actions'. It now evaluates Actions' own filter-pattern rules against the
branch lists really written in the gating workflows, with negative controls.
"""

from pathlib import Path
import re
import unittest

import yaml


WORKFLOWS = Path(__file__).parent
REPO = WORKFLOWS.parent.parent
INTEGRATION_GLOB = "integrate/**"
SHIPPING_BRANCH = "integrate/first-usable"

# Workflows that MUST run on every push to an integration branch.
# The value is why, so a future reader has to argue with a reason, not a list.
MUST_GATE_INTEGRATION = {
    "integration-gate.yml":
        "owns the four shipping-artifact build gates: the cargo workspace, "
        "apps/osl-hub with --features desktop, the frontend production build, "
        "and both Cloudflare Workers via wrangler deploy --dry-run",
    "rust-test.yml":
        "owns the workspace suite and hub-desktop-bin, the only job that builds "
        "the app as it ships (D-146)",
    "ts-test.yml":
        "owns the frontend production build and the Worker bundle gate (D-162)",
    "state-authority.yml": "owns the declared-state-authority invariant",
    "claims.yml": "owns the public website claim boundary",
}

# Workflows that must NEVER fire on an integration branch. These deploy,
# publish, promote, or open issues about production.
#
# D-195: the value is now (why it must not fire, the reason it cannot). The
# second half is the assertion. "no-push-trigger" means the workflow has no
# `push:` at all; "push-tags-only" means it has a `push:` that filters on tags
# and declares no branch filter, so branch pushes do not reach it. Anything
# else -- including a bare `push:` -- is a failure, because a bare `push:` fires
# on every branch in the repository and produces exactly the same empty
# `push.branches` list that the old version of this test read as safe.
MUST_NOT_GATE_INTEGRATION = {
    "osl-hub-release.yml": (
        "builds and publishes a signed release candidate", "push-tags-only"),
    "osl-hub-promote.yml": (
        "promotes a tested candidate to a public release", "no-push-trigger"),
    "reproducible-build.yml": (
        "reproduces a PUBLISHED hub-v* release", "push-tags-only"),
    "live-drift.yml": (
        "compares production against origin/main and opens issues", "no-push-trigger"),
    "selector-ci.yml": (
        "probes live third-party surfaces and opens issues", "no-push-trigger"),
}

# The jobs integration-gate.yml exists for. Deleting one is exactly the kind of
# quiet removal that produced D-146, so name them here rather than trusting a
# reviewer to notice a missing job in a diff.
REQUIRED_INTEGRATION_GATE_JOBS = {
    "workspace-build": "gate (a) the cargo workspace",
    "hub-desktop-build": "gate (b) apps/osl-hub --features desktop",
    "frontend-build": "gate (c) tsc --noEmit && vite build",
    "worker-bundles": "gate (d) both Workers via wrangler deploy --dry-run",
    "quarantine-ratchet": "the pre-existing-red ratchet",
    "integration-gate": "the aggregate that fails if any of the above is not success",
}

# D-172 split the workers lane in two so neither could mask the other. Every
# step in this matrix is `if: matrix.lane == '...'`, and a job whose steps all
# skip concludes SUCCESS -- so deleting a lane name silently greens that whole
# surface while `success'` stays green. Pin the list.
TS_TEST_LANES = {
    "webview", "osl-hub-ui", "keyserver-worker", "cipher-store-worker", "legacy-keyserver",
}


def load(name: str) -> dict:
    with (WORKFLOWS / name).open(encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def triggers(workflow: dict) -> dict:
    # PyYAML parses the bare key `on:` as the boolean True.
    return workflow.get("on", workflow.get(True)) or {}


def push_branches(workflow: dict) -> list:
    push = triggers(workflow).get("push") or {}
    return list(push.get("branches") or [])


def actions_pattern_to_regex(pattern: str) -> "re.Pattern":
    """GitHub Actions filter-pattern semantics, not pathlib's.

    `*` matches any character except `/`; `**` matches any character including
    `/`; `?` matches one character. Everything else is literal. Getting this
    wrong in the safe direction is what made the old glob test a tautology.
    """
    out = []
    i = 0
    while i < len(pattern):
        char = pattern[i]
        if char == "*":
            if pattern.startswith("**", i):
                out.append(".*")
                i += 2
            else:
                out.append("[^/]*")
                i += 1
        elif char == "?":
            out.append(".")
            i += 1
        else:
            out.append(re.escape(char))
            i += 1
    return re.compile(f"^{''.join(out)}$")


def branch_list_fires_on(patterns: list, ref: str) -> bool:
    """Does a `branches:` list select `ref`? `!pattern` excludes."""
    fires = False
    for pattern in patterns:
        if pattern.startswith("!"):
            if actions_pattern_to_regex(pattern[1:]).match(ref):
                fires = False
        elif actions_pattern_to_regex(pattern).match(ref):
            fires = True
    return fires


def why_push_cannot_reach_a_branch(workflow: dict) -> str:
    """Classify a workflow's `push:` trigger. D-195.

    Returns one of the recorded reasons, or a string starting with "FIRES"
    describing how a branch push does reach it.
    """
    trig = triggers(workflow)
    if "push" not in trig:
        return "no-push-trigger"
    push = trig["push"]
    if not push:
        # `push:` with nothing under it. Fires on EVERY branch. This is the case
        # the old test read as safe, because `push.branches` is [] here too.
        return "FIRES: bare `push:` with no filter fires on every branch"
    if not isinstance(push, dict):
        return f"FIRES: unrecognised push trigger {push!r}"
    if "branches" in push or "branches-ignore" in push:
        return "FIRES: declares a branch filter; check it explicitly"
    if push.get("tags") or push.get("tags-ignore"):
        return "push-tags-only"
    return "FIRES: a push trigger with neither a branch nor a tag filter"


class IntegrationBranchGatingTest(unittest.TestCase):
    def test_gates_cover_the_integration_branch(self) -> None:
        for name, why in MUST_GATE_INTEGRATION.items():
            with self.subTest(workflow=name):
                self.assertIn(
                    INTEGRATION_GLOB,
                    push_branches(load(name)),
                    f"{name} no longer runs on {INTEGRATION_GLOB}, and it {why}. "
                    "D-172: a gate that does not run on the branch we ship from "
                    "is decoration.",
                )

    def test_the_glob_actually_matches_the_branch_we_ship_from(self) -> None:
        # D-195. This used to compare two module constants through pathlib's
        # glob, which is neither the file under test nor Actions' semantics. It
        # now evaluates Actions' rules against the branch lists really written in
        # the gating workflows, and carries the negative controls that prove the
        # matcher can say no.
        self.assertTrue(actions_pattern_to_regex(INTEGRATION_GLOB).match(SHIPPING_BRANCH))
        self.assertFalse(
            actions_pattern_to_regex("integrate/*").match("integrate/a/b"),
            "single-star must not cross a slash, or this matcher would call a "
            "narrowed glob safe",
        )
        self.assertFalse(actions_pattern_to_regex(INTEGRATION_GLOB).match("main"))
        self.assertFalse(actions_pattern_to_regex("integration/**").match(SHIPPING_BRANCH))
        self.assertTrue(branch_list_fires_on(["main", INTEGRATION_GLOB], SHIPPING_BRANCH))
        self.assertFalse(branch_list_fires_on(["main"], SHIPPING_BRANCH))
        self.assertFalse(
            branch_list_fires_on([INTEGRATION_GLOB, "!integrate/first-usable"], SHIPPING_BRANCH),
            "an exclusion pattern must be honoured; otherwise one `!` line could "
            "un-gate the shipping branch while the glob still reads correct",
        )
        for name, why in MUST_GATE_INTEGRATION.items():
            with self.subTest(workflow=name):
                self.assertTrue(
                    branch_list_fires_on(push_branches(load(name)), SHIPPING_BRANCH),
                    f"{name}'s push filter does not actually select {SHIPPING_BRANCH}, "
                    f"and it {why}",
                )

    def test_publishing_workflows_never_fire_on_an_integration_branch(self) -> None:
        # D-195. The old version iterated `push.branches`, which is empty for all
        # five of these, so it made no assertion at all -- and a bare `push:`,
        # which fires on every branch, yields the same empty list. Assert the
        # REASON each one cannot be reached, and require it to be the recorded
        # one.
        for name, (why, expected_reason) in MUST_NOT_GATE_INTEGRATION.items():
            with self.subTest(workflow=name):
                workflow = load(name)
                reason = why_push_cannot_reach_a_branch(workflow)
                self.assertEqual(
                    expected_reason,
                    reason,
                    f"{name} {why}, and the reason a push to {SHIPPING_BRANCH} cannot "
                    f"reach it has changed from '{expected_reason}' to '{reason}'. "
                    "Work-in-progress branches must not reach it.",
                )
                # And belt-and-braces on the declared list, for the case where a
                # branch filter is added later: it must not select the branch.
                self.assertFalse(
                    branch_list_fires_on(push_branches(workflow), SHIPPING_BRANCH),
                    f"{name} would fire on {SHIPPING_BRANCH}, and it {why}",
                )

    def test_the_publishing_guard_can_fail(self) -> None:
        # A gate that cannot fail is decoration (this file's own lesson, learned
        # the hard way -- see the D-195 note at the top). Prove the classifier
        # rejects the exact mutant that passed the old test green.
        bare_push = yaml.safe_load("on:\n  push:\njobs: {}\n")
        self.assertTrue(
            why_push_cannot_reach_a_branch(bare_push).startswith("FIRES"),
            "a bare `push:` must be reported as firing on every branch",
        )
        self.assertEqual([], push_branches(bare_push),
                         "…and it yields the same empty branch list a tags-only "
                         "workflow does, which is why reading that list proved nothing")
        wired_to_integration = yaml.safe_load(
            "on:\n  push:\n    branches: ['integrate/**']\njobs: {}\n")
        self.assertTrue(branch_list_fires_on(push_branches(wired_to_integration), SHIPPING_BRANCH))
        tags_only = yaml.safe_load("on:\n  push:\n    tags: ['hub-v*']\njobs: {}\n")
        self.assertEqual("push-tags-only", why_push_cannot_reach_a_branch(tags_only))
        self.assertEqual("no-push-trigger", why_push_cannot_reach_a_branch(
            yaml.safe_load("on:\n  schedule:\n    - cron: '0 0 * * *'\njobs: {}\n")))

    def test_integration_gate_still_has_every_gate_it_was_built_for(self) -> None:
        jobs = load("integration-gate.yml").get("jobs") or {}
        for job, why in REQUIRED_INTEGRATION_GATE_JOBS.items():
            with self.subTest(job=job):
                self.assertIn(job, jobs, f"integration-gate.yml lost {job}: {why}")

    def test_the_aggregate_requires_every_other_job(self) -> None:
        jobs = load("integration-gate.yml").get("jobs") or {}
        aggregate = jobs.get("integration-gate") or {}
        needs = set(aggregate.get("needs") or [])
        # Compare against the jobs that actually exist, not against a constant:
        # a NEW gate job omitted from `needs` used to pass this test, and a gate
        # nothing depends on cannot fail the workflow.
        self.assertEqual(
            set(jobs) - {"integration-gate"},
            needs,
            "the integration-gate aggregate must depend on every other job in the "
            "workflow; a gate nothing depends on cannot fail it",
        )
        self.assertEqual(set(REQUIRED_INTEGRATION_GATE_JOBS) - {"integration-gate"}, needs)
        # Without `if: always()` a failed dependency SKIPS the aggregate, and a
        # skipped required check is not a red one.
        self.assertEqual("${{ always() }}", aggregate.get("if"))

    def test_each_shipping_gate_is_its_own_job(self) -> None:
        # The D-162 build gate was placed behind `npm run typecheck`, which exits
        # non-zero on pre-existing errors, so Actions aborted the job before the
        # gate ever ran. The structural defence is one job per gate: assert the
        # four build gates do not share a job with each other.
        jobs = load("integration-gate.yml").get("jobs") or {}
        build_gates = ["workspace-build", "hub-desktop-build", "frontend-build", "worker-bundles"]
        self.assertEqual(len(set(build_gates)), len(build_gates))
        for job in build_gates:
            self.assertIn(job, jobs)

    def test_gate_b_keeps_both_the_shipping_check_and_the_wider_one(self) -> None:
        # The shipping check is the hard gate. The `--all-targets` check is
        # strictly wider and found a defect nothing else in the repo could see;
        # it is ratcheted rather than dropped. Losing either is a real loss, and
        # dropping the wider one is the easy mistake, so name both.
        jobs = load("integration-gate.yml").get("jobs") or {}
        steps = (jobs.get("hub-desktop-build") or {}).get("steps") or []
        runs = " ".join(str(step.get("run", "")) for step in steps)
        self.assertIn(
            "cargo check --manifest-path apps/osl-hub/Cargo.toml --features desktop --locked",
            runs,
            "gate (b) must still run the shipping build",
        )
        self.assertIn(
            "node scripts/ci/hub-target-ratchet.mjs --self-test",
            runs,
            "the wider --all-targets check must still run, ratcheted, and must "
            "prove it can fail before it grades anything",
        )
        self.assertNotIn(
            "continue-on-error",
            str(jobs.get("hub-desktop-build")),
            "gate (b) must never be allowed to fail softly",
        )

    def test_the_windows_jobs_do_not_run_under_pwsh(self) -> None:
        # D-195. GitHub's default shell on windows-latest is pwsh, whose wrapper
        # surfaces only the LAST command's $LASTEXITCODE: a native non-zero exit
        # part-way through a multi-command `run:` does not abort the step. The
        # hub-target-ratchet self-test was exactly that shape, so its exit code
        # was discarded and "prove it can fail before it grades anything" was
        # unenforced on the one job where it mattered. osl-hub-release.yml
        # already sets `shell: bash` for this reason; integration-gate.yml did
        # not.
        workflow = load("integration-gate.yml")
        default_shell = ((workflow.get("defaults") or {}).get("run") or {}).get("shell")
        jobs = workflow.get("jobs") or {}
        for name, job in jobs.items():
            if not str(job.get("runs-on", "")).startswith("windows"):
                continue
            job_shell = ((job.get("defaults") or {}).get("run") or {}).get("shell")
            for step in job.get("steps") or []:
                if "run" not in step:
                    continue
                with self.subTest(job=name, step=step.get("name")):
                    self.assertIn(
                        "bash",
                        str(step.get("shell") or job_shell or default_shell),
                        f"{name} runs on Windows, so without `shell: bash` this step "
                        "runs under pwsh and only its LAST command's exit code is "
                        "surfaced",
                    )

    def test_the_hub_ratchet_self_test_is_its_own_step(self) -> None:
        # D-195, the structural half: a proof-of-fallibility whose exit code
        # shares a step with the thing it validates can be discarded by a shell
        # wrapper. As its own step it is the step's result.
        jobs = load("integration-gate.yml").get("jobs") or {}
        steps = (jobs.get("hub-desktop-build") or {}).get("steps") or []
        selftest = [s for s in steps if "--self-test" in str(s.get("run", ""))]
        self.assertEqual(1, len(selftest))
        self.assertEqual(
            1,
            len(str(selftest[0]["run"]).strip().splitlines()),
            "the ratchet self-test must be the only command in its step",
        )

    def test_no_gate_job_fails_softly(self) -> None:
        # `continue-on-error` anywhere in a gating workflow turns a gate into a
        # notification. There is no legitimate use for it in any of the five, so
        # check all five rather than only the one the previous version checked --
        # adding it to rust-test.yml or ts-test.yml used to pass this suite.
        #
        # Structural, not a substring search: the previous version grepped the
        # raw text, which cannot tell a live key from the word appearing inside a
        # comment that explains why it is banned.
        def keys_anywhere(node):
            if isinstance(node, dict):
                for key, value in node.items():
                    yield str(key)
                    yield from keys_anywhere(value)
            elif isinstance(node, list):
                for item in node:
                    yield from keys_anywhere(item)

        for name in ("integration-gate.yml", "rust-test.yml", "ts-test.yml",
                     "state-authority.yml", "claims.yml"):
            with self.subTest(workflow=name):
                self.assertNotIn(
                    "continue-on-error",
                    set(keys_anywhere(load(name))),
                    f"{name} gates integrate/**; a gate allowed to fail softly is a "
                    "notification",
                )

    def test_no_gating_trigger_is_path_filtered(self) -> None:
        # A `paths-ignore: ['**']` leaves `integrate/**` literally present in
        # push.branches -- every branch assertion above still passes -- and stops
        # every gate from running. None exist today; this is what notices one
        # arriving.
        for name in MUST_GATE_INTEGRATION:
            trig = triggers(load(name))
            for event in ("push", "pull_request"):
                spec = trig.get(event)
                if not isinstance(spec, dict):
                    continue
                with self.subTest(workflow=name, event=event):
                    self.assertNotIn("paths", spec)
                    self.assertNotIn("paths-ignore", spec)

    def test_no_cheap_check_stands_in_front_of_an_expensive_one(self) -> None:
        # D-172 found seven instances of one pattern: a gate placed downstream of
        # a step that is already red never executes, because Actions aborts a job
        # at its first failing step. The two structural rules that came out of it
        # are asserted here so they cannot be undone by a merge.
        rust = load("rust-test.yml").get("jobs") or {}
        self.assertIn("clippy", rust, "cargo clippy must be its own job; as step 5 of `test` it "
                                      "skipped cargo test --workspace, nextest and both "
                                      "apps/osl-hub steps on run 30931561981, immediately after "
                                      "fmt was moved out from in front of it")
        self.assertIn("fmt", rust, "cargo fmt must be its own job; as step 5 of `test` it "
                                   "skipped clippy, cargo test --workspace, nextest and both "
                                   "apps/osl-hub steps on run 30930446508")
        test_runs = " ".join(str(step.get("run", "")) for step in (rust.get("test") or {}).get("steps") or [])
        self.assertNotIn("cargo fmt", test_runs,
                         "cargo fmt is back in front of the Rust suite")
        self.assertNotIn("cargo clippy", test_runs,
                         "cargo clippy is back in front of the Rust suite")

        ts = load("ts-test.yml").get("jobs") or {}
        # The renderer's production build must precede its test suite: the suite
        # is red, and on run 30931564095 it reported `Build OSL Privacy UI` as
        # skipped -- the check that says the shipping renderer can be built at
        # all, standing behind the one that says it is correct.
        all_steps = (ts.get("ts-test") or {}).get("steps") or []
        ui_steps = [st for st in all_steps if "osl-hub-ui" in str(st.get("if", ""))]
        names = [st.get("name", "") for st in ui_steps]
        self.assertLess(
            names.index("Build OSL Privacy UI"),
            names.index("Test OSL Privacy UI"),
            "the osl-hub-ui production build must run before the vitest suite",
        )
        self.assertIs(
            False,
            ((ts.get("ts-test") or {}).get("strategy") or {}).get("fail-fast"),
            "the ts-test matrix must set fail-fast: false; on run 30930446663 one "
            "failing lane CANCELLED two others, and a cancelled lane is a result "
            "nobody has",
        )
        claim_steps = " ".join(str(step.get("run", "")) for step in (ts.get("ts-test") or {}).get("steps") or [])
        self.assertNotIn("check-app-claims", claim_steps,
                         "the claim gate is back in front of every ts-test lane")
        self.assertIn("claim-gates", ts, "the claim gate must still exist as its own job")

    def test_a_red_step_cannot_skip_the_steps_beneath_it(self) -> None:
        # D-194. Reordering is not a fix: there is no order in which a red step
        # is not in front of something. Hoisting fmt and clippy out of
        # rust-test.yml's `test` job promoted the known-red `cargo test
        # --workspace` into first position, and on run 30932294581 nextest,
        # `Test OSL Privacy core` and `Test OSL Privacy Windows identity
        # lifecycle` were reported `skipped` -- and the last two run nowhere else
        # except tag-triggered release CI. `!cancelled()` is what stops that.
        guarded = "!cancelled()"

        rust = load("rust-test.yml").get("jobs") or {}
        steps = (rust.get("test") or {}).get("steps") or []
        after_first_command = False
        for step in steps:
            if not after_first_command:
                if "run" in step and "cargo" in str(step["run"]):
                    after_first_command = True
                continue
            with self.subTest(job="test", step=step.get("name") or step.get("uses")):
                self.assertIn(
                    guarded, str(step.get("if", "")),
                    "every step below `cargo test --workspace` must survive it "
                    "being red, or D-194 comes straight back",
                )
        self.assertTrue(after_first_command, "rust-test.yml `test` no longer runs cargo at all")

        for job_name, names in (
            ("quality-checks", ("Rust workflow contract", "Integration-branch gating contract")),
        ):
            job_steps = (rust.get(job_name) or {}).get("steps") or []
            for step in job_steps:
                if step.get("name") in names:
                    with self.subTest(job=job_name, step=step.get("name")):
                        self.assertIn(guarded, str(step.get("if", "")))

        ts = load("ts-test.yml").get("jobs") or {}
        lane_steps = (ts.get("ts-test") or {}).get("steps") or []
        for step in lane_steps:
            condition = str(step.get("if", ""))
            name = str(step.get("name", ""))
            if "matrix.lane" not in condition or name.startswith("Install"):
                continue
            with self.subTest(step=name):
                self.assertIn(
                    guarded, condition,
                    f"'{name}' sits behind another step in its lane; while the lane's "
                    "typecheck is red it is reported `skipped` on every push",
                )
        plan = [s for s in (ts.get("claim-gates") or {}).get("steps") or []
                if s.get("name") == "Plan-governance-acceptance-gates"]
        self.assertEqual(1, len(plan))
        self.assertIn(guarded, str(plan[0].get("if", "")),
                      "the claim gate is red; without this it skips the plan-governance "
                      "gate beside it on every push")

    def test_the_ts_test_lane_list_is_pinned(self) -> None:
        # Every step in that matrix is `if: matrix.lane == '...'`, and a job whose
        # steps all skip concludes SUCCESS. Deleting `cipher-store-worker` from
        # the list silently greens that whole surface and `success'` stays green.
        ts = load("ts-test.yml").get("jobs") or {}
        lanes = (((ts.get("ts-test") or {}).get("strategy") or {}).get("matrix") or {}).get("lane") or []
        self.assertEqual(TS_TEST_LANES, set(lanes))

    def test_the_quarantine_ratchet_is_wired_in(self) -> None:
        jobs = load("integration-gate.yml").get("jobs") or {}
        steps = (jobs.get("quarantine-ratchet") or {}).get("steps") or []
        runs = " ".join(str(step.get("run", "")) for step in steps)
        self.assertIn(
            "scripts/ci/quarantine-ratchet.mjs --self-test",
            runs,
            "the ratchet must prove it fails in both directions before it grades anything",
        )
        self.assertIn("node scripts/ci/quarantine-ratchet.mjs", runs)

    def test_the_quarantine_pins_identities_and_nothing_is_run_by_nothing(self) -> None:
        # D-192: the ratchet claimed parity with scripts/ledger/state-baseline.json,
        # which pins a SET, while pinning a COUNT. D-193: three checks were
        # excluded from the ratchet AND executed by nothing, which is an allowlist
        # by another name. Both are structural properties of the config, so assert
        # them from outside the script that reads it.
        import json
        config = json.loads((REPO / "scripts" / "ci" / "quarantine.json").read_text(encoding="utf-8"))
        self.assertTrue(config.get("entries"))
        for entry in config["entries"]:
            with self.subTest(entry=entry.get("id")):
                self.assertNotIn(
                    "count", entry,
                    "D-192: a quarantine entry pins the SET of finding identities. "
                    "A count lets a real defect substitute for a fixed one.",
                )
                self.assertTrue(
                    isinstance(entry.get("ids"), list) or isinstance(entry.get("idsByEnvironment"), dict),
                    "every entry needs an id set",
                )
                if entry.get("extract") == "vitest":
                    self.assertEqual(
                        {"tests", "files", "skipped"}, set((entry.get("metrics") or {})),
                        "D-192 part 2: a vitest bucket that can silently shrink is not "
                        "quarantined, it is unobserved",
                    )
        for entry in config.get("notRatcheted") or []:
            with self.subTest(entry=entry.get("id")):
                self.assertTrue(
                    entry.get("executedBy"),
                    "D-193: excluded from the ratchet AND executed by nothing is an "
                    "allowlist by another name",
                )

    def test_the_skip_ci_hole_is_recorded_not_forgotten(self) -> None:
        # `[skip ci]` in a commit subject suppresses `push` and `pull_request`
        # outright, and every gating trigger is one of those two. There is no
        # repository setting that disables it, so the requirement is that the
        # decision is written down and that the one trigger a commit message
        # cannot suppress exists on every gating workflow.
        gate = (WORKFLOWS / "integration-gate.yml").read_text(encoding="utf-8")
        self.assertIn("[skip ci]", gate,
                      "the [skip ci] ruling must stay in the file it applies to")
        self.assertIn("RESIDUAL RISK", gate)
        for name in MUST_GATE_INTEGRATION:
            with self.subTest(workflow=name):
                self.assertIn("workflow_dispatch", triggers(load(name)),
                              f"{name} needs a trigger a commit message cannot suppress")


if __name__ == "__main__":
    unittest.main()
