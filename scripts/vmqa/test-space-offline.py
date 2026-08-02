#!/usr/bin/env python3
"""Static contract test for the T21-K5 VM driver."""
from pathlib import Path

source = (Path(__file__).parent / "space-offline.ps1").read_text()
for token in ("Disable-NetAdapter", "ADAPTER_STILL_UP", "Mode = 'offline-history'", "OFFLINE_HISTORY_NOT_READABLE", "Mode = 'offline-send'", "OFFLINE_COMPOSE_DID_NOT_STAY_QUEUED", "Mode = 'local-burn'", "LOCAL_BURN_WAS_NOT_IMMEDIATE", "Mode = 'remove-member'", "ExpectStaleRosterRefusal", "STALE_OFFLINE_SENDER_WAS_NOT_REFUSED"):
    assert token in source, f"missing offline proof guard: {token}"
assert source.index("Disable-NetAdapter") < source.index("Mode = 'offline-history'"), "sabotage: adapter must be disabled before offline checks"
assert source.index("Mode = 'remove-member'") < source.index("Enable-NetAdapter"), "stale roster must be created before reconnect"
