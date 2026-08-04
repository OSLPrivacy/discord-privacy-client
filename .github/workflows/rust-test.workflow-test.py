#!/usr/bin/env python3
"""Behavioral contract for the Rust workflow.

Two contracts live here: the cache write policy, and D-152's rule that the
shipping binary's test target has an owner in CI. The second one is the answer
to "what happens if someone deletes that job" -- this file is run by the
`quality-checks` job, which is a different job on a different runner, so the
deletion fails the build instead of passing quietly. A gate whose only guard is
itself is not a gate.
"""

from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).with_name("rust-test.yml")
RUST_CACHE = "Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae"
# The one command that compiles and runs `apps/osl-hub/src/main.rs` and the five
# other modules reachable only from it. `--features core --lib` compiles none of
# them, because the bin is `required-features = ["desktop"]`.
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


if __name__ == "__main__":
    unittest.main()
