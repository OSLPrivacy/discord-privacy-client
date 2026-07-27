#!/usr/bin/env python3
"""Fail-closed verifier for one owner-approved C4 shipping send.

The verifier is intentionally independent of Discord and OSL.  The Windows
collector writes one bounded JSON bundle and one PNG; this program decides only
whether that bundle contains every required, mutually consistent fact.

It never treats a QA-shell trail or receipt as production evidence.  Shipping
has no QA trail, so executable bytes, the production renderer success gate, and
independent UI Automation observations are the authorities.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import uuid
from pathlib import Path
from typing import Any


SCHEMA = "osl-c4-shipping-evidence-v1"
MAX_BUNDLE_BYTES = 256 * 1024
MAX_RUN_MS = 10 * 60 * 1000
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
QA_MARKERS = (
    b"discord-qa-shell",
    b"discord-qa-shell-v1",
    b"send_native_discord_qa_atomic_text",
    b"discord-qa-send-stage-receipt.json",
    b"osl-discord-qa-send-stage.txt",
)
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SUCCESS_STATUS = (
    "Sent privately through OSL. "
    "Discord received only the private-message marker."
)


class EvidenceError(ValueError):
    """A missing, ambiguous, stale, or contradictory evidence fact."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{label} must be an object")
    return value


def _list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise EvidenceError(f"{label} must be an array")
    return value


def _string(value: Any, label: str, *, maximum: int = 512) -> str:
    if not isinstance(value, str) or not value or len(value) > maximum:
        raise EvidenceError(f"{label} must be a non-empty bounded string")
    if any(ord(char) < 0x20 for char in value):
        raise EvidenceError(f"{label} contains a control character")
    return value


