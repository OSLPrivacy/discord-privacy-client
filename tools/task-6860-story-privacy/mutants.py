#!/usr/bin/env python3
"""TASK 6860 — starve the implementation and prove the check goes red.

Each mutation edits the real shipped source (or the real capture evidence),
re-runs the same unmodified check, and expects exit 1. Every source file is
restored from a hash-verified backup afterwards, and the run ends by showing
the restored tree still exits 0.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "crates/story-privacy/src/lib.rs"
SURFACE = ROOT / "apps/osl-hub-ui/src/story-privacy-6860.ts"
CHECK = ROOT / "tools/task-6860-story-privacy/check.py"
UI_ROOT = ROOT / "apps/osl-hub-ui"

CARGO = os.environ.get("CARGO", str(Path.home() / ".cargo/bin/cargo"))
TARGET_DIR = os.environ.get("CARGO_TARGET_DIR", "/mnt/d/osl-lane-targets/i")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def restore(path: Path, backup: Path) -> None:
    """Restore the original bytes with a fresh mtime.

    `shutil.copy2` would put the *old* timestamp back, which leaves cargo
    convinced the mutated artifact is still current and silently grades the
    next run against a stale binary.
    """
    path.write_bytes(backup.read_bytes())
    os.utime(path, None)


def substitute(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise SystemExit(
            f"TASK6860_MUTANT_SETUP anchor appears {text.count(old)} times in {path.name}: {old[:60]!r}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


def build_report(out: Path, work: Path) -> tuple[bool, bool, str]:
    """Rebuild and run the scenario runner. Returns (built, ran, detail)."""
    environment = dict(os.environ, CARGO_TARGET_DIR=TARGET_DIR, RUSTC_WRAPPER="")
    built = subprocess.run(
        [CARGO, "build", "-q", "-p", "story-privacy", "--bin", "task-6860-story-privacy"],
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
    )
    if built.returncode != 0:
        return False, False, built.stderr.strip().splitlines()[-1] if built.stderr.strip() else "build failed"
    ran = subprocess.run(
        [
            CARGO,
            "run",
            "-q",
            "-p",
            "story-privacy",
            "--bin",
            "task-6860-story-privacy",
            "--",
            str(out),
            str(work),
        ],
        cwd=ROOT,
        env=environment,
        capture_output=True,
        text=True,
    )
    if ran.returncode != 0:
        tail = ran.stderr.strip().splitlines()
        return True, False, tail[-1] if tail else "scenario runner refused"
    return True, True, ""


def render_ui(out: Path) -> bool:
    rendered = subprocess.run(
        ["npx", "vite-node", "scripts/render-story-privacy-6860.ts", str(out)],
        cwd=UI_ROOT,
        capture_output=True,
        text=True,
    )
    return rendered.returncode == 0


def run_check(report: Path, ui: Path, capture: Path, unsupported: Path) -> int:
    return subprocess.run(
        [
            sys.executable,
            str(CHECK),
            "--report",
            str(report),
            "--ui",
            str(ui),
            "--capture",
            str(capture),
            "--capture-unsupported",
            str(unsupported),
            "--ui-root",
            str(UI_ROOT),
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    ).returncode


# --- the mutations ---------------------------------------------------------

ENGINE_MUTATIONS = {
    "starve-audience-default": (
        "let audience = send_to.unwrap_or(defaults.audience);",
        "let audience = send_to.unwrap_or(StoryAudience::Everyone);",
    ),
    "starve-send-to-override": (
        "let audience = send_to.unwrap_or(defaults.audience);",
        "let audience = defaults.audience;",
    ),
    "starve-burn-boundary": (
        "if record.expires_at_ms <= now_ms {\n                // Key destruction",
        "if record.expires_at_ms + 1 <= now_ms {\n                // Key destruction",
    ),
    "starve-receipt-mode-off": (
        "        if !self.settings.view_receipts {\n            // Nothing is created: no row, no token, no count, no log line.\n            return Ok(ViewOutcome::NoSignalRecorded);\n        }\n",
        "",
    ),
    "starve-retroactive-erasure": (
        "            erasure.records_destroyed = self.receipts.record_count();\n            self.receipts.rows.clear();",
        "            erasure.records_destroyed = self.receipts.record_count();",
    ),
    "starve-zero-record-observer": (
        "        observation.files_scanned += 1;\n        observation.store_bytes_scanned += bytes.len();",
        "        observation.store_bytes_scanned += 0;",
    ),
    "starve-restart": (
        "    fn persist_stories(&self) -> Result<(), String> {\n",
        "    fn persist_stories(&self) -> Result<(), String> {\n        return Ok(());\n        #[allow(unreachable_code)]\n",
    ),
    "starve-unsupported-case": (
        "            control_enabled: false,\n            setting_on: false,\n            claims_protection: false,",
        "            control_enabled: false,\n            setting_on: false,\n            claims_protection: true,",
    ),
}

SURFACE_MUTATIONS = {
    "starve-unsupported-surface-copy": (
        "      claimsProtection: false,\n      copy: STORY_PRIVACY_COPY.shield_unavailable,",
        "      claimsProtection: true,\n      copy: STORY_PRIVACY_COPY.shield_disclosure,",
    ),
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--capture-cosmetic", type=Path, required=True)
    parser.add_argument("--capture-unsupported", type=Path, required=True)
    parser.add_argument("--workdir", type=Path, required=True)
    args = parser.parse_args()

    args.workdir.mkdir(parents=True, exist_ok=True)
    baseline_report = args.workdir / "report.json"
    baseline_ui = args.workdir / "ui.json"
    scenario_work = args.workdir / "work"

    before = {path: digest(path) for path in (ENGINE, SURFACE)}
    backups = {}
    backup_root = Path(tempfile.mkdtemp(prefix="task6860-backup-"))
    for path in (ENGINE, SURFACE):
        copy = backup_root / path.name
        shutil.copy2(path, copy)
        backups[path] = copy

    failures: list[str] = []
    try:
        built, ran, detail = build_report(baseline_report, scenario_work)
        if not (built and ran):
            print(f"TASK6860_MUTANT_SETUP baseline report failed: {detail}", file=sys.stderr)
            return 1
        if not render_ui(baseline_ui):
            print("TASK6860_MUTANT_SETUP baseline surface render failed", file=sys.stderr)
            return 1

        for name, (old, new) in list(ENGINE_MUTATIONS.items()) + [
            (key, value) for key, value in SURFACE_MUTATIONS.items()
        ]:
            target = ENGINE if name in ENGINE_MUTATIONS else SURFACE
            substitute(target, old, new)
            try:
                report = args.workdir / f"{name}-report.json"
                ui = args.workdir / f"{name}-ui.json"
                shutil.copy2(baseline_report, report)
                shutil.copy2(baseline_ui, ui)
                reason = ""
                if target is ENGINE:
                    built, ran, detail = build_report(report, args.workdir / f"{name}-work")
                    if not built:
                        print(f"TASK6860_MUTANT name={name} exit=BUILD-FAILED ({detail})")
                        failures.append(name)
                        continue
                    if not ran:
                        code = 1
                        reason = f"scenario runner refused: {detail}"
                    else:
                        code = run_check(report, ui, args.capture, args.capture_unsupported)
                else:
                    if not render_ui(ui):
                        code = 1
                        reason = "surface render refused"
                    else:
                        code = run_check(baseline_report, ui, args.capture, args.capture_unsupported)
            finally:
                restore(target, backups[target])
            print(f"TASK6860_MUTANT name={name} exit={code}{(' ' + reason) if reason else ''}")
            if code != 1:
                failures.append(name)

        # Evidence-side mutations: the check must read the image, not the claim.
        capture = json.loads(args.capture.read_text(encoding="utf-8"))

        missing = json.loads(json.dumps(capture))
        for section in ("baseline_capture", "shielded_capture"):
            missing[section]["image"] = str(args.workdir / f"absent-{section}.bmp")
        missing_path = args.workdir / "capture-missing-image.json"
        missing_path.write_text(json.dumps(missing), encoding="utf-8")
        code = run_check(baseline_report, baseline_ui, missing_path, args.capture_unsupported)
        print(f"TASK6860_MUTANT name=starve-real-capture exit={code}")
        if code != 1:
            failures.append("starve-real-capture")

        fabricated = json.loads(json.dumps(capture))
        fabricated["shielded_capture"]["shielded"]["known_colour_pixels"] = 0
        fabricated["shielded_capture"]["shielded"]["black_pixels"] = fabricated["shielded_capture"][
            "shielded"
        ]["total_pixels"]
        fabricated["baseline_capture"]["shielded"]["known_colour_pixels"] = fabricated[
            "baseline_capture"
        ]["shielded"]["total_pixels"]
        # Point both passes at the *cosmetic* run's images: the numbers now say
        # "protected" while the pixels on disk say the window was captured.
        cosmetic = json.loads(args.capture_cosmetic.read_text(encoding="utf-8"))
        for section in ("baseline_capture", "shielded_capture"):
            fabricated[section]["image"] = cosmetic[section]["image"]
        fabricated_path = args.workdir / "capture-fabricated.json"
        fabricated_path.write_text(json.dumps(fabricated), encoding="utf-8")
        code = run_check(baseline_report, baseline_ui, fabricated_path, args.capture_unsupported)
        print(f"TASK6860_MUTANT name=fabricated-capture-numbers exit={code}")
        if code != 1:
            failures.append("fabricated-capture-numbers")

        code = run_check(
            baseline_report, baseline_ui, args.capture_cosmetic, args.capture_unsupported
        )
        print(f"TASK6860_MUTANT name=cosmetic-shielding exit={code}")
        if code != 1:
            failures.append("cosmetic-shielding")

        unsupported_claims = json.loads(args.capture_unsupported.read_text(encoding="utf-8"))
        unsupported_claims["claims_protection"] = True
        unsupported_claims["control_enabled"] = True
        unsupported_claims["setting_on"] = True
        unsupported_claims["unavailable_copy"] = None
        unsupported_path = args.workdir / "capture-unsupported-claims.json"
        unsupported_path.write_text(json.dumps(unsupported_claims), encoding="utf-8")
        code = run_check(baseline_report, baseline_ui, args.capture, unsupported_path)
        print(f"TASK6860_MUTANT name=starve-unsupported-run exit={code}")
        if code != 1:
            failures.append("starve-unsupported-run")
    finally:
        for path, copy in backups.items():
            restore(path, copy)
        shutil.rmtree(backup_root, ignore_errors=True)

    after = {path: digest(path) for path in (ENGINE, SURFACE)}
    for path in before:
        state = "unchanged" if before[path] == after[path] else "CHANGED"
        print(f"TASK6860_RESTORED file={path.relative_to(ROOT)} sha256={after[path][:16]} {state}")
        if before[path] != after[path]:
            failures.append(f"restore:{path.name}")

    built, ran, detail = build_report(baseline_report, args.workdir / "restored-work")
    if not (built and ran):
        print(f"TASK6860_RESTORED report failed: {detail}", file=sys.stderr)
        return 1
    render_ui(baseline_ui)
    restored = run_check(baseline_report, baseline_ui, args.capture, args.capture_unsupported)
    print(f"TASK6860_RESTORED exit={restored}")
    if failures or restored != 0:
        print(f"TASK6860_MUTANTS FAIL {sorted(set(failures))}", file=sys.stderr)
        return 1
    print("TASK6860_MUTANTS PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
