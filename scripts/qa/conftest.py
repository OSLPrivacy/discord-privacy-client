from __future__ import annotations

from pathlib import Path

import pytest


EXACT_NAMED_TESTS_BY_FILE = {
    "test_osl_launch_instance_b_contract.py": {
        "instance_b_launcher_uses_private_temp_root_and_preserves_instance_a",
        "instance_b_confirm_creates_identity_registers_second_identity",
    },
    "test_redaction.py": {
        "Reject content-bearing accessibility bench artifacts",
    },
    "test_osl_design_product_contracts.py": {
        "Encode Burn's five guarantees and banned phrases.",
    },
}


def pytest_pycollect_makeitem(
    collector: pytest.Module,
    name: str,
    obj: object,
) -> pytest.Function | None:
    exact_names = EXACT_NAMED_TESTS_BY_FILE.get(Path(str(collector.path)).name)
    if exact_names is None:
        return None
    if name not in exact_names or not callable(obj):
        return None
    return pytest.Function.from_parent(collector, name=name, callobj=obj)
