# D-283. THIS FILE IS RUN BY A WORKFLOW. It was not, for its entire life.
#
# Its whole purpose is asserting the CI contract, and until 2026-08-05
# `grep -rn "test_root_cargo_ci_contract" .github/` returned nothing: the guard
# that grades the pipeline was executed by no part of the pipeline. That is
# D-160's shape ("verify_all.sh existed and no loop ever ran it") applied to the
# grader itself, and it is the fourth instance of the pattern found in one day --
# the branch that was never pushed (D-172), the hub excluded from clippy (D-252),
# the browser gates with no browser (D-233), and this. BEING A GATE DOES NOT MAKE
# SOMETHING RUN.
#
# It runs from `quality-checks` in .github/workflows/rust-test.yml, which is the
# job that already runs the other two workflow contracts. Do not move it into a
# job it grades: a contract enforced only by the thing it constrains is not
# enforced.
#
# What running it for the first time immediately found: THREE of its assertions
# were stale and the file was RED (exit 1). D-172 split `cargo fmt` and
# `cargo clippy` out of the `test` job into jobs of their own -- because they were
# masking the entire Rust suite -- and this file still demanded to find both
# INSIDE `test`. And `rust-version` became three-component ("1.97.1"), while the
# toolchain assertion appended a fourth (`f"{rust_version}.0"` -> "1.97.1.0").
# Nothing here was relaxed to make it green: the fmt/clippy assertions now demand
# the commands exist in the workflow AND that they are NOT in `test`, which is
# strictly stronger than what was written and pins D-172's split in place.
from __future__ import annotations

import copy
import re
import unittest
from pathlib import Path
import tomllib


ROOT = Path(__file__).resolve().parents[1]


