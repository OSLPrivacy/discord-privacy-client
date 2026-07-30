import json
import os
import shutil
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
LAUNCHER = ROOT / "scripts" / "qa" / "osl-launch-instance-b.ps1"
BUNDLE_A = "org.oslprivacy.hub"
BUNDLE_B = "org.oslprivacy.hubqab"


def require_windows_powershell() -> str:
    pwsh = shutil.which("pwsh") or shutil.which("powershell")
    if pwsh is None or sys.platform != "win32":
        raise unittest.SkipTest("osl-launch-instance-b.ps1 behavioral tests require Windows PowerShell")
    return pwsh


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8-sig"))


def write_fake_win32_module(path: Path) -> None:
    path.write_text(
        textwrap.dedent(
            r'''
            Add-Type @"
            using System;
            using System.Collections.Generic;
            using System.IO;

            public class P2PRect {
                public int L;
                public int T;
                public int R;
                public int B;
            }

            public class P2PW {
                public static string[] BundleMarkers() {
                    string state = Environment.GetEnvironmentVariable("OSL_LAUNCH_B_FAKE_STATE");
                    var markers = new List<string>();
                    markers.Add("1001=org.oslprivacy.hub=4097");
                    if (!String.IsNullOrEmpty(state) && File.Exists(Path.Combine(state, "b_started"))) {
                        markers.Add("4242=org.oslprivacy.hubqab=8194");
                    }
                    return markers.ToArray();
                }

                public static bool IsWindow(IntPtr hwnd) {
                    return true;
                }

                public static bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy, int flags) {
                    return true;
                }

                public static bool GetWindowRect(IntPtr hwnd, ref P2PRect rect) {
                    rect.L = 120;
                    rect.T = 80;
                    rect.R = 920;
                    rect.B = 680;
                    return true;
                }
            }
"@

            function Get-P2PDiscordPids { @() }

            function Get-P2PFileStamp {
                param([string]$Path, [string]$Label)
                if (-not (Test-Path -LiteralPath $Path)) {
                    return [pscustomobject]@{ exists = $false; path = $Path; sizeBytes = 0; sha256 = ''; written = $null }
                }
                $item = Get-Item -LiteralPath $Path
                $sha = if ($Label -eq 'instance-b-exe') {
                    $env:OSL_LAUNCH_B_FAKE_EXE_SHA
                } else {
                    $hash = [System.Security.Cryptography.SHA256]::Create()
                    try {
                        $bytes = [System.IO.File]::ReadAllBytes($Path)
                        -join ($hash.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') })
                    } finally {
                        $hash.Dispose()
                    }
                }
                [pscustomobject]@{
                    exists = $true
                    path = $Path
                    sizeBytes = [int64]$item.Length
                    sha256 = $sha
                    written = $item.LastWriteTime.ToString('o')
                }
            }

            function Get-P2PBundleMap {
                $map = @{}
                $map[1001] = 'org.oslprivacy.hub'
                if (Test-Path -LiteralPath (Join-Path $env:OSL_LAUNCH_B_FAKE_STATE 'b_started')) {
                    $map[4242] = 'org.oslprivacy.hubqab'
                }
                $map
            }

            function Get-P2PWindows {
                $windows = @(
                    [pscustomobject]@{ Hwnd = [int64]4098; Pid = 1001; Title = 'OSL Privacy'; Cls = 'Tauri Window'; Owner = 0 }
                )
                if (Test-Path -LiteralPath (Join-Path $env:OSL_LAUNCH_B_FAKE_STATE 'b_started')) {
                    $windows += [pscustomobject]@{ Hwnd = [int64]8195; Pid = 4242; Title = 'OSL Privacy'; Cls = 'Tauri Window'; Owner = 0 }
                }
                $windows
            }

            function Get-P2PFamily {
                param([int]$RootPid)
                @($RootPid, 4242)
            }

            function Test-P2PProcessAlive {
                param([int]$ProcId)
                $true
            }
            '''
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def write_fake_exe(path: Path) -> None:
    path.write_text(
        textwrap.dedent(
            r'''
            @echo off
            if "%1"=="--b6-preflight-only" (
              > "%TEMP%\osl-discord-qa-b6-preflight.v2.json" echo {"schemaVersion":2,"b6Preflight":{"schemaVersion":2,"binarySha256":"%OSL_LAUNCH_B_FAKE_EXE_SHA%","sourceCommit":"test-commit","serverDeploymentIdentity":"dedicated-qa","startupAllowed":true,"startupBlockers":[]}}
              exit /b 0
            )
            set "ROOT=%APPDATA%\%OSL_LAUNCH_B_FAKE_BUNDLE_B%\osl-core"
            mkdir "%ROOT%" >nul 2>nul
            > "%ROOT%\identity.json" echo fake-instance-b-identity
            > "%ROOT%\discord-qa-offer.v1.json" echo {"version":1,"friend_code":"OSLFR1.fake","osl_user_id":"fake-b","safety_number":"123456"}
            > "%TEMP%\osl-startup-trace.txt" echo fake-startup
            > "%OSL_LAUNCH_B_FAKE_STATE%\b_started" echo 1
            exit /b 0
            '''
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def prepare_harness(tmp_path: Path) -> dict:
    qa_dir = tmp_path / "qa"
    qa_dir.mkdir(parents=True)
    launcher = qa_dir / "osl-launch-instance-b.ps1"
    shutil.copy2(LAUNCHER, launcher)
    write_fake_win32_module(qa_dir / "osl-p2p-win32.ps1")
    exe_b = qa_dir / "fake-osl-b.cmd"
    write_fake_exe(exe_b)
    (qa_dir / "WebView2Loader.dll").write_bytes(b"fake-loader")

    appdata = tmp_path / "appdata"
    root_a = appdata / BUNDLE_A / "osl-core"
    root_a.mkdir(parents=True)
    (root_a / "identity.json").write_text("fake-instance-a-identity", encoding="utf-8")

    temp_a = tmp_path / "temp-a"
    temp_b = tmp_path / "temp-b"
    localappdata = tmp_path / "localappdata"
    state = tmp_path / "state"
    for directory in (temp_a, temp_b, localappdata, state):
        directory.mkdir()

    env = os.environ.copy()
    env.update(
        {
            "APPDATA": str(appdata),
            "LOCALAPPDATA": str(localappdata),
            "TEMP": str(temp_a),
            "TMP": str(temp_a),
            "OSL_LAUNCH_B_FAKE_STATE": str(state),
            "OSL_LAUNCH_B_FAKE_BUNDLE_B": BUNDLE_B,
            "OSL_LAUNCH_B_FAKE_EXE_SHA": "b" * 64,
        }
    )
    return {
        "launcher": launcher,
        "exe_b": exe_b,
        "appdata": appdata,
        "root_a": root_a,
        "temp_a": temp_a,
        "temp_b": temp_b,
        "state": state,
        "env": env,
    }


def run_launcher(pwsh: str, harness: dict, json_out: Path, confirm: bool) -> subprocess.CompletedProcess:
    args = [
        pwsh,
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        str(harness["launcher"]),
        "-ExeB",
        str(harness["exe_b"]),
        "-BundleB",
        BUNDLE_B,
        "-BundleA",
        BUNDLE_A,
        "-JsonOut",
        str(json_out),
        "-TempRootB",
        str(harness["temp_b"]),
        "-NoRelocate",
        "-Quiet",
    ]
    if confirm:
        args.append("-ConfirmCreatesIdentity")
    return subprocess.run(
        args,
        cwd=ROOT,
        env=harness["env"],
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )


def step(payload: dict, name: str) -> dict:
    matches = [candidate for candidate in payload["steps"] if candidate["step"] == name]
    assert matches, f"missing step {name}"
    return matches[-1]


def instance_b_launcher_uses_private_temp_root_and_preserves_instance_a(tmp_path: Path) -> None:
    pwsh = require_windows_powershell()
    harness = prepare_harness(tmp_path)
    result_path = tmp_path / "launch-result.json"

    result = run_launcher(pwsh, harness, result_path, confirm=True)

    assert result.returncode == 0, result.stderr
    payload = read_json(result_path)
    assert payload["overall"]["verdict"] == "ok"
    assert step(payload, "temp-isolation")["result"] == "ok"
    assert step(payload, "assert/temp-redirect")["result"] == "ok"
    assert step(payload, "assert/instance-a-untouched")["result"] == "ok"
    assert payload["instanceA"]["tempRoot"] == str(harness["temp_a"])
    assert payload["instanceB"]["tempRoot"] == str(harness["temp_b"])
    assert payload["instanceA"]["tempRoot"] != payload["instanceB"]["tempRoot"]
    assert payload["instanceA"]["identityUnchangedAcrossLaunch"] is True
    assert (harness["root_a"] / "identity.json").read_text(encoding="utf-8") == "fake-instance-a-identity"


def instance_b_confirm_creates_identity_registers_second_identity(tmp_path: Path) -> None:
    pwsh = require_windows_powershell()
    refused = prepare_harness(tmp_path / "refused")
    refused_path = tmp_path / "refused.json"

    refused_result = run_launcher(pwsh, refused, refused_path, confirm=False)

    assert refused_result.returncode == 2, refused_result.stderr
    refused_payload = read_json(refused_path)
    assert refused_payload["overall"]["verdict"] == "blocked"
    assert step(refused_payload, "gate/consent")["result"] == "failed"
    assert not (refused["state"] / "b_started").exists()
    assert not (refused["appdata"] / BUNDLE_B / "osl-core" / "identity.json").exists()

    confirmed = prepare_harness(tmp_path / "confirmed")
    confirmed_path = tmp_path / "confirmed.json"
    confirmed_result = run_launcher(pwsh, confirmed, confirmed_path, confirm=True)

    assert confirmed_result.returncode == 0, confirmed_result.stderr
    payload = read_json(confirmed_path)
    assert payload["overall"]["verdict"] == "ok"
    assert step(payload, "gate/consent")["result"] == "ok"
    assert step(payload, "assert/instance-b-identity")["result"] == "ok"
    identity_b = confirmed["appdata"] / BUNDLE_B / "osl-core" / "identity.json"
    offer_b = confirmed["appdata"] / BUNDLE_B / "osl-core" / "discord-qa-offer.v1.json"
    assert identity_b.is_file()
    assert offer_b.is_file()
    assert identity_b.read_text(encoding="utf-8") != (confirmed["root_a"] / "identity.json").read_text(encoding="utf-8")


test_instance_b_launcher_uses_private_temp_root_and_preserves_instance_a = (
    instance_b_launcher_uses_private_temp_root_and_preserves_instance_a
)
test_instance_b_confirm_creates_identity_registers_second_identity = (
    instance_b_confirm_creates_identity_registers_second_identity
)
