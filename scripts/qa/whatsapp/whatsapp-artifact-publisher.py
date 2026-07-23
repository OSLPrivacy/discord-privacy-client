#!/usr/bin/env python3
"""Publish a public QA build through Client 1's system-assigned identity.

The local Azure identity only transports bounded public artifact chunks through
the VM control plane. It never receives a Storage token or writes a blob.
"""
from __future__ import annotations

import argparse
import base64
import concurrent.futures
import gzip
import hashlib
import json
import os
import re
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

RESOURCE_GROUP = "OSL-WHATSAPP-TWO-CLIENT-LAB"
VM = "OSL-WhatsApp-Client-1"
LOCATION = "centralus"
HOST = "osltestartifactsa7d5.blob.core.windows.net"
ROOT = Path(__file__).resolve().parent
STAGE = ROOT / "stage-whatsapp-artifact-chunk.ps1"
FINALIZE = ROOT / "finalize-whatsapp-artifact-publish.ps1"
COMMAND = "whatsapp-artifact-publish"
DEFAULT_CHUNK_BYTES = 384 * 1024


class PublishError(RuntimeError):
    pass


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_receipt(output: str) -> dict[str, Any]:
    for line in reversed([line.strip() for line in output.splitlines() if line.strip()]):
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    raise PublishError("Azure RunCommand returned no bounded JSON receipt")


def run_command(
    script: Path,
    parameters: list[dict[str, str]],
    phase: str,
    command_name: str = COMMAND,
) -> dict[str, Any]:
    if command_name != COMMAND and not re.fullmatch(rf"{COMMAND}-[0-7]", command_name):
        raise PublishError(f"{phase}: RunCommand name is outside the publisher allowlist")
    with tempfile.TemporaryDirectory(prefix="osl-wa-runcommand-") as directory:
        request_path = Path(directory) / "request.json"
        arguments: list[str] = []
        for item in parameters:
            name, value = item.get("name"), item.get("value")
            if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9]{0,63}", name):
                raise PublishError(f"{phase}: invalid guest parameter name")
            if not isinstance(value, str) or "'" in value or "\r" in value or "\n" in value:
                raise PublishError(f"{phase}: invalid guest parameter value")
            arguments.extend([f"-{name}", f"'{value}'"])
        guest_script = (
            "& {\n" + script.read_text(encoding="utf-8") + "\n} "
            + " ".join(arguments) + "; if(-not $?){exit 1}"
        )
        request = {
            "location": LOCATION,
            "properties": {
                "source": {"script": guest_script},
                "asyncExecution": False,
                "timeoutInSeconds": 300,
            },
        }
        request_path.write_text(json.dumps(request, separators=(",", ":")), encoding="utf-8")
        os.chmod(request_path, 0o600)
        account = subprocess.run(
            ["az", "account", "show", "--query", "id", "--output", "tsv", "--only-show-errors"],
            text=True, capture_output=True,
        )
        if account.returncode != 0 or not re.fullmatch(r"[0-9a-fA-F-]{36}", account.stdout.strip()):
            raise PublishError(f"{phase}: Azure subscription identity failed closed")
        resource = (
            f"https://management.azure.com/subscriptions/{account.stdout.strip()}"
            f"/resourceGroups/{RESOURCE_GROUP}/providers/Microsoft.Compute"
            f"/virtualMachines/{VM}/runCommands/{command_name}?api-version=2024-03-01"
        )
        common = [
            "--resource-group", RESOURCE_GROUP, "--vm-name", VM,
            "--run-command-name", command_name,
        ]
        completed = subprocess.run(
            ["az", "rest", "--method", "put", "--url", resource,
             "--body", f"@{request_path}", "--output", "none", "--only-show-errors"],
            text=True, capture_output=True,
        )
        if completed.returncode != 0:
            raise PublishError(f"{phase}: Azure RunCommand failed closed")
        deadline = time.monotonic() + 330
        instance: dict[str, Any] | None = None
        while time.monotonic() < deadline:
            shown = subprocess.run(
                ["az", "vm", "run-command", "show", *common, "--expand", "instanceView",
                 "--query", "instanceView", "--output", "json", "--only-show-errors"],
                text=True, capture_output=True,
            )
            if shown.returncode != 0:
                raise PublishError(f"{phase}: Azure RunCommand receipt read failed closed")
            try:
                candidate = json.loads(shown.stdout)
            except json.JSONDecodeError as error:
                raise PublishError(f"{phase}: invalid Azure RunCommand receipt") from error
            if isinstance(candidate, dict):
                instance = candidate
                if candidate.get("executionState") in {"Succeeded", "Failed"}:
                    break
            time.sleep(2)
        if not isinstance(instance, dict) or instance.get("executionState") != "Succeeded" or instance.get("exitCode") != 0:
            raise PublishError(f"{phase}: Azure RunCommand terminal state failed closed")
        output = instance.get("output")
        if not isinstance(output, str):
            raise PublishError(f"{phase}: Azure RunCommand returned no output")
        return parse_receipt(output)


def parameter(name: str, value: str | int) -> dict[str, str]:
    return {"name": name, "value": str(value)}


