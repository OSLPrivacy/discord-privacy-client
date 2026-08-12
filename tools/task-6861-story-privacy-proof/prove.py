#!/usr/bin/env python3
"""TASK 6861 — the red proof for TASK 6860's story privacy settings.

Every mutation is applied inside its **own throwaway copy** of the tree. The
copy runs the *unmodified* 6860 pipeline — the shipped scenario runner, the
shipped surface renderer, and 6860's own `check.py` — against the real Windows
capture evidence, and the copy is deleted before the next mutation starts. The
working tree is never edited, so a mutation cannot leak out of the run.

Five families are starved, one behaviour per copy:

  data-minimization  a receipt is retained while receipts are off
  expiry             each burn boundary in turn is missed (1H, 12H, 24H)
  override           SEND TO is ignored and the default is taken instead
  platform           shielding is claimed with no supported primitive
  honesty            the protection promise is exposed without its qualification

A mutation only counts when the check exits 1 **and names the story or the
setting it starved** — an exit code on its own does not say the observer saw
the right thing. The expiry mutations additionally have to leave the other two
boundaries un-named, which is only possible if that copy really is running that
copy's binary.

The proof then proves itself. `--starve mutant|observer|restoration` re-runs it
with one of its own load-bearing parts removed, and each starved run has to come
back non-zero:

  mutant       the mutation is not applied, so a green check must fail the proof
  observer     the copy's check.py loses the receipts-off assertions, so a
               genuinely retained receipt goes unseen and must fail the proof
  restoration  a mutation is left behind in the restored copy, so the restored
               real-capture and unsupported cases must fail the proof
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools/task-6861-story-privacy-proof"
CHECK_REL = Path("tools/task-6860-story-privacy/check.py")
ENGINE_REL = Path("crates/story-privacy/src/lib.rs")
SURFACE_REL = Path("apps/osl-hub-ui/src/story-privacy-6860.ts")
MANIFEST_REL = Path("apps/osl-hub-ui/src/story-privacy-6860-manifest.json")
REAL_CAPTURE_REL = Path("tools/task-6861-story-privacy-proof/evidence/real-capture.json")
UNSUPPORTED_CAPTURE_REL = Path(
    "tools/task-6861-story-privacy-proof/evidence/unsupported-capture.json"
)

# Everything a mutation is allowed to reach. Hashed before and after the whole
# run: a copy that leaked back into the tree is a failed proof.
TREE_FILES = [ENGINE_REL, SURFACE_REL, MANIFEST_REL, CHECK_REL, REAL_CAPTURE_REL, UNSUPPORTED_CAPTURE_REL]

CARGO = os.environ.get("CARGO", str(Path.home() / ".cargo/bin/cargo"))
LANE_TARGET_DIR = os.environ.get("CARGO_TARGET_DIR", "/mnt/d/osl-lane-targets/i")
# The copies build into their own subdirectory of this lane's target dir. They
# must not share `<lane>/debug` with the working tree: a copy's sources are
# stamped with a fresh mtime, so an artifact it leaves behind looks newer than
# the working tree's own (older, unedited) sources and cargo serves the copy's
# binary to the next ordinary `cargo test` in the tree. That is a mutant leaking
# out of a throwaway copy through the build cache, and it made the 6860 gate
# report a failure the tree did not have.
TARGET_DIR = os.environ.get("TASK6861_TARGET_DIR", str(Path(LANE_TARGET_DIR) / "task6861-copies"))

UI_REL = Path("apps/osl-hub-ui")

QUALIFIED_DISCLOSURE = (
    "Blocks Windows screen capture of this story. It cannot block a camera pointed at "
    "your screen, an external capture device, or a photo of your display."
)
UNQUALIFIED_PROMISE = "Your story is protected from screenshots."


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# ---------------------------------------------------------------------------
# The mutations
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Edit:
    path: Path  # relative to the copy root
    old: str
    new: str


@dataclass(frozen=True)
class Mutation:
    name: str
    category: str
    starves: str
    edits: tuple[Edit, ...]
    # The red output must contain each of these — this is the "naming
    # story/setting" half of the finish line.
    must_name: tuple[str, ...]
    # And must not contain any of these, which is how a stale binary or a
    # blunderbuss mutation is told apart from the precise one.
    must_not_name: tuple[str, ...] = field(default=())
    # Evidence-side mutations edit the copy's capture JSON instead of source.
    capture_patch: tuple[tuple[str, object], ...] = field(default=())


def engine(old: str, new: str) -> Edit:
    return Edit(ENGINE_REL, old, new)


BURN_ANCHOR = (
    "            if record.expires_at_ms <= now_ms {\n                // Key destruction"
)


def missed_boundary(label: str, lifetime_id: str, others: tuple[str, str]) -> Mutation:
    """The sweep walks past exactly one boundary, so that story is retained."""
    return Mutation(
        name=f"miss-burn-boundary-{label.lower()}",
        category="expiry",
        starves=f"the {label} auto-burn boundary: the sweep skips {lifetime_id}, so the "
        "story's keys and sealed body are retained past its deadline",
        edits=(
            engine(
                BURN_ANCHOR,
                f'            if record.expires_at_ms <= now_ms && record.lifetime != "{lifetime_id}" {{\n'
                "                // Key destruction",
            ),
        ),
        must_name=(
            f"restart across the boundary did not burn {label}",
            f"sealed body survived the burn {label}",
            f"not marked burned {label}",
        ),
        must_not_name=(
            f"restart across the boundary did not burn {others[0]}",
            f"restart across the boundary did not burn {others[1]}",
        ),
    )


MUTATIONS: list[Mutation] = [
    Mutation(
        name="retain-receipt-while-off",
        category="data-minimization",
        starves="the receipts-off setting: the early return is deleted, so a view is "
        "recorded and retained while receipts are off",
        edits=(
            engine(
                "        if !self.settings.view_receipts {\n"
                "            // Nothing is created: no row, no token, no count, no log line.\n"
                "            return Ok(ViewOutcome::NoSignalRecorded);\n"
                "        }\n",
                "",
            ),
        ),
        must_name=(
            "receipts-off recorded a signal",
            "receipts-off: client kept a per-view record",
            "receipts-off: store kept a per-view record",
        ),
    ),
    Mutation(
        name="retain-receipt-after-switching-off",
        category="data-minimization",
        starves="the retroactive half of the receipts setting: switching receipts off "
        "stops clearing the rows the on-period wrote, so they are retained",
        edits=(
            engine(
                "            erasure.records_destroyed = self.receipts.record_count();\n"
                "            self.receipts.rows.clear();",
                "            erasure.records_destroyed = self.receipts.record_count();",
            ),
        ),
        must_name=(
            "receipts retroactive: client kept a per-view record",
            "receipts retroactive: store kept a per-view record",
        ),
    ),
    missed_boundary("1H", "story-lifetime-1h", ("12H", "24H")),
    missed_boundary("12H", "story-lifetime-12h", ("1H", "24H")),
    missed_boundary("24H", "story-lifetime-24h", ("1H", "12H")),
    Mutation(
        name="ignore-send-to-override",
        category="override",
        starves="the per-story SEND TO override: every story takes the default audience "
        "setting instead",
        edits=(
            engine(
                "let audience = send_to.unwrap_or(defaults.audience);",
                "let audience = defaults.audience;",
            ),
        ),
        must_name=(
            "override did not change the audience",
            "SEND TO override leaked to a friend",
            "SEND TO override leaked to a stranger",
        ),
    ),
    Mutation(
        name="claim-shield-without-primitive",
        category="platform",
        starves="the screenshot shield setting on a platform with no primitive: the "
        "no-primitive branch claims protection anyway",
        edits=(
            engine(
                "            control_enabled: false,\n"
                "            setting_on: false,\n"
                "            claims_protection: false,",
                "            control_enabled: false,\n"
                "            setting_on: false,\n"
                "            claims_protection: true,",
            ),
        ),
        must_name=(
            "unsupported state: claims protection",
            "unsupported defaults_state: claims protection",
            "unsupported state_after_restart: claims protection",
        ),
    ),
    Mutation(
        name="unsupported-run-claims-protection",
        category="platform",
        starves="the real platform answer for the screenshot shield: the run on the "
        "platform with no primitive reports a protected story",
        edits=(),
        capture_patch=(
            ("claims_protection", True),
            ("control_enabled", True),
            ("setting_on", True),
            ("primitive", "SetWindowDisplayAffinity/WDA_EXCLUDEFROMCAPTURE"),
        ),
        must_name=(
            "the unsupported run claimed protection",
            "the unsupported run left the control operable",
            "the unsupported run named a primitive",
        ),
        # This copy edits no Rust. If it were graded against the previous copy's
        # binary it would carry that copy's engine failure too.
        must_not_name=("unsupported state: claims protection",),
    ),
    Mutation(
        name="unqualified-protection-promise",
        category="honesty",
        starves="the screenshot shield promise: the shipped disclosure drops its "
        "qualification and promises protection outright",
        edits=(
            engine(
                f'pub const SHIELD_DISCLOSURE: &str = "{QUALIFIED_DISCLOSURE}";',
                f'pub const SHIELD_DISCLOSURE: &str = "{UNQUALIFIED_PROMISE}";',
            ),
            Edit(
                MANIFEST_REL,
                f'"shield_disclosure": "{QUALIFIED_DISCLOSURE}"',
                f'"shield_disclosure": "{UNQUALIFIED_PROMISE}"',
            ),
        ),
        must_name=(
            "shield disclosure never mentions a camera",
            "shield disclosure never mentions external capture",
        ),
    ),
    Mutation(
        name="unsupported-surface-promises-protection",
        category="honesty",
        starves="the shield row on a system with no primitive: it shows the protection "
        "disclosure and claims the story is protected",
        edits=(
            Edit(
                SURFACE_REL,
                "      claimsProtection: false,\n      copy: STORY_PRIVACY_COPY.shield_unavailable,",
                "      claimsProtection: true,\n      copy: STORY_PRIVACY_COPY.shield_disclosure,",
            ),
        ),
        must_name=(
            "unsupported surface claims protection",
            "unsupported surface shows a protection disclosure",
            "unsupported shield row state",
        ),
        # Surface-only: the engine in this copy must still be the honest one.
        must_not_name=("shield disclosure never mentions a camera",),
    ),
]

CATEGORIES = ["data-minimization", "expiry", "override", "platform", "honesty"]

# The observer starvation: the copy's check.py stops looking for retained
# receipts, so `retain-receipt-while-off` sails past it.
OBSERVER_STARVE = (
    '        equal(observation["client_records"], 0, f"{label}: client kept a per-view record")\n'
    '        equal(observation["store_records"], 0, f"{label}: store kept a per-view record")\n'
    '        equal(observation["log_records"], 0, f"{label}: log kept a per-view record")\n',
    "        pass\n",
)
OBSERVER_STARVE_SIGNAL = (
    '    equal(off["outcomes"], ["no-signal-recorded"] * 4, "receipts-off recorded a signal")\n',
    "\n",
)


# ---------------------------------------------------------------------------
# Throwaway copies
# ---------------------------------------------------------------------------


def make_copy(dest: Path) -> None:
    """A fresh, self-contained copy of everything the 6860 pipeline needs."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        shutil.copy2(ROOT / name, dest / name)
    shutil.copytree(ROOT / ".cargo", dest / ".cargo")
    shutil.copytree(ROOT / "crates", dest / "crates", symlinks=True)
    (dest / "tools").mkdir()
    shutil.copytree(ROOT / "tools/task-6860-story-privacy", dest / "tools/task-6860-story-privacy")
    shutil.copytree(TOOL, dest / "tools/task-6861-story-privacy-proof")
    ui = dest / UI_REL
    ui.mkdir(parents=True)
    for entry in (ROOT / UI_REL).iterdir():
        if entry.is_file():
            shutil.copy2(entry, ui / entry.name)
    for sub in ("src", "scripts"):
        shutil.copytree(ROOT / UI_REL / sub, ui / sub)
    # node_modules is a dependency, never a mutation target: link it so the copy
    # is cheap and so the copy cannot be accused of shipping a different vitest.
    (ui / "node_modules").symlink_to(ROOT / UI_REL / "node_modules")

    # Every copied file gets a fresh mtime. `copy2`/`copytree` preserve the
    # original timestamps, and cargo decides a unit is up to date by comparing
    # source mtimes against the fingerprint it recorded for the artifact already
    # sitting in the shared target dir. A copy that edits no Rust would then be
    # graded against whatever binary the *previous* copy built — which is how a
    # mutation gets attributed to the wrong starve. This is not cosmetic: it is
    # the difference between running this copy and running the last one.
    now = time.time()
    for current, _directories, files in os.walk(dest):
        for name in files:
            path = Path(current) / name
            if not path.is_symlink():
                os.utime(path, (now, now))


