from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "qa" / "osl-launch-instance-b.ps1"


def _powershell() -> str:
    executable = shutil.which("pwsh") or shutil.which("powershell")
    if executable is None:
        raise unittest.SkipTest("osl-launch-instance-b contract tests require PowerShell")
    return executable


def _run_fixture(
    tmp: Path,
    fixture: dict[str, object],
    *,
    confirm: bool,
) -> tuple[subprocess.CompletedProcess[str], dict[str, object]]:
    fixture_path = tmp / "fixture.json"
    out_path = tmp / "result.json"
    fixture_path.write_text(json.dumps(fixture), encoding="utf-8")
    command = [
        _powershell(),
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        str(SCRIPT),
        "-ExeB",
        str(tmp / "osl-privacy-hub.exe"),
        "-BundleB",
        str(fixture["bundleB"]),
        "-JsonOut",
        str(out_path),
        "-TempRootB",
        str(fixture["tempRootB"]),
        "-ContractFixtureJson",
        str(fixture_path),
        "-Quiet",
    ]
    if confirm:
        command.append("-ConfirmCreatesIdentity")
    completed = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )
    payload = json.loads(out_path.read_text(encoding="utf-8-sig"))
    return completed, payload


def _fixture(tmp: Path) -> dict[str, object]:
    return {
        "bundleA": "org.oslprivacy.hub",
        "bundleB": "org.oslprivacy.hubqab",
        "tempRootA": str(tmp / "temp-a"),
        "tempRootB": str(tmp / "temp-b"),
        "preflightStartupAllowed": True,
        "instanceAMarkerBefore": True,
        "instanceAMarkerAfter": True,
        "instanceAIdentityShaBefore": "a" * 64,
        "instanceAIdentityShaAfter": "a" * 64,
        "keyserverIdentitiesBefore": ["identity-a"],
        "keyserverIdentitiesAfter": ["identity-a", "identity-b"],
    }


def instance_b_launcher_uses_private_temp_root_and_preserves_instance_a() -> None:
    with tempfile.TemporaryDirectory() as raw_tmp:
        tmp = Path(raw_tmp)
        fixture = _fixture(tmp)

        completed, payload = _run_fixture(tmp, fixture, confirm=True)

        assert completed.returncode == 0, completed.stderr
        assert payload["overall"]["verdict"] == "ok"
        assert payload["instanceA"]["tempRoot"] != payload["instanceB"]["tempRoot"]
        assert payload["instanceA"]["identityUnchangedAcrossLaunch"] is True
        assert payload["instanceA"]["touchedByThisScript"] is False
        assert payload["instanceB"]["tempRootHonouredByChild"] is True
        assert (Path(str(fixture["tempRootB"])) / "osl-startup-trace.txt").is_file()
        assert not (Path(str(fixture["tempRootA"])) / "osl-startup-trace.txt").exists()


def instance_b_confirm_creates_identity_registers_second_identity() -> None:
    with tempfile.TemporaryDirectory() as raw_tmp:
        tmp = Path(raw_tmp)
        fixture = _fixture(tmp)

        blocked, blocked_payload = _run_fixture(tmp, fixture, confirm=False)
        assert blocked.returncode == 2, blocked.stderr
        assert blocked_payload["overall"]["verdict"] == "blocked"
        assert blocked_payload["steps"][-1]["step"] == "gate/consent"

        allowed, allowed_payload = _run_fixture(tmp, fixture, confirm=True)
        assert allowed.returncode == 0, allowed.stderr
        assert allowed_payload["overall"]["verdict"] == "ok"
        identity_step = next(
            step
            for step in allowed_payload["steps"]
            if step["step"] == "identity/keyserver-registration"
        )
        assert identity_step["result"] == "ok"
        assert identity_step["identitiesBefore"] == 1
        assert identity_step["identitiesAfter"] == 2
        assert allowed_payload["instanceB"]["registeredSecondIdentity"] is True


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(
        instance_b_launcher_uses_private_temp_root_and_preserves_instance_a,
    ))
    suite.addTest(unittest.FunctionTestCase(
        instance_b_confirm_creates_identity_registers_second_identity,
    ))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
