from __future__ import annotations

import base64
import copy
import hashlib
import json
from pathlib import Path, PureWindowsPath
import sys
import tempfile
import threading
import unittest


HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from conformance import (
    BOUNDARY_PATH,
    FIXTURE_PATH,
    ConformanceError,
    SEMANTIC_RULES,
    load_boundary,
    materialize_case,
    run_fixture,
    validate_document,
    validate_ledger_record,
    validate_request,
)
from ledger import (
    LEDGER_VERSION,
    MAX_TTL_MS,
    RECORD_KEYS,
    STATES,
    LedgerError,
    OneShotLedger,
    _seal_record,
    pipe_binding_digest,
)
from schema import (
    EXACT_SHIPPING_FEATURES,
    FILE_IDENTITY_KEYS,
    PROCESS_KEYS,
    READBACK_KEYS,
    SchemaError,
    SCHEMA_NAME,
    SCHEMA_VERSION,
    TARGET_KEYS,
    canonical_json,
    seal_receipt,
    sha256_hex,
    target_binding_digest,
    validate_receipt,
)
from verify import (
    FilesystemAuthorityVerifier,
    VerificationContext,
    VerificationError,
    verify_receipt,
)


NOW = 1_800_000_000_000
CHALLENGE = "11" * 32
OTHER_CHALLENGE = "22" * 32


def process(
    pid: int,
    start: int,
    path: str,
    digest_byte: str,
    *,
    session: int = 2,
) -> dict:
    return {
        "pid": pid,
        "processStartTime100ns": start,
        "sessionId": session,
        "executablePath": path,
        "executableSha256": digest_byte * 64,
        "fileIdentity": {
            "volumeSerialNumber": 7,
            "fileIndex": pid * 10,
            "fileSize": 5_000_000 + pid,
            "lastWriteTime100ns": 133_000_000_000_000_000 + pid,
        },
    }


def make_receipt() -> dict:
    emitter = process(
        4242,
        133_100_000_000_000_000,
        r"C:\Program Files\OSL\osl-privacy-hub.exe",
        "a",
    )
    target = {
        **process(
            7331,
            133_100_000_000_000_100,
            r"C:\Users\Owner\AppData\Local\Discord\app-1.2.3\Discord.exe",
            "b",
        ),
        "bindingSha256": "",
        "hwnd": 0x123456,
        "rootHwnd": 0x123456,
        "hostGeneration": 9,
        "publisher": "Discord Inc.",
    }
    target["bindingSha256"] = target_binding_digest(target)
    binding = target["bindingSha256"]
    carrier_bytes = b"ordinary cover text\nsecond line"
    carrier_digest = sha256_hex(carrier_bytes)
    empty_digest = sha256_hex(b"")
    receipt = {
        "schema": "osl.c4.native-placement-receipt",
        "version": 3,
        "evidenceKind": "native-placement",
        "challenge": CHALLENGE,
        "emittedAtUnixMs": NOW + 1_500,
        "monotonicStagesMs": {
            "challengeClaimed": 0,
            "preSendReadback": 300,
            "sendInjected": 500,
            "postContextRevalidated": 900,
            "receiptEmitted": 1_000,
        },
        "emitter": emitter,
        "build": {
            "features": ["core", "desktop"],
            "debugAssertions": False,
            "targetOs": "windows",
            "targetArch": "x86_64",
            "profile": "release",
        },
        "target": target,
        "carrier": {
            "targetBindingSha256": binding,
            "utf8B64": base64.b64encode(carrier_bytes).decode("ascii"),
            "sha256": carrier_digest,
            "byteLength": len(carrier_bytes),
            "utf16Length": len(carrier_bytes.decode("utf-8").encode("utf-16-le")) // 2,
        },
        "preSend": {
            "targetBindingSha256": binding,
            "readback": {
                "targetBindingSha256": binding,
                "classification": "exact",
                "complete": True,
                "sha256": carrier_digest,
                "byteLength": len(carrier_bytes),
                "utf16Length": len(
                    carrier_bytes.decode("utf-8").encode("utf-16-le")
                )
                // 2,
            },
            "foreground": {
                "targetBindingSha256": binding,
                "foregroundHwnd": target["hwnd"],
                "foregroundRootHwnd": target["rootHwnd"],
                "foregroundPid": target["pid"],
                "targetRootHwnd": target["rootHwnd"],
                "keyboardFocusProven": True,
                "targetOwnedByTrustedProcess": True,
            },
        },
        "action": {
            "targetBindingSha256": binding,
            "mechanism": "sendinput_enter",
            "attempted": True,
            "acceptedInputCount": 2,
            "enterCertainty": "injected_once",
            "retryPolicy": "never_auto_retry",
            "actionSequence": 1,
        },
        "postSend": {
            "targetBindingSha256": binding,
            "readback": {
                "targetBindingSha256": binding,
                "classification": "empty",
                "complete": True,
                "sha256": empty_digest,
                "byteLength": 0,
                "utf16Length": 0,
            },
            "hostRevalidated": True,
            "overlayContextUnchanged": True,
            "carrierConsumed": True,
            "sentRowProven": True,
            "rowDelta": 1,
            "status": "sent",
        },
        "receiptDigestSha256": "",
    }
    return seal_receipt(receipt)


def encode(receipt: dict) -> bytes:
    return canonical_json(receipt)


def context(receipt: dict, *, source: str = "synthetic") -> VerificationContext:
    return VerificationContext(
        challenge=CHALLENGE,
        issued_at_unix_ms=NOW,
        now_unix_ms=NOW + 2_000,
        expires_at_unix_ms=NOW + 60_000,
        source=source,
        expected_emitter=copy.deepcopy(receipt["emitter"]),
        pipe_client=copy.deepcopy(receipt["emitter"]),
        expected_target=copy.deepcopy(receipt["target"]),
        pipe_first_instance=True,
        pipe_remote_clients_rejected=True,
        pipe_acl_exact_user=True,
    )


def recovered_ledger(directory: str, now: int = NOW) -> OneShotLedger:
    ledger = OneShotLedger(directory)
    ledger.recover_incomplete(now)
    return ledger


def runtime_context(receipt: dict) -> VerificationContext:
    root = r"C:\ProgramData\OSL\C4\ledger"
    record_name = hashlib.sha256(bytes.fromhex(CHALLENGE)).hexdigest() + ".json"
    base = context(receipt, source="runtime_named_pipe")
    return VerificationContext(
        **{
            **base.__dict__,
            "filesystem_authority_attestation": {
                "schema": "osl.c4.filesystem-authority-attestation",
                "version": 3,
                "authoritySource": "native_windows_owner",
                "challenge": CHALLENGE,
                "checkedAtUnixMs": NOW + 1_600,
                "ledgerRoot": root,
                "ledgerRecordPath": root + "\\" + record_name,
                "rootIdentity": {
                    "volumeSerialNumber": 7,
                    "fileIndex": 99,
                },
                "daclSha256": "d" * 64,
                "ownerSidSha256": "e" * 64,
            },
        }
    )


class FakeRuntimeLedger:
    def __init__(self, receipt: dict):
        self.root = PureWindowsPath(r"C:\ProgramData\OSL\C4\ledger")
        self.recovery_complete = True
        self.record = {
            "state": "connected",
            "issuedAtUnixMs": NOW,
            "expiresAtUnixMs": NOW + 60_000,
            "updatedAtUnixMs": NOW + 1,
            "pipeBindingSha256": pipe_binding_digest(receipt["emitter"]),
        }
        self.consume_calls = 0

    def record_path(self, challenge: str) -> PureWindowsPath:
        digest = hashlib.sha256(bytes.fromhex(challenge)).hexdigest()
        return self.root / f"{digest}.json"

    def read(self, challenge: str) -> dict:
        return copy.deepcopy(self.record)

    def consume(
        self,
        challenge: str,
        pipe_client_binding: dict,
        receipt_frame_sha256: str,
        now_ms: int,
    ) -> None:
        if self.record["state"] != "connected":
            raise LedgerError("challenge is not connected or was already consumed")
        self.consume_calls += 1
        self.record["state"] = "consumed"
        self.record["receiptFrameSha256"] = receipt_frame_sha256


