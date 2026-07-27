#!/usr/bin/env python3
"""Adversarial lightweight tests for the F1 producer boundary."""

from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

import vmqa_f1_producer as producer_module


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[1]
BUILD_SCRIPT = HERE / "vmqa_build_evidence.py"
TEST_KEY_TEXT = ("11" * 32 + "\n").encode("ascii")


class F1ProducerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture_root_owner = tempfile.TemporaryDirectory(
            prefix="vmqa-f1-producer-fixture-"
        )
        root = Path(cls.fixture_root_owner.name)
        cls.fixture_bundle = root / "bundle"
        subprocess.run(
            [
                "python3",
                str(BUILD_SCRIPT),
                "create-fixture",
                "--source-repo",
                str(REPO_ROOT),
                "--output",
                str(cls.fixture_bundle),
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        cls.fixture_seal = (
            producer_module.build_evidence.fixture_seal_path(
                cls.fixture_bundle
            )
        )
        if not cls.fixture_seal.is_file():
            raise AssertionError("fixture producer seal was not created")

    @classmethod
    def tearDownClass(cls) -> None:
        cls.fixture_root_owner.cleanup()

    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(
            prefix="vmqa-f1-producer-test-"
        )
        self.root = Path(self.owned.name)
        self.authority = self.root / "osl-qa"
        self.private = self.authority / "private"
        self.staging = self.authority / "f1-staging"
        self.authority.mkdir(mode=0o755)
        self.private.mkdir(mode=0o700)
        self.staging.mkdir(mode=0o700)
        self.key = self.private / "f1-native-witness.key"
        self.key.write_bytes(TEST_KEY_TEXT)
        self.key.chmod(0o600)
        self.identity = producer_module.ProducerIdentity(
            name=producer_module.PRODUCER_USER,
            uid=os.geteuid(),
            gid=os.getegid(),
            shell="/usr/sbin/nologin",
        )
        self.layout = producer_module.ProducerLayout(
            authority_root=self.authority,
            key_directory=self.private,
            key_path=self.key,
            staging_root=self.staging,
            authority_uid=os.geteuid(),
            enforce_installed_programs=False,
        )

    def tearDown(self) -> None:
        self.owned.cleanup()

    def preflight(self, bundle: Path | None = None) -> dict[str, object]:
        return producer_module.preflight(
            bundle or self.fixture_bundle,
            layout=self.layout,
            producer=self.identity,
            effective_uid=self.identity.uid,
            internal_fixture_seal=self.fixture_seal,
        )

    def copied_bundle_and_seal(self) -> tuple[Path, Path]:
        bundle = self.root / "mutated-bundle"
        shutil.copytree(self.fixture_bundle, bundle)
        seal = self.root / "mutated.producer-seal.json"
        shutil.copy2(self.fixture_seal, seal)
        return bundle, seal

    def test_positive_preflight_is_nonwriting_and_exactly_bound(self) -> None:
        before = list(self.staging.iterdir())
        result = self.preflight()
        self.assertEqual(result["status"], "ready")
        self.assertEqual(
            result["sourceCommit"], producer_module.PINNED_COMMIT
        )
        self.assertEqual(result["sourceTree"], producer_module.PINNED_TREE)
        self.assertEqual(list(self.staging.iterdir()), before)
        self.assertFalse(Path(result["wouldStage"]).exists())

    def test_stage_binds_published_bytes_and_retains_no_key(self) -> None:
        result = producer_module.stage(
            self.fixture_bundle,
            layout=self.layout,
            producer=self.identity,
            effective_uid=self.identity.uid,
            internal_fixture_seal=self.fixture_seal,
        )
        receipt = Path(result["receiptPath"])
        self.assertTrue(receipt.is_file())
        self.assertEqual(receipt.stat().st_mode & 0o777, 0o600)
        receipt_bytes = receipt.read_bytes()
        self.assertNotIn(TEST_KEY_TEXT.rstrip(), receipt_bytes)
        self.assertEqual(
            producer_module.sha256_file(
                Path(result["producerSeal"]["path"])
            ),
            result["producerSeal"]["sha256"],
        )
        self.assertEqual(
            result["nativeWitnessKeyId"],
            producer_module.sha256_bytes(bytes.fromhex("11" * 32)),
        )
        self.assertEqual(
            producer_module.sha256_file(Path(result["executable"]["path"])),
            result["executable"]["sha256"],
        )
        self.assertEqual(
            producer_module.sha256_file(Path(result["loader"]["path"])),
            result["loader"]["sha256"],
        )

    def test_missing_key_refuses_before_bundle_admission(self) -> None:
        self.key.unlink()
        with mock.patch.object(
            producer_module,
            "inspect_admitted_bundle",
            side_effect=AssertionError("bundle verifier must not run"),
        ):
            with self.assertRaisesRegex(
                producer_module.ProducerError, "witness key"
            ):
                self.preflight()

    def test_wrong_key_acl_refuses(self) -> None:
        self.key.chmod(0o640)
        with self.assertRaisesRegex(
            producer_module.ProducerError, "wrong mode"
        ):
            self.preflight()

    def test_key_symlink_refuses(self) -> None:
        actual = self.root / "caller-key"
        actual.write_bytes(TEST_KEY_TEXT)
        actual.chmod(0o600)
        self.key.unlink()
        self.key.symlink_to(actual)
        with self.assertRaisesRegex(
            producer_module.ProducerError, "missing or unsafe"
        ):
            self.preflight()

    def test_wrong_directory_acl_refuses(self) -> None:
        self.private.chmod(0o755)
        with self.assertRaisesRegex(
            producer_module.ProducerError, "mode 0700"
        ):
            self.preflight()

    def test_wrong_effective_identity_refuses_before_key_read(self) -> None:
        with mock.patch.object(
            producer_module,
            "read_witness_key",
            side_effect=AssertionError("key must not be read"),
        ):
            with self.assertRaisesRegex(
                producer_module.ProducerError, "dedicated producer identity"
            ):
                producer_module.preflight(
                    self.fixture_bundle,
                    layout=self.layout,
                    producer=self.identity,
                    effective_uid=self.identity.uid + 1,
                    internal_fixture_seal=self.fixture_seal,
                )

    def test_login_capable_producer_refuses(self) -> None:
        identity = producer_module.ProducerIdentity(
            name=producer_module.PRODUCER_USER,
            uid=self.identity.uid,
            gid=self.identity.gid,
            shell="/bin/bash",
        )
        with self.assertRaisesRegex(
            producer_module.ProducerError, "must be non-login"
        ):
            producer_module.preflight(
                self.fixture_bundle,
                layout=self.layout,
                producer=identity,
                effective_uid=identity.uid,
                internal_fixture_seal=self.fixture_seal,
            )

    def test_fixture_bundle_is_forbidden_by_production_admission(self) -> None:
        with self.assertRaisesRegex(
            producer_module.ProducerError, "fixture build evidence is forbidden"
        ):
            producer_module.preflight(
                self.fixture_bundle,
                layout=self.layout,
                producer=self.identity,
                effective_uid=self.identity.uid,
            )

    def test_wrong_source_commit_refuses(self) -> None:
        bundle, seal = self.copied_bundle_and_seal()
        identity_path = bundle / "build-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["source"]["commit"] = "0" * 40
        identity_path.write_text(json.dumps(identity) + "\n")
        with self.assertRaisesRegex(
            producer_module.ProducerError, "producer-admitted|producer seal"
        ):
            producer_module.preflight(
                bundle,
                layout=self.layout,
                producer=self.identity,
                effective_uid=self.identity.uid,
                internal_fixture_seal=seal,
            )

    def test_wrong_source_tree_refuses(self) -> None:
        bundle, seal = self.copied_bundle_and_seal()
        identity_path = bundle / "build-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["source"]["tree"] = "0" * 40
        identity_path.write_text(json.dumps(identity) + "\n")
        with self.assertRaisesRegex(
            producer_module.ProducerError, "producer-admitted|producer seal"
        ):
            producer_module.preflight(
                bundle,
                layout=self.layout,
                producer=self.identity,
                effective_uid=self.identity.uid,
                internal_fixture_seal=seal,
            )

    def test_wrong_executable_bytes_refuse(self) -> None:
        bundle, seal = self.copied_bundle_and_seal()
        (bundle / producer_module.build_evidence.FINAL_EXE).write_bytes(
            b"not the admitted executable\n"
        )
        with self.assertRaisesRegex(
            producer_module.ProducerError,
            "independent executable|published executable",
        ):
            producer_module.preflight(
                bundle,
                layout=self.layout,
                producer=self.identity,
                effective_uid=self.identity.uid,
                internal_fixture_seal=seal,
            )

    def test_wrong_executable_hash_refuses(self) -> None:
        bundle, seal = self.copied_bundle_and_seal()
        identity_path = bundle / "build-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["artifacts"]["executable"]["sha256"] = "0" * 64
        identity_path.write_text(json.dumps(identity) + "\n")
        with self.assertRaisesRegex(
            producer_module.ProducerError, "producer-admitted|producer seal"
        ):
            producer_module.preflight(
                bundle,
                layout=self.layout,
                producer=self.identity,
                effective_uid=self.identity.uid,
                internal_fixture_seal=seal,
            )

    def test_replay_refuses_existing_stage_without_replacement(self) -> None:
        first = producer_module.stage(
            self.fixture_bundle,
            layout=self.layout,
            producer=self.identity,
            effective_uid=self.identity.uid,
            internal_fixture_seal=self.fixture_seal,
        )
        receipt = Path(first["receiptPath"])
        before = receipt.read_bytes()
        with self.assertRaisesRegex(
            producer_module.ProducerError, "replay refused"
        ):
            self.preflight()
        self.assertEqual(receipt.read_bytes(), before)

    def test_destination_appearing_at_publish_is_not_replaced(self) -> None:
        admitted = producer_module.inspect_admitted_bundle(
            self.fixture_bundle,
            internal_fixture_seal=self.fixture_seal,
        )
        destination = producer_module.staging_destination(
            self.layout, admitted
        )
        original_rename = producer_module.build_evidence.rename_noreplace

        def create_racer(
            source: Path, parent_fd: int, destination_name: str
        ) -> None:
            destination.write_bytes(b"caller destination\n")
            original_rename(source, parent_fd, destination_name)

        with mock.patch.object(
            producer_module.build_evidence,
            "rename_noreplace",
            side_effect=create_racer,
        ):
            with self.assertRaisesRegex(
                producer_module.build_evidence.EvidenceError,
                "destination appeared",
            ):
                producer_module.stage(
                    self.fixture_bundle,
                    layout=self.layout,
                    producer=self.identity,
                    effective_uid=self.identity.uid,
                    internal_fixture_seal=self.fixture_seal,
                )
        self.assertEqual(destination.read_bytes(), b"caller destination\n")
        self.assertEqual(
            [
                path
                for path in self.staging.iterdir()
                if path.name.startswith(".f1-stage-")
            ],
            [],
        )

    def test_caller_controlled_key_path_option_refuses(self) -> None:
        caller_key = self.root / "caller-selected.key"
        with mock.patch.object(
            producer_module,
            "resolve_producer_identity",
            side_effect=AssertionError("parser must refuse first"),
        ), redirect_stderr(io.StringIO()):
            rc = producer_module.main(
                [
                    "bootstrap-key",
                    "--import-stdin",
                    "--key-path",
                    str(caller_key),
                ]
            )
        self.assertEqual(rc, 9)
        self.assertFalse(caller_key.exists())

    def test_caller_cannot_supply_exe_hash_source_or_destination(self) -> None:
        forbidden = {
            "--exe": str(self.root / "caller.exe"),
            "--expected-exe-sha256": "0" * 64,
            "--expected-commit": "0" * 40,
            "--expected-tree": "0" * 40,
            "--output": str(self.root / "caller-output"),
        }
        for option, value in forbidden.items():
            with self.subTest(option=option), mock.patch.object(
                producer_module,
                "resolve_producer_identity",
                side_effect=AssertionError("parser must refuse first"),
            ), redirect_stderr(io.StringIO()):
                rc = producer_module.main(
                    [
                        "preflight",
                        "--bundle",
                        str(self.fixture_bundle),
                        option,
                        value,
                    ]
                )
            self.assertEqual(rc, 9)

    def test_import_bootstrap_writes_only_fixed_key_without_echo(self) -> None:
        self.key.unlink()
        result = producer_module.bootstrap_key(
            layout=self.layout,
            producer=self.identity,
            generate=False,
            import_stream=io.BytesIO(TEST_KEY_TEXT),
            effective_uid=self.identity.uid,
        )
        self.assertEqual(self.key.read_bytes(), TEST_KEY_TEXT)
        self.assertEqual(self.key.stat().st_mode & 0o777, 0o600)
        self.assertNotIn("11" * 32, json.dumps(result))
        self.assertEqual(
            {path.name for path in self.private.iterdir()},
            {self.key.name},
        )

    def test_generation_path_can_be_tested_without_real_random_key(self) -> None:
        self.key.unlink()
        with mock.patch.object(
            producer_module.secrets, "token_hex", return_value="22" * 32
        ):
            result = producer_module.bootstrap_key(
                layout=self.layout,
                producer=self.identity,
                generate=True,
                import_stream=None,
                effective_uid=self.identity.uid,
            )
        self.assertEqual(self.key.read_bytes(), ("22" * 32 + "\n").encode())
        self.assertNotIn("22" * 32, json.dumps(result))

    def test_existing_key_refuses_without_overwrite(self) -> None:
        before = self.key.read_bytes()
        with self.assertRaisesRegex(
            producer_module.ProducerError, "already exists"
        ):
            producer_module.bootstrap_key(
                layout=self.layout,
                producer=self.identity,
                generate=False,
                import_stream=io.BytesIO(("33" * 32 + "\n").encode()),
                effective_uid=self.identity.uid,
            )
        self.assertEqual(self.key.read_bytes(), before)

    def test_invalid_import_does_not_leave_key_or_temp_secret(self) -> None:
        self.key.unlink()
        with self.assertRaisesRegex(
            producer_module.ProducerError, "exactly 32"
        ):
            producer_module.bootstrap_key(
                layout=self.layout,
                producer=self.identity,
                generate=False,
                import_stream=io.BytesIO(b"caller garbage"),
                effective_uid=self.identity.uid,
            )
        self.assertEqual(list(self.private.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
