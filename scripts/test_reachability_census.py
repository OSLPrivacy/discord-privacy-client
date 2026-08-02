import tempfile
import unittest
from pathlib import Path

from scripts.reachability_census import analyze_tree


class ReachabilityCensusTest(unittest.TestCase):
    def test_cfg_test_function_is_unreachable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "lib.rs").write_text(
                "fn main() { live(); }\n"
                "fn live() {}\n"
                "fn test_only() {}\n"
                "#[cfg(test)]\n"
                "fn only_test_calls_it() { test_only(); }\n",
                encoding="utf-8",
            )
            _, buckets, _ = analyze_tree(root)
        unreachable = {item.name for items in buckets.values() for item in items}
        self.assertIn("test_only", unreachable)
        self.assertNotIn("live", unreachable)


if __name__ == "__main__":
    unittest.main()
