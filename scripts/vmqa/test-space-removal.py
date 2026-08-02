#!/usr/bin/env python3
"""Static contract test for the T21-K2 VM driver."""
from pathlib import Path

source = (Path(__file__).parent / "space-removal.ps1").read_text()
required = ("[string]$IdleSenderVm", "SPACE_REMOVAL_REQUIRES_THREE_DISTINCT_VMS", "Invoke-SpaceUia $idleSender", "REMOVED_MEMBER_DECRYPTED_IDLE_SENDER_MESSAGE", "-ExpectAbsent $text", "SPACE_UIA_TRANSCRIPT_MISSING")
for token in required:
    assert token in source, f"missing removal-proof guard: {token}"
assert "Invoke-SpaceUia $remover @{ Mode = 'send'" not in source, "sabotage: remover send makes proof vacuous"
