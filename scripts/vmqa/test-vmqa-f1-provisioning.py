#!/usr/bin/env python3
"""Lightweight adversarial tests for the fixed F1 provisioning preflight."""

from __future__ import annotations

import argparse
import json
import os
import stat
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

import vmqa_f1_provisioning_preflight as preflight


class F1ProvisioningTests(unittest.TestCase):
    def setUp(self) -> None:
        self.owned = tempfile.TemporaryDirectory(
            prefix="vmqa-f1-provisioning-"
        )
        self.root = Path(self.owned.name)
        self.uid = os.geteuid()
        self.opt = self.root / "opt"
        self.install = self.opt / "osl-vmqa"
        self.bin = self.install / "bin"
        self.toolchain = self.install / "toolchain"
        self.tool_bin = self.toolchain / "bin"
        self.var_root = self.root / "var"
        self.var_lib = self.var_root / "lib"
        self.vmqa_state = self.var_lib / "osl-vmqa"
        self.authority = self.var_lib / "osl-qa"
        self.private = self.authority / "private"
        self.staging = self.authority / "f1-staging"
        self.seals = self.vmqa_state / "producer-seals"
        for directory, mode in (
            (self.opt, 0o755),
            (self.install, 0o755),
            (self.bin, 0o755),
            (self.toolchain, 0o755),
            (self.tool_bin, 0o755),
            (self.var_root, 0o755),
            (self.var_lib, 0o755),
            (self.vmqa_state, 0o755),
            (self.authority, 0o755),
            (self.private, 0o700),
            (self.staging, 0o700),
            (self.seals, 0o755),
        ):
            directory.mkdir(parents=True, exist_ok=True)
            directory.chmod(mode)
        self.layout = preflight.ProvisioningLayout(
            opt_root=self.opt,
            install_root=self.install,
            bin_root=self.bin,
            toolchain_root=self.toolchain,
            toolchain_bin=self.tool_bin,
            var_root=self.var_root,
            var_lib_root=self.var_lib,
            vmqa_state_root=self.vmqa_state,
            authority_root=self.authority,
            private_root=self.private,
            staging_root=self.staging,
            seal_root=self.seals,
            root_uid=self.uid,
        )
        self.identity = preflight.ProducerIdentity(
            name=preflight.PRODUCER_USER,
            uid=self.uid,
            gid=os.getegid(),
            shell="/usr/sbin/nologin",
        )
        self.program_pins: dict[str, str] = {}
        for name in preflight.PROGRAM_PINS:
            self.program_pins[name] = self._write_pinned(
                self.bin / name, f"fixed {name}\n".encode()
            )
        self._write_mode(
            self.bin / "vmqa_f1_producer.py",
            b"fixed root-owned producer entrypoint\n",
            0o555,
        )
        self._write_mode(
            self.bin / Path(preflight.__file__).name,
            b"fixed provisioning entrypoint\n",
            0o555,
        )
        self.tool_pins: dict[str, tuple[Path, str]] = {}
        for name in ("git", "npm", "node", "osl-cargo", "rustc", "cargo"):
            path = self.tool_bin / name
            self.tool_pins[name] = (
                path,
                self._write_pinned(path, f"fixed {name}\n".encode()),
            )
        self._write_mode(
            self.seals / ".seal-chain-state.json", b"state\n", 0o600
        )
        self._write_mode(self.seals / ".seal-chain.lock", b"", 0o600)
        self._write_mode(
            self.staging / preflight.producer.ADMISSION_LOCK_NAME,
            b"",
            0o600,
        )
        self.fixture_key = bytes.fromhex("11" * 32)
        self._write_mode(
            self.private / "f1-native-witness.key",
            ("11" * 32 + "\n").encode("ascii"),
            0o600,
        )
        self.admitted = self._make_stage()

    def tearDown(self) -> None:
        self.owned.cleanup()

    @staticmethod
    def _sha(value: bytes) -> str:
        return preflight.sha256_bytes(value)

    @staticmethod
    def _write_mode(path: Path, value: bytes, mode: int) -> None:
        path.write_bytes(value)
        path.chmod(mode)

    def _write_pinned(self, path: Path, value: bytes) -> str:
        self._write_mode(path, value, 0o555)
        return self._sha(value)

    def _make_stage(self) -> SimpleNamespace:
        identity_bytes = b'{"fixed":"identity"}\n'
        exe_bytes = b"sealed exact executable\n"
        loader_bytes = b"sealed exact loader\n"
        seal_bytes = b'{"fixed":"producer-seal"}\n'
        identity_sha = self._sha(identity_bytes)
        exe_sha = self._sha(exe_bytes)
        loader_sha = self._sha(loader_bytes)
        seal_sha = self._sha(seal_bytes)
        destination = self.staging / exe_sha
        outputs = destination / "bundle/outputs"
        outputs.mkdir(parents=True)
        destination.chmod(0o700)
        self._write_mode(
            destination / "bundle/build-identity.json",
            identity_bytes,
            0o644,
        )
        self._write_mode(
            outputs / "osl-privacy-hub.exe", exe_bytes, 0o644
        )
        self._write_mode(outputs / "WebView2Loader.dll", loader_bytes, 0o644)
        self._write_mode(
            destination / "producer-seal.json", seal_bytes, 0o400
        )
        admitted = SimpleNamespace(
            mode="production",
            identity_sha256=identity_sha,
            executable_sha256=exe_sha,
            executable_size=len(exe_bytes),
            loader_sha256=loader_sha,
            loader_size=len(loader_bytes),
            producer_seal=self.seals / f"{identity_sha}.json",
            producer_seal_bytes=seal_bytes,
            producer_seal_sha256=seal_sha,
            seal_generation=1,
            previous_seal_sha256=preflight.build_evidence.EMPTY_SHA256,
            seal_transition="initial",
        )
        state = preflight.producer.admission_state_record(admitted)
        self._write_mode(
            self.staging / preflight.producer.ADMISSION_STATE_NAME,
            preflight.canonical_json(state),
            0o600,
        )
        snapshot = preflight.producer.terminal_destination_snapshot(
            destination,
            reported_root=destination,
            producer_uid=self.uid,
        )
        receipt = {
            "schemaVersion": preflight.producer.SCHEMA_VERSION,
            "mode": "production",
            "producer": preflight.PRODUCER_USER,
            "source": {
                "commit": preflight.producer.PINNED_COMMIT,
                "tree": preflight.producer.PINNED_TREE,
            },
            "buildIdentitySha256": identity_sha,
            "producerSeal": {
                "authorityPath": admitted.producer_seal.as_posix(),
                "path": (destination / "producer-seal.json").as_posix(),
                "sha256": seal_sha,
                "generation": 1,
                "previousSealSha256": preflight.build_evidence.EMPTY_SHA256,
                "transition": "initial",
            },
            "executable": {
                "path": (
                    destination
                    / "bundle"
                    / preflight.build_evidence.FINAL_EXE
                ).as_posix(),
                "sha256": exe_sha,
                "sizeBytes": len(exe_bytes),
            },
            "loader": {
                "path": (
                    destination
                    / "bundle"
                    / preflight.build_evidence.FINAL_LOADER
                ).as_posix(),
                "sha256": loader_sha,
                "sizeBytes": len(loader_bytes),
            },
            "nativeWitnessKeyId": self._sha(self.fixture_key),
            "admissionTransition": {
                "kind": "initial-admission",
                "fromGeneration": 0,
                "fromSealSha256": preflight.build_evidence.EMPTY_SHA256,
                "fromIdentitySha256": preflight.build_evidence.EMPTY_SHA256,
                "toGeneration": 1,
                "toSealSha256": seal_sha,
                "toIdentitySha256": identity_sha,
                "globalPreviousSealSha256": (
                    preflight.build_evidence.EMPTY_SHA256
                ),
            },
            "terminalSnapshot": snapshot,
            "terminalSnapshotSha256": self._sha(
                preflight.canonical_json(snapshot)
            ),
        }
        self._write_mode(
            destination / "staging-receipt.json",
            preflight.canonical_json(receipt),
            0o600,
        )
        return admitted

    def run_preflight(self) -> dict[str, object]:
        return preflight.preflight_host(
            layout=self.layout,
            identity=self.identity,
            effective_uid=0,
            program_pins=self.program_pins,
            tool_pins=self.tool_pins,
            inspect_bundle=lambda _: self.admitted,
        )

    def receipt_path(self) -> Path:
        return (
            self.staging
            / self.admitted.executable_sha256
            / "staging-receipt.json"
        )

    def mutate_receipt(self, callback: object) -> None:
        path = self.receipt_path()
        value = json.loads(path.read_text())
        callback(value)  # type: ignore[operator]
        self._write_mode(path, preflight.canonical_json(value), 0o600)

    def test_nonempty_dry_run_accepts_exact_state_without_writes(self) -> None:
        before = {
            path.relative_to(self.root).as_posix(): (
                path.stat().st_mode,
                self._sha(path.read_bytes()) if path.is_file() else None,
            )
            for path in self.root.rglob("*")
        }
        result = self.run_preflight()
        after = {
            path.relative_to(self.root).as_posix(): (
                path.stat().st_mode,
                self._sha(path.read_bytes()) if path.is_file() else None,
            )
            for path in self.root.rglob("*")
        }
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["writesPerformed"], 0)
        self.assertEqual(
            result["currentStage"]["executableSha256"],
            self.admitted.executable_sha256,
        )
        self.assertEqual(before, after)

    def test_missing_and_wrong_account_identity_refuse(self) -> None:
        with mock.patch.object(
            preflight.pwd, "getpwnam", side_effect=KeyError
        ):
            with self.assertRaisesRegex(
                preflight.ProvisioningError, "account is missing"
            ):
                preflight.resolve_producer_identity()
        cases = (
            (SimpleNamespace(**{**self.identity.__dict__, "name": "caller"}), "identity"),
            (SimpleNamespace(**{**self.identity.__dict__, "uid": 0}), "identity"),
            (
                SimpleNamespace(
                    **{**self.identity.__dict__, "shell": "/bin/bash"}
                ),
                "non-login",
            ),
        )
        for identity, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(
                preflight.ProvisioningError, reason
            ):
                preflight.validate_administrator_and_identity(
                    identity, effective_uid=0
                )
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "requires uid 0"
        ):
            preflight.validate_administrator_and_identity(
                self.identity, effective_uid=1234
            )

    def test_wrong_owner_mode_type_and_symlink_refuse(self) -> None:
        value = SimpleNamespace(
            st_mode=stat.S_IFREG | 0o555, st_uid=self.uid
        )
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong owner"
        ):
            preflight.validate_stat(
                value,
                kind="file",
                owner_uid=self.uid + 1,
                mode=0o555,
                label="fixture",
            )
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong mode"
        ):
            preflight.validate_stat(
                value,
                kind="file",
                owner_uid=self.uid,
                mode=0o444,
                label="fixture",
            )
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "not a real directory"
        ):
            preflight.validate_stat(
                value,
                kind="directory",
                owner_uid=self.uid,
                mode=0o555,
                label="fixture",
            )
        target = self.root / "real"
        target.write_bytes(b"x")
        link = self.root / "link"
        link.symlink_to(target)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "missing or unsafe"
        ):
            preflight.read_fixed_file(
                link,
                owner_uid=self.uid,
                mode=0o644,
                label="symlink",
            )
        ancestor_root = self.root / "ancestor-root"
        real_bin = self.root / "real-bin"
        ancestor_root.mkdir(mode=0o755)
        real_bin.mkdir(mode=0o755)
        (ancestor_root / "bin").symlink_to(real_bin)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "missing or unsafe"
        ):
            preflight.require_root_owned_ancestors(
                ancestor_root / "bin/tool",
                root=ancestor_root,
                root_uid=self.uid,
                label="symlinked tool",
            )

    def test_missing_program_wrong_mode_and_wrong_hash_refuse(self) -> None:
        program = self.bin / "vmqa_build_evidence.py"
        program.unlink()
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "missing"
        ):
            self.run_preflight()
        self._write_mode(program, b"fixed vmqa_build_evidence.py\n", 0o555)
        program.chmod(0o755)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong mode"
        ):
            self.run_preflight()
        program.chmod(0o755)
        program.write_bytes(b"wrong program bytes\n")
        program.chmod(0o555)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "hash differs"
        ):
            self.run_preflight()

    def test_missing_tool_wrong_mode_hash_and_pin_refuse(self) -> None:
        cargo, digest = self.tool_pins["cargo"]
        cargo.unlink()
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "missing"
        ):
            self.run_preflight()
        self._write_mode(cargo, b"fixed cargo\n", 0o755)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong mode"
        ):
            self.run_preflight()
        self._write_mode(cargo, b"wrong cargo\n", 0o555)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "hash differs"
        ):
            self.run_preflight()
        bad_pins = dict(self.tool_pins)
        bad_pins["cargo"] = (self.root / "caller-cargo", digest)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "escapes fixed toolchain"
        ):
            preflight.validate_tool_pin_contract(
                bad_pins, self.toolchain
            )
        bad_pins = dict(self.tool_pins)
        bad_pins["cargo"] = (cargo, "bad")
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "hash is invalid"
        ):
            preflight.validate_tool_pin_contract(
                bad_pins, self.toolchain
            )

    def test_current_liam_owned_production_tool_pins_remain_fail_closed(
        self,
    ) -> None:
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "escapes fixed toolchain"
        ):
            preflight.validate_tool_pin_contract(
                preflight.build_evidence.PRODUCTION_TOOL_PINS,
                preflight.FIXED_TOOLCHAIN_ROOT,
            )

    def test_committed_program_hash_pins_match_exact_source_bytes(self) -> None:
        script_root = Path(preflight.__file__).resolve().parent
        self.assertEqual(
            self._sha(
                (
                    script_root / "vmqa_f1_provisioning_preflight.py"
                ).read_bytes()
            ),
            preflight.producer.PINNED_PROVISIONING_PROGRAM_SHA256,
        )
        for name, expected in preflight.PROGRAM_PINS.items():
            with self.subTest(name=name):
                self.assertEqual(
                    self._sha((script_root / name).read_bytes()), expected
                )

    def test_wrong_authority_mode_and_missing_monotonic_state_refuse(self) -> None:
        self.private.chmod(0o755)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong mode"
        ):
            self.run_preflight()
        self.private.chmod(0o700)
        self.var_lib.chmod(0o700)
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "wrong mode"
        ):
            self.run_preflight()
        self.var_lib.chmod(0o755)
        (self.seals / ".seal-chain-state.json").unlink()
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "missing"
        ):
            self.run_preflight()

    def test_missing_or_wrong_key_metadata_and_unexpected_stage_refuse(
        self,
    ) -> None:
        key = self.private / "f1-native-witness.key"
        key.unlink()
        with self.assertRaisesRegex(
            preflight.producer.ProducerError, "witness key"
        ):
            self.run_preflight()
        self._write_mode(
            key, ("11" * 32 + "\n").encode("ascii"), 0o640
        )
        with self.assertRaisesRegex(
            preflight.producer.ProducerError, "wrong mode"
        ):
            self.run_preflight()
        key.chmod(0o600)
        (self.staging / "caller-junk").write_bytes(b"junk")
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "unexpected entry"
        ):
            self.run_preflight()

    def test_fixture_or_noncurrent_seal_refuses(self) -> None:
        self.admitted.mode = "fixture"
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "not production-sealed"
        ):
            self.run_preflight()
        self.admitted.mode = "production"
        state_path = (
            self.staging / preflight.producer.ADMISSION_STATE_NAME
        )
        state = json.loads(state_path.read_text())
        state["sealSha256"] = "f" * 64
        self._write_mode(
            state_path, preflight.canonical_json(state), 0o600
        )
        with self.assertRaisesRegex(
            preflight.ProvisioningError, "exactly one protected stage"
        ):
            self.run_preflight()

    def test_wrong_commit_tree_identity_exe_loader_and_receipt_refuse(
        self,
    ) -> None:
        mutations = (
            (
                "commit",
                lambda value: value["source"].__setitem__("commit", "0" * 40),
                "sealed receipt",
            ),
            (
                "tree",
                lambda value: value["source"].__setitem__("tree", "0" * 40),
                "sealed receipt",
            ),
            (
                "identity",
                lambda value: value.__setitem__(
                    "buildIdentitySha256", "0" * 64
                ),
                "sealed receipt",
            ),
            (
                "receipt hash",
                lambda value: value.__setitem__(
                    "terminalSnapshotSha256", "0" * 64
                ),
                "sealed receipt",
            ),
        )
        for name, mutation, reason in mutations:
            with self.subTest(name=name):
                original = self.receipt_path().read_bytes()
                self.mutate_receipt(mutation)
                with self.assertRaisesRegex(
                    preflight.ProvisioningError, reason
                ):
                    self.run_preflight()
                self._write_mode(self.receipt_path(), original, 0o600)
        destination = self.receipt_path().parent
        for name, path in (
            (
                "executable",
                destination
                / "bundle"
                / preflight.build_evidence.FINAL_EXE,
            ),
            (
                "loader",
                destination
                / "bundle"
                / preflight.build_evidence.FINAL_LOADER,
            ),
            ("identity", destination / "bundle/build-identity.json"),
            ("seal", destination / "producer-seal.json"),
        ):
            with self.subTest(name=name):
                original = path.read_bytes()
                if name == "seal":
                    path.chmod(0o600)
                with path.open("ab") as handle:
                    handle.write(b"wrong bytes")
                with self.assertRaisesRegex(
                    (preflight.ProvisioningError, preflight.producer.ProducerError),
                    "terminal .*|sealed receipt",
                ):
                    self.run_preflight()
                self._write_mode(
                    path, original, 0o400 if name == "seal" else 0o644
                )

    def test_cli_rejects_every_caller_selected_authority(self) -> None:
        for option in (
            "--path",
            "--expected-hash",
            "--identity",
            "--tool-pin",
            "--fixture",
        ):
            with self.subTest(option=option), mock.patch.object(
                preflight,
                "production_preflight",
                side_effect=AssertionError("must reject before inspection"),
            ), mock.patch.object(
                argparse.ArgumentParser, "print_usage"
            ):
                self.assertEqual(preflight.main([option, "caller"]), 9)


if __name__ == "__main__":
    unittest.main()
