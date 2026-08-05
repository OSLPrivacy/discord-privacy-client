#!/usr/bin/env python3
"""Behavioral contract for the Rust workflow.

Three contracts live here: the cache write policy, D-152's rule that the
shipping binary's test target has an owner in CI, and D-160's rule that some job
runs all eight Binding Ledgers. The last two are the answer to "what happens if
someone deletes that job" -- this file is run by the `quality-checks` job, which
is a different job, so the deletion fails the build instead of passing quietly.
A gate whose only guard is itself is not a gate.
"""

import json
from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).with_name("rust-test.yml")
REPO_ROOT = WORKFLOW.parent.parent.parent
LEDGER_RUNNER = "scripts/ledger/all.mjs"
STATE_BASELINE = REPO_ROOT / "scripts" / "ledger" / "state-baseline.json"
RUST_CACHE = "Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae"
# The one command that compiles and runs `apps/osl-hub/src/main.rs` and the five
# other modules reachable only from it. `--features core --lib` compiles none of
# them, because the bin is `required-features = ["desktop"]`.
HUB_LINT_RATCHET = "scripts/ci/hub-lint-ratchet.mjs"
HUB_LINT_BASELINES = ("hub-clippy-baseline.json", "hub-fmt-baseline.json")
BIN_TEST_FRAGMENTS = (
    "cargo test",
    "--manifest-path apps/osl-hub/Cargo.toml",
    "--features desktop",
    "--bin osl-privacy-hub",
)


def steps_running_the_desktop_binary_tests() -> list[dict]:
    workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    return [
        step
        for job in workflow["jobs"].values()
        for step in job.get("steps", [])
        if all(fragment in str(step.get("run", "")) for fragment in BIN_TEST_FRAGMENTS)
    ]


def steps_running_the_binding_ledgers() -> list[dict]:
    workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    return [
        step
        for job in workflow["jobs"].values()
        for step in job.get("steps", [])
        if LEDGER_RUNNER in str(step.get("run", ""))
    ]


def jobs_running_the_binding_ledgers() -> list[dict]:
    workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    return [
        job
        for job in workflow["jobs"].values()
        if any(LEDGER_RUNNER in str(step.get("run", "")) for step in job.get("steps", []))
    ]


def cache_save_enabled(ref: str) -> bool:
    workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    steps = workflow["jobs"]["test"]["steps"]
    cache_step = next(step for step in steps if step.get("uses") == RUST_CACHE)
    save_if = cache_step["with"]["save-if"]

    if save_if != "${{ github.ref == 'refs/heads/main' }}":
        raise ValueError(f"unexpected rust-cache save condition: {save_if!r}")
    return ref == "refs/heads/main"


class RustCacheSavePolicyTest(unittest.TestCase):
    def test_only_main_can_save_the_rust_cache(self) -> None:
        self.assertTrue(cache_save_enabled("refs/heads/main"))
        self.assertFalse(cache_save_enabled("refs/pull/42/merge"))
        self.assertFalse(cache_save_enabled("refs/heads/feature/cache-tuning"))


