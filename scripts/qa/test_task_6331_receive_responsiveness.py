from __future__ import annotations

import copy
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from task_6331_receive_responsiveness import (
    ReconciliationError,
    _elapsed_ms_ceiling,
    reconcile,
)


FREQUENCY = 1_000
START = 10_000
END = START + 300_000
SURFACE = {
    "hwnd": "0x6331",
    "process_id": 63310,
    "thread_id": 63311,
    "runtime_id": [42, 6331, 7],
    "generation": 1,
    "automation_id": "osl-protected-receive-surface",
    "name": "Messages prepared or opened in this OSL panel",
}


def look_offsets() -> list[int]:
    result: list[int] = []
    at = 0
    for index in range(50):
        result.append(at)
        at += 4_000 if index % 2 == 0 else 8_000
    return result


def build_pair(run: str) -> tuple[dict, dict]:
    probes = []
    for index, offset in enumerate(range(0, 300_000, 50)):
        kind = "pointer" if index % 2 == 0 else "keyboard"
        sent = START + offset
        response = sent + 10
        input_id = f"{run}-input-{index:04d}"
        probes.append({
            "input_id": input_id,
            "kind": kind,
            "target_surface": copy.deepcopy(SURFACE),
            "injection_api": "SendInput",
            "direct_invocation": False,
            "sent_qpc": sent,
            "response_qpc": response,
            "latency_ms": 10,
            "completed_gap_ms": None if index == 0 else 50,
            "os_hook": {
                "input_id": input_id,
                "qpc": sent + 1,
                "hook": "WH_MOUSE_LL" if kind == "pointer" else "WH_KEYBOARD_LL",
                "api": "SendInput",
                "injected_flag": "LLMHF_INJECTED" if kind == "pointer" else "LLKHF_INJECTED",
            },
            "visible_response": {
                "kind": "pixel" if kind == "pointer" else "scroll",
                "changed": True,
                "qpc": response,
                "surface": copy.deepcopy(SURFACE),
            },
        })

    offsets = [index * 6_000 for index in range(50)] if run == "quiet" else look_offsets()
    markers = []
    changes = []
    looks = []
    reads = []
    for index, offset in enumerate(offsets):
        provider = "Discord" if index % 2 == 0 else "WhatsApp"
        provider_id = f"{run}-provider-{index:03d}"
        look_id = f"{run}-look-{index:03d}"
        qpc = START + offset
        markers.append({
            "provider_id": provider_id,
            "provider": provider,
            "marker_text": f"TASK6331 {run} marker {index:03d}",
            "scheduled_offset_ms": offset,
            "scheduled_qpc": qpc,
            "provider_qpc": qpc,
        })
        witness_index = (offset + 100) // 50
        changes.append({
            "provider_id": provider_id,
            "marker_text": f"TASK6331 {run} marker {index:03d}",
            "change_kind": "row_addition",
            "row_runtime_id": f"row-{run}-{index:03d}",
            "surface": copy.deepcopy(SURFACE),
            "observed_qpc": qpc + 100,
            "latency_ms": 100,
            "witness_input_id": f"{run}-input-{witness_index:04d}",
        })
        looks.append({"look_id": look_id, "provider_id": provider_id, "provider": provider, "qpc": qpc, "surface": copy.deepcopy(SURFACE)})
        reads.append({"read_id": f"read-{index:03d}", "look_id": look_id, "provider_id": provider_id, "provider": provider, "qpc": qpc, "surface": copy.deepcopy(SURFACE)})

    ordinary = []
    if run == "busy":
        for index in range(200):
            offset = index * 1_500
            ordinary.append({
                "provider_id": f"busy-ordinary-{index:03d}",
                "scheduled_offset_ms": offset,
                "scheduled_qpc": START + offset,
                "provider_qpc": START + offset,
            })

    observer = {
        "schema": "osl-task-6331-observer-v1",
        "run": run,
        "observer_process": {"pid": 63312 if run == "quiet" else 63313, "path": "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe", "role": "outside-windows-observer"},
        "surface": copy.deepcopy(SURFACE),
        "clock": {"qpc_frequency": FREQUENCY, "run_start_qpc": START, "run_end_qpc": END},
        "identity_resolutions": [{"qpc": START - 1, "reason": "pre_run", "surface": copy.deepcopy(SURFACE)}],
        "destroyed_runtime_ids": [],
        "probes": probes,
        "probe_count": len(probes),
        "longest_latency_ms": 10,
        "longest_completed_gap_ms": 50,
        "content_changes": changes,
        "content_change_count": len(changes),
        "outside_interval_stalls": [],
    }
    metrics = {
        "protected_marker_count": 50,
        "look_count": 50,
        "carrier_screen_read_count": 50,
        "discord_closest_read_ms": 12_000,
        "whatsapp_closest_read_ms": 12_000,
    }
    if run == "busy":
        metrics.update({"ordinary_message_count": 200, "closest_look_ms": 4_000, "quiet_look_count": 50})
    workload = {
        "schema": "osl-task-6331-workload-v1",
        "run": run,
        "surface": copy.deepcopy(SURFACE),
        "clock": {"qpc_frequency": FREQUENCY, "run_start_qpc": START, "run_end_qpc": END},
        "osl_process_id": SURFACE["process_id"],
        "harness_process_id": 70001,
        "workload_process_id": 70002,
        "provider_process_ids": [70003, 70004],
        "protected_markers": markers,
        "ordinary_messages": ordinary,
        "looks": looks,
        "carrier_reads": reads,
        "metrics": metrics,
    }
    return observer, workload


