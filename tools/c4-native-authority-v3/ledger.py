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
import re
import stat
import threading
from typing import Any, Callable, Iterator

from schema import (
    PROCESS_KEYS,
    SHA256_RE,
    SchemaError,
    canonical_json,
    sha256_hex,
    validate_process_binding,
)


LEDGER_VERSION = 2
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
    "receiptFrameSha256",
    "updatedAtUnixMs",
    "recordDigestSha256",
}
RECORD_NAME_RE = re.compile(r"^[0-9a-f]{64}\.json$")
FILE_ATTRIBUTE_REPARSE_POINT = 0x00000400
MAX_RECORD_BYTES = 16 * 1024
WATERMARK_BYTES = 8


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
    if "recordDigestSha256" not in body:
        raise LedgerError("ledger record digest field is absent")
    body.pop("recordDigestSha256")
    return sha256_hex(b"OSL/C4/challenge-ledger/v2\x00" + canonical_json(body))


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


def pipe_binding_digest(binding: dict[str, Any]) -> str:
    if type(binding) is not dict or set(binding) != PROCESS_KEYS:
        raise LedgerError("pipe binding fields are not exact")
    try:
        validate_process_binding(binding, "pipeClient")
    except SchemaError as error:
        raise LedgerError("pipe binding is invalid") from error
    return sha256_hex(b"OSL/C4/pipe-client-binding/v1\x00" + canonical_json(binding))


def _is_reparse_point(metadata: os.stat_result) -> bool:
    attributes = getattr(metadata, "st_file_attributes", 0)
    return bool(attributes & FILE_ATTRIBUTE_REPARSE_POINT)


def _file_identity(metadata: os.stat_result) -> tuple[int, int]:
    return metadata.st_dev, metadata.st_ino


def _require_private_posix_file(metadata: os.stat_result, label: str) -> None:
    if os.name == "nt":
        return
    if metadata.st_uid != os.geteuid():
        raise LedgerError(f"{label} is not owned by the current user")
    if stat.S_IMODE(metadata.st_mode) & 0o077:
        raise LedgerError(f"{label} permissions are not private")


def _inspect_leaf(path: Path, label: str, *, directory: bool) -> os.stat_result:
    try:
        metadata = os.lstat(path)
    except OSError as error:
        raise LedgerError(f"{label} could not be inspected") from error
    expected = stat.S_ISDIR if directory else stat.S_ISREG
    if (
        stat.S_ISLNK(metadata.st_mode)
        or _is_reparse_point(metadata)
        or not expected(metadata.st_mode)
    ):
        raise LedgerError(f"{label} is linked, reparse-backed, or irregular")
    if not directory and metadata.st_nlink != 1:
        raise LedgerError(f"{label} has multiple filesystem links")
    _require_private_posix_file(metadata, label)
    return metadata


def _open_existing_regular(path: Path, label: str, flags: int) -> int:
    before = _inspect_leaf(path, label, directory=False)
    open_flags = flags | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, open_flags)
    except OSError as error:
        raise LedgerError(f"{label} could not be opened safely") from error
    try:
        opened = os.fstat(descriptor)
        after = _inspect_leaf(path, label, directory=False)
        if (
            not stat.S_ISREG(opened.st_mode)
            or _is_reparse_point(opened)
            or _file_identity(before) != _file_identity(opened)
            or _file_identity(after) != _file_identity(opened)
        ):
            raise LedgerError(f"{label} identity changed while opening")
        _require_private_posix_file(opened, label)
        return descriptor
    except Exception:
        os.close(descriptor)
        raise


