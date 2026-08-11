#!/usr/bin/env python3
"""Break-test TASK 4919's lifecycle verifier with isolated package models.

These tests deliberately use TASK 4919's unit receipt model.  They prove that
the verifier's semantics catch each requested escaped class and that the break
proof itself is non-vacuous.  They do not turn the fixture into a Windows
capture and never print the production ``TOR-4919-GREEN`` marker.
"""

from __future__ import annotations

import copy
import json
import shutil
import sys
import tempfile
import unittest
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import task4919_verify as verifier
from test_task4919_verify import ReceiptFixture, digest


@dataclass(frozen=True)
class Mutation:
    mutation_id: str
    caller_class: str
    state: str
    process: str
    destination: str
    protocol: str
    byte_kind: str
    byte_count: int
    required_transition_type: str
    required_transition: str
    lan_protocol: str | None = None
    delayed_job: bool = False
    dynamic_unlisted: bool = False


MUTATIONS = (
    Mutation(
        "dns_after_wake",
        "dns",
        "sleep_wake",
        "osl-privacy-hub.exe",
        "wake-check.invalid:53",
        "dns",
        "dnsBytes",
        73,
        "power_transition",
        "wake",
    ),
    Mutation(
        "keyserver_after_interface_change",
        "keyserver_directory",
        "network_interface_change",
        "osl-privacy-hub.exe",
        "keyserver.oslprivacy.com:443",
        "tcp",
        "directBytes",
        211,
        "network_transition",
        "interface",
    ),
    Mutation(
        "attachment_after_idle",
        "attachment",
        "forced_delayed_background_jobs",
        "osl-privacy-hub.exe",
        "store.oslprivacy.com:443",
        "tcp",
        "directBytes",
        307,
        "lifecycle",
        "idle_24h",
    ),
    Mutation(
        "updater_after_sidecar_recovery",
        "updater",
        "sidecar_recovery",
        "osl-updater.exe",
        "updates.oslprivacy.com:443",
        "tcp",
        "directBytes",
        401,
        "sidecar_transition",
        "recovery",
    ),
    Mutation(
        "telemetry_after_sidecar_recovery",
        "telemetry",
        "sidecar_recovery",
        "osl-privacy-hub.exe",
        "telemetry.oslprivacy.com:443",
        "tcp",
        "directBytes",
        419,
        "sidecar_transition",
        "recovery",
    ),
    Mutation(
        "delayed_job_after_app_restart",
        "polling",
        "app_restart",
        "osl-delayed-job.exe",
        "directory.oslprivacy.com:443",
        "tcp",
        "directBytes",
        509,
        "process_transition",
        "app_restart",
        delayed_job=True,
    ),
    Mutation(
        "tcp_lan_after_os_restart",
        "osl_lan",
        "os_restart",
        "osl-lan.exe",
        "192.168.50.23:4916",
        "tcp",
        "lanBytes",
        61,
        "process_transition",
        "os_restart",
        lan_protocol="tcp",
    ),
    Mutation(
        "udp_lan_after_os_restart",
        "osl_lan",
        "os_restart",
        "osl-lan.exe",
        "239.255.49.16:4916",
        "udp",
        "lanBytes",
        67,
        "process_transition",
        "os_restart",
        lan_protocol="udp",
    ),
    Mutation(
        "dynamic_unlisted_socket_after_os_restart",
        "unclassified",
        "os_restart",
        "osl-dynamic-plugin.dll",
        "unlisted.invalid:9443",
        "tcp",
        "directBytes",
        89,
        "process_transition",
        "os_restart",
        dynamic_unlisted=True,
    ),
)


def read_events(root: Path) -> list[dict]:
    return [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]


def write_bundle(root: Path, manifest: dict, events: list[dict]) -> None:
    events.sort(key=lambda row: verifier.parse_time(row["atUtc"], "4919b event"))
    for sequence, event in enumerate(events):
        event["sequence"] = sequence
    event_path = root / "events.jsonl"
    event_path.write_text(
        "".join(json.dumps(event, sort_keys=True) + "\n" for event in events),
        encoding="utf-8",
    )
    manifest["events"] = {
        "artifact": "events.jsonl",
        "sha256": verifier.sha256_file(event_path),
        "count": len(events),
    }
    (root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True), encoding="utf-8")


