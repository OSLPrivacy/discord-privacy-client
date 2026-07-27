#!/usr/bin/python3 -I
"""Fail-closed producer bootstrap and staging for the F1 native witness."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
import pwd
import re
import secrets
import shutil
import stat
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, BinaryIO

SCHEMA_VERSION = 2
ADMISSION_STATE_SCHEMA_VERSION = 1
PINNED_COMMIT = "1f745c85bb23cf79a956aa87d623905e20f83cf1"
PINNED_TREE = "1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a"
PRODUCER_USER = "osl-vmqa-producer"
FIXED_PROGRAM = Path("/opt/osl-vmqa/bin/vmqa_f1_producer.py")
FIXED_BUILD_EVIDENCE_PROGRAM = Path(
    "/opt/osl-vmqa/bin/vmqa_build_evidence.py"
)
FIXED_OPT_ROOT = Path("/opt")
FIXED_INSTALL_ROOT = Path("/opt/osl-vmqa")
FIXED_INSTALL_BIN = FIXED_INSTALL_ROOT / "bin"
AUTHORITY_ROOT = Path("/var/lib/osl-qa")
KEY_DIRECTORY = AUTHORITY_ROOT / "private"
KEY_PATH = KEY_DIRECTORY / "f1-native-witness.key"
STAGING_ROOT = AUTHORITY_ROOT / "f1-staging"
ADMISSION_STATE_NAME = ".f1-admission-state.json"
ADMISSION_LOCK_NAME = ".f1-admission.lock"
KEY_RE = re.compile(rb"[0-9a-f]{64}\n?")
SHA_RE = re.compile(r"[0-9a-f]{64}")


def load_build_evidence_module() -> Any:
    """Load the adjacent reviewed module without consulting PYTHONPATH."""
    module_path = Path(__file__).absolute().with_name(
        "vmqa_build_evidence.py"
    )
    if Path(__file__).absolute() == FIXED_PROGRAM:
        for path in (FIXED_OPT_ROOT, FIXED_INSTALL_ROOT, FIXED_INSTALL_BIN):
            value = os.lstat(path)
            if (
                not stat.S_ISDIR(value.st_mode)
                or stat.S_ISLNK(value.st_mode)
                or value.st_uid != 0
                or stat.S_IMODE(value.st_mode) != 0o755
            ):
                raise RuntimeError(
                    "fixed VMQA program hierarchy is not root-owned mode 0755"
                )
        for path in (FIXED_PROGRAM, FIXED_BUILD_EVIDENCE_PROGRAM):
            value = os.lstat(path)
            if (
                not stat.S_ISREG(value.st_mode)
                or stat.S_ISLNK(value.st_mode)
                or value.st_uid != 0
                or stat.S_IMODE(value.st_mode) != 0o555
            ):
                raise RuntimeError(
                    "fixed VMQA program is not root-owned mode 0555"
                )
    spec = importlib.util.spec_from_file_location(
        "_vmqa_f1_build_evidence", module_path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load adjacent VMQA build-evidence module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


build_evidence = load_build_evidence_module()


class ProducerError(ValueError):
    """A production precondition or integrity check failed."""


class StrictParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise ProducerError(f"arguments refused: {message}")


@dataclass(frozen=True)
class ProducerIdentity:
    name: str
    uid: int
    gid: int
    shell: str


@dataclass(frozen=True)
class ProducerLayout:
    authority_root: Path = AUTHORITY_ROOT
    key_directory: Path = KEY_DIRECTORY
    key_path: Path = KEY_PATH
    staging_root: Path = STAGING_ROOT
    authority_uid: int = 0
    enforce_installed_programs: bool = True


@dataclass(frozen=True)
class AdmittedBuild:
    mode: str
    identity: dict[str, Any]
    identity_sha256: str
    executable_sha256: str
    executable_size: int
    loader_sha256: str
    loader_size: int
    producer_seal: Path
    producer_seal_bytes: bytes
    producer_seal_sha256: str
    seal_generation: int
    previous_seal_sha256: str
    seal_transition: str


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")


def resolve_producer_identity() -> ProducerIdentity:
    try:
        value = pwd.getpwnam(PRODUCER_USER)
    except KeyError as exc:
        raise ProducerError(
            f"dedicated producer account is not provisioned: {PRODUCER_USER}"
        ) from exc
    return ProducerIdentity(
        name=value.pw_name,
        uid=value.pw_uid,
        gid=value.pw_gid,
        shell=value.pw_shell,
    )


def require_producer_identity(
    producer: ProducerIdentity, *, effective_uid: int | None = None
) -> None:
    if producer.name != PRODUCER_USER:
        raise ProducerError("resolved producer identity is not the fixed account")
    if Path(producer.shell).name not in {"false", "nologin"}:
        raise ProducerError("dedicated producer account must be non-login")
    observed_uid = os.geteuid() if effective_uid is None else effective_uid
    if observed_uid != producer.uid:
        raise ProducerError(
            "operation must run as the dedicated producer identity"
        )


def require_real_directory(
    path: Path, *, owner_uid: int, mode: int, label: str
) -> os.stat_result:
    if not path.is_absolute():
        raise ProducerError(f"{label} path is not absolute")
    try:
        value = os.lstat(path)
    except OSError as exc:
        raise ProducerError(f"{label} is missing: {path}") from exc
    if not stat.S_ISDIR(value.st_mode) or stat.S_ISLNK(value.st_mode):
        raise ProducerError(f"{label} must be a real directory")
    cursor = Path(path.anchor)
    for part in path.parts[1:]:
        cursor /= part
        if cursor.is_symlink():
            raise ProducerError(f"{label} contains a symlink component")
    if value.st_uid != owner_uid or stat.S_IMODE(value.st_mode) != mode:
        raise ProducerError(
            f"{label} must have owner uid {owner_uid} and mode {mode:04o}"
        )
    return value


def require_fixed_program(path: Path, label: str) -> None:
    try:
        value = os.lstat(path)
    except OSError as exc:
        raise ProducerError(f"{label} is not installed at {path}") from exc
    if (
        not stat.S_ISREG(value.st_mode)
        or stat.S_ISLNK(value.st_mode)
        or value.st_uid != 0
        or stat.S_IMODE(value.st_mode) != 0o555
    ):
        raise ProducerError(
            f"{label} must be a root-owned regular file with mode 0555"
        )


def require_installed_programs(layout: ProducerLayout) -> None:
    if not layout.enforce_installed_programs:
        return
    require_real_directory(
        FIXED_OPT_ROOT,
        owner_uid=0,
        mode=0o755,
        label="system /opt root",
    )
    require_real_directory(
        FIXED_INSTALL_ROOT,
        owner_uid=0,
        mode=0o755,
        label="VMQA fixed installation root",
    )
    require_real_directory(
        FIXED_INSTALL_BIN,
        owner_uid=0,
        mode=0o755,
        label="VMQA fixed program directory",
    )
    invoked = Path(__file__).absolute()
    imported = Path(build_evidence.__file__).absolute()
    if invoked != FIXED_PROGRAM or imported != FIXED_BUILD_EVIDENCE_PROGRAM:
        raise ProducerError(
            "production workflow must run from the fixed /opt/osl-vmqa installation"
        )
    require_fixed_program(FIXED_PROGRAM, "F1 producer program")
    require_fixed_program(
        FIXED_BUILD_EVIDENCE_PROGRAM, "VMQA build-evidence program"
    )


def require_layout(
    layout: ProducerLayout,
    producer: ProducerIdentity,
    *,
    require_key: bool,
) -> tuple[bytes, str] | None:
    require_real_directory(
        layout.authority_root,
        owner_uid=layout.authority_uid,
        mode=0o755,
        label="F1 producer authority root",
    )
    require_real_directory(
        layout.key_directory,
        owner_uid=producer.uid,
        mode=0o700,
        label="F1 private key directory",
    )
    require_real_directory(
        layout.staging_root,
        owner_uid=producer.uid,
        mode=0o700,
        label="F1 protected staging root",
    )
    if layout.key_path.parent != layout.key_directory:
        raise ProducerError("witness key path is not the fixed producer path")
    if require_key:
        return read_witness_key(layout.key_path, producer.uid)
    return None


def read_regular_once(
    path: Path, *, owner_uid: int | None, mode: int | None, label: str
) -> tuple[bytes, os.stat_result]:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        before = os.lstat(path)
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ProducerError(f"{label} is missing or unsafe") from exc
    try:
        opened = os.fstat(descriptor)
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or not stat.S_ISREG(opened.st_mode)
            or (before.st_dev, before.st_ino)
            != (opened.st_dev, opened.st_ino)
        ):
            raise ProducerError(f"{label} was replaced or is not a regular file")
        if owner_uid is not None and opened.st_uid != owner_uid:
            raise ProducerError(f"{label} has the wrong owner")
        if mode is not None and stat.S_IMODE(opened.st_mode) != mode:
            raise ProducerError(f"{label} has the wrong mode")
        chunks: list[bytes] = []
        while True:
            block = os.read(descriptor, 1024 * 1024)
            if not block:
                break
            chunks.append(block)
        value = b"".join(chunks)
        after = os.lstat(path)
        if (after.st_dev, after.st_ino) != (opened.st_dev, opened.st_ino):
            raise ProducerError(f"{label} changed while it was read")
        return value, opened
    finally:
        os.close(descriptor)


def read_witness_key(path: Path, producer_uid: int) -> tuple[bytes, str]:
    raw, _ = read_regular_once(
        path,
        owner_uid=producer_uid,
        mode=0o600,
        label="F1 producer witness key",
    )
    if KEY_RE.fullmatch(raw) is None:
        raise ProducerError(
            "F1 producer witness key must contain exactly 32 lowercase-hex bytes"
        )
    key = bytes.fromhex(raw.rstrip(b"\n").decode("ascii"))
    return key, sha256_bytes(key)


def snapshot_stat_identity(value: os.stat_result) -> tuple[int, ...]:
    """Fields which must remain stable throughout one terminal snapshot."""
    return (
        value.st_dev,
        value.st_ino,
        value.st_mode,
        value.st_uid,
        value.st_gid,
        value.st_nlink,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
    )


def open_snapshot_descriptor(
    name: str | Path,
    *,
    parent_fd: int | None,
    owner_uid: int,
    directory: bool,
    label: str,
) -> tuple[int, os.stat_result]:
    flags = os.O_RDONLY
    if directory:
        flags |= getattr(os, "O_DIRECTORY", 0)
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        before = os.stat(
            name, dir_fd=parent_fd, follow_symlinks=False
        )
        descriptor = os.open(name, flags, dir_fd=parent_fd)
    except OSError as exc:
        raise ProducerError(f"{label} is missing or unsafe") from exc
    try:
        opened = os.fstat(descriptor)
        expected_type = stat.S_ISDIR if directory else stat.S_ISREG
        if (
            not expected_type(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or not expected_type(opened.st_mode)
            or before.st_uid != owner_uid
            or opened.st_uid != owner_uid
            or snapshot_stat_identity(before)
            != snapshot_stat_identity(opened)
        ):
            raise ProducerError(f"{label} descriptor identity is unsafe")
        return descriptor, opened
    except BaseException:
        os.close(descriptor)
        raise


def snapshot_record(
    descriptor: int,
    opened: os.stat_result,
    *,
    reported_path: Path,
    directory: bool,
    label: str,
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "path": reported_path.as_posix(),
        "type": "directory" if directory else "file",
        "device": opened.st_dev,
        "inode": opened.st_ino,
        "mode": stat.S_IMODE(opened.st_mode),
        "linkCount": opened.st_nlink,
    }
    if not directory:
        digest = hashlib.sha256()
        size = 0
        os.lseek(descriptor, 0, os.SEEK_SET)
        while True:
            block = os.read(descriptor, 1024 * 1024)
            if not block:
                break
            digest.update(block)
            size += len(block)
        if size != opened.st_size:
            raise ProducerError(f"{label} size changed while hashing")
        record.update({"sizeBytes": size, "sha256": digest.hexdigest()})
    return record


def terminal_destination_snapshot(
    actual_root: Path,
    *,
    reported_root: Path,
    producer_uid: int,
) -> dict[str, Any]:
    # Open the root once, resolve every descendant with openat(), and retain
    # the complete descriptor tree until every file has been hashed and every
    # name (including the root path) has been terminally revalidated.
    descriptors: dict[str, int] = {}
    opened: dict[str, os.stat_result] = {}
    bindings: dict[str, tuple[int | None, str | Path]] = {}
    directory_flags = {
        "stageDirectory": True,
        "bundleDirectory": True,
        "outputsDirectory": True,
        "identity": False,
        "executable": False,
        "loader": False,
        "producerSeal": False,
    }
    reported_paths = {
        "stageDirectory": reported_root,
        "bundleDirectory": reported_root / "bundle",
        "outputsDirectory": reported_root / "bundle" / "outputs",
        "identity": reported_root / "bundle" / "build-identity.json",
        "executable": reported_root / "bundle" / build_evidence.FINAL_EXE,
        "loader": reported_root / "bundle" / build_evidence.FINAL_LOADER,
        "producerSeal": reported_root / "producer-seal.json",
    }
    try:
        root_fd, root_stat = open_snapshot_descriptor(
            actual_root,
            parent_fd=None,
            owner_uid=producer_uid,
            directory=True,
            label="terminal stageDirectory",
        )
        descriptors["stageDirectory"] = root_fd
        opened["stageDirectory"] = root_stat
        bindings["stageDirectory"] = (None, actual_root)

        child_specs = (
            ("bundleDirectory", "stageDirectory", "bundle"),
            ("outputsDirectory", "bundleDirectory", "outputs"),
            ("identity", "bundleDirectory", "build-identity.json"),
            (
                "executable",
                "outputsDirectory",
                Path(build_evidence.FINAL_EXE).name,
            ),
            (
                "loader",
                "outputsDirectory",
                Path(build_evidence.FINAL_LOADER).name,
            ),
            ("producerSeal", "stageDirectory", "producer-seal.json"),
        )
        for name, parent_name, basename in child_specs:
            descriptor, value = open_snapshot_descriptor(
                basename,
                parent_fd=descriptors[parent_name],
                owner_uid=producer_uid,
                directory=directory_flags[name],
                label=f"terminal {name}",
            )
            descriptors[name] = descriptor
            opened[name] = value
            bindings[name] = (descriptors[parent_name], basename)

        snapshot = {
            name: snapshot_record(
                descriptors[name],
                opened[name],
                reported_path=reported_paths[name],
                directory=directory_flags[name],
                label=f"terminal {name}",
            )
            for name in directory_flags
        }

        # These checks occur only after the last file hash. The descriptor
        # tree stays live, so every pathname is checked against the same open
        # root and the root pathname itself is checked last.
        for name in reversed(tuple(directory_flags)):
            after_fd = os.fstat(descriptors[name])
            parent_fd, basename = bindings[name]
            after_name = os.stat(
                basename, dir_fd=parent_fd, follow_symlinks=False
            )
            if (
                snapshot_stat_identity(after_fd)
                != snapshot_stat_identity(opened[name])
                or snapshot_stat_identity(after_name)
                != snapshot_stat_identity(opened[name])
            ):
                raise ProducerError(
                    f"terminal {name} changed during descriptor-rooted snapshot"
                )
        return snapshot
    except OSError as exc:
        raise ProducerError(
            "terminal destination changed during descriptor-rooted snapshot"
        ) from exc
    finally:
        for descriptor in reversed(tuple(descriptors.values())):
            os.close(descriptor)


def validate_terminal_snapshot(
    snapshot: dict[str, Any], admitted: AdmittedBuild
) -> None:
    expected = {
        "identity": (admitted.identity_sha256, None),
        "executable": (
            admitted.executable_sha256,
            admitted.executable_size,
        ),
        "loader": (admitted.loader_sha256, admitted.loader_size),
        "producerSeal": (
            admitted.producer_seal_sha256,
            len(admitted.producer_seal_bytes),
        ),
    }
    for name, (digest, size) in expected.items():
        value = snapshot.get(name)
        if not isinstance(value, dict) or value.get("sha256") != digest:
            raise ProducerError(
                f"terminal {name} hash differs from admitted bytes"
            )
        if size is not None and value.get("sizeBytes") != size:
            raise ProducerError(
                f"terminal {name} size differs from admitted bytes"
            )
    for name in (
        "stageDirectory",
        "bundleDirectory",
        "outputsDirectory",
    ):
        value = snapshot.get(name)
        if (
            not isinstance(value, dict)
            or value.get("type") != "directory"
            or type(value.get("device")) is not int
            or type(value.get("inode")) is not int
        ):
            raise ProducerError(f"terminal {name} snapshot is invalid")


def admission_state_path(layout: ProducerLayout) -> Path:
    return layout.staging_root / ADMISSION_STATE_NAME


def admission_lock_path(layout: ProducerLayout) -> Path:
    return layout.staging_root / ADMISSION_LOCK_NAME


def initial_admission_state() -> dict[str, Any]:
    return {
        "schemaVersion": ADMISSION_STATE_SCHEMA_VERSION,
        "generation": 0,
        "sealSha256": build_evidence.EMPTY_SHA256,
        "identitySha256": build_evidence.EMPTY_SHA256,
    }


def admission_state_record(admitted: AdmittedBuild) -> dict[str, Any]:
    return {
        "schemaVersion": ADMISSION_STATE_SCHEMA_VERSION,
        "generation": admitted.seal_generation,
        "sealSha256": admitted.producer_seal_sha256,
        "identitySha256": admitted.identity_sha256,
    }


def read_admission_state(
    layout: ProducerLayout, producer_uid: int
) -> dict[str, Any]:
    path = admission_state_path(layout)
    if not os.path.lexists(path):
        return initial_admission_state()
    raw, _ = read_regular_once(
        path,
        owner_uid=producer_uid,
        mode=0o600,
        label="F1 admission state",
    )
    try:
        state = json.loads(raw.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ProducerError("F1 admission state is not valid JSON") from exc
    if not isinstance(state, dict) or set(state) != {
        "schemaVersion",
        "generation",
        "sealSha256",
        "identitySha256",
    }:
        raise ProducerError("F1 admission state fields are not exact")
    if (
        state["schemaVersion"] != ADMISSION_STATE_SCHEMA_VERSION
        or type(state["generation"]) is not int
        or state["generation"] < 1
        or not isinstance(state["sealSha256"], str)
        or SHA_RE.fullmatch(state["sealSha256"]) is None
        or not isinstance(state["identitySha256"], str)
        or SHA_RE.fullmatch(state["identitySha256"]) is None
    ):
        raise ProducerError("F1 admission state is invalid")
    return state


def permitted_admission_transition(
    state: dict[str, Any], admitted: AdmittedBuild
) -> dict[str, Any]:
    if admitted.seal_generation <= state["generation"]:
        raise ProducerError(
            "seal replay refused: generation is not newer than admitted state"
        )
    if state["generation"] == 0:
        if (
            admitted.seal_generation != 1
            or admitted.previous_seal_sha256 != build_evidence.EMPTY_SHA256
            or admitted.seal_transition != "initial"
        ):
            raise ProducerError(
                "initial F1 admission must be the producer-seal genesis"
            )
    elif (
        admitted.seal_generation != state["generation"] + 1
        or admitted.previous_seal_sha256 != state["sealSha256"]
        or admitted.seal_transition != "successor"
    ):
        raise ProducerError(
            "candidate seal is not the direct successor of admitted state"
        )
    return {
        "kind": (
            "initial-admission"
            if state["generation"] == 0
            else "monotonic-advance"
        ),
        "fromGeneration": state["generation"],
        "fromSealSha256": state["sealSha256"],
        "fromIdentitySha256": state["identitySha256"],
        "toGeneration": admitted.seal_generation,
        "toSealSha256": admitted.producer_seal_sha256,
        "toIdentitySha256": admitted.identity_sha256,
        "globalPreviousSealSha256": admitted.previous_seal_sha256,
    }


def open_admission_lock(
    layout: ProducerLayout, producer_uid: int
) -> int:
    path = admission_lock_path(layout)
    flags = os.O_RDWR | os.O_CREAT
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags, 0o600)
    except OSError as exc:
        raise ProducerError("F1 admission lock is unavailable") from exc
    try:
        value = os.fstat(descriptor)
        if (
            not stat.S_ISREG(value.st_mode)
            or value.st_uid != producer_uid
            or stat.S_IMODE(value.st_mode) != 0o600
        ):
            raise ProducerError("F1 admission lock has unsafe ownership or mode")
        fcntl.flock(descriptor, fcntl.LOCK_EX)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def replace_admission_state(
    layout: ProducerLayout,
    producer_uid: int,
    *,
    expected: dict[str, Any],
    admitted: AdmittedBuild,
) -> dict[str, Any]:
    if read_admission_state(layout, producer_uid) != expected:
        raise ProducerError("F1 admission state changed before monotonic advance")
    target = admission_state_record(admitted)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=".f1-admission-state-", dir=layout.staging_root
    )
    temporary = Path(temporary_name)
    parent_fd = os.open(
        layout.staging_root,
        os.O_RDONLY | getattr(os, "O_DIRECTORY", 0),
    )
    try:
        os.fchmod(descriptor, 0o600)
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(canonical_json(target))
            handle.flush()
            os.fsync(handle.fileno())
        state_path = admission_state_path(layout)
        if os.path.lexists(state_path):
            current = os.lstat(state_path)
            if (
                not stat.S_ISREG(current.st_mode)
                or stat.S_ISLNK(current.st_mode)
                or current.st_uid != producer_uid
                or stat.S_IMODE(current.st_mode) != 0o600
            ):
                raise ProducerError(
                    "F1 admission state has unsafe ownership or mode"
                )
        os.replace(temporary, state_path)
        os.fsync(parent_fd)
        if read_admission_state(layout, producer_uid) != target:
            raise ProducerError("published F1 admission state changed")
        return target
    finally:
        os.close(parent_fd)
        try:
            os.close(descriptor)
        except OSError:
            pass
        if temporary.exists():
            temporary.unlink()


def validate_imported_key(raw: bytes) -> bytes:
    if KEY_RE.fullmatch(raw) is None:
        raise ProducerError(
            "imported witness key must contain exactly 32 lowercase-hex bytes"
        )
    return raw.rstrip(b"\n") + b"\n"


def bootstrap_key(
    *,
    layout: ProducerLayout,
    producer: ProducerIdentity,
    generate: bool,
    import_stream: BinaryIO | None,
    effective_uid: int | None = None,
) -> dict[str, str]:
    require_producer_identity(producer, effective_uid=effective_uid)
    require_installed_programs(layout)
    require_layout(layout, producer, require_key=False)
    if os.path.lexists(layout.key_path):
        raise ProducerError("F1 producer witness key already exists")
    if generate == (import_stream is not None):
        raise ProducerError("select exactly one of generate or import from stdin")
    if generate:
        raw = (secrets.token_hex(32) + "\n").encode("ascii")
    else:
        assert import_stream is not None
        raw = validate_imported_key(import_stream.read(66))

    temporary: Path | None = None
    parent_fd = os.open(
        layout.key_directory,
        os.O_RDONLY | getattr(os, "O_DIRECTORY", 0),
    )
    try:
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=".f1-witness-key-",
            dir=layout.key_directory,
        )
        temporary = Path(temporary_name)
        try:
            os.fchmod(descriptor, 0o600)
            with os.fdopen(descriptor, "wb") as handle:
                handle.write(raw)
                handle.flush()
                os.fsync(handle.fileno())
            temporary_stat = os.lstat(temporary)
            if (
                temporary_stat.st_uid != producer.uid
                or stat.S_IMODE(temporary_stat.st_mode) != 0o600
            ):
                raise ProducerError(
                    "temporary witness key has unsafe ownership or mode"
                )
            build_evidence.rename_noreplace(
                temporary, parent_fd, layout.key_path.name
            )
            temporary = None
            os.fsync(parent_fd)
        except BaseException:
            try:
                os.close(descriptor)
            except OSError:
                pass
            raise
    finally:
        os.close(parent_fd)
        if temporary is not None and temporary.exists():
            temporary.unlink()

    _, key_id = read_witness_key(layout.key_path, producer.uid)
    return {"status": "created", "keyId": key_id}


def require_identity_boundary(identity: dict[str, Any]) -> None:
    source = identity.get("source")
    build = identity.get("build")
    artifacts = identity.get("artifacts")
    if not isinstance(source, dict) or (
        source.get("commit") != PINNED_COMMIT
        or source.get("tree") != PINNED_TREE
        or source.get("clean") is not True
    ):
        raise ProducerError("build identity is not bound to the exact F1 source")
    if not isinstance(build, dict) or (
        build.get("target") != "x86_64-pc-windows-gnu"
        or build.get("profile") != "release"
        or build.get("features") != ["desktop"]
    ):
        raise ProducerError("build identity is not the exact Windows release")
    if not isinstance(artifacts, dict):
        raise ProducerError("build identity artifacts are missing")
    for name in ("executable", "loader"):
        artifact = artifacts.get(name)
        if (
            not isinstance(artifact, dict)
            or not isinstance(artifact.get("sha256"), str)
            or SHA_RE.fullmatch(artifact["sha256"]) is None
            or type(artifact.get("sizeBytes")) is not int
            or artifact["sizeBytes"] < 1
        ):
            raise ProducerError(f"build identity {name} binding is invalid")


def inspect_admitted_bundle(
    bundle: Path,
    *,
    internal_fixture_seal: Path | None = None,
) -> AdmittedBuild:
    try:
        identity = build_evidence.verify_bundle(
            bundle,
            allow_fixture=internal_fixture_seal is not None,
            fixture_seal=internal_fixture_seal,
        )
    except (build_evidence.EvidenceError, OSError) as exc:
        raise ProducerError(f"build bundle is not producer-admitted: {exc}") from exc
    require_identity_boundary(identity)

    identity_bytes, _ = read_regular_once(
        bundle / "build-identity.json",
        owner_uid=None,
        mode=None,
        label="retained build identity",
    )
    identity_sha = sha256_bytes(identity_bytes)
    try:
        reopened_identity = json.loads(identity_bytes.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ProducerError("retained build identity is not valid JSON") from exc
    if reopened_identity != identity:
        raise ProducerError("retained build identity changed after verification")

    mode = "fixture" if internal_fixture_seal is not None else "production"
    try:
        seal_path = build_evidence.verify_producer_seal(
            bundle / "build-identity.json",
            mode=mode,
            allow_fixture=internal_fixture_seal is not None,
            fixture_seal=internal_fixture_seal,
            identity_sha=identity_sha,
        )
    except (build_evidence.EvidenceError, OSError) as exc:
        raise ProducerError(
            f"independent producer seal refused the build identity: {exc}"
        ) from exc
    seal_bytes, _ = read_regular_once(
        seal_path,
        owner_uid=None,
        mode=0o444,
        label="independent producer seal",
    )
    try:
        reopened_seal = json.loads(seal_bytes.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ProducerError("independent producer seal is not valid JSON") from exc
    try:
        seal_record = build_evidence.validate_producer_seal_record(
            reopened_seal, identity_sha, mode
        )
    except build_evidence.EvidenceError as exc:
        raise ProducerError(
            f"independent producer seal changed after verification: {exc}"
        ) from exc

    executable_bytes, executable_stat = read_regular_once(
        bundle / build_evidence.FINAL_EXE,
        owner_uid=None,
        mode=None,
        label="release executable",
    )
    loader_bytes, loader_stat = read_regular_once(
        bundle / build_evidence.FINAL_LOADER,
        owner_uid=None,
        mode=None,
        label="release loader",
    )
    executable = identity["artifacts"]["executable"]
    loader = identity["artifacts"]["loader"]
    executable_sha = sha256_bytes(executable_bytes)
    loader_sha = sha256_bytes(loader_bytes)
    if (
        executable_sha != executable["sha256"]
        or executable_stat.st_size != executable["sizeBytes"]
    ):
        raise ProducerError(
            "release executable differs from the independently sealed identity"
        )
    if (
        loader_sha != loader["sha256"]
        or loader_stat.st_size != loader["sizeBytes"]
    ):
        raise ProducerError(
            "release loader differs from the independently sealed identity"
        )
    return AdmittedBuild(
        mode=mode,
        identity=identity,
        identity_sha256=identity_sha,
        executable_sha256=executable_sha,
        executable_size=executable_stat.st_size,
        loader_sha256=loader_sha,
        loader_size=loader_stat.st_size,
        producer_seal=seal_path,
        producer_seal_bytes=seal_bytes,
        producer_seal_sha256=sha256_bytes(seal_bytes),
        seal_generation=seal_record["generation"],
        previous_seal_sha256=seal_record["previousSealSha256"],
        seal_transition=seal_record["transition"],
    )


def staging_destination(
    layout: ProducerLayout, admitted: AdmittedBuild
) -> Path:
    return layout.staging_root / admitted.executable_sha256


def preflight(
    bundle: Path,
    *,
    layout: ProducerLayout,
    producer: ProducerIdentity,
    effective_uid: int | None = None,
    internal_fixture_seal: Path | None = None,
) -> dict[str, Any]:
    require_producer_identity(producer, effective_uid=effective_uid)
    require_installed_programs(layout)
    key = require_layout(layout, producer, require_key=True)
    assert key is not None
    _, key_id = key
    admitted = inspect_admitted_bundle(
        bundle, internal_fixture_seal=internal_fixture_seal
    )
    admission_state = read_admission_state(layout, producer.uid)
    transition = permitted_admission_transition(admission_state, admitted)
    destination = staging_destination(layout, admitted)
    if os.path.lexists(destination):
        raise ProducerError(
            "staging replay refused: executable hash already has a retained admission"
        )
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "ready",
        "mode": admitted.mode,
        "sourceCommit": PINNED_COMMIT,
        "sourceTree": PINNED_TREE,
        "buildIdentitySha256": admitted.identity_sha256,
        "executableSha256": admitted.executable_sha256,
        "loaderSha256": admitted.loader_sha256,
        "nativeWitnessKeyId": key_id,
        "sealGeneration": admitted.seal_generation,
        "previousSealSha256": admitted.previous_seal_sha256,
        "sealTransition": admitted.seal_transition,
        "admissionState": admission_state,
        "admissionTransition": transition,
        "wouldStage": destination.as_posix(),
    }


def receipt_for(
    admitted: AdmittedBuild,
    key_id: str,
    destination: Path,
    *,
    terminal_snapshot: dict[str, Any],
    admission_transition: dict[str, Any],
) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "mode": admitted.mode,
        "producer": (
            "internal-vmqa-fixture"
            if admitted.mode == "fixture"
            else PRODUCER_USER
        ),
        "source": {"commit": PINNED_COMMIT, "tree": PINNED_TREE},
        "buildIdentitySha256": admitted.identity_sha256,
        "producerSeal": {
            "authorityPath": admitted.producer_seal.as_posix(),
            "path": (destination / "producer-seal.json").as_posix(),
            "sha256": admitted.producer_seal_sha256,
            "generation": admitted.seal_generation,
            "previousSealSha256": admitted.previous_seal_sha256,
            "transition": admitted.seal_transition,
        },
        "executable": {
            "path": (destination / "bundle" / build_evidence.FINAL_EXE).as_posix(),
            "sha256": admitted.executable_sha256,
            "sizeBytes": admitted.executable_size,
        },
        "loader": {
            "path": (
                destination / "bundle" / build_evidence.FINAL_LOADER
            ).as_posix(),
            "sha256": admitted.loader_sha256,
            "sizeBytes": admitted.loader_size,
        },
        "nativeWitnessKeyId": key_id,
        "admissionTransition": admission_transition,
        "terminalSnapshot": terminal_snapshot,
        "terminalSnapshotSha256": sha256_bytes(
            canonical_json(terminal_snapshot)
        ),
    }


def stage(
    bundle: Path,
    *,
    layout: ProducerLayout,
    producer: ProducerIdentity,
    effective_uid: int | None = None,
    internal_fixture_seal: Path | None = None,
) -> dict[str, Any]:
    # Refuse identity/layout failures before creating even the protected lock.
    preflight(
        bundle,
        layout=layout,
        producer=producer,
        effective_uid=effective_uid,
        internal_fixture_seal=internal_fixture_seal,
    )
    admission_lock = open_admission_lock(layout, producer.uid)
    temporary: Path | None = None
    destination: Path | None = None
    published = False
    published_inode: tuple[int, int] | None = None
    parent_fd: int | None = None
    try:
        # Re-run the entire dry preflight while holding the monotonic lock.
        summary = preflight(
            bundle,
            layout=layout,
            producer=producer,
            effective_uid=effective_uid,
            internal_fixture_seal=internal_fixture_seal,
        )
        admitted = inspect_admitted_bundle(
            bundle, internal_fixture_seal=internal_fixture_seal
        )
        if (
            admitted.identity_sha256 != summary["buildIdentitySha256"]
            or admitted.executable_sha256 != summary["executableSha256"]
            or admitted.loader_sha256 != summary["loaderSha256"]
            or admitted.seal_generation != summary["sealGeneration"]
            or admitted.producer_seal_sha256
            != summary["admissionTransition"]["toSealSha256"]
        ):
            raise ProducerError("build bundle changed after preflight")
        destination = staging_destination(layout, admitted)
        temporary = Path(
            tempfile.mkdtemp(prefix=".f1-stage-", dir=layout.staging_root)
        )
        parent_fd = os.open(
            layout.staging_root,
            os.O_RDONLY | getattr(os, "O_DIRECTORY", 0),
        )
        copied_bundle = temporary / "bundle"
        shutil.copytree(bundle, copied_bundle)
        copied = inspect_admitted_bundle(
            copied_bundle, internal_fixture_seal=internal_fixture_seal
        )
        if copied != admitted:
            raise ProducerError("copied bundle differs from admitted source bytes")
        retained_seal_path = temporary / "producer-seal.json"
        with retained_seal_path.open("xb") as handle:
            handle.write(copied.producer_seal_bytes)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(retained_seal_path, 0o400)
        owned_snapshot = terminal_destination_snapshot(
            temporary,
            reported_root=destination,
            producer_uid=producer.uid,
        )
        validate_terminal_snapshot(owned_snapshot, copied)
        receipt = receipt_for(
            copied,
            summary["nativeWitnessKeyId"],
            destination,
            terminal_snapshot=owned_snapshot,
            admission_transition=summary["admissionTransition"],
        )
        receipt_path = temporary / "staging-receipt.json"
        with receipt_path.open("xb") as handle:
            handle.write(canonical_json(receipt))
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(receipt_path, 0o600)
        _, key_id_before_publish = read_witness_key(
            layout.key_path, producer.uid
        )
        if key_id_before_publish != summary["nativeWitnessKeyId"]:
            raise ProducerError("witness key changed after preflight")
        build_evidence.rename_noreplace(
            temporary, parent_fd, destination.name
        )
        published = True
        final_stat = os.stat(destination, follow_symlinks=False)
        published_inode = (final_stat.st_dev, final_stat.st_ino)
        if published_inode != (
            owned_snapshot["stageDirectory"]["device"],
            owned_snapshot["stageDirectory"]["inode"],
        ):
            raise ProducerError("published stage directory inode changed")
        os.fsync(parent_fd)

        final_bundle = destination / "bundle"
        final = inspect_admitted_bundle(
            final_bundle, internal_fixture_seal=internal_fixture_seal
        )
        if final != admitted:
            raise ProducerError("published bundle differs from admitted bytes")
        final_seal_bytes, _ = read_regular_once(
            destination / "producer-seal.json",
            owner_uid=producer.uid,
            mode=0o400,
            label="published producer seal",
        )
        if final_seal_bytes != admitted.producer_seal_bytes:
            raise ProducerError("published producer seal changed")
        _, final_key_id = read_witness_key(layout.key_path, producer.uid)
        if final_key_id != summary["nativeWitnessKeyId"]:
            raise ProducerError("witness key changed during publication")
        before_state = summary["admissionState"]
        final_state = replace_admission_state(
            layout,
            producer.uid,
            expected=before_state,
            admitted=admitted,
        )
        if final_state != admission_state_record(admitted):
            raise ProducerError("F1 admission state did not advance exactly")
        final_receipt_bytes, final_receipt_stat = read_regular_once(
            destination / "staging-receipt.json",
            owner_uid=producer.uid,
            mode=0o600,
            label="published staging receipt",
        )
        if (
            final_receipt_bytes != canonical_json(receipt)
            or final_receipt_stat.st_nlink != 1
        ):
            raise ProducerError("published staging receipt changed")
        if read_admission_state(layout, producer.uid) != final_state:
            raise ProducerError("F1 admission state changed after publication")

        # This is deliberately the final destination access before return.
        # Every receipt-bound inode and hash is reopened and recomputed.
        terminal_snapshot = terminal_destination_snapshot(
            destination,
            reported_root=destination,
            producer_uid=producer.uid,
        )
        validate_terminal_snapshot(terminal_snapshot, admitted)
        if (
            terminal_snapshot != receipt["terminalSnapshot"]
            or sha256_bytes(canonical_json(terminal_snapshot))
            != receipt["terminalSnapshotSha256"]
        ):
            raise ProducerError(
                "terminal destination snapshot differs from staging receipt"
            )
        return {
            **receipt,
            "receiptPath": (
                destination / "staging-receipt.json"
            ).as_posix(),
            "receiptSha256": sha256_bytes(final_receipt_bytes),
        }
    except BaseException:
        if not published and temporary is not None and temporary.exists():
            shutil.rmtree(temporary)
        elif (
            published
            and published_inode is not None
            and destination is not None
        ):
            try:
                current = os.stat(destination, follow_symlinks=False)
                if (
                    stat.S_ISDIR(current.st_mode)
                    and (current.st_dev, current.st_ino) == published_inode
                ):
                    shutil.rmtree(destination)
            except OSError:
                pass
        raise
    finally:
        if parent_fd is not None:
            os.close(parent_fd)
        fcntl.flock(admission_lock, fcntl.LOCK_UN)
        os.close(admission_lock)


def build_parser() -> StrictParser:
    parser = StrictParser(allow_abbrev=False)
    commands = parser.add_subparsers(dest="command", required=True)
    bootstrap = commands.add_parser("bootstrap-key", allow_abbrev=False)
    choice = bootstrap.add_mutually_exclusive_group(required=True)
    choice.add_argument("--generate", action="store_true")
    choice.add_argument("--import-stdin", action="store_true")
    for name in ("preflight", "stage"):
        command = commands.add_parser(name, allow_abbrev=False)
        command.add_argument("--bundle", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        producer = resolve_producer_identity()
        layout = ProducerLayout()
        if args.command == "bootstrap-key":
            result = bootstrap_key(
                layout=layout,
                producer=producer,
                generate=args.generate,
                import_stream=(
                    sys.stdin.buffer if args.import_stdin else None
                ),
            )
        elif args.command == "preflight":
            result = preflight(
                Path(args.bundle), layout=layout, producer=producer
            )
        else:
            result = stage(
                Path(args.bundle), layout=layout, producer=producer
            )
        print(canonical_json(result).decode("utf-8"), end="")
        return 0
    except (
        ProducerError,
        build_evidence.EvidenceError,
        OSError,
        shutil.Error,
    ) as exc:
        print(f"VMQA F1 PRODUCER REFUSED: {exc}", file=sys.stderr)
        return 9


if __name__ == "__main__":
    raise SystemExit(main())