class DesktopBinaryTestsHaveAnOwnerTest(unittest.TestCase):
    """D-152 mutant [3]: delete the job and this is what tells you.

    `apps/osl-hub`'s `[[bin]]` is `required-features = ["desktop"]` and the
    package is excluded from the root workspace, so with no job running the
    command below, every line of `main.rs` and of the five modules only it
    declares is compiled by nothing, run by nothing, and not linted by
    `clippy --workspace -D warnings` either. That is exactly the state D-152
    records: 116 tests that did not compile, and a deletion at
    `main.rs`'s lifecycle tick that no gate in the repository could see.
    """

    def test_some_job_runs_the_desktop_binary_test_target(self) -> None:
        steps = steps_running_the_desktop_binary_tests()
        self.assertTrue(
            steps,
            "no CI job runs `cargo test --manifest-path apps/osl-hub/Cargo.toml "
            "--features desktop --bin osl-privacy-hub`. Without it the shipping "
            "binary's own tests are compiled by nothing and run by nothing "
            "(D-152), and `tauri_build::build()` -- the only validator of "
            "capabilities/*.json against the command registry -- never runs "
            "either (D-146).",
        )

    def test_the_binary_tests_run_single_threaded(self) -> None:
        # `set_file_storage_key` is process-global; a multi-threaded run of this
        # target is a flake generator, and a flaky job is a job that gets muted.
        for step in steps_running_the_desktop_binary_tests():
            self.assertIn("--test-threads=1", step["run"])

    def test_the_binary_tests_run_where_the_app_ships(self) -> None:
        # Windows compiles the cfg(windows) halves of the binary as well. On any
        # other runner they are neither built nor run, which is the shape of
        # every defect in this family.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        owners = [
            job
            for job in workflow["jobs"].values()
            if any(
                all(
                    fragment in str(step.get("run", ""))
                    for fragment in BIN_TEST_FRAGMENTS
                )
                for step in job.get("steps", [])
            )
        ]
        self.assertTrue(owners)
        for job in owners:
            self.assertIn("windows", job["runs-on"])

    def test_the_frontend_is_built_before_cargo_in_that_job(self) -> None:
        # `tauri_build::build()` reads `frontendDist`, so a job that skips the
        # renderer build fails in the build script with an error about a missing
        # directory rather than running a single test.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        for job in workflow["jobs"].values():
            steps = job.get("steps", [])
            cargo_at = [
                index
                for index, step in enumerate(steps)
                if all(
                    fragment in str(step.get("run", ""))
                    for fragment in BIN_TEST_FRAGMENTS
                )
            ]
            if not cargo_at:
                continue
            build_at = [
                index
                for index, step in enumerate(steps)
                if "npm run build" in str(step.get("run", ""))
                and step.get("working-directory") == "apps/osl-hub-ui"
            ]
            self.assertTrue(
                build_at, "the desktop binary job must build the embedded frontend"
            )
            self.assertLess(min(build_at), min(cargo_at))


class BindingLedgersHaveAnOwnerTest(unittest.TestCase):
    """D-160 mutant [4]: delete the job and this is what tells you.

    Before D-160, `grep -rn "scripts/ledger" .github/workflows/` returned a
    single comment. Eight ledgers -- five artifacts of ACL discipline, the
    command registry, the event surface, the router, real rollup bundle
    membership -- and the only thing that ran any of them was a human typing the
    command. Everything they caught was caught that way, which means everything
    they would have caught on a day nobody typed it was not caught at all.

    This test is run by `quality-checks`, a different job from the one it
    guards, for the same reason D-152's is.
    """

    def test_some_job_runs_all_eight_binding_ledgers(self) -> None:
        self.assertTrue(
            steps_running_the_binding_ledgers(),
            "no CI job runs `node scripts/ledger/all.mjs`. The eight Binding "
            "Ledgers are then enforced only by a human remembering to type the "
            "command -- which is the exact failure PLAN.md 9 records as "
            "'verify_all.sh existed and no loop ever ran it' (D-160).",
        )

    def test_the_ledgers_run_with_no_cache(self) -> None:
        # Ledgers 1 and 7 read real rollup module ids. Without --no-cache they
        # read scripts/ledger/.cache instead, which on a fresh CI checkout does
        # not exist -- so the job would fail on a cache complaint rather than
        # report the tree, and the obvious "fix" is to make it stop checking.
        for step in steps_running_the_binding_ledgers():
            self.assertIn("--no-cache", step["run"])

    def test_the_ledger_job_never_invokes_cargo(self) -> None:
        # It is a node-only check. A cargo step here would put it on the
        # contended build path for no coverage it does not already have.
        for job in jobs_running_the_binding_ledgers():
            for step in job.get("steps", []):
                self.assertNotIn("cargo", str(step.get("run", "")))

    def test_the_ledger_job_installs_the_renderer_dependencies(self) -> None:
        # Ledger 7 asks rollup for `this.getModuleIds()` by driving a real vite
        # build, and vite is a devDependency of apps/osl-hub-ui, not of the repo
        # root. Without `npm ci` there the job dies before it reports anything.
        for job in jobs_running_the_binding_ledgers():
            installs = [
                step
                for step in job.get("steps", [])
                if "npm ci" in str(step.get("run", ""))
                and step.get("working-directory") == "apps/osl-hub-ui"
            ]
            self.assertTrue(
                installs,
                "the Binding Ledger job must install apps/osl-hub-ui's "
                "dependencies; ledger 7 drives rollup",
            )


