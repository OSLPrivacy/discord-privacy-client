#!/usr/bin/env python3
"""Validate the change-time state-authority registry.

The registry intentionally uses JSON in a Markdown fence: it is easy to review,
requires no Python dependency, and gives CI an unambiguous contract to enforce.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REGISTRY = ROOT / "docs/engineering/state-authority-registry.md"
RECORD_FENCE = re.compile(r"```json\s*\n(.*?)\n```", re.DOTALL)
# These are deliberately narrow write/creation operations.  The gate examines
# added lines only, so ordinary edits to existing stateful files do not require
# a registry amendment.
STATEFUL_CHANGE = re.compile(
    r"\b(localStorage\.(?:setItem|removeItem)|sessionStorage\.(?:setItem|removeItem)|"
    r"(?:writeFile|write_text|write_all|save|persist)\s*\(|INSERT\s+INTO|UPDATE\s+\w+\s+SET|"
    r"new\s+(?:Shared)?Worker\s*\(|(?:tokio::)?spawn\s*\()",
    re.IGNORECASE,
)
REQUIRED_LIFECYCLE = {
    "serialization", "migration", "default", "downgrade", "startup", "restart",
    "crash", "account_switch", "teardown",
}
REQUIRED_TOP_LEVEL = {
    "id", "datum", "kind", "authority", "readers", "writers", "resets", "graphs",
    "lifecycle", "reuse", "conflicts", "paths",
}
# `github.event.before` is 40 zeros on a branch's first push.  Any all-zero sha
# in a range means the range cannot resolve to a real base.
ZERO_SHA = re.compile(r"\b0{7,40}\b")
# Cap the refusal listing so an ungradable run stays a readable diagnosis and
# does not turn into a wall of every stateful file in the repository.
REFUSAL_SAMPLE = 20


def load_records(path: Path) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8")
    matches = RECORD_FENCE.findall(text)
    if len(matches) != 1:
        raise ValueError(f"{path}: expected exactly one JSON registry fence")
    payload = json.loads(matches[0])
    if not isinstance(payload, dict) or not isinstance(payload.get("records"), list):
        raise ValueError(f"{path}: registry JSON must contain a records array")
    if not all(isinstance(record, dict) for record in payload["records"]):
        raise ValueError(f"{path}: every registry record must be an object")
    return payload["records"]


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def validate(records: list[dict[str, Any]], changed_paths: set[str] | None = None) -> list[str]:
    errors: list[str] = []
    ids: set[str] = set()
    declared_paths: set[str] = set()
    for index, record in enumerate(records, start=1):
        label = str(record.get("id", f"record #{index}"))
        missing = REQUIRED_TOP_LEVEL - record.keys()
        if missing:
            errors.append(f"{label}: missing required fields: {', '.join(sorted(missing))}")
            continue
        if not nonempty(record["id"]) or not re.fullmatch(r"[a-z0-9][a-z0-9-]*", record["id"]):
            errors.append(f"{label}: id must be lowercase kebab-case")
        elif record["id"] in ids:
            errors.append(f"{label}: duplicate registry id")
        ids.add(record.get("id", ""))
        if record.get("kind") not in {"persisted-state", "flag", "worker"}:
            errors.append(f"{label}: kind must be persisted-state, flag, or worker")
        authority = record.get("authority")
        if not isinstance(authority, dict) or not all(nonempty(authority.get(k)) for k in ("owner", "path", "reason")):
            errors.append(f"{label}: authority needs owner, path, and reason")
        readers, writers, resets = record.get("readers"), record.get("writers"), record.get("resets")
        for field, value in (("readers", readers), ("writers", writers), ("resets", resets)):
            if not isinstance(value, list) or not value or not all(isinstance(item, dict) and nonempty(item.get("actor")) and nonempty(item.get("path")) for item in value):
                errors.append(f"{label}: {field} must be a non-empty inventory of actor/path entries")
        if isinstance(writers, list) and len(writers) != 1:
            errors.append(f"{label}: exactly one writer is allowed; route other mutations through its authority")
        graphs = record.get("graphs")
        if not isinstance(graphs, dict) or not all(nonempty(graphs.get(k)) for k in ("dependency", "interaction")):
            errors.append(f"{label}: graphs needs dependency and interaction placements")
        lifecycle = record.get("lifecycle")
        if not isinstance(lifecycle, dict):
            errors.append(f"{label}: lifecycle must be an object")
        else:
            absent = [key for key in REQUIRED_LIFECYCLE if not nonempty(lifecycle.get(key))]
            if absent:
                errors.append(f"{label}: lifecycle missing: {', '.join(sorted(absent))}")
        for field in ("reuse", "conflicts"):
            if not nonempty(record.get(field)):
                errors.append(f"{label}: {field} must contain an explicit review note")
        paths = record.get("paths")
        if not isinstance(paths, list) or not paths or not all(nonempty(path) for path in paths):
            errors.append(f"{label}: paths must identify every implementing source file")
        else:
            declared_paths.update(paths)
        mirrors = record.get("mirrors", [])
        if mirrors:
            if not isinstance(mirrors, list) or len(mirrors) < 2:
                errors.append(f"{label}: mirrors must enumerate at least two surfaces")
            else:
                for mirror in mirrors:
                    if not isinstance(mirror, dict) or not all(nonempty(mirror.get(k)) for k in ("surface", "path", "drift_assertion")):
                        errors.append(f"{label}: every mirror requires surface, path, and drift_assertion")
    if changed_paths:
        unregistered = changed_paths - declared_paths
        if unregistered:
            errors.append("new persisted state/worker has no registry path: " + ", ".join(sorted(unregistered)))
    return errors


def declared_paths(records: list[dict[str, Any]]) -> set[str]:
    """Every source path any record claims to implement."""
    paths: set[str] = set()
    for record in records:
        value = record.get("paths")
        if isinstance(value, list):
            paths.update(path for path in value if nonempty(path))
    return paths


def degenerate_range(git_range: str | None) -> str:
    """Say why `git_range` cannot grade anything, or "" when it can.

    The workflow substitutes `github.event.before`, which is all zeros on a
    branch's first push.  Treating that as "nothing to check" is what made this
    gate pass vacuously, so every unusable shape is named here instead.
    """
    if git_range is None:
        return "no --git-range was supplied, so no added line was compared against the registry"
    text = git_range.strip()
    if not text:
        return "--git-range was empty, so no added line was compared against the registry"
    if ZERO_SHA.search(text):
        return (
            f"--git-range {git_range!r} names an all-zero sha (unborn base / first push of a branch), "
            "so no added line was compared against the registry"
        )
    return ""


def tracked_stateful_files(root: Path) -> set[str]:
    """Tracked files containing a stateful construct, by the same regex the gate grades with.

    This is a *predicate*, not a second gate: it only answers "is there stateful
    input this run failed to grade?" and supplies a bounded sample for the
    refusal message.  It never fails an individual file.
    """
    result = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    if result.returncode:
        raise ValueError(f"cannot census tracked files: {result.stderr.strip()}")
    hits: set[str] = set()
    for name in result.stdout.split("\0"):
        if not name:
            continue
        try:
            text = (root / name).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if STATEFUL_CHANGE.search(text):
            hits.add(name)
    return hits


def added_stateful_paths(git_range: str, root: Path) -> set[str]:
    result = subprocess.run(
        ["git", "diff", "--unified=0", git_range, "--"], cwd=root, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    if result.returncode:
        raise ValueError(f"cannot inspect git range {git_range}: {result.stderr.strip()}")
    current: str | None = None
    paths: set[str] = set()
    for line in result.stdout.splitlines():
        if line.startswith("+++ b/"):
            current = line[6:]
        elif current and line.startswith("+") and not line.startswith("+++") and STATEFUL_CHANGE.search(line[1:]):
            paths.add(current)
    return paths


def main() -> int:
    parser = argparse.ArgumentParser(description="check single-authority state contracts")
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    parser.add_argument("--git-range", help="validate stateful added lines in this git revision range")
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    try:
        records = load_records(args.registry)
        unusable = degenerate_range(args.git_range)
        if unusable:
            # D-247: refuse instead of reporting success on a run that compared
            # nothing.  Bounded on purpose -- it only refuses when there IS
            # stateful input it could not grade, and it names that input.
            census = tracked_stateful_files(args.root)
            if census:
                return refuse(unusable, records, census)
            # Genuinely nothing stateful in the tree: the registry-only check is
            # the whole contract and passing it is a real result.
        changed = added_stateful_paths(args.git_range, args.root) if not unusable else None
        errors = validate(records, changed)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"state-authority: {exc}", file=sys.stderr)
        return 2
    if errors:
        print("state-authority check failed:", file=sys.stderr)
        print("\n".join(f"- {error}" for error in errors), file=sys.stderr)
        return 1
    if changed is None:
        print(f"state-authority: {len(records)} record(s) validated; no stateful construct exists in the tree")
    else:
        if not declared_paths(records):
            print(
                "state-authority: WARNING -- the registry declares 0 paths. This range added no "
                "stateful line, so there was nothing to grade against it, but no state authority "
                "has ever been declared.",
                file=sys.stderr,
            )
        print(
            f"state-authority: {len(records)} record(s) validated; graded {args.git_range} "
            f"({len(changed)} changed file(s) with added stateful lines)"
        )
    return 0


def refuse(unusable: str, records: list[dict[str, Any]], census: set[str]) -> int:
    """Fail closed on an ungradable run, naming exactly what went ungraded."""
    declared = declared_paths(records)
    ungraded = sorted(census - declared)
    print("state-authority: REFUSING -- this run could not grade anything.", file=sys.stderr)
    print(f"- {unusable}", file=sys.stderr)
    if not declared:
        print(
            f"- the registry declares 0 paths across {len(records)} record(s): no state authority "
            "has ever been declared, so there is nothing for a change to be graded against",
            file=sys.stderr,
        )
    print(
        f"- {len(census)} tracked file(s) contain a stateful construct "
        f"(storage write, worker creation, spawn, or SQL mutation); "
        f"{len(ungraded)} of them are not covered by any registry path. None of them were graded.",
        file=sys.stderr,
    )
    for path in ungraded[:REFUSAL_SAMPLE]:
        print(f"    ungraded: {path}", file=sys.stderr)
    if len(ungraded) > REFUSAL_SAMPLE:
        print(f"    ... and {len(ungraded) - REFUSAL_SAMPLE} more (listing capped at {REFUSAL_SAMPLE})", file=sys.stderr)
    print(
        "This refusal clears as soon as the gate can grade again. Two independent remedies, "
        "either of which is enough:\n"
        "  1. Give it a real range: --git-range <base>...<head> with a resolvable base "
        "(on a first push use the merge-base with the default branch, not github.event.before).\n"
        "  2. Declare the state: add records to docs/engineering/state-authority-registry.md so "
        "the stateful paths above are covered.\n"
        "Do NOT satisfy this by narrowing STATEFUL_CHANGE or by adding a skip: that restores the "
        "vacuous pass this refusal exists to remove.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