class Task6331ReconcilerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.quiet_observer, self.quiet_workload = build_pair("quiet")
        self.busy_observer, self.busy_workload = build_pair("busy")

    def assertFails(self, needle: str) -> None:
        with self.assertRaisesRegex(ReconciliationError, needle):
            reconcile(self.quiet_observer, self.quiet_workload, self.busy_observer, self.busy_workload)

    def test_full_virtual_five_minute_logs_pass(self) -> None:
        result = reconcile(self.quiet_observer, self.quiet_workload, self.busy_observer, self.busy_workload)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual((result["quiet"]["probes"], result["busy"]["probes"]), (6000, 6000))
        self.assertEqual((result["quiet"]["markers"], result["busy"]["ordinary_messages"]), (50, 200))

    def test_qpc_elapsed_values_are_conservative_and_need_not_be_whole_milliseconds(self) -> None:
        self.assertEqual(_elapsed_ms_ceiling(1, 10_000_000), 1)
        self.assertEqual(_elapsed_ms_ceiling(10_000_001, 10_000_000), 1_001)
        with self.assertRaisesRegex(ReconciliationError, "negative QPC delta"):
            _elapsed_ms_ceiling(-1, 10_000_000)

    def test_windows_observer_uses_sendinput_hooks_and_read_only_uia(self) -> None:
        source_path = Path(__file__).with_name("task-6331-receive-responsiveness.ps1")
        source = source_path.read_text(encoding="utf-8-sig")
        for required in (
            "SendInput(",
            "SetWindowsHookEx(13",
            "SetWindowsHookEx(14",
            "WH_KEYBOARD_LL",
            "WH_MOUSE_LL",
            "AutomationIdProperty",
            "osl-protected-receive-surface",
            "QueryPerformanceCounter",
        ):
            self.assertIn(required, source, f"Windows observer lacks {required}")
        forbidden = {
            "PostMessage": r"\bPostMessage\s*\(",
            "SendKeys": r"\bSendKeys\b",
            "SetFocus": r"\.SetFocus\s*\(",
            "InvokePattern": r"\bInvokePattern\b",
            "ScrollPattern.Scroll": r"\.Scroll\s*\(",
            "test-hook": r"(?i)(?:test[-_ ]hook|testHook\s*=\s*\$true)",
        }
        for label, pattern in forbidden.items():
            self.assertIsNone(re.search(pattern, source), f"Windows observer contains forbidden {label}")

    def test_empty_probe_and_marker_sets_fail(self) -> None:
        self.quiet_observer["probes"] = []
        self.assertFails("empty probe set")
        self.setUp()
        self.quiet_workload["protected_markers"] = []
        self.assertFails("protected markers count=0")

    def test_quiet_and_busy_501ms_surface_only_stalls_fail_on_outside_latency(self) -> None:
        for run in ("quiet", "busy"):
            self.setUp()
            observer = self.quiet_observer if run == "quiet" else self.busy_observer
            probe = observer["probes"][100]
            probe["response_qpc"] = probe["sent_qpc"] + 501
            probe["visible_response"]["qpc"] = probe["response_qpc"]
            probe["latency_ms"] = 501
            probe["os_hook"]["qpc"] = probe["sent_qpc"] + 1
            self.assertFails(rf"run={run}.*input_id={run}-input-0100")

    def test_missing_content_update_names_provider(self) -> None:
        missing = self.busy_observer["content_changes"].pop(17)
        self.busy_observer["content_change_count"] -= 1
        self.assertFails(rf"provider_id={missing['provider_id']}")

    def test_removed_probe_breaks_uninterrupted_stream(self) -> None:
        self.quiet_observer["probes"].pop(20)
        self.quiet_observer["probe_count"] -= 1
        self.assertFails("probe streams are not strictly alternating|missing pointer interval")

    def test_direct_uia_or_test_hook_input_fails(self) -> None:
        probe = self.quiet_observer["probes"][9]
        probe["injection_api"] = "UIAutomation.InvokePattern"
        probe["direct_invocation"] = True
        self.assertFails("synthetic/UIA/test-hook/direct invocation")

    def test_probe_at_other_osl_control_fails(self) -> None:
        self.busy_observer["probes"][33]["target_surface"]["runtime_id"] = [99, 1]
        self.assertFails("probe targeted another OSL control")

    def test_sacrificial_control_cannot_be_bound_as_receive_surface(self) -> None:
        for record in (self.quiet_observer, self.quiet_workload):
            record["surface"]["automation_id"] = "responsive-sacrificial-button"
            record["surface"]["name"] = "Still responsive"
        self.assertFails("names another OSL control")

    def test_stale_recreated_and_destroyed_identity_fail(self) -> None:
        changed = copy.deepcopy(SURFACE)
        changed["generation"] = 2
        self.quiet_observer["identity_resolutions"].append({"qpc": START + 1000, "surface": changed})
        self.assertFails("stale/recreated or different control identity")
        self.setUp()
        self.busy_observer["destroyed_runtime_ids"] = [copy.deepcopy(SURFACE["runtime_id"])]
        self.assertFails("identity was destroyed or reused")

    def test_observer_inside_osl_harness_or_provider_fails(self) -> None:
        for pid in (SURFACE["process_id"], 70001, 70003):
            self.setUp()
            self.quiet_observer["observer_process"]["pid"] = pid
            self.assertFails("observer pid=.*inside OSL/harness/provider")

    def test_missing_os_hook_and_missing_visible_response_fail(self) -> None:
        self.quiet_observer["probes"][7]["os_hook"]["input_id"] = "unmatched"
        self.assertFails("missing SendInput OS hook")
        self.setUp()
        self.busy_observer["probes"][8]["visible_response"]["changed"] = False
        self.assertFails("missing same-surface visible response")

    def test_completed_gap_above_500ms_fails(self) -> None:
        # Keep the individual latency legal while producing a 550ms response gap.
        probe = self.quiet_observer["probes"][10]
        probe["sent_qpc"] += 500
        probe["response_qpc"] += 500
        probe["os_hook"]["qpc"] += 500
        probe["visible_response"]["qpc"] += 500
        probe["completed_gap_ms"] = 550
        self.assertFails("completed-probe gap 550ms exceeds 500ms")

    def test_fabricated_pause_count_and_content_latency_fail(self) -> None:
        self.quiet_observer["probes"][1]["latency_ms"] = 0
        self.assertFails("fabricated probe latency")
        self.setUp()
        self.busy_observer["content_changes"][2]["latency_ms"] = 0
        self.assertFails("fabricated content latency")
        self.setUp()
        self.quiet_workload["metrics"]["look_count"] = 49
        self.assertFails("fabricated look count")

    def test_content_wrong_surface_row_reuse_and_receive_bound_fail(self) -> None:
        self.quiet_observer["content_changes"][3]["surface"]["hwnd"] = "0xOTHER"
        self.assertFails("protected marker changed another OSL control")
        self.setUp()
        self.quiet_observer["content_changes"][3]["row_runtime_id"] = self.quiet_observer["content_changes"][2]["row_runtime_id"]
        self.assertFails("multiple provider IDs matched one content row")
        self.setUp()
        change = self.busy_observer["content_changes"][4]
        change["observed_qpc"] = self.busy_workload["protected_markers"][4]["provider_qpc"] + 15_001
        change["latency_ms"] = 15_001
        self.assertFails("receive latency 15001ms")

    def test_quiet_read_budget_and_provider_floors_fail(self) -> None:
        for index in range(6):
            extra = copy.deepcopy(self.quiet_workload["carrier_reads"][index])
            extra["read_id"] = f"extra-{index}"
            extra["look_id"] = f"extra-look-{index}"
            self.quiet_workload["carrier_reads"].append(extra)
            look = copy.deepcopy(self.quiet_workload["looks"][index])
            look["look_id"] = extra["look_id"]
            self.quiet_workload["looks"].append(look)
        self.quiet_workload["metrics"]["look_count"] = 56
        self.quiet_workload["metrics"]["carrier_screen_read_count"] = 56
        self.assertFails("quiet carrier screen reads=56")
        self.setUp()
        self.quiet_workload["carrier_reads"][2]["qpc"] = self.quiet_workload["carrier_reads"][0]["qpc"] + 749
        self.quiet_workload["metrics"]["discord_closest_read_ms"] = 749
        self.assertFails("Discord carrier-read spacing=749ms")
        self.setUp()
        self.busy_workload["carrier_reads"][3]["qpc"] = self.busy_workload["carrier_reads"][1]["qpc"] + 1_999
        self.busy_workload["metrics"]["whatsapp_closest_read_ms"] = 1_999
        self.assertFails("WhatsApp carrier-read spacing=1999ms")

    def test_busy_look_count_and_four_second_floor_fail(self) -> None:
        self.busy_workload["looks"] = self.busy_workload["looks"][:44]
        self.busy_workload["carrier_reads"] = self.busy_workload["carrier_reads"][:44]
        self.busy_workload["metrics"]["look_count"] = 44
        self.busy_workload["metrics"]["carrier_screen_read_count"] = 44
        self.assertFails("outside 10 percent")
        self.setUp()
        self.busy_workload["looks"][1]["qpc"] = self.busy_workload["looks"][0]["qpc"] + 3_999
        self.busy_workload["carrier_reads"][1]["qpc"] = self.busy_workload["looks"][1]["qpc"]
        self.busy_workload["metrics"]["closest_look_ms"] = 3_999
        self.assertFails("closest look gap=3999ms")

    def test_exact_workload_schedules_and_unique_provider_ids_fail(self) -> None:
        self.quiet_workload["protected_markers"][4]["scheduled_qpc"] += 1
        self.assertFails("protected markers schedule mismatch")
        self.setUp()
        self.busy_workload["ordinary_messages"].pop()
        self.busy_workload["metrics"]["ordinary_message_count"] = 199
        self.assertFails("ordinary messages count=199 expected=200")
        self.setUp()
        self.busy_workload["ordinary_messages"][1]["provider_id"] = self.busy_workload["ordinary_messages"][0]["provider_id"]
        self.assertFails("duplicate ordinary message provider_id")

    def test_outside_interval_stall_does_not_substitute_or_fail_green_runs(self) -> None:
        self.quiet_observer["outside_interval_stalls"] = [{"start_qpc": END + 1, "duration_ms": 900}]
        result = reconcile(self.quiet_observer, self.quiet_workload, self.busy_observer, self.busy_workload)
        self.assertEqual(result["status"], "PASS")

    def test_cli_names_failure_and_exits_one(self) -> None:
        self.quiet_observer["probes"][0]["target_surface"]["hwnd"] = "0xSACRIFICIAL"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            values = {
                "qo.json": self.quiet_observer,
                "qw.json": self.quiet_workload,
                "bo.json": self.busy_observer,
                "bw.json": self.busy_workload,
            }
            for name, value in values.items():
                (root / name).write_text(json.dumps(value), encoding="utf-8")
            script = Path(__file__).with_name("task_6331_receive_responsiveness.py")
            completed = subprocess.run(
                [sys.executable, str(script), "--quiet-observer", str(root / "qo.json"), "--quiet-workload", str(root / "qw.json"), "--busy-observer", str(root / "bo.json"), "--busy-workload", str(root / "bw.json")],
                text=True,
                capture_output=True,
                check=False,
            )
        self.assertEqual(completed.returncode, 1)
        self.assertIn("TASK6331_FAIL=", completed.stderr)
        self.assertIn("run=quiet", completed.stderr)
        self.assertIn("input_id=quiet-input-0000", completed.stderr)
        self.assertIn("timestamps=", completed.stderr)


if __name__ == "__main__":
    unittest.main()
