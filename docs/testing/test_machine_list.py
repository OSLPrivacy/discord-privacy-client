import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECK = ROOT / "docs/testing/check-test-machine-list.py"
LIST = ROOT / "docs/testing/test-machine-list.json"

def test_starting_list_is_complete_and_has_expected_groups():
    data = json.loads(LIST.read_text())
    names = {item["name"] for item in data["machines"]}
    assert len(names) == 16
    assert "osl-build-pc" in names
    assert sum(item["system"] == "Windows 11" for item in data["machines"]) == 10
    assert sum(item["system"] == "Azure capacity reservation" for item in data["machines"]) == 5
    result = subprocess.run([sys.executable, str(CHECK), str(LIST)], capture_output=True, text=True)
    assert result.returncode == 0
    assert "machine_list_entries=16" in result.stdout

def test_missing_field_exits_one():
    data = json.loads(LIST.read_text())
    del data["machines"][0]["location"]
    with tempfile.NamedTemporaryFile("w", suffix=".json") as f:
        json.dump(data, f)
        f.flush()
        result = subprocess.run([sys.executable, str(CHECK), f.name], capture_output=True, text=True)
    assert result.returncode == 1
    assert "missing location" in result.stderr
