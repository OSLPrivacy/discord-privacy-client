#!/usr/bin/env python3
"""Remove one root signature in a temporary copy and propagate verifier exit 1."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> int:
    repository = Path(sys.argv[1] if len(sys.argv) > 1 else "release-trust").resolve()
    target = Path(os.environ.get("CARGO_TARGET_DIR", "target"))
    verifier = target / "debug" / ("verify-release-trust.exe" if os.name == "nt" else "verify-release-trust")
    if not verifier.is_file():
        print(f"TASK5169b REFUSED: verifier binary missing at {verifier}", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="osl-task-5169b-") as temporary:
        broken = Path(temporary) / "release-trust"
        shutil.copytree(repository, broken)
        root_path = broken / "metadata" / "root.json"
        root = json.loads(root_path.read_text(encoding="utf-8"))
        root["signatures"] = root["signatures"][:1]
        root_path.write_text(json.dumps(root, indent=2, sort_keys=True), encoding="utf-8")
        completed = subprocess.run([str(verifier), str(broken)], check=False)

    print(
        "TASK5169b starving_second_root_signature signatures=1 required=2 "
        f"exit={completed.returncode}"
    )
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