class LedgerEightBaselineIsARatchetTest(unittest.TestCase):
    """D-160: the baseline is a visible number that may only go down.

    Ledger 8 is RED with 20 and that is correct -- D-092, D-108 and D-114 are
    open defects. Running all eight and failing on red would make CI
    permanently red, and a permanently-red CI is one everybody learns to ignore:
    the same defeat in a different costume. So the count is ratcheted.

    The two things that would quietly undo it are (a) deleting the baseline file
    so the comparison has nothing to compare against, and (b) letting the number
    and the list of ids disagree, at which point the number is no longer
    checkable and drifts. Both fail here.
    """

    def test_the_baseline_file_exists(self) -> None:
        self.assertTrue(
            STATE_BASELINE.exists(),
            f"{STATE_BASELINE} is missing. Ledger 8's ratchet has nothing to "
            "ratchet against, and the choice becomes 'permanently red' or "
            "'not run at all' again.",
        )

    def test_the_count_matches_the_ids(self) -> None:
        doc = json.loads(STATE_BASELINE.read_text(encoding="utf-8"))
        self.assertIsInstance(doc.get("openViolations"), int)
        self.assertEqual(
            doc["openViolations"],
            len(doc["ids"]),
            'state-baseline.json: "openViolations" must equal len("ids"), the '
            "same discipline as highWaterMark in exceptions/*.json. The number "
            "and the list are the same fact stated twice so that neither can "
            "drift unnoticed.",
        )

    def test_the_baseline_is_not_an_exception_file(self) -> None:
        # The exception mechanism means "this does not ship". Ledger 8's rows
        # ship: they are defects with owners. If someone ever migrates them into
        # exceptions/state.json the finding is deleted rather than tracked, so
        # that file must stay empty of them.
        exceptions = REPO_ROOT / "scripts" / "ledger" / "exceptions" / "state.json"
        entries = json.loads(exceptions.read_text(encoding="utf-8"))["entries"]
        baseline_ids = set(json.loads(STATE_BASELINE.read_text(encoding="utf-8"))["ids"])
        excepted = {entry["id"] for entry in entries}
        self.assertFalse(
            excepted & baseline_ids,
            "a ledger-8 baseline id was moved into exceptions/state.json. An "
            "exception is an admission that something does not ship; these ship "
            "and are owned by D-092/D-108/D-114. Track the number, do not "
            "except the row.",
        )


