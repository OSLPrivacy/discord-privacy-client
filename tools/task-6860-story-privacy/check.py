#!/usr/bin/env python3
"""TASK 6860 — fail-closed check for story privacy settings and honest shielding.

Grades four artifacts, none of which it produces itself:

* the Rust scenario report from `story-privacy`'s production engine,
* the rendered story privacy surface from the shipped TypeScript module,
* one real Windows capture taken through `BitBlt` from the desktop DC, and
* the same capture binary's honest answer on a platform with no primitive.

Every pixel number in the capture is re-derived here from the BMP on disk, so a
report that merely *says* the shielded window was excluded does not pass. The
shielded verdict is differential across the two captures — the shielded window's
region must change completely while the control's does not move at all — because
`WDA_EXCLUDEFROMCAPTURE` removes a window from the capture rather than painting
it black.
"""

from __future__ import annotations

import argparse
import json
import struct
import subprocess
import sys
from pathlib import Path

AUDIENCES = ["story-audience-everyone", "story-audience-friends", "story-audience-verified"]
LIFETIMES = ["story-lifetime-1h", "story-lifetime-12h", "story-lifetime-24h"]
LIFETIME_LABELS = ["1H", "12H", "24H"]
LIFETIME_SECONDS = [3600, 43200, 86400]
PRIMITIVE = "SetWindowDisplayAffinity/WDA_EXCLUDEFROMCAPTURE"
EXCLUDE_FROM_CAPTURE_AFFINITY = 0x11

FAILURES: list[str] = []


def fail(detail: str) -> None:
    FAILURES.append(detail)
    print(f"TASK6860 FAIL {detail}", file=sys.stderr)


def need(condition: bool, detail: str) -> None:
    if not condition:
        fail(detail)


def equal(actual, expected, detail: str) -> None:
    if actual != expected:
        fail(f"{detail} actual={actual!r} expected={expected!r}")


def windows_path(value: str) -> Path:
    """Resolve a Windows path from WSL, leaving POSIX paths alone."""
    if len(value) > 2 and value[1] == ":":
        drive = value[0].lower()
        return Path(f"/mnt/{drive}/" + value[2:].replace("\\", "/").lstrip("/"))
    return Path(value)


# ---------------------------------------------------------------------------
# Independent BMP reader — the capture is graded from the image, not the claim
# ---------------------------------------------------------------------------


def read_bmp(path: Path):
    data = path.read_bytes()
    if len(data) < 54 or data[:2] != b"BM":
        raise ValueError(f"{path} is not a BMP")
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    planes, bits = struct.unpack_from("<HH", data, 26)
    if planes != 1 or bits != 32:
        raise ValueError(f"{path} is not a 32-bit BMP")
    bottom_up = height > 0
    height = abs(height)
    return data, offset, width, height, bottom_up


def region_pixels(path: Path, origin, rect, inset: int = 8):
    """Every RGB triple inside a screen rect, read straight out of the image."""
    data, offset, width, height, bottom_up = read_bmp(path)
    origin_x, origin_y = origin
    left = max(rect[0] - origin_x + inset, 0)
    top = max(rect[1] - origin_y + inset, 0)
    right = min(rect[2] - origin_x - inset, width)
    bottom = min(rect[3] - origin_y - inset, height)
    row_bytes = width * 4
    pixels = []
    for y in range(top, bottom):
        source_row = (height - 1 - y) if bottom_up else y
        base = offset + source_row * row_bytes
        for x in range(left, right):
            index = base + x * 4
            pixels.append((data[index + 2], data[index + 1], data[index]))
    return pixels


def region_stats(path: Path, origin, rect, known_rgb, inset: int = 8):
    """Count known-colour and black pixels inside a screen rect, from the image."""
    known = tuple(known_rgb)
    pixels = region_pixels(path, origin, rect, inset)
    return {
        "total_pixels": len(pixels),
        "known_colour_pixels": sum(1 for pixel in pixels if pixel == known),
        "black_pixels": sum(1 for pixel in pixels if pixel == (0, 0, 0)),
    }


