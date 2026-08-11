#!/usr/bin/env python3

from __future__ import annotations

import contextlib
import copy
import hashlib
import io
import json
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import task4919_verify as verifier


def digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def stamp(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


class ReceiptFixture:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.package_hash = digest("one immutable release-packaged Windows build")
        self.start = datetime(2026, 1, 1, tzinfo=timezone.utc)
        cursor = self.start
        self.ranges: dict[str, tuple[datetime, datetime]] = {}
        for state in verifier.LIFECYCLE_STATES:
            duration = timedelta(hours=24) if state == "idle_24h" else timedelta(seconds=30)
            self.ranges[state] = (cursor, cursor + duration)
            cursor += duration
        self.end = cursor + timedelta(seconds=30)
        self.events: list[dict] = []
        self.inventory: list[dict] = []
        self.manifest: dict = {}
        self._build()

    def add(self, at: datetime, event_type: str, **fields: object) -> None:
        self.events.append({"atUtc": stamp(at), "type": event_type, **fields})

    def _build(self) -> None:
        package_path = self.root / "osl-release.msix"
        package_path.write_text(
            "one immutable release-packaged Windows build", encoding="utf-8"
        )
        collector_dir = self.root / "raw"
        collector_dir.mkdir(parents=True)
        collectors = []
        for collector_id in sorted(verifier.REQUIRED_COLLECTORS):
            path = collector_dir / f"{collector_id}.etl"
            path.write_text(f"independent raw {collector_id}\n", encoding="utf-8")
            collectors.append(
                {
                    "id": collector_id,
                    "independentOfApp": True,
                    "continuous": True,
                    "artifact": str(path.relative_to(self.root)),
                    "sha256": verifier.sha256_file(path),
                }
            )

        for caller_class in verifier.NON_LAN_CLASSES:
            caller_id = f"caller-{caller_class}"
            self.inventory.append(
                {
                    "callerId": caller_id,
                    "class": caller_class,
                    "process": "osl-privacy-hub.exe",
                    "ownedByOsl": True,
                    "imageSha256": digest(f"image-{caller_id}"),
                }
            )
        for protocol, operation in verifier.LAN_OPERATIONS:
            caller_id = f"caller-lan-{protocol}-{operation}"
            self.inventory.append(
                {
                    "callerId": caller_id,
                    "class": "osl_lan",
                    "protocol": protocol,
                    "operation": operation,
                    "process": "osl-privacy-hub.exe",
                    "ownedByOsl": True,
                    "imageSha256": digest(f"image-{caller_id}"),
                }
            )

        inventory_path = self.root / "independent-inventory.json"
        inventory_path.write_text(json.dumps(self.inventory, sort_keys=True), encoding="utf-8")

        # Collector continuity is intentionally dense: a declared continuous
        # observer with an hour-long hole is rejected by the verifier.
        heartbeat = self.start
        while heartbeat <= self.end:
            self.add(
                heartbeat,
                "observer_heartbeat",
                collectors=sorted(verifier.REQUIRED_COLLECTORS),
            )
            heartbeat += timedelta(seconds=60)

        for state, (begin, finish) in self.ranges.items():
            self.add(begin, "lifecycle", state=state, phase="begin")
            self.add(finish, "lifecycle", state=state, phase="end")

        onboarding = self.ranges["fresh_onboarding"][0] + timedelta(seconds=1)
        for row in self.inventory:
            common = {
                "callerId": row["callerId"],
                "imageSha256": row["imageSha256"],
            }
            self.add(onboarding, "kernel_caller_discovered", **common)
            self.add(onboarding, "image_load", **common)

        subjects = [
            {"subjectId": "hub", "kind": "process"},
            {"subjectId": "sidecar", "kind": "child"},
            {"subjectId": "update-service", "kind": "service"},
            {"subjectId": "delayed-poll", "kind": "scheduled_job"},
        ]
        for subject in subjects:
            self.add(onboarding, "kernel_process_discovered", **subject)

        auth = digest("launch-bound-random-authentication")
        self.add(
            onboarding,
            "sidecar_launch",
            sidecarId="sidecar-1",
            authBindingSha256=auth,
            ownedByOsl=True,
            packageSha256=self.package_hash,
        )
        self.add(
            onboarding,
            "tor_circuit",
            circuitId="circuit-1",
            sidecarId="sidecar-1",
            verifiedPublicTor=True,
            independent=True,
            relays=["guard", "middle", "exit"],
        )
        recovery_at = self.ranges["sidecar_recovery"][0] + timedelta(seconds=1)
        self.add(
            recovery_at,
            "sidecar_launch",
            sidecarId="sidecar-2",
            authBindingSha256=auth,
            ownedByOsl=True,
            packageSha256=self.package_hash,
        )
        self.add(
            recovery_at,
            "tor_circuit",
            circuitId="circuit-2",
            sidecarId="sidecar-2",
            verifiedPublicTor=True,
            independent=True,
            relays=["guard-2", "middle-2", "exit-2"],
        )

        self.add(
            self.ranges["sleep_wake"][0] + timedelta(seconds=1),
            "power_transition",
            transition="sleep",
        )
        self.add(
            self.ranges["sleep_wake"][0] + timedelta(seconds=2),
            "power_transition",
            transition="wake",
        )
        self.add(
            self.ranges["network_interface_change"][0] + timedelta(seconds=1),
            "network_transition",
            transition="interface",
        )
        self.add(
            self.ranges["address_change"][0] + timedelta(seconds=1),
            "network_transition",
            transition="address",
        )
        self.add(
            self.ranges["sidecar_death"][0] + timedelta(seconds=1),
            "sidecar_transition",
            transition="death",
            sidecarId="sidecar-1",
        )
        self.add(
            recovery_at,
            "sidecar_transition",
            transition="recovery",
            sidecarId="sidecar-2",
        )
        self.add(
            self.ranges["app_restart"][0] + timedelta(seconds=1),
            "process_transition",
            transition="app_restart",
        )
        self.add(
            self.ranges["os_restart"][0] + timedelta(seconds=1),
            "process_transition",
            transition="os_restart",
        )
        self.add(
            self.ranges["packaged_component_update"][0] + timedelta(seconds=1),
            "component_update",
            packageSha256=self.package_hash,
            oldComponentSha256=digest("component-v1"),
            newComponentSha256=digest("component-v2"),
        )
        self.add(
            self.ranges["forced_delayed_background_jobs"][0] + timedelta(seconds=1),
            "delayed_job",
            forced=True,
        )

        for state in verifier.ACTIVE_STATES:
            at = self.ranges[state][0] + (
                timedelta(milliseconds=500)
                if state == "sidecar_death"
                else timedelta(seconds=3)
            )
            self.add(at, "route_snapshot", state=state, torSelected=True, oslLanEnabled=False)
            sidecar_id = "sidecar-2" if state in verifier.ACTIVE_STATES[8:] else "sidecar-1"
            circuit_id = "circuit-2" if sidecar_id == "sidecar-2" else "circuit-1"
            for caller_class in verifier.NON_LAN_CLASSES:
                attempt_id = f"positive-{state}-{caller_class}"
                request_hash = digest(f"request-{attempt_id}")
                response_hash = digest(f"response-{attempt_id}")
                self.add(
                    at,
                    "positive_attempt",
                    attemptId=attempt_id,
                    state=state,
                    **{"class": caller_class},
                    callerId=f"caller-{caller_class}",
                    packageSha256=self.package_hash,
                    authBindingSha256=auth,
                    sidecarId=sidecar_id,
                    circuitId=circuit_id,
                    requestSha256=request_hash,
                    responseSha256=response_hash,
                )
                self.add(
                    at,
                    "independent_endpoint",
                    attemptId=attempt_id,
                    independent=True,
                    publicTorExitObserved=True,
                    requestSha256=request_hash,
                    responseSha256=response_hash,
                )
                self.add(
                    at,
                    "kernel_network",
                    flowId=f"flow-{attempt_id}",
                    attemptId=attempt_id,
                    owner="app",
                    outsideLoopback=False,
                    bytes=128,
                    protocol="tcp",
                    classification="owned_sidecar",
                    authenticated=True,
                )
            self.add(
                at,
                "kernel_network",
                flowId=f"flow-sidecar-{state}",
                owner="sidecar",
                outsideLoopback=True,
                bytes=512,
                protocol="tcp",
                classification="tor_relay",
            )
            for protocol, operation in verifier.LAN_OPERATIONS:
                attempt_id = f"lan-{state}-{protocol}-{operation}"
                self.add(
                    at,
                    "lan_attempt",
                    attemptId=attempt_id,
                    state=state,
                    protocol=protocol,
                    operation=operation,
                    callerId=f"caller-lan-{protocol}-{operation}",
                    refused=True,
                    packetCount=0,
                    bytes=0,
                    copy=verifier.EXACT_LAN_COPY,
                )
                self.add(
                    at,
                    "uia_text",
                    attemptId=attempt_id,
                    text=verifier.EXACT_LAN_COPY,
                    top=100,
                )
                self.add(
                    at,
                    "kernel_network",
                    flowId=f"flow-{attempt_id}",
                    attemptId=attempt_id,
                    owner="app",
                    outsideLoopback=False,
                    bytes=0,
                    protocol=protocol,
                    classification="lan_refused",
                )

        choice_at = self.ranges["fresh_onboarding"][0] + timedelta(seconds=2)
        self.add(
            choice_at,
            "uia_text",
            location="route_lan_choice",
            text=verifier.EXACT_LAN_COPY,
            top=80,
        )

        control_at = self.ranges["packaged_component_update"][1] + timedelta(seconds=1)
        controls = [
            ("tor_off", False, False, {}),
            ("lan_on", False, True, {}),
            ("lan_tcp_marked", False, True, {"protocol": "tcp", "packets": 1, "bytes": 32}),
            ("lan_udp_marked", False, True, {"protocol": "udp", "packets": 1, "bytes": 32}),
            ("lan_off", False, False, {}),
            ("tor_on", True, False, {}),
            (
                "tor_marked",
                True,
                False,
                {"throughAuthenticatedSidecar": True, "verifiedPublicTor": True},
            ),
            ("tor_off_final", False, False, {}),
            ("direct_marked", False, False, {"directBytes": 32}),
        ]
        for offset, (step, tor, lan, extra) in enumerate(controls):
            self.add(
                control_at + timedelta(seconds=offset),
                "off_tor_control",
                step=step,
                torSelected=tor,
                oslLanEnabled=lan,
                packageSha256=self.package_hash,
                **extra,
            )

        self.manifest = {
            "schema": "osl-task-4919-receipt/v1",
            "receiptKind": "unit-fixture/v1",
            "fixture": True,
            "platform": {"os": "Windows", "kernel": "NT", "machineId": "fixture-machine"},
            "package": {
                "sha256": self.package_hash,
                "artifact": "osl-release.msix",
                "releasePackaged": True,
                "immutable": True,
                "format": "msix",
                "windowsAuthenticodeVerified": True,
            },
            "captureStartUtc": stamp(self.start),
            "captureEndUtc": stamp(self.end),
            "observation": {
                "continuous": True,
                "gaps": [],
                "startedBeforeLaunch": True,
                "stoppedAfterLastDelayedEvent": True,
            },
            "collectors": collectors,
            "inventory": {
                "artifact": "independent-inventory.json",
                "sha256": verifier.sha256_file(inventory_path),
                "generator": "independent-windows-pe-import-symbol-runtime-scan/v1",
                "handEdited": False,
                "shippingCallerCount": len(self.inventory),
            },
            "ownedSubjects": subjects,
            "launchAuthBindingSha256": auth,
        }
        self.write()

    def write(self) -> None:
        self.events.sort(
            key=lambda event: verifier.parse_time(event["atUtc"], "fixture event time")
        )
        for sequence, event in enumerate(self.events):
            event["sequence"] = sequence
        events_path = self.root / "events.jsonl"
        events_path.write_text(
            "".join(json.dumps(event, sort_keys=True) + "\n" for event in self.events),
            encoding="utf-8",
        )
        self.manifest["events"] = {
            "artifact": "events.jsonl",
            "sha256": verifier.sha256_file(events_path),
            "count": len(self.events),
        }
        (self.root / "manifest.json").write_text(
            json.dumps(self.manifest, sort_keys=True), encoding="utf-8"
        )


class Task4919VerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="task4919-test-")
        self.root = Path(self.temp.name)
        self.fixture = ReceiptFixture(self.root)
        self.original_events = copy.deepcopy(self.fixture.events)
        self.original_manifest = copy.deepcopy(self.fixture.manifest)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def reset(self) -> None:
        self.fixture.events = copy.deepcopy(self.original_events)
        self.fixture.manifest = copy.deepcopy(self.original_manifest)

    def assert_red(self, expected: str) -> None:
        self.fixture.write()
        with self.assertRaisesRegex(verifier.VerificationError, expected):
            verifier.verify_bundle(self.root, require_production=False)

    def test_complete_unit_model_reaches_every_required_count_without_release_green(self) -> None:
        result = verifier.verify_bundle(self.root, require_production=False)
        self.assertEqual(result["states"], 13)
        self.assertEqual(result["active_states"], 12)
        self.assertEqual(result["idle_seconds"], 86_400)
        self.assertEqual(result["collectors"], 9)
        self.assertEqual(result["callers"], 16)
        self.assertEqual(result["subjects"], 4)
        self.assertEqual(result["positive_attempts"], 120)
        self.assertEqual(result["lan_attempts"], 72)
        self.assertEqual(result["uia_copy_observations"], 73)
        self.assertEqual(result["unclassified_or_escaped_writes"], 0)

        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            exit_code = verifier.main(["--bundle", str(self.root)])
        self.assertEqual(exit_code, 1)
        self.assertNotIn("TOR-4919-GREEN", stdout.getvalue())
        self.assertIn("receiptKind", stderr.getvalue())

    def test_every_lifecycle_state_is_individually_mandatory(self) -> None:
        for state in verifier.LIFECYCLE_STATES:
            with self.subTest(state=state):
                self.reset()
                self.fixture.events = [
                    event
                    for event in self.fixture.events
                    if not (
                        event.get("type") == "lifecycle"
                        and event.get("state") == state
                        and event.get("phase") == "begin"
                    )
                ]
                self.assert_red(state)

    def test_every_non_lan_class_is_positive_in_every_active_state(self) -> None:
        for caller_class in verifier.NON_LAN_CLASSES:
            with self.subTest(caller_class=caller_class):
                self.reset()
                self.fixture.events = [
                    event
                    for event in self.fixture.events
                    if not (
                        event.get("type") == "positive_attempt"
                        and event.get("state") == "ordinary_use"
                        and event.get("class") == caller_class
                    )
                ]
                self.assert_red(f"state=ordinary_use class={caller_class}")

    def test_every_tcp_udp_lan_operation_is_mandatory(self) -> None:
        for protocol, operation in verifier.LAN_OPERATIONS:
            with self.subTest(protocol=protocol, operation=operation):
                self.reset()
                self.fixture.events = [
                    event
                    for event in self.fixture.events
                    if not (
                        event.get("type") == "lan_attempt"
                        and event.get("state") == "ordinary_use"
                        and event.get("protocol") == protocol
                        and event.get("operation") == operation
                    )
                ]
                self.assert_red(f"{protocol}/{operation}")

    def test_visible_warning_never_authorizes_lan_bypass(self) -> None:
        attempt = next(event for event in self.fixture.events if event.get("type") == "lan_attempt")
        attempt["refused"] = False
        self.assert_red("not refused")

    def test_one_lan_packet_names_the_escaped_byte(self) -> None:
        attempt = next(event for event in self.fixture.events if event.get("type") == "lan_attempt")
        attempt["packetCount"] = 1
        attempt["bytes"] = 16
        self.assert_red("escaped LAN byte")

    def test_exact_copy_and_above_zero_uia_are_mandatory(self) -> None:
        row = next(
            event
            for event in self.fixture.events
            if event.get("type") == "uia_text" and event.get("attemptId")
        )
        row["top"] = 0
        self.assert_red("above-0 UIA text")

    def test_simultaneous_tor_and_lan_is_rejected(self) -> None:
        route = next(event for event in self.fixture.events if event.get("type") == "route_snapshot")
        route["oslLanEnabled"] = True
        self.assert_red("Tor and OSL LAN together")

    def test_direct_dns_and_unclassified_socket_are_each_rejected(self) -> None:
        network = next(
            event
            for event in self.fixture.events
            if event.get("type") == "kernel_network" and event.get("owner") == "app"
        )
        network.update(outsideLoopback=True, protocol="dns", bytes=1, classification="direct_dns")
        self.assert_red("direct DNS byte escaped Tor")

        self.reset()
        network = next(event for event in self.fixture.events if event.get("type") == "kernel_network")
        network["classification"] = "unclassified"
        self.assert_red("unclassified socket")

    def test_request_response_hashes_must_match_independent_endpoint(self) -> None:
        endpoint = next(
            event for event in self.fixture.events if event.get("type") == "independent_endpoint"
        )
        endpoint["responseSha256"] = digest("wrong response")
        self.assert_red("request/response hash mismatch")

    def test_inventory_and_newly_loaded_callers_are_reconciled(self) -> None:
        discovered = next(
            event for event in self.fixture.events if event.get("type") == "kernel_caller_discovered"
        )
        self.fixture.events.remove(discovered)
        self.assert_red("kernel caller reconciliation failed")

        self.reset()
        loaded = next(event for event in self.fixture.events if event.get("type") == "image_load")
        self.fixture.events.remove(loaded)
        self.assert_red("newly loaded caller reconciliation failed")

    def test_process_child_service_and_job_census_cannot_be_starved(self) -> None:
        self.fixture.manifest["ownedSubjects"] = self.fixture.manifest["ownedSubjects"][:-1]
        self.assert_red("scheduled-job census is incomplete")

    def test_sidecar_authentication_and_public_circuit_are_required(self) -> None:
        attempt = next(
            event for event in self.fixture.events if event.get("type") == "positive_attempt"
        )
        attempt["authBindingSha256"] = digest("wrong launch")
        self.assert_red("bypassed launch-bound authenticated sidecar")

        self.reset()
        circuit = next(event for event in self.fixture.events if event.get("type") == "tor_circuit")
        circuit["verifiedPublicTor"] = False
        for event in self.fixture.events:
            if event.get("type") == "positive_attempt" and event.get("circuitId") == circuit["circuitId"]:
                event["circuitId"] = "missing-circuit"
        self.assert_red("lacks verified public Tor circuit")

    def test_observation_must_cover_24h_and_continue_after_delayed_event(self) -> None:
        idle_end = next(
            event
            for event in self.fixture.events
            if event.get("type") == "lifecycle"
            and event.get("state") == "idle_24h"
            and event.get("phase") == "end"
        )
        idle_end["atUtc"] = stamp(self.fixture.ranges["idle_24h"][0] + timedelta(hours=23))
        self.assert_red("idle_24h")

        self.reset()
        self.fixture.manifest["observation"]["stoppedAfterLastDelayedEvent"] = False
        self.assert_red("stopped before the last delayed event")

    def test_each_independent_collector_and_heartbeat_are_required(self) -> None:
        self.fixture.manifest["collectors"] = self.fixture.manifest["collectors"][:-1]
        self.assert_red("missing kernel observer/collector")

        self.reset()
        heartbeat = next(
            event for event in self.fixture.events if event.get("type") == "observer_heartbeat"
        )
        heartbeat["collectors"] = heartbeat["collectors"][:-1]
        self.assert_red("heartbeat starved")

    def test_same_build_off_tor_tcp_udp_and_return_controls_are_mandatory(self) -> None:
        self.fixture.events = [
            event
            for event in self.fixture.events
            if not (
                event.get("type") == "off_tor_control"
                and event.get("step") == "lan_udp_marked"
            )
        ]
        self.assert_red("control sequence")

        self.reset()
        direct = next(
            event
            for event in self.fixture.events
            if event.get("type") == "off_tor_control"
            and event.get("step") == "direct_marked"
        )
        direct["directBytes"] = 0
        self.assert_red("marked direct traffic")

    def test_component_update_and_immutable_package_are_both_bound(self) -> None:
        update = next(event for event in self.fixture.events if event.get("type") == "component_update")
        update["packageSha256"] = digest("other package")
        self.assert_red("changed the immutable package identity")

    def test_artifact_tamper_is_rejected_before_semantic_verification(self) -> None:
        (self.root / "raw" / "windows_kernel_socket.etl").write_text(
            "tampered\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(verifier.VerificationError, "artifact hash mismatch"):
            verifier.verify_bundle(self.root, require_production=False)


if __name__ == "__main__":
    unittest.main()