class FakeFilesystemAuthorityVerifier:
    def __init__(self, result: object = True):
        self.result = result
        self.calls = 0

    def verify_filesystem_authority(self, **kwargs: object) -> object:
        self.calls += 1
        return self.result


class FakeNativeFilesystemApi:
    def __init__(
        self,
        *,
        root_identity: dict[str, int] | None = None,
        dacl_sha256: str = "d" * 64,
        owner_sid_sha256: str = "e" * 64,
        current_user_sid_sha256: str = "e" * 64,
        dacl_protected: bool = True,
        record_is_regular_child: bool = True,
    ) -> None:
        self.root_identity = root_identity or {
            "volumeSerialNumber": 7,
            "fileIndex": 99,
        }
        self.dacl_sha256 = dacl_sha256
        self.owner_sid_sha256 = owner_sid_sha256
        self.current_sid_sha256 = current_user_sid_sha256
        self.dacl_protected = dacl_protected
        self.record_child = record_is_regular_child
        self.identity_calls: list[tuple[str, bool]] = []

    def file_identity(self, path: str, *, directory: bool) -> dict[str, int]:
        self.identity_calls.append((path, directory))
        return copy.deepcopy(self.root_identity)

    def record_is_regular_child(self, root: str, record_path: str) -> bool:
        return self.record_child

    def security_hashes(self, path: str) -> tuple[str, str, bool]:
        return self.dacl_sha256, self.owner_sid_sha256, self.dacl_protected

    def current_user_sid_sha256(self) -> str:
        return self.current_sid_sha256


def reseal(receipt: dict) -> dict:
    receipt["receiptDigestSha256"] = ""
    return seal_receipt(receipt)


def set_carrier(receipt: dict, carrier_bytes: bytes) -> None:
    binding = receipt["target"]["bindingSha256"]
    carrier_digest = sha256_hex(carrier_bytes)
    carrier_text = carrier_bytes.decode("utf-8")
    carrier_utf16 = len(carrier_text.encode("utf-16-le")) // 2
    receipt["carrier"] = {
        "targetBindingSha256": binding,
        "utf8B64": base64.b64encode(carrier_bytes).decode("ascii"),
        "sha256": carrier_digest,
        "byteLength": len(carrier_bytes),
        "utf16Length": carrier_utf16,
    }
    receipt["preSend"]["readback"] = {
        "targetBindingSha256": binding,
        "classification": "exact",
        "complete": True,
        "sha256": carrier_digest,
        "byteLength": len(carrier_bytes),
        "utf16Length": carrier_utf16,
    }


def rebind_target(receipt: dict) -> None:
    receipt["target"]["bindingSha256"] = ""
    binding = target_binding_digest(receipt["target"])
    receipt["target"]["bindingSha256"] = binding
    receipt["carrier"]["targetBindingSha256"] = binding
    receipt["preSend"]["targetBindingSha256"] = binding
    receipt["preSend"]["readback"]["targetBindingSha256"] = binding
    receipt["preSend"]["foreground"]["targetBindingSha256"] = binding
    receipt["action"]["targetBindingSha256"] = binding
    receipt["postSend"]["targetBindingSha256"] = binding
    receipt["postSend"]["readback"]["targetBindingSha256"] = binding


