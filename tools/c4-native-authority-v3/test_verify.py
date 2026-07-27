from __future__ import annotations

import base64
import copy
import hashlib
import json
from pathlib import Path
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
)
from schema import (
    EXACT_SHIPPING_FEATURES,
    FILE_IDENTITY_KEYS,
    PROCESS_KEYS,
    READBACK_KEYS,
    SCHEMA_NAME,
    SCHEMA_VERSION,
    TARGET_KEYS,
    canonical_json,
    seal_receipt,
    sha256_hex,
    target_binding_digest,
)
from verify import VerificationContext, VerificationError, verify_receipt


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


def reseal(receipt: dict) -> dict:
    receipt["receiptDigestSha256"] = ""
    return seal_receipt(receipt)


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


class CrossLanguageConformanceTests(unittest.TestCase):
    def test_machine_readable_fixture_accepts_only_parser_crypto_positive(self) -> None:
        result = run_fixture()
        self.assertEqual(result["invalidMutationsRejected"], 29)
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
            "2108b6da08ddb303af05a7951930f1a442c9b42548f5bc46eb20143ead9d8302",
        )
        self.assertEqual(
            hashlib.sha256(FIXTURE_PATH.read_bytes()).hexdigest(),
            "811af3378798f91cdd08adb6889fe764272821c4da8e319bfbc5de70fcc6aa5c",
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


class OneShotLedgerTests(unittest.TestCase):
    def test_issue_connect_consume_is_atomic_and_replay_is_rejected(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW)
            ledger.connect(CHALLENGE, pipe, NOW + 1)
            ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 2)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "consumed")
            with self.assertRaises(LedgerError):
                ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 3)
            with self.assertRaises(LedgerError):
                ledger.issue(CHALLENGE, NOW + 4)

    def test_only_one_concurrent_connection_can_claim_a_challenge(self) -> None:
        receipt = make_receipt()
        pipe = receipt["emitter"]
        outcomes: list[str] = []
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
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
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, receipt["emitter"], NOW + 11)
            self.assertEqual(ledger.read(CHALLENGE)["state"], "expired")

    def test_crash_recovery_abandons_nonterminal_challenges(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
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
                    ledger = OneShotLedger(directory)
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
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW)
            validate_ledger_record(ledger.read(CHALLENGE))
            ledger.connect(CHALLENGE, pipe, NOW + 1)
            validate_ledger_record(ledger.read(CHALLENGE))
            ledger.consume(CHALLENGE, pipe, "e" * 64, NOW + 2)
            validate_ledger_record(ledger.read(CHALLENGE))

        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW, ttl_ms=10)
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, pipe, NOW + 11)
            validate_ledger_record(ledger.read(CHALLENGE))

        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW)
            ledger.recover_incomplete(NOW + 1)
            validate_ledger_record(ledger.read(CHALLENGE))

    def test_ledger_state_fields_and_binding_types_are_strict(self) -> None:
        receipt = make_receipt()
        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
            ledger.issue(CHALLENGE, NOW)
            bad_binding = copy.deepcopy(receipt["emitter"])
            bad_binding["pid"] = str(bad_binding["pid"])
            with self.assertRaises(LedgerError):
                ledger.connect(CHALLENGE, bad_binding, NOW + 1)

        with tempfile.TemporaryDirectory() as directory:
            ledger = OneShotLedger(directory)
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


if __name__ == "__main__":
    unittest.main()