def change_package_identity(root: Path, manifest: dict, events: list[dict], mutation_id: str) -> str:
    old_hash = manifest["package"]["sha256"]
    package = root / manifest["package"]["artifact"]
    package.write_bytes(package.read_bytes() + f"\n4919b:{mutation_id}\n".encode())
    new_hash = verifier.sha256_file(package)
    manifest["package"]["sha256"] = new_hash
    for event in events:
        if event.get("packageSha256") == old_hash:
            event["packageSha256"] = new_hash
    return new_hash


def attempt_for(events: list[dict], spec: Mutation) -> dict:
    if spec.dynamic_unlisted:
        return next(
            event
            for event in events
            if event.get("type") == "dynamic_socket_attempt"
            and event.get("mutationId") == spec.mutation_id
        )
    if spec.lan_protocol:
        return next(
            event
            for event in events
            if event.get("type") == "lan_attempt"
            and event.get("state") == spec.state
            and event.get("protocol") == spec.lan_protocol
            and event.get("operation") == "connection"
        )
    return next(
        event
        for event in events
        if event.get("type") == "positive_attempt"
        and event.get("state") == spec.state
        and event.get("class") == spec.caller_class
    )


def make_mutant(source: Path, target: Path, spec: Mutation) -> str:
    shutil.copytree(source, target)
    manifest = json.loads((target / "manifest.json").read_text())
    events = read_events(target)
    new_package_hash = change_package_identity(target, manifest, events, spec.mutation_id)

    state_start = next(
        event
        for event in events
        if event.get("type") == "lifecycle"
        and event.get("state") == spec.state
        and event.get("phase") == "begin"
    )["atUtc"]
    operation_at = (
        next(
            event["atUtc"]
            for event in events
            if event.get("type") == "route_snapshot" and event.get("state") == spec.state
        )
        if spec.dynamic_unlisted
        else state_start
    )

    if spec.dynamic_unlisted:
        attempt_id = f"dynamic-{spec.mutation_id}"
        caller_id = "caller-dynamic-unlisted"
        image_hash = digest(f"image-{spec.mutation_id}")
        events.extend(
            [
                {
                    "atUtc": operation_at,
                    "type": "dynamic_socket_attempt",
                    "attemptId": attempt_id,
                    "mutationId": spec.mutation_id,
                    "callerId": caller_id,
                    "packageSha256": new_package_hash,
                    "destination": spec.destination,
                },
                {
                    "atUtc": operation_at,
                    "type": "kernel_caller_discovered",
                    "callerId": caller_id,
                    "imageSha256": image_hash,
                    "process": spec.process,
                    "state": spec.state,
                    "destination": spec.destination,
                    "directBytes": spec.byte_count,
                    "dnsBytes": 0,
                    "lanBytes": 0,
                },
                {
                    "atUtc": operation_at,
                    "type": "image_load",
                    "callerId": caller_id,
                    "imageSha256": image_hash,
                },
            ]
        )
    else:
        attempt = attempt_for(events, spec)
        attempt_id = attempt["attemptId"]

    kernel = next(
        (
            event
            for event in events
            if event.get("type") == "kernel_network"
            and event.get("attemptId") == attempt_id
        ),
        None,
    )
    if kernel is None:
        kernel = {
            "atUtc": operation_at,
            "type": "kernel_network",
            "flowId": f"flow-{attempt_id}",
            "attemptId": attempt_id,
        }
        events.append(kernel)
    kernel.update(
        owner="app",
        outsideLoopback=True,
        bytes=spec.byte_count,
        protocol=spec.protocol,
        classification="unclassified" if spec.dynamic_unlisted else "direct_escape",
        authenticated=False,
        process=spec.process,
        **{
            "class": spec.caller_class,
            "state": spec.state,
            "destination": spec.destination,
            "directBytes": spec.byte_count if spec.byte_kind == "directBytes" else 0,
            "dnsBytes": spec.byte_count if spec.byte_kind == "dnsBytes" else 0,
            "lanBytes": spec.byte_count if spec.byte_kind == "lanBytes" else 0,
            "observerIndependent": True,
            "outsideOwnedSidecar": True,
            "outsideVerifiedTorCircuit": True,
        },
    )

    branch = {
        "atUtc": kernel["atUtc"],
        "type": "shipping_branch",
        "mutationId": spec.mutation_id,
        "branchId": f"shipping/{spec.mutation_id}",
        "count": 1,
        "attemptId": attempt_id,
        "packageSha256": new_package_hash,
    }
    endpoint = {
        "atUtc": kernel["atUtc"],
        "type": "external_escape_endpoint",
        "mutationId": spec.mutation_id,
        "attemptId": attempt_id,
        "destination": spec.destination,
        "bytes": spec.byte_count,
        "independent": True,
    }
    events.extend([branch, endpoint])
    if spec.delayed_job:
        events.append(
            {
                "atUtc": kernel["atUtc"],
                "type": "delayed_job",
                "mutationId": spec.mutation_id,
                "attemptId": attempt_id,
                "forced": False,
                "ran": True,
            }
        )

    write_bundle(target, manifest, events)
    return attempt_id


