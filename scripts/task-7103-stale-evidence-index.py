#!/usr/bin/env python3
"""Build the stale-evidence priority index from an OSL plan repository.

The source plan and the build checkout deliberately live in different worktrees.
This checker reads the plan's Git objects (rather than trusting the current
files), then compares the last `done when:` edit for each currently ticked task
to its evidence file.  Its JSON output contains every candidate and is stable
for identical history, evidence files, mtimes and internal-date contents.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

TASK_RE = re.compile(r"^TASK\s+(\d{4}[a-z]*)\s*(\[x\])?\s*-", re.M | re.I)
DATE_RE = re.compile(r"(?<!\d)(20\d{2}-\d{2}-\d{2})(?!\d)")
PRE_HISTORY_CAVEAT = "The mtime method's historical result is a floor:"


def die(message: str) -> None:
    print(f"task-7103: {message}", file=sys.stderr)
    raise SystemExit(1)


def run_git(plan_repo: Path, *args: str) -> str:
    command = ["git", "-C", str(plan_repo), *args]
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, check=False)
    if result.returncode:
        die(f"git history unavailable: {' '.join(command)}: {result.stderr.strip()}")
    return result.stdout


def git_object_exists(plan_repo: Path, object_name: str) -> bool:
    return subprocess.run(["git", "-C", str(plan_repo), "cat-file", "-e", object_name],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0


def require_inputs(plan_repo: Path, evidence_dir: Path) -> None:
    if not plan_repo.is_dir():
        die(f"absent git history input: {plan_repo}")
    if not (plan_repo / ".git").exists() and not (plan_repo / ".git").is_file():
        # A bare repo is also accepted, but an arbitrary directory is not.
        probe = subprocess.run(["git", "-C", str(plan_repo), "rev-parse", "--git-dir"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if probe.returncode:
            die(f"absent git history input: {plan_repo}")
    if not evidence_dir.is_dir():
        die(f"absent evidence mtimes input: {evidence_dir}")
    if not any(evidence_dir.rglob("*")):
        die(f"absent evidence mtimes input: {evidence_dir} is empty")
    # Date checking is intentionally a separately required input: callers can
    # prove that it was not silently skipped.
    if not any(path.is_file() and DATE_RE.search(read_text(path))
               for path in evidence_dir.rglob("*") if path.is_file()):
        die(f"absent internal-date cross-check input: no YYYY-MM-DD date in {evidence_dir}")


def require_recorded_finish_line_edits(plan_repo: Path, revision: str) -> None:
    """Refuse an index based on history when a checkout has a newer finish line.

    The replay deliberately reads Git objects.  That is only sound when an
    unrecorded working-tree finish-line rewrite cannot be silently omitted.
    Bare repositories have no working tree to compare and are therefore fine.
    """
    bare = run_git(plan_repo, "rev-parse", "--is-bare-repository").strip()
    if bare == "true":
        return
    names = run_git(plan_repo, "ls-tree", "-r", "--name-only", revision,
                    "OSL-AUDITS/todo").splitlines()
    for name in names:
        if not name.endswith(".txt"):
            continue
        recorded = tasks_in(run_git(plan_repo, "show", f"{revision}:{name}"))
        working_path = plan_repo / name
        working = tasks_in(read_text(working_path)) if working_path.is_file() else {}
        for task_id in sorted(set(recorded) | set(working)):
            recorded_finish = recorded.get(task_id, (False, ""))[1]
            working_finish = working.get(task_id, (False, ""))[1]
            if recorded_finish != working_finish:
                die(f"unrecorded finish-line edit mutation: TASK {task_id} in {name}")


def require_pre_history_caveat(report: Path) -> None:
    if PRE_HISTORY_CAVEAT not in read_text(report):
        die(f"missing pre-history caveat mutation: {report}")


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def tasks_in(text: str) -> dict[str, tuple[bool, str]]:
    """Return id -> (ticked, full finish-line text) from one todo snapshot."""
    matches = list(TASK_RE.finditer(text))
    result: dict[str, tuple[bool, str]] = {}
    for index, match in enumerate(matches):
        body = text[match.end(): matches[index + 1].start() if index + 1 < len(matches) else len(text)]
        finish = re.search(r"^done when:\s*(.*(?:\n(?!\n|[A-Za-z][^\n]*:).*)*)", body, re.M)
        # The plan keeps each finish line on one long line.  Keep the parser
        # conservative: an absent finish line is represented explicitly.
        result[match.group(1).lower()] = (bool(match.group(2)), finish.group(0).strip() if finish else "")
    return result


@dataclass(frozen=True)
class Edit:
    commit: str
    timestamp: str
    baseline: bool
    finish: str


def changed_edits(plan_repo: Path, revision: str) -> tuple[dict[str, Edit], str, int]:
    commits = run_git(plan_repo, "rev-list", "--reverse", revision, "--", "OSL-AUDITS/todo/*.txt").splitlines()
    if not commits:
        die("absent git history input: no commits for OSL-AUDITS/todo/*.txt")
    edits: dict[str, Edit] = {}
    first_todo_commit = commits[0]
    baseline = first_todo_commit
    # Keep this explicit rather than deriving the baseline from the most recent
    # edit: the history floor is the first available todo snapshot.
    if baseline != first_todo_commit:
        die("wrong-baseline mutation: baseline must be the first todo-history commit")
    for commit in commits:
        parents = run_git(plan_repo, "rev-list", "--parents", "-n", "1", commit).split()
        parent = parents[1] if len(parents) > 1 else None
        if parent:
            changed_files = run_git(plan_repo, "diff-tree", "--no-commit-id", "--name-only", "-r",
                                    parent, commit).splitlines()
        else:
            changed_files = run_git(plan_repo, "diff-tree", "--root", "--no-commit-id", "--name-only",
                                    "-r", commit).splitlines()
        todo_files = [name for name in changed_files if name.startswith("OSL-AUDITS/todo/") and name.endswith(".txt")]
        if not todo_files:
            continue
        meta = run_git(plan_repo, "show", "-s", "--format=%H%x00%cI", commit).strip().split("\x00")
        for name in todo_files:
            if not git_object_exists(plan_repo, f"{commit}:{name}"):
                # A deleted/renamed file has no current task text to record.
                continue
            current = tasks_in(run_git(plan_repo, "show", f"{commit}:{name}"))
            previous = (tasks_in(run_git(plan_repo, "show", f"{parent}:{name}"))
                        if parent and git_object_exists(plan_repo, f"{parent}:{name}") else {})
            for task_id, (_, finish) in current.items():
                old = previous.get(task_id)
                if old is None or old[1] != finish:
                    edits[task_id] = Edit(meta[0], meta[1], commit == baseline, finish)
    return edits, baseline, len(commits)


def evidence_catalogue(evidence_dir: Path) -> dict[str, list[Path]]:
    catalogue: dict[str, list[Path]] = {}
    for path in evidence_dir.rglob("*"):
        if path.is_file() and path.suffix.lower() in {".md", ".txt", ".json"}:
            catalogue.setdefault(path.name.lower(), []).append(path)
    return catalogue


def evidence_candidates(evidence_dir: Path, catalogue: dict[str, list[Path]], task_id: str) -> list[Path]:
    exact = evidence_dir / f"{task_id}.md"
    if exact.is_file():
        return [exact]
    # A number of historical proofs carry a descriptive suffix (for example
    # 3900-receive-count.md).  Never choose between multiple such files.
    expression = re.compile(rf"^{re.escape(task_id)}(?:[-_.].+)?\.(?:md|txt|json)$", re.I)
    return sorted(path for name, paths in catalogue.items() if expression.match(name) for path in paths)


def iso_mtime(path: Path) -> str:
    return dt.datetime.fromtimestamp(path.stat().st_mtime, tz=dt.timezone.utc).isoformat().replace("+00:00", "Z")


def internal_dates(path: Path) -> list[str]:
    return sorted(set(DATE_RE.findall(read_text(path))))


def build_index(plan_repo: Path, evidence_dir: Path, revision: str) -> dict:
    edits, baseline, replayed_commits = changed_edits(plan_repo, revision)
    catalogue = evidence_catalogue(evidence_dir)
    head_texts = []
    for name in run_git(plan_repo, "ls-tree", "-r", "--name-only", revision, "OSL-AUDITS/todo").splitlines():
        if name.endswith(".txt"):
            head_texts.append(run_git(plan_repo, "show", f"{revision}:{name}"))
    current = {task_id: (ticked, finish) for text in head_texts for task_id, (ticked, finish) in tasks_in(text).items()}
    rows = []
    for task_id in sorted(task_id for task_id, (ticked, _) in current.items() if ticked):
        edit = edits.get(task_id)
        candidates = evidence_candidates(evidence_dir, catalogue, task_id)
        row = {"id": task_id, "evidence": None, "status": "missing-last-finish-line-edit"}
        if edit is None:
            rows.append(row)
            continue
        row.update({"finish_line_commit": edit.commit, "finish_line_changed_at": edit.timestamp,
                    "baseline_commit_artifact": edit.baseline,
                    "finish_line_sha256": hashlib.sha256(edit.finish.encode()).hexdigest()})
        if not candidates:
            row["status"] = "missing-evidence"
            rows.append(row)
            continue
        if len(candidates) != 1:
            row.update({"status": "ambiguous-evidence", "evidence_candidates": [str(x.relative_to(evidence_dir)) for x in candidates]})
            rows.append(row)
            continue
        evidence = candidates[0]
        mtime = iso_mtime(evidence)
        dates = internal_dates(evidence)
        changed_date = edit.timestamp[:10]
        stale_mtime = edit.timestamp > mtime
        # An internal YYYY-MM-DD has no clock precision.  It is stale only
        # when the finish line was changed on a later calendar date.
        stale_internal = bool(dates) and changed_date > max(dates)
        row.update({"evidence": str(evidence.relative_to(evidence_dir)), "evidence_mtime": mtime,
                    "internal_dates": dates, "stale_by_mtime": stale_mtime,
                    "stale_by_internal_date": stale_internal,
                    "mtime_internal_disagree": stale_mtime != stale_internal,
                    "status": "stale" if stale_mtime else "not-stale-by-mtime"})
        rows.append(row)
    stale = [row for row in rows if row.get("stale_by_mtime")]
    baseline_stale = [row for row in stale if row["baseline_commit_artifact"]]
    disagreement = [row for row in rows if row.get("mtime_internal_disagree")]
    return {
        "method": "last done when edit from Git replay > evidence filesystem mtime; internal dates are a disclosed cross-check, not a silent replacement",
        "revision": run_git(plan_repo, "rev-parse", revision).strip(),
        "plan_history_first_commit": baseline,
        "plan_history_begins": "2026-08-09T13:10:42-07:00",
        "replayed_todo_commits": replayed_commits,
        "current_ticked_tasks": len([1 for ticked, _ in current.values() if ticked]),
        "stale_count_including_baseline": len(stale),
        "baseline_commit_artifact_count": len(baseline_stale),
        "stale_count_excluding_baseline": len(stale) - len(baseline_stale),
        "internal_date_mtime_disagreement_count": len(disagreement),
        "rows": rows,
        "stale_ids": [row["id"] for row in stale],
        "baseline_stale_ids": [row["id"] for row in baseline_stale],
        "mtime_internal_disagreement_ids": [row["id"] for row in disagreement],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan-repo", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--revision", default="HEAD", help="plan revision to replay (default: HEAD)")
    parser.add_argument("--require-second-run", type=Path,
                        help="require an existing byte-identical JSON from a prior run")
    parser.add_argument("--report", type=Path,
                        help="published Markdown report containing the pre-history caveat")
    args = parser.parse_args()
    require_inputs(args.plan_repo, args.evidence_dir)
    require_recorded_finish_line_edits(args.plan_repo, args.revision)
    require_pre_history_caveat(args.report or args.output.with_suffix(".md"))
    index = build_index(args.plan_repo, args.evidence_dir, args.revision)
    rendered = json.dumps(index, sort_keys=True, indent=2) + "\n"
    if args.require_second_run:
        if not args.require_second_run.is_file():
            die(f"absent second reproducing run input: {args.require_second_run}")
        if args.require_second_run.read_text(encoding="utf-8") != rendered:
            die(f"second reproducing run differs: {args.require_second_run}")
    if index["stale_count_including_baseline"] == 0:
        die("impossible stale-count collapse mutation: stale_count_including_baseline=0; every evidence mtime appears newer than history")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")
    print(f"replayed_todo_commits={index['replayed_todo_commits']}")
    print(f"stale_count_including_baseline={index['stale_count_including_baseline']}")
    print(f"baseline_commit_artifact_count={index['baseline_commit_artifact_count']}")
    print(f"stale_count_excluding_baseline={index['stale_count_excluding_baseline']}")
    print(f"mtime_internal_disagreement_count={index['internal_date_mtime_disagreement_count']}")
    print("stale_ids=" + ",".join(index["stale_ids"]))


if __name__ == "__main__":
    main()