def region_changed(before: Path, after: Path, origin, rect, inset: int = 8) -> int:
    """How many pixels of a rect differ between two captures of the same screen."""
    first = region_pixels(before, origin, rect, inset)
    second = region_pixels(after, origin, rect, inset)
    if len(first) != len(second):
        raise ValueError("the two captures do not cover the same region")
    return sum(1 for a, b in zip(first, second) if a != b)


# ---------------------------------------------------------------------------


def check_vocabulary(report, ui) -> None:
    equal(report["stable_ids"]["audiences"], AUDIENCES, "audience stable ids")
    equal(report["stable_ids"]["lifetimes"], LIFETIMES, "lifetime stable ids")
    equal(report["stable_ids"]["lifetime_labels"], LIFETIME_LABELS, "lifetime labels")
    equal(report["stable_ids"]["lifetime_seconds"], LIFETIME_SECONDS, "lifetime seconds")
    equal([row["id"] for row in ui["audiences"]], AUDIENCES, "surface audience ids")
    equal([row["id"] for row in ui["lifetimes"]], LIFETIMES, "surface lifetime ids")
    equal([row["label"] for row in ui["lifetimes"]], LIFETIME_LABELS, "surface lifetime labels")
    equal([row["seconds"] for row in ui["lifetimes"]], LIFETIME_SECONDS, "surface lifetime seconds")
    equal(report["copy"], ui["copy"], "engine and surface copy disagree")
    for key in ("shield_disclosure", "shield_unavailable", "receipts_on_viewer", "receipts_off_viewer"):
        need(len(report["copy"].get(key, "")) > 20, f"copy {key} is missing or trivial")
    need("camera" in report["copy"]["shield_disclosure"], "shield disclosure never mentions a camera")
    need(
        "external capture device" in report["copy"]["shield_disclosure"],
        "shield disclosure never mentions external capture",
    )


def check_defaults(report) -> int:
    cells = report["defaults_inheritance"]
    equal(len(cells), 9, "default inheritance matrix size")
    equal(sorted({cell["default_audience"] for cell in cells}), sorted(AUDIENCES), "audiences exercised")
    equal(sorted({cell["default_lifetime"] for cell in cells}), sorted(LIFETIMES), "lifetimes exercised")
    for cell in cells:
        name = f"{cell['default_audience']}/{cell['default_lifetime']}"
        need(cell["story_survived_restart"], f"the story itself was lost on restart {name}")
        equal(cell["story_audience"], cell["default_audience"], f"story ignored the audience default {name}")
        equal(cell["story_lifetime"], cell["default_lifetime"], f"story ignored the lifetime default {name}")
        equal(cell["audience_source"], "inherited-default", f"story did not record inheritance {name}")
        equal(cell["expires_at_ms"], cell["expected_expires_at_ms"], f"burn deadline {name}")
        equal(cell["audience_size"], cell["expected_audience_size"], f"frozen audience size {name}")
        equal(
            cell["defaults_after_restart_audience"],
            cell["default_audience"],
            f"audience default lost on restart {name}",
        )
        equal(
            cell["defaults_after_restart_lifetime"],
            cell["default_lifetime"],
            f"lifetime default lost on restart {name}",
        )
        need(cell["audience_size"] > 0, f"empty frozen audience {name}")
    return len(cells)


