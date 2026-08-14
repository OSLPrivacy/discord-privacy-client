#!/usr/bin/env python3
"""Build the TASK 6987 dropped-integration ledger.

The deferred-edit note is deliberately an input, not an authority: every
record is joined to the blob in the named lane and to the comparable blob on
the integration side.  The result is JSON so a subsequent integration can
query it instead of re-reading a hand written merge diary.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any


LANE_RE = re.compile(r"\b(lane/[a-z])\b", re.I)
PATH_RE = re.compile(r"((?:apps/(?:osl-hub-ui|osl-hub)/src|[A-Za-z0-9_.-]+)[A-Za-z0-9_./-]+\.(?:ts|tsx|rs))")
COUNT_RE = re.compile(r"\b([0-9][0-9,]*)\s+lines?\b", re.I)
MERGE_DISPOSITION_RE = re.compile(r"\b(graft(?:ed)?|keep\s+RC|union)\b", re.I)
TASK_RE = re.compile(r"\[x\][^\n]*?\bTASK\s*([0-9]{3,6})\b", re.I)
TOP_TS = re.compile(
    r"^(?:export\s+)?(?:declare\s+)?(?:async\s+)?(?:function|const|class|type|interface)\s+([A-Za-z_$][\w$]*)\b"
)
TOP_RS = re.compile(
    r"^(?:pub\s+)?(?:async\s+)?(?:fn|const|struct|enum|trait|type)\s+([A-Za-z_][\w]*)\b"
)
IDENT_RE = re.compile(r"\b[A-Za-z_$][\w$]*\b")
CALL_RE = re.compile(r"\b([A-Za-z_$][\w$]*)\s*(?:<[^\n>()]{0,100}>)?\(")


class LedgerError(RuntimeError):
    pass


def run_git(*args: str, text: bool = True, check: bool = True) -> str:
    completed = subprocess.run(["git", *args], text=text, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE)
    if check and completed.returncode:
        raise LedgerError(completed.stderr.strip() or "git " + " ".join(args))
    return completed.stdout if text else completed.stdout.decode("utf-8", "replace")


def required_file(raw: str, label: str) -> Path:
    path = Path(raw)
    if not path.is_file():
        raise LedgerError(f"TASK 6987 starved {label}: {path}")
    return path


def git_ref(ref: str, label: str) -> None:
    try:
        run_git("rev-parse", "--verify", f"{ref}^{{commit}}")
    except LedgerError as exc:
        raise LedgerError(f"TASK 6987 starved {label}: {ref}") from exc


def git_tree(ref: str, path: str, label: str) -> None:
    if subprocess.run(["git", "cat-file", "-e", f"{ref}:{path}"], stdout=subprocess.DEVNULL,
                      stderr=subprocess.DEVNULL).returncode:
        # git cat-file -e is deliberately silent; the explicit label is the
        # useful failure in a red proof.
        raise LedgerError(f"TASK 6987 starved {label}: {ref}:{path}")


def normalise_merge_disposition(value: str) -> str:
    value = value.lower().replace("  ", " ")
    if value.startswith("graft"):
        return "graft"
    if value.startswith("keep"):
        return "keep RC"
    if value == "union":
        return "union"
    raise LedgerError(f"TASK 6987 unknown deferred disposition: {value}")


def parse_deferred_log(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        lane = LANE_RE.search(raw)
        if not lane:
            continue
        source_path = PATH_RE.search(raw)
        count = COUNT_RE.search(raw)
        disposition = MERGE_DISPOSITION_RE.search(raw)
        if not (source_path and count and disposition):
            raise LedgerError(f"TASK 6987 malformed deferred row line {number}: {raw}")
        rows.append({
            "lane": lane.group(1).lower(),
            "path": source_path.group(1),
            "line_count": int(count.group(1).replace(",", "")),
            "merge_disposition": normalise_merge_disposition(disposition.group(1)),
            "log_line": number,
        })
    if not rows:
        raise LedgerError("TASK 6987 starved deferred log: it contains no deferred rows")
    return rows


def blob(ref: str, source_path: str, label: str) -> dict[str, Any]:
    spec = f"{ref}:{source_path}"
    oid = run_git("rev-parse", "--verify", spec, check=False).strip()
    if not oid:
        raise LedgerError(f"TASK 6987 starved object lookup: {spec} ({label})")
    content = run_git("show", spec, check=False)
    if content == "":
        # Empty files are valid, but an absent object is not.  cat-file settles
        # that distinction without ever reading from the working tree.
        kind = run_git("cat-file", "-t", oid, check=False).strip()
        if kind != "blob":
            raise LedgerError(f"TASK 6987 starved object lookup: {spec} ({label})")
    return {"ref": ref, "object": oid, "sha256": hashlib.sha256(content.encode()).hexdigest(),
            "line_count": len(content.splitlines()), "content": content}


def changed_lines(base: str, lane: str, source_path: str) -> list[str]:
    """Added lane-side lines, calculated from objects; never from the diary."""
    diff = run_git("diff", "--unified=0", base, lane, "--", source_path, check=False)
    return [line[1:] for line in diff.splitlines()
            if line.startswith("+") and not line.startswith("+++")]


def collect_deferred(rows: list[dict[str, Any]], integration: str) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for row in rows:
        git_ref(row["lane"], "lane branch")
        base = run_git("merge-base", row["lane"], integration, check=False).strip()
        if not base:
            raise LedgerError(f"TASK 6987 starved object lookup: no merge base for {row['lane']} and {integration}")
        lane_blob = blob(row["lane"], row["path"], "lane side")
        base_blob = blob(base, row["path"], "integration-side ancestor")
        excerpt = changed_lines(base, row["lane"], row["path"])
        result.append({**row, "object_base": base, "lane_blob": lane_blob,
                       "integration_base_blob": base_blob,
                       "discarded_content": "\n".join(excerpt) + ("\n" if excerpt else ""),
                       "discarded_content_line_count": len(excerpt)})
    return result


def corpus_lines(ref: str, patterns: list[str] | None = None):
    patterns = patterns or ["."]
    arguments = ["grep", "-n", "-I"]
    for pattern in patterns:
        arguments.extend(("-e", pattern))
    arguments.extend((ref, "--", "apps/osl-hub-ui/src", "apps/osl-hub/src"))
    corpus = run_git(*arguments)
    prefix = re.compile(rf"^{re.escape(ref)}:(.+):(\d+):(.*)$")
    for raw in corpus.splitlines():
        match = prefix.match(raw)
        if not match:
            continue
        source_path, line_no_text, line = match.groups()
        # The census is for shipping sources; colocated test fixtures would
        # otherwise drown out the integration's product symbols.
        if not source_path.endswith((".ts", ".tsx", ".rs")) or ".test." in source_path:
            continue
        yield source_path, int(line_no_text), line


def definitions_for(ref: str) -> tuple[list[dict[str, Any]], dict[tuple[str, str], set[int]], dict[str, list[dict[str, Any]]]]:
    definitions: list[dict[str, Any]] = []
    definition_lines: dict[tuple[str, str], set[int]] = defaultdict(set)
    calls: dict[str, list[dict[str, Any]]] = defaultdict(list)
    # One Git process per ref is intentional. A per-file `git show` turns the
    # 27-ref census into tens of thousands of processes and makes it unusable.
    for source_path, line_no, line in corpus_lines(ref):
        matcher = TOP_RS if source_path.endswith(".rs") else TOP_TS
        found = matcher.match(line)
        defined_here = found.group(1) if found else None
        if not line.lstrip().startswith("import "):
            for called in CALL_RE.findall(line):
                if called == defined_here:
                    continue
                calls[called].append({"ref": ref, "path": source_path, "line": line_no})
        if found:
            name = found.group(1)
            record = {"symbol": name, "ref": ref, "path": source_path, "line": line_no,
                      "language": "rust" if source_path.endswith(".rs") else "typescript"}
            definitions.append(record)
            definition_lines[(source_path, name)].add(line_no)
    return definitions, definition_lines, calls


def census(refs: list[str]) -> dict[str, Any]:
    all_definitions: list[dict[str, Any]] = []
    all_calls: dict[str, list[dict[str, Any]]] = defaultdict(list)
    definition_keys: set[str] = set()
    definition_lines_by_ref: dict[str, dict[tuple[str, str], set[int]]] = {}
    observed_calls: dict[str, list[dict[str, Any]]] = defaultdict(list)

    def initial(ref: str):
        defs, def_lines, calls = definitions_for(ref)
        return ref, defs, def_lines, calls

    # The refs are independent objects. Bounded parallel reads keep a complete
    # 27-branch census practical without weakening either census direction.
    with ThreadPoolExecutor(max_workers=min(8, len(refs))) as pool:
        initial_rows = list(pool.map(initial, refs))
    for ref, defs, def_lines, calls in initial_rows:
        all_definitions.extend(defs)
        definition_keys.update(item["symbol"] for item in defs)
        definition_lines_by_ref[ref] = def_lines
        for name, locations in calls.items():
            all_calls[name].extend(locations)
            observed_calls[name].extend(locations)

    # Definitions themselves are always candidates. Function-style calls and
    # named imports make missing definitions visible too (including a dangling
    # import such as offlineCapabilitiesMarkup), without dumping every English
    # word from strings and comments into the ledger.
    # A missing local call normally has a definition on another lane; retain
    # those global names.  The suffix set also catches locally-shaped markup
    # calls that have been dropped from every scanned ref, without mislabelling
    # the JavaScript/Rust standard libraries as integration damage.
    candidates = definition_keys | {
        name for name in observed_calls
        if name.endswith(("Markup", "Open", "Header"))
    }
    all_calls = defaultdict(list, {name: locations for name, locations in all_calls.items() if name in candidates})
    definitions_by_symbol: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for item in all_definitions:
        definitions_by_symbol[item["symbol"]].append(item)
    symbol_rows = []
    for name in sorted(candidates):
        definitions = definitions_by_symbol.get(name, [])
        calls = all_calls.get(name, [])
        symbol_rows.append({"symbol": name, "definitions": definitions, "calls": calls,
                            "definition_count": len(definitions), "call_count": len(calls)})
    # Defining a symbol on another lane does not repair the integration branch.
    # Classify these per ref, while retaining the cross-ref locations above.
    missing: list[dict[str, Any]] = []
    uncalled: list[dict[str, Any]] = []
    duplicates: list[dict[str, Any]] = []
    # The complete symbol table carries every ref. Damage classifications are
    # deliberately evaluated on the named integration target: a lane's local
    # definition must not hide a missing integration definition.
    for ref in [refs[-1]]:
        for item in symbol_rows:
            definitions = [entry for entry in item["definitions"] if entry["ref"] == ref]
            calls = [entry for entry in item["calls"] if entry["ref"] == ref]
            if calls and not definitions:
                missing.append({"symbol": item["symbol"], "ref": ref, "definitions": [], "calls": calls})
            if definitions and not calls:
                uncalled.append({"symbol": item["symbol"], "ref": ref, "definitions": definitions, "calls": []})
            if len(definitions) > 1:
                duplicates.append({"symbol": item["symbol"], "ref": ref, "definitions": definitions, "calls": calls})
    return {"refs": refs, "symbols": symbol_rows, "calls_without_definition": missing,
            "definitions_without_call": uncalled, "duplicate_definitions": duplicates}


def parse_rules(path: Path) -> dict[str, Any]:
    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise LedgerError(f"TASK 6987 malformed disposition rule: {exc}") from exc
    return parsed.get("tasks", parsed) if isinstance(parsed, dict) else {}


def task_ids(path: Path) -> list[str]:
    values = sorted(set(TASK_RE.findall(path.read_text(encoding="utf-8"))))
    if not values:
        raise LedgerError("TASK 6987 starved reachability input: no ticked TASK ids")
    return values


def commits_for_task(task: str) -> list[str]:
    return [commit for commit in run_git("log", "--all", "--format=%H", "--grep", rf"\bTASK[[:space:]]*{task}\b", "-i").splitlines() if commit]


def reaches(commit: str, integration: str) -> bool:
    return subprocess.run(["git", "merge-base", "--is-ancestor", commit, integration]).returncode == 0


def reachability(path: Path, rules_path: Path, integration: str) -> list[dict[str, Any]]:
    rules = parse_rules(rules_path)
    rows: list[dict[str, Any]] = []
    for task in task_ids(path):
        commits = commits_for_task(task)
        pending = [commit for commit in commits if not reaches(commit, integration)]
        if not pending:
            continue
        rule = rules.get(task)
        if not isinstance(rule, dict) or rule.get("disposition") not in {"restored", "superseded", "open"}:
            raise LedgerError(f"TASK 6987 disposition rule missing/unknown for TASK {task}")
        disposition = rule["disposition"]
        replacement = rule.get("replacement")
        if disposition == "superseded" and not isinstance(replacement, str):
            raise LedgerError(f"TASK 6987 superseded TASK {task} has no replacement")
        if disposition != "superseded" and replacement is not None:
            raise LedgerError(f"TASK 6987 {disposition} TASK {task} must not name a replacement")
        for commit in pending:
            refs = run_git("branch", "--contains", commit, "--format=%(refname:short)").splitlines()
            rows.append({"task": task, "commit": commit, "refs": refs, "disposition": disposition,
                         "replacement": replacement})
    if any(item["disposition"] not in {"restored", "superseded", "open"} for item in rows):
        raise LedgerError("TASK 6987 blank or unknown reachability disposition")
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--deferred-log", required=True)
    parser.add_argument("--reachability-input", required=True)
    parser.add_argument("--disposition-rules", required=True)
    parser.add_argument("--integration", required=True)
    parser.add_argument("--rc", required=True)
    parser.add_argument("--lanes", nargs="+", required=True)
    parser.add_argument("--census-ui-root", default="apps/osl-hub-ui/src")
    parser.add_argument("--census-hub-root", default="apps/osl-hub/src")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    try:
        deferred_log = required_file(args.deferred_log, "deferred log")
        reachability_input = required_file(args.reachability_input, "reachability input")
        disposition_rules = required_file(args.disposition_rules, "disposition rule")
        git_ref(args.integration, "integration branch")
        git_ref(args.rc, "rc branch")
        git_tree(args.integration, args.census_ui_root, "census UI direction")
        git_tree(args.integration, args.census_hub_root, "census hub direction")
        expected_lanes = {f"lane/{letter}" for letter in "bcdefghijklmnopqrstuvwxyz"}
        if set(args.lanes) != expected_lanes or len(args.lanes) != len(expected_lanes):
            missing = ",".join(sorted(expected_lanes - set(args.lanes))) or "unexpected lane list"
            raise LedgerError(f"TASK 6987 starved lane branch list: {missing}")
        for lane in args.lanes:
            git_ref(lane, "lane branch")
        deferred = collect_deferred(parse_deferred_log(deferred_log), args.integration)
        report = {
            "task": 6987,
            "integration": args.integration,
            "rc": args.rc,
            "deferred": deferred,
            "deferred_summary": {"rows": len(deferred), "lines": sum(item["line_count"] for item in deferred),
                                 "merge_dispositions": dict(Counter(item["merge_disposition"] for item in deferred))},
            "census": census([*args.lanes, args.rc, args.integration]),
            "reachability": reachability(reachability_input, disposition_rules, args.integration),
        }
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("w", encoding="utf-8") as handle:
            json.dump(report, handle, indent=2, sort_keys=True)
            handle.write("\n")
        print("TASK 6987 ledger " + json.dumps({
            "rows": report["deferred_summary"]["rows"], "lines": report["deferred_summary"]["lines"],
            "merge_dispositions": report["deferred_summary"]["merge_dispositions"],
            "reachability_open": sum(row["disposition"] == "open" for row in report["reachability"]),
        }, sort_keys=True))
        return 0
    except LedgerError as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
