#!/usr/bin/env python3
"""Metadata-only Telegram Desktop UI Automation measurement.

This intentionally returns exposure booleans and counts, never Telegram names,
messages, participant names, automation IDs, or text values.
"""

from __future__ import annotations

import json
import sys
from collections.abc import Iterable, Mapping
from pathlib import Path
from typing import Any

MINIMUM_STABLE_ROWS = 2
MINIMUM_TELEGRAM_VERSION = (6, 8, 3)


def is_current_telegram_version(version: tuple[int, int, int, int]) -> bool:
    """Require the upstream accessibility release, not merely any Telegram build."""
    return version[:3] >= MINIMUM_TELEGRAM_VERSION


def _visible(node: Mapping[str, Any]) -> bool:
    rect = node.get("rectangle", {})
    return bool(node.get("visible")) and rect.get("width", 0) >= 120 and 12 <= rect.get("height", 0) <= 180


def classify(samples: Iterable[Iterable[Mapping[str, Any]]]) -> dict[str, object]:
    """Classify repeated metadata snapshots conservatively.

    A row is stable only when the same non-content fingerprint appears in every
    sample.  Text is inspected only to produce a boolean, never serialized.
    """
    snapshots = [list(sample) for sample in samples]
    if not snapshots:
        raise ValueError("at least one UIA snapshot is required")

    row_sets: list[set[tuple[int, int, int, int]]] = []
    text_exposed_sets: list[set[tuple[int, int, int, int]]] = []
    composer_present = False
    composer_text_pattern = False
    title_exposed = False
    participant_identity_exposed = False

    for nodes in snapshots:
        rows: set[tuple[int, int, int, int]] = set()
        exposed_rows: set[tuple[int, int, int, int]] = set()
        for node in nodes:
            rect = node.get("rectangle", {})
            fingerprint = (rect.get("left", 0), rect.get("top", 0), rect.get("width", 0), rect.get("height", 0))
            if node.get("control_type") == "ListItem" and _visible(node):
                rows.add(fingerprint)
                if node.get("text_exposed"):
                    exposed_rows.add(fingerprint)
            if node.get("is_composer"):
                composer_present = True
                composer_text_pattern = composer_text_pattern or bool(node.get("text_pattern"))
            title_exposed = title_exposed or bool(node.get("is_conversation_title") and node.get("text_exposed"))
            participant_identity_exposed = participant_identity_exposed or bool(
                node.get("is_participant_identity") and node.get("text_exposed")
            )
        row_sets.append(rows)
        text_exposed_sets.append(exposed_rows)

    stable_rows = set.intersection(*row_sets)
    stable_text_rows = set.intersection(*text_exposed_sets)
    return {
        "Schema": "telegram-uia-probe/v1",
        "Samples": len(snapshots),
        "StableListItemRowCount": len(stable_rows),
        "StableTextExposedRowCount": len(stable_text_rows),
        "Composer": {"Present": composer_present, "TextPattern": composer_text_pattern},
        "Conversation": {
            "TitleTextExposed": title_exposed,
            "ParticipantIdentityTextExposed": participant_identity_exposed,
        },
        "Verdict": "supported"
        if len(stable_rows) >= MINIMUM_STABLE_ROWS and len(stable_text_rows) >= MINIMUM_STABLE_ROWS
        else "externally blocked",
    }


def _text_exposed(wrapper: Any) -> bool:
    try:
        if wrapper.window_text().strip():
            return True
    except Exception:
        pass
    try:
        return bool(wrapper.iface_value.CurrentValue.strip())
    except Exception:
        return False


def _snapshot(window: Any) -> list[dict[str, object]]:
    nodes: list[dict[str, object]] = []
    for wrapper in window.descendants():
        info = wrapper.element_info
        rect = wrapper.rectangle()
        control_type = info.control_type
        visible = wrapper.is_visible() and rect.width > 0 and rect.height > 0
        # These flags deliberately use geometry and UIA control metadata only.
        # The only content read is converted immediately to a boolean.
        nodes.append(
            {
                "control_type": control_type,
                "visible": visible,
                "rectangle": {"left": rect.left, "top": rect.top, "width": rect.width, "height": rect.height},
                "text_exposed": _text_exposed(wrapper),
                "text_pattern": bool(getattr(wrapper, "iface_text", None) or getattr(wrapper, "iface_value", None)),
                "is_composer": control_type == "Edit" and visible and rect.top > window.rectangle().top + window.rectangle().height // 2,
                "is_conversation_title": control_type == "Text" and visible and rect.top < window.rectangle().top + 180,
                "is_participant_identity": control_type in {"ListItem", "Text"} and visible and rect.left < window.rectangle().left + 360,
            }
        )
    return nodes


def run(samples: int, delay_seconds: float) -> dict[str, object]:
    try:
        from pywinauto import Desktop
        import psutil
        import win32api
    except ImportError as error:
        raise RuntimeError("pywinauto is required; install it in the VM's Python environment") from error

    windows = [window for window in Desktop(backend="uia").windows() if window.process_id() and "Telegram" in window.window_text()]
    if len(windows) != 1:
        raise RuntimeError("exactly one visible Telegram Desktop UIA window is required")
    window = windows[0]
    executable = psutil.Process(window.process_id()).exe()
    version_info = win32api.GetFileVersionInfo(executable, "\\")
    version = (
        version_info["FileVersionMS"] >> 16,
        version_info["FileVersionMS"] & 0xFFFF,
        version_info["FileVersionLS"] >> 16,
        version_info["FileVersionLS"] & 0xFFFF,
    )
    if not is_current_telegram_version(version):
        raise RuntimeError(f"Telegram Desktop {'.'.join(map(str, version[:3]))} is older than 6.8.3")
    snapshots = []
    for index in range(samples):
        snapshots.append(_snapshot(window))
        if index + 1 < samples:
            import time

            time.sleep(delay_seconds)
    result = classify(snapshots)
    result["TelegramVersion"] = ".".join(map(str, version[:3]))
    return result


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: telegram.py SAMPLES DELAY_SECONDS")
    try:
        print(json.dumps(run(int(sys.argv[1]), float(sys.argv[2])), separators=(",", ":")))
    except Exception as error:
        print(json.dumps({"Schema": "telegram-uia-probe/v1", "Verdict": "inconclusive", "Error": str(error)}))
        raise SystemExit(1)