def _read_toml(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def _yaml_scalar(value: str) -> str:
    raw = value.strip()
    if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in {"'", '"'}:
        return raw[1:-1]
    return raw


def _job_lines(path: Path, job_name: str) -> list[str]:
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
    assert collected, f"{path} job {job_name} was not found"
    return collected


def _step_runs(path: Path, job_name: str) -> list[str]:
    runs: list[str] = []
    for line in _job_lines(path, job_name):
        match = re.match(r"^\s+(?:-\s+)?run:\s+(.+?)\s*$", line)
        if match:
            runs.append(_yaml_scalar(match.group(1)))
    return runs


def _job_names(path: Path) -> list[str]:
    """Every top-level job key, in file order."""
    names: list[str] = []
    in_jobs = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if re.match(r"^jobs:\s*$", line):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        if re.match(r"^\S", line):
            break
        job_match = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if job_match:
            names.append(job_match.group(1))
    assert names, f"{path} declares no jobs"
    return names


def _workflow_runs(path: Path) -> list[str]:
    """Every `run:` scalar in the whole workflow, not just one job."""
    runs: list[str] = []
    for job in _job_names(path):
        runs.extend(_step_runs(path, job))
    return runs


def _toolchain_triple(version: str) -> tuple[int, ...]:
    """`1.97` and `1.97.0` are the same pin; `1.97.1` is not either of them.

    The assertion this feeds used to be `toolchain == f"{rust_version}.0"`, which
    silently encoded the assumption that `rust-version` is two-component. When
    Cargo.toml moved to "1.97.1" that produced the nonsense expectation
    "1.97.1.0" and the whole file went red -- which nobody saw, because nothing
    ran it (D-283). Normalising both sides keeps the comparison EXACT while
    letting either spelling be written down.
    """
    parts = [int(part) for part in version.split(".")]
    assert 2 <= len(parts) <= 3, f"not a rustc version: {version}"
    while len(parts) < 3:
        parts.append(0)
    return tuple(parts)


# D-284. `cargo fmt --all -- --check` at the repo root is NOT evidence that the
# repository is formatted: `--all` resolves to the workspace MEMBERS, so every
# path in `[workspace] exclude` is invisible to it. `apps/osl-hub` -- the largest
# component in the tree, 185,383 lines -- sat at 50 unseen diff hunks while the
# root command was exit 0, and the conductor blamed four of them on whoever last
# touched the file.
#
# So the exclusion list is CENSUSED here rather than trusted. Every excluded
# package must be covered by a fmt command in rust-test.yml, either directly
# (`cargo fmt --manifest-path <pkg>/Cargo.toml`) or through a script that this
# file then reads to confirm the script really names that manifest -- a coverage
# claim that is not checked against the tool making it is the self-satisfying
# gate of D-285.
#
# A package may be deliberately uncovered, but only by being written into
# `FMT_UNCOVERED_EXCLUSIONS` below, WITH ITS MEASURED COST. That set is frozen:
# adding an exclusion without covering it fails this contract. It is a ratchet,
# not an exception file.
FMT_COVERAGE_BY_SCRIPT: dict[str, str] = {
    # `node scripts/ci/hub-lint-ratchet.mjs fmt` runs
    # `cargo fmt --manifest-path apps/osl-hub/Cargo.toml --all -- --check` and
    # grades it against scripts/ci/hub-fmt-baseline.json (D-268).
    "scripts/ci/hub-lint-ratchet.mjs": "apps/osl-hub",
}

FMT_UNCOVERED_EXCLUSIONS: dict[str, str] = {
    # The legacy Tauri shell. It is built by no release job and ships nothing;
    # D-159 is the ruling that a green line about code that does not ship is
    # worse than no line, and the same applies to a red one. MEASURED on
    # 2026-08-05, not assumed: `cargo fmt --manifest-path src-tauri/Cargo.toml
    # --all -- --check` is exit 1 with exactly ONE diff hunk, an `assert!` reflow
    # at src-tauri/src/main.rs:3496 inside a test. Wiring it would make the `fmt`
    # job permanently red for a reflow in dead code. Recorded, not hidden.
    "src-tauri": "legacy shell, ships nothing; 1 hunk at src-tauri/src/main.rs:3496",
}


def _fmt_covered_packages(rust_ci: Path) -> set[str]:
    covered: set[str] = set()
    for command in _workflow_runs(rust_ci):
        words = re.sub(r"\s+", " ", command).strip().split(" ")
        if words[:2] == ["cargo", "fmt"] and "--manifest-path" in words:
            manifest = words[words.index("--manifest-path") + 1]
            assert manifest.endswith("/Cargo.toml"), manifest
            covered.add(manifest[: -len("/Cargo.toml")])
        for script, package in FMT_COVERAGE_BY_SCRIPT.items():
            if script not in words:
                continue
            # The script is only allowed to stand in for the package if it
            # genuinely reaches it. D-285: a coverage claim checked against
            # nothing is satisfied by its own declaration.
            source = ROOT / script
            assert source.is_file(), f"{script} is claimed as fmt coverage and does not exist"
            text = source.read_text(encoding="utf-8")
            assert f"{package}/Cargo.toml" in text, (
                f"{script} is claimed to fmt {package} and never names its manifest"
            )
            assert "fmt" in text, f"{script} is claimed as fmt coverage and never runs fmt"
            covered.add(package)
    return covered


def _assert_no_excluded_package_is_silently_unformatted(cargo: dict, rust_ci: Path) -> None:
    excludes = [
        package
        for package in cargo["workspace"].get("exclude", [])
        if (ROOT / package / "Cargo.toml").is_file()
    ]
    assert excludes, "the exclusion census read nothing; it did not run"
    covered = _fmt_covered_packages(rust_ci)
    uncovered = [
        package
        for package in excludes
        if package not in covered and package not in FMT_UNCOVERED_EXCLUSIONS
    ]
    assert uncovered == [], (
        "workspace-excluded packages that no fmt gate reaches and that are not "
        f"recorded as deliberately uncovered: {uncovered}"
    )
    # The recorded-uncovered set may not name a package that is not excluded --
    # otherwise it decays into a list nobody prunes.
    stale = [package for package in FMT_UNCOVERED_EXCLUSIONS if package not in excludes]
    assert stale == [], f"recorded as uncovered but no longer excluded: {stale}"


def _rust_toolchain(path: Path, job_name: str) -> str:
    lines = _job_lines(path, job_name)
    for index, line in enumerate(lines):
        uses = re.match(r"^\s+-\s+uses:\s+(.+?)\s*$", line)
        if not uses or not _yaml_scalar(uses.group(1)).startswith(
            "dtolnay/rust-toolchain@",
        ):
            continue
        for follow in lines[index + 1:]:
            if re.match(r"^\s+-\s+", follow):
                break
            toolchain = re.match(r"^\s+toolchain:\s+(.+?)\s*$", follow)
            if toolchain:
                return _yaml_scalar(toolchain.group(1))
    raise AssertionError(f"{job_name} does not install a Rust toolchain")


def _command_words(commands: list[str]) -> set[tuple[str, ...]]:
    words: set[tuple[str, ...]] = set()
    for command in commands:
        normalized = re.sub(r"\s+", " ", command).strip()
        words.add(tuple(normalized.split(" ")))
    return words


def _assert_protected_command(
    protected_commands: set[tuple[str, ...]],
    command: tuple[str, ...],
    label: str,
) -> None:
    assert command in protected_commands, (
        f"missing {label}: `{' '.join(command)}`; "
        "scripts/test_root_cargo_ci_contract.py build `success' no longer exits 0"
    )


def _assert_root_cargo_ci_contract(cargo: dict) -> None:
    workspace = cargo["workspace"]
    members = workspace["members"]
    excludes = workspace.get("exclude", [])
    assert isinstance(members, list)
    assert members
    assert isinstance(excludes, list)

    missing_member_manifests = [
        member
        for member in members
        if not (ROOT / member / "Cargo.toml").is_file()
    ]
    assert missing_member_manifests == []
    assert "src-tauri" not in members
    assert "src-tauri" in excludes
    assert (ROOT / "src-tauri" / "Cargo.toml").is_file()

    rust_version = cargo["workspace"]["package"]["rust-version"]
    rust_ci = ROOT / ".github" / "workflows" / "rust-test.yml"
    expected_toolchain = _toolchain_triple(rust_version)
    assert _toolchain_triple(_rust_toolchain(rust_ci, "test")) == expected_toolchain
    # Stronger than the single `test` pin this replaced: EVERY job in the
    # workflow that installs a toolchain must install the same one. A second job
    # grading the tree on a different rustc is a report about a build nobody
    # ships.
    for job in _job_names(rust_ci):
        try:
            declared = _rust_toolchain(rust_ci, job)
        except AssertionError:
            continue
        assert _toolchain_triple(declared) == expected_toolchain, (
            f"{job} pins {declared}, Cargo.toml says {rust_version}"
        )

    protected_commands = _command_words(_step_runs(rust_ci, "test"))
    workflow_commands = _command_words(_workflow_runs(rust_ci))
    fmt_command = ("cargo", "fmt", "--all", "--", "--check")
    clippy_command = (
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    )
    # D-172/D-194. These two used to be demanded INSIDE `test`, and that is
    # exactly where they must not be. Actions aborts a job at its first failing
    # step, so while `cargo fmt` stood at position 5 of `test` it reported
    # `skipped` for clippy, `cargo test --workspace`, nextest and both
    # apps/osl-hub steps -- a formatting nit hiding the entire Rust suite, twice.
    # The contract is therefore two-sided: the commands must still run, and they
    # must not run in front of the suite.
    assert fmt_command in workflow_commands
    assert clippy_command in workflow_commands
    assert fmt_command not in protected_commands
    assert clippy_command not in protected_commands
    assert ("cargo", "test", "--workspace") in protected_commands

    _assert_no_excluded_package_is_silently_unformatted(cargo, rust_ci)

    app_manifest = "apps/osl-hub/Cargo.toml"
    assert "apps/osl-hub" not in members
    _assert_protected_command(
        protected_commands,
        (
            "cargo",
            "test",
            "--manifest-path",
            app_manifest,
            "--features",
            "core",
            "--lib",
        ),
        "OSL Privacy core library build",
    )
    _assert_protected_command(
        protected_commands,
        (
            "cargo",
            "test",
            "--manifest-path",
            app_manifest,
            "--features",
            "core",
            "--test",
            "windows_identity_lifecycle",
        ),
        "OSL Privacy Windows identity lifecycle build",
    )


def success_contract() -> None:
    cargo = _read_toml(ROOT / "Cargo.toml")
    _assert_root_cargo_ci_contract(cargo)

    testcase = unittest.TestCase()

    tauri_in_workspace = copy.deepcopy(cargo)
    tauri_in_workspace["workspace"]["members"].append("src-tauri")
    with testcase.assertRaises(AssertionError):
        _assert_root_cargo_ci_contract(tauri_in_workspace)

    missing_legacy_exclusion = copy.deepcopy(cargo)
    missing_legacy_exclusion["workspace"]["exclude"].remove("src-tauri")
    with testcase.assertRaises(AssertionError):
        _assert_root_cargo_ci_contract(missing_legacy_exclusion)

    # D-283. The three assertions this file grew were each added because the
    # first real CI run of the file found the old ones stale. Each is proved able
    # to fail here, in the same style as the two above -- a contract nobody has
    # seen refuse is the thing D-283 is about.
    drifted_toolchain = copy.deepcopy(cargo)
    drifted_toolchain["workspace"]["package"]["rust-version"] = "1.98.0"
    with testcase.assertRaises(AssertionError):
        _assert_root_cargo_ci_contract(drifted_toolchain)

    # D-284: with the hub's fmt coverage withdrawn, the census must name it.
    rust_ci = ROOT / ".github" / "workflows" / "rust-test.yml"
    saved_coverage = dict(FMT_COVERAGE_BY_SCRIPT)
    saved_uncovered = dict(FMT_UNCOVERED_EXCLUSIONS)
    try:
        FMT_COVERAGE_BY_SCRIPT.clear()
        with testcase.assertRaises(AssertionError):
            _assert_no_excluded_package_is_silently_unformatted(cargo, rust_ci)
        FMT_COVERAGE_BY_SCRIPT.update(saved_coverage)
        # ...and a recorded-uncovered entry for a package nobody excludes any
        # more must not be allowed to sit there unread.
        FMT_UNCOVERED_EXCLUSIONS["crates/does-not-exist"] = "planted by the self-proof"
        with testcase.assertRaises(AssertionError):
            _assert_no_excluded_package_is_silently_unformatted(cargo, rust_ci)
    finally:
        FMT_COVERAGE_BY_SCRIPT.clear()
        FMT_COVERAGE_BY_SCRIPT.update(saved_coverage)
        FMT_UNCOVERED_EXCLUSIONS.clear()
        FMT_UNCOVERED_EXCLUSIONS.update(saved_uncovered)


success_contract.__name__ = "success'"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(success_contract))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
