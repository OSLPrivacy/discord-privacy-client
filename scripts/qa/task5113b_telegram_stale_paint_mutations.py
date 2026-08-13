#!/usr/bin/env python3
"""Production-source mutation proof for TASK 5113b.

Each candidate alters the shipping Telegram repair controller, then reruns the
unchanged parent acceptance target.  The test target drives the controller's
port (not a saved capture, manifest label, or fixture renderer), so a passing
candidate can only paint by executing the altered production function.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "apps/osl-hub/src/native_telegram_adapter.rs"
MANIFEST = ROOT / "apps/osl-hub/Cargo.toml"
TEST = "task_5113_telegram_appearance_repair"


MUTANTS: dict[str, tuple[str, str, str]] = {
    "appearance-invalidation-drift-guard": (
        "guard.invalidate(DriftEvent::BeforeReveal);",
        "// TASK5113B_MUTANT: retain the prior appearance fingerprint.",
        "colour_only_restyle_replaces_a_warmed_fingerprint",
    ),
    "pre-repair-hide-painter": (
        "    port.hide_before_repair();\n    let target = match port.resolve_structure(surface) {",
        "    // TASK5113B_MUTANT: stale OSL pixels remain visible during repair.\n    let target = match port.resolve_structure(surface) {",
        "geometry_colour_type_radius_and_spacing_drift_repair_before_repaint",
    ),
    "live-second-sampler": (
        """    let second = match port.sample(&target) {
        Ok(sample) if valid_measured_paint(&sample.paint) => sample,
        _ => {
            return refuse_telegram_appearance(
                port,
                TelegramAppearanceRepairFailure::SampleUnavailable,
            )
        }
    };""",
        "    let second = first.clone(); // TASK5113B_MUTANT: disconnect live second sampler.",
        "zero_or_multiple_structure_and_missing_or_unstable_samples_refuse_without_paint",
    ),
    "post-repair-local-comparison": (
        "if !port.local_5103_matches(&target) {",
        "if false && !port.local_5103_matches(&target) { // TASK5113B_MUTANT",
        "local_5103_failure_and_stale_repaint_exit_with_a_single_5105_refusal",
    ),
    "capture-exclusion-before-painter": (
        "if !port.capture_exclusion_verified(&target) {",
        "if false && !port.capture_exclusion_verified(&target) { // TASK5113B_MUTANT",
        "capture_exclusion_is_read_back_after_live_samples_and_before_paint",
    ),
}


def invoke() -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = "/mnt/d/osl-lane-targets/i"
    env["RUSTC_WRAPPER"] = ""
    return subprocess.run(
        [
            "cargo", "test", "--manifest-path", str(MANIFEST),
            "--no-default-features", "--features", "core", "--test", TEST,
            "--", "--test-threads=1",
        ],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    starved = os.environ.get("TASK5113B_STARVE_MUTANT", "")
    if starved:
        if starved not in MUTANTS:
            print(f"TASK5113B FAIL unknown mutant starvation={starved}", file=sys.stderr)
            return 1
        print(f"TASK5113B FAIL missing production mutation={starved}", file=sys.stderr)
        return 1

    original = SOURCE.read_text(encoding="utf-8")
    control = invoke()
    if control.returncode != 0:
        print(f"TASK5113B FAIL restored parent acceptance exit={control.returncode}\n{control.stdout}{control.stderr}", file=sys.stderr)
        return 1

    for name, (old, new, visible_defect) in MUTANTS.items():
        if original.count(old) != 1:
            print(f"TASK5113B FAIL mutant={name} production source anchor count={original.count(old)}", file=sys.stderr)
            return 1
        SOURCE.write_text(original.replace(old, new, 1), encoding="utf-8")
        try:
            red = invoke()
        finally:
            SOURCE.write_text(original, encoding="utf-8")
        combined = red.stdout + red.stderr
        if red.returncode != 101 or visible_defect not in combined:
            print(
                f"TASK5113B FAIL mutant={name} expected_exit=101 expected_defect={visible_defect} "
                f"actual_exit={red.returncode}\n{combined}",
                file=sys.stderr,
            )
            return 1
        print(f"TASK5113B MUTANT name={name} exit=101 visible_defect={visible_defect}")

    restored = invoke()
    if restored.returncode != 0:
        print(f"TASK5113B FAIL restoration exit={restored.returncode}\n{restored.stdout}{restored.stderr}", file=sys.stderr)
        return 1
    print(f"TASK5113B PASS mutants={len(MUTANTS)} red_exit=101 restored_exit=0 parent=production-controller")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