def check_burn(report) -> int:
    cells = report["burn_boundaries"]
    equal(len(cells), 3, "burn boundary count")
    equal([cell["lifetime"] for cell in cells], LIFETIMES, "burn boundaries exercised")
    equal([cell["lifetime_seconds"] for cell in cells], LIFETIME_SECONDS, "burn boundary seconds")
    for cell in cells:
        name = cell["lifetime_label"]
        equal(
            cell["expires_at_ms"] - cell["created_at_ms"],
            cell["lifetime_seconds"] * 1000,
            f"deadline arithmetic {name}",
        )
        equal(cell["before_sweep_burned"], 0, f"burned before its deadline {name}")
        need(cell["before_readable"], f"unreadable before its deadline {name}")
        need(cell["before_byte_identical"], f"body not byte-identical before the deadline {name}")
        equal(cell["at_sweep_burned"], 1, f"restart across the boundary did not burn {name}")
        need(not cell["at_boundary_readable"], f"still readable at the boundary {name}")
        equal(cell["at_boundary_bytes"], 0, f"bytes recovered at the boundary {name}")
        need(cell["burned_at_boundary"], f"not marked burned {name}")
        equal(cell["sealed_bytes_after_burn"], 0, f"sealed body survived the burn {name}")
        need(cell["retained_ciphertext_bytes"] > 0, f"nothing was ever sealed {name}")
        equal(cell["later_already_burned"], 1, f"burn state lost on a second restart {name}")
        need(not cell["after_second_restart_readable"], f"resurrected after restart {name}")
        need(cell["still_burned"], f"burn not durable {name}")
        equal(cell["restored_backup_sweep_burned"], 1, f"restored offline copy did not burn {name}")
        need(not cell["restored_backup_readable"], f"restored offline copy still opens {name}")
        equal(cell["restored_backup_bytes"], 0, f"restored offline copy recovered bytes {name}")
    return len(cells)


def check_override(report) -> None:
    section = report["send_to_override"]
    inherited = section["inherited"]
    equal(inherited["audience"], "story-audience-everyone", "inherited audience")
    equal(inherited["source"], "inherited-default", "inherited source")
    equal(inherited["audience_size"], 6, "inherited audience size")
    need(inherited["stranger_readable"], "EVERYONE default excluded a stranger")

    overridden = section["overridden"]
    equal(overridden["audience"], "story-audience-verified", "override did not change the audience")
    equal(overridden["source"], "send-to-override", "override not recorded as an override")
    equal(overridden["audience_size"], 3, "override audience size")
    need(overridden["verified_readable"], "SEND TO VERIFIED shut out a verified reader")
    need(overridden["verified_bytes"] > 0, "verified reader recovered no bytes")
    need(not overridden["friend_only_readable"], "SEND TO override leaked to a friend")
    equal(overridden["friend_only_bytes"], 0, "friend recovered bytes from an override")
    need(not overridden["stranger_readable"], "SEND TO override leaked to a stranger")
    equal(overridden["stranger_bytes"], 0, "stranger recovered bytes from an override")
    need(not overridden["friend_only_addressed"], "friend holds a key slot on an override")
    need(not overridden["stranger_addressed"], "stranger holds a key slot on an override")

    widened = section["widened"]
    equal(widened["audience"], "story-audience-everyone", "override could not widen a narrow default")
    equal(widened["source"], "send-to-override", "widened source")
    equal(widened["audience_size"], 6, "widened audience size")
    need(widened["stranger_readable"], "widened override did not reach a stranger")

    narrow = section["narrow_default"]
    equal(narrow["audience"], "story-audience-verified", "narrow default not inherited")
    equal(narrow["source"], "inherited-default", "narrow default source")
    equal(narrow["audience_size"], 3, "narrow default audience size")
    need(not narrow["stranger_readable"], "narrow default leaked to a stranger")

    snapshot = section["relationship_snapshot"]
    need(
        snapshot["published_before_change_still_readable"],
        "a later relationship change reached an already published story",
    )
    equal(snapshot["published_after_change_size"], 2, "later relationship change ignored for new stories")
    need(not snapshot["published_after_change_readable"], "demoted person still reached by a new story")


def check_observation(observation, label: str, expect_zero: bool) -> None:
    need(observation["probes"] >= 4, f"{label}: fewer than four probe identities")
    need(observation["files_scanned"] >= 3, f"{label}: observer looked at fewer than three files")
    need(observation["store_bytes_scanned"] > 0, f"{label}: store observer scanned 0 bytes")
    equal(observation["client_identity_hits"], 0, f"{label}: viewer identity in the client")
    equal(observation["store_identity_hits"], 0, f"{label}: viewer identity in the store")
    equal(observation["log_identity_hits"], 0, f"{label}: viewer identity in the log")
    if expect_zero:
        equal(observation["client_records"], 0, f"{label}: client kept a per-view record")
        equal(observation["store_records"], 0, f"{label}: store kept a per-view record")
        equal(observation["log_records"], 0, f"{label}: log kept a per-view record")


