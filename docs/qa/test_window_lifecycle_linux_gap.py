"""Contract for T20's Linux window-lifecycle scope boundary."""

from pathlib import Path
import unittest


DOCUMENT = Path(__file__).with_name("window-lifecycle-linux-gap.md")


class LinuxWindowLifecycleGapTests(unittest.TestCase):
    def test_boundary_states_t20_noncoverage_and_future_owner(self) -> None:
        document = DOCUMENT.read_text(encoding="utf-8")

        self.assertIn("## What T20 does not cover on Linux", document)
        self.assertIn("## What the future live-USB track must cover", document)
        self.assertIn("not a port", document.lower())
        self.assertIn("not a linux test plan", document.lower())
        self.assertIn("hardware + Linux + OSL", document)
        self.assertIn("live USB, no persistence", document)


if __name__ == "__main__":
    unittest.main()
