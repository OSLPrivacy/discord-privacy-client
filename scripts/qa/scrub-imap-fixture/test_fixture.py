from __future__ import annotations

import socket
import socketserver
import sys
import threading
from pathlib import Path

import pytest

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
from seed import seed
from verify import assert_seeded


class ImapFixtureHandler(socketserver.StreamRequestHandler):
    messages: list[bytes] = []

    def handle(self) -> None:
        self.wfile.write(b"* OK OSL fixture ready\r\n")
        while line := self.rfile.readline():
            tag, command, *arguments = line.decode().rstrip("\r\n").split(" ")
            verb = command.upper()
            if verb == "CAPABILITY":
                self.wfile.write(b"* CAPABILITY IMAP4rev1 AUTH=PLAIN\r\n" + f"{tag} OK CAPABILITY complete\r\n".encode())
            elif verb == "LOGIN":
                self.wfile.write(f"{tag} OK LOGIN complete\r\n".encode())
            elif verb == "CREATE":
                self.wfile.write(f"{tag} OK CREATE complete\r\n".encode())
            elif verb == "APPEND":
                size = int(arguments[-1].strip("{}"))
                self.wfile.write(b"+ send message\r\n")
                self.messages.append(self.rfile.read(size))
                self.rfile.read(2)
                self.wfile.write(f"{tag} OK APPEND complete\r\n".encode())
            elif verb == "SELECT":
                self.wfile.write(f"* {len(self.messages)} EXISTS\r\n{tag} OK SELECT complete\r\n".encode())
            elif verb == "SEARCH":
                ids = b" ".join(str(i).encode() for i in range(1, len(self.messages) + 1))
                self.wfile.write(b"* SEARCH " + ids + f"\r\n{tag} OK SEARCH complete\r\n".encode())
            elif verb == "FETCH":
                message = self.messages[int(arguments[0]) - 1]
                self.wfile.write(
                    f"* {arguments[0]} FETCH (RFC822.HEADER {{{len(message)}}}\r\n".encode()
                    + message
                    + b")\r\n"
                    + f"{tag} OK FETCH complete\r\n".encode()
                )
            elif verb == "LOGOUT":
                self.wfile.write(b"* BYE done\r\n" + f"{tag} OK LOGOUT complete\r\n".encode())
                return
            else:
                self.wfile.write(f"{tag} BAD unsupported command\r\n".encode())


@pytest.fixture
def imap_target() -> tuple[str, int]:
    ImapFixtureHandler.messages = []
    server = socketserver.ThreadingTCPServer(("127.0.0.1", 0), ImapFixtureHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_address
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def test_scr_e3_seeded_target_supports_login_search_and_fetch(imap_target: tuple[str, int]) -> None:
    host, port = imap_target
    seed(host, port, 3, "fixture", "password")
    assert_seeded(host, port, 3, "fixture", "password")


def test_scr_e3_wrong_port_fails_instead_of_silently_seeding(imap_target: tuple[str, int]) -> None:
    host, _ = imap_target
    with socket.socket() as probe:
        probe.bind((host, 0))
        wrong_port = probe.getsockname()[1]
    with pytest.raises((ConnectionRefusedError, OSError)):
        seed(host, wrong_port, 1, "fixture", "password")


def test_scr_e3_refuses_a_non_fixture_host() -> None:
    with pytest.raises(ValueError):
        seed("mail.example.test", 143, 1, "fixture", "password")
