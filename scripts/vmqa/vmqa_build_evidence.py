#!/usr/bin/env python3
"""Create and verify retained evidence for an exact Windows VMQA build."""

from __future__ import annotations

import argparse
import ctypes
import errno
import fcntl
import hashlib
import json
import os
import pwd
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path, PurePosixPath
from typing import Any


SCHEMA_VERSION = 2
PINNED_COMMIT = "1f745c85bb23cf79a956aa87d623905e20f83cf1"
PINNED_TREE = "1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a"
PINNED_LOADER_SOURCE = Path(
    os.environ.get(
        "OSL_VMQA_WEBVIEW2_LOADER",
        "/mnt/c/OSL-Scrub-Demo/WebView2Loader.dll",
    )
)
PINNED_LOADER_SHA256 = (
    "8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c"
)
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
TARGET_ARTIFACT_PATH = "x86_64-pc-windows-gnu/release/osl-privacy-hub.exe"
NPM_SETUP_COMMAND = ["npm", "ci", "--no-audit", "--no-fund"]
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
IDENTITY_COMMANDS = [EXPECTED_COMMANDS[0], EXPECTED_COMMANDS[1][:-1]]
FINAL_EXE = "outputs/osl-privacy-hub.exe"
FINAL_LOADER = "outputs/WebView2Loader.dll"
FINAL_DIST = "outputs/dist"
FINAL_EVIDENCE = "build-evidence"
PRODUCTION_SEAL_DIRECTORY = Path("/var/lib/osl-vmqa/producer-seals")
PRODUCTION_SEAL_USER = "osl-vmqa-producer"
SEAL_SCHEMA_VERSION = 2
SEAL_STATE_SCHEMA_VERSION = 1
PRODUCTION_SEAL_STATE = PRODUCTION_SEAL_DIRECTORY / ".seal-chain-state.json"
PRODUCTION_SEAL_LOCK = PRODUCTION_SEAL_DIRECTORY / ".seal-chain.lock"
PRODUCTION_HOME = Path(os.environ.get("OSL_VMQA_PRODUCTION_HOME", "/home/osl-vmqa"))
FIXTURE_LOADER_BYTES = b"fixture-WebView2Loader-produced-by-vmqa\n"
EVIDENCE_FILES = {
    "source.tar",
    "dist.tar",
    "dist-manifest.json",
    "npm-ci.log",
    "npm-ci.stderr",
    "npm-build.log",
    "npm-build.stderr",
    "cargo-build.jsonl",
    "cargo-build.stderr",
    "WebView2Loader.dll",
    "build-log.json",
}
REQUIRED_SOURCE_PATHS = {
    "Cargo.toml",
    "apps/osl-hub/Cargo.toml",
    "apps/osl-hub-ui/package.json",
}
REQUIRED_PRODUCT_MARKERS = {
    "apps/osl-hub/Cargo.toml": re.compile(rb'name\s*=\s*"osl-privacy-hub"')
}
PRODUCTION_TOOL_PINS = {
    "git": (
        Path("/usr/bin/git"),
        "2a8c18fbf43da9f692d75474c72bea9dfd796c260b0f3dfe456376abc3bbd668",
    ),
    "npm": (
        PRODUCTION_HOME / ".nvm/versions/node/v24.14.0/bin/npm",
        "8e5f6f3429f8cdbe693cdc29904e9d5a7b127a494bd15c804bd54c7403bfcbe7",
    ),
    "node": (
        PRODUCTION_HOME / ".nvm/versions/node/v24.14.0/bin/node",
        "e237a2839d0cbdc9a9a2adda1a184afc0f5b20306ffbe923af5686550472d8a8",
    ),
    "osl-cargo": (
        PRODUCTION_HOME / ".local/bin/osl-cargo",
        "b27c3f5c974184f086fcb24da95d6397c333155f2ab2e5e638eafdac57da600a",
    ),
    "rustc": (
        PRODUCTION_HOME / ".cargo/bin/rustc",
        "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10",
    ),
    "cargo": (
        PRODUCTION_HOME / ".cargo/bin/cargo",
        "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10",
    ),
}
FIXTURE_TOOL_HASHES = {
    "git": "2a8c18fbf43da9f692d75474c72bea9dfd796c260b0f3dfe456376abc3bbd668",
    "npm": "98e96abb7d7cb4d4ab4ec48c28ea8c9b554dd36f5516781a2153de7656c63738",
    "node": "cb67aa7b5cab36272fe17bc6cf971c73545da2f4cc97669dcde9f32dc1e72bdf",
    "osl-cargo": "50523faf76d2cf9ac9e8b0e1f1e7821e574ec4278b5d82147d0263863227e0a1",
    "rustc": "3fff206f83d8b45a2c637a02e4ff1bd39cefbaf311d3f97e496dc0e974ac0196",
    "cargo": "2e9339e58bac8d36b1312ea8fbcab6c045eb401ac4fc0842ff64a9bab88b1afe",
    "osl-cargo-outside": "31148061d99f824e925c5f274585bf2ba3be00d025af8604f8b4c07eb842767d",
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


def load_json_with_sha(path: Path, label: str) -> tuple[Any, str]:
    try:
        payload = path.read_bytes()
        return (
            json.loads(payload.decode("utf-8")),
            hashlib.sha256(payload).hexdigest(),
        )
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise EvidenceError(f"{label} is not valid UTF-8 JSON: {exc}") from exc


def load_json(path: Path, label: str) -> Any:
    return load_json_with_sha(path, label)[0]


def fixture_seal_path(bundle: Path) -> Path:
    return bundle.with_name(f"{bundle.name}.producer-seal.json")


def require_real_directory(path: Path, label: str) -> os.stat_result:
    try:
        value = os.lstat(path)
    except OSError as exc:
        raise EvidenceError(f"{label} is missing: {path}") from exc
    if not stat.S_ISDIR(value.st_mode) or stat.S_ISLNK(value.st_mode):
        raise EvidenceError(f"{label} must be a real directory: {path}")
    cursor = Path(path.anchor)
    for part in path.parts[1:]:
        cursor /= part
        if cursor.is_symlink():
            raise EvidenceError(f"{label} contains a symlink component: {cursor}")
    return value


def production_seal_owner(*, for_write: bool) -> int:
    try:
        producer = pwd.getpwnam(PRODUCTION_SEAL_USER)
    except KeyError as exc:
        raise EvidenceError(
            f"dedicated producer account is not provisioned: {PRODUCTION_SEAL_USER}"
        ) from exc
    if Path(producer.pw_shell).name not in {"false", "nologin"}:
        raise EvidenceError("dedicated producer account must be non-login")
    directory_stat = require_real_directory(
        PRODUCTION_SEAL_DIRECTORY, "protected producer-seal directory"
    )
    if directory_stat.st_uid != producer.pw_uid:
        raise EvidenceError(
            "protected producer-seal directory is not owned by the dedicated producer"
        )
    if stat.S_IMODE(directory_stat.st_mode) != 0o755:
        raise EvidenceError(
            "protected producer-seal directory mode must be exactly 0755"
        )
    if for_write and os.geteuid() != producer.pw_uid:
        raise EvidenceError(
            "production create must run as the dedicated producer identity"
        )
    return producer.pw_uid


def producer_seal_record(
    identity_sha: str,
    mode: str,
    *,
    generation: int = 1,
    previous_seal_sha256: str = EMPTY_SHA256,
    transition: str | None = None,
) -> dict[str, Any]:
    if not SHA_RE.fullmatch(identity_sha):
        raise EvidenceError("producer-seal identity digest is invalid")
    if type(generation) is not int or generation < 1:
        raise EvidenceError("producer-seal generation must be a positive integer")
    if not SHA_RE.fullmatch(previous_seal_sha256):
        raise EvidenceError("producer-seal predecessor digest is invalid")
    expected_transition = "initial" if generation == 1 else "successor"
    if transition is None:
        transition = expected_transition
    if transition != expected_transition:
        raise EvidenceError("producer-seal transition is not permitted")
    if generation == 1 and previous_seal_sha256 != EMPTY_SHA256:
        raise EvidenceError("initial producer seal has a predecessor")
    if generation > 1 and previous_seal_sha256 == EMPTY_SHA256:
        raise EvidenceError("successor producer seal lacks its predecessor")
    return {
        "schemaVersion": SEAL_SCHEMA_VERSION,
        "producer": (
            "internal-vmqa-fixture"
            if mode == "fixture"
            else PRODUCTION_SEAL_USER
        ),
        "mode": mode,
        "identitySha256": identity_sha,
        "sourceCommit": PINNED_COMMIT,
        "sourceTree": PINNED_TREE,
        "generation": generation,
        "previousSealSha256": previous_seal_sha256,
        "transition": transition,
    }


def validate_producer_seal_record(
    value: Any, identity_sha: str, mode: str
) -> dict[str, Any]:
    seal = exact_object(
        value,
        {
            "schemaVersion",
            "producer",
            "mode",
            "identitySha256",
            "sourceCommit",
            "sourceTree",
            "generation",
            "previousSealSha256",
            "transition",
        },
        "producerSeal",
    )
    expected = producer_seal_record(
        identity_sha,
        mode,
        generation=seal["generation"],
        previous_seal_sha256=seal["previousSealSha256"],
        transition=seal["transition"],
    )
    if seal != expected:
        raise EvidenceError(
            "producer-authenticated seal does not bind the retained build identity"
        )
    return seal


def seal_state_record(
    generation: int, identity_sha: str, seal_sha: str
) -> dict[str, Any]:
    if type(generation) is not int or generation < 1:
        raise EvidenceError("producer-seal state generation is invalid")
    if not SHA_RE.fullmatch(identity_sha) or not SHA_RE.fullmatch(seal_sha):
        raise EvidenceError("producer-seal state digest is invalid")
    return {
        "schemaVersion": SEAL_STATE_SCHEMA_VERSION,
        "generation": generation,
        "identitySha256": identity_sha,
        "sealSha256": seal_sha,
    }


def read_production_seal_state(
    owner_uid: int, *, required: bool
) -> dict[str, Any] | None:
    try:
        value = os.lstat(PRODUCTION_SEAL_STATE)
    except FileNotFoundError:
        if required:
            raise EvidenceError("producer-seal monotonic state is missing")
        return None
    except OSError as exc:
        raise EvidenceError("producer-seal monotonic state is unavailable") from exc
    if (
        not stat.S_ISREG(value.st_mode)
        or stat.S_ISLNK(value.st_mode)
        or value.st_uid != owner_uid
        or stat.S_IMODE(value.st_mode) != 0o600
    ):
        raise EvidenceError("producer-seal monotonic state has unsafe ownership or mode")
    state = exact_object(
        load_json(PRODUCTION_SEAL_STATE, "producer-seal monotonic state"),
        {"schemaVersion", "generation", "identitySha256", "sealSha256"},
        "producerSealState",
    )
    if state != seal_state_record(
        state["generation"], state["identitySha256"], state["sealSha256"]
    ):
        raise EvidenceError("producer-seal monotonic state is not canonical")
    return state


def open_production_seal_lock(owner_uid: int) -> int:
    flags = os.O_RDWR | os.O_CREAT
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(PRODUCTION_SEAL_LOCK, flags, 0o600)
    except OSError as exc:
        raise EvidenceError("producer-seal lock is unavailable") from exc
    try:
        value = os.fstat(descriptor)
        if (
            not stat.S_ISREG(value.st_mode)
            or value.st_uid != owner_uid
            or stat.S_IMODE(value.st_mode) != 0o600
        ):
            raise EvidenceError("producer-seal lock has unsafe ownership or mode")
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def replace_production_seal_state(
    state: dict[str, Any], owner_uid: int
) -> None:
    parent_fd = os.open(
        PRODUCTION_SEAL_DIRECTORY,
        os.O_RDONLY | getattr(os, "O_DIRECTORY", 0),
    )
    temporary_fd, temporary_name = tempfile.mkstemp(
        prefix=".vmqa-seal-state-", dir=PRODUCTION_SEAL_DIRECTORY
    )
    temporary = Path(temporary_name)
    try:
        payload = (
            json.dumps(state, sort_keys=True, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        os.fchmod(temporary_fd, 0o600)
        with os.fdopen(temporary_fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        temporary_stat = os.lstat(temporary)
        if temporary_stat.st_uid != owner_uid:
            raise EvidenceError("temporary producer-seal state has the wrong owner")
        if os.path.lexists(PRODUCTION_SEAL_STATE):
            existing = os.lstat(PRODUCTION_SEAL_STATE)
            if (
                not stat.S_ISREG(existing.st_mode)
                or stat.S_ISLNK(existing.st_mode)
                or existing.st_uid != owner_uid
                or stat.S_IMODE(existing.st_mode) != 0o600
            ):
                raise EvidenceError(
                    "producer-seal monotonic state has unsafe ownership or mode"
                )
        os.replace(temporary, PRODUCTION_SEAL_STATE)
        os.fsync(parent_fd)
        observed = read_production_seal_state(owner_uid, required=True)
        if observed != state:
            raise EvidenceError("published producer-seal state changed")
    finally:
        os.close(parent_fd)
        if temporary.exists():
            temporary.unlink()


def read_seal_bytes(path: Path, expected_owner: int | None) -> bytes:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        before = os.lstat(path)
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise EvidenceError("producer-authenticated seal is unavailable") from exc
    try:
        opened = os.fstat(descriptor)
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or not stat.S_ISREG(opened.st_mode)
            or stat.S_IMODE(opened.st_mode) != 0o444
            or (
                expected_owner is not None
                and opened.st_uid != expected_owner
            )
            or (before.st_dev, before.st_ino)
            != (opened.st_dev, opened.st_ino)
        ):
            raise EvidenceError(
                "producer-authenticated seal has unsafe ownership or mode"
            )
        chunks: list[bytes] = []
        while True:
            block = os.read(descriptor, 1024 * 1024)
            if not block:
                break
            chunks.append(block)
        payload = b"".join(chunks)
        after = os.lstat(path)
        if (
            (after.st_dev, after.st_ino)
            != (opened.st_dev, opened.st_ino)
            or after.st_mode != opened.st_mode
        ):
            raise EvidenceError("producer-authenticated seal changed while read")
        return payload
    finally:
        os.close(descriptor)


def validate_production_seal_chain(
    seal: dict[str, Any], seal_sha: str, owner_uid: int
) -> None:
    current = seal
    current_sha = seal_sha
    seen: set[str] = set()
    while current["generation"] > 1:
        previous_sha = current["previousSealSha256"]
        if previous_sha in seen or current_sha in seen:
            raise EvidenceError("producer-seal chain contains a cycle")
        seen.add(current_sha)
        matches: list[tuple[dict[str, Any], str]] = []
        for candidate in PRODUCTION_SEAL_DIRECTORY.iterdir():
            if re.fullmatch(r"[0-9a-f]{64}\.json", candidate.name) is None:
                continue
            try:
                candidate_bytes = read_seal_bytes(candidate, owner_uid)
            except EvidenceError:
                continue
            candidate_sha = hashlib.sha256(candidate_bytes).hexdigest()
            if candidate_sha != previous_sha:
                continue
            try:
                candidate_value = json.loads(candidate_bytes.decode("utf-8"))
                candidate_identity = candidate.name.removesuffix(".json")
                candidate_record = validate_producer_seal_record(
                    candidate_value, candidate_identity, "production"
                )
            except (UnicodeError, json.JSONDecodeError, EvidenceError):
                continue
            matches.append((candidate_record, candidate_sha))
        if len(matches) != 1:
            raise EvidenceError(
                "producer-seal predecessor is missing or ambiguous"
            )
        predecessor, predecessor_sha = matches[0]
        if predecessor["generation"] != current["generation"] - 1:
            raise EvidenceError(
                "producer-seal generation transition is not contiguous"
            )
        current = predecessor
        current_sha = predecessor_sha
    if (
        current["generation"] != 1
        or current["transition"] != "initial"
        or current["previousSealSha256"] != EMPTY_SHA256
    ):
        raise EvidenceError("producer-seal chain lacks a permitted genesis")


def verify_producer_seal(
    identity_path: Path,
    *,
    mode: str,
    allow_fixture: bool = False,
    fixture_seal: Path | None = None,
    identity_sha: str | None = None,
) -> Path:
    if mode not in ("production", "fixture"):
        raise EvidenceError("producer-seal mode is not exact")
    if identity_sha is None:
        identity_sha = sha256_file(identity_path)
    if not SHA_RE.fullmatch(identity_sha):
        raise EvidenceError("producer-seal identity digest is invalid")
    if mode == "fixture":
        if not allow_fixture:
            raise EvidenceError("fixture producer seal is forbidden in production")
        if fixture_seal is None:
            raise EvidenceError("internal fixture validation requires its detached producer seal")
        seal_path = fixture_seal
        expected_owner = None
        expected_mode = 0o444
    else:
        if fixture_seal is not None:
            raise EvidenceError("caller-selected producer seal is forbidden in production")
        expected_owner = production_seal_owner(for_write=False)
        expected_mode = 0o444
        seal_path = PRODUCTION_SEAL_DIRECTORY / f"{identity_sha}.json"
    try:
        seal_stat = os.lstat(seal_path)
    except OSError as exc:
        raise EvidenceError(
            f"producer-authenticated seal is missing for identity {identity_sha}"
        ) from exc
    if (
        not stat.S_ISREG(seal_stat.st_mode)
        or stat.S_ISLNK(seal_stat.st_mode)
        or stat.S_IMODE(seal_stat.st_mode) != expected_mode
        or (expected_owner is not None and seal_stat.st_uid != expected_owner)
    ):
        raise EvidenceError("producer-authenticated seal has unsafe ownership or mode")
    seal_bytes = read_seal_bytes(seal_path, expected_owner)
    try:
        seal_value = json.loads(seal_bytes.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise EvidenceError("producer seal is not valid UTF-8 JSON") from exc
    seal = validate_producer_seal_record(seal_value, identity_sha, mode)
    if mode == "production":
        assert expected_owner is not None
        state = read_production_seal_state(expected_owner, required=True)
        assert state is not None
        seal_sha = hashlib.sha256(seal_bytes).hexdigest()
        if state != seal_state_record(
            seal["generation"], identity_sha, seal_sha
        ):
            raise EvidenceError(
                "producer seal is not the current monotonic generation"
            )
        validate_production_seal_chain(seal, seal_sha, expected_owner)
    return seal_path


def publish_producer_seal(
    identity_path: Path,
    bundle_output: Path,
    *,
    fixture: bool,
) -> tuple[Path, tuple[int, int]]:
    mode = "fixture" if fixture else "production"
    identity_sha = sha256_file(identity_path)
    lock_fd: int | None = None
    parent_fd: int | None = None
    temporary: Path | None = None
    published_seal: Path | None = None
    published_inode: tuple[int, int] | None = None
    state_committed = False
    try:
        if fixture:
            record = producer_seal_record(identity_sha, mode)
            seal_path = fixture_seal_path(bundle_output)
            parent = bundle_output.parent
            owner_uid = os.geteuid()
        else:
            owner_uid = production_seal_owner(for_write=True)
            lock_fd = open_production_seal_lock(owner_uid)
            state = read_production_seal_state(owner_uid, required=False)
            generation = 1 if state is None else state["generation"] + 1
            previous_sha = EMPTY_SHA256 if state is None else state["sealSha256"]
            record = producer_seal_record(
                identity_sha,
                mode,
                generation=generation,
                previous_seal_sha256=previous_sha,
            )
            seal_path = PRODUCTION_SEAL_DIRECTORY / f"{identity_sha}.json"
            parent = PRODUCTION_SEAL_DIRECTORY
        if os.path.lexists(seal_path):
            raise EvidenceError(f"producer seal already exists: {seal_path}")
        flags = os.O_RDONLY | os.O_DIRECTORY
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        parent_fd = os.open(parent, flags)
        temporary_fd, temporary_name = tempfile.mkstemp(
            prefix=".vmqa-producer-seal-", dir=parent
        )
        temporary = Path(temporary_name)
        payload = (
            json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n"
        ).encode("utf-8")
        with os.fdopen(temporary_fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        temporary.chmod(0o444)
        temporary_stat = os.stat(temporary, follow_symlinks=False)
        inode = (temporary_stat.st_dev, temporary_stat.st_ino)
        rename_noreplace(temporary, parent_fd, seal_path.name)
        published_seal = seal_path
        published_inode = inode
        final_stat = os.stat(seal_path, follow_symlinks=False)
        if (final_stat.st_dev, final_stat.st_ino) != inode:
            raise EvidenceError("published producer seal path was swapped")
        if not fixture:
            seal_sha = hashlib.sha256(payload).hexdigest()
            replace_production_seal_state(
                seal_state_record(record["generation"], identity_sha, seal_sha),
                owner_uid,
            )
            state_committed = True
            verify_producer_seal(
                identity_path,
                mode="production",
                identity_sha=identity_sha,
            )
        return seal_path, inode
    except BaseException:
        if (
            published_seal is not None
            and published_inode is not None
            and (fixture or not state_committed)
        ):
            try:
                current = os.lstat(published_seal)
                if (
                    stat.S_ISREG(current.st_mode)
                    and not stat.S_ISLNK(current.st_mode)
                    and (current.st_dev, current.st_ino) == published_inode
                ):
                    published_seal.unlink()
            except OSError:
                pass
        raise
    finally:
        if parent_fd is not None:
            os.close(parent_fd)
        if lock_fd is not None:
            fcntl.flock(lock_fd, fcntl.LOCK_UN)
            os.close(lock_fd)
        if temporary is not None and temporary.exists():
            temporary.unlink()


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
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise EvidenceError(f"dist contains a symlink: {path}")
        if not path.is_file():
            continue
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


def git_environment() -> dict[str, str]:
    return {
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1",
        "HOME": "/nonexistent",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "PATH": "/usr/bin:/bin",
    }


def git_output(repository: Path, git_path: Path, *args: str) -> str:
    result = subprocess.run(
        [str(git_path), "-C", str(repository), *args],
        env=git_environment(),
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise EvidenceError(result.stderr.strip() or "git command failed")
    return result.stdout.strip()


def require_new_bundle_path(raw_path: str) -> tuple[Path, int]:
    output = Path(os.path.abspath(raw_path))
    if os.path.lexists(output):
        raise EvidenceError(f"bundle destination already exists: {output}")
    parent = output.parent
    if not parent.is_dir():
        raise EvidenceError(f"bundle parent must already exist: {parent}")
    cursor = Path(output.anchor)
    for part in output.parts[1:-1]:
        cursor /= part
        if cursor.is_symlink():
            raise EvidenceError(f"bundle path contains a symlink component: {cursor}")
    flags = os.O_RDONLY | os.O_DIRECTORY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    parent_fd = os.open(parent, flags)
    parent_stat = os.stat(parent, follow_symlinks=False)
    opened_stat = os.fstat(parent_fd)
    if (parent_stat.st_dev, parent_stat.st_ino) != (
        opened_stat.st_dev,
        opened_stat.st_ino,
    ):
        os.close(parent_fd)
        raise EvidenceError("bundle parent changed while it was opened")
    return output, parent_fd


def rename_noreplace(source: Path, parent_fd: int, destination_name: str) -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    renameat2 = getattr(libc, "renameat2", None)
    if renameat2 is None:
        raise EvidenceError("atomic no-overwrite publication requires renameat2")
    renameat2.argtypes = [
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_uint,
    ]
    renameat2.restype = ctypes.c_int
    result = renameat2(
        -100,
        os.fsencode(source),
        parent_fd,
        os.fsencode(destination_name),
        1,
    )
    if result != 0:
        code = ctypes.get_errno()
        if code == errno.EEXIST:
            raise EvidenceError("bundle destination appeared before atomic publication")
        raise OSError(code, os.strerror(code))


def extract_source_tar(source_tar: Path, scratch: Path) -> None:
    with tarfile.open(source_tar, "r:") as archive:
        for member in archive.getmembers():
            name = safe_member_name(member.name, "source archive")
            destination = scratch / name
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
                continue
            if not member.isfile():
                raise EvidenceError("source archive contains an unsupported extraction member")
            destination.parent.mkdir(parents=True, exist_ok=True)
            data = archive.extractfile(member)
            if data is None:
                raise EvidenceError(f"source archive member cannot be read: {name}")
            destination.write_bytes(data.read())
            destination.chmod(member.mode & 0o777)


def archive_pinned_source(provider: Path, destination: Path, git_path: Path) -> None:
    try:
        provider_commit = git_output(
            provider, git_path, "rev-parse", f"{PINNED_COMMIT}^{{commit}}"
        )
    except EvidenceError as exc:
        raise EvidenceError("source provider lacks the immutable pinned commit") from exc
    if provider_commit != PINNED_COMMIT:
        raise EvidenceError("source provider lacks the immutable pinned commit")
    if (
        git_output(provider, git_path, "rev-parse", f"{PINNED_COMMIT}^{{tree}}")
        != PINNED_TREE
    ):
        raise EvidenceError("source provider pinned tree differs from immutable pin")
    subprocess.run(
        [
            str(git_path),
            "-C",
            str(provider),
            "archive",
            "--format=tar",
            "-o",
            str(destination),
            PINNED_COMMIT,
        ],
        env=git_environment(),
        check=True,
    )
    with tarfile.open(destination, "r:") as archive:
        files = {
            safe_member_name(member.name, "source archive")
            for member in archive.getmembers()
            if member.isfile()
        }
        if not REQUIRED_SOURCE_PATHS.issubset(files):
            raise EvidenceError(
                f"source archive lacks product files: {sorted(REQUIRED_SOURCE_PATHS - files)}"
            )
        # Paths alone are not provenance: a clean look-alike repository can
        # reproduce the same tree shape and feed an arbitrary executable.
        # Require the pinned product package marker in the archived bytes.
        for marker_path, marker in REQUIRED_PRODUCT_MARKERS.items():
            member = archive.extractfile(marker_path)
            if member is None or marker.search(member.read()) is None:
                raise EvidenceError("source archive is not the pinned OSL product")
        if archive.pax_headers.get("comment") != PINNED_COMMIT:
            raise EvidenceError("source archive does not carry the named Git commit")
        if git_tree_from_archive(archive) != PINNED_TREE:
            raise EvidenceError("source archive bytes do not reproduce the named Git tree")


def run_command(
    argv: list[str], cwd: Path, environment: dict[str, str]
) -> tuple[bytes, bytes]:
    result = subprocess.run(
        argv,
        cwd=cwd,
        env=environment,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise EvidenceError(f"build command failed ({result.returncode}): {argv!r}")
    return result.stdout, result.stderr


def selected_tool_pins(
    fixture: bool, fixture_scenario: str = "valid"
) -> dict[str, tuple[Path, str]]:
    if not fixture:
        return PRODUCTION_TOOL_PINS
    fixture_dir = Path(__file__).resolve().parent / "fixtures" / "fake-build-tools"
    cargo_tool = (
        "osl-cargo-outside" if fixture_scenario == "outside-artifact" else "osl-cargo"
    )
    return {
        "git": (Path("/usr/bin/git"), FIXTURE_TOOL_HASHES["git"]),
        "osl-cargo": (
            fixture_dir / cargo_tool,
            FIXTURE_TOOL_HASHES[cargo_tool],
        ),
        **{
            name: (fixture_dir / name, FIXTURE_TOOL_HASHES[name])
            for name in ("npm", "node", "rustc", "cargo")
        },
    }


def validate_tools(
    fixture: bool, fixture_scenario: str = "valid"
) -> dict[str, dict[str, str]]:
    tools: dict[str, dict[str, str]] = {}
    for name, (invoked, expected_sha) in selected_tool_pins(
        fixture, fixture_scenario
    ).items():
        if not invoked.is_file():
            raise EvidenceError(f"pinned {name} tool is missing: {invoked}")
        resolved = invoked.resolve(strict=True)
        observed_sha = sha256_file(resolved)
        if observed_sha != expected_sha:
            raise EvidenceError(
                f"pinned {name} tool changed: expected {expected_sha}, got {observed_sha}"
            )
        tools[name] = {
            "name": name,
            "invokedPath": invoked.as_posix(),
            "resolvedPath": resolved.as_posix(),
            "sha256": observed_sha,
        }
    return tools


def build_environments(
    tools: dict[str, dict[str, str]],
    work: Path,
    cargo_target: Path,
) -> tuple[dict[str, str], dict[str, str]]:
    path_dirs: list[str] = []
    for name in ("node", "cargo", "rustc", "osl-cargo", "npm"):
        directory = str(Path(tools[name]["invokedPath"]).parent)
        if directory not in path_dirs:
            path_dirs.append(directory)
    for directory in ("/usr/bin", "/bin"):
        if directory not in path_dirs:
            path_dirs.append(directory)
    production_home = PRODUCTION_HOME.as_posix()
    production_user = os.environ.get("OSL_VMQA_PRODUCTION_USER", "osl-vmqa")
    common = {
        "HOME": production_home,
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "LOGNAME": production_user,
        "PATH": ":".join(path_dirs),
        "USER": production_user,
    }
    npm_environment = {
        **common,
        "npm_config_audit": "false",
        "npm_config_cache": (work / "npm-cache").as_posix(),
        "npm_config_fund": "false",
    }
    cargo_environment = {
        **common,
        "CARGO_HOME": (PRODUCTION_HOME / ".cargo").as_posix(),
        "CARGO_INCREMENTAL": "1",
        "CARGO_TARGET_DIR": cargo_target.as_posix(),
        "OSL_CARGO_JOBS": "3",
        "OSL_CARGO_MAXLOAD": "24",
        "OSL_CARGO_MIN_HOST_FREE_GB": "50",
        "RUSTUP_HOME": (PRODUCTION_HOME / ".rustup").as_posix(),
    }
    return npm_environment, cargo_environment


def read_loader_once(fixture: bool) -> tuple[bytes, str]:
    if fixture:
        return FIXTURE_LOADER_BYTES, "fixture:embedded"
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(PINNED_LOADER_SOURCE, flags)
    except OSError as exc:
        raise EvidenceError("immutable VMQA WebView2 loader is missing") from exc
    with os.fdopen(descriptor, "rb") as handle:
        data = handle.read()
    if hashlib.sha256(data).hexdigest() != PINNED_LOADER_SHA256:
        raise EvidenceError("immutable VMQA WebView2 loader changed")
    return data, PINNED_LOADER_SOURCE.as_posix()


def execution_record(
    *,
    logical_argv: list[str],
    actual_argv: list[str],
    cwd: str,
    environment: dict[str, str],
    stdout_file: str,
    stderr_file: str,
    evidence: Path,
) -> dict[str, Any]:
    return {
        "logicalArgv": logical_argv,
        "argv": actual_argv,
        "cwd": cwd,
        "environment": environment,
        "exitCode": 0,
        "stdoutFile": stdout_file,
        "stdoutSha256": sha256_file(evidence / stdout_file),
        "stderrFile": stderr_file,
        "stderrSha256": sha256_file(evidence / stderr_file),
    }


def make_identity(log: dict[str, Any], evidence: Path) -> dict[str, Any]:
    return {
        "schemaVersion": 2,
        "source": {
            "commit": log["source"]["commit"],
            "tree": log["source"]["tree"],
            "clean": True,
            "dirtyFingerprint": EMPTY_SHA256,
        },
        "ui": {
            "path": FINAL_DIST,
            "distSha256": log["ui"]["distSha256"],
        },
        "build": {
            "target": "x86_64-pc-windows-gnu",
            "features": ["desktop"],
            "profile": "release",
            "commands": IDENTITY_COMMANDS,
            "toolchain": log["toolchain"],
        },
        "artifacts": {
            "executable": {
                "name": "osl-privacy-hub.exe",
                "path": FINAL_EXE,
                "sha256": log["artifact"]["sha256"],
                "sizeBytes": log["artifact"]["sizeBytes"],
            },
            "loader": {
                "name": "WebView2Loader.dll",
                "path": FINAL_LOADER,
                "sha256": log["loader"]["sha256"],
                "sizeBytes": log["loader"]["sizeBytes"],
            },
        },
        "evidence": {
            "directory": FINAL_EVIDENCE,
            "sourceArchiveSha256": sha256_file(evidence / "source.tar"),
            "distArchiveSha256": sha256_file(evidence / "dist.tar"),
            "distManifestSha256": sha256_file(evidence / "dist-manifest.json"),
            "npmBuildLogSha256": sha256_file(evidence / "npm-build.log"),
            "cargoBuildLogSha256": sha256_file(evidence / "cargo-build.jsonl"),
            "buildLogSha256": sha256_file(evidence / "build-log.json"),
        },
    }


def create_evidence(args: argparse.Namespace) -> None:
    forbidden_inputs = {
        name: getattr(args, name)
        for name in (
            "dist",
            "exe",
            "loader",
            "npm_log",
            "cargo_log",
            "expected_commit",
            "expected_tree",
            "exe_destination",
            "dist_destination",
            "loader_destination",
            "shared_target_dir",
        )
        if getattr(args, name) is not None
    }
    if forbidden_inputs:
        raise EvidenceError(
            "caller-authored build inputs are forbidden: "
            + ", ".join(sorted(forbidden_inputs))
        )
    fixture = bool(args.fixture)
    fixture_scenario = args.fixture_scenario
    provider = Path(args.source_repo).resolve()
    if not fixture:
        # Refuse before resolving or executing any build tool unless the fixed
        # producer-only trust store is ready.
        production_seal_owner(for_write=True)
    output, parent_fd = require_new_bundle_path(args.output)
    if fixture and not output.as_posix().startswith("/tmp/"):
        os.close(parent_fd)
        raise EvidenceError("fixture bundles are restricted to /tmp")
    if fixture and os.path.lexists(fixture_seal_path(output)):
        os.close(parent_fd)
        raise EvidenceError(
            f"producer seal destination already exists: {fixture_seal_path(output)}"
        )
    tools = validate_tools(fixture, fixture_scenario)
    loader_bytes, loader_source = read_loader_once(fixture)
    work = Path(tempfile.mkdtemp(prefix=".vmqa-build-", dir=output.parent))
    published = False
    published_inode: tuple[int, int] | None = None
    seal_path: Path | None = None
    seal_inode: tuple[int, int] | None = None
    try:
        bundle = work / "bundle"
        evidence = bundle / FINAL_EVIDENCE
        outputs = bundle / "outputs"
        scratch = work / "scratch"
        cargo_target = work / "cargo-target"
        bundle.mkdir()
        evidence.mkdir()
        outputs.mkdir()
        scratch.mkdir()
        cargo_target.mkdir()
        if any(cargo_target.iterdir()):
            raise EvidenceError("producer-owned Cargo target was not born empty")
        source_tar = evidence / "source.tar"
        archive_pinned_source(provider, source_tar, Path(tools["git"]["invokedPath"]))
        extract_source_tar(source_tar, scratch)
        ui_dir = scratch / "apps/osl-hub-ui"
        hub_dir = scratch / "apps/osl-hub"
        npm_environment, cargo_environment = build_environments(
            tools, work, cargo_target
        )
        npm_ci_argv = [tools["npm"]["invokedPath"], *NPM_SETUP_COMMAND[1:]]
        npm_build_argv = [tools["npm"]["invokedPath"], *EXPECTED_COMMANDS[0][1:]]
        cargo_argv = [
            tools["osl-cargo"]["invokedPath"],
            *EXPECTED_COMMANDS[1][1:],
        ]
        npm_ci_stdout, npm_ci_stderr = run_command(
            npm_ci_argv, ui_dir, npm_environment
        )
        npm_stdout, npm_stderr = run_command(
            npm_build_argv, ui_dir, npm_environment
        )
        cargo_stdout, cargo_stderr = run_command(
            cargo_argv, hub_dir, cargo_environment
        )
        (evidence / "npm-ci.log").write_bytes(npm_ci_stdout)
        (evidence / "npm-ci.stderr").write_bytes(npm_ci_stderr)
        (evidence / "npm-build.log").write_bytes(npm_stdout)
        (evidence / "npm-build.stderr").write_bytes(npm_stderr)
        (evidence / "cargo-build.jsonl").write_bytes(cargo_stdout)
        (evidence / "cargo-build.stderr").write_bytes(cargo_stderr)
        (evidence / "WebView2Loader.dll").write_bytes(loader_bytes)
        if hashlib.sha256((evidence / "WebView2Loader.dll").read_bytes()).hexdigest() != hashlib.sha256(loader_bytes).hexdigest():
            raise EvidenceError("retained loader bytes changed after the single owned read")
        dist = ui_dir / "dist"
        if not dist.is_dir():
            raise EvidenceError("owned npm build did not produce dist directory")
        reported_artifact = Path(
            parse_cargo_artifact(evidence / "cargo-build.jsonl")
        )
        artifact_path = (
            reported_artifact
            if reported_artifact.is_absolute()
            else hub_dir / reported_artifact
        ).resolve()
        canonical_artifact = (cargo_target / TARGET_ARTIFACT_PATH).resolve()
        if artifact_path != canonical_artifact or not canonical_artifact.is_file():
            raise EvidenceError(
                "owned cargo build did not produce the canonical fresh-target executable"
            )
        entries = dist_entries_from_directory(dist)
        (evidence / "dist-manifest.json").write_text(
            json.dumps(
                {"schemaVersion": 2, "files": entries},
                sort_keys=True,
                separators=(",", ":"),
            )
            + "\n",
            encoding="utf-8",
        )
        write_dist_tar(dist, evidence / "dist.tar", entries)
        def version(command: list[str], environment: dict[str, str]) -> str:
            result = subprocess.run(
                command,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )
            if result.returncode != 0 or not result.stdout.strip():
                raise EvidenceError(f"toolchain command failed: {command!r}")
            return result.stdout.strip()

        tool_descriptors = [tools[name] for name in sorted(tools)]
        log = {
            "schemaVersion": 2,
            "mode": "fixture" if fixture else "production",
            "source": {
                "commit": PINNED_COMMIT,
                "tree": PINNED_TREE,
                "clean": True,
                "dirtyFingerprint": EMPTY_SHA256,
                "archiveFile": "source.tar",
                "archiveSha256": sha256_file(source_tar),
            },
            "ui": {
                "distSha256": dist_digest(entries),
                "archiveFile": "dist.tar",
                "archiveSha256": sha256_file(evidence / "dist.tar"),
                "manifestFile": "dist-manifest.json",
                "manifestSha256": sha256_file(evidence / "dist-manifest.json"),
                "finalPath": FINAL_DIST,
            },
            "commands": EXPECTED_COMMANDS,
            "toolchain": {
                "rustc": version(
                    [tools["rustc"]["invokedPath"], "-Vv"], cargo_environment
                ).replace("\n", ";"),
                "cargo": version(
                    [tools["cargo"]["invokedPath"], "-V"], cargo_environment
                ),
                "node": version(
                    [tools["node"]["invokedPath"], "-v"], npm_environment
                ),
                "npm": version(
                    [tools["npm"]["invokedPath"], "-v"], npm_environment
                ),
                "tools": tool_descriptors,
            },
            "outputs": {
                "npmFile": "npm-build.log",
                "npmSha256": sha256_file(evidence / "npm-build.log"),
                "cargoFile": "cargo-build.jsonl",
                "cargoSha256": sha256_file(evidence / "cargo-build.jsonl"),
            },
            "artifact": {
                "path": canonical_artifact.as_posix(),
                "finalPath": FINAL_EXE,
                "sha256": sha256_file(canonical_artifact),
                "sizeBytes": canonical_artifact.stat().st_size,
            },
            "loader": {
                "sourcePath": loader_source,
                "file": "WebView2Loader.dll",
                "finalPath": FINAL_LOADER,
                "sha256": hashlib.sha256(loader_bytes).hexdigest(),
                "sizeBytes": len(loader_bytes),
            },
            "execution": [
                execution_record(
                    logical_argv=NPM_SETUP_COMMAND,
                    actual_argv=npm_ci_argv,
                    cwd="apps/osl-hub-ui",
                    environment=npm_environment,
                    stdout_file="npm-ci.log",
                    stderr_file="npm-ci.stderr",
                    evidence=evidence,
                ),
                execution_record(
                    logical_argv=EXPECTED_COMMANDS[0],
                    actual_argv=npm_build_argv,
                    cwd="apps/osl-hub-ui",
                    environment=npm_environment,
                    stdout_file="npm-build.log",
                    stderr_file="npm-build.stderr",
                    evidence=evidence,
                ),
                execution_record(
                    logical_argv=EXPECTED_COMMANDS[1],
                    actual_argv=cargo_argv,
                    cwd="apps/osl-hub",
                    environment=cargo_environment,
                    stdout_file="cargo-build.jsonl",
                    stderr_file="cargo-build.stderr",
                    evidence=evidence,
                ),
            ],
        }
        (evidence / "build-log.json").write_text(
            json.dumps(log, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        shutil.copyfile(canonical_artifact, outputs / "osl-privacy-hub.exe")
        shutil.copyfile(evidence / "WebView2Loader.dll", outputs / "WebView2Loader.dll")
        shutil.copytree(dist, outputs / "dist")
        identity = make_identity(log, evidence)
        (bundle / "build-identity.json").write_text(
            json.dumps(identity, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        verify_bundle(bundle, allow_fixture=fixture, require_seal=False)
        parent_stat = os.stat(output.parent, follow_symlinks=False)
        opened_stat = os.fstat(parent_fd)
        if (parent_stat.st_dev, parent_stat.st_ino) != (
            opened_stat.st_dev,
            opened_stat.st_ino,
        ):
            raise EvidenceError("bundle parent changed before publication")
        staged_stat = os.stat(bundle, follow_symlinks=False)
        published_inode = (staged_stat.st_dev, staged_stat.st_ino)
        rename_noreplace(bundle, parent_fd, output.name)
        published = True
        final_stat = os.stat(output, follow_symlinks=False)
        if (final_stat.st_dev, final_stat.st_ino) != published_inode:
            raise EvidenceError("published bundle path was swapped")
        verify_bundle(output, allow_fixture=fixture, require_seal=False)
        final_stat = os.stat(output, follow_symlinks=False)
        if (final_stat.st_dev, final_stat.st_ino) != published_inode:
            raise EvidenceError("published bundle changed during final validation")
        seal_path, seal_inode = publish_producer_seal(
            output / "build-identity.json",
            output,
            fixture=fixture,
        )
        verify_bundle(
            output,
            allow_fixture=fixture,
            fixture_seal=seal_path if fixture else None,
        )
        final_stat = os.stat(output, follow_symlinks=False)
        if (final_stat.st_dev, final_stat.st_ino) != published_inode:
            raise EvidenceError("published bundle changed after producer sealing")
        print(f"producer_seal={seal_path}")
    except Exception:
        should_cleanup_seal = False
        if fixture and seal_path is not None and seal_inode is not None:
            try:
                current_seal = os.stat(seal_path, follow_symlinks=False)
                should_cleanup_seal = (
                    not seal_path.is_symlink()
                    and seal_path.is_file()
                    and (current_seal.st_dev, current_seal.st_ino) == seal_inode
                )
            except OSError:
                pass
        if should_cleanup_seal:
            seal_path.unlink()
        should_cleanup = False
        committed_production_seal = (
            not fixture and seal_path is not None and seal_inode is not None
        )
        if (
            published
            and published_inode is not None
            and not committed_production_seal
        ):
            try:
                current = os.stat(output, follow_symlinks=False)
                should_cleanup = (
                    output.is_dir()
                    and not output.is_symlink()
                    and (current.st_dev, current.st_ino) == published_inode
                )
            except OSError:
                pass
        if should_cleanup:
            shutil.rmtree(output)
        raise
    finally:
        os.close(parent_fd)
        if work.exists():
            shutil.rmtree(work)


def verify_evidence(
    directory: Path, exe_path: Path, identity: dict[str, Any]
) -> dict[str, Any]:
    actual_files = {path.name for path in directory.iterdir()}
    if actual_files != EVIDENCE_FILES or not all(
        (directory / name).is_file() and not (directory / name).is_symlink()
        for name in EVIDENCE_FILES
    ):
        raise EvidenceError(
            f"build evidence files are not exact; missing={sorted(EVIDENCE_FILES - actual_files)} "
            f"unknown={sorted(actual_files - EVIDENCE_FILES)}"
        )
    log = exact_object(
        load_json(directory / "build-log.json", "build log"),
        {
            "schemaVersion",
            "mode",
            "source",
            "ui",
            "commands",
            "toolchain",
            "outputs",
            "artifact",
            "loader",
            "execution",
        },
        "buildLog",
    )
    if type(log["schemaVersion"]) is not int or log["schemaVersion"] != 2:
        raise EvidenceError("buildLog.schemaVersion must be exactly 2")
    if log["mode"] not in ("production", "fixture"):
        raise EvidenceError("buildLog.mode is not exact")
    fixture = log["mode"] == "fixture"
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
            "finalPath",
        },
        "buildLog.ui",
    )
    outputs = exact_object(
        log["outputs"],
        {"npmFile", "npmSha256", "cargoFile", "cargoSha256"},
        "buildLog.outputs",
    )
    artifact = exact_object(
        log["artifact"],
        {"path", "finalPath", "sha256", "sizeBytes"},
        "buildLog.artifact",
    )
    loader = exact_object(
        log["loader"],
        {"sourcePath", "file", "finalPath", "sha256", "sizeBytes"},
        "buildLog.loader",
    )
    expected_loader_source = (
        "fixture:embedded" if fixture else PINNED_LOADER_SOURCE.as_posix()
    )
    expected_loader_sha = (
        hashlib.sha256(FIXTURE_LOADER_BYTES).hexdigest()
        if fixture
        else PINNED_LOADER_SHA256
    )
    if (
        loader["sourcePath"] != expected_loader_source
        or loader["file"] != "WebView2Loader.dll"
        or loader["finalPath"] != FINAL_LOADER
        or loader["sha256"] != expected_loader_sha
        or type(loader["sizeBytes"]) is not int
        or loader["sizeBytes"] < 1
        or sha256_file(directory / "WebView2Loader.dll") != loader["sha256"]
        or (directory / "WebView2Loader.dll").stat().st_size != loader["sizeBytes"]
    ):
        raise EvidenceError(
            "build log loader does not bind the immutable retained bytes"
        )
    if not isinstance(artifact["path"], str):
        raise EvidenceError("build log artifact path must be a string")
    artifact_path = PurePosixPath(artifact["path"])
    target_suffix = PurePosixPath(TARGET_ARTIFACT_PATH)
    if (
        not artifact_path.is_absolute()
        or artifact_path.parts[-len(target_suffix.parts) :] != target_suffix.parts
    ):
        raise EvidenceError("build log artifact is not in the producer-owned Cargo target")
    retained_target = artifact_path
    for _ in target_suffix.parts:
        retained_target = retained_target.parent
    if retained_target.name != "cargo-target":
        raise EvidenceError("build log artifact target is not producer-owned")
    retained_work = retained_target.parent
    if not retained_work.name.startswith(".vmqa-build-"):
        raise EvidenceError("build log artifact target is not inside producer scratch")

    toolchain = exact_object(
        log["toolchain"],
        {"rustc", "cargo", "node", "npm", "tools"},
        "buildLog.toolchain",
    )
    for name in ("rustc", "cargo", "node", "npm"):
        if not isinstance(toolchain[name], str) or not toolchain[name]:
            raise EvidenceError(f"build log toolchain {name} must be a nonempty string")
    if not isinstance(toolchain["tools"], list) or len(toolchain["tools"]) != 6:
        raise EvidenceError("build log tool descriptors are not exact")
    observed_tools: dict[str, dict[str, str]] = {}
    for index, raw in enumerate(toolchain["tools"]):
        descriptor = exact_object(
            raw,
            {"name", "invokedPath", "resolvedPath", "sha256"},
            f"buildLog.toolchain.tools[{index}]",
        )
        name = descriptor["name"]
        if (
            not isinstance(name, str)
            or name in observed_tools
            or not all(
                isinstance(descriptor[key], str) and descriptor[key]
                for key in ("invokedPath", "resolvedPath")
            )
            or not isinstance(descriptor["sha256"], str)
            or not SHA_RE.fullmatch(descriptor["sha256"])
        ):
            raise EvidenceError("build log tool descriptor is invalid")
        observed_tools[name] = descriptor
    pins = selected_tool_pins(fixture)
    if set(observed_tools) != set(pins):
        raise EvidenceError("build log tool names are not exact")
    for name, (invoked, expected_sha) in pins.items():
        descriptor = observed_tools[name]
        if (
            descriptor["invokedPath"] != invoked.as_posix()
            or descriptor["resolvedPath"] != invoked.resolve(strict=True).as_posix()
            or descriptor["sha256"] != expected_sha
        ):
            raise EvidenceError(f"build log {name} path/hash differs from immutable pin")

    npm_environment, cargo_environment = build_environments(
        observed_tools, retained_work, Path(retained_target.as_posix())
    )
    npm_ci_argv = [
        observed_tools["npm"]["invokedPath"],
        *NPM_SETUP_COMMAND[1:],
    ]
    npm_build_argv = [
        observed_tools["npm"]["invokedPath"],
        *EXPECTED_COMMANDS[0][1:],
    ]
    cargo_argv = [
        observed_tools["osl-cargo"]["invokedPath"],
        *EXPECTED_COMMANDS[1][1:],
    ]
    execution = log["execution"]
    if not isinstance(execution, list) or len(execution) != 3:
        raise EvidenceError("build log execution records are not exact")
    expected_execution = [
        (
            NPM_SETUP_COMMAND,
            npm_ci_argv,
            "apps/osl-hub-ui",
            npm_environment,
            "npm-ci.log",
            "npm-ci.stderr",
        ),
        (
            EXPECTED_COMMANDS[0],
            npm_build_argv,
            "apps/osl-hub-ui",
            npm_environment,
            "npm-build.log",
            "npm-build.stderr",
        ),
        (
            EXPECTED_COMMANDS[1],
            cargo_argv,
            "apps/osl-hub",
            cargo_environment,
            "cargo-build.jsonl",
            "cargo-build.stderr",
        ),
    ]
    for index, (
        logical_argv,
        actual_argv,
        cwd,
        environment,
        stdout_file,
        stderr_file,
    ) in enumerate(expected_execution):
        record = exact_object(
            execution[index],
            {
                "logicalArgv",
                "argv",
                "cwd",
                "environment",
                "exitCode",
                "stdoutFile",
                "stdoutSha256",
                "stderrFile",
                "stderrSha256",
            },
            f"buildLog.execution[{index}]",
        )
        if (
            record["logicalArgv"] != logical_argv
            or record["argv"] != actual_argv
            or record["cwd"] != cwd
            or record["environment"] != environment
            or record["exitCode"] != 0
            or record["stdoutFile"] != stdout_file
            or record["stderrFile"] != stderr_file
            or record["stdoutSha256"] != sha256_file(directory / stdout_file)
            or record["stderrSha256"] != sha256_file(directory / stderr_file)
        ):
            raise EvidenceError(
                "build log execution record differs from retained invocation bytes"
            )
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
    if source["commit"] != PINNED_COMMIT or source["tree"] != PINNED_TREE:
        raise EvidenceError("build log source differs from immutable VMQA pin")
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
    if set(file_bindings) != EVIDENCE_FILES - {
        "build-log.json",
        "npm-ci.log",
        "npm-ci.stderr",
        "npm-build.stderr",
        "cargo-build.stderr",
        "WebView2Loader.dll",
    }:
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
        for marker_path, marker in REQUIRED_PRODUCT_MARKERS.items():
            member = archive.extractfile(marker_path)
            if member is None or marker.search(member.read()) is None:
                raise EvidenceError("source archive is not the pinned OSL product")
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
        entry = exact_object(
            raw,
            {"path", "sha256", "sizeBytes"},
            f"distManifest.files[{index}]",
        )
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
    if (
        ui["finalPath"] != FINAL_DIST
        or ui["distSha256"] != dist_digest(entries)
    ):
        raise EvidenceError("build log dist digest differs from retained dist bytes")
    cargo_artifact = PurePosixPath(
        parse_cargo_artifact(directory / "cargo-build.jsonl")
    )
    if cargo_artifact != artifact_path:
        raise EvidenceError(
            "retained cargo artifact is not the canonical owned fresh-target output"
        )
    if (
        artifact["finalPath"] != FINAL_EXE
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
            "directory",
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
        "directory": FINAL_EVIDENCE,
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
    if identity["ui"] != {
        "path": FINAL_DIST,
        "distSha256": ui["distSha256"],
    }:
        raise EvidenceError("build identity dist digest differs from build log")
    if identity["build"]["commands"] != IDENTITY_COMMANDS:
        raise EvidenceError("build identity argv differs from build log")
    if identity["build"]["toolchain"] != toolchain:
        raise EvidenceError("build identity toolchain differs from build log")
    if identity["artifacts"]["executable"] != {
        "name": "osl-privacy-hub.exe",
        "path": FINAL_EXE,
        "sha256": artifact["sha256"],
        "sizeBytes": artifact["sizeBytes"],
    }:
        raise EvidenceError("build identity executable differs from build log")
    if identity["artifacts"]["loader"] != {
        "name": "WebView2Loader.dll",
        "path": FINAL_LOADER,
        "sha256": loader["sha256"],
        "sizeBytes": loader["sizeBytes"],
    }:
        raise EvidenceError("build identity loader differs from retained build evidence")
    return log


def verify_bundle(
    bundle: Path,
    *,
    allow_fixture: bool = False,
    fixture_seal: Path | None = None,
    require_seal: bool = True,
) -> dict[str, Any]:
    if bundle.is_symlink() or not bundle.is_dir():
        raise EvidenceError("build bundle must be a real directory")
    actual_root = {path.name for path in bundle.iterdir()}
    if actual_root != {"build-identity.json", FINAL_EVIDENCE, "outputs"}:
        raise EvidenceError("build bundle root entries are not exact")
    identity_path = bundle / "build-identity.json"
    evidence = bundle / FINAL_EVIDENCE
    outputs = bundle / "outputs"
    if (
        identity_path.is_symlink()
        or not identity_path.is_file()
        or evidence.is_symlink()
        or not evidence.is_dir()
        or outputs.is_symlink()
        or not outputs.is_dir()
    ):
        raise EvidenceError("build bundle contains a symlink or wrong entry type")
    output_entries = {path.name for path in outputs.iterdir()}
    if output_entries != {"osl-privacy-hub.exe", "WebView2Loader.dll", "dist"}:
        raise EvidenceError("build bundle output entries are not exact")
    exe = bundle / FINAL_EXE
    loader = bundle / FINAL_LOADER
    dist = bundle / FINAL_DIST
    if (
        exe.is_symlink()
        or not exe.is_file()
        or loader.is_symlink()
        or not loader.is_file()
        or dist.is_symlink()
        or not dist.is_dir()
    ):
        raise EvidenceError("build bundle output contains a symlink or wrong type")
    identity_value, identity_sha = load_json_with_sha(
        identity_path, "build identity"
    )
    identity = exact_object(
        identity_value,
        {"schemaVersion", "source", "ui", "build", "artifacts", "evidence"},
        "buildIdentity",
    )
    if identity["schemaVersion"] != SCHEMA_VERSION:
        raise EvidenceError("buildIdentity.schemaVersion must be exactly 2")
    exact_object(
        identity["source"],
        {"commit", "tree", "clean", "dirtyFingerprint"},
        "buildIdentity.source",
    )
    exact_object(identity["ui"], {"path", "distSha256"}, "buildIdentity.ui")
    build = exact_object(
        identity["build"],
        {"target", "features", "profile", "commands", "toolchain"},
        "buildIdentity.build",
    )
    if (
        build["target"] != "x86_64-pc-windows-gnu"
        or build["features"] != ["desktop"]
        or build["profile"] != "release"
        or build["commands"] != IDENTITY_COMMANDS
    ):
        raise EvidenceError("build identity build boundary is not exact")
    artifacts = exact_object(
        identity["artifacts"], {"executable", "loader"}, "buildIdentity.artifacts"
    )
    exact_object(
        artifacts["executable"],
        {"name", "path", "sha256", "sizeBytes"},
        "buildIdentity.artifacts.executable",
    )
    exact_object(
        artifacts["loader"],
        {"name", "path", "sha256", "sizeBytes"},
        "buildIdentity.artifacts.loader",
    )
    log = verify_evidence(evidence, exe, identity)
    if log["mode"] == "fixture" and not allow_fixture:
        raise EvidenceError("fixture build evidence is forbidden in production")
    if log["mode"] == "production" and fixture_seal is not None:
        raise EvidenceError("caller-selected producer seal is forbidden in production")
    manifest = load_json(evidence / "dist-manifest.json", "dist manifest")
    if dist_entries_from_directory(dist) != manifest["files"]:
        raise EvidenceError("published dist differs from retained manifest")
    if (
        sha256_file(loader) != artifacts["loader"]["sha256"]
        or loader.stat().st_size != artifacts["loader"]["sizeBytes"]
        or loader.read_bytes() != (evidence / "WebView2Loader.dll").read_bytes()
    ):
        raise EvidenceError("published loader differs from retained owned bytes")
    if (
        sha256_file(exe) != artifacts["executable"]["sha256"]
        or exe.stat().st_size != artifacts["executable"]["sizeBytes"]
    ):
        raise EvidenceError("published executable differs from retained identity")
    if require_seal:
        verify_producer_seal(
            identity_path,
            mode=log["mode"],
            allow_fixture=allow_fixture,
            fixture_seal=fixture_seal,
            identity_sha=identity_sha,
        )
    return identity


def verify_bundle_command(args: argparse.Namespace) -> None:
    fixture_seal = (
        Path(args.internal_test_seal)
        if args.internal_test_seal is not None
        else None
    )
    verify_bundle(
        Path(args.bundle),
        allow_fixture=bool(args.internal_test_fixture),
        fixture_seal=fixture_seal,
    )


class VmqaBuildEvidenceBehaviourTests(unittest.TestCase):
    def test_verify_retained_vmqa_build_evidence_from_exact_producer_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as raw_root:
            root = Path(raw_root)
            identity_path = root / "build-identity.json"
            identity_path.write_bytes(
                b'{"schemaVersion":2,"fixture":"producer-authenticated"}\n'
            )
            seal_path = root / "bundle.producer-seal.json"
            identity_sha = sha256_file(identity_path)
            seal_path.write_text(
                json.dumps(
                    producer_seal_record(identity_sha, "fixture"),
                    sort_keys=True,
                    separators=(",", ":"),
                )
                + "\n",
                encoding="utf-8",
            )
            seal_path.chmod(0o444)

            self.assertEqual(
                verify_producer_seal(
                    identity_path,
                    mode="fixture",
                    allow_fixture=True,
                    fixture_seal=seal_path,
                ),
                seal_path,
            )

            identity_path.write_bytes(
                b'{"schemaVersion":2,"fixture":"same path, different producer bytes"}\n'
            )
            with self.assertRaisesRegex(
                EvidenceError,
                "does not bind the retained build identity",
            ):
                verify_producer_seal(
                    identity_path,
                    mode="fixture",
                    allow_fixture=True,
                    fixture_seal=seal_path,
                )


def _scripts_vmqa_test_vmqa_contract_py() -> None:
    testcase = VmqaBuildEvidenceBehaviourTests(
        "test_verify_retained_vmqa_build_evidence_from_exact_producer_bytes"
    )
    testcase.test_verify_retained_vmqa_build_evidence_from_exact_producer_bytes()


_scripts_vmqa_test_vmqa_contract_py.__name__ = "scripts/vmqa/test-vmqa-contract.py"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    tests.addTest(unittest.FunctionTestCase(_scripts_vmqa_test_vmqa_contract_py))
    return tests


def main() -> int:
    parser = argparse.ArgumentParser(allow_abbrev=False)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command, fixture, fixture_scenario in (
        ("create", False, "valid"),
        ("create-fixture", True, "valid"),
        ("create-fixture-outside-artifact", True, "outside-artifact"),
    ):
        create = subparsers.add_parser(command, allow_abbrev=False)
        create.add_argument("--source-repo", required=True)
        create.add_argument("--output", required=True)
        for forbidden_option in (
            "--dist",
            "--exe",
            "--loader",
            "--npm-log",
            "--cargo-log",
            "--expected-commit",
            "--expected-tree",
            "--exe-destination",
            "--dist-destination",
            "--loader-destination",
            "--shared-target-dir",
        ):
            create.add_argument(forbidden_option, help=argparse.SUPPRESS)
        create.set_defaults(
            function=create_evidence,
            fixture=fixture,
            fixture_scenario=fixture_scenario,
        )
    verify = subparsers.add_parser("verify-bundle", allow_abbrev=False)
    verify.add_argument("--bundle", required=True)
    verify.add_argument(
        "--internal-test-fixture",
        action="store_true",
        help=argparse.SUPPRESS,
    )
    verify.add_argument("--internal-test-seal", help=argparse.SUPPRESS)
    verify.set_defaults(function=verify_bundle_command)
    args = parser.parse_args()
    try:
        args.function(args)
    except (EvidenceError, OSError, tarfile.TarError, subprocess.CalledProcessError) as exc:
        print(f"VMQA BUILD EVIDENCE INVALID: {exc}", file=sys.stderr)
        return 9
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
