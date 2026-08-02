#!/usr/bin/env python3
"""Report Rust functions that are unreachable from shipped application entrypoints.

The census deliberately analyses an exported ``git archive HEAD`` rather than the
working directory.  That keeps the report reproducible and excludes local test
fixtures or uncommitted experiments from the measurement.
"""

from __future__ import annotations

import argparse
from bisect import bisect_right
import datetime as dt
import re
import shutil
import subprocess
import tarfile
import tempfile
from collections import defaultdict, deque
from dataclasses import dataclass
from pathlib import Path


FUNCTION = re.compile(
    r"(?m)^(?P<prefix>[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:async[ \t]+)?(?:unsafe[ \t]+)?)"
    r"fn[ \t]+(?P<name>[A-Za-z_][A-Za-z0-9_]*)[ \t]*(?:<[^>{}()]*>)?[ \t]*\(")
CALL = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*!?\s*\(")
TAURI_COMMAND = re.compile(r"#\s*\[\s*tauri::command(?:\s*\([^]]*\))?\s*\]")
IGNORED_CALLS = {
    "if", "while", "for", "loop", "match", "Some", "Ok", "Err", "Self",
    "String", "Vec", "Box", "Arc", "Mutex", "RwLock", "HashMap", "HashSet",
}


@dataclass(frozen=True, eq=False)
class Function:
    name: str
    path: Path
    line: int
    public: bool
    body: str
    tauri_command: bool

    @property
    def label(self) -> str:
        return f"{self.path}:{self.line}:{self.name}"


def _matching_brace(text: str, opening: int) -> int:
    depth = 0
    for index in range(opening, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    return len(text) - 1


def _brace_pairs(text: str) -> dict[int, int]:
    """Map every opening brace to its matching closing brace in one pass."""
    stack: list[int] = []
    pairs: dict[int, int] = {}
    for index, character in enumerate(text):
        if character == "{":
            stack.append(index)
        elif character == "}" and stack:
            pairs[stack.pop()] = index
    return pairs


def _without_test_items(text: str) -> str:
    """Blank cfg(test) items while preserving offsets and line numbers."""
    result = list(text)
    for match in re.finditer(r"(?m)^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", text):
        start = match.start()
        item = text.find("{", match.end())
        semi = text.find(";", match.end())
        if item == -1 or (semi != -1 and semi < item):
            end = semi if semi != -1 else match.end()
        else:
            end = _matching_brace(text, item)
        for index in range(start, end + 1):
            if result[index] != "\n":
                result[index] = " "
    return "".join(result)


def _functions_in(path: Path, root: Path) -> list[Function]:
    text = _without_test_items(path.read_text(encoding="utf-8"))
    braces = _brace_pairs(text)
    line_starts = [index for index, character in enumerate(text) if character == "\n"]
    functions: list[Function] = []
    for match in FUNCTION.finditer(text):
        opening = text.find("{", match.end())
        if opening == -1:
            continue
        end = braces.get(opening, len(text) - 1)
        previous = text[max(0, match.start() - 400):match.start()]
        functions.append(Function(
            name=match.group("name"),
            path=path.relative_to(root),
            line=bisect_right(line_starts, match.start()) + 1,
            public="pub" in match.group("prefix"),
            body=text[opening + 1:end],
            tauri_command=bool(TAURI_COMMAND.search(previous)),
        ))
    return functions


def _macro_commands(root: Path) -> set[str]:
    surface = root / "apps/osl-hub/src/hub_command_surface.rs"
    if not surface.exists():
        return set()
    text = _without_test_items(surface.read_text(encoding="utf-8"))
    marker = text.find("macro_rules! hub_tauri_commands")
    if marker < 0:
        return set()
    opening = text.find("{", marker)
    body = text[opening:_matching_brace(text, opening)]
    callback = body.find("$callback! {")
    if callback < 0:
        return set()
    list_opening = body.find("{", callback)
    entries = body[list_opening + 1:_matching_brace(body, list_opening)]
    return set(re.findall(r"(?m)^\s*(?:#\[[^\]]+\]\s*)*([a-z][A-Za-z0-9_]*)\s*,", entries))


def analyze_tree(root: Path, evidence_root: Path | None = None) -> tuple[list[Function], dict[str, list[Function]], set[str]]:
    """Return all functions, classified unreachable functions, and root names."""
    functions = [function for path in root.rglob("*.rs") for function in _functions_in(path, root)]
    by_name: dict[str, list[Function]] = defaultdict(list)
    for function in functions:
        by_name[function.name].append(function)

    roots = {"main"} | _macro_commands(root)
    roots.update(function.name for function in functions if function.tauri_command)
    reachable: set[Function] = set()
    queue = deque(function for name in roots for function in by_name.get(name, []))
    while queue:
        function = queue.popleft()
        if function in reachable:
            continue
        reachable.add(function)
        for call in CALL.findall(function.body):
            if call not in IGNORED_CALLS:
                queue.extend(candidate for candidate in by_name.get(call, []) if candidate not in reachable)

    evidence_root = evidence_root or root
    evidence = "\n".join(
        path.read_text(encoding="utf-8", errors="ignore")
        for path in evidence_root.rglob("*.md")
        if "reachability-" not in path.name
    )
    buckets: dict[str, list[Function]] = defaultdict(list)
    for function in functions:
        if function in reachable:
            continue
        citation = re.compile(rf"{re.escape(function.path.name)}:{function.line}\b")
        calls_live_logic = any(
            candidate in reachable
            for call in CALL.findall(function.body)
            for candidate in by_name.get(call, [])
        )
        if citation.search(evidence):
            bucket = "cited as evidence for a claim"
        elif function.public and calls_live_logic:
            bucket = "benign dead wrapper over live logic"
        elif function.public:
            bucket = "wire (behaviour genuinely absent)"
        else:
            bucket = "delete"
        buckets[bucket].append(function)
    return functions, buckets, roots


def snapshot(repo: Path) -> Path:
    destination = Path(tempfile.mkdtemp(prefix="reachability-census-"))
    archive = subprocess.run(
        ["git", "archive", "HEAD"], cwd=repo, check=True, capture_output=True
    ).stdout
    with tarfile.open(fileobj=__import__("io").BytesIO(archive)) as bundle:
        bundle.extractall(destination, filter="data")
    return destination


def write_report(repo: Path, output: Path, census_root: Path) -> None:
    functions, buckets, roots = analyze_tree(census_root, repo / "docs")
    today = dt.date.today().isoformat()
    lines = [
        f"# Reachability census — {today}", "",
        "This report was generated from `git archive HEAD`; `#[cfg(test)]` items are excluded.",
        "Roots are `fn main`, every `#[tauri::command]`, and `hub_tauri_commands!` entries.", "",
        f"- Functions scanned: {len(functions)}",
        f"- Root names: {len(roots)}",
        f"- Unreachable functions: {sum(map(len, buckets.values()))}", "",
    ]
    for bucket in (
        "delete", "wire (behaviour genuinely absent)",
        "benign dead wrapper over live logic", "cited as evidence for a claim",
    ):
        entries = sorted(buckets.get(bucket, []), key=lambda item: item.label)
        lines.extend([f"## {bucket} ({len(entries)})", "", "| function | location |", "|---|---|"])
        lines.extend(f"| `{item.name}` | `{item.path}:{item.line}` |" for item in entries)
        lines.append("")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    repo = args.repo.resolve()
    output = args.output or repo / "docs/reports" / f"reachability-{dt.date.today().isoformat()}.md"
    census_root = snapshot(repo)
    try:
        write_report(repo, output, census_root)
    finally:
        shutil.rmtree(census_root)


if __name__ == "__main__":
    main()