def check_receipts(report) -> dict:
    receipts = report["receipts"]
    copy = report["copy"]

    on = receipts["on"]
    equal(on["count_after_four_opens"], 4, "four distinct opens did not produce four")
    equal(on["count_after_identical_replays"], 4, "an identical replay moved the count")
    equal(on["count_after_new_repeat_open"], 5, "a genuinely new repeat open did not count")
    equal(on["count_after_restart"], 5, "the aggregate did not survive restart")
    need(on["outsider_refused"], "someone outside the audience registered a view")
    equal(sorted(on["signal_fields"]), ["story_id", "view_count"], "the view signal carries extra fields")
    equal(on["signal"]["view_count"], 5, "signal count")
    equal(on["pre_open_copy"], copy["receipts_on_viewer"], "receipts-on viewer copy")
    check_observation(on["observation"], "receipts-on", expect_zero=False)
    check_observation(on["observation_after_restart"], "receipts-on restart", expect_zero=False)
    need(on["observation"]["log_records"] > 0, "receipts-on wrote no view signal at all")
    need(on["observation"]["store_records"] > 0, "receipts-on persisted no aggregate at all")

    off = receipts["off"]
    equal(off["outcomes"], ["no-signal-recorded"] * 4, "receipts-off recorded a signal")
    need(not off["signal_present"], "receipts-off exposed a poster signal")
    need(not off["signal_present_after_restart"], "receipts-off exposed a poster signal after restart")
    equal(off["pre_open_copy"], copy["receipts_off_viewer"], "receipts-off viewer copy")
    check_observation(off["observation"], "receipts-off", expect_zero=True)
    check_observation(off["observation_after_restart"], "receipts-off restart", expect_zero=True)

    retro = receipts["retroactive"]
    equal(retro["count_before_switch_off"], 4, "the on-period did not record four views")
    need(retro["records_before_switch_off"] > 0, "the on-period left nothing to erase")
    equal(retro["rows_destroyed"], 1, "switching off destroyed no receipt row")
    equal(retro["log_lines_destroyed"], 4, "switching off left view signals in the log")
    need(retro["records_destroyed"] >= 9, "switching off destroyed too few records")
    need(not retro["signal_after_switch_off"], "a poster signal survived switching receipts off")
    check_observation(retro["observation_after"], "receipts retroactive", expect_zero=True)
    check_observation(retro["observation_after_restart"], "receipts retroactive restart", expect_zero=True)
    return {
        "on_count": on["count_after_new_repeat_open"],
        "off_records": sum(
            off["observation"][key] for key in ("client_records", "store_records", "log_records")
        ),
        "retro_records": sum(
            retro["observation_after"][key]
            for key in ("client_records", "store_records", "log_records")
        ),
        "erased": retro["records_destroyed"],
    }


