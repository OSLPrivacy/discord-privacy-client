#!/usr/bin/env python3
"""TLS 1.3-only signaling front door for the pinned LiveKit deployment.

LiveKit deliberately expects TLS termination in front of its HTTP/WebSocket
signaling port.  This task-owned front door has one policy: a TLS 1.3 session is
required.  A clear offer receives the exact stable refusal required by the OSL
contract and is never proxied upstream.
"""

from __future__ import annotations

import argparse
import json
import select
import socket
import ssl
import sys
import threading
from pathlib import Path


def emit(**event: object) -> None:
    print(json.dumps(event, sort_keys=True), flush=True)


def relay(client: ssl.SSLSocket, upstream_address: tuple[str, int]) -> None:
    upstream = socket.create_connection(upstream_address, timeout=10)
    client.setblocking(False)
    upstream.setblocking(False)
    sockets = [client, upstream]
    try:
        while True:
            readable, _, exceptional = select.select(sockets, [], sockets, 1.0)
            if exceptional:
                return
            for source in readable:
                target = upstream if source is client else client
                try:
                    data = source.recv(65536)
                except (BlockingIOError, ssl.SSLWantReadError):
                    continue
                if not data:
                    return
                view = memoryview(data)
                while view:
                    try:
                        sent = target.send(view)
                        view = view[sent:]
                    except (BlockingIOError, ssl.SSLWantWriteError):
                        select.select([], [target], [], 1.0)
    finally:
        upstream.close()
        client.close()


def handle(
    raw: socket.socket,
    peer: tuple[str, int],
    context: ssl.SSLContext,
    upstream_address: tuple[str, int],
    refusal: str,
) -> None:
    try:
        first = raw.recv(1, socket.MSG_PEEK)
        if first != b"\x16":
            raw.sendall((refusal + "\n").encode("ascii"))
            emit(event="clear_refused", peer=f"{peer[0]}:{peer[1]}", verdict=refusal)
            raw.close()
            return
        client = context.wrap_socket(raw, server_side=True)
        emit(
            event="tls_handshake",
            peer=f"{peer[0]}:{peer[1]}",
            protocol=client.version(),
            cipher=client.cipher()[0],
            transport_encryption_required=True,
        )
        relay(client, upstream_address)
    except Exception as error:  # handshake failures are fail-closed
        emit(event="tls_rejected", peer=f"{peer[0]}:{peer[1]}", error=str(error))
        raw.close()


def parse_address(value: str) -> tuple[str, int]:
    host, port = value.rsplit(":", 1)
    return host, int(port)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    args = parser.parse_args()
    config_path = Path(args.config).resolve()
    config = json.loads(config_path.read_text(encoding="utf-8"))
    expected = {
        "listen",
        "upstream",
        "minimum_tls",
        "certificate",
        "private_key",
        "clear_refusal",
    }
    if set(config) != expected:
        raise SystemExit(f"front-door config fields differ: {sorted(set(config) ^ expected)}")
    if config["minimum_tls"] != "TLSv1.3":
        raise SystemExit("transport encryption required")
    if config["clear_refusal"] != "transport encryption required":
        raise SystemExit("transport encryption required")

    task_root = config_path.parent.parent
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_3
    context.maximum_version = ssl.TLSVersion.TLSv1_3
    context.load_cert_chain(
        task_root / config["certificate"], task_root / config["private_key"]
    )
    listen_address = parse_address(config["listen"])
    upstream_address = parse_address(config["upstream"])
    listener = socket.create_server(listen_address, reuse_port=False)
    emit(
        event="listening",
        listen=config["listen"],
        upstream=config["upstream"],
        minimum_tls=config["minimum_tls"],
        pid=__import__("os").getpid(),
    )
    while True:
        raw, peer = listener.accept()
        threading.Thread(
            target=handle,
            args=(raw, peer, context, upstream_address, config["clear_refusal"]),
            daemon=True,
        ).start()


if __name__ == "__main__":
    sys.exit(main())

