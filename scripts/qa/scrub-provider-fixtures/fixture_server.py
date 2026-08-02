#!/usr/bin/env python3
"""Loopback-only scripted targets for Scrub's non-IMAP provider adapters."""

from __future__ import annotations

import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

PROVIDERS = frozenset(("reddit", "x", "instagram", "gmail-rest", "discord-ui", "facebook-activity"))
OPERATIONS = frozenset(("enumerate", "inspect", "delete", "verify"))
OUTCOMES = frozenset(("success", "refusal", "rate-limit", "ambiguous"))


def fixture_response(provider: str, operation: str, outcome: str) -> tuple[int, dict[str, object], dict[str, str]]:
    """Return a deterministic, provider-neutral adapter response.

    `ambiguous` deliberately avoids a success-shaped receipt. An adapter must turn
    it into UNKNOWN and stop rather than retrying a potentially destructive call.
    """
    if provider not in PROVIDERS or operation not in OPERATIONS or outcome not in OUTCOMES:
        return 404, {"error": "unknown fixture route"}, {}
    item = {"id": f"{provider}-seed-001", "provider": provider, "owned_by_fixture": True}
    if outcome == "success":
        value: object = [item] if operation == "enumerate" else item
        if operation == "inspect":
            value = {**item, "state": "present", "content_fingerprint": "fixture-fingerprint-v1"}
        if operation == "delete":
            value = {**item, "accepted": True}
        if operation == "verify":
            value = {**item, "outcome": "confirmed-deleted"}
        return 200, {"outcome": outcome, "operation": operation, "value": value}, {}
    if outcome == "refusal":
        return 403, {"outcome": outcome, "operation": operation, "error": "fixture refusal: consent required"}, {}
    if outcome == "rate-limit":
        return 429, {"outcome": outcome, "operation": operation, "error": "fixture rate limit: stop the run"}, {"Retry-After": "60"}
    return 503, {"outcome": outcome, "operation": operation, "error": "fixture ambiguous outcome: state unknown; do not retry"}, {}


class FixtureHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802
        self._respond()

    def do_DELETE(self) -> None:  # noqa: N802
        self._respond()

    def log_message(self, format: str, *args: object) -> None:
        return

    def _respond(self) -> None:
        parsed = urlparse(self.path)
        parts = parsed.path.strip("/").split("/")
        query = parse_qs(parsed.query)
        if len(parts) != 3 or parts[0] != "v1":
            status, body, headers = 404, {"error": "unknown fixture route"}, {}
        else:
            outcome = query.get("outcome", ["success"])[0]
            status, body, headers = fixture_response(parts[1], parts[2], outcome)
        encoded = json.dumps(body, sort_keys=True).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        for name, value in headers.items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(encoded)


def make_server(host: str = "127.0.0.1", port: int = 0) -> ThreadingHTTPServer:
    if host not in {"127.0.0.1", "::1", "localhost"}:
        raise ValueError("refusing to bind a Scrub fixture outside loopback")
    return ThreadingHTTPServer((host, port), FixtureHandler)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8787)
    args = parser.parse_args()
    server = make_server(args.host, args.port)
    print(f"Scrub provider fixtures listening on http://{args.host}:{server.server_port}")
    server.serve_forever()


if __name__ == "__main__":
    main()