def _integer(value: Any, label: str, *, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise EvidenceError(f"{label} must be an integer >= {minimum}")
    return value


def _boolean(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise EvidenceError(f"{label} must be boolean")
    return value


def _sha256(value: Any, label: str) -> str:
    text = _string(value, label, maximum=64)
    if not SHA256_RE.fullmatch(text):
        raise EvidenceError(f"{label} must be lowercase SHA-256")
    return text


def _exact_keys(value: dict[str, Any], label: str, expected: set[str]) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise EvidenceError(f"{label} keys differ; missing={missing}, extra={extra}")


def _at(
    value: Any,
    label: str,
    run_start: int,
    run_end: int,
    *,
    after: int | None = None,
    before: int | None = None,
) -> int:
    observed = _integer(value, label, minimum=1)
    lower = run_start if after is None else max(run_start, after)
    upper = run_end if before is None else min(run_end, before)
    if observed < lower or observed > upper:
        raise EvidenceError(f"{label} is outside its run/order window")
    return observed


def _hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _resolve_artifact(bundle_path: Path, value: Any, label: str) -> Path:
    relative = Path(_string(value, label, maximum=240))
    if relative.is_absolute():
        raise EvidenceError(f"{label} must be relative to the evidence bundle")
    root = bundle_path.parent.resolve()
    resolved = (root / relative).resolve()
    try:
        resolved.relative_to(root)
    except ValueError as error:
        raise EvidenceError(f"{label} escapes the evidence bundle") from error
    if not resolved.is_file():
        raise EvidenceError(f"{label} is missing")
    return resolved


def _scan_for_markers(path: Path) -> None:
    longest = max(map(len, QA_MARKERS))
    carry = b""
    with path.open("rb") as handle:
        while True:
            block = handle.read(1024 * 1024)
            if not block:
                break
            searchable = (carry + block).lower()
            for marker in QA_MARKERS:
                if marker.lower() in searchable:
                    raise EvidenceError(
                        f"executable contains forbidden QA-shell marker: "
                        f"{marker.decode('ascii')}"
                    )
            carry = searchable[-(longest - 1) :]


def _same_binding(
    value: dict[str, Any],
    label: str,
    *,
    run_id: str,
    executable_sha: str,
    target: str,
) -> None:
    if value["runId"] != run_id:
        raise EvidenceError(f"{label}.runId does not bind to this run")
    if value["executableSha256"] != executable_sha:
        raise EvidenceError(f"{label}.executableSha256 does not bind to this executable")
    if value["targetConversation"] != target:
        raise EvidenceError(f"{label}.targetConversation is the wrong target")


def verify_bundle(bundle_path: Path, expected_target: str) -> dict[str, Any]:
    bundle_path = bundle_path.resolve()
    if not bundle_path.is_file():
        raise EvidenceError("bundle is missing")
    if bundle_path.stat().st_size <= 0 or bundle_path.stat().st_size > MAX_BUNDLE_BYTES:
        raise EvidenceError("bundle size is invalid")
    try:
        bundle = json.loads(
            bundle_path.read_text(encoding="utf-8"),
            object_pairs_hook=_reject_duplicate_keys,
        )
    except EvidenceError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise EvidenceError("bundle is not bounded UTF-8 JSON") from error
    bundle = _object(bundle, "bundle")
    _exact_keys(
        bundle,
        "bundle",
        {
            "schema",
            "runId",
            "runStartUnixMs",
            "runEndUnixMs",
            "targetConversation",
            "build",
            "executable",
            "commandReceipt",
            "preEnterReadback",
            "postEnterComposer",
            "conversationRows",
            "screenshot",
        },
    )
    if bundle["schema"] != SCHEMA:
        raise EvidenceError("unsupported evidence schema")
    try:
        run_id = str(uuid.UUID(_string(bundle["runId"], "runId", maximum=36)))
    except (ValueError, AttributeError) as error:
        raise EvidenceError("runId must be a canonical UUID") from error
    if run_id != bundle["runId"]:
        raise EvidenceError("runId must be a canonical UUID")
    run_start = _integer(bundle["runStartUnixMs"], "runStartUnixMs", minimum=1)
    run_end = _integer(bundle["runEndUnixMs"], "runEndUnixMs", minimum=run_start)
    if run_end - run_start > MAX_RUN_MS:
        raise EvidenceError("run exceeds the ten-minute evidence window")
    target = _string(bundle["targetConversation"], "targetConversation", maximum=128)
    if target != expected_target:
        raise EvidenceError("bundle target does not match the operator-approved target")

    build = _object(bundle["build"], "build")
    _exact_keys(
        build,
        "build",
        {
            "frontendCommand",
            "cargoCommand",
            "cargoFeatures",
            "qaShell",
            "observedAtUnixMs",
            "executableSha256",
        },
    )
    if _string(build["frontendCommand"], "build.frontendCommand") != (
        "npm --prefix apps/osl-hub-ui run build"
    ):
        raise EvidenceError("frontend was not built in shipping mode")
    cargo_command = _string(build["cargoCommand"], "build.cargoCommand")
    if "osl-cargo" not in cargo_command:
        raise EvidenceError("shipping desktop build did not use osl-cargo")
    features = _list(build["cargoFeatures"], "build.cargoFeatures")
    if features != ["desktop"]:
        raise EvidenceError("shipping build features must be exactly ['desktop']")
    if _boolean(build["qaShell"], "build.qaShell"):
        raise EvidenceError("QA-shell build is inadmissible")
    if "discord-qa" in cargo_command.lower():
        raise EvidenceError("cargo command mentions a QA-shell feature")
    build_at = _integer(build["observedAtUnixMs"], "build.observedAtUnixMs", minimum=1)
    if build_at > run_start:
        raise EvidenceError("shipping build receipt postdates this run")

    executable = _object(bundle["executable"], "executable")
    _exact_keys(
        executable,
        "executable",
        {"path", "sha256", "processId", "processStartedAtUnixMs"},
    )
    executable_path = Path(_string(executable["path"], "executable.path", maximum=1024))
    if not executable_path.is_absolute() or not executable_path.is_file():
        raise EvidenceError("exact executable path is absent")
    executable_sha = _sha256(executable["sha256"], "executable.sha256")
    if _sha256(build["executableSha256"], "build.executableSha256") != executable_sha:
        raise EvidenceError("shipping build receipt is bound to another executable")
    if _hash_file(executable_path) != executable_sha:
        raise EvidenceError("exact executable hash mismatch")
    _scan_for_markers(executable_path)
    _integer(executable["processId"], "executable.processId", minimum=1)
    process_started = _integer(
        executable["processStartedAtUnixMs"],
        "executable.processStartedAtUnixMs",
        minimum=1,
    )
    if process_started < run_start - 5_000 or process_started > run_end:
        raise EvidenceError("process start does not bind to this run")

    receipt = _object(bundle["commandReceipt"], "commandReceipt")
    _exact_keys(
        receipt,
        "commandReceipt",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "source",
            "receiptAuthority",
            "uiControlAutomationId",
            "backendCommand",
            "status",
            "placed",
            "enterSent",
            "qaShell",
            "rendererStatus",
        },
    )
    _same_binding(
        receipt,
        "commandReceipt",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    receipt_at = _at(
        receipt["observedAtUnixMs"],
        "commandReceipt.observedAtUnixMs",
        run_start,
        run_end,
    )
    required_receipt_values = {
        "source": "production-overlay-ui",
        "receiptAuthority": "shipping-renderer-success-gate",
        "uiControlAutomationId": "prepare-protected",
        "backendCommand": "send_native_discord_overlay_carrier",
        "status": "sent",
        "rendererStatus": SUCCESS_STATUS,
    }
    for key, expected in required_receipt_values.items():
        if receipt[key] != expected:
            raise EvidenceError(f"commandReceipt.{key} is not shipping production evidence")
    if not _boolean(receipt["placed"], "commandReceipt.placed"):
        raise EvidenceError("production command did not prove placement")
    if not _boolean(receipt["enterSent"], "commandReceipt.enterSent"):
        raise EvidenceError("production command did not prove Enter")
    if _boolean(receipt["qaShell"], "commandReceipt.qaShell"):
        raise EvidenceError("QA-shell command receipt is inadmissible")

    pre = _object(bundle["preEnterReadback"], "preEnterReadback")
    _exact_keys(
        pre,
        "preEnterReadback",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "authority",
            "relation",
            "readCount",
            "utf8Bytes",
            "composerTextSha256",
            "exact",
        },
    )
    _same_binding(
        pre,
        "preEnterReadback",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    pre_at = _at(
        pre["observedAtUnixMs"],
        "preEnterReadback.observedAtUnixMs",
        run_start,
        run_end,
        before=receipt_at,
    )
    if pre["authority"] != "native-pre-enter-exact-readback":
        raise EvidenceError("pre-Enter readback lacks the production authority")
    if pre["relation"] != "rawExact":
        raise EvidenceError("pre-Enter readback was not byte-exact")
    if _integer(pre["readCount"], "preEnterReadback.readCount", minimum=0) != 1:
        raise EvidenceError("pre-Enter readback must be present exactly once")
    if _integer(pre["utf8Bytes"], "preEnterReadback.utf8Bytes", minimum=1) > 16_384:
        raise EvidenceError("pre-Enter carrier is unbounded")
    carrier_sha = _sha256(
        pre["composerTextSha256"],
        "preEnterReadback.composerTextSha256",
    )
    if not _boolean(pre["exact"], "preEnterReadback.exact"):
        raise EvidenceError("pre-Enter exact-readback proof is false")

    post = _object(bundle["postEnterComposer"], "postEnterComposer")
    _exact_keys(
        post,
        "postEnterComposer",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "readCount",
            "utf8Bytes",
            "composerTextSha256",
            "empty",
        },
    )
    _same_binding(
        post,
        "postEnterComposer",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    post_at = _at(
        post["observedAtUnixMs"],
        "postEnterComposer.observedAtUnixMs",
        run_start,
        run_end,
        after=receipt_at,
    )
    if _integer(post["readCount"], "postEnterComposer.readCount", minimum=0) != 1:
        raise EvidenceError("post-Enter composer evidence must be present exactly once")
    if _integer(post["utf8Bytes"], "postEnterComposer.utf8Bytes", minimum=0) != 0:
        raise EvidenceError("post-Enter composer is not empty")
    if _sha256(post["composerTextSha256"], "postEnterComposer.composerTextSha256") != EMPTY_SHA256:
        raise EvidenceError("post-Enter empty-composer hash is wrong")
    if not _boolean(post["empty"], "postEnterComposer.empty"):
        raise EvidenceError("post-Enter composer evidence is not empty")

    rows = _object(bundle["conversationRows"], "conversationRows")
    _exact_keys(rows, "conversationRows", {"before", "after", "newRows"})

    def row_snapshot(value: Any, label: str, *, after_time: int | None = None) -> tuple[int, str, int]:
        snapshot = _object(value, label)
        _exact_keys(
            snapshot,
            label,
            {
                "runId",
                "executableSha256",
                "targetConversation",
                "observedAtUnixMs",
                "namedConversationMatches",
                "transcriptMatches",
                "readCount",
                "rowCount",
                "targetBindingSha256",
            },
        )
        _same_binding(
            snapshot,
            label,
            run_id=run_id,
            executable_sha=executable_sha,
            target=target,
        )
        observed = _at(
            snapshot["observedAtUnixMs"],
            f"{label}.observedAtUnixMs",
            run_start,
            run_end,
            after=after_time,
        )
        if _integer(snapshot["namedConversationMatches"], f"{label}.namedConversationMatches") != 1:
            raise EvidenceError(f"{label} does not bind one unique named conversation")
        if _integer(snapshot["transcriptMatches"], f"{label}.transcriptMatches") != 1:
            raise EvidenceError(f"{label} does not bind one unique transcript")
        if _integer(snapshot["readCount"], f"{label}.readCount") != 1:
            raise EvidenceError(f"{label} row evidence must occur exactly once")
        count = _integer(snapshot["rowCount"], f"{label}.rowCount")
        binding = _sha256(snapshot["targetBindingSha256"], f"{label}.targetBindingSha256")
        return count, binding, observed

    before_count, before_binding, before_at = row_snapshot(rows["before"], "conversationRows.before")
    if before_at > pre_at:
        raise EvidenceError("before-row snapshot happened after pre-Enter readback")
    after_count, after_binding, after_at = row_snapshot(
        rows["after"],
        "conversationRows.after",
        after_time=post_at,
    )
    if before_binding != after_binding:
        raise EvidenceError("conversation target changed during the send")
    if after_count - before_count != 1:
        raise EvidenceError("conversation must gain exactly one row")

    new_rows = _list(rows["newRows"], "conversationRows.newRows")
    if len(new_rows) != 1:
        raise EvidenceError("new-row evidence must contain exactly one row")
    new_row = _object(new_rows[0], "conversationRows.newRows[0]")
    _exact_keys(
        new_row,
        "conversationRows.newRows[0]",
        {
            "targetConversation",
            "targetBindingSha256",
            "carrierTextSha256",
            "rowIdentitySha256",
            "matchCount",
        },
    )
    if new_row["targetConversation"] != target:
        raise EvidenceError("new row belongs to the wrong target")
    if _sha256(new_row["targetBindingSha256"], "newRow.targetBindingSha256") != before_binding:
        raise EvidenceError("new row is not bound to the named conversation")
    if _sha256(new_row["carrierTextSha256"], "newRow.carrierTextSha256") != carrier_sha:
        raise EvidenceError("new row does not contain the exact pre-Enter carrier")
    _sha256(new_row["rowIdentitySha256"], "newRow.rowIdentitySha256")
    if _integer(new_row["matchCount"], "newRow.matchCount") != 1:
        raise EvidenceError("new carrier row is missing or duplicated")

    screenshot = _object(bundle["screenshot"], "screenshot")
    _exact_keys(
        screenshot,
        "screenshot",
        {
            "path",
            "sha256",
            "observedAtUnixMs",
            "targetConversation",
            "namedConversationMatches",
            "newRowMatches",
        },
    )
    screenshot_path = _resolve_artifact(bundle_path, screenshot["path"], "screenshot.path")
    if screenshot_path.suffix.lower() != ".png":
        raise EvidenceError("screenshot must be PNG")
    with screenshot_path.open("rb") as handle:
        if handle.read(8) != b"\x89PNG\r\n\x1a\n":
            raise EvidenceError("screenshot does not have a PNG signature")
    if _hash_file(screenshot_path) != _sha256(screenshot["sha256"], "screenshot.sha256"):
        raise EvidenceError("screenshot hash mismatch")
    _at(
        screenshot["observedAtUnixMs"],
        "screenshot.observedAtUnixMs",
        run_start,
        run_end,
        after=after_at,
    )
    if screenshot["targetConversation"] != target:
        raise EvidenceError("screenshot is bound to the wrong target")
    if _integer(screenshot["namedConversationMatches"], "screenshot.namedConversationMatches") != 1:
        raise EvidenceError("screenshot lacks one unique named conversation")
    if _integer(screenshot["newRowMatches"], "screenshot.newRowMatches") != 1:
        raise EvidenceError("screenshot lacks one unique new row")
    screenshot_mtime_ms = screenshot_path.stat().st_mtime_ns // 1_000_000
    if screenshot_mtime_ms < run_start or screenshot_mtime_ms > run_end + 2_000:
        raise EvidenceError("screenshot file is stale or outside this run")

    return {
        "verdict": "pass",
        "tier": "runtime-candidate",
        "schema": SCHEMA,
        "runId": run_id,
        "executableSha256": executable_sha,
        "targetConversation": target,
        "rowDelta": 1,
        "productionReceipt": "sent/placed/enterSent",
        "preEnterReadback": "rawExact",
        "postEnterComposer": "empty",
        "screenshotSha256": screenshot["sha256"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--bundle", type=Path)
    mode.add_argument("--check-executable", type=Path)
    parser.add_argument("--expected-target")
    args = parser.parse_args()
    try:
        if args.check_executable is not None:
            executable = args.check_executable.resolve()
            if not executable.is_file():
                raise EvidenceError("exact executable path is absent")
            _scan_for_markers(executable)
            result = {
                "verdict": "pass",
                "tier": "binary-preflight-only",
                "executableSha256": _hash_file(executable),
                "qaShellMarkers": "absent",
            }
        else:
            if not args.expected_target:
                raise EvidenceError("--expected-target is required with --bundle")
            result = verify_bundle(args.bundle, args.expected_target)
    except EvidenceError as error:
        print(
            json.dumps(
                {
                    "verdict": "reject",
                    "tier": "unmeasurable",
                    "reason": str(error),
                },
                separators=(",", ":"),
            )
        )
        return 1
    print(json.dumps(result, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