class NativeAuthorityV3Tests(unittest.TestCase):
    def test_synthetic_parser_crypto_positive_is_never_runtime_or_full_c4(self) -> None:
        receipt = make_receipt()
        verdict = verify_receipt(encode(receipt), context(receipt))
        self.assertEqual(verdict.status, "synthetic-parser-crypto-valid")
        self.assertTrue(verdict.parser_crypto_valid)
        self.assertFalse(verdict.runtime_receipt_accepted)
        self.assertFalse(verdict.full_c4_success)
        self.assertEqual(verdict.point_delta, 0)

    def test_all_seventeen_negative_mutations_fail_closed(self) -> None:
        names = [
            "unknown_top_level_field",
            "wrong_schema_version",
            "non_shipping_feature_set",
            "challenge_mismatch",
            "expired_challenge",
            "pipe_client_pid_mismatch",
            "emitter_process_start_mismatch",
            "emitter_executable_path_mismatch",
            "emitter_executable_hash_mismatch",
            "target_hwnd_mismatch",
            "target_executable_binding_mismatch",
            "foreground_binding_mismatch",
            "carrier_length_mismatch",
            "pre_send_readback_not_exact",
            "wrong_send_mechanism",
            "retry_safety_weakened",
            "post_context_mutated",
        ]
        self.assertEqual(len(names), 17)
        for name in names:
            with self.subTest(mutation=name):
                receipt = make_receipt()
                ctx = context(receipt)
                if name == "unknown_top_level_field":
                    receipt["callerVerdict"] = "pass"
                elif name == "wrong_schema_version":
                    receipt["version"] = 4
                elif name == "non_shipping_feature_set":
                    receipt["build"]["features"].append("discord-qa-shell")
                elif name == "challenge_mismatch":
                    receipt["challenge"] = OTHER_CHALLENGE
                elif name == "expired_challenge":
                    ctx = VerificationContext(
                        **{
                            **ctx.__dict__,
                            "now_unix_ms": NOW + 60_001,
                        }
                    )
                elif name == "pipe_client_pid_mismatch":
                    pipe = copy.deepcopy(ctx.pipe_client)
                    pipe["pid"] += 1
                    ctx = VerificationContext(**{**ctx.__dict__, "pipe_client": pipe})
                elif name == "emitter_process_start_mismatch":
                    expected = copy.deepcopy(ctx.expected_emitter)
                    expected["processStartTime100ns"] += 1
                    ctx = VerificationContext(
                        **{**ctx.__dict__, "expected_emitter": expected}
                    )
                elif name == "emitter_executable_path_mismatch":
                    expected = copy.deepcopy(ctx.expected_emitter)
                    expected["executablePath"] = r"C:\Other\osl-privacy-hub.exe"
                    ctx = VerificationContext(
                        **{**ctx.__dict__, "expected_emitter": expected}
                    )
                elif name == "emitter_executable_hash_mismatch":
                    expected = copy.deepcopy(ctx.expected_emitter)
                    expected["executableSha256"] = "c" * 64
                    ctx = VerificationContext(
                        **{**ctx.__dict__, "expected_emitter": expected}
                    )
                elif name == "target_hwnd_mismatch":
                    expected = copy.deepcopy(ctx.expected_target)
                    expected["hwnd"] += 1
                    ctx = VerificationContext(
                        **{**ctx.__dict__, "expected_target": expected}
                    )
                elif name == "target_executable_binding_mismatch":
                    receipt["target"]["executableSha256"] = "d" * 64
                    rebind_target(receipt)
                elif name == "foreground_binding_mismatch":
                    receipt["preSend"]["foreground"]["foregroundRootHwnd"] += 1
                elif name == "carrier_length_mismatch":
                    receipt["carrier"]["byteLength"] += 1
                elif name == "pre_send_readback_not_exact":
                    receipt["preSend"]["readback"]["classification"] = "partial"
                elif name == "wrong_send_mechanism":
                    receipt["action"]["mechanism"] = "uia_send_action"
                elif name == "retry_safety_weakened":
                    receipt["action"]["retryPolicy"] = "retry"
                elif name == "post_context_mutated":
                    receipt["postSend"]["overlayContextUnchanged"] = False
                if name != "expired_challenge":
                    receipt = reseal(receipt)
                with self.assertRaises(VerificationError):
                    verify_receipt(encode(receipt), ctx)

    def test_additional_enter_readback_carrier_and_row_mutations_fail_closed(self) -> None:
        mutations = {
            "carrier_hash": lambda value: value["carrier"].__setitem__(
                "sha256", "f" * 64
            ),
            "post_readback": lambda value: value["postSend"]["readback"].__setitem__(
                "classification", "unreadable"
            ),
            "row_delta": lambda value: value["postSend"].__setitem__("rowDelta", 0),
            "enter_certainty": lambda value: value["action"].__setitem__(
                "enterCertainty", "unknown"
            ),
            "partial_enter_injection": lambda value: value["action"].__setitem__(
                "acceptedInputCount", 1
            ),
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                receipt = make_receipt()
                mutate(receipt)
                receipt = reseal(receipt)
                with self.assertRaises(VerificationError):
                    verify_receipt(encode(receipt), context(make_receipt()))

    def test_readback_carrier_and_send_stages_share_one_target_binding(self) -> None:
        mutations = {
            "carrier_binding": lambda value: value["carrier"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "pre_send_binding": lambda value: value["preSend"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "pre_send_readback_binding": lambda value: value["preSend"][
                "readback"
            ].__setitem__("targetBindingSha256", "f" * 64),
            "foreground_binding": lambda value: value["preSend"][
                "foreground"
            ].__setitem__("targetBindingSha256", "f" * 64),
            "action_binding": lambda value: value["action"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "post_send_binding": lambda value: value["postSend"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "post_send_readback_binding": lambda value: value["postSend"][
                "readback"
            ].__setitem__("targetBindingSha256", "f" * 64),
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                receipt = make_receipt()
                mutate(receipt)
                receipt = reseal(receipt)
                with self.assertRaises(VerificationError):
                    verify_receipt(encode(receipt), context(make_receipt()))

    def readback(self) -> None:
        receipt = make_receipt()
        validate_receipt(receipt)

        binding_mutations = {
            "carrier": lambda value: value["carrier"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "preSend": lambda value: value["preSend"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "preSend.readback": lambda value: value["preSend"][
                "readback"
            ].__setitem__("targetBindingSha256", "f" * 64),
            "postSend": lambda value: value["postSend"].__setitem__(
                "targetBindingSha256", "f" * 64
            ),
            "postSend.readback": lambda value: value["postSend"][
                "readback"
            ].__setitem__("targetBindingSha256", "f" * 64),
        }
        for name, mutate in binding_mutations.items():
            with self.subTest(binding=name):
                mutated = make_receipt()
                mutate(mutated)
                mutated = reseal(mutated)
                with self.assertRaisesRegex(
                    SchemaError,
                    "does not match the Discord target binding",
                ):
                    validate_receipt(mutated)

        readback_mutations = {
            "pre-readback digest": lambda value: value["preSend"][
                "readback"
            ].__setitem__("sha256", "f" * 64),
            "pre-readback byte length": lambda value: value["preSend"][
                "readback"
            ].__setitem__("byteLength", value["carrier"]["byteLength"] + 1),
            "post-readback digest": lambda value: value["postSend"][
                "readback"
            ].__setitem__("sha256", value["carrier"]["sha256"]),
            "post-readback byte length": lambda value: value["postSend"][
                "readback"
            ].__setitem__("byteLength", value["carrier"]["byteLength"]),
        }
        for name, mutate in readback_mutations.items():
            with self.subTest(readback=name):
                mutated = make_receipt()
                mutate(mutated)
                mutated = reseal(mutated)
                with self.assertRaisesRegex(SchemaError, "does not match"):
                    validate_receipt(mutated)

    def test_verification_context_requires_all_independent_fact_sets(self) -> None:
        receipt = make_receipt()
        base = context(receipt)
        cases = {
            "expected-emitter": {
                **base.__dict__,
                "expected_emitter": None,
            },
            "pipe-client": {
                **base.__dict__,
                "pipe_client": None,
            },
            "expected-target": {
                **base.__dict__,
                "expected_target": None,
            },
        }
        for name, fields in cases.items():
            with self.subTest(missing=name):
                with self.assertRaises(VerificationError):
                    verify_receipt(encode(receipt), VerificationContext(**fields))

        with self.assertRaisesRegex(
            VerificationError,
            "verification context is invalid",
        ):
            verify_receipt(encode(receipt), base.__dict__)  # type: ignore[arg-type]

    def test_carrier_del_control_character_fails_closed(self) -> None:
        receipt = make_receipt()
        set_carrier(receipt, b"ordinary cover text\x7f")
        receipt = reseal(receipt)
        with self.assertRaises(VerificationError):
            verify_receipt(encode(receipt), context(make_receipt()))

    def test_receipt_digest_mutation_is_rejected(self) -> None:
        receipt = make_receipt()
        receipt["emittedAtUnixMs"] += 1
        with self.assertRaises(VerificationError):
            verify_receipt(encode(receipt), context(make_receipt()))

    def test_duplicate_json_key_is_rejected(self) -> None:
        receipt = make_receipt()
        raw = encode(receipt)
        duplicate = raw[:-1] + b',"version":3}'
        with self.assertRaises(VerificationError):
            verify_receipt(duplicate, context(receipt))

    def test_noncanonical_json_frame_is_rejected(self) -> None:
        receipt = make_receipt()
        with self.assertRaises(VerificationError):
            verify_receipt(encode(receipt) + b"\n", context(receipt))

    def test_synthetic_evidence_may_not_consume_a_ledger(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            with self.assertRaises(VerificationError):
                verify_receipt(encode(receipt), context(receipt), ledger=ledger)
        with self.assertRaises(VerificationError):
            verify_receipt(
                encode(receipt),
                context(receipt),
                filesystem_authority_verifier=FakeFilesystemAuthorityVerifier(),
            )

    def test_runtime_acceptance_binds_exact_paths_and_refuses_replay_first(self) -> None:
        receipt = make_receipt()
        ledger = FakeRuntimeLedger(receipt)
        authority = FakeFilesystemAuthorityVerifier()
        ctx = runtime_context(receipt)

        verdict = verify_receipt(
            encode(receipt),
            ctx,
            ledger=ledger,
            filesystem_authority_verifier=authority,
        )
        self.assertTrue(verdict.runtime_receipt_accepted)
        self.assertFalse(verdict.full_c4_success)
        self.assertEqual(verdict.point_delta, 0)
        self.assertEqual(verdict.receipt_frame_sha256, sha256_hex(encode(receipt)))
        self.assertEqual(authority.calls, 1)
        self.assertEqual(ledger.consume_calls, 1)

        with self.assertRaises(VerificationError):
            verify_receipt(
                encode(receipt),
                ctx,
                ledger=ledger,
                filesystem_authority_verifier=authority,
            )
        self.assertEqual(authority.calls, 1)
        self.assertEqual(ledger.consume_calls, 1)

    def test_verify(self) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        authority = FakeFilesystemAuthorityVerifier()

        unconnected = FakeRuntimeLedger(receipt)
        unconnected.record["state"] = "issued"
        with self.assertRaises(VerificationError):
            verify_receipt(
                encode(receipt),
                ctx,
                ledger=unconnected,
                filesystem_authority_verifier=authority,
            )
        self.assertEqual(unconnected.consume_calls, 0)
        self.assertEqual(authority.calls, 0)

        wrong_pipe = FakeRuntimeLedger(receipt)
        wrong_pipe.record["pipeBindingSha256"] = "f" * 64
        with self.assertRaises(VerificationError):
            verify_receipt(
                encode(receipt),
                ctx,
                ledger=wrong_pipe,
                filesystem_authority_verifier=authority,
            )
        self.assertEqual(wrong_pipe.consume_calls, 0)
        self.assertEqual(authority.calls, 0)

        ledger = FakeRuntimeLedger(receipt)
        verdict = verify_receipt(
            encode(receipt),
            ctx,
            ledger=ledger,
            filesystem_authority_verifier=authority,
        )
        self.assertTrue(verdict.runtime_receipt_accepted)
        self.assertEqual(ledger.consume_calls, 1)
        self.assertEqual(ledger.record["state"], "consumed")

        with self.assertRaises(VerificationError):
            verify_receipt(
                encode(receipt),
                ctx,
                ledger=ledger,
                filesystem_authority_verifier=authority,
            )
        self.assertEqual(ledger.consume_calls, 1)

    def test_runtime_authority_and_request_path_refusals_do_not_consume(self) -> None:
        receipt = make_receipt()
        base = runtime_context(receipt)
        mutations = {
            "wrong-root": {
                "ledgerRoot": r"C:\ProgramData\OSL\C4\other",
            },
            "wrong-record": {
                "ledgerRecordPath": (
                    "C:\\ProgramData\\OSL\\C4\\ledger\\"
                    + "f" * 64
                    + ".json"
                ),
            },
            "authority-before-receipt": {
                "checkedAtUnixMs": NOW + 1_499,
            },
        }
        for name, changes in mutations.items():
            with self.subTest(refusal=name):
                ledger = FakeRuntimeLedger(receipt)
                authority = FakeFilesystemAuthorityVerifier()
                attestation = copy.deepcopy(
                    base.filesystem_authority_attestation
                )
                attestation.update(changes)
                ctx = VerificationContext(
                    **{
                        **base.__dict__,
                        "filesystem_authority_attestation": attestation,
                    }
                )
                with self.assertRaises(VerificationError):
                    verify_receipt(
                        encode(receipt),
                        ctx,
                        ledger=ledger,
                        filesystem_authority_verifier=authority,
                    )
                self.assertEqual(ledger.consume_calls, 0)

        for result in (False, 1, None):
            with self.subTest(native_result=result):
                ledger = FakeRuntimeLedger(receipt)
                with self.assertRaises(VerificationError):
                    verify_receipt(
                        encode(receipt),
                        base,
                        ledger=ledger,
                        filesystem_authority_verifier=(
                            FakeFilesystemAuthorityVerifier(result)
                        ),
                    )
                self.assertEqual(ledger.consume_calls, 0)

    def test_runtime_request_window_and_pipe_binding_refuse_before_native_authority(
        self,
    ) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        for mutation in ("window", "pipe"):
            with self.subTest(mutation=mutation):
                ledger = FakeRuntimeLedger(receipt)
                if mutation == "window":
                    ledger.record["expiresAtUnixMs"] += 1
                else:
                    ledger.record["pipeBindingSha256"] = "f" * 64
                authority = FakeFilesystemAuthorityVerifier()
                with self.assertRaises(VerificationError):
                    verify_receipt(
                        encode(receipt),
                        ctx,
                        ledger=ledger,
                        filesystem_authority_verifier=authority,
                    )
                self.assertEqual(authority.calls, 0)
                self.assertEqual(ledger.consume_calls, 0)

    def test_filesystem_authority_verifier_requires_native_proof(self) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        attestation = copy.deepcopy(ctx.filesystem_authority_attestation)
        verifier = FilesystemAuthorityVerifier(
            native_api=FakeNativeFilesystemApi()
        )

        self.assertTrue(
            verifier.verify_filesystem_authority(
                ledger_root=attestation["ledgerRoot"],
                ledger_record_path=attestation["ledgerRecordPath"],
                attestation=attestation,
                challenge=CHALLENGE,
                expected_emitter=receipt["emitter"],
                now_unix_ms=NOW + 2_000,
            )
        )

    def test_filesystem_authority_verifier_fails_closed_without_windows_api(
        self,
    ) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        attestation = copy.deepcopy(ctx.filesystem_authority_attestation)
        verifier = FilesystemAuthorityVerifier()

        self.assertFalse(
            verifier.verify_filesystem_authority(
                ledger_root=attestation["ledgerRoot"],
                ledger_record_path=attestation["ledgerRecordPath"],
                attestation=attestation,
                challenge=CHALLENGE,
                expected_emitter=receipt["emitter"],
                now_unix_ms=NOW + 2_000,
            )
        )

    def test_filesystem_authority_verifier_rejects_native_mismatches(
        self,
    ) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        attestation = copy.deepcopy(ctx.filesystem_authority_attestation)
        cases = {
            "root-identity": FakeNativeFilesystemApi(
                root_identity={"volumeSerialNumber": 7, "fileIndex": 100}
            ),
            "record-child": FakeNativeFilesystemApi(
                record_is_regular_child=False
            ),
            "dacl": FakeNativeFilesystemApi(dacl_sha256="f" * 64),
            "owner": FakeNativeFilesystemApi(owner_sid_sha256="f" * 64),
            "current-user": FakeNativeFilesystemApi(
                current_user_sid_sha256="f" * 64
            ),
            "inherited-dacl": FakeNativeFilesystemApi(dacl_protected=False),
        }

        for name, native_api in cases.items():
            with self.subTest(mismatch=name):
                verifier = FilesystemAuthorityVerifier(native_api=native_api)
                self.assertFalse(
                    verifier.verify_filesystem_authority(
                        ledger_root=attestation["ledgerRoot"],
                        ledger_record_path=attestation["ledgerRecordPath"],
                        attestation=attestation,
                        challenge=CHALLENGE,
                        expected_emitter=receipt["emitter"],
                        now_unix_ms=NOW + 2_000,
                    )
                )

    def test_filesystem_authority_verifier_rejects_caller_path_claims(
        self,
    ) -> None:
        receipt = make_receipt()
        ctx = runtime_context(receipt)
        attestation = copy.deepcopy(ctx.filesystem_authority_attestation)
        verifier = FilesystemAuthorityVerifier(
            native_api=FakeNativeFilesystemApi()
        )

        for name, changes in {
            "other-root": {"ledgerRoot": r"C:\ProgramData\OSL\C4\other"},
            "other-record": {
                "ledgerRecordPath": (
                    "C:\\ProgramData\\OSL\\C4\\ledger\\" + "f" * 64 + ".json"
                )
            },
            "future-check": {"checkedAtUnixMs": NOW + 2_001},
        }.items():
            with self.subTest(claim=name):
                mutated = copy.deepcopy(attestation)
                mutated.update(changes)
                self.assertFalse(
                    verifier.verify_filesystem_authority(
                        ledger_root=attestation["ledgerRoot"],
                        ledger_record_path=attestation["ledgerRecordPath"],
                        attestation=mutated,
                        challenge=CHALLENGE,
                        expected_emitter=receipt["emitter"],
                        now_unix_ms=NOW + 2_000,
                    )
                )


class CrossLanguageConformanceTests(unittest.TestCase):
    def test_machine_readable_fixture_accepts_only_parser_crypto_positive(self) -> None:
        result = run_fixture()
        self.assertEqual(result["invalidMutationsRejected"], 42)
        self.assertTrue(result["syntheticParserCryptoValid"])
        self.assertFalse(result["runtimeReceiptAccepted"])
        self.assertFalse(result["fullC4Success"])
        self.assertEqual(result["pointDelta"], 0)

    def test_all_fixture_mutations_are_named_and_fail_the_boundary(self) -> None:
        boundary = load_boundary()
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        names = [case["name"] for case in fixture["invalidMutations"]]
        self.assertEqual(len(names), len(set(names)))
        self.assertTrue(
            {
                "unknown_top_level_receipt_field",
                "unknown_request_field",
                "unknown_nested_receipt_field",
                "number_as_string_coercion",
                "boolean_as_integer_coercion",
                "duplicate_receipt_json_key",
                "noncanonical_receipt_json",
                "integral_float_rejected",
                "missing_required_request_field",
                "missing_required_receipt_field",
                "monotonic_stage_order_reversed",
                "monotonic_duration_exceeds_wall_elapsed",
                "synthetic_filesystem_authority_forbidden",
                "reserved_windows_path_component",
                "non_ascii_windows_path",
            }.issubset(names)
        )
        valid = materialize_case(fixture, None, boundary=boundary)
        validate_request(valid, boundary=boundary)
        for case in fixture["invalidMutations"]:
            with self.subTest(mutation=case["name"]):
                invalid = materialize_case(fixture, case, boundary=boundary)
                with self.assertRaises(ConformanceError):
                    validate_request(invalid, boundary=boundary)

    def test_every_object_boundary_rejects_an_unknown_key(self) -> None:
        boundary = load_boundary()
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        base = materialize_case(fixture, None, boundary=boundary)
        validate_request(base, boundary=boundary)

        def object_paths(
            value: object,
            prefix: tuple[str, ...] = (),
        ) -> list[tuple[str, ...]]:
            paths: list[tuple[str, ...]] = []
            if type(value) is dict:
                paths.append(prefix)
                for key, child in value.items():
                    paths.extend(object_paths(child, prefix + (key,)))
            elif type(value) is list:
                for index, child in enumerate(value):
                    paths.extend(object_paths(child, prefix + (str(index),)))
            return paths

        # receiptUtf8B64 contains a separately validated object, exercised by
        # the fixture's receipt mutations. This sweep covers every request-side
        # object and prevents a future schema from silently opening one.
        for path in object_paths(base):
            with self.subTest(path="/" + "/".join(path)):
                mutated = copy.deepcopy(base)
                target: object = mutated
                for part in path:
                    target = target[int(part)] if type(target) is list else target[part]
                self.assertIs(type(target), dict)
                target["__unknownConformanceField"] = True
                with self.assertRaises(ConformanceError):
                    validate_request(mutated, boundary=boundary)

    def test_schema_code_and_fixture_contract_cannot_drift_silently(self) -> None:
        boundary = load_boundary()
        definitions = boundary["$defs"]
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        receipt = fixture["validReceipt"]

        self.assertEqual(SCHEMA_NAME, receipt["schema"])
        self.assertEqual(SCHEMA_VERSION, receipt["version"])
        self.assertEqual(
            EXACT_SHIPPING_FEATURES,
            [
                item["const"]
                for item in definitions["nativeReceipt"]["properties"]["build"][
                    "properties"
                ]["features"]["prefixItems"]
            ],
        )
        self.assertEqual(
            FILE_IDENTITY_KEYS,
            set(definitions["fileIdentity"]["required"]),
        )
        self.assertEqual(PROCESS_KEYS, set(definitions["processBinding"]["required"]))
        self.assertEqual(TARGET_KEYS, set(definitions["targetBinding"]["required"]))
        self.assertEqual(READBACK_KEYS, set(definitions["readback"]["required"]))
        self.assertEqual(
            LEDGER_VERSION,
            definitions["ledgerRecord"]["properties"]["version"]["const"],
        )
        self.assertEqual(
            STATES,
            set(definitions["ledgerRecord"]["properties"]["state"]["enum"]),
        )
        self.assertEqual(RECORD_KEYS, set(definitions["ledgerRecord"]["required"]))
        self.assertEqual(MAX_TTL_MS, 60_000)
        self.assertEqual(
            tuple(boundary["x-c4-semanticRules"]),
            SEMANTIC_RULES,
        )
        self.assertEqual(
            hashlib.sha256(BOUNDARY_PATH.read_bytes()).hexdigest(),
            "6fdacb12edba0d84bce02c9a62635820877efb93256ce594d69e27ede2c8318a",
        )
        self.assertEqual(
            hashlib.sha256(FIXTURE_PATH.read_bytes()).hexdigest(),
            "3fa64d9bc3402040594109c991cb3ae4331349e63470ddce59a027bf6b1ba3e8",
        )

    def test_result_authority_booleans_and_unknown_fields_are_closed(self) -> None:
        boundary = load_boundary()
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        result = fixture["expectedSyntheticResult"]
        validate_document("verificationResult", result, boundary=boundary)
        mutations = {
            "runtime": ("runtimeReceiptAccepted", True),
            "full-c4": ("fullC4Success", True),
            "points": ("pointDelta", 1),
            "status": ("status", "runtime-native-receipt-valid"),
            "source": ("source", "runtime_named_pipe"),
        }
        for name, (field, value) in mutations.items():
            with self.subTest(mutation=name):
                invalid = copy.deepcopy(result)
                invalid[field] = value
                with self.assertRaises(ConformanceError):
                    validate_document(
                        "verificationResult",
                        invalid,
                        boundary=boundary,
                    )
        invalid = copy.deepcopy(result)
        invalid["callerAward"] = 4
        with self.assertRaises(ConformanceError):
            validate_document("verificationResult", invalid, boundary=boundary)

    def test_runtime_attestation_schema_and_request_path_mutations_fail_closed(
        self,
    ) -> None:
        boundary = load_boundary()
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        request = materialize_case(fixture, None, boundary=boundary)
        root = r"C:\ProgramData\OSL\C4\ledger"
        record_name = hashlib.sha256(bytes.fromhex(CHALLENGE)).hexdigest() + ".json"
        request["source"] = "runtime_named_pipe"
        request["filesystemAuthorityAttestation"] = {
            "schema": "osl.c4.filesystem-authority-attestation",
            "version": 3,
            "authoritySource": "native_windows_owner",
            "challenge": CHALLENGE,
            "checkedAtUnixMs": NOW + 1_600,
            "ledgerRoot": root,
            "ledgerRecordPath": root + "\\" + record_name,
            "rootIdentity": {
                "volumeSerialNumber": 7,
                "fileIndex": 99,
            },
            "daclSha256": "d" * 64,
            "ownerSidSha256": "e" * 64,
        }
        validate_request(request, boundary=boundary)

        def mutate_unknown(value: dict) -> None:
            value["filesystemAuthorityAttestation"]["callerAuthority"] = True

        def mutate_challenge(value: dict) -> None:
            value["filesystemAuthorityAttestation"]["challenge"] = OTHER_CHALLENGE

        def mutate_time(value: dict) -> None:
            value["filesystemAuthorityAttestation"]["checkedAtUnixMs"] = NOW + 1_499

        def mutate_path(value: dict) -> None:
            value["filesystemAuthorityAttestation"]["ledgerRecordPath"] = (
                root + "\\" + "f" * 64 + ".json"
            )

        def mutate_pipe(value: dict) -> None:
            value["pipeProtections"]["aclExactUser"] = False

        def mutate_reserved_root(value: dict) -> None:
            reserved_root = r"C:\CON\ledger"
            value["filesystemAuthorityAttestation"]["ledgerRoot"] = reserved_root
            value["filesystemAuthorityAttestation"]["ledgerRecordPath"] = (
                reserved_root + "\\" + record_name
            )

        def mutate_root_identity(value: dict) -> None:
            value["filesystemAuthorityAttestation"]["rootIdentity"][
                "callerIdentity"
            ] = 1

        for name, mutation in {
            "unknown-field": mutate_unknown,
            "wrong-challenge": mutate_challenge,
            "pre-receipt-time": mutate_time,
            "wrong-request-path": mutate_path,
            "missing-pipe-protection": mutate_pipe,
            "reserved-root": mutate_reserved_root,
            "unknown-root-identity-field": mutate_root_identity,
        }.items():
            with self.subTest(mutation=name):
                invalid = copy.deepcopy(request)
                mutation(invalid)
                with self.assertRaises(ConformanceError):
                    validate_request(invalid, boundary=boundary)

    def test_surrogate_and_fractional_schema_values_refuse_before_validation(
        self,
    ) -> None:
        boundary = load_boundary()
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        request = materialize_case(fixture, None, boundary=boundary)
        for name, field, value in (
            ("surrogate", "executablePath", "C:\\Bad\ud800\\hub.exe"),
            ("fraction", "pid", 4242.5),
        ):
            with self.subTest(mutation=name):
                invalid = copy.deepcopy(request)
                invalid["expectedEmitter"][field] = value
                with self.assertRaises(ConformanceError):
                    validate_request(invalid, boundary=boundary)


class OneShotLedgerTests(unittest.TestCase):
    def test_ledger(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)

            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, pipe, "c" * 64, NOW + 1)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "issued")

            connected_digest = ledger.connect(CHALLENGE, pipe, NOW + 2)
            connected = ledger.read(CHALLENGE)
            self.assertEqual(connected["state"], "connected")
            self.assertEqual(connected["pipeBindingSha256"], connected_digest)
            self.assertIsNone(connected["receiptFrameSha256"])

            receipt_digest = "d" * 64
            ledger.consume(CHALLENGE, pipe, receipt_digest, NOW + 3)
            consumed = ledger.read(CHALLENGE)
            self.assertEqual(consumed["state"], "consumed")
            self.assertEqual(consumed["receiptFrameSha256"], receipt_digest)

            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 4)

            restarted = OneShotLedger(directory)
            self.assertEqual(restarted.recover_incomplete(NOW + 5), 0)
            persisted = restarted.read(CHALLENGE)
            self.assertEqual(persisted["state"], "consumed")
            self.assertEqual(persisted["receiptFrameSha256"], receipt_digest)

    def test_recovery_is_mandatory_before_any_work(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            with self.assertRaises(LedgerError):
                ledger.issue(CHALLENGE, NOW)
            with self.assertRaises(LedgerError):
                ledger.read(CHALLENGE)
            self.assertEqual(ledger.recover_incomplete(NOW), 0)
            ledger.issue(CHALLENGE, NOW)
            ledger.connect(CHALLENGE, receipt["emitter"], NOW + 1)

    def test_issue_connect_consume_is_atomic_and_replay_is_rejected(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            ledger.connect(CHALLENGE, pipe, NOW + 1)
            ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 2)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "consumed")
            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 3)
            with self.assertRaises(LedgerError):
                ledger.issue(CHALLENGE, NOW + 4)
            restarted = OneShotLedger(directory)
            self.assertEqual(restarted.recover_incomplete(NOW + 4), 0)
            with self.assertRaises(LedgerError):
                restarted.consume(CHALLENGE, pipe, "e" * 64, NOW + 5)

    def test_consume_requires_connected_pipe_and_preserves_available_state(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 1)
            issued = ledger.read(CHALLENGE)
            self.assertEqual(issued["state"], "issued")
            self.assertIsNone(issued["pipeBindingSha256"])
            self.assertIsNone(issued["receiptFrameSha256"])

    def test_wrong_connected_pipe_cannot_consume_or_spoil_the_claim(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        wrong_pipe = process(
            5252,
            133_100_000_000_000_050,
            r"C:\Program Files\OSL\osl-privacy-hub.exe",
            "c",
        )
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            binding = ledger.connect(CHALLENGE, pipe, NOW + 1)
            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, wrong_pipe, "d" * 64, NOW + 2)
            connected = ledger.read(CHALLENGE)
            self.assertEqual(connected["state"], "connected")
            self.assertEqual(connected["pipeBindingSha256"], binding)
            self.assertIsNone(connected["receiptFrameSha256"])
            ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 3)
            consumed = ledger.read(CHALLENGE)
            self.assertEqual(consumed["state"], "consumed")
            self.assertEqual(consumed["receiptFrameSha256"], "e" * 64)

    def test_only_one_concurrent_consume_can_claim_connected_challenge(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        outcomes: list[tuple[str, str]] = []
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            ledger.connect(CHALLENGE, pipe, NOW + 1)
            barrier = threading.Barrier(16)

            def consume(index: int) -> None:
                digest = f"{index + 1:064x}"
                try:
                    barrier.wait()
                    ledger.consume(CHALLENGE, pipe, digest, NOW + 2)
                    outcomes.append(("consumed", digest))
                except (threading.BrokenBarrierError, LedgerError) as error:
                    outcomes.append(("rejected", str(error)))

            threads = [
                threading.Thread(target=consume, args=(index,))
                for index in range(16)
            ]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            consumed = [digest for status, digest in outcomes if status == "consumed"]
            self.assertEqual(len(consumed), 1)
            self.assertEqual(len(outcomes), 16)
            self.assertEqual(
                sum(1 for status, _ in outcomes if status == "rejected"),
                15,
            )
            record = ledger.read(CHALLENGE)
            self.assertEqual(record["state"], "consumed")
            self.assertEqual(record["receiptFrameSha256"], consumed[0])

    def test_only_one_concurrent_connection_can_claim_a_challenge(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        outcomes: list[str] = []
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)

            def connect() -> None:
                try:
                    ledger.connect(CHALLENGE, pipe, NOW + 1)
                    outcomes.append("connected")
                except LedgerError:
                    outcomes.append("rejected")

            threads = [threading.Thread(target=connect) for _ in range(16)]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            self.assertEqual(outcomes.count("connected"), 1)
            self.assertEqual(outcomes.count("rejected"), 15)

    def test_expired_challenge_is_terminal(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, receipt["emitter"], NOW + 11)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "expired")

    def test_crash_recovery_abandons_nonterminal_challenges(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            ledger.connect(CHALLENGE, receipt["emitter"], NOW + 1)
            restarted = OneShotLedger(directory)
            self.assertEqual(restarted.recover_incomplete(NOW + 2), 1)
            self.assertEqual(restarted.read(CHALLENGE)["state"], "abandoned")
            with self.assertRaises(LedgerError):
                restarted.consume(CHALLENGE, receipt["emitter"], "e" * 64, NOW + 3)

    def test_unknown_field_state_and_integrity_fail_closed(self) -> None:
        for mutation in ("unknown-field", "unknown-state", "bad-integrity"):
            with self.subTest(mutation=mutation):
                with tempfile.TemporaryDirectory() as directory:
                    ledger = recovered_ledger(directory)
                    ledger.issue(CHALLENGE, NOW)
                    path = next(Path(directory).glob("*.json"))
                    record = json.loads(path.read_text(encoding="utf-8"))
                    if mutation == "unknown-field":
                        record["callerStatus"] = "consumed"
                    elif mutation == "unknown-state":
                        record["state"] = "maybe"
                    else:
                        record["recordDigestSha256"] = "0" * 64
                    path.write_text(json.dumps(record) + "\n", encoding="utf-8")
                    with self.assertRaises(LedgerError):
                        ledger.read(CHALLENGE)

    def test_live_ledger_records_conform_to_each_declared_state_shape(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            validate_ledger_record(ledger.read(CHALLENGE))
            ledger.connect(CHALLENGE, pipe, NOW + 1)
            validate_ledger_record(ledger.read(CHALLENGE))
            ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 2)
            validate_ledger_record(ledger.read(CHALLENGE))

        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, pipe, NOW + 11)
            validate_ledger_record(ledger.read(CHALLENGE))

        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            restarted = OneShotLedger(directory)
            self.assertEqual(restarted.recover_incomplete(NOW + 1), 1)
            validate_ledger_record(restarted.read(CHALLENGE))

    def test_ledger_state_fields_and_binding_types_are_strict(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            bad_binding = copy.deepcopy(receipt["emitter"])
            bad_binding["pid"] = str(bad_binding["pid"])
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, bad_binding, NOW + 1)

        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            record = ledger.read(CHALLENGE)
            record["state"] = "connected"
            # Recompute integrity so the rejection proves state-field
            # coherence, not merely a stale digest.
            record = _seal_record(record)
            path = next(Path(directory).glob("*.json"))
            path.write_bytes(canonical_json(record) + b"\n")
            with self.assertRaises(LedgerError):
                ledger.read(CHALLENGE)
            with self.assertRaises(ConformanceError):
                validate_ledger_record(record)

    def test_ledger_wide_watermark_survives_instances_and_empty_recovery(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first = OneShotLedger(directory)
            second = OneShotLedger(directory)
            self.assertEqual(first.recover_incomplete(NOW + 200), 0)
            with self.assertRaises(LedgerError):
                second.recover_incomplete(NOW + 150)
            self.assertEqual(second.recover_incomplete(NOW + 200), 0)
            watermark = Path(directory) / ".ledger.watermark"
            self.assertEqual(
                int.from_bytes(watermark.read_bytes(), "big"),
                NOW + 200,
            )

    def test_malformed_or_linked_watermark_refuses_recovery(self) -> None:
        for mutation in ("truncated", "out-of-range", "hardlink"):
            with self.subTest(mutation=mutation):
                with tempfile.TemporaryDirectory() as directory:
                    ledger = recovered_ledger(directory)
                    watermark = Path(directory) / ".ledger.watermark"
                    if mutation == "truncated":
                        watermark.write_bytes(b"\x00" * 7)
                    elif mutation == "out-of-range":
                        watermark.write_bytes((1 << 63).to_bytes(8, "big"))
                    else:
                        alias = Path(directory) / "watermark-alias"
                        alias.hardlink_to(watermark)
                    restarted = OneShotLedger(directory)
                    with self.assertRaises(LedgerError):
                        restarted.recover_incomplete(NOW + 1)

    def test_missing_watermark_after_recovery_refuses_work(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            (Path(directory) / ".ledger.watermark").unlink()
            with self.assertRaises(LedgerError):
                ledger.read(CHALLENGE)
            with self.assertRaises(LedgerError):
                ledger.issue(OTHER_CHALLENGE, NOW + 1)

    def test_backdated_transitions_and_exact_expiry_refuse(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, receipt["emitter"], NOW - 1)
            ledger.connect(CHALLENGE, receipt["emitter"], NOW + 1)
            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, receipt["emitter"], "e" * 64, NOW)

        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, receipt["emitter"], NOW + 10)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "expired")

    def test_nonexpired_records_at_exact_expiry_refuse_even_if_resealed(self) -> None:
        receipt = make_receipt()
        for state in ("connected", "consumed"):
            with self.subTest(state=state):
                with tempfile.TemporaryDirectory() as directory:
                    ledger = recovered_ledger(directory)
                    ledger.issue(CHALLENGE, NOW, ttl_ms=10)
                    ledger.connect(CHALLENGE, receipt["emitter"], NOW + 1)
                    if state == "consumed":
                        ledger.consume(
                            CHALLENGE,
                            receipt["emitter"],
                            "e" * 64,
                            NOW + 2,
                        )
                    path = next(Path(directory).glob("*.json"))
                    record = ledger.read(CHALLENGE)
                    record["updatedAtUnixMs"] = record["expiresAtUnixMs"]
                    record = _seal_record(record)
                    path.write_bytes(canonical_json(record) + b"\n")
                    with self.assertRaises(LedgerError):
                        ledger.read(CHALLENGE)
                    with self.assertRaises(ConformanceError):
                        validate_ledger_record(record)

    def test_record_filename_is_bound_to_declared_challenge_hash(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            path = next(Path(directory).glob("*.json"))
            path.rename(Path(directory) / ("f" * 64 + ".json"))
            restarted = OneShotLedger(directory)
            with self.assertRaises(LedgerError):
                restarted.recover_incomplete(NOW + 1)

    def test_root_links_and_record_hardlinks_refuse(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            real_root = Path(directory) / "real"
            real_root.mkdir(mode=0o700)
            linked_root = Path(directory) / "linked"
            linked_root.symlink_to(real_root, target_is_directory=True)
            with self.assertRaises(LedgerError):
                OneShotLedger(linked_root)

        if not hasattr(Path, "hardlink_to"):
            self.skipTest("hard links are unavailable")
        with tempfile.TemporaryDirectory() as directory:
            ledger = recovered_ledger(directory)
            ledger.issue(CHALLENGE, NOW)
            path = next(Path(directory).glob("*.json"))
            alias = Path(directory) / ("f" * 64 + ".json")
            alias.hardlink_to(path)
            with self.assertRaises(LedgerError):
                ledger.read(CHALLENGE)


def ledger() -> None:
    receipt = make_receipt()
    pipe = receipt["emitter"]
    wrong_pipe = process(
        5252,
        133_100_000_000_000_050,
        r"C:\Program Files\OSL\osl-privacy-hub.exe",
        "c",
    )
    receipt_digest = "d" * 64
    testcase = unittest.TestCase()

    with tempfile.TemporaryDirectory() as directory:
        ledger_store = recovered_ledger(directory)
        ledger_store.issue(CHALLENGE, NOW)

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "c" * 64, NOW + 1)
        issued = ledger_store.read(CHALLENGE)
        testcase.assertEqual(issued["state"], "issued")
        testcase.assertIsNone(issued["pipeBindingSha256"])
        testcase.assertIsNone(issued["receiptFrameSha256"])

        binding = ledger_store.connect(CHALLENGE, pipe, NOW + 2)
        connected = ledger_store.read(CHALLENGE)
        testcase.assertEqual(connected["state"], "connected")
        testcase.assertEqual(connected["pipeBindingSha256"], binding)
        testcase.assertIsNone(connected["receiptFrameSha256"])

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, wrong_pipe, "c" * 64, NOW + 3)
        connected = ledger_store.read(CHALLENGE)
        testcase.assertEqual(connected["state"], "connected")
        testcase.assertEqual(connected["pipeBindingSha256"], binding)
        testcase.assertIsNone(connected["receiptFrameSha256"])

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "0" * 64, NOW + 4)
        connected = ledger_store.read(CHALLENGE)
        testcase.assertEqual(connected["state"], "connected")
        testcase.assertEqual(connected["pipeBindingSha256"], binding)
        testcase.assertIsNone(connected["receiptFrameSha256"])

        ledger_store.consume(CHALLENGE, pipe, receipt_digest, NOW + 5)
        consumed = ledger_store.read(CHALLENGE)
        testcase.assertEqual(consumed["state"], "consumed")
        testcase.assertEqual(consumed["pipeBindingSha256"], binding)
        testcase.assertEqual(consumed["receiptFrameSha256"], receipt_digest)

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "e" * 64, NOW + 6)

        restarted = OneShotLedger(directory)
        testcase.assertEqual(restarted.recover_incomplete(NOW + 7), 0)
        persisted = restarted.read(CHALLENGE)
        testcase.assertEqual(persisted["state"], "consumed")
        testcase.assertEqual(persisted["pipeBindingSha256"], binding)
        testcase.assertEqual(persisted["receiptFrameSha256"], receipt_digest)


def define_c4_one_shot_challenge_ledger_contract() -> None:
    receipt = make_receipt()
    pipe = receipt["emitter"]
    wrong_pipe = process(
        5252,
        133_100_000_000_000_050,
        r"C:\Program Files\OSL\osl-privacy-hub.exe",
        "c",
    )
    testcase = unittest.TestCase()

    with tempfile.TemporaryDirectory() as directory:
        ledger_store = OneShotLedger(directory)
        with testcase.assertRaises(LedgerError):
            ledger_store.issue(CHALLENGE, NOW)

        testcase.assertEqual(ledger_store.recover_incomplete(NOW), 0)
        ledger_store.issue(CHALLENGE, NOW + 1)

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "c" * 64, NOW + 2)
        issued = ledger_store.read(CHALLENGE)
        testcase.assertEqual(issued["state"], "issued")
        testcase.assertIsNone(issued["pipeBindingSha256"])
        testcase.assertIsNone(issued["receiptFrameSha256"])

        binding = ledger_store.connect(CHALLENGE, pipe, NOW + 3)
        with testcase.assertRaises(LedgerError):
            ledger_store.connect(CHALLENGE, pipe, NOW + 4)

        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, wrong_pipe, "d" * 64, NOW + 5)
        connected = ledger_store.read(CHALLENGE)
        testcase.assertEqual(connected["state"], "connected")
        testcase.assertEqual(connected["pipeBindingSha256"], binding)
        testcase.assertIsNone(connected["receiptFrameSha256"])

        receipt_digest = "e" * 64
        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "0" * 64, NOW + 6)
        connected = ledger_store.read(CHALLENGE)
        testcase.assertEqual(connected["state"], "connected")
        testcase.assertEqual(connected["pipeBindingSha256"], binding)
        testcase.assertIsNone(connected["receiptFrameSha256"])

        ledger_store.consume(CHALLENGE, pipe, receipt_digest, NOW + 7)
        with testcase.assertRaises(LedgerError):
            ledger_store.issue(CHALLENGE, NOW + 8)
        with testcase.assertRaises(LedgerError):
            ledger_store.consume(CHALLENGE, pipe, "f" * 64, NOW + 9)

        consumed = ledger_store.read(CHALLENGE)
        testcase.assertEqual(consumed["state"], "consumed")
        testcase.assertEqual(consumed["pipeBindingSha256"], binding)
        testcase.assertEqual(consumed["receiptFrameSha256"], receipt_digest)

        restarted = OneShotLedger(directory)
        testcase.assertEqual(restarted.recover_incomplete(NOW + 10), 0)
        persisted = restarted.read(CHALLENGE)
        testcase.assertEqual(persisted["state"], "consumed")
        testcase.assertEqual(persisted["pipeBindingSha256"], binding)
        testcase.assertEqual(persisted["receiptFrameSha256"], receipt_digest)

    define_the_c4_v3_native_authority_receipt_schema_for_exact_shipping_builds()