def check_shield_engine(report) -> None:
    shield = report["shield"]
    copy = report["copy"]
    equal(shield["primitive"], PRIMITIVE, "named capture-protection primitive")

    unsupported = shield["unsupported"]
    equal(unsupported["refusal"], copy["shield_unavailable"], "turning the shield on was not refused")
    for label in ("state", "defaults_state", "state_after_restart"):
        state = unsupported[label]
        need(not state["supported"], f"unsupported {label}: claims a primitive exists")
        need(not state["control_enabled"], f"unsupported {label}: control is still operable")
        need(not state["setting_on"], f"unsupported {label}: setting is on")
        need(not state["claims_protection"], f"unsupported {label}: claims protection")
        equal(state["primitive"], None, f"unsupported {label}: names a primitive")
        equal(state["disclosure"], None, f"unsupported {label}: shows a protection disclosure")
        equal(state["unavailable_copy"], copy["shield_unavailable"], f"unsupported {label}: honest copy")
    need(
        not unsupported["story_shield_on_at_publish"],
        "a story recorded shielding on a platform that cannot shield",
    )

    supported = shield["supported"]
    for label in ("state", "state_after_restart"):
        state = supported[label]
        need(state["supported"], f"supported {label}: reports no primitive")
        need(state["control_enabled"], f"supported {label}: control disabled")
        need(state["setting_on"], f"supported {label}: setting lost")
        need(state["claims_protection"], f"supported {label}: no claim after enabling")
        equal(state["primitive"], PRIMITIVE, f"supported {label}: primitive")
        equal(state["disclosure"], copy["shield_disclosure"], f"supported {label}: disclosure")
        equal(state["unavailable_copy"], None, f"supported {label}: unavailable copy shown")
    need(supported["story_shield_on_at_publish"], "a shielded story did not record the shield")
    need(
        not supported["off_state"]["claims_protection"],
        "the shield claims protection while switched off",
    )

    ported = shield["ported_profile_on_unsupported"]
    need(not ported["setting_on"], "a shield setting carried onto a platform with no primitive")
    need(not ported["claims_protection"], "a ported profile claims protection with no primitive")

    native = shield["native"]
    expected = native["target_os"] == "windows"
    equal(native["state"]["supported"], expected, "the engine disagrees with the real platform")
    if not expected:
        need(not native["state"]["claims_protection"], "native platform claims protection")
        need(not native["application"]["enforced"], "native platform reports enforcement")
        need(native["application"]["error"] is not None, "native refusal carried no reason")


def check_surface(report, ui) -> None:
    copy = report["copy"]
    supported = ui["supported"]["settings_markup"]
    unsupported = ui["unsupported"]["settings_markup"]

    for audience in AUDIENCES:
        need(f'data-story-audience="{audience}"' in supported, f"surface omits audience {audience}")
    for lifetime, seconds in zip(LIFETIMES, LIFETIME_SECONDS):
        need(f'data-story-lifetime="{lifetime}"' in supported, f"surface omits lifetime {lifetime}")
        need(
            f'data-story-lifetime-seconds="{seconds}"' in supported,
            f"surface omits the {lifetime} deadline",
        )

    need('data-story-shield-available="true"' in supported, "surface hides an available shield")
    need('data-story-shield-claims-protection="true"' in supported, "shielded surface makes no claim")
    need(copy["shield_disclosure"] in supported, "shielded surface hides the camera disclosure")

    need('data-story-shield-available="false"' in unsupported, "unsupported surface claims availability")
    need(
        'data-story-shield-claims-protection="false"' in unsupported,
        "unsupported surface claims protection",
    )
    need('aria-disabled="true"' in unsupported, "unsupported shield control is not disabled")
    need("disabled>OFF</button>" in unsupported, "unsupported shield control is still operable")
    need(copy["shield_unavailable"] in unsupported, "unsupported surface hides the honest copy")
    need(copy["shield_disclosure"] not in unsupported, "unsupported surface shows a protection disclosure")
    need("protected" not in unsupported.lower(), "unsupported surface still uses protection language")

    equal(ui["unsupported"]["shield"]["claimsProtection"], False, "unsupported shield row state")
    equal(ui["supported"]["shield"]["claimsProtection"], True, "supported shield row state")

    inherited = ui["composer_inherited"]
    equal(inherited["resolved"]["source"], "inherited-default", "composer inheritance source")
    equal(inherited["resolved"]["audience"], "story-audience-friends", "composer inherited audience")
    need('data-story-audience-source="inherited-default"' in inherited["markup"], "composer inheritance markup")
    override = ui["composer_override"]
    equal(override["resolved"]["source"], "send-to-override", "composer override source")
    equal(override["resolved"]["audience"], "story-audience-verified", "composer override audience")
    need('data-story-audience-source="send-to-override"' in override["markup"], "composer override markup")
    need(
        'data-story-resolved-audience="story-audience-verified"' in override["markup"],
        "composer override does not change the resolved audience",
    )

    need(copy["receipts_on_viewer"] in ui["receipts_on"]["viewer_markup"], "receipts-on viewer copy missing")
    need(copy["receipts_off_viewer"] in ui["receipts_off"]["viewer_markup"], "receipts-off viewer copy missing")
    need(
        copy["receipts_on_viewer"] not in ui["receipts_off"]["viewer_markup"],
        "receipts-off surface shows the counted-view copy",
    )


