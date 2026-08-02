from pathlib import Path


def test_t5_t18_uses_accessibility_not_capture_and_preserves_exact_comparison():
    source = Path(__file__).with_name("t5-safety-number.ps1").read_text()
    assert "UIAutomationClient" in source
    assert "Shared verification code for this friend" in source
    assert "friend-verification-input" in source
    assert "owned-confirmation-submit" in source
    assert "$localCode -cne $peerCode" in source
    assert "Set-UiaText $input $peerCode" in source
    assert "Invoke-Uia $confirm" in source
    assert "Screenshot" not in source
