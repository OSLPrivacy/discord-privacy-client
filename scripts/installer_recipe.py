#!/usr/bin/env python3
"""Build a local OSL installer fixture from explicit release inputs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def _require_text(value: Any, message: str) -> str:
    if not isinstance(value, str) or not value:
        raise SystemExit(message)
    return value


def run_recipe(input_dir: Path, output_dir: Path) -> Path:
    recipe_file = input_dir / "recipe.json"
    if not recipe_file.is_file():
        raise SystemExit("missing recipe.json")

    recipe = json.loads(recipe_file.read_text(encoding="utf-8"))
    if not isinstance(recipe, dict):
        raise SystemExit("recipe input must be an object")

    version = _require_text(recipe.get("version"), "missing version")
    build = recipe.get("build")
    if not isinstance(build, dict):
        raise SystemExit("missing build input")
    fingerprint = _require_text(build.get("fingerprint"), "missing build input")

    output_dir.mkdir(parents=True, exist_ok=True)
    installer = output_dir / f"OSL-{version}.exe"
    installer.write_text(
        "\n".join(
            (
                "OSL installer fixture",
                f"version={version}",
                f"build_fingerprint={fingerprint}",
                "",
            )
        ),
        encoding="utf-8",
        newline="\n",
    )
    print(f"wrote {installer.name} fingerprint {fingerprint}")
    return installer


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-dir", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    run_recipe(args.input_dir, args.output_dir)


if __name__ == "__main__":
    main()
