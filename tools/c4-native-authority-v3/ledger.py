"""Crash-safe, fail-closed one-shot challenge ledger for C4 receipt capture."""

from __future__ import annotations

import contextlib
import copy
import ctypes
import hashlib
import hmac
import json
import os
from pathlib import Path
import threading
from typing import Any, Iterator

from schema import (
    PROCESS_KEYS,
    SHA256_RE,
    SchemaError,
    canonical_json,
    sha256_hex,
    validate_process_binding,
)


LEDGER_VERSION = 1
MAX_TTL_MS = 60_000
MAX_INTEGER = (1 << 63) - 1
STATES = {"issued", "connected", "consumed", "expired", "abandoned"}
TERMINAL_STATES = {"consumed", "expired", "abandoned"}
RECORD_KEYS = {
    "version",
    "challengeSha256",
    "state",
    "issuedAtUnixMs",
    "expiresAtUnixMs",
    "pipeBindingSha256",
    "receiptDigestSha256",
    "updatedAtUnixMs",
    "recordDigestSha256",
}


class LedgerError(RuntimeError):
    """The one-shot ledger cannot safely authorize the requested transition."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise LedgerError(f"duplicate ledger key: {key}")
        value[key] = item
    return value


def _record_digest(record: dict[str, Any]) -> str:
    body = copy.deepcopy(record)
    body.pop("recordDigestSha256", None)
    return sha256_hex(b"OSL/C4/challenge-ledger/v1\x00" + canonical_json(body))


def _seal_record(record: dict[str, Any]) -> dict[str, Any]:
    sealed = copy.deepcopy(record)
    sealed["recordDigestSha256"] = _record_digest(sealed)
    return sealed


def _challenge_hash(challenge: str) -> str:
    if type(challenge) is not str or SHA256_RE.fullmatch(challenge) is None:
        raise LedgerError("challenge is not 32-byte lowercase hex")
    if challenge == "0" * 64:
        raise LedgerError("zero challenge is forbidden")
    return hashlib.sha256(bytes.fromhex(challenge)).hexdigest()


def binding_digest(binding: dict[str, Any]) -> str:
    if type(binding) is not dict or set(binding) != PROCESS_KEYS:
        raise LedgerError("pipe binding fields are not exact")
    try:
        validate_process_binding(binding, "pipeClient")
    except SchemaError as error:
        raise LedgerError("pipe binding is invalid") from error
    return sha256_hex(b"OSL/C4/pipe-client-binding/v1\x00" + canonical_json(binding))


@contextlib.contextmanager
def _platform_file_lock(path: Path) -> Iterator[None]:
    path.parent.mkdir(parents=True, exist_ok=True)
    handle = open(path, "a+b")
    try:
        if handle.tell() == 0:
            handle.write(b"\x00")
            handle.flush()
            os.fsync(handle.fileno())
        handle.seek(0)
        if os.name == "nt":
            import msvcrt

            msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK, 1)
        else:
            import fcntl

            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            handle.seek(0)
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    finally:
        handle.close()


class OneShotLedger:
    """One durable record per challenge, with atomic terminal transitions.

    The caller must invoke ``recover_incomplete`` once on verifier startup.
    Recovery abandons every prior nonterminal challenge; it never resumes one.
    """

    def __init__(self, root: Path | str):
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=True, mode=0o700)
        self._thread_lock = threading.Lock()
        self._lock_path = self.root / ".ledger.lock"

    def _path(self, challenge: str) -> Path:
        return self.root / f"{_challenge_hash(challenge)}.json"

    @contextlib.contextmanager
    def _locked(self) -> Iterator[None]:
        with self._thread_lock:
            with _platform_file_lock(self._lock_path):
                yield

    def _encode(self, record: dict[str, Any]) -> bytes:
        return canonical_json(_seal_record(record)) + b"\n"

    def _read_path(self, path: Path) -> dict[str, Any]:
        try:
            raw = path.read_bytes()
        except OSError as error:
            raise LedgerError("ledger record could not be read") from error
        if not raw or len(raw) > 16 * 1024 or not raw.endswith(b"\n"):
            raise LedgerError("ledger record is truncated or oversized")
        try:
            record = json.loads(
                raw.decode("utf-8"),
                object_pairs_hook=_reject_duplicate_keys,
                parse_constant=lambda value: (_ for _ in ()).throw(
                    LedgerError(f"invalid ledger number: {value}")
                ),
            )
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise LedgerError("ledger record is malformed") from error
        if type(record) is not dict or set(record) != RECORD_KEYS:
            raise LedgerError("ledger record fields are not exact")
        if type(record["version"]) is not int or record["version"] != LEDGER_VERSION:
            raise LedgerError("ledger record version is invalid")
        if type(record["challengeSha256"]) is not str or SHA256_RE.fullmatch(
            record["challengeSha256"]
        ) is None:
            raise LedgerError("ledger challenge hash is invalid")
        if type(record["state"]) is not str or record["state"] not in STATES:
            raise LedgerError("ledger state is unknown")
        for key in ("issuedAtUnixMs", "expiresAtUnixMs", "updatedAtUnixMs"):
            if (
                type(record[key]) is not int
                or not 0 <= record[key] <= MAX_INTEGER
            ):
                raise LedgerError(f"ledger {key} is invalid")
        for key in ("pipeBindingSha256", "receiptDigestSha256"):
            value = record[key]
            if value is not None and (
                type(value) is not str or SHA256_RE.fullmatch(value) is None
            ):
                raise LedgerError(f"ledger {key} is invalid")
        issued = record["issuedAtUnixMs"]
        expires = record["expiresAtUnixMs"]
        updated = record["updatedAtUnixMs"]
        if not issued < expires <= issued + MAX_TTL_MS or updated < issued:
            raise LedgerError("ledger timestamps are incoherent")
        state = record["state"]
        pipe_digest = record["pipeBindingSha256"]
        receipt_digest = record["receiptDigestSha256"]
        if state == "issued" and (
            pipe_digest is not None or receipt_digest is not None
        ):
            raise LedgerError("issued ledger fields are incoherent")
        if state == "connected" and (
            pipe_digest is None or receipt_digest is not None
        ):
            raise LedgerError("connected ledger fields are incoherent")
        if state == "consumed" and (
            pipe_digest is None or receipt_digest is None
        ):
            raise LedgerError("consumed ledger fields are incoherent")
        if state in {"expired", "abandoned"} and receipt_digest is not None:
            raise LedgerError("terminal ledger fields are incoherent")
        if state == "expired":
            if updated <= expires:
                raise LedgerError("expired ledger timestamp is incoherent")
        elif updated > expires:
            raise LedgerError("non-expired ledger timestamp is incoherent")
        claimed = record["recordDigestSha256"]
        if type(claimed) is not str or not hmac.compare_digest(
            claimed, _record_digest(record)
        ):
            raise LedgerError("ledger record integrity failed")
        return record

    def read(self, challenge: str) -> dict[str, Any]:
        expected = _challenge_hash(challenge)
        with self._locked():
            record = self._read_path(self._path(challenge))
        if record["challengeSha256"] != expected:
            raise LedgerError("ledger record belongs to another challenge")
        return record

    def _atomic_replace(self, path: Path, record: dict[str, Any]) -> None:
        encoded = self._encode(record)
        temporary = path.with_name(
            f".{path.name}.tmp-{os.getpid()}-{threading.get_ident()}"
        )
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
        descriptor = None
        try:
            descriptor = os.open(temporary, flags, 0o600)
            with os.fdopen(descriptor, "wb", closefd=True) as handle:
                descriptor = None
                handle.write(encoded)
                handle.flush()
                os.fsync(handle.fileno())
            self._replace_write_through(temporary, path)
            self._fsync_directory()
        except OSError as error:
            raise LedgerError("ledger transition could not be committed") from error
        finally:
            if descriptor is not None:
                os.close(descriptor)
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass

    def _replace_write_through(self, source: Path, destination: Path) -> None:
        if os.name != "nt":
            os.replace(source, destination)
            return
        # os.replace does not request write-through durability on Windows.
        # MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH makes the
        # one-shot transition reach stable storage before it is reported.
        move_file_ex = ctypes.windll.kernel32.MoveFileExW
        move_file_ex.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint32]
        move_file_ex.restype = ctypes.c_int
        if not move_file_ex(str(source), str(destination), 0x1 | 0x8):
            raise ctypes.WinError()

    def _fsync_directory(self) -> None:
        if os.name == "nt":
            return
        descriptor = os.open(self.root, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    def issue(self, challenge: str, now_ms: int, ttl_ms: int = MAX_TTL_MS) -> None:
        challenge_hash = _challenge_hash(challenge)
        if type(now_ms) is not int or not 0 <= now_ms <= MAX_INTEGER:
            raise LedgerError("issue time is invalid")
        if type(ttl_ms) is not int or not 1 <= ttl_ms <= MAX_TTL_MS:
            raise LedgerError("challenge lifetime is invalid")
        if now_ms > MAX_INTEGER - ttl_ms:
            raise LedgerError("challenge expiry is out of range")
        record = {
            "version": LEDGER_VERSION,
            "challengeSha256": challenge_hash,
            "state": "issued",
            "issuedAtUnixMs": now_ms,
            "expiresAtUnixMs": now_ms + ttl_ms,
            "pipeBindingSha256": None,
            "receiptDigestSha256": None,
            "updatedAtUnixMs": now_ms,
            "recordDigestSha256": "",
        }
        path = self._path(challenge)
        with self._locked():
            try:
                descriptor = os.open(
                    path,
                    os.O_WRONLY | os.O_CREAT | os.O_EXCL,
                    0o600,
                )
            except FileExistsError as error:
                raise LedgerError("challenge was already issued") from error
            try:
                with os.fdopen(descriptor, "wb", closefd=True) as handle:
                    handle.write(self._encode(record))
                    handle.flush()
                    os.fsync(handle.fileno())
                self._fsync_directory()
            except OSError as error:
                try:
                    path.unlink()
                except FileNotFoundError:
                    pass
                raise LedgerError("challenge issuance could not be committed") from error

    def _load_for_transition(
        self, challenge: str, now_ms: int
    ) -> tuple[Path, dict[str, Any]]:
        if type(now_ms) is not int or not 0 <= now_ms <= MAX_INTEGER:
            raise LedgerError("transition time is invalid")
        expected = _challenge_hash(challenge)
        path = self._path(challenge)
        record = self._read_path(path)
        if record["challengeSha256"] != expected:
            raise LedgerError("ledger record belongs to another challenge")
        if now_ms > record["expiresAtUnixMs"]:
            if record["state"] not in TERMINAL_STATES:
                record["state"] = "expired"
                record["updatedAtUnixMs"] = now_ms
                self._atomic_replace(path, record)
            raise LedgerError("challenge expired")
        return path, record

    def connect(
        self,
        challenge: str,
        pipe_client_binding: dict[str, Any],
        now_ms: int,
    ) -> str:
        digest = binding_digest(pipe_client_binding)
        with self._locked():
            path, record = self._load_for_transition(challenge, now_ms)
            if record["state"] != "issued":
                raise LedgerError("challenge is not available for connection")
            record["state"] = "connected"
            record["pipeBindingSha256"] = digest
            record["updatedAtUnixMs"] = now_ms
            self._atomic_replace(path, record)
        return digest

    def consume(
        self,
        challenge: str,
        pipe_client_binding: dict[str, Any],
        receipt_sha256: str,
        now_ms: int,
    ) -> None:
        if type(receipt_sha256) is not str or SHA256_RE.fullmatch(receipt_sha256) is None:
            raise LedgerError("receipt digest is invalid")
        expected_binding = binding_digest(pipe_client_binding)
        with self._locked():
            path, record = self._load_for_transition(challenge, now_ms)
            if record["state"] != "connected":
                raise LedgerError("challenge is not connected or was already consumed")
            if not hmac.compare_digest(
                record["pipeBindingSha256"] or "", expected_binding
            ):
                raise LedgerError("pipe client changed after connection")
            record["state"] = "consumed"
            record["receiptDigestSha256"] = receipt_sha256
            record["updatedAtUnixMs"] = now_ms
            self._atomic_replace(path, record)

    def recover_incomplete(self, now_ms: int) -> int:
        if type(now_ms) is not int or not 0 <= now_ms <= MAX_INTEGER:
            raise LedgerError("recovery time is invalid")
        changed = 0
        with self._locked():
            for path in sorted(self.root.glob("*.json")):
                record = self._read_path(path)
                if record["state"] in TERMINAL_STATES:
                    continue
                record["state"] = (
                    "expired" if now_ms > record["expiresAtUnixMs"] else "abandoned"
                )
                record["updatedAtUnixMs"] = now_ms
                self._atomic_replace(path, record)
                changed += 1
        return changed