@contextlib.contextmanager
def _platform_file_lock(
    path: Path,
    assert_root_stable: Callable[[], None],
) -> Iterator[None]:
    assert_root_stable()
    try:
        descriptor = _open_existing_regular(path, "ledger lock", os.O_RDWR)
    except LedgerError:
        try:
            descriptor = os.open(
                path,
                os.O_RDWR
                | os.O_CREAT
                | os.O_EXCL
                | getattr(os, "O_CLOEXEC", 0)
                | getattr(os, "O_NOFOLLOW", 0),
                0o600,
            )
        except FileExistsError:
            descriptor = _open_existing_regular(path, "ledger lock", os.O_RDWR)
        except OSError as error:
            raise LedgerError("ledger lock could not be created safely") from error
        created = os.fstat(descriptor)
        if not stat.S_ISREG(created.st_mode) or _is_reparse_point(created):
            os.close(descriptor)
            raise LedgerError("ledger lock is irregular")
        _require_private_posix_file(created, "ledger lock")
    handle = os.fdopen(descriptor, "r+b", closefd=True)
    try:
        size = os.fstat(handle.fileno()).st_size
        if size not in (0, 1):
            raise LedgerError("ledger lock encoding is invalid")
        if size == 0:
            handle.write(b"\x00")
            handle.flush()
            os.fsync(handle.fileno())
        else:
            handle.seek(0)
            if handle.read(1) != b"\x00":
                raise LedgerError("ledger lock encoding is invalid")
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
        assert_root_stable()