def transition_precedes(events: list[dict], spec: Mutation, attempt_time: datetime) -> bool:
    if spec.required_transition_type == "lifecycle":
        matches = [
            event
            for event in events
            if event.get("type") == "lifecycle"
            and event.get("state") == spec.required_transition
            and event.get("phase") == "end"
        ]
    else:
        matches = [
            event
            for event in events
            if event.get("type") == spec.required_transition_type
            and event.get("transition") == spec.required_transition
        ]
    return bool(matches) and verifier.parse_time(matches[0]["atUtc"], "transition") < attempt_time


def validate_break_proof(root: Path, spec: Mutation, pristine_hash: str) -> dict:
    manifest = json.loads((root / "manifest.json").read_text())
    events = read_events(root)
    package_hash = verifier.sha256_file(root / manifest["package"]["artifact"])
    if package_hash == pristine_hash or package_hash != manifest["package"]["sha256"]:
        raise AssertionError("mutation did not produce a distinct, hash-bound package copy")
    branches = [
        event
        for event in events
        if event.get("type") == "shipping_branch" and event.get("mutationId") == spec.mutation_id
    ]
    if len(branches) != 1 or branches[0].get("count") != 1:
        raise AssertionError("shipping branch counter was starved")
    try:
        attempt = attempt_for(events, spec)
    except StopIteration as exc:
        raise AssertionError("positive attempt was starved") from exc
    attempt_id = attempt.get("attemptId")
    if branches[0].get("attemptId") != attempt_id:
        raise AssertionError("shipping branch did not run the positive attempt")
    kernel = next(
        (
            event
            for event in events
            if event.get("type") == "kernel_network" and event.get("attemptId") == attempt_id
        ),
        None,
    )
    if not kernel or kernel.get("bytes", 0) <= 0:
        raise AssertionError("mutation never sent a positive byte")
    if not all(
        kernel.get(field) is True
        for field in ("observerIndependent", "outsideOwnedSidecar", "outsideVerifiedTorCircuit")
    ):
        raise AssertionError("independent packet/DNS observer was starved")
    if spec.protocol == "dns" and kernel.get("dnsBytes", 0) <= 0:
        raise AssertionError("independent DNS observer was starved")
    attempt_time = verifier.parse_time(kernel["atUtc"], "mutated attempt")
    if not transition_precedes(events, spec, attempt_time):
        raise AssertionError("required lifecycle transition was starved or late")
    capture_end = verifier.parse_time(manifest["captureEndUtc"], "capture end")
    if capture_end <= attempt_time or not manifest["observation"].get("stoppedAfterLastDelayedEvent"):
        raise AssertionError("observation window closed before the escape")
    endpoints = [
        event
        for event in events
        if event.get("type") == "external_escape_endpoint"
        and event.get("mutationId") == spec.mutation_id
        and event.get("attemptId") == attempt_id
        and event.get("independent") is True
    ]
    if len(endpoints) != 1 or endpoints[0].get("bytes", 0) <= 0:
        raise AssertionError("independent external endpoint was starved")
    caller_id = attempt.get("callerId")
    process_seen = any(
        event.get("type") == "kernel_caller_discovered" and event.get("callerId") == caller_id
        for event in events
    )
    if not process_seen:
        raise AssertionError("independent process observer was starved")
    if spec.delayed_job and not any(
        event.get("type") == "delayed_job"
        and event.get("mutationId") == spec.mutation_id
        and event.get("ran") is True
        for event in events
    ):
        raise AssertionError("delayed job was starved")
    return kernel


