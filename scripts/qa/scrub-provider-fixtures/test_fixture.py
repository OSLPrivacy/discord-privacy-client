from __future__ import annotations

import json
import sys
import threading
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import pytest

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
from fixture_server import OPERATIONS, OUTCOMES, PROVIDERS, make_server


@pytest.fixture
def provider_target() -> str:
    server = make_server()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        host, port = server.server_address
        yield f"http://{host}:{port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def request(base: str, provider: str, operation: str, outcome: str) -> tuple[int, dict[str, object], dict[str, str]]:
    method = "DELETE" if operation == "delete" else "GET"
    target = f"{base}/v1/{provider}/{operation}?outcome={outcome}"
    try:
        with urlopen(Request(target, method=method), timeout=2) as response:
            return response.status, json.load(response), dict(response.headers.items())
    except HTTPError as error:
        return error.code, json.load(error), dict(error.headers.items())


@pytest.mark.parametrize("provider", sorted(PROVIDERS))
@pytest.mark.parametrize("operation", sorted(OPERATIONS))
@pytest.mark.parametrize("outcome", sorted(OUTCOMES))
def test_scr_e5_each_seeded_target_scripts_every_operation_and_outcome(
    provider_target: str, provider: str, operation: str, outcome: str
) -> None:
    status, body, headers = request(provider_target, provider, operation, outcome)
    assert body["outcome"] == outcome
    assert body["operation"] == operation
    if outcome == "success":
        assert status == 200
        assert "value" in body
    elif outcome == "refusal":
        assert status == 403
    elif outcome == "rate-limit":
        assert status == 429
        assert headers["Retry-After"] == "60"
    else:
        assert status == 503
        assert "unknown" in str(body["error"]).lower()


def test_scr_e5_refuses_non_loopback_fixture_binding() -> None:
    with pytest.raises(ValueError, match="outside loopback"):
        make_server("198.51.100.10")
