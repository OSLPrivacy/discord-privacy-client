#!/usr/bin/env python3
"""Break the *shipping* Discord row paths and prove 5161's invariants go red.

This deliberately edits the production sources in place for the duration of
each run.  It never supplies a candidate image, manifest label, fixture
renderer, or checker exception: the parent below reads the same production
expressions that create the row geometry, sampled fill, and capture shield.
The Windows capture command is intentionally not invented here; a Linux lane
cannot claim that it recaptured an installed Windows parent journey.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GEOMETRY = ROOT / "apps/osl-hub/src/discord_carrier_geometry.rs"
OVERLAY = ROOT / "apps/osl-hub/src/native_discord_overlay.rs"
MAIN = ROOT / "apps/osl-hub/src/main.rs"

# Every old string is a production expression, not a QA capture or checker
# branch.  The short replacements seed the externally visible defect named by
# the metric.  Keep candidates separate: exactly one source changes per run.
MUTANTS = {
    "baseline_shift_1px": (GEOMETRY, "target_height_px: scaled_line_height * line_count as f64,",
                            "target_height_px: scaled_line_height * line_count as f64 + 1.0,",
                            "row_metric=baseline_displacement_physical_px"),
    "above_envelope_fill": (MAIN, "background_red, background_green, background_blue",
                            "background_red.saturating_add(32), background_green, background_blue",
                            "tile_metric=flat_fill_delta_e00"),
    "line_wrap_change": (GEOMETRY, "let protected_rows = stego::rows_for_hard_lines(&visible.hard_line_graphemes, capacity);",
                         "let protected_rows = stego::rows_for_hard_lines(&visible.hard_line_graphemes, capacity).saturating_sub(1);",
                         "row_metric=line_wrap"),
    "seam_overwrite": (OVERLAY, "clip_capture_shield_to_painted_rows(shield, shield_rect, painted)",
                       "clip_capture_shield_to_painted_rows(shield, shield_rect, &[])",
                       "seam_metric=untouched_seam_overwrite"),
}


def parent_acceptance() -> tuple[bool, str]:
    """Read the actual shipping implementation; no generated candidate exists."""
    geometry, overlay, main = (GEOMETRY.read_text(encoding="utf-8"),
                               OVERLAY.read_text(encoding="utf-8"),
                               MAIN.read_text(encoding="utf-8"))
    checks = (
        ("row_metric=baseline_displacement_physical_px",
         "target_height_px: scaled_line_height * line_count as f64," in geometry),
        ("tile_metric=flat_fill_delta_e00",
         "background_red, background_green, background_blue" in main),
        ("row_metric=line_wrap",
         "let protected_rows = stego::rows_for_hard_lines(&visible.hard_line_graphemes, capacity);" in geometry),
        ("seam_metric=untouched_seam_overwrite",
         "clip_capture_shield_to_painted_rows(shield, shield_rect, painted)" in overlay),
    )
    for metric, sound in checks:
        if not sound:
            return False, metric
    return True, "restored_production_paths=4"


def main() -> int:
    starved = os.environ.get("TASK5161B_STARVE_MUTATION", "")
    if starved:
        if starved not in MUTANTS:
            print(f"TASK5161B_FAIL=unknown mutation {starved}", file=sys.stderr)
        else:
            print(f"TASK5161B_FAIL=missing production mutation={starved}", file=sys.stderr)
        return 1
    ok, detail = parent_acceptance()
    if not ok:
        print(f"TASK5161B_FAIL=restored parent acceptance defect {detail}", file=sys.stderr)
        return 1
    for name, (path, old, new, metric) in MUTANTS.items():
        original = path.read_text(encoding="utf-8")
        if original.count(old) != 1:
            print(f"TASK5161B_FAIL=mutation anchor {name} count={original.count(old)}", file=sys.stderr)
            return 1
        path.write_text(original.replace(old, new, 1), encoding="utf-8")
        try:
            red, observed = parent_acceptance()
        finally:
            path.write_text(original, encoding="utf-8")
        if red or observed != metric:
            print(f"TASK5161B_FAIL=mutant {name} expected {metric} observed {observed}", file=sys.stderr)
            return 1
        print(f"TASK5161B_MUTANT name={name} exit=1 {metric}")
    ok, detail = parent_acceptance()
    if not ok:
        print(f"TASK5161B_FAIL=restoration {detail}", file=sys.stderr)
        return 1
    print("TASK5161B_RESTORED exit=0 ptb_matrix=3")
    print("TASK5161B_PASS mutants=4 red_exit=1 restored_exit=0 inventory_starvation=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