class OneShotLedger:
    """One durable record per challenge, with atomic terminal transitions.

    The caller must invoke ``recover_incomplete`` once on verifier startup.
    Recovery abandons every prior nonterminal challenge; it never resumes one.
    """

    def __init__(self, root: Path | str):
        requested = Path(root)
        if not requested.is_absolute():
            raise LedgerError("ledger root must be absolute")
        if os.path.normpath(str(requested)) != str(requested):
            raise LedgerError("ledger root must be lexically canonical")
        metadata = _inspect_leaf(requested, "ledger root", directory=True)
        try:
            resolved = requested.resolve(strict=True)
        except OSError as error:
            raise LedgerError("ledger root could not be resolved") from error
        if os.path.normcase(str(resolved)) != os.path.normcase(str(requested)):
            raise LedgerError("ledger root must not contain aliases or links")
        self.root = requested
        self._root_identity = _file_identity(metadata)
        self._thread_lock = threading.Lock()
        self._lock_path = self.root / ".ledger.lock"
        self._watermark_path = self.root / ".ledger.watermark"
        self._recovery_complete = False
        self._last_observed_unix_ms: int | None = None

    @property
    def recovery_complete(self) -> bool:
        return self._recovery_complete

    def _assert_root_stable(self) -> None:
        metadata = _inspect_leaf(self.root, "ledger root", directory=True)
        if _file_identity(metadata) != self._root_identity:
            raise LedgerError("ledger root identity changed")
        try:
            resolved = self.root.resolve(strict=True)
        except OSError as error:
            raise LedgerError("ledger root could not be re-resolved") from error
        if os.path.normcase(str(resolved)) != os.path.normcase(str(self.root)):
            raise LedgerError("ledger root resolution changed")

    def _require_recovery(self) -> None:
        if not self._recovery_complete:
            raise LedgerError("ledger recovery is required before work")

    def _check_clock(self, now_ms: int, label: str) -> None:
        if type(now_ms) is not int or not 0 <= now_ms <= MAX_INTEGER:
            raise LedgerError(f"{label} time is invalid")
        if (
            self._last_observed_unix_ms is not None
            and now_ms < self._last_observed_unix_ms
        ):
            raise LedgerError("ledger clock moved backwards")

    def _observe_clock(self, now_ms: int) -> None:
        self._last_observed_unix_ms = max(
            self._last_observed_unix_ms or 0,
            now_ms,
        )

    def _scan_records(self) -> list[tuple[Path, dict[str, Any]]]:
        try:
            entries = sorted(os.scandir(self.root), key=lambda entry: entry.name)
        except OSError as error:
            raise LedgerError("ledger root could not be enumerated") from error
        records: list[tuple[Path, dict[str, Any]]] = []
        for entry in entries:
            if entry.name == ".ledger.lock":
                _inspect_leaf(self._lock_path, "ledger lock", directory=False)
                continue
            if entry.name == ".ledger.watermark":
                self._read_watermark()
                continue
            if RECORD_NAME_RE.fullmatch(entry.name) is None:
                raise LedgerError("ledger root contains an unexpected entry")
            path = self.root / entry.name
            records.append((path, self._read_path(path)))
        return records

    def _read_watermark(self) -> int | None:
        try:
            descriptor = _open_existing_regular(
                self._watermark_path,
                "ledger watermark",
                os.O_RDONLY,
            )
        except LedgerError as error:
            try:
                os.lstat(self._watermark_path)
            except FileNotFoundError:
                return None
            except OSError as inspect_error:
                raise LedgerError("ledger watermark could not be inspected") from inspect_error
            raise error
        with os.fdopen(descriptor, "rb", closefd=True) as handle:
            encoded = handle.read(WATERMARK_BYTES + 1)
        if len(encoded) != WATERMARK_BYTES:
            raise LedgerError("ledger watermark encoding is invalid")
        value = int.from_bytes(encoded, "big", signed=False)
        if value > MAX_INTEGER:
            raise LedgerError("ledger watermark is out of range")
        return value

    def _require_recovered_watermark(self) -> int:
        watermark = self._read_watermark()
        if watermark is None:
            raise LedgerError("ledger watermark is absent after recovery")
        return watermark

    def _commit_watermark(self, now_ms: int) -> None:
        encoded = now_ms.to_bytes(WATERMARK_BYTES, "big", signed=False)
        try:
            _inspect_leaf(
                self._watermark_path,
                "ledger watermark",
                directory=False,
            )
        except LedgerError:
            try:
                os.lstat(self._watermark_path)
            except FileNotFoundError:
                self._create_watermark(encoded)
                self._observe_clock(now_ms)
                return
            except OSError as error:
                raise LedgerError("ledger watermark could not be inspected") from error
            raise

        temporary = self._watermark_path.with_name(
            f".ledger.watermark.tmp-{os.getpid()}-{threading.get_ident()}"
        )
        descriptor = None
        try:
            descriptor = os.open(
                temporary,
                os.O_WRONLY
                | os.O_CREAT
                | os.O_EXCL
                | getattr(os, "O_CLOEXEC", 0)
                | getattr(os, "O_NOFOLLOW", 0),
                0o600,
            )
            created = os.fstat(descriptor)
            if not stat.S_ISREG(created.st_mode) or _is_reparse_point(created):
                raise LedgerError("ledger watermark temporary file is irregular")
            _require_private_posix_file(created, "ledger watermark temporary file")
            with os.fdopen(descriptor, "wb", closefd=True) as handle:
                descriptor = None
                handle.write(encoded)
                handle.flush()
                os.fsync(handle.fileno())
            _inspect_leaf(
                temporary,
                "ledger watermark temporary file",
                directory=False,
            )
            self._assert_root_stable()
            _inspect_leaf(
                self._watermark_path,
                "ledger watermark",
                directory=False,
            )
            self._replace_write_through(temporary, self._watermark_path)
            _inspect_leaf(
                self._watermark_path,
                "ledger watermark",
                directory=False,
            )
            self._fsync_directory()
            self._observe_clock(now_ms)
        except (OSError, LedgerError) as error:
            raise LedgerError("ledger watermark could not be committed") from error
        finally:
            if descriptor is not None:
                os.close(descriptor)
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass

    def _create_watermark(self, encoded: bytes) -> None:
        try:
            descriptor = os.open(
                self._watermark_path,
                os.O_WRONLY
                | os.O_CREAT
                | os.O_EXCL
                | getattr(os, "O_CLOEXEC", 0)
                | getattr(os, "O_NOFOLLOW", 0),
                0o600,
            )
        except OSError as error:
            raise LedgerError("ledger watermark could not be created") from error
        try:
            with os.fdopen(descriptor, "wb", closefd=True) as handle:
                created = os.fstat(handle.fileno())
                if not stat.S_ISREG(created.st_mode) or _is_reparse_point(created):
                    raise LedgerError("ledger watermark is irregular")
                _require_private_posix_file(created, "ledger watermark")
                handle.write(encoded)
                handle.flush()
                os.fsync(handle.fileno())
            _inspect_leaf(
                self._watermark_path,
                "ledger watermark",
                directory=False,
            )
            self._fsync_directory()
        except (OSError, LedgerError) as error:
            try:
                self._watermark_path.unlink()
            except FileNotFoundError:
                pass
            raise LedgerError("ledger watermark could not be committed") from error

    def _require_global_clock(
        self,
        now_ms: int,
        records: list[tuple[Path, dict[str, Any]]] | None = None,
    ) -> list[tuple[Path, dict[str, Any]]]:
        active_records = records if records is not None else self._scan_records()
        observed = [
            record["updatedAtUnixMs"] for _, record in active_records
        ]
        watermark = self._read_watermark()
        if watermark is not None:
            observed.append(watermark)
        elif self._recovery_complete:
            raise LedgerError("ledger watermark is absent after recovery")
        latest = max(observed, default=None)
        if latest is not None and now_ms < latest:
            raise LedgerError("ledger clock predates another ledger record")
        return active_records

    def _path(self, challenge: str) -> Path:
        return self.root / f"{_challenge_hash(challenge)}.json"

    def record_path(self, challenge: str) -> Path:
        """Return the only admissible record path for ``challenge``."""

        return self._path(challenge)

    @contextlib.contextmanager
    def _locked(self, *, recovery_required: bool = True) -> Iterator[None]:
        if recovery_required:
            self._require_recovery()
        with self._thread_lock:
            with _platform_file_lock(self._lock_path, self._assert_root_stable):
                self._assert_root_stable()
                yield
                self._assert_root_stable()

    def _encode(self, record: dict[str, Any]) -> bytes:
        return canonical_json(_seal_record(record)) + b"\n"

    def _read_path(self, path: Path) -> dict[str, Any]:
        try:
            descriptor = _open_existing_regular(path, "ledger record", os.O_RDONLY)
            with os.fdopen(descriptor, "rb", closefd=True) as handle:
                raw = handle.read(MAX_RECORD_BYTES + 1)
        except (OSError, LedgerError) as error:
            raise LedgerError("ledger record could not be read") from error
        if not raw or len(raw) > MAX_RECORD_BYTES or not raw.endswith(b"\n"):
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
        try:
            canonical = canonical_json(record)
        except (UnicodeEncodeError, ValueError) as error:
            raise LedgerError("ledger record cannot be canonically encoded") from error
        if canonical + b"\n" != raw:
            raise LedgerError("ledger record is not canonical JSON")
        if type(record) is not dict or set(record) != RECORD_KEYS:
            raise LedgerError("ledger record fields are not exact")
        if type(record["version"]) is not int or record["version"] != LEDGER_VERSION:
            raise LedgerError("ledger record version is invalid")
        if type(record["challengeSha256"]) is not str or SHA256_RE.fullmatch(
            record["challengeSha256"]
        ) is None:
            raise LedgerError("ledger challenge hash is invalid")
        if path.name != f"{record['challengeSha256']}.json":
            raise LedgerError("ledger record is not bound to its request path")
        if type(record["state"]) is not str or record["state"] not in STATES:
            raise LedgerError("ledger state is unknown")
        for key in ("issuedAtUnixMs", "expiresAtUnixMs", "updatedAtUnixMs"):
            if (
                type(record[key]) is not int
                or not 0 <= record[key] <= MAX_INTEGER
            ):
                raise LedgerError(f"ledger {key} is invalid")
        for key in ("pipeBindingSha256", "receiptFrameSha256"):
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
        receipt_frame_sha256 = record["receiptFrameSha256"]
        if state == "issued" and (
            pipe_digest is not None or receipt_frame_sha256 is not None
        ):
            raise LedgerError("issued ledger fields are incoherent")
        if state == "connected" and (
            pipe_digest is None or receipt_frame_sha256 is not None
        ):
            raise LedgerError("connected ledger fields are incoherent")
        if state == "consumed" and (
            pipe_digest is None or receipt_frame_sha256 is None
        ):
            raise LedgerError("consumed ledger fields are incoherent")
        if state in {"expired", "abandoned"} and receipt_frame_sha256 is not None:
            raise LedgerError("terminal ledger fields are incoherent")
        if state == "expired":
            if updated < expires:
                raise LedgerError("expired ledger timestamp is incoherent")
        elif state == "issued":
            if updated != issued:
                raise LedgerError("issued ledger timestamp is incoherent")
        elif updated >= expires:
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
            self._require_recovered_watermark()
        if record["challengeSha256"] != expected:
            raise LedgerError("ledger record belongs to another challenge")
        return record

    def _atomic_replace(self, path: Path, record: dict[str, Any]) -> None:
        self._assert_root_stable()
        _inspect_leaf(path, "ledger record", directory=False)
        encoded = self._encode(record)
        temporary = path.with_name(
            f".{path.name}.tmp-{os.getpid()}-{threading.get_ident()}"
        )
        flags = (
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0)
        )
        descriptor = None
        try:
            descriptor = os.open(temporary, flags, 0o600)
            created = os.fstat(descriptor)
            if not stat.S_ISREG(created.st_mode) or _is_reparse_point(created):
                raise LedgerError("ledger temporary file is irregular")
            _require_private_posix_file(created, "ledger temporary file")
            with os.fdopen(descriptor, "wb", closefd=True) as handle:
                descriptor = None
                handle.write(encoded)
                handle.flush()
                os.fsync(handle.fileno())
            _inspect_leaf(temporary, "ledger temporary file", directory=False)
            self._assert_root_stable()
            _inspect_leaf(path, "ledger record", directory=False)
            self._replace_write_through(temporary, path)
            _inspect_leaf(path, "ledger record", directory=False)
            self._fsync_directory()
        except (OSError, LedgerError) as error:
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
        self._assert_root_stable()
        descriptor = os.open(
            self.root,
            os.O_RDONLY
            | getattr(os, "O_DIRECTORY", 0)
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0),
        )
        try:
            opened = os.fstat(descriptor)
            if (
                not stat.S_ISDIR(opened.st_mode)
                or _file_identity(opened) != self._root_identity
            ):
                raise LedgerError("ledger root identity changed while syncing")
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    def issue(self, challenge: str, now_ms: int, ttl_ms: int = MAX_TTL_MS) -> None:
        challenge_hash = _challenge_hash(challenge)
        self._check_clock(now_ms, "issue")
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
            "receiptFrameSha256": None,
            "updatedAtUnixMs": now_ms,
            "recordDigestSha256": "",
        }
        path = self._path(challenge)
        with self._locked():
            self._check_clock(now_ms, "issue")
            self._require_global_clock(now_ms)
            try:
                os.lstat(path)
            except FileNotFoundError:
                pass
            except OSError as error:
                raise LedgerError("challenge path could not be inspected") from error
            else:
                _inspect_leaf(path, "ledger record", directory=False)
                raise LedgerError("challenge was already issued")
            try:
                descriptor = os.open(
                    path,
                    os.O_WRONLY
                    | os.O_CREAT
                    | os.O_EXCL
                    | getattr(os, "O_CLOEXEC", 0)
                    | getattr(os, "O_NOFOLLOW", 0),
                    0o600,
                )
            except FileExistsError as error:
                raise LedgerError("challenge was already issued") from error
            try:
                with os.fdopen(descriptor, "wb", closefd=True) as handle:
                    created = os.fstat(handle.fileno())
                    if not stat.S_ISREG(created.st_mode) or _is_reparse_point(
                        created
                    ):
                        raise LedgerError("ledger record is irregular")
                    _require_private_posix_file(created, "ledger record")
                    handle.write(self._encode(record))
                    handle.flush()
                    os.fsync(handle.fileno())
                _inspect_leaf(path, "ledger record", directory=False)
                self._fsync_directory()
                self._commit_watermark(now_ms)
            except (OSError, LedgerError) as error:
                try:
                    path.unlink()
                except FileNotFoundError:
                    pass
                raise LedgerError("challenge issuance could not be committed") from error

    def _load_for_transition(
        self, challenge: str, now_ms: int
    ) -> tuple[Path, dict[str, Any]]:
        self._check_clock(now_ms, "transition")
        self._require_global_clock(now_ms)
        expected = _challenge_hash(challenge)
        path = self._path(challenge)
        record = self._read_path(path)
        if record["challengeSha256"] != expected:
            raise LedgerError("ledger record belongs to another challenge")
        if now_ms < record["updatedAtUnixMs"]:
            raise LedgerError("ledger clock predates the record")
        if now_ms >= record["expiresAtUnixMs"]:
            if record["state"] not in TERMINAL_STATES:
                record["state"] = "expired"
                record["updatedAtUnixMs"] = now_ms
                self._atomic_replace(path, record)
                self._commit_watermark(now_ms)
            raise LedgerError("challenge expired")
        return path, record

    def connect(
        self,
        challenge: str,
        pipe_client_binding: dict[str, Any],
        now_ms: int,
    ) -> str:
        digest = pipe_binding_digest(pipe_client_binding)
        with self._locked():
            path, record = self._load_for_transition(challenge, now_ms)
            if record["state"] != "issued":
                raise LedgerError("challenge is not available for connection")
            record["state"] = "connected"
            record["pipeBindingSha256"] = digest
            record["updatedAtUnixMs"] = now_ms
            self._atomic_replace(path, record)
            self._commit_watermark(now_ms)
        return digest

    def consume(
        self,
        challenge: str,
        pipe_client_binding: dict[str, Any],
        receipt_frame_sha256: str,
        now_ms: int,
    ) -> None:
        if (
            type(receipt_frame_sha256) is not str
            or SHA256_RE.fullmatch(receipt_frame_sha256) is None
        ):
            raise LedgerError("receipt frame digest is invalid")
        expected_binding = pipe_binding_digest(pipe_client_binding)
        with self._locked():
            path, record = self._load_for_transition(challenge, now_ms)
            if record["state"] != "connected":
                raise LedgerError("challenge is not connected or was already consumed")
            if not hmac.compare_digest(
                record["pipeBindingSha256"] or "", expected_binding
            ):
                raise LedgerError("pipe client changed after connection")
            record["state"] = "consumed"
            record["receiptFrameSha256"] = receipt_frame_sha256
            record["updatedAtUnixMs"] = now_ms
            self._atomic_replace(path, record)
            self._commit_watermark(now_ms)

    def recover_incomplete(self, now_ms: int) -> int:
        self._check_clock(now_ms, "recovery")
        if self._recovery_complete:
            with self._locked(recovery_required=False):
                self._require_global_clock(now_ms)
                self._commit_watermark(now_ms)
            return 0
        changed = 0
        with self._locked(recovery_required=False):
            self._check_clock(now_ms, "recovery")
            records = self._require_global_clock(now_ms)
            for path, record in records:
                if record["state"] in TERMINAL_STATES:
                    continue
                record["state"] = (
                    "expired" if now_ms >= record["expiresAtUnixMs"] else "abandoned"
                )
                record["updatedAtUnixMs"] = now_ms
                self._atomic_replace(path, record)
                changed += 1
            self._commit_watermark(now_ms)
            self._recovery_complete = True
        return changed