def check_real_capture(capture, report) -> dict:
    equal(capture.get("target_os"), "windows", "the real capture did not run on Windows")
    need(capture.get("platform_supported") is True, "the capture platform reports no primitive")
    need(capture.get("capture_attempted") is True, "no capture was attempted")
    need(capture.get("cosmetic_mode") is False, "the graded capture ran in cosmetic mode")
    equal(capture.get("capture_method"), "win32-bitblt-srccopy-captureblt-desktop-dc", "capture method")
    equal(capture.get("primitive"), PRIMITIVE, "capture primitive")

    application = capture.get("application", {})
    equal(application.get("mode"), "production-primitive", "the shield was not applied by the product")
    need(application.get("enforced") is True, "the production shield reported no enforcement")
    need(application.get("claims_protection") is True, "an enforced shield made no claim")
    equal(application.get("error"), None, "the shield application carried an error")
    equal(application.get("disclosure"), report["copy"]["shield_disclosure"], "capture disclosure")

    equal(capture.get("affinity_control"), 0, "the control window was protected too")
    equal(
        capture.get("affinity_shielded"),
        EXCLUDE_FROM_CAPTURE_AFFINITY,
        "the shielded window's display affinity",
    )

    region = capture["capture_region"]
    origin = (region["left"], region["top"])
    known = tuple(capture["known_colour_rgb"])
    control_rect = capture["control_rect"]
    shielded_rect = capture["shielded_rect"]

    measured = {}
    for pass_name in ("baseline_capture", "shielded_capture"):
        section = capture[pass_name]
        need(section.get("captured") is True, f"{pass_name} produced no image")
        image = windows_path(section["image"])
        need(image.exists(), f"{pass_name} image is missing at {image}")
        if not image.exists():
            continue
        need(image.stat().st_size > 10_000, f"{pass_name} image is too small to be a real capture")
        measured[pass_name] = {
            "control": region_stats(image, origin, control_rect, known),
            "shielded": region_stats(image, origin, shielded_rect, known),
            "bytes": image.stat().st_size,
            "path": image,
        }
        for region_name in ("control", "shielded"):
            equal(
                measured[pass_name][region_name],
                section[region_name],
                f"{pass_name} {region_name}: the reported pixels are not the pixels in the image",
            )

    if len(measured) != 2:
        return {}

    baseline = measured["baseline_capture"]
    shielded = measured["shielded_capture"]
    for region_name in ("control", "shielded"):
        need(
            baseline[region_name]["total_pixels"] > 10_000,
            f"baseline {region_name} region is too small to judge",
        )
        equal(
            baseline[region_name]["known_colour_pixels"],
            baseline[region_name]["total_pixels"],
            f"baseline {region_name} window was not fully captured before shielding",
        )
    equal(
        shielded["control"]["known_colour_pixels"],
        shielded["control"]["total_pixels"],
        "the unshielded control window stopped being captured",
    )
    equal(
        shielded["shielded"]["known_colour_pixels"],
        0,
        "the shielded window still appears in a real OS capture",
    )

    # `WDA_EXCLUDEFROMCAPTURE` (0x11) does not paint the window black — that is
    # the older `WDA_MONITOR` (0x01). It removes the window from the capture
    # altogether, so whatever is behind it shows through. The honest test is
    # therefore differential: between the two passes the shielded window's whole
    # region must change, while the unshielded control's region must not move at
    # all. That also rules out the lazy explanations for a colourless region —
    # a corrupt second frame, or a capture of a different part of the screen.
    shielded_changed = region_changed(
        baseline["path"], shielded["path"], origin, shielded_rect
    )
    control_changed = region_changed(baseline["path"], shielded["path"], origin, control_rect)
    region_total = baseline["shielded"]["total_pixels"]
    need(
        shielded_changed >= region_total * 99 // 100,
        f"the shielded window did not leave the capture: only {shielded_changed} of "
        f"{region_total} pixels changed once the shield was applied",
    )
    equal(
        control_changed,
        0,
        "the unshielded control region moved between the two captures, so the "
        "second capture is not a comparable frame",
    )
    return {
        "baseline_known": baseline["shielded"]["known_colour_pixels"],
        "shielded_known": shielded["shielded"]["known_colour_pixels"],
        "shielded_changed": shielded_changed,
        "control_changed": control_changed,
        "control_known": shielded["control"]["known_colour_pixels"],
        "region_pixels": region_total,
    }


