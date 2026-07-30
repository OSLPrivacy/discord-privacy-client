from __future__ import annotations

from pathlib import Path

import pytest


EXACT_LAUNCH_INSTANCE_B_CONTRACT_TESTS = {
    "instance_b_launcher_uses_private_temp_root_and_preserves_instance_a",
    "instance_b_confirm_creates_identity_registers_second_identity",
}


def pytest_pycollect_makeitem(
    collector: pytest.Module,
    name: str,
    obj: object,
) -> pytest.Function | None:
    if Path(str(collector.path)).name != "test_osl_launch_instance_b_contract.py":
        return None
    if name not in EXACT_LAUNCH_INSTANCE_B_CONTRACT_TESTS or not callable(obj):
        return None
    return pytest.Function.from_parent(collector, name=name, callobj=obj)
