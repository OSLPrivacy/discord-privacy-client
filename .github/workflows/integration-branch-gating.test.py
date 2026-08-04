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
"""

from pathlib import Path
import unittest

import yaml


WORKFLOWS = Path(__file__).parent
INTEGRATION_GLOB = "integrate/**"

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
MUST_NOT_GATE_INTEGRATION = {
    "osl-hub-release.yml": "builds and publishes a signed release candidate",
    "osl-hub-promote.yml": "promotes a tested candidate to a public release",
    "reproducible-build.yml": "reproduces a PUBLISHED hub-v* release",
    "live-drift.yml": "compares production against origin/main and opens issues",
    "selector-ci.yml": "probes live third-party surfaces and opens issues",
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


def load(name: str) -> dict:
    with (WORKFLOWS / name).open(encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def push_branches(workflow: dict) -> list:
    # PyYAML parses the bare key `on:` as the boolean True.
    triggers = workflow.get("on", workflow.get(True)) or {}
    push = triggers.get("push") or {}
    return list(push.get("branches") or [])


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
        # `integrate/**` must cover `integrate/first-usable`. Narrowing the glob
        # to something that no longer matches would pass the test above while
        # gating nothing, so check the match itself.
        self.assertTrue(
            Path("integrate/first-usable").match(INTEGRATION_GLOB.replace("**", "*")),
            "the integration glob no longer matches integrate/first-usable",
        )

    def test_publishing_workflows_never_fire_on_an_integration_branch(self) -> None:
        for name, why in MUST_NOT_GATE_INTEGRATION.items():
            with self.subTest(workflow=name):
                branches = push_branches(load(name))
                for branch in branches:
                    self.assertFalse(
                        branch.startswith("integrate/"),
                        f"{name} would fire on {branch}, and it {why}. "
                        "Work-in-progress branches must not reach it.",
                    )

    def test_integration_gate_still_has_every_gate_it_was_built_for(self) -> None:
        jobs = load("integration-gate.yml").get("jobs") or {}
        for job, why in REQUIRED_INTEGRATION_GATE_JOBS.items():
            with self.subTest(job=job):
                self.assertIn(job, jobs, f"integration-gate.yml lost {job}: {why}")

    def test_the_aggregate_requires_every_other_job(self) -> None:
        jobs = load("integration-gate.yml").get("jobs") or {}
        aggregate = jobs.get("integration-gate") or {}
        needs = set(aggregate.get("needs") or [])
        expected = set(REQUIRED_INTEGRATION_GATE_JOBS) - {"integration-gate"}
        self.assertEqual(
            expected,
            needs,
            "the integration-gate aggregate must depend on every gate job; a gate "
            "nothing depends on cannot fail the workflow",
        )
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

    def test_no_gate_job_fails_softly(self) -> None:
        # `continue-on-error` anywhere in this workflow would turn a gate into a
        # notification. There is no legitimate use for it here.
        self.assertNotIn("continue-on-error", (WORKFLOWS / "integration-gate.yml").read_text(encoding="utf-8"))

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


if __name__ == "__main__":
    unittest.main()
