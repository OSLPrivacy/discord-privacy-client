"""Static contract for TASK 4152's fail-closed live-send preflight."""

import unittest
from pathlib import Path


SOURCE = (Path(__file__).parent / "task-4152-discord-two-way-preflight.ps1").read_text()


class Task4152PreflightContract(unittest.TestCase):
    def test_only_the_two_owned_release_channels_are_named(self) -> None:
        self.assertIn("@('Canary', 'PTB')", SOURCE)
        self.assertIn("release_channel -ceq 'Canary'", SOURCE)
        self.assertIn("release_channel -ceq 'PTB'", SOURCE)
        self.assertNotIn("--user-data-dir", SOURCE.split("caller supplied", 1)[0])

    def test_both_receiver_faults_are_terminal(self) -> None:
        self.assertIn("[switch]$StubCanaryReceiver", SOURCE)
        self.assertIn("[switch]$StubPtbReceiver", SOURCE)
        self.assertIn("Canary receiving job stubbed", SOURCE)
        self.assertIn("PTB receiving job stubbed", SOURCE)
        self.assertIn("if (-not $record.ready_for_two_way_send) { exit 1 }", SOURCE)

    def test_no_blind_send_path_exists_before_receivers_are_ready(self) -> None:
        self.assertIn("send_attempts = 0", SOURCE)
        self.assertIn("never permission to attempt a blind Discord send", SOURCE)
        self.assertNotIn("SendKeys", SOURCE)
        self.assertNotIn("InvokePattern]::Invoke", SOURCE)


if __name__ == "__main__":
    unittest.main()
