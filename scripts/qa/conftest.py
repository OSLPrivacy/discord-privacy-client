from __future__ import annotations

from pathlib import Path

import pytest


EXACT_LAUNCH_INSTANCE_B_CONTRACT_TESTS = {
    "instance_b_launcher_uses_private_temp_root_and_preserves_instance_a",
    "instance_b_confirm_creates_identity_registers_second_identity",
}

EXACT_P2P_PAIR_TESTS = {
    "osl_p2p_pair_refuses_same_osl_user_id",
}


def pytest_pycollect_makeitem(
    collector: pytest.Module,
    name: str,
    obj: object,
) -> pytest.Function | None:
    exact_tests_by_file = {
        "test_osl_launch_instance_b_contract.py": EXACT_LAUNCH_INSTANCE_B_CONTRACT_TESTS,
        "test_osl_p2p_pair.py": EXACT_P2P_PAIR_TESTS,
    }
    exact_tests = exact_tests_by_file.get(Path(str(collector.path)).name)
    if exact_tests is None:
        return None
    if name not in exact_tests or not callable(obj):
        return None
    return pytest.Function.from_parent(collector, name=name, callobj=obj)
