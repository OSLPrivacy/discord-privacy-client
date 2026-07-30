import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


def _run_osl_p2p_pair_refuses_same_osl_user_id(tmp_path: Path) -> None:
    pwsh = shutil.which("pwsh") or shutil.which("powershell")
    if pwsh is None or sys.platform != "win32":
        raise unittest.SkipTest("osl-p2p-pair.ps1 refusal test requires Windows PowerShell")

    appdata = tmp_path / "appdata"
    bundle_a = "org.oslprivacy.hub"
    bundle_b = "org.oslprivacy.hub.b"
    root_a = appdata / bundle_a / "osl-core"
    root_b = appdata / bundle_b / "osl-core"
    root_a.mkdir(parents=True)
    root_b.mkdir(parents=True)

    offer = {"schemaVersion": 1, "osl_user_id": "same-opaque-osl-user"}
    (root_a / "discord-qa-offer.v1.json").write_text(json.dumps(offer), encoding="utf-8")
    (root_b / "discord-qa-offer.v1.json").write_text(json.dumps(offer), encoding="utf-8")
    out = tmp_path / "pair-result.json"

    env = os.environ.copy()
    env["APPDATA"] = str(appdata)
    result = subprocess.run(
        [
            pwsh,
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            str(Path("scripts/qa/osl-p2p-pair.ps1")),
            "-BundleB",
            bundle_b,
            "-JsonOut",
            str(out),
            "-Quiet",
        ],
        cwd=Path(__file__).resolve().parents[2],
        env=env,
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )

    assert result.returncode == 1, result.stderr
    payload = json.loads(out.read_text(encoding="utf-8-sig"))
    assert payload["overall"]["verdict"] == "failed"
    assert payload["steps"][-1]["step"] == "gate/two-identities"
    assert payload["steps"][-1]["result"] == "failed"
    assert not (root_a / "discord-qa-peer-offer.v1.json").exists()
    assert not (root_b / "discord-qa-peer-offer.v1.json").exists()


def osl_p2p_pair_refuses_same_osl_user_id() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        _run_osl_p2p_pair_refuses_same_osl_user_id(Path(tmp))


def test_osl_p2p_pair_refuses_same_osl_user_id(tmp_path: Path) -> None:
    _run_osl_p2p_pair_refuses_same_osl_user_id(tmp_path)
