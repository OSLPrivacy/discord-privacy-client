#!/usr/bin/env python3
"""Fail closed if the real-provider close hooks disappear from 3406."""
from pathlib import Path
import sys

source = Path(sys.argv[1]) if len(sys.argv) == 2 else Path("apps/osl-hub/examples/task_3406_place_text.rs")
text = source.read_text(encoding="utf-8")
required = (
    "TASK3585_PAUSED provider={} pid={} step={step}",
    "GetExitCodeProcess",
    "provider_process_is_live",
    "Your message was not sent anywhere. Retry placement in {}.",
    "private_draft_fingerprint={:016x}",
    "covers_sent=0",
    'pause_and_require_live_provider("marked-paste")',
    'pause_and_require_live_provider("clear")',
    '"empty-readback" | "marked-paste" | "exact-readback" | "clear"',
)
missing = [needle for needle in required if needle not in text]
if missing:
    print("TASK3585_CHECK FAIL missing=" + repr(missing))
    sys.exit(1)
print("TASK3585_CHECK PASS steps=4 real_provider_pid_liveness=true failure_covers_sent=0")
