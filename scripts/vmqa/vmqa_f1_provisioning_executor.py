#!/usr/bin/python3 -I
"""Fail-closed execution seam for the seven-step F1 provisioning plan.

Production execution has no trusted authorization receipt pinned yet.  The
pure state-machine entry point exists so crash/retry and target-binding
behavior can be tested without touching a host, Azure, Windows, or a secret.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
import stat
import sys
from pathlib import Path
from collections.abc import Callable
from typing import Any, Protocol


SCRIPT_ROOT = Path(__file__).resolve().parent
PLAN_PROGRAM = SCRIPT_ROOT / "vmqa_f1_provisioning_plan.py"
PLAN_SPEC = importlib.util.spec_from_file_location(
    "_vmqa_f1_provisioning_plan", PLAN_PROGRAM
)
if PLAN_SPEC is None or PLAN_SPEC.loader is None:
    raise RuntimeError("cannot load pinned provisioning-plan validator")
plan = importlib.util.module_from_spec(PLAN_SPEC)
PLAN_SPEC.loader.exec_module(plan)

SCHEMA_VERSION = 1
AUTHORIZATION_KIND = "vmqa-f1-provisioning-authorization"
CHECKPOINT_KIND = "vmqa-f1-provisioning-checkpoint"
RECEIPT_KIND = "vmqa-f1-provisioning-transition-receipt"
COORDINATOR_COMMIT = "95e32b97db5122dbdb21e8531e6111af24a5e7fa"
COORDINATOR_BLOB = "80b4e5a736a0b4125b01210f965a5eaa9ea754af"
COORDINATOR_FILE_SHA256 = (
    "1604dc25e1f85d88a447905325e645973df8c91f5083a274047e3f4b5fb38f06"
)
COORDINATOR_PATH = "docs/reports/coordinator-state-2026-07-26.md"
RECORDED_DECISION = (
    "target-owned-by-scrub-and-explicit-owner-authorization-not-granted"
)
TENANT_ID = "79981b01-1944-4da0-aa9a-fb9f63bddb5e"
SUBSCRIPTION_NAME = "Azure for Students"
RESOURCE_GROUP = "OSL-TWO-CLIENT-LAB-INDEPENDENT"
VM_NAME = "OSL-Independent-Client-1"
VM_ID = "a36c21c3-c563-4320-a8eb-70bc588c7caf"
SESSION_ID = 1
WARM_SNAPSHOT = "OSL-Independent-Client-1-WARM-iteration-20260726"
WARM_LINEAGE_TAG = "warm-iteration"
RUNTIME_AUTHORITY = "separate-live-f1-runtime"
FIXED_EXECUTOR_PATH = "/opt/osl-vmqa/bin/vmqa_f1_provisioning_executor.py"
PLAN_STORE = "/var/lib/osl-vmqa/plans"
AUTHORIZATION_STORE = "/var/lib/osl-vmqa/authorizations"
STATE_STORE = "/var/lib/osl-vmqa/executor-state"
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SHA_RE = plan.SHA_RE
MAX_JSON_BYTES = plan.MAX_JSON_BYTES

# This is intentionally absent.  A future, separately reviewed source commit
# must pin the independently issued authorization receipt before production
# execution can pass.  A caller-supplied receipt or token array is not trust.
PINNED_AUTHORIZATION_RECEIPT_SHA256: str | None = None

AUTHORIZATION_TOKENS = (
    "vmqa-f1-provisioning-executor-v1",
    f"coordinator-commit:{COORDINATOR_COMMIT}",
    f"coordinator-blob:{COORDINATOR_BLOB}",
    f"coordinator-file-sha256:{COORDINATOR_FILE_SHA256}",
    f"tenant:{TENANT_ID}",
    f"subscription-name:{SUBSCRIPTION_NAME}",
    f"resource-group:{RESOURCE_GROUP}",
    f"vm:{VM_NAME}",
    f"vm-id:{VM_ID}",
    f"session:{SESSION_ID}",
    f"warm-snapshot:{WARM_SNAPSHOT}",
    f"warm-lineage:{WARM_LINEAGE_TAG}",
    f"runtime-authority:{RUNTIME_AUTHORITY}",
    "transition-count:7",
)


class ExecutorError(ValueError):
    """The requested execution is not exactly authorized and bound."""


class SimulatedCrash(RuntimeError):
    """A fixture-only crash after a simulated transition mutation."""


class StrictParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise ExecutorError(f"arguments refused: {message}")


class TransitionBackend(Protocol):
    def observe(self) -> dict[str, Any]:
        """Return the current target and completed-transition bindings."""

    def apply(
        self,
        transition_id: str,
        idempotency_key: str,
        authorization_sha256: str,
    ) -> None:
        """Apply exactly one transition, idempotently."""


def canonical_json(value: object) -> bytes:
    return plan.canonical_json(value)


def sha256_bytes(value: bytes) -> str:
    return plan.sha256_bytes(value)


def exact_object(
    value: Any, keys: set[str], label: str
) -> dict[str, Any]:
    try:
        return plan.exact_object(value, keys, label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def require_sha(value: Any, label: str) -> str:
    try:
        return plan.require_sha(value, label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def require_exact_value(actual: Any, expected: Any, label: str) -> None:
    try:
        plan.require_exact_value(actual, expected, label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def reject_secret_fields(value: Any, label: str) -> None:
    try:
        plan.reject_secret_fields(value, label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def json_pointer(value: Any, pointer: str) -> Any:
    if not pointer.startswith("/") or pointer == "/":
        raise ExecutorError("transition binding pointer is invalid")
    current = value
    for token in pointer[1:].split("/"):
        token = token.replace("~1", "/").replace("~0", "~")
        if not isinstance(current, dict) or token not in current:
            raise ExecutorError("transition binding pointer is unresolved")
        current = current[token]
    return current


def transition_binding_hash(
    payload: dict[str, Any], transition: dict[str, Any]
) -> str:
    bound = [
        {"pointer": pointer, "value": json_pointer(payload, pointer)}
        for pointer in transition["bindsTo"]
    ]
    return sha256_bytes(canonical_json(bound))


def plan_bindings(manifest: dict[str, Any]) -> dict[str, Any]:
    try:
        verified = plan.verify_manifest(manifest)
    except plan.PlanError as exc:
        raise ExecutorError(f"plan refused: {exc}") from exc
    payload = manifest["payload"]
    transitions = payload["operatorTransitions"]
    binding_hashes = {
        transition["id"]: transition_binding_hash(payload, transition)
        for transition in transitions
    }
    binding = {
        "planPayloadSha256": verified["payloadSha256"],
        "predecessorSnapshotSha256": verified[
            "predecessorSnapshotSha256"
        ],
        "sourceCommit": verified["sourceCommit"],
        "sourceTree": verified["sourceTree"],
        "bundleManifestSha256": verified["bundleManifestSha256"],
        "releaseExecutableSha256": payload["release"]["executableSha256"],
        "releaseTerminalSnapshotSha256": payload["release"][
            "terminalSnapshotSha256"
        ],
        "releaseProducerSealSha256": payload["release"][
            "producerSealSha256"
        ],
        "toolchainTreeSha256": verified["toolchainTreeSha256"],
        "guestAclSha256": sha256_bytes(
            canonical_json(payload["guestAcl"])
        ),
        "runtimeSha256": sha256_bytes(canonical_json(payload["runtime"])),
        "transitionBindingSha256": binding_hashes,
    }
    binding["bindingSha256"] = sha256_bytes(canonical_json(binding))
    return binding


def fixed_target() -> dict[str, Any]:
    return {
        "tenantId": TENANT_ID,
        "subscriptionName": SUBSCRIPTION_NAME,
        "resourceGroup": RESOURCE_GROUP,
        "vmName": VM_NAME,
        "vmId": VM_ID,
        "interactiveSessionId": SESSION_ID,
        "warmSnapshot": WARM_SNAPSHOT,
        "warmLineageTag": WARM_LINEAGE_TAG,
        "warmVerified": True,
        "runtimeAuthority": RUNTIME_AUTHORITY,
    }


def authorization_template(
    manifest: dict[str, Any],
    *,
    authorization_sequence: int,
    previous_authorization_sha256: str,
    run_nonce: str,
) -> dict[str, Any]:
    """Build fixture input; it is not trusted until its hash is source-pinned."""
    if (
        not isinstance(authorization_sequence, int)
        or isinstance(authorization_sequence, bool)
        or authorization_sequence <= 0
    ):
        raise ExecutorError("authorization sequence is invalid")
    require_sha(previous_authorization_sha256, "previous authorization")
    require_sha(run_nonce, "run nonce")
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": AUTHORIZATION_KIND,
        "decision": "explicit-live-f1-provisioning-authorized",
        "authority": {
            "coordinatorCommit": COORDINATOR_COMMIT,
            "coordinatorBlob": COORDINATOR_BLOB,
            "coordinatorPath": COORDINATOR_PATH,
            "coordinatorFileSha256": COORDINATOR_FILE_SHA256,
        },
        "authorizationTokens": list(AUTHORIZATION_TOKENS),
        "authorizationSequence": authorization_sequence,
        "previousAuthorizationSha256": previous_authorization_sha256,
        "runNonce": run_nonce,
        "target": fixed_target(),
        "bindings": plan_bindings(manifest),
    }


def validate_authorization(
    value: Any,
    manifest: dict[str, Any],
    *,
    trusted_authorization_sha256: str | None = None,
) -> tuple[dict[str, Any], str]:
    reject_secret_fields(value, "authorization")
    auth = exact_object(
        value,
        {
            "schemaVersion",
            "kind",
            "decision",
            "authority",
            "authorizationTokens",
            "authorizationSequence",
            "previousAuthorizationSha256",
            "runNonce",
            "target",
            "bindings",
        },
        "authorization",
    )
    if (
        not isinstance(auth["schemaVersion"], int)
        or isinstance(auth["schemaVersion"], bool)
        or auth["schemaVersion"] != SCHEMA_VERSION
    ):
        raise ExecutorError("authorization schema differs")
    if auth["kind"] != AUTHORIZATION_KIND:
        raise ExecutorError("authorization kind differs")
    if auth["decision"] != "explicit-live-f1-provisioning-authorized":
        raise ExecutorError("authorization decision is not affirmative")
    expected_authority = {
        "coordinatorCommit": COORDINATOR_COMMIT,
        "coordinatorBlob": COORDINATOR_BLOB,
        "coordinatorPath": COORDINATOR_PATH,
        "coordinatorFileSha256": COORDINATOR_FILE_SHA256,
    }
    require_exact_value(
        auth["authority"],
        expected_authority,
        "coordinator authority snapshot",
    )
    require_exact_value(
        auth["authorizationTokens"],
        list(AUTHORIZATION_TOKENS),
        "fixed authorization token array",
    )
    sequence = auth["authorizationSequence"]
    if (
        not isinstance(sequence, int)
        or isinstance(sequence, bool)
        or sequence <= 0
    ):
        raise ExecutorError("authorization sequence is invalid")
    require_sha(auth["previousAuthorizationSha256"], "previous authorization")
    require_sha(auth["runNonce"], "run nonce")
    require_exact_value(
        auth["target"],
        fixed_target(),
        "authorization tenant/VM/session/lineage",
    )
    expected_bindings = plan_bindings(manifest)
    require_exact_value(
        auth["bindings"],
        expected_bindings,
        "authorization plan/hash/ACL/runtime binding",
    )
    auth_sha = sha256_bytes(canonical_json(auth))
    pin = (
        PINNED_AUTHORIZATION_RECEIPT_SHA256
        if trusted_authorization_sha256 is None
        else trusted_authorization_sha256
    )
    if pin is None:
        raise ExecutorError(
            "production authorization is not source-pinned; "
            "recorded coordinator decision remains not granted"
        )
    require_sha(pin, "trusted authorization pin")
    if auth_sha != pin:
        raise ExecutorError("authorization receipt is not the source-pinned object")
    return copy.deepcopy(auth), auth_sha


def expected_operator_paths(plan_sha: str) -> dict[str, str]:
    require_sha(plan_sha, "plan payload hash")
    return {
        "manifest": f"{PLAN_STORE}/{plan_sha}.json",
        "authorization": f"{AUTHORIZATION_STORE}/{plan_sha}.json",
        "stateRoot": f"{STATE_STORE}/{plan_sha}",
    }


def operator_argv(manifest: dict[str, Any]) -> list[str]:
    binding = plan_bindings(manifest)
    paths = expected_operator_paths(binding["planPayloadSha256"])
    return [
        FIXED_EXECUTOR_PATH,
        "execute",
        "--manifest",
        paths["manifest"],
        "--authorization",
        paths["authorization"],
        "--state-root",
        paths["stateRoot"],
    ]


def gate_report(manifest: dict[str, Any]) -> dict[str, Any]:
    bindings = plan_bindings(manifest)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": "vmqa-f1-provisioning-execution-gate",
        "status": "blocked",
        "reason": RECORDED_DECISION,
        "coordinator": {
            "commit": COORDINATOR_COMMIT,
            "blob": COORDINATOR_BLOB,
            "path": COORDINATOR_PATH,
            "fileSha256": COORDINATOR_FILE_SHA256,
        },
        "target": fixed_target(),
        "bindings": bindings,
        "authorizationPinPresent": (
            PINNED_AUTHORIZATION_RECEIPT_SHA256 is not None
        ),
        "readOnlyAzurePreflightRun": False,
        "writesPerformed": 0,
        "executionPermitted": False,
        "operatorArgv": operator_argv(manifest),
    }


def initial_checkpoint(
    manifest: dict[str, Any],
    authorization: dict[str, Any],
    authorization_sha: str,
) -> dict[str, Any]:
    bindings = plan_bindings(manifest)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": CHECKPOINT_KIND,
        "mode": "simulation",
        "planPayloadSha256": bindings["planPayloadSha256"],
        "planBindingSha256": bindings["bindingSha256"],
        "authorizationSha256": authorization_sha,
        "authorizationSequence": authorization["authorizationSequence"],
        "previousAuthorizationSha256": authorization[
            "previousAuthorizationSha256"
        ],
        "runNonce": authorization["runNonce"],
        "nextOrdinal": 1,
        "receiptChainHeadSha256": EMPTY_SHA256,
        "receipts": [],
    }


def validate_checkpoint(
    value: Any,
    manifest: dict[str, Any],
    authorization: dict[str, Any],
    authorization_sha: str,
) -> dict[str, Any]:
    checkpoint = exact_object(
        value,
        {
            "schemaVersion",
            "kind",
            "mode",
            "planPayloadSha256",
            "planBindingSha256",
            "authorizationSha256",
            "authorizationSequence",
            "previousAuthorizationSha256",
            "runNonce",
            "nextOrdinal",
            "receiptChainHeadSha256",
            "receipts",
        },
        "checkpoint",
    )
    bindings = plan_bindings(manifest)
    expected_header = {
        "schemaVersion": SCHEMA_VERSION,
        "kind": CHECKPOINT_KIND,
        "mode": "simulation",
        "planPayloadSha256": bindings["planPayloadSha256"],
        "planBindingSha256": bindings["bindingSha256"],
        "authorizationSha256": authorization_sha,
        "authorizationSequence": authorization["authorizationSequence"],
        "previousAuthorizationSha256": authorization[
            "previousAuthorizationSha256"
        ],
        "runNonce": authorization["runNonce"],
    }
    for key, expected in expected_header.items():
        if type(checkpoint[key]) is not type(expected) or checkpoint[key] != expected:
            raise ExecutorError(f"checkpoint {key} differs")
    receipts = checkpoint["receipts"]
    if not isinstance(receipts, list):
        raise ExecutorError("checkpoint receipts are not an array")
    transitions = manifest["payload"]["operatorTransitions"]
    if len(receipts) > len(transitions):
        raise ExecutorError("checkpoint has excess receipts")
    previous = EMPTY_SHA256
    for index, receipt in enumerate(receipts):
        transition = transitions[index]
        exact_object(
            receipt,
            {
                "schemaVersion",
                "kind",
                "mode",
                "ordinal",
                "transitionId",
                "completion",
                "attempt",
                "idempotencyKey",
                "planPayloadSha256",
                "planBindingSha256",
                "transitionBindingSha256",
                "authorizationSha256",
                "authorizationSequence",
                "runNonce",
                "targetSha256",
                "preObservationSha256",
                "postObservationSha256",
                "previousReceiptSha256",
            },
            f"receipt {index + 1}",
        )
        if (
            not isinstance(receipt["schemaVersion"], int)
            or isinstance(receipt["schemaVersion"], bool)
            or receipt["schemaVersion"] != SCHEMA_VERSION
            or receipt["kind"] != RECEIPT_KIND
            or receipt["mode"] != "simulation"
            or not isinstance(receipt["ordinal"], int)
            or isinstance(receipt["ordinal"], bool)
            or receipt["ordinal"] != transition["ordinal"]
            or receipt["transitionId"] != transition["id"]
            or receipt["completion"] not in {"applied", "resumed"}
            or not isinstance(receipt["attempt"], int)
            or isinstance(receipt["attempt"], bool)
            or receipt["attempt"] <= 0
            or receipt["planPayloadSha256"]
            != bindings["planPayloadSha256"]
            or receipt["planBindingSha256"] != bindings["bindingSha256"]
            or receipt["transitionBindingSha256"]
            != bindings["transitionBindingSha256"][transition["id"]]
            or receipt["authorizationSha256"] != authorization_sha
            or receipt["authorizationSequence"]
            != authorization["authorizationSequence"]
            or receipt["runNonce"] != authorization["runNonce"]
            or receipt["targetSha256"]
            != sha256_bytes(canonical_json(fixed_target()))
            or receipt["previousReceiptSha256"] != previous
        ):
            raise ExecutorError("receipt chain or exact binding differs")
        for key in (
            "idempotencyKey",
            "preObservationSha256",
            "postObservationSha256",
        ):
            require_sha(receipt[key], f"receipt {key}")
        expected_key = idempotency_key(
            manifest, authorization_sha, transition
        )
        if receipt["idempotencyKey"] != expected_key:
            raise ExecutorError("receipt idempotency key differs")
        previous = sha256_bytes(canonical_json(receipt))
    if checkpoint["receiptChainHeadSha256"] != previous:
        raise ExecutorError("checkpoint receipt-chain head differs")
    if checkpoint["nextOrdinal"] != len(receipts) + 1:
        raise ExecutorError("checkpoint next ordinal differs")
    return copy.deepcopy(checkpoint)


def idempotency_key(
    manifest: dict[str, Any],
    authorization_sha: str,
    transition: dict[str, Any],
) -> str:
    bindings = plan_bindings(manifest)
    return sha256_bytes(
        canonical_json(
            {
                "planPayloadSha256": bindings["planPayloadSha256"],
                "planBindingSha256": bindings["bindingSha256"],
                "authorizationSha256": authorization_sha,
                "ordinal": transition["ordinal"],
                "transitionId": transition["id"],
                "transitionBindingSha256": bindings[
                    "transitionBindingSha256"
                ][transition["id"]],
            }
        )
    )


def validate_observation(
    value: Any,
    manifest: dict[str, Any],
    *,
    expected_completed: list[str],
    expected_authorization_head: str,
) -> dict[str, Any]:
    observation = exact_object(
        value,
        {
            "target",
            "runtimeSha256",
            "completedBindingSha256",
            "authorizationHeadSha256",
        },
        "backend observation",
    )
    require_exact_value(
        observation["target"],
        fixed_target(),
        "observed tenant/VM/session/lineage",
    )
    bindings = plan_bindings(manifest)
    if observation["runtimeSha256"] != bindings["runtimeSha256"]:
        raise ExecutorError("observed runtime differs")
    completed = observation["completedBindingSha256"]
    if not isinstance(completed, dict):
        raise ExecutorError("observed transition bindings are not an object")
    allowed = bindings["transitionBindingSha256"]
    if not set(completed).issubset(set(allowed)):
        raise ExecutorError("observed transition inventory differs")
    for transition_id, digest in completed.items():
        require_sha(digest, f"observed {transition_id} binding")
        if digest != allowed[transition_id]:
            raise ExecutorError(
                f"observed {transition_id} hash/ACL binding differs"
            )
    if list(completed) != expected_completed:
        raise ExecutorError(
            "observed completed transitions are not the exact ordered prefix"
        )
    require_sha(
        observation["authorizationHeadSha256"],
        "observed authorization head",
    )
    if observation["authorizationHeadSha256"] != expected_authorization_head:
        raise ExecutorError("authorization lineage is stale or replayed")
    return copy.deepcopy(observation)


def execute_simulation(
    manifest: dict[str, Any],
    authorization: dict[str, Any],
    *,
    trusted_authorization_sha256: str,
    backend: TransitionBackend,
    checkpoint: dict[str, Any] | None = None,
    crash_after_apply: str | None = None,
    checkpoint_sink: Callable[[dict[str, Any]], None] | None = None,
) -> dict[str, Any]:
    """Run the pure state machine.  Not reachable from the production CLI."""
    normalized_auth, auth_sha = validate_authorization(
        authorization,
        manifest,
        trusted_authorization_sha256=trusted_authorization_sha256,
    )
    state = (
        initial_checkpoint(manifest, normalized_auth, auth_sha)
        if checkpoint is None
        else validate_checkpoint(
            checkpoint, manifest, normalized_auth, auth_sha
        )
    )
    transitions = manifest["payload"]["operatorTransitions"]
    while state["nextOrdinal"] <= len(transitions):
        transition = transitions[state["nextOrdinal"] - 1]
        transition_id = transition["id"]
        completed_before = [
            item["id"] for item in transitions[: state["nextOrdinal"] - 1]
        ]
        raw_pre = backend.observe()
        pre_completed = raw_pre.get("completedBindingSha256")
        if not isinstance(pre_completed, dict):
            raise ExecutorError(
                "observed transition bindings are not an object"
            )
        already_applied = transition_id in pre_completed
        expected_pre = completed_before + (
            [transition_id] if already_applied else []
        )
        pre = validate_observation(
            raw_pre,
            manifest,
            expected_completed=expected_pre,
            expected_authorization_head=(
                auth_sha
                if already_applied or completed_before
                else normalized_auth["previousAuthorizationSha256"]
            ),
        )
        pre_sha = sha256_bytes(canonical_json(pre))
        key = idempotency_key(manifest, auth_sha, transition)
        if not already_applied:
            backend.apply(transition_id, key, auth_sha)
            if crash_after_apply == transition_id:
                raise SimulatedCrash(
                    f"simulated crash after {transition_id} apply"
                )
        post = validate_observation(
            backend.observe(),
            manifest,
            expected_completed=completed_before + [transition_id],
            expected_authorization_head=auth_sha,
        )
        receipt = {
            "schemaVersion": SCHEMA_VERSION,
            "kind": RECEIPT_KIND,
            "mode": "simulation",
            "ordinal": transition["ordinal"],
            "transitionId": transition_id,
            "completion": "resumed" if already_applied else "applied",
            "attempt": 1,
            "idempotencyKey": key,
            "planPayloadSha256": state["planPayloadSha256"],
            "planBindingSha256": state["planBindingSha256"],
            "transitionBindingSha256": plan_bindings(manifest)[
                "transitionBindingSha256"
            ][transition_id],
            "authorizationSha256": auth_sha,
            "authorizationSequence": normalized_auth[
                "authorizationSequence"
            ],
            "runNonce": normalized_auth["runNonce"],
            "targetSha256": sha256_bytes(canonical_json(fixed_target())),
            "preObservationSha256": pre_sha,
            "postObservationSha256": sha256_bytes(canonical_json(post)),
            "previousReceiptSha256": state["receiptChainHeadSha256"],
        }
        state["receipts"].append(receipt)
        state["receiptChainHeadSha256"] = sha256_bytes(
            canonical_json(receipt)
        )
        state["nextOrdinal"] += 1
        validate_checkpoint(state, manifest, normalized_auth, auth_sha)
        if checkpoint_sink is not None:
            checkpoint_sink(copy.deepcopy(state))
    return copy.deepcopy(state)


def read_regular_once(path: Path, label: str) -> bytes:
    try:
        return plan.read_regular_once(path, label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def load_json_path(path_text: str, label: str) -> dict[str, Any]:
    path = Path(path_text)
    if not path.is_absolute():
        raise ExecutorError(f"{label} path must be absolute")
    try:
        return plan.decode_json(read_regular_once(path, label), label)
    except plan.PlanError as exc:
        raise ExecutorError(str(exc)) from exc


def require_fixed_executor_installation() -> None:
    if str(Path(__file__).resolve()) != FIXED_EXECUTOR_PATH:
        raise ExecutorError("executor is not running from its fixed installation")
    info = os.lstat(FIXED_EXECUTOR_PATH)
    if (
        not stat.S_ISREG(info.st_mode)
        or info.st_uid != 0
        or info.st_gid != 0
        or stat.S_IMODE(info.st_mode) != 0o555
        or info.st_nlink != 1
    ):
        raise ExecutorError("executor owner/mode/type is not root:root 0555")


def build_parser() -> StrictParser:
    parser = StrictParser(allow_abbrev=False)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("preflight", "operator-command"):
        command = commands.add_parser(name, allow_abbrev=False)
        command.add_argument("--manifest", required=True)
    execute = commands.add_parser("execute", allow_abbrev=False)
    execute.add_argument("--manifest", required=True)
    execute.add_argument("--authorization", required=True)
    execute.add_argument("--state-root", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        manifest = load_json_path(args.manifest, "plan manifest")
        if args.command == "preflight":
            sys.stdout.write(canonical_json(gate_report(manifest)).decode())
            return 3
        if args.command == "operator-command":
            value = {
                "schemaVersion": SCHEMA_VERSION,
                "kind": "vmqa-f1-provisioning-operator-command",
                "status": "authorization-required",
                "argv": operator_argv(manifest),
                "writesPerformed": 0,
                "executionPermitted": False,
            }
            sys.stdout.write(canonical_json(value).decode())
            return 0
        if args.command == "execute":
            bindings = plan_bindings(manifest)
            expected = expected_operator_paths(bindings["planPayloadSha256"])
            if (
                args.manifest != expected["manifest"]
                or args.authorization != expected["authorization"]
                or args.state_root != expected["stateRoot"]
            ):
                raise ExecutorError(
                    "execute paths are not exactly derived from the plan hash"
                )
            require_fixed_executor_installation()
            authorization = load_json_path(
                args.authorization, "authorization receipt"
            )
            validate_authorization(authorization, manifest)
            raise ExecutorError(
                "no production transition adapter is admitted by this seam"
            )
        raise ExecutorError("unknown operation")
    except (ExecutorError, OSError) as exc:
        print(f"VMQA F1 EXECUTOR REFUSED: {exc}", file=sys.stderr)
        return 9


if __name__ == "__main__":
    raise SystemExit(main())