class HubLintCensusHasAnOwnerTest(unittest.TestCase):
    """D-268/D-252: delete the step and this is what tells you.

    `apps/osl-hub` is `exclude`d from the root workspace, so
    `cargo clippy --workspace --all-targets -- -D warnings` has never linted one
    line of it -- and `-p osl-hub` does not resolve, so nobody notices. Three
    defects came straight out of that blindness: D-269 (two `#[test]` attributes
    on one function and none on the next, so a registration proof never ran; only
    `duplicate_macro_attributes` sees it), D-253 (a stale schema arm that would
    have skipped a migration; only `unreachable_patterns` sees it) and D-268 (the
    `[[bin]]`'s 22,634 lines, in no census on any platform).

    Like D-152's contract above, this runs from `quality-checks` -- a different
    job from the one it guards.
    """

    def hub_lint_steps(self, mode: str) -> list[tuple[dict, dict]]:
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        return [
            (job, step)
            for job in workflow["jobs"].values()
            for step in job.get("steps", [])
            if f"{HUB_LINT_RATCHET} {mode}" in str(step.get("run", ""))
        ]

    def test_some_job_runs_the_hub_clippy_census(self) -> None:
        self.assertTrue(
            self.hub_lint_steps("clippy"),
            f"no CI job runs `node {HUB_LINT_RATCHET} clippy`. The hub is then "
            "linted by nothing, on any platform, which is the state D-252 and "
            "D-268 record.",
        )

    def test_the_census_runs_where_the_app_ships(self) -> None:
        # 71 of the 302 known src/ sites are Windows-only, and 54 of the 55
        # Linux-only ones are `dead_code` on items that are LIVE on Windows. An
        # ubuntu hub lint job would grade the wrong tree and would demand 54
        # FALSE `#[allow(dead_code)]` on shipping code.
        for job, _step in self.hub_lint_steps("clippy"):
            self.assertIn("windows", job["runs-on"])

    def test_the_frontend_is_built_before_the_census(self) -> None:
        # `--features desktop` runs `tauri_build::build()`, which reads
        # frontendDist. Without the renderer the step dies in the build script
        # and reports a missing directory instead of a census.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        for job in workflow["jobs"].values():
            steps = job.get("steps", [])
            census_at = [
                i for i, s in enumerate(steps)
                if f"{HUB_LINT_RATCHET} clippy" in str(s.get("run", ""))
            ]
            if not census_at:
                continue
            build_at = [
                i for i, s in enumerate(steps)
                if "npm run build" in str(s.get("run", ""))
                and s.get("working-directory") == "apps/osl-hub-ui"
            ]
            self.assertTrue(build_at, "the hub clippy census job must build the embedded frontend")
            self.assertLess(min(build_at), min(census_at))

    def test_the_census_cannot_mask_the_desktop_binary_test(self) -> None:
        # D-194, the whole of it. A lint has stood in front of the entire Rust
        # suite twice in this repository. The census is the LAST step of its job
        # and every step of that job from the census onward carries
        # `!cancelled()`, so a red ratchet cannot skip the binary test and a red
        # binary test cannot skip the ratchet.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        for job in workflow["jobs"].values():
            steps = job.get("steps", [])
            census_at = [
                i for i, s in enumerate(steps)
                if f"{HUB_LINT_RATCHET} clippy" in str(s.get("run", ""))
            ]
            if not census_at:
                continue
            bin_at = [
                i for i, s in enumerate(steps)
                if all(f in str(s.get("run", "")) for f in BIN_TEST_FRAGMENTS)
            ]
            if bin_at:
                self.assertGreater(min(census_at), max(bin_at))
            for step in steps[min(census_at):]:
                self.assertEqual(step.get("if"), "${{ !cancelled() }}", step.get("name"))

    def test_the_grader_is_proved_able_to_fail_before_it_grades(self) -> None:
        # D-195. A ratchet nobody has watched fail is decoration, and its proof
        # is a step of its own so that its exit code is a step result rather than
        # a line inside a multi-command `run:`.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        for job in workflow["jobs"].values():
            steps = job.get("steps", [])
            graded_at = [
                i for i, s in enumerate(steps)
                if HUB_LINT_RATCHET in str(s.get("run", ""))
                and "--self-test" not in str(s.get("run", ""))
            ]
            if not graded_at:
                continue
            self_test_at = [
                i for i, s in enumerate(steps)
                if f"{HUB_LINT_RATCHET} --self-test" in str(s.get("run", ""))
            ]
            self.assertTrue(self_test_at, "the hub lint ratchet must prove it can fail first")
            self.assertLess(min(self_test_at), min(graded_at))

    def test_the_census_job_installs_clippy(self) -> None:
        # rustup's minimal profile does not carry clippy, and dtolnay's action
        # installs only what is asked for.
        for job, _step in self.hub_lint_steps("clippy"):
            components = [
                str(step.get("with", {}).get("components", ""))
                for step in job.get("steps", [])
                if str(step.get("uses", "")).startswith("dtolnay/rust-toolchain")
            ]
            self.assertTrue(any("clippy" in c for c in components))

    def test_the_census_command_is_the_one_that_sees_the_binary(self) -> None:
        # These four are not style. Without `--features desktop` the 22,634-line
        # `[[bin]]` is not built and `web_surface_a11y_spike.rs` does not compile;
        # without `--all-targets` the tests are not linted; without `--keep-going`
        # a single failing target makes the census depend on which of the others
        # happened to compile first; and `-p osl-hub` does not resolve, so the
        # manifest path is the only way in.
        source = (REPO_ROOT / HUB_LINT_RATCHET).read_text(encoding="utf-8")
        for fragment in (
            "'--manifest-path', 'apps/osl-hub/Cargo.toml'",
            "'--features', 'desktop'",
            "'--all-targets'",
            "'--keep-going'",
        ):
            self.assertIn(fragment, source)

    def test_the_census_silences_no_lint(self) -> None:
        # The baseline is the mechanism for pre-existing debt. An `-A` or
        # `--allow` on the command line would lower the number without lowering
        # the debt, and would do it invisibly.
        source = (REPO_ROOT / HUB_LINT_RATCHET).read_text(encoding="utf-8")
        args = source.split("export const MODES")[1].split("// ---")[0]
        for forbidden in ("'-A'", "'--allow'", "'--cap-lints'"):
            self.assertNotIn(forbidden, args)