def semantic_gate_exit(root: Path) -> tuple[int, str]:
    try:
        verifier.verify_bundle(root, require_production=False)
    except verifier.VerificationError as exc:
        return 1, f"TOR-4919-RED: {exc}"
    return 0, "TASK4919B_SEMANTIC_MODEL_ACCEPTED"


def validate_restored_control(root: Path) -> dict:
    result = verifier.verify_bundle(root, require_production=False)
    if result["positive_attempts"] != 120 or result["lan_attempts"] != 72:
        raise AssertionError("restored control starved required attempts")
    events = read_events(root)
    for spec in MUTATIONS:
        if spec.dynamic_unlisted:
            continue
        attempt = attempt_for(events, spec)
        kernel = next(
            event
            for event in events
            if event.get("type") == "kernel_network"
            and event.get("attemptId") == attempt.get("attemptId")
        )
        if spec.lan_protocol:
            if (
                attempt.get("refused") is not True
                or attempt.get("bytes") != 0
                or attempt.get("packetCount") != 0
                or attempt.get("copy") != verifier.EXACT_LAN_COPY
                or kernel.get("bytes") != 0
                or kernel.get("outsideLoopback") is not False
            ):
                raise AssertionError(f"restored 4916 LAN refusal failed: {spec.mutation_id}")
        else:
            endpoint = next(
                event
                for event in events
                if event.get("type") == "independent_endpoint"
                and event.get("attemptId") == attempt.get("attemptId")
            )
            if (
                kernel.get("classification") != "owned_sidecar"
                or kernel.get("outsideLoopback") is not False
                or kernel.get("authenticated") is not True
                or endpoint.get("publicTorExitObserved") is not True
            ):
                raise AssertionError(f"restored Tor route failed: {spec.mutation_id}")
    return result


