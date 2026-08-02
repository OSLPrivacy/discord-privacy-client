#!/usr/bin/env python3
"""Static contract test for the T21-K3 VM driver."""
from pathlib import Path

source = (Path(__file__).parent / "space-join-history.ps1").read_text()
for token in ("[ValidateRange(300, 900)]", "Start-Sleep -Seconds $ReemitWaitSeconds", "Invoke-SpaceUia $sender @{ Mode = 'send'; SpaceId = $SpaceId; Text = $before }", "JOINER_READ_PREJOIN_MESSAGE", "JOINER_DID_NOT_RENDER_EXACTLY_ONE_POSTJOIN_MESSAGE", "[regex]::Matches($log"):
    assert token in source, f"missing join-history guard: {token}"
assert source.index("Text = $before") < source.index("Mode = 'add-member'"), "sabotage: joining before corpus makes proof vacuous"
