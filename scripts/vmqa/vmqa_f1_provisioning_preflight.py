#!/usr/bin/python3 -I
"""Read-only administrator preflight for the fixed F1 producer installation."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import pwd
import re
import stat
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


SCHEMA_VERSION = 1
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
FIXED_OPT_ROOT = Path("/opt")
FIXED_INSTALL_ROOT = Path("/opt/osl-vmqa")
FIXED_BIN_ROOT = FIXED_INSTALL_ROOT / "bin"
FIXED_TOOLCHAIN_ROOT = FIXED_INSTALL_ROOT / "toolchain"
FIXED_TOOLCHAIN_BIN = FIXED_TOOLCHAIN_ROOT / "bin"
FIXED_VAR_ROOT = Path("/var")
FIXED_VAR_LIB_ROOT = FIXED_VAR_ROOT / "lib"
FIXED_VMQA_STATE_ROOT = FIXED_VAR_LIB_ROOT / "osl-vmqa"
FIXED_AUTHORITY_ROOT = Path("/var/lib/osl-qa")
FIXED_PRIVATE_ROOT = FIXED_AUTHORITY_ROOT / "private"
FIXED_STAGING_ROOT = FIXED_AUTHORITY_ROOT / "f1-staging"
FIXED_SEAL_ROOT = Path("/var/lib/osl-vmqa/producer-seals")
PRODUCER_USER = "osl-vmqa-producer"
PROGRAM_PINS = {
    "vmqa_build_evidence.py": (
        "fc8041443f1841ccf0173e598fb1fcdd80ed03afc24f32608fb4de29137ac625"
    ),
    "vmqa-f1-windows-provisioning-preflight.ps1": (
        "7fcc04012c31a6c1e7ab9bde28f57927ccd3886e4cbc91ff2aef1f246f0dde88"
    ),
}


class ProvisioningError(ValueError):
    """A fixed provisioning prerequisite is absent or unsafe."""


class StrictParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise ProvisioningError(f"arguments refused: {message}")


@dataclass(frozen=True)
class ProducerIdentity:
    name: str
    uid: int
    gid: int
    shell: str


@dataclass(frozen=True)
class ProvisioningLayout:
    opt_root: Path = FIXED_OPT_ROOT
    install_root: Path = FIXED_INSTALL_ROOT
    bin_root: Path = FIXED_BIN_ROOT
    toolchain_root: Path = FIXED_TOOLCHAIN_ROOT
    toolchain_bin: Path = FIXED_TOOLCHAIN_BIN
    var_root: Path = FIXED_VAR_ROOT
    var_lib_root: Path = FIXED_VAR_LIB_ROOT
    vmqa_state_root: Path = FIXED_VMQA_STATE_ROOT
    authority_root: Path = FIXED_AUTHORITY_ROOT
    private_root: Path = FIXED_PRIVATE_ROOT
    staging_root: Path = FIXED_STAGING_ROOT
    seal_root: Path = FIXED_SEAL_ROOT
    root_uid: int = 0


def load_adjacent(name: str, filename: str) -> Any:
    path = Path(__file__).absolute().with_name(filename)
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load adjacent {filename}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    try:
        spec.loader.exec_module(module)
    except BaseException:
        sys.modules.pop(name, None)
        raise
    return module


producer = load_adjacent("_vmqa_provisioning_f1_producer", "vmqa_f1_producer.py")
build_evidence = producer.build_evidence


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise ProvisioningError(f"{label} fields are not exact")
    return value


def resolve_producer_identity() -> ProducerIdentity:
    try:
        value = pwd.getpwnam(PRODUCER_USER)
    except KeyError as exc:
        raise ProvisioningError(
            f"dedicated producer account is missing: {PRODUCER_USER}"
        ) from exc
    return ProducerIdentity(
        name=value.pw_name,
        uid=value.pw_uid,
        gid=value.pw_gid,
        shell=value.pw_shell,
    )


def validate_administrator_and_identity(
    identity: ProducerIdentity, *, effective_uid: int
) -> None:
    if effective_uid != 0:
        raise ProvisioningError(
            "host provisioning preflight requires uid 0 administrator"
        )
    if identity.name != PRODUCER_USER or identity.uid == 0:
        raise ProvisioningError("producer identity is not the dedicated account")
    if Path(identity.shell).name not in {"false", "nologin"}:
        raise ProvisioningError("producer account must have a non-login shell")


def stat_kind(value: os.stat_result) -> str:
    if stat.S_ISDIR(value.st_mode):
        return "directory"
    if stat.S_ISREG(value.st_mode):
        return "file"
    if stat.S_ISLNK(value.st_mode):
        return "symlink"
    return "other"


def validate_stat(
    value: os.stat_result,
    *,
    kind: str,
    owner_uid: int,
    mode: int,
    label: str,
) -> None:
    if stat_kind(value) != kind:
        raise ProvisioningError(f"{label} is not a real {kind}")
    if value.st_uid != owner_uid:
        raise ProvisioningError(f"{label} has the wrong owner")
    if stat.S_IMODE(value.st_mode) != mode:
        raise ProvisioningError(f"{label} has the wrong mode")


def require_directory(
    path: Path, *, owner_uid: int, mode: int, label: str
) -> os.stat_result:
    try:
        before = os.lstat(path)
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ProvisioningError(f"{label} is missing or unsafe") from exc
    try:
        opened = os.fstat(descriptor)
        validate_stat(
            before,
            kind="directory",
            owner_uid=owner_uid,
            mode=mode,
            label=label,
        )
        validate_stat(
            opened,
            kind="directory",
            owner_uid=owner_uid,
            mode=mode,
            label=label,
        )
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise ProvisioningError(f"{label} changed while opened")
        return opened
    finally:
        os.close(descriptor)


def read_fixed_file(
    path: Path,
    *,
    owner_uid: int,
    mode: int,
    label: str,
    expected_sha256: str | None = None,
) -> bytes:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        before = os.lstat(path)
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ProvisioningError(f"{label} is missing or unsafe") from exc
    try:
        opened = os.fstat(descriptor)
        validate_stat(
            before,
            kind="file",
            owner_uid=owner_uid,
            mode=mode,
            label=label,
        )
        validate_stat(
            opened,
            kind="file",
            owner_uid=owner_uid,
            mode=mode,
            label=label,
        )
        if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise ProvisioningError(f"{label} changed while opened")
        digest = hashlib.sha256()
        chunks: list[bytes] = []
        while True:
            block = os.read(descriptor, 1024 * 1024)
            if not block:
                break
            digest.update(block)
            chunks.append(block)
        after_fd = os.fstat(descriptor)
        after_path = os.lstat(path)
        if (
            (after_fd.st_dev, after_fd.st_ino, after_fd.st_size)
            != (opened.st_dev, opened.st_ino, opened.st_size)
            or (after_path.st_dev, after_path.st_ino, after_path.st_size)
            != (opened.st_dev, opened.st_ino, opened.st_size)
        ):
            raise ProvisioningError(f"{label} changed while hashed")
        observed_sha = digest.hexdigest()
        if expected_sha256 is not None and observed_sha != expected_sha256:
            raise ProvisioningError(f"{label} hash differs from immutable pin")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def require_root_owned_ancestors(
    path: Path, *, root: Path, root_uid: int, label: str
) -> None:
    try:
        relative = path.relative_to(root)
    except ValueError as exc:
        raise ProvisioningError(f"{label} escapes the fixed root") from exc
    cursor = root
    require_directory(
        cursor, owner_uid=root_uid, mode=0o755, label=f"{label} root"
    )
    for part in relative.parts[:-1]:
        cursor /= part
        require_directory(
            cursor,
            owner_uid=root_uid,
            mode=0o755,
            label=f"{label} ancestor",
        )


def validate_tool_pin_contract(
    tool_pins: dict[str, tuple[Path, str]], toolchain_root: Path
) -> None:
    if set(tool_pins) != {
        "git",
        "npm",
        "node",
        "osl-cargo",
        "rustc",
        "cargo",
    }:
        raise ProvisioningError("production tool pin names are not exact")
    for name, (path, digest) in tool_pins.items():
        if not path.is_absolute():
            raise ProvisioningError(f"pinned {name} path is not absolute")
        try:
            path.relative_to(toolchain_root)
        except ValueError as exc:
            raise ProvisioningError(
                f"pinned {name} path escapes fixed toolchain root"
            ) from exc
        if SHA_RE.fullmatch(digest) is None:
            raise ProvisioningError(f"pinned {name} hash is invalid")


def verify_installation(
    layout: ProvisioningLayout,
    *,
    program_pins: dict[str, str],
    tool_pins: dict[str, tuple[Path, str]],
) -> dict[str, str]:
    for path, label in (
        (layout.opt_root, "system opt root"),
        (layout.install_root, "VMQA installation root"),
        (layout.bin_root, "VMQA program directory"),
        (layout.toolchain_root, "VMQA toolchain root"),
        (layout.toolchain_bin, "VMQA toolchain bin"),
    ):
        require_directory(
            path, owner_uid=layout.root_uid, mode=0o755, label=label
        )
    if any(SHA_RE.fullmatch(value) is None for value in program_pins.values()):
        raise ProvisioningError("fixed program pin is not a SHA-256 digest")
    for filename, digest in program_pins.items():
        read_fixed_file(
            layout.bin_root / filename,
            owner_uid=layout.root_uid,
            mode=0o555,
            label=f"fixed program {filename}",
            expected_sha256=digest,
        )
    # The producer entrypoint is the root-owned source trust anchor. It
    # independently hash-pins this preflight before importing it.
    read_fixed_file(
        layout.bin_root / "vmqa_f1_producer.py",
        owner_uid=layout.root_uid,
        mode=0o555,
        label="fixed producer entrypoint",
    )
    read_fixed_file(
        layout.bin_root / Path(__file__).name,
        owner_uid=layout.root_uid,
        mode=0o555,
        label="fixed provisioning preflight",
    )
    validate_tool_pin_contract(tool_pins, layout.toolchain_root)
    observed_tools: dict[str, str] = {}
    for name, (path, digest) in tool_pins.items():
        require_root_owned_ancestors(
            path,
            root=layout.toolchain_root,
            root_uid=layout.root_uid,
            label=f"pinned {name}",
        )
        read_fixed_file(
            path,
            owner_uid=layout.root_uid,
            mode=0o555,
            label=f"pinned {name}",
            expected_sha256=digest,
        )
        observed_tools[name] = digest
    return observed_tools


def verify_authority_layout(
    layout: ProvisioningLayout, identity: ProducerIdentity
) -> str:
    for path, label in (
        (layout.var_root, "system var root"),
        (layout.var_lib_root, "system var lib root"),
        (layout.vmqa_state_root, "VMQA state root"),
    ):
        require_directory(
            path, owner_uid=layout.root_uid, mode=0o755, label=label
        )
    require_directory(
        layout.authority_root,
        owner_uid=layout.root_uid,
        mode=0o755,
        label="F1 authority root",
    )
    require_directory(
        layout.private_root,
        owner_uid=identity.uid,
        mode=0o700,
        label="F1 private directory",
    )
    require_directory(
        layout.staging_root,
        owner_uid=identity.uid,
        mode=0o700,
        label="F1 staging directory",
    )
    require_directory(
        layout.seal_root,
        owner_uid=identity.uid,
        mode=0o755,
        label="producer seal directory",
    )
    for path, label in (
        (layout.seal_root / ".seal-chain-state.json", "producer seal state"),
        (layout.seal_root / ".seal-chain.lock", "producer seal lock"),
        (
            layout.staging_root / producer.ADMISSION_STATE_NAME,
            "F1 admission state",
        ),
        (
            layout.staging_root / producer.ADMISSION_LOCK_NAME,
            "F1 admission lock",
        ),
    ):
        read_fixed_file(
            path,
            owner_uid=identity.uid,
            mode=0o600,
            label=label,
        )
    _, key_id = producer.read_witness_key(
        layout.private_root / "f1-native-witness.key", identity.uid
    )
    return key_id


def load_receipt(path: Path, owner_uid: int) -> tuple[dict[str, Any], bytes]:
    raw = read_fixed_file(
        path,
        owner_uid=owner_uid,
        mode=0o600,
        label="F1 staging receipt",
    )
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise ProvisioningError("F1 staging receipt is not valid JSON") from exc
    return (
        exact_object(
            value,
            {
                "schemaVersion",
                "mode",
                "producer",
                "source",
                "buildIdentitySha256",
                "producerSeal",
                "executable",
                "loader",
                "nativeWitnessKeyId",
                "admissionTransition",
                "terminalSnapshot",
                "terminalSnapshotSha256",
            },
            "F1 staging receipt",
        ),
        raw,
    )


def select_current_stage(
    layout: ProvisioningLayout,
    identity: ProducerIdentity,
    state: dict[str, Any],
) -> tuple[Path, dict[str, Any], bytes]:
    matches: list[tuple[Path, dict[str, Any], bytes]] = []
    for child in layout.staging_root.iterdir():
        if child.name in {
            producer.ADMISSION_STATE_NAME,
            producer.ADMISSION_LOCK_NAME,
        }:
            continue
        if SHA_RE.fullmatch(child.name) is None:
            raise ProvisioningError(
                "F1 staging root contains an unexpected entry"
            )
        try:
            value = os.lstat(child)
        except OSError as exc:
            raise ProvisioningError("F1 stage entry is unavailable") from exc
        if (
            not stat.S_ISDIR(value.st_mode)
            or stat.S_ISLNK(value.st_mode)
            or value.st_uid != identity.uid
            or stat.S_IMODE(value.st_mode) != 0o700
        ):
            raise ProvisioningError("F1 stage entry is unsafe")
        receipt, raw = load_receipt(
            child / "staging-receipt.json", identity.uid
        )
        seal = receipt.get("producerSeal")
        if (
            isinstance(seal, dict)
            and seal.get("sha256") == state["sealSha256"]
        ):
            matches.append((child, receipt, raw))
    if len(matches) != 1:
        raise ProvisioningError(
            "exactly one protected stage must match current admission state"
        )
    return matches[0]


def validate_current_stage(
    layout: ProvisioningLayout,
    identity: ProducerIdentity,
    *,
    key_id: str,
    inspect_bundle: Callable[[Path], Any],
) -> dict[str, Any]:
    producer_layout = producer.ProducerLayout(
        authority_root=layout.authority_root,
        key_directory=layout.private_root,
        key_path=layout.private_root / "f1-native-witness.key",
        staging_root=layout.staging_root,
        authority_uid=layout.root_uid,
        enforce_installed_programs=False,
    )
    state = producer.read_admission_state(producer_layout, identity.uid)
    if state["generation"] < 1:
        raise ProvisioningError("F1 admission state has no sealed generation")
    destination, receipt, receipt_bytes = select_current_stage(
        layout, identity, state
    )
    admitted = inspect_bundle(destination / "bundle")
    if admitted.mode != "production":
        raise ProvisioningError("current F1 stage is not production-sealed")
    if (
        admitted.seal_generation != state["generation"]
        or admitted.producer_seal_sha256 != state["sealSha256"]
        or admitted.identity_sha256 != state["identitySha256"]
    ):
        raise ProvisioningError("current stage differs from admission state")
    snapshot = producer.terminal_destination_snapshot(
        destination,
        reported_root=destination,
        producer_uid=identity.uid,
    )
    producer.validate_terminal_snapshot(snapshot, admitted)
    source = exact_object(receipt["source"], {"commit", "tree"}, "receipt source")
    seal = exact_object(
        receipt["producerSeal"],
        {
            "authorityPath",
            "path",
            "sha256",
            "generation",
            "previousSealSha256",
            "transition",
        },
        "receipt producer seal",
    )
    executable = exact_object(
        receipt["executable"],
        {"path", "sha256", "sizeBytes"},
        "receipt executable",
    )
    loader = exact_object(
        receipt["loader"],
        {"path", "sha256", "sizeBytes"},
        "receipt loader",
    )
    transition = exact_object(
        receipt["admissionTransition"],
        {
            "kind",
            "fromGeneration",
            "fromSealSha256",
            "fromIdentitySha256",
            "toGeneration",
            "toSealSha256",
            "toIdentitySha256",
            "globalPreviousSealSha256",
        },
        "receipt admission transition",
    )
    expected_from_generation = admitted.seal_generation - 1
    expected_kind = (
        "initial-admission"
        if admitted.seal_generation == 1
        else "monotonic-advance"
    )
    if (
        receipt["schemaVersion"] != producer.SCHEMA_VERSION
        or receipt["mode"] != "production"
        or receipt["producer"] != PRODUCER_USER
        or source
        != {"commit": producer.PINNED_COMMIT, "tree": producer.PINNED_TREE}
        or receipt["buildIdentitySha256"] != admitted.identity_sha256
        or not isinstance(receipt["nativeWitnessKeyId"], str)
        or receipt["nativeWitnessKeyId"] != key_id
        or destination.name != admitted.executable_sha256
        or seal["authorityPath"] != admitted.producer_seal.as_posix()
        or seal["path"] != (destination / "producer-seal.json").as_posix()
        or seal["sha256"] != admitted.producer_seal_sha256
        or seal["generation"] != admitted.seal_generation
        or seal["previousSealSha256"] != admitted.previous_seal_sha256
        or seal["transition"] != admitted.seal_transition
        or executable
        != {
            "path": (
                destination / "bundle" / build_evidence.FINAL_EXE
            ).as_posix(),
            "sha256": admitted.executable_sha256,
            "sizeBytes": admitted.executable_size,
        }
        or loader
        != {
            "path": (
                destination / "bundle" / build_evidence.FINAL_LOADER
            ).as_posix(),
            "sha256": admitted.loader_sha256,
            "sizeBytes": admitted.loader_size,
        }
        or transition["kind"] != expected_kind
        or transition["fromGeneration"] != expected_from_generation
        or transition["fromSealSha256"]
        != admitted.previous_seal_sha256
        or not isinstance(transition["fromIdentitySha256"], str)
        or SHA_RE.fullmatch(transition["fromIdentitySha256"]) is None
        or (
            admitted.seal_generation == 1
            and transition["fromIdentitySha256"] != build_evidence.EMPTY_SHA256
        )
        or transition["toGeneration"] != admitted.seal_generation
        or transition["toSealSha256"]
        != admitted.producer_seal_sha256
        or transition["toIdentitySha256"] != admitted.identity_sha256
        or transition["globalPreviousSealSha256"]
        != admitted.previous_seal_sha256
        or receipt["terminalSnapshot"] != snapshot
        or receipt["terminalSnapshotSha256"]
        != sha256_bytes(canonical_json(snapshot))
    ):
        raise ProvisioningError(
            "current protected stage differs from its exact sealed receipt"
        )
    reread = read_fixed_file(
        destination / "staging-receipt.json",
        owner_uid=identity.uid,
        mode=0o600,
        label="terminal F1 staging receipt",
    )
    if reread != receipt_bytes:
        raise ProvisioningError("F1 staging receipt changed during preflight")
    return {
        "generation": admitted.seal_generation,
        "identitySha256": admitted.identity_sha256,
        "executableSha256": admitted.executable_sha256,
        "loaderSha256": admitted.loader_sha256,
        "stage": destination.as_posix(),
    }


def preflight_host(
    *,
    layout: ProvisioningLayout,
    identity: ProducerIdentity,
    effective_uid: int,
    program_pins: dict[str, str],
    tool_pins: dict[str, tuple[Path, str]],
    inspect_bundle: Callable[[Path], Any],
) -> dict[str, Any]:
    validate_administrator_and_identity(identity, effective_uid=effective_uid)
    tools = verify_installation(
        layout, program_pins=program_pins, tool_pins=tool_pins
    )
    key_id = verify_authority_layout(layout, identity)
    stage = validate_current_stage(
        layout, identity, key_id=key_id, inspect_bundle=inspect_bundle
    )
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "ready",
        "producer": identity.name,
        "source": {
            "commit": producer.PINNED_COMMIT,
            "tree": producer.PINNED_TREE,
        },
        "programPins": program_pins,
        "toolPins": tools,
        "currentStage": stage,
        "writesPerformed": 0,
        "windowsPreflightRequired": True,
    }


def production_preflight() -> dict[str, Any]:
    layout = ProvisioningLayout()
    if (
        Path(__file__).absolute()
        != FIXED_BIN_ROOT / "vmqa_f1_provisioning_preflight.py"
    ):
        raise ProvisioningError(
            "production preflight must run from fixed root-owned installation"
        )
    return preflight_host(
        layout=layout,
        identity=resolve_producer_identity(),
        effective_uid=os.geteuid(),
        program_pins=PROGRAM_PINS,
        tool_pins=build_evidence.PRODUCTION_TOOL_PINS,
        inspect_bundle=lambda path: producer.inspect_admitted_bundle(path),
    )


def main(argv: list[str] | None = None) -> int:
    parser = StrictParser(allow_abbrev=False)
    try:
        parser.parse_args(argv)
        raise ProvisioningError(
            "invoke provisioning-preflight through fixed vmqa_f1_producer.py"
        )
    except (
        ProvisioningError,
        producer.ProducerError,
        build_evidence.EvidenceError,
        OSError,
    ) as exc:
        print(f"VMQA F1 PROVISIONING REFUSED: {exc}", file=sys.stderr)
        return 9


if __name__ == "__main__":
    raise SystemExit(main())