def substitute(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"TASK6861_SETUP anchor appears {count} times in {path.name}: {old[:70]!r}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


# ---------------------------------------------------------------------------
# Running the unmodified 6860 pipeline inside a copy
# ---------------------------------------------------------------------------


def run_pipeline(copy: Path) -> tuple[int, str, str]:
    """Build, run and grade inside `copy`. Returns (exit, stdout, stderr)."""
    environment = dict(os.environ, CARGO_TARGET_DIR=TARGET_DIR)
    environment.pop("RUSTC_WRAPPER", None)
    report = copy / "report.json"
    ui_json = copy / "ui.json"

    ran = subprocess.run(
        [
            CARGO, "run", "-q", "-p", "story-privacy",
            "--bin", "task-6860-story-privacy", "--",
            str(report), str(copy / "scenario-work"),
        ],
        cwd=copy, env=environment, capture_output=True, text=True,
    )
    if ran.returncode != 0:
        tail = (ran.stderr.strip().splitlines() or ["scenario runner refused"])[-1]
        return 1, "", f"TASK6860 FAIL the scenario runner refused to run: {tail}"

    rendered = subprocess.run(
        ["npx", "vite-node", "scripts/render-story-privacy-6860.ts", str(ui_json)],
        cwd=copy / UI_REL, capture_output=True, text=True,
    )
    if rendered.returncode != 0:
        tail = (rendered.stderr.strip().splitlines() or ["surface render refused"])[-1]
        return 1, "", f"TASK6860 FAIL the surface renderer refused to run: {tail}"

    graded = subprocess.run(
        [
            sys.executable, str(copy / CHECK_REL),
            "--report", str(report),
            "--ui", str(ui_json),
            "--capture", str(copy / REAL_CAPTURE_REL),
            "--capture-unsupported", str(copy / UNSUPPORTED_CAPTURE_REL),
            "--ui-root", str(copy / UI_REL),
        ],
        cwd=copy, capture_output=True, text=True,
    )
    return graded.returncode, graded.stdout, graded.stderr


def prepare(copy: Path, mutation: Mutation | None, starve: str | None) -> list[str]:
    """Populate a copy. Returns notes about anything deliberately starved."""
    make_copy(copy)
    notes: list[str] = []

    if mutation is not None and starve != "mutant":
        for edit in mutation.edits:
            substitute(copy / edit.path, edit.old, edit.new)
        if mutation.capture_patch:
            path = copy / UNSUPPORTED_CAPTURE_REL
            payload = json.loads(path.read_text(encoding="utf-8"))
            for key, value in mutation.capture_patch:
                payload[key] = value
            path.write_text(json.dumps(payload, indent=1), encoding="utf-8")
    elif mutation is not None:
        notes.append("mutation not applied (starve=mutant)")

    if starve == "observer":
        substitute(copy / CHECK_REL, *OBSERVER_STARVE)
        substitute(copy / CHECK_REL, *OBSERVER_STARVE_SIGNAL)
        notes.append("check.py in the copy lost its receipts-off assertions (starve=observer)")
    return notes


def check_integrity(copy: Path) -> str | None:
    """The copy has to be grading with 6860's own check, byte for byte."""
    if digest(copy / CHECK_REL) != digest(ROOT / CHECK_REL):
        return "the copy is not grading with the unmodified 6860 check"
    return None


def discard(copy: Path) -> bool:
    shutil.rmtree(copy, ignore_errors=True)
    return not copy.exists()


# ---------------------------------------------------------------------------
# The proof
# ---------------------------------------------------------------------------


def named(output: str, needles: tuple[str, ...]) -> tuple[list[str], list[str]]:
    present = [needle for needle in needles if needle in output]
    absent = [needle for needle in needles if needle not in output]
    return present, absent


def parse_pass_line(stdout: str) -> dict[str, str]:
    for line in stdout.splitlines():
        if line.startswith("TASK6860 PASS"):
            fields = {}
            for token in line.split()[2:]:
                if "=" in token:
                    key, value = token.split("=", 1)
                    fields[key] = value
            return fields
    return {}


def run_restored(copies: Path, starve: str | None, failures: list[str]) -> None:
    """The restored copy: no mutation, real capture, unsupported case, must pass."""
    copy = copies / "restored"
    notes = []
    make_copy(copy)
    if starve == "restoration":
        leak = MUTATIONS[0]
        for edit in leak.edits:
            substitute(copy / edit.path, edit.old, edit.new)
        notes.append(f"restoration starved: {leak.name} left behind in the restored copy")
    code, stdout, stderr = run_pipeline(copy)
    fields = parse_pass_line(stdout)
    ok = code == 0
    # A pass that measured nothing is not a pass.
    region = fields.get("capture_region_pixels", "0")
    real_capture_graded = (
        fields.get("shielded_changed") == region
        and region.isdigit()
        and int(region) > 10_000
        and fields.get("control_changed") == "0"
        and fields.get("shielded_known") == "0"
        and fields.get("affinity_shielded") == "0x11"
    )
    unsupported_graded = fields.get("unsupported_claims") == "False"
    print(
        f"TASK6861_RESTORED exit={code} "
        f"real_capture={'graded' if real_capture_graded else 'NOT-GRADED'} "
        f"unsupported={'graded' if unsupported_graded else 'NOT-GRADED'} "
        f"region_pixels={fields.get('capture_region_pixels', '-')} "
        f"shielded_changed={fields.get('shielded_changed', '-')} "
        f"control_changed={fields.get('control_changed', '-')} "
        f"unsupported_claims={fields.get('unsupported_claims', '-')} "
        f"surface_tests={fields.get('surface_tests', '-')}"
        + (" " + "; ".join(notes) if notes else "")
    )
    if not ok:
        for line in stderr.strip().splitlines()[:6]:
            print(f"    {line}")
        failures.append("restored: the restored copy did not pass")
    if not real_capture_graded:
        failures.append("restored: the real capture was not graded")
    if not unsupported_graded:
        failures.append("restored: the unsupported case was not graded")
    if not discard(copy):
        failures.append("restored: the copy was not discarded")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workdir", type=Path, default=Path("/tmp/task6861"))
    parser.add_argument("--only", action="append", default=None)
    parser.add_argument("--starve", choices=["mutant", "observer", "restoration"], default=None)
    parser.add_argument("--no-self-test", action="store_true")
    parser.add_argument("--show-red", action="store_true", help="print every red line a copy produced")
    args = parser.parse_args()

    selected = [m for m in MUTATIONS if not args.only or m.name in args.only]
    if args.only and len(selected) != len(args.only):
        raise SystemExit(f"TASK6861_SETUP unknown mutation in {args.only}")

    copies = args.workdir / "copies"
    if copies.exists():
        shutil.rmtree(copies)
    copies.mkdir(parents=True)

    print(f"TASK6861_TARGET copies_build_in={TARGET_DIR} lane_target={LANE_TARGET_DIR}")
    if Path(TARGET_DIR).resolve() == Path(LANE_TARGET_DIR).resolve():
        raise SystemExit("TASK6861_SETUP the copies must not build into the lane's own target dir")

    before = {rel: digest(ROOT / rel) for rel in TREE_FILES}
    failures: list[str] = []
    started = time.time()

    for mutation in selected:
        copy = copies / mutation.name
        notes = prepare(copy, mutation, args.starve)
        integrity = check_integrity(copy)
        code, stdout, stderr = run_pipeline(copy)
        output = stdout + stderr
        present, absent = named(output, mutation.must_name)
        leaked, _ = named(output, mutation.must_not_name)
        discarded = discard(copy)

        verdict = []
        if code != 1:
            verdict.append(f"exit {code}, expected 1")
        if absent:
            verdict.append(f"did not name {absent}")
        if leaked:
            verdict.append(f"named an unrelated boundary {leaked}")
        if integrity:
            verdict.append(integrity)
        if not discarded:
            verdict.append("copy was not discarded")

        print(
            f"TASK6861_MUTANT name={mutation.name} category={mutation.category} "
            f"exit={code} named={len(present)}/{len(mutation.must_name)} "
            f"unrelated={len(leaked)} copy={'discarded' if discarded else 'LEFT-BEHIND'}"
            f"{' ' + '; '.join(notes) if notes else ''}"
            f"{' FAIL: ' + '; '.join(verdict) if verdict else ''}"
        )
        print(f"    starves: {mutation.starves}")
        for needle in present:
            print(f"    named: TASK6860 FAIL {needle}")
        if args.show_red:
            for line in stderr.strip().splitlines():
                print(f"    red: {line[:220]}")
        if integrity:
            print(f"    observer: {integrity}")
        if verdict:
            failures.append(f"{mutation.name}: {'; '.join(verdict)}")

    if not args.only or args.starve == "restoration":
        run_restored(copies, args.starve, failures)

    after = {rel: digest(ROOT / rel) for rel in TREE_FILES}
    for rel in TREE_FILES:
        state = "unchanged" if before[rel] == after[rel] else "CHANGED"
        print(f"TASK6861_TREE file={rel} sha256={after[rel][:16]} {state}")
        if before[rel] != after[rel]:
            failures.append(f"tree: {rel} was edited by the proof")

    leftovers = sorted(p.name for p in copies.iterdir()) if copies.exists() else []
    print(f"TASK6861_COPIES root={copies} left_behind={leftovers}")
    if leftovers:
        failures.append(f"copies left behind: {leftovers}")

    covered = sorted({m.category for m in selected})
    elapsed = int(time.time() - started)

    if not args.starve and not args.no_self_test and not args.only:
        for kind, only in (
            ("mutant", "retain-receipt-while-off"),
            ("observer", "retain-receipt-while-off"),
            ("restoration", "retain-receipt-while-off"),
        ):
            sub = subprocess.run(
                [
                    sys.executable, str(Path(__file__).resolve()),
                    "--workdir", str(args.workdir / f"selftest-{kind}"),
                    "--starve", kind, "--only", only,
                ],
                capture_output=True, text=True,
            )
            reason = ""
            for line in (sub.stdout + sub.stderr).splitlines():
                if "FAIL" in line and line.startswith("TASK6861"):
                    reason = line.strip()
                    break
            print(f"TASK6861_SELFTEST starve={kind} exit={sub.returncode} {reason}")
            if sub.returncode == 0:
                failures.append(f"selftest: starving the {kind} did not fail the proof")
            shutil.rmtree(args.workdir / f"selftest-{kind}", ignore_errors=True)

    if failures:
        for detail in failures:
            print(f"TASK6861 FAIL {detail}", file=sys.stderr)
        print(f"TASK6861 FAIL failures={len(failures)}", file=sys.stderr)
        return 1

    print(
        f"TASK6861 PASS mutations={len(selected)} categories={len(covered)} "
        f"({','.join(covered)}) copies_discarded={len(selected) + (0 if args.only else 1)} "
        f"seconds={elapsed}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
