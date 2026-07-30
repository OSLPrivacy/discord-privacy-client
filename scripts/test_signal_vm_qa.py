#!/usr/bin/env python3
"""Compatibility entry point for the embedded Signal VM QA self-tests."""

from pathlib import Path
import importlib.util
import sys


PROGRAM = Path(__file__).parent / "qa" / "signal_vm_qa.py"
SPEC = importlib.util.spec_from_file_location("signal_vm_qa", PROGRAM)
assert SPEC is not None and SPEC.loader is not None
signal_vm_qa = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = signal_vm_qa
SPEC.loader.exec_module(signal_vm_qa)


if __name__ == "__main__":
    raise SystemExit(signal_vm_qa.run_self_tests())