define_c4_one_shot_challenge_ledger_contract.__name__ = (
    "Define the C4 one-shot challenge ledger contract"
)


def define_the_c4_v3_native_authority_receipt_schema_for_exact_shipping_builds() -> None:
    testcase = unittest.TestCase()
    receipt = make_receipt()

    validate_receipt(receipt)
    testcase.assertEqual(receipt["schema"], SCHEMA_NAME)
    testcase.assertEqual(receipt["version"], SCHEMA_VERSION)
    testcase.assertEqual(receipt["build"]["features"], EXACT_SHIPPING_FEATURES)

    non_shipping_features = copy.deepcopy(receipt)
    non_shipping_features["build"]["features"] = ["desktop", "core"]
    non_shipping_features = reseal(non_shipping_features)
    with testcase.assertRaisesRegex(SchemaError, "exact shipping set"):
        validate_receipt(non_shipping_features)

    debug_build = copy.deepcopy(receipt)
    debug_build["build"]["debugAssertions"] = True
    debug_build = reseal(debug_build)
    with testcase.assertRaisesRegex(SchemaError, "debugAssertions"):
        validate_receipt(debug_build)

    wrong_os = copy.deepcopy(receipt)
    wrong_os["build"]["targetOs"] = "linux"
    wrong_os = reseal(wrong_os)
    with testcase.assertRaisesRegex(SchemaError, "targetOs"):
        validate_receipt(wrong_os)

    wrong_arch = copy.deepcopy(receipt)
    wrong_arch["build"]["targetArch"] = "aarch64"
    wrong_arch = reseal(wrong_arch)
    with testcase.assertRaisesRegex(SchemaError, "targetArch"):
        validate_receipt(wrong_arch)

    non_release = copy.deepcopy(receipt)
    non_release["build"]["profile"] = "debug"
    non_release = reseal(non_release)
    with testcase.assertRaisesRegex(SchemaError, "profile"):
        validate_receipt(non_release)