def write_receipt(path: Path, receipt: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(receipt, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--invocation", required=True)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--exe-sha256", required=True)
    parser.add_argument("--loader-sha256", required=True)
    parser.add_argument("--chunk-bytes", type=int, default=DEFAULT_CHUNK_BYTES)
    parser.add_argument("--workers", type=int, choices=range(1, 9), default=1)
    parser.add_argument("--finalize-only", action="store_true")
    parser.add_argument("--receipt-dir", type=Path, default=ROOT / "receipts")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", args.invocation):
        raise ValueError("invalid invocation ID")
    if not 32 * 1024 <= args.chunk_bytes <= 512 * 1024:
        raise ValueError("chunk size is outside the bounded transport contract")
    expected_exe = args.exe_sha256.lower()
    expected_loader = args.loader_sha256.lower()
    if not re.fullmatch(r"[a-f0-9]{64}", expected_exe) or not re.fullmatch(r"[a-f0-9]{64}", expected_loader):
        raise ValueError("invalid artifact hash")
    if sha256_file(args.exe) != expected_exe:
        raise PublishError("local executable hash mismatch")

    raw = args.exe.read_bytes()
    compressed = gzip.compress(raw, compresslevel=9, mtime=0)
    del raw
    gzip_hash = sha256_bytes(compressed)
    chunks = [compressed[index:index + args.chunk_bytes] for index in range(0, len(compressed), args.chunk_bytes)]
    if not chunks or len(chunks) > 4096:
        raise PublishError("compressed artifact exceeds the bounded chunk contract")
    print(f"artifact has {len(chunks)} bounded chunks", flush=True)
    progress_lock = threading.Lock()

    def stage(index: int, chunk: bytes, lane: int) -> None:
        encoded = base64.b64encode(chunk).decode("ascii")
        if len(encoded) > 700000:
            raise PublishError("encoded artifact chunk exceeds the guest bound")
        receipt = run_command(STAGE, [
            parameter("InvocationId", args.invocation), parameter("ChunkIndex", index),
            parameter("ChunkCount", len(chunks)), parameter("ChunkSha256", sha256_bytes(chunk)),
            parameter("ChunkBase64", encoded),
        ], f"chunk {index + 1}/{len(chunks)}", f"{COMMAND}-{lane}" if args.workers > 1 else COMMAND)
        if (receipt.get("Schema") != "whatsapp-artifact-chunk/v1"
                or receipt.get("InvocationId") != args.invocation
                or receipt.get("ChunkIndex") != index
                or receipt.get("ChunkCount") != len(chunks)
                or receipt.get("ChunkSha256") != sha256_bytes(chunk)
                or receipt.get("Status") not in {"staged", "alreadyStaged"}
                or receipt.get("Terminal") is not True):
            raise PublishError(f"chunk {index + 1}/{len(chunks)}: semantic receipt rejected")
        with progress_lock:
            print(f"staged chunk {index + 1}/{len(chunks)}", flush=True)

    def stage_lane(lane: int) -> None:
        for index in range(lane, len(chunks), args.workers):
            stage(index, chunks[index], lane)

    if not args.finalize_only:
        print(f"staging with {args.workers} managed lanes", flush=True)
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
            futures = [executor.submit(stage_lane, lane) for lane in range(args.workers)]
            for future in concurrent.futures.as_completed(futures):
                future.result()

    result = run_command(FINALIZE, [
        parameter("InvocationId", args.invocation), parameter("ChunkCount", len(chunks)),
        parameter("GzipSha256", gzip_hash), parameter("ExeSha256", expected_exe),
        parameter("LoaderSha256", expected_loader),
    ], "finalize")
    expected_exe_uri = f"https://{HOST}/osl-whatsapp-qa-client1/builds/{expected_exe}/OSL%20Privacy.exe"
    expected_loader_uri = f"https://{HOST}/osl-whatsapp-qa-client1/builds/{expected_exe}/WebView2Loader.dll"
    if (result.get("Schema") != "whatsapp-artifact-publish/v1"
            or result.get("Status") != "publishedByManagedIdentity"
            or result.get("ExeUri") != expected_exe_uri
            or result.get("LoaderUri") != expected_loader_uri
            or result.get("ExeSha256") != expected_exe
            or result.get("LoaderSha256") != expected_loader
            or result.get("ChunkCount") != len(chunks)
            or result.get("Terminal") is not True
            or result.get("SecretRead") is not False
            or result.get("TokenReturned") is not False):
        raise PublishError("managed-identity publication receipt failed semantic validation")
    final = {
        "schema": "whatsapp-artifact-publisher-controller/v1", "invocationId": args.invocation,
        "status": "published", "exeUri": expected_exe_uri, "exeSha256": expected_exe,
        "loaderUri": expected_loader_uri, "loaderSha256": expected_loader,
        "chunkCount": len(chunks), "managedIdentityUpload": True, "secretRead": False,
    }
    write_receipt(args.receipt_dir / f"{args.invocation}.json", final)
    print(json.dumps(final, separators=(",", ":")), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
