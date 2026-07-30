from __future__ import annotations

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


def success_contract() -> None:
    cargo = _read_toml(ROOT / "Cargo.toml")
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
    assert _rust_toolchain(rust_ci, "test") == f"{rust_version}.0"

    protected_commands = _command_words(_step_runs(rust_ci, "test"))
    assert ("cargo", "fmt", "--all", "--", "--check") in protected_commands
    assert (
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ) in protected_commands
    assert ("cargo", "test", "--workspace") in protected_commands

    app_manifest = "apps/osl-hub/Cargo.toml"
    assert "apps/osl-hub" not in members
    assert (
        "cargo",
        "test",
        "--manifest-path",
        app_manifest,
        "--features",
        "core",
        "--lib",
    ) in protected_commands
    assert (
        "cargo",
        "test",
        "--manifest-path",
        app_manifest,
        "--features",
        "core",
        "--test",
        "windows_identity_lifecycle",
    ) in protected_commands


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