class Task4919bMutationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="task4919b-")
        self.root = Path(self.temp.name)
        self.pristine = self.root / "pristine"
        self.pristine.mkdir()
        ReceiptFixture(self.pristine)
        self.pristine_hash = json.loads((self.pristine / "manifest.json").read_text())["package"]["sha256"]

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_each_named_mutation_is_isolated_nonvacuous_and_exactly_named(self) -> None:
        package_hashes: set[str] = set()
        for spec in MUTATIONS:
            with self.subTest(mutation=spec.mutation_id):
                with tempfile.TemporaryDirectory(prefix=f"copy-{spec.mutation_id}-", dir=self.root) as copy_dir:
                    copy_root = Path(copy_dir) / "package"
                    make_mutant(self.pristine, copy_root, spec)
                    kernel = validate_break_proof(copy_root, spec, self.pristine_hash)
                    package_hashes.add(json.loads((copy_root / "manifest.json").read_text())["package"]["sha256"])
                    exit_code, diagnostic = semantic_gate_exit(copy_root)
                    self.assertEqual(exit_code, 1)
                    for exact in (
                        f"process={spec.process}",
                        f"class={spec.caller_class}",
                        f"lifecycle_state={spec.state}",
                        f"destination={spec.destination}",
                        f"direct_bytes={kernel['directBytes']}",
                        f"dns_bytes={kernel['dnsBytes']}",
                        f"lan_bytes={kernel['lanBytes']}",
                    ):
                        self.assertIn(exact, diagnostic)
                    if spec.dynamic_unlisted:
                        self.assertIn("unclassified dynamically loaded socket caller", diagnostic)
                    print(
                        f"TASK4919B_MUTATION mutation={spec.mutation_id} branch_count=1 "
                        f"positive_bytes={spec.byte_count} verifier_exit={exit_code} {diagnostic}"
                    )
                self.assertFalse(copy_root.exists(), "throwaway packaged copy was not discarded")
        self.assertEqual(len(package_hashes), len(MUTATIONS))
        print(
            f"TASK4919B_PACKAGED_COPIES={len(MUTATIONS)} "
            f"unique_package_hashes={len(package_hashes)} discarded={len(MUTATIONS)}"
        )

    def test_starving_any_break_proof_input_is_rejected(self) -> None:
        spec = MUTATIONS[0]
        cases = {
            "branch_counter": lambda events, manifest: next(
                event for event in events if event.get("type") == "shipping_branch"
            ).update(count=0),
            "positive_attempt": lambda events, manifest: events.remove(attempt_for(events, spec)),
            "transition": lambda events, manifest: events.remove(
                next(event for event in events if event.get("type") == "power_transition" and event.get("transition") == "wake")
            ),
            "external_endpoint": lambda events, manifest: events.remove(
                next(event for event in events if event.get("type") == "external_escape_endpoint")
            ),
            "packet_dns_observer": lambda events, manifest: next(
                event for event in events if event.get("type") == "kernel_network" and event.get("dnsBytes") == spec.byte_count
            ).update(observerIndependent=False),
            "process_observer": lambda events, manifest: events.remove(
                next(event for event in events if event.get("type") == "kernel_caller_discovered" and event.get("callerId") == "caller-dns")
            ),
            "positive_send": lambda events, manifest: next(
                event for event in events if event.get("type") == "kernel_network" and event.get("dnsBytes") == spec.byte_count
            ).update(bytes=0, dnsBytes=0),
            "observation_tail": lambda events, manifest: manifest["observation"].update(stoppedAfterLastDelayedEvent=False),
        }
        for name, starve in cases.items():
            with self.subTest(starved=name):
                target = self.root / f"starved-{name}"
                make_mutant(self.pristine, target, spec)
                manifest = json.loads((target / "manifest.json").read_text())
                events = read_events(target)
                starve(events, manifest)
                write_bundle(target, manifest, events)
                with self.assertRaises(AssertionError):
                    validate_break_proof(target, spec, self.pristine_hash)

        delayed = MUTATIONS[5]
        target = self.root / "starved-delayed-job"
        make_mutant(self.pristine, target, delayed)
        manifest = json.loads((target / "manifest.json").read_text())
        events = [event for event in read_events(target) if event.get("mutationId") != delayed.mutation_id or event.get("type") != "delayed_job"]
        write_bundle(target, manifest, events)
        with self.assertRaisesRegex(AssertionError, "delayed job"):
            validate_break_proof(target, delayed, self.pristine_hash)

        restored = self.root / "starved-restored-control"
        shutil.copytree(self.pristine, restored)
        manifest = json.loads((restored / "manifest.json").read_text())
        events = read_events(restored)
        events.remove(attempt_for(events, MUTATIONS[0]))
        write_bundle(restored, manifest, events)
        with self.assertRaises(verifier.VerificationError):
            validate_restored_control(restored)
        print("TASK4919B_STARVATION_CASES=10 all_rejected=true")

    def test_restored_immutable_model_runs_every_attempt_but_cannot_claim_release_green(self) -> None:
        result = validate_restored_control(self.pristine)
        self.assertEqual(result["unclassified_or_escaped_writes"], 0)
        self.assertEqual(result["control_tcp_packets"], 1)
        self.assertEqual(result["control_udp_packets"], 1)
        exit_code, diagnostic = semantic_gate_exit(self.pristine)
        self.assertEqual(exit_code, 0)
        self.assertNotIn("TOR-4919-GREEN", diagnostic)
        print(
            "TASK4919B_RESTORED_MODEL restored_shipping_attempts=8 "
            "positive_attempts=120 lan_refusals=72 "
            "escaped_bytes=0 control_tcp_packets=1 control_udp_packets=1 "
            "production_green=forbidden"
        )

    def test_production_cli_still_refuses_the_unit_package(self) -> None:
        self.assertEqual(verifier.main(["--bundle", str(self.pristine)]), 1)


if __name__ == "__main__":
    unittest.main()