define_the_c4_v3_native_authority_receipt_schema_for_exact_shipping_builds.__name__ = (
    "Define the C4 v3 native authority receipt schema for exact shipping builds"
)


def c4_v3_native_authority_receipt_schema_test_verify_py() -> None:
    define_c4_one_shot_challenge_ledger_contract()
    define_the_c4_v3_native_authority_receipt_schema_for_exact_shipping_builds()


c4_v3_native_authority_receipt_schema_test_verify_py.__name__ = "test_verify.py"


def validate_carrier_and_pre_post_readbacks_against_one_discord_target_binding() -> None:
    testcase = unittest.TestCase()
    receipt = make_receipt()
    binding = receipt["target"]["bindingSha256"]

    validate_receipt(receipt)
    testcase.assertEqual(receipt["carrier"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["preSend"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["preSend"]["readback"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["postSend"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["postSend"]["readback"]["targetBindingSha256"], binding)

    wrong_binding = "f" * 64
    for path, mutate in {
        "carrier": lambda value: value["carrier"].__setitem__(
            "targetBindingSha256", wrong_binding
        ),
        "preSend": lambda value: value["preSend"].__setitem__(
            "targetBindingSha256", wrong_binding
        ),
        "preSend.readback": lambda value: value["preSend"]["readback"].__setitem__(
            "targetBindingSha256", wrong_binding
        ),
        "postSend": lambda value: value["postSend"].__setitem__(
            "targetBindingSha256", wrong_binding
        ),
        "postSend.readback": lambda value: value["postSend"]["readback"].__setitem__(
            "targetBindingSha256", wrong_binding
        ),
    }.items():
        with testcase.subTest(path=path):
            mutated = copy.deepcopy(receipt)
            mutate(mutated)
            mutated = reseal(mutated)
            with testcase.assertRaisesRegex(SchemaError, "target binding"):
                validate_receipt(mutated)

    wrong_pre_readback = copy.deepcopy(receipt)
    wrong_pre_readback["preSend"]["readback"]["sha256"] = sha256_hex(b"different")
    wrong_pre_readback = reseal(wrong_pre_readback)
    with testcase.assertRaisesRegex(SchemaError, "preSend.readback digest"):
        validate_receipt(wrong_pre_readback)

    changed_carrier = copy.deepcopy(receipt)
    changed_carrier_bytes = b"same target, different carrier"
    changed_carrier["carrier"]["utf8B64"] = base64.b64encode(
        changed_carrier_bytes,
    ).decode("ascii")
    changed_carrier["carrier"]["sha256"] = sha256_hex(changed_carrier_bytes)
    changed_carrier["carrier"]["byteLength"] = len(changed_carrier_bytes)
    changed_carrier["carrier"]["utf16Length"] = len(
        changed_carrier_bytes.decode("utf-8").encode("utf-16-le"),
    ) // 2
    changed_carrier = reseal(changed_carrier)
    with testcase.assertRaisesRegex(SchemaError, "preSend.readback digest"):
        validate_receipt(changed_carrier)

    wrong_post_readback = copy.deepcopy(receipt)
    wrong_post_readback["postSend"]["readback"]["byteLength"] = 1
    wrong_post_readback = reseal(wrong_post_readback)
    with testcase.assertRaisesRegex(SchemaError, "postSend.readback byte length"):
        validate_receipt(wrong_post_readback)

    wrong_post_classification = copy.deepcopy(receipt)
    wrong_post_classification["postSend"]["readback"]["classification"] = "exact"
    wrong_post_classification = reseal(wrong_post_classification)
    with testcase.assertRaisesRegex(SchemaError, "postSend.readback.classification"):
        validate_receipt(wrong_post_classification)


validate_carrier_and_pre_post_readbacks_against_one_discord_target_binding.__name__ = (
    "Validate carrier and pre/post readbacks against one Discord target binding"
)


def readback_contract() -> None:
    validate_carrier_and_pre_post_readbacks_against_one_discord_target_binding()


def readback_empty_post_send_contract() -> None:
    testcase = unittest.TestCase()
    receipt = make_receipt()
    binding = receipt["target"]["bindingSha256"]
    carrier_digest = receipt["carrier"]["sha256"]

    validate_receipt(receipt)
    testcase.assertEqual(receipt["carrier"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["preSend"]["readback"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["postSend"]["readback"]["targetBindingSha256"], binding)
    testcase.assertEqual(receipt["preSend"]["readback"]["sha256"], carrier_digest)
    testcase.assertEqual(receipt["postSend"]["readback"]["sha256"], sha256_hex(b""))

    mismatched_binding = copy.deepcopy(receipt)
    mismatched_binding["postSend"]["readback"]["targetBindingSha256"] = "f" * 64
    mismatched_binding = reseal(mismatched_binding)
    with testcase.assertRaisesRegex(SchemaError, "postSend.readback.*target binding"):
        validate_receipt(mismatched_binding)

    permissive_pre_readback = copy.deepcopy(receipt)
    permissive_pre_readback["preSend"]["readback"]["classification"] = "partial"
    permissive_pre_readback = reseal(permissive_pre_readback)
    with testcase.assertRaisesRegex(SchemaError, "preSend.readback.classification"):
        validate_receipt(permissive_pre_readback)

    false_empty_readback = copy.deepcopy(receipt)
    false_empty_readback["postSend"]["readback"]["sha256"] = carrier_digest
    false_empty_readback["postSend"]["readback"]["byteLength"] = receipt["carrier"][
        "byteLength"
    ]
    false_empty_readback["postSend"]["readback"]["utf16Length"] = receipt["carrier"][
        "utf16Length"
    ]
    false_empty_readback = reseal(false_empty_readback)
    with testcase.assertRaisesRegex(SchemaError, "postSend.readback digest"):
        validate_receipt(false_empty_readback)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, pattern
    suite = unittest.TestSuite()
    suite.addTests(tests)
    suite.addTest(NativeAuthorityV3Tests("readback"))
    suite.addTest(unittest.FunctionTestCase(ledger))
    suite.addTest(unittest.FunctionTestCase(
        define_c4_one_shot_challenge_ledger_contract,
    ))
    suite.addTest(unittest.FunctionTestCase(
        define_the_c4_v3_native_authority_receipt_schema_for_exact_shipping_builds,
    ))
    suite.addTest(unittest.FunctionTestCase(
        c4_v3_native_authority_receipt_schema_test_verify_py,
    ))
    suite.addTest(unittest.FunctionTestCase(
        validate_carrier_and_pre_post_readbacks_against_one_discord_target_binding,
    ))
    suite.addTest(unittest.FunctionTestCase(readback_contract))
    suite.addTest(unittest.FunctionTestCase(readback_empty_post_send_contract))
    return suite


if __name__ == "__main__":
    unittest.main()
