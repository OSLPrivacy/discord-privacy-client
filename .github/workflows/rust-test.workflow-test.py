#!/usr/bin/env python3
"""Behavioral contract for the Rust workflow's cache write policy."""

from pathlib import Path
import unittest

import yaml


WORKFLOW = Path(__file__).with_name("rust-test.yml")
RUST_CACHE = "Swatinem/rust-cache@42dc69e1aa15d09112580998cf2ef0119e2e91ae"


def cache_save_enabled(ref: str) -> bool:
    workflow = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    steps = workflow["jobs"]["test"]["steps"]
    cache_step = next(step for step in steps if step.get("uses") == RUST_CACHE)
    save_if = cache_step["with"]["save-if"]

    if save_if != "${{ github.ref == 'refs/heads/main' }}":
        raise ValueError(f"unexpected rust-cache save condition: {save_if!r}")
    return ref == "refs/heads/main"


class RustCacheSavePolicyTest(unittest.TestCase):
    def test_only_main_can_save_the_rust_cache(self) -> None:
        self.assertTrue(cache_save_enabled("refs/heads/main"))
        self.assertFalse(cache_save_enabled("refs/pull/42/merge"))
        self.assertFalse(cache_save_enabled("refs/heads/feature/cache-tuning"))


if __name__ == "__main__":
    unittest.main()
