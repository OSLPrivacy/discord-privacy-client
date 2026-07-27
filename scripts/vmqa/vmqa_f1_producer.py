#!/usr/bin/python3 -I
"""Fail-closed producer bootstrap and staging for the F1 native witness."""

from __future__ import annotations

import argparse
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

SCHEMA_VERSION = 1
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
    expected_seal = build_evidence.producer_seal_record(identity_sha, mode)
    if reopened_seal != expected_seal:
        raise ProducerError("independent producer seal changed after verification")

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
        "wouldStage": destination.as_posix(),
    }


def receipt_for(
    admitted: AdmittedBuild, key_id: str, destination: Path
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
    }


def stage(
    bundle: Path,
    *,
    layout: ProducerLayout,
    producer: ProducerIdentity,
    effective_uid: int | None = None,
    internal_fixture_seal: Path | None = None,
) -> dict[str, Any]:
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
    ):
        raise ProducerError("build bundle changed after preflight")
    destination = staging_destination(layout, admitted)
    temporary = Path(
        tempfile.mkdtemp(prefix=".f1-stage-", dir=layout.staging_root)
    )
    published = False
    published_inode: tuple[int, int] | None = None
    parent_fd = os.open(
        layout.staging_root,
        os.O_RDONLY | getattr(os, "O_DIRECTORY", 0),
    )
    try:
        copied_bundle = temporary / "bundle"
        shutil.copytree(bundle, copied_bundle)
        copied = inspect_admitted_bundle(
            copied_bundle, internal_fixture_seal=internal_fixture_seal
        )
        if copied != admitted:
            raise ProducerError("copied bundle differs from admitted source bytes")
        receipt = receipt_for(
            copied, summary["nativeWitnessKeyId"], destination
        )
        retained_seal_path = temporary / "producer-seal.json"
        with retained_seal_path.open("xb") as handle:
            handle.write(copied.producer_seal_bytes)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(retained_seal_path, 0o400)
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
        _, final_key_id = read_witness_key(layout.key_path, producer.uid)
        if final_key_id != summary["nativeWitnessKeyId"]:
            raise ProducerError("witness key changed during publication")
        return {
            **receipt,
            "receiptPath": (
                destination / "staging-receipt.json"
            ).as_posix(),
            "receiptSha256": sha256_bytes(final_receipt_bytes),
        }
    except BaseException:
        if not published and temporary.exists():
            shutil.rmtree(temporary)
        elif published and published_inode is not None:
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
        os.close(parent_fd)


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