class HubFmtGateHasAnOwnerTest(unittest.TestCase):
    """D-268: the same exclusion blinded the fmt job, and nobody had noticed.

    `cargo fmt --all -- --check` was exit 0 at the repo root and exit 1 with 50
    diff hunks inside `apps/osl-hub`.
    """

    def test_the_root_fmt_check_still_runs(self) -> None:
        # Adding the hub must never be allowed to arrive as a REPLACEMENT for
        # the root check. Both, or neither is a gate.
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        runs = [str(s.get("run", "")) for j in workflow["jobs"].values() for s in j.get("steps", [])]
        self.assertTrue(any("cargo fmt --all -- --check" in r for r in runs))

    def test_some_job_runs_the_hub_fmt_ratchet(self) -> None:
        workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
        runs = [str(s.get("run", "")) for j in workflow["jobs"].values() for s in j.get("steps", [])]
        self.assertTrue(
            any(f"{HUB_LINT_RATCHET} fmt" in r for r in runs),
            f"no CI job runs `node {HUB_LINT_RATCHET} fmt`; `cargo fmt --all` at "
            "the repo root does not reach apps/osl-hub (D-268).",
        )


class HubLintBaselinesAreRatchetsTest(unittest.TestCase):
    """The two ways a baseline quietly stops being one.

    (a) The file goes missing, so the comparison has nothing to compare against
    and the only choices left are 'permanently red' or 'not run at all'.
    (b) The number and the list of sites drift apart, at which point the number
    is no longer checkable. Ledger 8 states this rule as
    `openViolations == len(ids)`; this is the same rule for a counted map.
    """

    def baselines(self) -> list[Path]:
        return [REPO_ROOT / "scripts" / "ci" / name for name in HUB_LINT_BASELINES]

    def test_the_baseline_files_exist(self) -> None:
        for path in self.baselines():
            self.assertTrue(path.exists(), f"{path} is missing; the hub lint ratchet has nothing to ratchet against")

    def test_a_recorded_baseline_number_matches_its_sites(self) -> None:
        for path in self.baselines():
            doc = json.loads(path.read_text(encoding="utf-8"))
            if not doc.get("recorded"):
                # An unrecorded baseline is the bootstrap state: the grading step
                # is RED and prints the census to commit. It must not also be
                # allowed to claim a number.
                self.assertIsNone(doc.get("openWarnings"), path)
                continue
            self.assertIsInstance(doc.get("openWarnings"), int, path)
            self.assertIsInstance(doc.get("sites"), dict, path)
            self.assertEqual(doc["openWarnings"], sum(doc["sites"].values()), path)

    def test_no_clippy_config_undercuts_the_census(self) -> None:
        # The cheap way to make this gate green is a clippy.toml or a `[lints]`
        # table, and both lower the number without lowering the debt. If one is
        # ever wanted, it has to arrive by deleting this test and arguing for it.
        for candidate in (
            REPO_ROOT / "clippy.toml",
            REPO_ROOT / ".clippy.toml",
            REPO_ROOT / "apps" / "osl-hub" / "clippy.toml",
            REPO_ROOT / "apps" / "osl-hub" / ".clippy.toml",
        ):
            self.assertFalse(candidate.exists(), f"{candidate} suppresses what the D-268 census exists to count")
        manifest = (REPO_ROOT / "apps" / "osl-hub" / "Cargo.toml").read_text(encoding="utf-8")
        self.assertNotIn("[lints]", manifest)


if __name__ == "__main__":
    unittest.main()
