#!/usr/bin/env python3
"""Prove a seeded fixture accepts LOGIN, SEARCH, and FETCH."""

from __future__ import annotations

import argparse
import imaplib

from seed import DEFAULT_PASSWORD, DEFAULT_USER, FIXTURE_HOSTS


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--messages", type=int, required=True)
    parser.add_argument("--username", default=DEFAULT_USER)
    parser.add_argument("--password", default=DEFAULT_PASSWORD)
    return parser.parse_args()


def assert_seeded(host: str, port: int, messages: int, username: str, password: str) -> None:
    if host not in FIXTURE_HOSTS:
        raise ValueError("refusing to verify a non-fixture IMAP host")
    with imaplib.IMAP4(host, port, timeout=5) as client:
        status, _ = client.login(username, password)
        if status != "OK":
            raise RuntimeError("fixture login failed")
        status, _ = client.select("INBOX")
        if status != "OK":
            raise RuntimeError("could not select fixture inbox")
        status, data = client.search(None, "ALL")
        if status != "OK":
            raise RuntimeError("fixture search failed")
        ids = data[0].split()
        if len(ids) != messages:
            raise RuntimeError(f"expected {messages} messages, found {len(ids)}")
        for message_id in ids:
            status, fetched = client.fetch(message_id, "(RFC822.HEADER)")
            if status != "OK" or not fetched or not isinstance(fetched[0], tuple):
                raise RuntimeError(f"could not fetch fixture message {message_id!r}")
        client.logout()


def main() -> None:
    args = parse_args()
    assert_seeded(args.host, args.port, args.messages, args.username, args.password)


if __name__ == "__main__":
    main()
