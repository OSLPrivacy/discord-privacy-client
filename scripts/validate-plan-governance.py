#!/usr/bin/env python3
"""CI entry point for the plan governance acceptance gates."""
from __future__ import annotations
import pathlib, subprocess, sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
TESTS=("test_change_set.py","test_feature_view_sync.py","test_spec_change.py","test_trap_ledger.py","test_spec_control_guard.py")
def main():
 for test in TESTS:
  completed=subprocess.run([sys.executable,str(ROOT/'claude-bridge/tests'/test)],cwd=ROOT)
  if completed.returncode:return completed.returncode
 return 0
if __name__=='__main__':raise SystemExit(main())