def check_unsupported_capture(capture, report) -> None:
    need(capture.get("platform_supported") is False, "the unsupported run reported a primitive")
    need(capture.get("capture_attempted") is False, "the unsupported run claimed a capture")
    need(capture.get("claims_protection") is False, "the unsupported run claimed protection")
    need(capture.get("control_enabled") is False, "the unsupported run left the control operable")
    need(capture.get("setting_on") is False, "the unsupported run left the setting on")
    equal(capture.get("primitive"), None, "the unsupported run named a primitive")
    equal(capture.get("disclosure"), None, "the unsupported run showed a protection disclosure")
    equal(
        capture.get("unavailable_copy"),
        report["copy"]["shield_unavailable"],
        "the unsupported run's honest copy",
    )
    need(capture.get("application_enforced") is False, "the unsupported run reported enforcement")
    need(capture.get("application_error") is not None, "the unsupported refusal carried no reason")
    need(capture.get("target_os") != "windows", "the unsupported case ran on Windows")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--ui", type=Path, required=True)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--capture-unsupported", type=Path, required=True)
    parser.add_argument("--ui-root", type=Path, default=Path("apps/osl-hub-ui"))
    parser.add_argument("--skip-vitest", action="store_true")
    args = parser.parse_args()

    report = json.loads(args.report.read_text(encoding="utf-8"))
    ui = json.loads(args.ui.read_text(encoding="utf-8"))
    capture = json.loads(args.capture.read_text(encoding="utf-8"))
    unsupported_capture = json.loads(args.capture_unsupported.read_text(encoding="utf-8"))

    check_vocabulary(report, ui)
    defaults = check_defaults(report)
    burns = check_burn(report)
    check_override(report)
    receipts = check_receipts(report)
    check_shield_engine(report)
    check_surface(report, ui)
    capture_numbers = check_real_capture(capture, report)
    check_unsupported_capture(unsupported_capture, report)

    unit_tests = "skipped"
    if not args.skip_vitest:
        result = subprocess.run(
            ["npx", "vitest", "run", "src/story-privacy-6860.test.ts", "--reporter=basic"],
            cwd=args.ui_root,
            capture_output=True,
            text=True,
        )
        passed = 0
        for line in (result.stdout + result.stderr).splitlines():
            if "Tests" in line and "passed" in line:
                for token in line.split():
                    if token.isdigit():
                        passed = int(token)
                        break
        if result.returncode != 0:
            fail(f"surface unit tests exited {result.returncode}")
        need(passed >= 10, f"surface unit tests reported only {passed} passing")
        unit_tests = f"{passed} passed"

    if FAILURES:
        print(f"TASK6860 FAIL failures={len(FAILURES)}", file=sys.stderr)
        return 1
    print(
        "TASK6860 PASS "
        f"default_cells={defaults} burn_boundaries={burns} "
        f"receipts_on_count={receipts['on_count']} receipts_off_records={receipts['off_records']} "
        f"retroactive_records={receipts['retro_records']} retroactive_erased={receipts['erased']} "
        f"capture_region_pixels={capture_numbers.get('region_pixels')} "
        f"baseline_shielded_known={capture_numbers.get('baseline_known')} "
        f"shielded_known={capture_numbers.get('shielded_known')} "
        f"shielded_changed={capture_numbers.get('shielded_changed')} "
        f"control_changed={capture_numbers.get('control_changed')} "
        f"control_known={capture_numbers.get('control_known')} "
        f"affinity_shielded=0x{capture.get('affinity_shielded', 0):02X} "
        f"unsupported_claims={unsupported_capture.get('claims_protection')} "
        f"surface_tests={unit_tests}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
