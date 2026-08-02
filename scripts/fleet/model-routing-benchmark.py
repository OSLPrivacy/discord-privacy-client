#!/usr/bin/env python3
"""T8-D4: turn measured model runs into an expiring routing table.

Input deliberately contains observations rather than vendor/model claims.  A
sample is a real task attempt and records the review outcome as well as its
costs.  The selector only considers profiles whose one-sided Wilson lower
bound for clean accepted work clears the task-class threshold.
"""
from __future__ import annotations

import argparse
import json
import math
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


REQUIRED_SAMPLE = {
    "accepted", "defects", "security_misses", "rework_usd", "wait_usd",
    "wall_seconds", "input_tokens", "output_tokens", "model_cost_usd",
    "tool_reliability", "context_retention", "human_review_seconds",
}


def wilson_lower(successes: int, total: int, z: float = 1.645) -> float:
    """One-sided 95% Wilson lower bound (not a prestige or speed ranking)."""
    if total == 0:
        return 0.0
    p = successes / total
    denominator = 1 + z * z / total
    centre = p + z * z / (2 * total)
    spread = z * math.sqrt((p * (1 - p) + z * z / (4 * total)) / total)
    return max(0.0, (centre - spread) / denominator)


def validate(payload: dict[str, Any]) -> None:
    if not isinstance(payload.get("task_classes"), list) or not payload["task_classes"]:
        raise ValueError("task_classes must be a non-empty list")
    if not isinstance(payload.get("profiles"), list) or not payload["profiles"]:
        raise ValueError("profiles must be a non-empty list")
    for task in payload["task_classes"]:
        if not isinstance(task.get("id"), str) or not 0 < task.get("quality_threshold", 0) <= 1:
            raise ValueError("each task class requires id and quality_threshold in (0, 1]")
    for profile in payload["profiles"]:
        if not isinstance(profile.get("id"), str):
            raise ValueError("each profile requires id")
        samples = profile.get("samples")
        if not isinstance(samples, list) or not samples:
            raise ValueError(f"{profile.get('id', 'profile')}: samples must be non-empty")
        for sample in samples:
            missing = REQUIRED_SAMPLE - sample.keys()
            if missing:
                raise ValueError(f"{profile['id']}: sample missing {', '.join(sorted(missing))}")
            if not isinstance(sample["accepted"], bool) or not isinstance(sample["defects"], int) or not isinstance(sample["security_misses"], int):
                raise ValueError(f"{profile['id']}: accepted must be boolean; defects/security_misses integers")


def metrics(samples: list[dict[str, Any]]) -> dict[str, Any]:
    clean = [s for s in samples if s["accepted"] and s["defects"] == 0 and s["security_misses"] == 0]
    n = len(samples)
    total_cost = sum(float(s["model_cost_usd"]) + float(s["rework_usd"]) + float(s["wait_usd"]) for s in samples)
    return {
        "attempts": n,
        "clean_accepted": len(clean),
        "lower_confidence_quality": wilson_lower(len(clean), n),
        "quality_adjusted_cost_usd": total_cost / max(len(clean), 1),
        "average_total_cost_usd": total_cost / n,
        "defects": sum(s["defects"] for s in samples),
        "security_misses": sum(s["security_misses"] for s in samples),
        "average_wall_seconds": sum(float(s["wall_seconds"]) for s in samples) / n,
        "input_tokens": sum(int(s["input_tokens"]) for s in samples),
        "output_tokens": sum(int(s["output_tokens"]) for s in samples),
        "average_tool_reliability": sum(float(s["tool_reliability"]) for s in samples) / n,
        "average_context_retention": sum(float(s["context_retention"]) for s in samples) / n,
        "human_review_seconds": sum(float(s["human_review_seconds"]) for s in samples),
    }


def build_table(payload: dict[str, Any]) -> dict[str, Any]:
    validate(payload)
    computed = {p["id"]: metrics(p["samples"]) for p in payload["profiles"]}
    routes: dict[str, Any] = {}
    for task in payload["task_classes"]:
        eligible = [(computed[p["id"]]["average_total_cost_usd"], p["id"], computed[p["id"]])
                    for p in payload["profiles"]
                    if computed[p["id"]]["lower_confidence_quality"] >= task["quality_threshold"]]
        if not eligible:
            raise ValueError(f"{task['id']}: no profile meets lower-confidence threshold")
        _, profile_id, result = min(eligible)
        routes[task["id"]] = {
            "profile": profile_id,
            "quality_threshold": task["quality_threshold"],
            "lower_confidence_quality": result["lower_confidence_quality"],
            "average_total_cost_usd": result["average_total_cost_usd"],
        }
    return {
        "format": "osl-model-routing-v1",
        "generated_at": payload["generated_at"],
        "expires_at": payload["expires_at"],
        "profiles": computed,
        "routes": routes,
    }


def self_test() -> None:
    base = {"accepted": True, "defects": 0, "security_misses": 0, "rework_usd": 0,
            "wait_usd": 0, "wall_seconds": 10, "input_tokens": 1, "output_tokens": 1,
            "tool_reliability": 1, "context_retention": 1, "human_review_seconds": 1}
    cheap = [{**base, "model_cost_usd": 1} for _ in range(20)]
    weak = [{**base, "model_cost_usd": .01, "accepted": i < 12} for i in range(20)]
    payload = {"generated_at": "2026-08-02T00:00:00Z", "expires_at": "2026-09-01T00:00:00Z",
               "task_classes": [{"id": "mechanical", "quality_threshold": .70}],
               "profiles": [{"id": "cheap-but-weak", "samples": weak}, {"id": "qualifying", "samples": cheap}]}
    assert build_table(payload)["routes"]["mechanical"]["profile"] == "qualifying"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("input", nargs="?", type=Path, help="measured benchmark JSON")
    parser.add_argument("--output", type=Path, help="write routing table JSON")
    parser.add_argument("--self-test", action="store_true", help="run T8-T18 fixture")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("T8-T18: routing fixture passed")
        return 0
    if args.input is None:
        parser.error("input is required unless --self-test is used")
    try:
        table = build_table(json.loads(args.input.read_text(encoding="utf-8")))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"model-routing: {exc}", file=__import__("sys").stderr)
        return 2
    encoded = json.dumps(table, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
