#!/usr/bin/env python3
"""Create and verify retained evidence for an exact Windows VMQA build."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path, PurePosixPath
from typing import Any


SCHEMA_VERSION = 2
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
EXPECTED_COMMANDS = [
    ["npm", "run", "build"],
    [
        "osl-cargo",
        "build",
        "--release",
        "--features",
        "desktop",
        "--bin",
        "osl-privacy-hub",
        "--target",
        "x86_64-pc-windows-gnu",
        "--message-format=json",
    ],
]
EVIDENCE_FILES = {
    "source.tar",
    "dist.tar",
    "dist-manifest.json",
    "npm-build.log",
    "cargo-build.jsonl",
    "build-log.json",
}
REQUIRED_SOURCE_PATHS = {
    "Cargo.toml",
    "apps/osl-hub/Cargo.toml",
    "apps/osl-hub-ui/package.json",
}


class EvidenceError(ValueError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        actual = set(value) if isinstance(value, dict) else set()
        raise EvidenceError(
            f"{label} fields are not exact; missing={sorted(keys - actual)} "
            f"unknown={sorted(actual - keys)}"
        )
    return value


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"{label} is not valid UTF-8 JSON: {exc}") from exc


def safe_member_name(name: str, label: str) -> str:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or not path.parts:
        raise EvidenceError(f"{label} contains unsafe path {name!r}")
    return path.as_posix()


def git_object_sha(kind: str, payload: bytes) -> bytes:
    header = f"{kind} {len(payload)}\0".encode("ascii")
    return hashlib.sha1(header + payload).digest()


def git_tree_from_archive(archive: tarfile.TarFile) -> str:
    root: dict[str, Any] = {}
    for member in archive.getmembers():
        name = safe_member_name(member.name, "source archive")
        if member.isdir():
            continue
        parts = PurePosixPath(name).parts
        node = root
        for part in parts[:-1]:
            child = node.setdefault(part, {})
            if not isinstance(child, dict):
                raise EvidenceError("source archive has a file/directory path collision")
            node = child
        if parts[-1] in node:
            raise EvidenceError("source archive contains a duplicate path")
        if member.issym():
            mode = b"120000"
            payload = member.linkname.encode("utf-8")
        elif member.isfile():
            mode = b"100755" if member.mode & 0o111 else b"100644"
            extracted = archive.extractfile(member)
            if extracted is None:
                raise EvidenceError(f"source archive member cannot be read: {name}")
            payload = extracted.read()
        else:
            raise EvidenceError("source archive contains an unsupported member type")
        node[parts[-1]] = (mode, git_object_sha("blob", payload))

    def hash_tree(node: dict[str, Any]) -> bytes:
        entries: list[tuple[bytes, bytes, bytes]] = []
        for name, value in node.items():
            encoded = name.encode("utf-8")
            if isinstance(value, dict):
                entries.append((encoded + b"/", b"40000", hash_tree(value)))
            else:
                mode, digest = value
                entries.append((encoded, mode, digest))
        payload = b"".join(
            mode + b" " + sort_name.rstrip(b"/") + b"\0" + digest
            for sort_name, mode, digest in sorted(entries, key=lambda entry: entry[0])
        )
        return git_object_sha("tree", payload)

    return hash_tree(root).hex()


def dist_entries_from_directory(directory: Path) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for path in sorted(candidate for candidate in directory.rglob("*") if candidate.is_file()):
        relative = path.relative_to(directory).as_posix()
        entries.append(
            {
                "path": safe_member_name(relative, "dist"),
                "sha256": sha256_file(path),
                "sizeBytes": path.stat().st_size,
            }
        )
    if not entries:
        raise EvidenceError("dist directory is empty")
    return entries


def dist_digest(entries: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for entry in entries:
        digest.update(
            f"{entry['sha256']}  ./{entry['path']}\n".encode("utf-8")
        )
    return digest.hexdigest()


def write_dist_tar(directory: Path, path: Path, entries: list[dict[str, Any]]) -> None:
    with tarfile.open(path, "w", format=tarfile.PAX_FORMAT) as archive:
        for entry in entries:
            source = directory / entry["path"]
            info = tarfile.TarInfo(entry["path"])
            info.size = source.stat().st_size
            info.mode = 0o644
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            with source.open("rb") as handle:
                archive.addfile(info, handle)


def verify_dist_tar(path: Path, manifest: list[dict[str, Any]]) -> None:
    observed: list[dict[str, Any]] = []
    with tarfile.open(path, "r:") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                raise EvidenceError("dist archive contains a non-file member")
            name = safe_member_name(member.name, "dist archive")
            extracted = archive.extractfile(member)
            if extracted is None:
                raise EvidenceError(f"dist archive member cannot be read: {name}")
            data = extracted.read()
            observed.append(
                {
                    "path": name,
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "sizeBytes": len(data),
                }
            )
    if observed != manifest:
        raise EvidenceError("dist manifest differs from retained dist archive bytes")


def parse_cargo_artifact(path: Path) -> str:
    artifacts: list[str] = []
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw_line.strip():
            continue
        try:
            value = json.loads(raw_line)
        except json.JSONDecodeError as exc:
            raise EvidenceError(
                f"cargo build log line {line_number} is not JSON: {exc}"
            ) from exc
        if (
            isinstance(value, dict)
            and value.get("reason") == "compiler-artifact"
            and isinstance(value.get("target"), dict)
            and value["target"].get("name") == "osl-privacy-hub"
            and isinstance(value.get("executable"), str)
            and value["executable"]
        ):
            artifacts.append(value["executable"])
    if len(artifacts) != 1:
        raise EvidenceError(
            f"cargo build log must contain exactly one OSL executable artifact, got {len(artifacts)}"
        )
    return artifacts[0]


def git_output(repository: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repository), *args],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise EvidenceError(result.stderr.strip() or "git command failed")
    return result.stdout.strip()


def create_evidence(args: argparse.Namespace) -> None:
    source = Path(args.source_repo).resolve()
    dist = Path(args.dist).resolve()
    exe = Path(args.exe).resolve()
    loader = Path(args.loader).resolve()
    npm_log = Path(args.npm_log).resolve()
    cargo_log = Path(args.cargo_log).resolve()
    output = Path(args.output).resolve()
    for path, label in (
        (exe, "executable"),
        (loader, "loader"),
        (npm_log, "npm log"),
        (cargo_log, "cargo log"),
    ):
        if not path.is_file():
            raise EvidenceError(f"{label} is missing: {path}")
    if not dist.is_dir():
        raise EvidenceError(f"dist directory is missing: {dist}")
    if git_output(source, "status", "--porcelain=v1", "--untracked-files=all"):
        raise EvidenceError("source worktree is not clean")
    commit = git_output(source, "rev-parse", "HEAD")
    tree = git_output(source, "rev-parse", "HEAD^{tree}")
    if not COMMIT_RE.fullmatch(commit) or not COMMIT_RE.fullmatch(tree):
        raise EvidenceError("source commit/tree is not exact")
    if (
        commit != args.expected_commit
        or tree != args.expected_tree
        or not COMMIT_RE.fullmatch(args.expected_commit)
        or not COMMIT_RE.fullmatch(args.expected_tree)
    ):
        raise EvidenceError("source differs from independently expected commit/tree")
    output.mkdir(parents=True, exist_ok=True)
    actual = {path.name for path in output.iterdir()}
    if actual:
        raise EvidenceError(f"evidence output directory is not empty: {sorted(actual)}")
    source_tar = output / "source.tar"
    subprocess.run(
        ["git", "-C", str(source), "archive", "--format=tar", "-o", str(source_tar), "HEAD"],
        check=True,
    )
    with tarfile.open(source_tar, "r:") as archive:
        files = {
            safe_member_name(member.name, "source archive")
            for member in archive.getmembers()
            if member.isfile()
        }
        if not REQUIRED_SOURCE_PATHS.issubset(files):
            raise EvidenceError(
                f"source archive lacks product files: {sorted(REQUIRED_SOURCE_PATHS - files)}"
            )
        if archive.pax_headers.get("comment") != commit:
            raise EvidenceError("source archive does not carry the named Git commit")
        if git_tree_from_archive(archive) != tree:
            raise EvidenceError("source archive bytes do not reproduce the named Git tree")
    entries = dist_entries_from_directory(dist)
    (output / "dist-manifest.json").write_text(
        json.dumps(
            {"schemaVersion": 2, "files": entries},
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    write_dist_tar(dist, output / "dist.tar", entries)
    (output / "npm-build.log").write_bytes(npm_log.read_bytes())
    (output / "cargo-build.jsonl").write_bytes(cargo_log.read_bytes())
    artifact_path = Path(parse_cargo_artifact(cargo_log)).resolve()
    if artifact_path != exe:
        raise EvidenceError(
            f"cargo compiler artifact {artifact_path} differs from supplied executable {exe}"
        )
    try:
        artifact_relative = exe.relative_to(source).as_posix()
    except ValueError as exc:
        raise EvidenceError("executable is outside the named source repository") from exc
    if not artifact_relative.endswith(
        "/x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
    ):
        raise EvidenceError("executable is not the exact Windows release artifact path")
    osl_cargo = shutil.which("osl-cargo")
    if osl_cargo is None:
        raise EvidenceError("osl-cargo is unavailable")
    def version(*command: str) -> str:
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        if result.returncode != 0 or not result.stdout.strip():
            raise EvidenceError(f"toolchain command failed: {command!r}")
        return result.stdout.strip()
    log = {
        "schemaVersion": 2,
        "source": {
            "commit": commit,
            "tree": tree,
            "clean": True,
            "dirtyFingerprint": EMPTY_SHA256,
            "archiveFile": "source.tar",
            "archiveSha256": sha256_file(source_tar),
        },
        "ui": {
            "distSha256": dist_digest(entries),
            "archiveFile": "dist.tar",
            "archiveSha256": sha256_file(output / "dist.tar"),
            "manifestFile": "dist-manifest.json",
            "manifestSha256": sha256_file(output / "dist-manifest.json"),
        },
        "commands": EXPECTED_COMMANDS,
        "toolchain": {
            "rustc": version("rustc", "-Vv").replace("\n", ";"),
            "cargo": version("cargo", "-V"),
            "node": version("node", "-v"),
            "npm": version("npm", "-v"),
            "oslCargoSha256": sha256_file(Path(osl_cargo)),
        },
        "outputs": {
            "npmFile": "npm-build.log",
            "npmSha256": sha256_file(output / "npm-build.log"),
            "cargoFile": "cargo-build.jsonl",
            "cargoSha256": sha256_file(output / "cargo-build.jsonl"),
        },
        "artifact": {
            "path": artifact_relative,
            "sha256": sha256_file(exe),
            "sizeBytes": exe.stat().st_size,
        },
    }
    (output / "build-log.json").write_text(
        json.dumps(log, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def verify_evidence(
    directory: Path, exe_path: Path, identity: dict[str, Any]
) -> dict[str, Any]:
    actual_files = {path.name for path in directory.iterdir()}
    if actual_files != EVIDENCE_FILES or not all(
        (directory / name).is_file() for name in EVIDENCE_FILES
    ):
        raise EvidenceError(
            f"build evidence files are not exact; missing={sorted(EVIDENCE_FILES - actual_files)} "
            f"unknown={sorted(actual_files - EVIDENCE_FILES)}"
        )
    log = exact_object(
        load_json(directory / "build-log.json", "build log"),
        {
            "schemaVersion",
            "source",
            "ui",
            "commands",
            "toolchain",
            "outputs",
            "artifact",
        },
        "buildLog",
    )
    if type(log["schemaVersion"]) is not int or log["schemaVersion"] != 2:
        raise EvidenceError("buildLog.schemaVersion must be exactly 2")
    source = exact_object(
        log["source"],
        {"commit", "tree", "clean", "dirtyFingerprint", "archiveFile", "archiveSha256"},
        "buildLog.source",
    )
    ui = exact_object(
        log["ui"],
        {
            "distSha256",
            "archiveFile",
            "archiveSha256",
            "manifestFile",
            "manifestSha256",
        },
        "buildLog.ui",
    )
    outputs = exact_object(
        log["outputs"],
        {"npmFile", "npmSha256", "cargoFile", "cargoSha256"},
        "buildLog.outputs",
    )
    artifact = exact_object(
        log["artifact"], {"path", "sha256", "sizeBytes"}, "buildLog.artifact"
    )
    toolchain = exact_object(
        log["toolchain"],
        {"rustc", "cargo", "node", "npm", "oslCargoSha256"},
        "buildLog.toolchain",
    )
    for name in ("rustc", "cargo", "node", "npm"):
        if not isinstance(toolchain[name], str) or not toolchain[name]:
            raise EvidenceError(f"build log toolchain {name} must be a nonempty string")
    if (
        not isinstance(toolchain["oslCargoSha256"], str)
        or not SHA_RE.fullmatch(toolchain["oslCargoSha256"])
    ):
        raise EvidenceError("build log osl-cargo digest is invalid")
    if log["commands"] != EXPECTED_COMMANDS:
        raise EvidenceError("build log argv is not the exact supported build")
    if (
        not isinstance(source["commit"], str)
        or not COMMIT_RE.fullmatch(source["commit"])
        or not isinstance(source["tree"], str)
        or not COMMIT_RE.fullmatch(source["tree"])
        or source["clean"] is not True
        or source["dirtyFingerprint"] != EMPTY_SHA256
    ):
        raise EvidenceError("build log source identity is invalid")
    expected_names = {
        "source.tar",
        "dist.tar",
        "dist-manifest.json",
        "npm-build.log",
        "cargo-build.jsonl",
    }
    for value, label in (
        (source["archiveFile"], "buildLog.source.archiveFile"),
        (ui["archiveFile"], "buildLog.ui.archiveFile"),
        (ui["manifestFile"], "buildLog.ui.manifestFile"),
        (outputs["npmFile"], "buildLog.outputs.npmFile"),
        (outputs["cargoFile"], "buildLog.outputs.cargoFile"),
    ):
        if not isinstance(value, str) or value not in expected_names:
            raise EvidenceError(f"{label} is not an exact evidence filename")
    for value, label in (
        (source["archiveSha256"], "buildLog.source.archiveSha256"),
        (ui["distSha256"], "buildLog.ui.distSha256"),
        (ui["archiveSha256"], "buildLog.ui.archiveSha256"),
        (ui["manifestSha256"], "buildLog.ui.manifestSha256"),
        (outputs["npmSha256"], "buildLog.outputs.npmSha256"),
        (outputs["cargoSha256"], "buildLog.outputs.cargoSha256"),
    ):
        if not isinstance(value, str) or not SHA_RE.fullmatch(value):
            raise EvidenceError(f"{label} must be lowercase SHA-256")
    file_bindings = {
        source["archiveFile"]: source["archiveSha256"],
        ui["archiveFile"]: ui["archiveSha256"],
        ui["manifestFile"]: ui["manifestSha256"],
        outputs["npmFile"]: outputs["npmSha256"],
        outputs["cargoFile"]: outputs["cargoSha256"],
    }
    if set(file_bindings) != EVIDENCE_FILES - {"build-log.json"}:
        raise EvidenceError("build log evidence filenames are not exact")
    for name, expected_sha in file_bindings.items():
        if not isinstance(expected_sha, str) or not SHA_RE.fullmatch(expected_sha):
            raise EvidenceError(f"build log digest is invalid for {name}")
        if sha256_file(directory / name) != expected_sha:
            raise EvidenceError(f"build log digest differs for {name}")
    with tarfile.open(directory / "source.tar", "r:") as archive:
        files = {
            safe_member_name(member.name, "source archive")
            for member in archive.getmembers()
            if member.isfile()
        }
        if not REQUIRED_SOURCE_PATHS.issubset(files):
            raise EvidenceError("source archive is not an OSL product source archive")
        if archive.pax_headers.get("comment") != source["commit"]:
            raise EvidenceError("source archive commit differs from build log")
        if git_tree_from_archive(archive) != source["tree"]:
            raise EvidenceError("source archive bytes do not reproduce the build-log tree")
    manifest = exact_object(
        load_json(directory / "dist-manifest.json", "dist manifest"),
        {"schemaVersion", "files"},
        "distManifest",
    )
    if type(manifest["schemaVersion"]) is not int or manifest["schemaVersion"] != 2:
        raise EvidenceError("dist manifest schemaVersion must be exactly 2")
    if not isinstance(manifest["files"], list) or not manifest["files"]:
        raise EvidenceError("dist manifest files must be a nonempty array")
    entries: list[dict[str, Any]] = []
    seen: set[str] = set()
    for index, raw in enumerate(manifest["files"]):
        entry = exact_object(raw, {"path", "sha256", "sizeBytes"}, f"distManifest.files[{index}]")
        if not isinstance(entry["path"], str):
            raise EvidenceError("dist manifest path must be a string")
        name = safe_member_name(entry["path"], "dist manifest")
        if name in seen:
            raise EvidenceError("dist manifest contains a duplicate path")
        seen.add(name)
        if not isinstance(entry["sha256"], str) or not SHA_RE.fullmatch(entry["sha256"]):
            raise EvidenceError("dist manifest sha256 must be lowercase SHA-256")
        if type(entry["sizeBytes"]) is not int or entry["sizeBytes"] < 0:
            raise EvidenceError("dist manifest sizeBytes must be a nonnegative integer")
        entries.append(entry)
    if entries != sorted(entries, key=lambda entry: entry["path"]):
        raise EvidenceError("dist manifest is not path-sorted")
    verify_dist_tar(directory / "dist.tar", entries)
    if ui["distSha256"] != dist_digest(entries):
        raise EvidenceError("build log dist digest differs from retained dist bytes")
    cargo_artifact = Path(parse_cargo_artifact(directory / "cargo-build.jsonl")).resolve()
    if cargo_artifact != exe_path.resolve():
        raise EvidenceError("retained cargo artifact path differs from independent executable")
    if (
        not isinstance(artifact["path"], str)
        or not artifact["path"].endswith(
            "/x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
        )
        or Path(cargo_artifact).name != PurePosixPath(artifact["path"]).name
        or not isinstance(artifact["sha256"], str)
        or not SHA_RE.fullmatch(artifact["sha256"])
        or artifact["sha256"] != sha256_file(exe_path)
        or type(artifact["sizeBytes"]) is not int
        or artifact["sizeBytes"] != exe_path.stat().st_size
    ):
        raise EvidenceError("build log artifact does not bind the independent executable")
    evidence = exact_object(
        identity["evidence"],
        {
            "sourceArchiveSha256",
            "distArchiveSha256",
            "distManifestSha256",
            "npmBuildLogSha256",
            "cargoBuildLogSha256",
            "buildLogSha256",
        },
        "buildIdentity.evidence",
    )
    expected_evidence = {
        "sourceArchiveSha256": sha256_file(directory / "source.tar"),
        "distArchiveSha256": sha256_file(directory / "dist.tar"),
        "distManifestSha256": sha256_file(directory / "dist-manifest.json"),
        "npmBuildLogSha256": sha256_file(directory / "npm-build.log"),
        "cargoBuildLogSha256": sha256_file(directory / "cargo-build.jsonl"),
        "buildLogSha256": sha256_file(directory / "build-log.json"),
    }
    if evidence != expected_evidence:
        raise EvidenceError("build identity differs from retained build evidence hashes")
    if identity["source"] != {
        "commit": source["commit"],
        "tree": source["tree"],
        "clean": source["clean"],
        "dirtyFingerprint": source["dirtyFingerprint"],
    }:
        raise EvidenceError("build identity source differs from build log")
    if identity["ui"]["distSha256"] != ui["distSha256"]:
        raise EvidenceError("build identity dist digest differs from build log")
    if identity["build"]["commands"] != [
        EXPECTED_COMMANDS[0],
        EXPECTED_COMMANDS[1][:-1],
    ]:
        raise EvidenceError("build identity argv differs from build log")
    if identity["build"]["toolchain"] != toolchain:
        raise EvidenceError("build identity toolchain differs from build log")
    if identity["artifacts"]["executable"] != {
        "name": "osl-privacy-hub.exe",
        "sha256": artifact["sha256"],
        "sizeBytes": artifact["sizeBytes"],
    }:
        raise EvidenceError("build identity executable differs from build log")
    return log


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create")
    create.add_argument("--source-repo", required=True)
    create.add_argument("--dist", required=True)
    create.add_argument("--exe", required=True)
    create.add_argument("--loader", required=True)
    create.add_argument("--npm-log", required=True)
    create.add_argument("--cargo-log", required=True)
    create.add_argument("--output", required=True)
    create.add_argument("--expected-commit", required=True)
    create.add_argument("--expected-tree", required=True)
    create.set_defaults(function=create_evidence)
    args = parser.parse_args()
    try:
        args.function(args)
    except (EvidenceError, OSError, tarfile.TarError, subprocess.CalledProcessError) as exc:
        print(f"VMQA BUILD EVIDENCE INVALID: {exc}", file=sys.stderr)
        return 9
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
