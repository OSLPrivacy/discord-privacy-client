import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/qa/task0041-signal-carrier-claim.ps1"


class Task0041ClaimVerifierContract(unittest.TestCase):
    def test_requires_one_custom_profile_and_a_nontrivial_window_capture(self):
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("--user-data-dir", source)
        self.assertIn("$processes.Count -ne 1", source)
        self.assertIn("db.sqlite", source)
        self.assertIn("CopyFromScreen", source)
        self.assertIn("signal-signed-in-account.png", source)

    def test_persists_machine_account_directory_and_capture_in_roster(self):
        source = SCRIPT.read_text(encoding="utf-8")
        for key in ("machine", "account_id", "data_directory", "capture"):
            self.assertIn(key, source)


if __name__ == "__main__":
    unittest.main()
