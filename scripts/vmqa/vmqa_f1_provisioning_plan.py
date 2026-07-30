#!/usr/bin/python3 -I
"""Generate and verify a deterministic, non-executable F1 provisioning plan."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path, PurePosixPath
from typing import Any


SCHEMA_VERSION = 1
INPUT_KIND = "vmqa-f1-provisioning-plan-input"
MANIFEST_KIND = "vmqa-f1-provisioning-plan"
PINNED_RELEASE_COMMIT = "1f745c85bb23cf79a956aa87d623905e20f83cf1"
PINNED_RELEASE_TREE = "1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a"
PREDECESSOR_COMMIT = "2279be3b789f4aadbcc35db57d6105ab820e3bf6"
PREDECESSOR_TREE = "a9b9d5027f6123a4a1c4b012641ae2f7f43ec299"
PRODUCER_NAME = "osl-vmqa-producer"
PRODUCER_HOME = "/var/lib/osl-vmqa"
PRODUCER_SHELL = "/usr/sbin/nologin"
INSTALL_ROOT = "/opt/osl-vmqa"
BIN_ROOT = INSTALL_ROOT + "/bin"
TOOLCHAIN_ROOT = INSTALL_ROOT + "/toolchain"
TOOLCHAIN_BIN = TOOLCHAIN_ROOT + "/bin"
AUTHORITY_ROOT = "/var/lib/osl-qa"
HOST_STAGE_ROOT = AUTHORITY_ROOT + "/f1-staging"
HOST_KEY_PATH = AUTHORITY_ROOT + "/private/f1-native-witness.key"
WINDOWS_ROOT = r"C:\ProgramData\OSL-QA"
WINDOWS_STAGE_ROOT = WINDOWS_ROOT + r"\f1-staging"
WINDOWS_KEY_PATH = WINDOWS_ROOT + r"\private\f1-native-witness.key"
WINDOWS_SYSTEM_SID = "S-1-5-18"
RUNTIME_VM = "OSL-Independent-Client-1"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
HASH40_RE = re.compile(r"^[0-9a-f]{40}$")
MODE_RE = re.compile(r"^0[0-7]{3}$")
MAX_JSON_BYTES = 1024 * 1024
TOOL_NAMES = ("cargo", "git", "node", "npm", "osl-cargo", "rustc")
TRANSITION_IDS = (
    "host-producer-account",
    "root-program-installation",
    "protected-toolchain-installation",
    "producer-witness-key",
    "pinned-release-stage",
    "guest-system-provisioning",
    "separately-authorized-runtime",
)
FORBIDDEN_SECRET_FIELDS = {
    "key",
    "keyBytes",
    "keyMaterial",
    "rawKey",
    "secret",
    "secretBytes",
    "seed",
    "pem",
    "privateKey",
    "environment",
    "stdinPayload",
    "importPayload",
}

PROGRAM_PINS = (
    (
        "vmqa_f1_producer.py",
        "65751666b2160eb654e32c0288f054404684ba26a222aeabea86ed3b6a8f988c",
    ),
    (
        "vmqa_build_evidence.py",
        "fc8041443f1841ccf0173e598fb1fcdd80ed03afc24f32608fb4de29137ac625",
    ),
    (
        "vmqa_f1_provisioning_preflight.py",
        "78a71701b9ea169ea621303863a5f57129e0da01d457f40004e9447c7e259b91",
    ),
    (
        "vmqa-f1-windows-provisioning-preflight.ps1",
        "7fcc04012c31a6c1e7ab9bde28f57927ccd3886e4cbc91ff2aef1f246f0dde88",
    ),
    (
        "vmqa-run.sh",
        "0c6df93bb55a17ce04140a142d553e004c733629599b700e02fadb551c4ac216",
    ),
)

PREDECESSOR_SNAPSHOT = {
    "schemaVersion": 1,
    "kind": "vmqa-f1-provisioning-contract",
    "commit": PREDECESSOR_COMMIT,
    "tree": PREDECESSOR_TREE,
    "releaseSource": {
        "commit": PINNED_RELEASE_COMMIT,
        "tree": PINNED_RELEASE_TREE,
    },
    "programPins": [
        {"path": f"{BIN_ROOT}/{name}", "sha256": digest}
        for name, digest in PROGRAM_PINS[:4]
    ],
}


class PlanError(ValueError):
    """The requested plan or manifest is not closed and deterministic."""


class StrictParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise PlanError(f"arguments refused: {message}")


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n"
    ).encode("ascii")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


PREDECESSOR_SNAPSHOT_SHA256 = sha256_bytes(
    canonical_json(PREDECESSOR_SNAPSHOT)
)


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise PlanError(f"{label} fields are not exact")
    return value


def exact_list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise PlanError(f"{label} is not an array")
    return value


def require_exact_value(actual: Any, expected: Any, label: str) -> None:
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or set(actual) != set(expected):
            raise PlanError(f"{label} fields are not exact")
        for key in expected:
            require_exact_value(actual[key], expected[key], f"{label}.{key}")
        return
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise PlanError(f"{label} array is not exact")
        for index, (actual_item, expected_item) in enumerate(
            zip(actual, expected, strict=True)
        ):
            require_exact_value(
                actual_item, expected_item, f"{label}[{index}]"
            )
        return
    if type(actual) is not type(expected) or actual != expected:
        raise PlanError(f"{label} value or JSON type differs")


def require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise PlanError(f"{label} is not a nonempty string")
    return value


def require_sha(value: Any, label: str) -> str:
    text = require_string(value, label)
    if SHA_RE.fullmatch(text) is None:
        raise PlanError(f"{label} is not a SHA-256 digest")
    return text


def require_mode(value: Any, expected: str, label: str) -> None:
    if not isinstance(value, str) or MODE_RE.fullmatch(value) is None:
        raise PlanError(f"{label} mode is invalid")
    if value != expected:
        raise PlanError(f"{label} mode is not {expected}")


def reject_secret_fields(value: Any, label: str = "document") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key in FORBIDDEN_SECRET_FIELDS:
                raise PlanError(f"{label} contains forbidden secret field {key}")
            reject_secret_fields(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            reject_secret_fields(child, f"{label}[{index}]")


def no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise PlanError(f"duplicate JSON field refused: {key}")
        result[key] = value
    return result


def decode_json(raw: bytes, label: str) -> dict[str, Any]:
    if not raw or len(raw) > MAX_JSON_BYTES:
        raise PlanError(f"{label} size is invalid")
    try:
        text = raw.decode("utf-8", errors="strict")
        value = json.loads(text, object_pairs_hook=no_duplicate_object)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PlanError(f"{label} is not strict JSON") from exc
    if not isinstance(value, dict):
        raise PlanError(f"{label} root is not an object")
    reject_secret_fields(value, label)
    return value


def read_regular_once(path: Path, label: str) -> bytes:
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise PlanError(f"{label} cannot be opened safely") from exc
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise PlanError(f"{label} is not a single-link regular file")
        chunks: list[bytes] = []
        total = 0
        while True:
            block = os.read(descriptor, 65536)
            if not block:
                break
            total += len(block)
            if total > MAX_JSON_BYTES:
                raise PlanError(f"{label} is too large")
            chunks.append(block)
        after = os.fstat(descriptor)
        if (
            (before.st_dev, before.st_ino, before.st_size)
            != (after.st_dev, after.st_ino, after.st_size)
        ):
            raise PlanError(f"{label} changed while read")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def fixed_programs() -> list[dict[str, Any]]:
    return [
        {
            "name": name,
            "path": f"{BIN_ROOT}/{name}",
            "owner": "root",
            "group": "root",
            "mode": "0555",
            "sha256": digest,
        }
        for name, digest in PROGRAM_PINS
    ]


def fixed_host_directories(producer_uid: int, producer_gid: int) -> list[dict[str, Any]]:
    root_paths = (
        "/opt",
        INSTALL_ROOT,
        BIN_ROOT,
        TOOLCHAIN_ROOT,
        TOOLCHAIN_BIN,
        "/var",
        "/var/lib",
        "/var/lib/osl-vmqa",
        AUTHORITY_ROOT,
    )
    producer_paths = (
        AUTHORITY_ROOT + "/private",
        HOST_STAGE_ROOT,
        "/var/lib/osl-vmqa/producer-seals",
    )
    return [
        {
            "path": path,
            "owner": "root",
            "uid": 0,
            "group": "root",
            "gid": 0,
            "mode": "0755",
            "kind": "directory",
        }
        for path in root_paths
    ] + [
        {
            "path": path,
            "owner": PRODUCER_NAME,
            "uid": producer_uid,
            "group": PRODUCER_NAME,
            "gid": producer_gid,
            "mode": "0700",
            "kind": "directory",
        }
        for path in producer_paths
    ]


def validate_account(value: Any) -> dict[str, Any]:
    account = exact_object(
        value,
        {"name", "uid", "gid", "home", "shell", "locked"},
        "producer account",
    )
    if (
        account["name"] != PRODUCER_NAME
        or not isinstance(account["uid"], int)
        or isinstance(account["uid"], bool)
        or account["uid"] <= 0
        or not isinstance(account["gid"], int)
        or isinstance(account["gid"], bool)
        or account["gid"] <= 0
        or account["home"] != PRODUCER_HOME
        or account["shell"] != PRODUCER_SHELL
        or account["locked"] is not True
    ):
        raise PlanError("producer account identity or non-login policy differs")
    return dict(account)


def tool_tree_payload(toolchain: dict[str, Any]) -> dict[str, Any]:
    return {
        "root": toolchain["root"],
        "measurementMethod": toolchain["measurementMethod"],
        "tools": toolchain["tools"],
    }


def validate_toolchain(value: Any) -> dict[str, Any]:
    toolchain = exact_object(
        value,
        {
            "root",
            "owner",
            "group",
            "mode",
            "measurementMethod",
            "tools",
            "treeSha256",
        },
        "toolchain",
    )
    if (
        toolchain["root"] != TOOLCHAIN_ROOT
        or toolchain["owner"] != "root"
        or toolchain["group"] != "root"
        or toolchain["measurementMethod"] != "independent-offline-sha256"
    ):
        raise PlanError("toolchain root, owner, or measurement method differs")
    require_mode(toolchain["mode"], "0755", "toolchain root")
    tools = exact_list(toolchain["tools"], "toolchain tools")
    if len(tools) != len(TOOL_NAMES):
        raise PlanError("toolchain tool inventory is not exact")
    normalized: list[dict[str, Any]] = []
    for expected_name, raw in zip(TOOL_NAMES, tools, strict=True):
        tool = exact_object(
            raw,
            {"name", "path", "owner", "group", "mode", "sha256"},
            f"tool {expected_name}",
        )
        expected_path = f"{TOOLCHAIN_BIN}/{expected_name}"
        if (
            tool["name"] != expected_name
            or tool["path"] != expected_path
            or tool["owner"] != "root"
            or tool["group"] != "root"
        ):
            raise PlanError(f"tool {expected_name} identity or owner differs")
        try:
            PurePosixPath(tool["path"]).relative_to(TOOLCHAIN_ROOT)
        except ValueError as exc:
            raise PlanError(f"tool {expected_name} escapes fixed root") from exc
        require_mode(tool["mode"], "0555", f"tool {expected_name}")
        require_sha(tool["sha256"], f"tool {expected_name} hash")
        normalized.append(dict(tool))
    normalized_toolchain = dict(toolchain)
    normalized_toolchain["tools"] = normalized
    expected_tree = sha256_bytes(canonical_json(tool_tree_payload(normalized_toolchain)))
    if toolchain["treeSha256"] != expected_tree:
        raise PlanError("toolchain tree hash does not bind the exact inventory")
    require_sha(toolchain["treeSha256"], "toolchain tree hash")
    return normalized_toolchain


def validate_witness_key(value: Any, account: dict[str, Any]) -> dict[str, Any]:
    key = exact_object(
        value,
        {
            "path",
            "present",
            "owner",
            "uid",
            "group",
            "gid",
            "mode",
            "keyId",
            "keyBytesRead",
        },
        "witness-key metadata",
    )
    if (
        key["path"] != HOST_KEY_PATH
        or key["present"] is not True
        or key["owner"] != PRODUCER_NAME
        or not isinstance(key["uid"], int)
        or isinstance(key["uid"], bool)
        or key["uid"] != account["uid"]
        or key["group"] != PRODUCER_NAME
        or not isinstance(key["gid"], int)
        or isinstance(key["gid"], bool)
        or key["gid"] != account["gid"]
        or key["keyBytesRead"] is not False
    ):
        raise PlanError("witness-key presence metadata differs")
    require_mode(key["mode"], "0600", "witness key")
    require_sha(key["keyId"], "witness key ID")
    return dict(key)


def host_stage_path(executable_sha256: str) -> str:
    return f"{HOST_STAGE_ROOT}/{executable_sha256}"


def validate_release(value: Any) -> dict[str, Any]:
    release = exact_object(
        value,
        {
            "sourceCommit",
            "sourceTree",
            "bundleManifestSha256",
            "buildIdentitySha256",
            "executableSha256",
            "loaderSha256",
            "producerSealSha256",
            "terminalSnapshotSha256",
            "sealGeneration",
            "previousSealSha256",
            "transition",
            "stagePath",
        },
        "release",
    )
    if (
        release["sourceCommit"] != PINNED_RELEASE_COMMIT
        or release["sourceTree"] != PINNED_RELEASE_TREE
    ):
        raise PlanError("release source commit or tree differs")
    for field in (
        "bundleManifestSha256",
        "buildIdentitySha256",
        "executableSha256",
        "loaderSha256",
        "producerSealSha256",
        "terminalSnapshotSha256",
        "previousSealSha256",
    ):
        require_sha(release[field], f"release {field}")
    if (
        not isinstance(release["sealGeneration"], int)
        or isinstance(release["sealGeneration"], bool)
        or release["sealGeneration"] != 1
        or release["previousSealSha256"] != EMPTY_SHA256
        or release["transition"] != "initial"
    ):
        raise PlanError("release does not directly descend from the empty predecessor")
    expected_stage = host_stage_path(release["executableSha256"])
    if release["stagePath"] != expected_stage:
        raise PlanError("release stage path is not derived from executable hash")
    return dict(release)


def validate_acl_template(value: Any) -> dict[str, Any]:
    acl = exact_object(
        value,
        {
            "ownerSid",
            "daclProtected",
            "inheritanceEnabled",
            "ace",
        },
        "guest ACL template",
    )
    ace = exact_object(
        acl["ace"],
        {"sid", "type", "rights", "inherited", "propagation"},
        "guest ACL ACE",
    )
    if (
        acl["ownerSid"] != WINDOWS_SYSTEM_SID
        or acl["daclProtected"] is not True
        or acl["inheritanceEnabled"] is not False
        or ace
        != {
            "sid": WINDOWS_SYSTEM_SID,
            "type": "Allow",
            "rights": "FullControl",
            "inherited": False,
            "propagation": "None",
        }
    ):
        raise PlanError("guest ACL is not protected SYSTEM-only FullControl")
    return {
        **acl,
        "ace": dict(ace),
    }


def guest_paths(executable_sha256: str) -> list[tuple[str, str]]:
    stage = WINDOWS_STAGE_ROOT + "\\" + executable_sha256
    return [
        (WINDOWS_ROOT, "directory"),
        (WINDOWS_ROOT + r"\bin", "directory"),
        (WINDOWS_STAGE_ROOT, "directory"),
        (stage, "directory"),
        (stage + r"\bundle", "directory"),
        (stage + r"\bundle\outputs", "directory"),
        (stage + r"\bundle\build-evidence", "directory"),
        (stage + r"\bundle\outputs\dist", "directory"),
        (stage + r"\bundle\build-identity.json", "file"),
        (stage + r"\bundle\outputs\osl-privacy-hub.exe", "file"),
        (stage + r"\bundle\outputs\WebView2Loader.dll", "file"),
        (stage + r"\producer-seal.json", "file"),
        (stage + r"\staging-receipt.json", "file"),
        (WINDOWS_ROOT + r"\private", "directory"),
        (WINDOWS_KEY_PATH, "file"),
    ]


def guest_acl_entries(
    executable_sha256: str, acl: dict[str, Any]
) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for path, kind in guest_paths(executable_sha256):
        inheritance = (
            "ContainerInherit,ObjectInherit" if kind == "directory" else "None"
        )
        entries.append(
            {
                "path": path,
                "kind": kind,
                "ownerSid": acl["ownerSid"],
                "daclProtected": acl["daclProtected"],
                "inheritanceEnabled": acl["inheritanceEnabled"],
                "aces": [
                    {
                        **acl["ace"],
                        "inheritance": inheritance,
                    }
                ],
            }
        )
    return entries


def fixed_contract() -> dict[str, Any]:
    return {
        "provisioningCommit": PREDECESSOR_COMMIT,
        "provisioningTree": PREDECESSOR_TREE,
        "predecessorSnapshotSha256": PREDECESSOR_SNAPSHOT_SHA256,
        "releaseCommit": PINNED_RELEASE_COMMIT,
        "releaseTree": PINNED_RELEASE_TREE,
    }


def fixed_runtime(release: dict[str, Any]) -> dict[str, Any]:
    return {
        "authorization": "separate-live-f1-runtime",
        "authorizationRequired": True,
        "automaticExecution": False,
        "commandEncoding": "argv",
        "argv": [
            f"{BIN_ROOT}/vmqa-run.sh",
            "selftest",
            "--vm",
            RUNTIME_VM,
            "--bundle-dir",
            release["stagePath"] + "/bundle",
        ],
    }


def fixed_transitions() -> list[dict[str, Any]]:
    authorities = (
        "human-host-administrator",
        "human-host-administrator",
        "human-host-administrator",
        "dedicated-producer",
        "dedicated-producer",
        "authorized-guest-provisioner",
        "separate-live-runtime-authority",
    )
    bindings = (
        ["/producerAccount"],
        ["/hostDirectories", "/programs"],
        ["/toolchain"],
        ["/witnessKey"],
        ["/release", "/contract"],
        ["/guestAcl", "/release/executableSha256"],
        ["/runtime", "/release", "/guestAcl"],
    )
    return [
        {
            "ordinal": index,
            "id": transition_id,
            "authority": authority,
            "status": "planned",
            "writesPerformed": 0,
            "bindsTo": binding,
        }
        for index, (transition_id, authority, binding) in enumerate(
            zip(TRANSITION_IDS, authorities, bindings, strict=True), start=1
        )
    ]


def validate_plan_input(value: Any) -> dict[str, Any]:
    reject_secret_fields(value, "plan input")
    root = exact_object(
        value,
        {
            "schemaVersion",
            "kind",
            "predecessorSnapshotSha256",
            "producerAccount",
            "toolchain",
            "witnessKey",
            "release",
            "guestAcl",
        },
        "plan input",
    )
    if (
        not isinstance(root["schemaVersion"], int)
        or isinstance(root["schemaVersion"], bool)
        or root["schemaVersion"] != SCHEMA_VERSION
        or root["kind"] != INPUT_KIND
    ):
        raise PlanError("plan input schema or kind differs")
    if root["predecessorSnapshotSha256"] != PREDECESSOR_SNAPSHOT_SHA256:
        raise PlanError("predecessor snapshot lineage is stale or forked")
    account = validate_account(root["producerAccount"])
    toolchain = validate_toolchain(root["toolchain"])
    key = validate_witness_key(root["witnessKey"], account)
    release = validate_release(root["release"])
    acl = validate_acl_template(root["guestAcl"])
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": INPUT_KIND,
        "predecessorSnapshotSha256": PREDECESSOR_SNAPSHOT_SHA256,
        "producerAccount": account,
        "toolchain": toolchain,
        "witnessKey": key,
        "release": release,
        "guestAcl": acl,
    }


def build_payload(plan_input: dict[str, Any]) -> dict[str, Any]:
    normalized = validate_plan_input(plan_input)
    account = normalized["producerAccount"]
    release = normalized["release"]
    acl = normalized["guestAcl"]
    return {
        "status": "planned",
        "assurance": "offline-plan-only",
        "executionPermitted": False,
        "writesPerformed": 0,
        "contract": fixed_contract(),
        "producerAccount": account,
        "hostDirectories": fixed_host_directories(
            account["uid"], account["gid"]
        ),
        "programs": fixed_programs(),
        "toolchain": normalized["toolchain"],
        "witnessKey": normalized["witnessKey"],
        "release": release,
        "guestAcl": {
            "template": acl,
            "entries": guest_acl_entries(release["executableSha256"], acl),
        },
        "runtime": fixed_runtime(release),
        "operatorTransitions": fixed_transitions(),
    }


def generate_manifest(plan_input: dict[str, Any]) -> dict[str, Any]:
    payload = build_payload(plan_input)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": MANIFEST_KIND,
        "payload": payload,
        "payloadSha256": sha256_bytes(canonical_json(payload)),
    }


def verify_manifest(value: Any) -> dict[str, Any]:
    reject_secret_fields(value, "plan manifest")
    manifest = exact_object(
        value,
        {"schemaVersion", "kind", "payload", "payloadSha256"},
        "plan manifest",
    )
    if (
        not isinstance(manifest["schemaVersion"], int)
        or isinstance(manifest["schemaVersion"], bool)
        or manifest["schemaVersion"] != SCHEMA_VERSION
        or manifest["kind"] != MANIFEST_KIND
    ):
        raise PlanError("plan manifest schema or kind differs")
    payload = exact_object(
        manifest["payload"],
        {
            "status",
            "assurance",
            "executionPermitted",
            "writesPerformed",
            "contract",
            "producerAccount",
            "hostDirectories",
            "programs",
            "toolchain",
            "witnessKey",
            "release",
            "guestAcl",
            "runtime",
            "operatorTransitions",
        },
        "plan payload",
    )
    if (
        payload["status"] != "planned"
        or payload["assurance"] != "offline-plan-only"
        or payload["executionPermitted"] is not False
        or not isinstance(payload["writesPerformed"], int)
        or isinstance(payload["writesPerformed"], bool)
        or payload["writesPerformed"] != 0
    ):
        raise PlanError("plan attempts to claim readiness or execution")
    require_exact_value(
        payload["contract"], fixed_contract(), "plan contract"
    )
    account = validate_account(payload["producerAccount"])
    expected_directories = fixed_host_directories(
        account["uid"], account["gid"]
    )
    require_exact_value(
        payload["hostDirectories"],
        expected_directories,
        "host directory plan",
    )
    require_exact_value(
        payload["programs"], fixed_programs(), "fixed program plan"
    )
    toolchain = validate_toolchain(payload["toolchain"])
    key = validate_witness_key(payload["witnessKey"], account)
    release = validate_release(payload["release"])
    guest = exact_object(
        payload["guestAcl"], {"template", "entries"}, "guest ACL plan"
    )
    acl = validate_acl_template(guest["template"])
    expected_entries = guest_acl_entries(release["executableSha256"], acl)
    require_exact_value(
        guest["entries"], expected_entries, "guest ACL entries"
    )
    require_exact_value(
        payload["runtime"], fixed_runtime(release), "runtime plan"
    )
    require_exact_value(
        payload["operatorTransitions"],
        fixed_transitions(),
        "operator transitions",
    )
    for transition in payload["operatorTransitions"]:
        if (
            not isinstance(transition["ordinal"], int)
            or isinstance(transition["ordinal"], bool)
            or not isinstance(transition["writesPerformed"], int)
            or isinstance(transition["writesPerformed"], bool)
        ):
            raise PlanError("transition integer fields are not JSON integers")
    expected_sha = sha256_bytes(canonical_json(payload))
    if manifest["payloadSha256"] != expected_sha:
        raise PlanError("plan payload hash differs")
    require_sha(manifest["payloadSha256"], "plan payload hash")
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "valid-plan-only",
        "payloadSha256": expected_sha,
        "sourceCommit": release["sourceCommit"],
        "sourceTree": release["sourceTree"],
        "bundleManifestSha256": release["bundleManifestSha256"],
        "toolchainTreeSha256": toolchain["treeSha256"],
        "predecessorSnapshotSha256": PREDECESSOR_SNAPSHOT_SHA256,
        "keyId": key["keyId"],
        "writesPerformed": 0,
        "executionPermitted": False,
    }


def load_json_path(path_text: str, label: str) -> dict[str, Any]:
    path = Path(path_text)
    if not path.is_absolute():
        raise PlanError(f"{label} path must be absolute")
    return decode_json(read_regular_once(path, label), label)


def build_parser() -> StrictParser:
    parser = StrictParser(allow_abbrev=False)
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", allow_abbrev=False)
    plan.add_argument("--input", required=True)
    verify = commands.add_parser("verify", allow_abbrev=False)
    verify.add_argument("--manifest", required=True)
    commands.add_parser("execute", allow_abbrev=False)
    return parser


def main(argv: list[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        if args.command == "execute":
            raise PlanError(
                "mutation execution is forbidden; this tool only plans or verifies"
            )
        if args.command == "plan":
            value = generate_manifest(load_json_path(args.input, "plan input"))
        elif args.command == "verify":
            value = verify_manifest(
                load_json_path(args.manifest, "plan manifest")
            )
        else:
            raise PlanError("unknown operation")
        sys.stdout.write(canonical_json(value).decode("ascii"))
        return 0
    except PlanError as exc:
        print(f"VMQA F1 PLAN REFUSED: {exc}", file=sys.stderr)
        return 9


if __name__ == "__main__":
    raise SystemExit(main())
