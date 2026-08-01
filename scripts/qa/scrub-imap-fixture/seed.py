#!/usr/bin/env python3
"""Seed only the loopback-only IMAP fixtures declared in compose.yml."""

from __future__ import annotations

import argparse
import imaplib
from email.message import EmailMessage


FIXTURE_HOSTS = frozenset(("127.0.0.1", "localhost", "uidplus", "nouidplus"))
DEFAULT_USER = "fixture"
DEFAULT_PASSWORD = "pass"
LABEL_FOLDERS = ("[Gmail]/All Mail", "[Gmail]/Starred", "[Gmail]/Trash")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--messages", type=int, default=3)
    parser.add_argument("--username", default=DEFAULT_USER)
    parser.add_argument("--password", default=DEFAULT_PASSWORD)
    return parser.parse_args()


def seed(host: str, port: int, messages: int, username: str, password: str) -> None:
    if host not in FIXTURE_HOSTS:
        raise ValueError("refusing to seed a non-fixture IMAP host")
    if messages < 1:
        raise ValueError("--messages must be at least 1")

    with imaplib.IMAP4(host, port, timeout=5) as client:
        status, _ = client.login(username, password)
        if status != "OK":
            raise RuntimeError("fixture login failed")
        for folder in LABEL_FOLDERS:
            client.create(folder)
        for number in range(1, messages + 1):
            message = EmailMessage()
            message["From"] = "fixture@osl.invalid"
            message["To"] = "scrub@osl.invalid"
            message["Subject"] = f"OSL fixture message {number}"
            message["Message-ID"] = f"<osl-fixture-{number}@osl.invalid>"
            message.set_content(f"seeded fixture message {number}")
            status, _ = client.append("INBOX", None, None, message.as_bytes())
            if status != "OK":
                raise RuntimeError(f"could not append fixture message {number}")
        client.logout()


def main() -> None:
    args = parse_args()
    seed(args.host, args.port, args.messages, args.username, args.password)


if __name__ == "__main__":
    main()
