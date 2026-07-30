"""Verifier-side C4 native receipt v3 authority checks.

The pipe server is intentionally out of scope here. Its independently observed
client process facts are mandatory inputs to this verifier; caller-authored
receipt fields never substitute for them.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
import hashlib
import hmac
from os import PathLike
import os
import re
from typing import Any, Literal

from ledger import LedgerError, OneShotLedger, pipe_binding_digest
from schema import (
    MAX_RECEIPT_BYTES,
    PROCESS_KEYS,
    SHA256_RE,
    SchemaError,
    TARGET_KEYS,
    canonical_json,
    parse_receipt,
    sha256_hex,
    validate_process_binding,
    validate_target,
)


EvidenceSource = Literal["synthetic", "runtime_named_pipe"]
FILESYSTEM_ATTESTATION_KEYS = {
    "schema",
    "version",
    "authoritySource",
    "challenge",
    "checkedAtUnixMs",
    "ledgerRoot",
    "ledgerRecordPath",
    "rootIdentity",
    "daclSha256",
    "ownerSidSha256",
}
ROOT_IDENTITY_KEYS = {"volumeSerialNumber", "fileIndex"}
WINDOWS_DIRECTORY_RE = re.compile(
    r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[^\\/:*?\"<>|\x00]+$"
)
WINDOWS_FILE_RE = re.compile(
    r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[^\\/:*?\"<>|\x00]+\.json$"
)
WINDOWS_RESERVED_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{index}" for index in range(1, 10)),
    *(f"LPT{index}" for index in range(1, 10)),
}


class VerificationError(ValueError):
    """Evidence did not meet the v3 native-authority acceptance contract."""


class _FilesystemNativeApi:
    """Tiny ctypes boundary for the Windows file facts this verifier trusts."""

    def __init__(self) -> None:
        if os.name != "nt":
            raise OSError("native filesystem authority requires Windows")
        import ctypes
        from ctypes import wintypes

        self._ctypes = ctypes
        self._wintypes = wintypes
        self._kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self._advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)

        class Filetime(ctypes.Structure):
            _fields_ = [
                ("dwLowDateTime", wintypes.DWORD),
                ("dwHighDateTime", wintypes.DWORD),
            ]

        class ByHandleFileInformation(ctypes.Structure):
            _fields_ = [
                ("dwFileAttributes", wintypes.DWORD),
                ("ftCreationTime", Filetime),
                ("ftLastAccessTime", Filetime),
                ("ftLastWriteTime", Filetime),
                ("dwVolumeSerialNumber", wintypes.DWORD),
                ("nFileSizeHigh", wintypes.DWORD),
                ("nFileSizeLow", wintypes.DWORD),
                ("nNumberOfLinks", wintypes.DWORD),
                ("nFileIndexHigh", wintypes.DWORD),
                ("nFileIndexLow", wintypes.DWORD),
            ]

        class Acl(ctypes.Structure):
            _fields_ = [
                ("AclRevision", wintypes.BYTE),
                ("Sbz1", wintypes.BYTE),
                ("AclSize", wintypes.WORD),
                ("AceCount", wintypes.WORD),
                ("Sbz2", wintypes.WORD),
            ]

        class SidAndAttributes(ctypes.Structure):
            _fields_ = [
                ("Sid", wintypes.LPVOID),
                ("Attributes", wintypes.DWORD),
            ]

        class TokenUser(ctypes.Structure):
            _fields_ = [("User", SidAndAttributes)]

        self._ByHandleFileInformation = ByHandleFileInformation
        self._Acl = Acl
        self._TokenUser = TokenUser

        self._kernel32.CreateFileW.argtypes = [
            wintypes.LPCWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPVOID,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.HANDLE,
        ]
        self._kernel32.CreateFileW.restype = wintypes.HANDLE
        self._kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
        self._kernel32.CloseHandle.restype = wintypes.BOOL
        self._kernel32.GetFileAttributesW.argtypes = [wintypes.LPCWSTR]
        self._kernel32.GetFileAttributesW.restype = wintypes.DWORD
        self._kernel32.GetFileInformationByHandle.argtypes = [
            wintypes.HANDLE,
            ctypes.POINTER(ByHandleFileInformation),
        ]
        self._kernel32.GetFileInformationByHandle.restype = wintypes.BOOL
        self._kernel32.GetFinalPathNameByHandleW.argtypes = [
            wintypes.HANDLE,
            wintypes.LPWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
        ]
        self._kernel32.GetFinalPathNameByHandleW.restype = wintypes.DWORD
        self._kernel32.GetCurrentProcess.argtypes = []
        self._kernel32.GetCurrentProcess.restype = wintypes.HANDLE
        self._kernel32.LocalFree.argtypes = [wintypes.HLOCAL]
        self._kernel32.LocalFree.restype = wintypes.HLOCAL

        self._advapi32.GetNamedSecurityInfoW.argtypes = [
            wintypes.LPWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.LPVOID),
            ctypes.POINTER(wintypes.LPVOID),
            ctypes.POINTER(wintypes.LPVOID),
            ctypes.POINTER(wintypes.LPVOID),
            ctypes.POINTER(wintypes.LPVOID),
        ]
        self._advapi32.GetNamedSecurityInfoW.restype = wintypes.DWORD
        self._advapi32.GetLengthSid.argtypes = [wintypes.LPVOID]
        self._advapi32.GetLengthSid.restype = wintypes.DWORD
        self._advapi32.GetSecurityDescriptorControl.argtypes = [
            wintypes.LPVOID,
            ctypes.POINTER(wintypes.WORD),
            ctypes.POINTER(wintypes.DWORD),
        ]
        self._advapi32.GetSecurityDescriptorControl.restype = wintypes.BOOL
        self._advapi32.OpenProcessToken.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.HANDLE),
        ]
        self._advapi32.OpenProcessToken.restype = wintypes.BOOL
        self._advapi32.GetTokenInformation.argtypes = [
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.LPVOID,
            wintypes.DWORD,
            ctypes.POINTER(wintypes.DWORD),
        ]
        self._advapi32.GetTokenInformation.restype = wintypes.BOOL

    def _raise_last_error(self) -> None:
        raise self._ctypes.WinError(self._ctypes.get_last_error())

    def _hash(self, value: bytes) -> str:
        return hashlib.sha256(value).hexdigest()

    def _sid_hash(self, sid: object) -> str:
        length = self._advapi32.GetLengthSid(sid)
        if length == 0:
            self._raise_last_error()
        return self._hash(self._ctypes.string_at(sid, length))

    def _assert_not_reparse_point(self, path: str) -> None:
        invalid_file_attributes = 0xFFFFFFFF
        file_attribute_reparse_point = 0x00000400
        attributes = self._kernel32.GetFileAttributesW(path)
        if attributes == invalid_file_attributes:
            self._raise_last_error()
        if attributes & file_attribute_reparse_point:
            raise OSError("filesystem authority path is a reparse point")

    def _open_path(self, path: str, *, directory: bool) -> object:
        self._assert_not_reparse_point(path)
        desired_access = 0x80 | 0x00020000  # FILE_READ_ATTRIBUTES | READ_CONTROL
        share_all = 0x00000001 | 0x00000002 | 0x00000004
        open_existing = 3
        file_flag_backup_semantics = 0x02000000
        flags = file_flag_backup_semantics if directory else 0
        handle = self._kernel32.CreateFileW(
            path,
            desired_access,
            share_all,
            None,
            open_existing,
            flags,
            None,
        )
        if handle == self._wintypes.HANDLE(-1).value:
            self._raise_last_error()
        return handle

    def _close(self, handle: object) -> None:
        if not self._kernel32.CloseHandle(handle):
            self._raise_last_error()

    def _final_path(self, handle: object) -> str:
        buffer_len = 32768
        buffer = self._ctypes.create_unicode_buffer(buffer_len)
        volume_name_dos = 0
        written = self._kernel32.GetFinalPathNameByHandleW(
            handle,
            buffer,
            buffer_len,
            volume_name_dos,
        )
        if written == 0 or written >= buffer_len:
            self._raise_last_error()
        value = buffer.value
        if value.startswith("\\\\?\\"):
            value = value[4:]
        return value

    def file_identity(self, path: str, *, directory: bool) -> dict[str, int]:
        handle = self._open_path(path, directory=directory)
        try:
            final_path = self._final_path(handle)
            if not hmac.compare_digest(final_path.casefold(), path.casefold()):
                raise OSError("filesystem authority path resolves through an alias")
            information = self._ByHandleFileInformation()
            if not self._kernel32.GetFileInformationByHandle(
                handle,
                self._ctypes.byref(information),
            ):
                self._raise_last_error()
            file_index = (
                int(information.nFileIndexHigh) << 32
            ) | int(information.nFileIndexLow)
            return {
                "volumeSerialNumber": int(information.dwVolumeSerialNumber),
                "fileIndex": file_index,
            }
        finally:
            self._close(handle)

    def record_is_regular_child(self, root: str, record_path: str) -> bool:
        root_handle = self._open_path(root, directory=True)
        try:
            root_final = self._final_path(root_handle).casefold()
        finally:
            self._close(root_handle)
        record_handle = self._open_path(record_path, directory=False)
        try:
            record_final = self._final_path(record_handle).casefold()
        finally:
            self._close(record_handle)
        return (
            hmac.compare_digest(root_final, root.casefold())
            and hmac.compare_digest(record_final, record_path.casefold())
            and record_final.startswith(root_final.rstrip("\\") + "\\")
        )

    def security_hashes(self, path: str) -> tuple[str, str, bool]:
        ctypes = self._ctypes
        owner = self._wintypes.LPVOID()
        dacl = self._wintypes.LPVOID()
        descriptor = self._wintypes.LPVOID()
        se_file_object = 1
        owner_security_information = 0x00000001
        dacl_security_information = 0x00000004
        error = self._advapi32.GetNamedSecurityInfoW(
            path,
            se_file_object,
            owner_security_information | dacl_security_information,
            ctypes.byref(owner),
            None,
            ctypes.byref(dacl),
            None,
            ctypes.byref(descriptor),
        )
        if error != 0:
            raise OSError(error, "filesystem authority security descriptor failed")
        try:
            if not owner or not dacl or not descriptor:
                raise OSError("filesystem authority owner or DACL is absent")
            acl = ctypes.cast(dacl, ctypes.POINTER(self._Acl)).contents
            if acl.AclSize <= 0:
                raise OSError("filesystem authority DACL is empty")
            control = self._wintypes.WORD()
            revision = self._wintypes.DWORD()
            if not self._advapi32.GetSecurityDescriptorControl(
                descriptor,
                ctypes.byref(control),
                ctypes.byref(revision),
            ):
                self._raise_last_error()
            se_dacl_protected = 0x1000
            return (
                self._hash(ctypes.string_at(dacl, int(acl.AclSize))),
                self._sid_hash(owner),
                bool(int(control.value) & se_dacl_protected),
            )
        finally:
            self._kernel32.LocalFree(descriptor)

    def current_user_sid_sha256(self) -> str:
        ctypes = self._ctypes
        token_query = 0x0008
        token_user = 1
        token = self._wintypes.HANDLE()
        if not self._advapi32.OpenProcessToken(
            self._kernel32.GetCurrentProcess(),
            token_query,
            ctypes.byref(token),
        ):
            self._raise_last_error()
        try:
            needed = self._wintypes.DWORD()
            self._advapi32.GetTokenInformation(
                token,
                token_user,
                None,
                0,
                ctypes.byref(needed),
            )
            if needed.value <= 0:
                self._raise_last_error()
            buffer = ctypes.create_string_buffer(int(needed.value))
            if not self._advapi32.GetTokenInformation(
                token,
                token_user,
                buffer,
                needed.value,
                ctypes.byref(needed),
            ):
                self._raise_last_error()
            user = ctypes.cast(
                buffer,
                ctypes.POINTER(self._TokenUser),
            ).contents.User
            return self._sid_hash(user.Sid)
        finally:
            self._close(token)


class FilesystemAuthorityVerifier:
    """Verify native-owned C4 ledger authority with Windows filesystem APIs.

    Caller JSON is treated only as a claim. This verifier returns ``True`` only
    when native Windows calls independently reproduce the ledger root identity,
    the root owner SID digest, the protected DACL digest, and the exact record
    path under that root. Non-Windows hosts and missing native facts refuse.
    """

    def __init__(self, native_api: object | None = None) -> None:
        self._native_api = native_api

    def verify_filesystem_authority(
        self,
        *,
        ledger_root: PathLike[str],
        ledger_record_path: PathLike[str],
        attestation: dict[str, Any],
        challenge: str,
        expected_emitter: dict[str, Any],
        now_unix_ms: int,
    ) -> bool:
        """Return the literal boolean True only for a native-proven binding."""

        try:
            if self._native_api is None and os.name != "nt":
                return False
            if (
                type(now_unix_ms) is not int
                or not 0 <= now_unix_ms <= (1 << 63) - 1
            ):
                return False
            if (
                type(expected_emitter) is not dict
                or set(expected_emitter) != PROCESS_KEYS
            ):
                return False
            validate_process_binding(expected_emitter, "expectedEmitter")
            if (
                type(attestation) is not dict
                or set(attestation) != FILESYSTEM_ATTESTATION_KEYS
            ):
                return False
            if (
                attestation["schema"] != "osl.c4.filesystem-authority-attestation"
                or attestation["version"] != 3
                or attestation["authoritySource"] != "native_windows_owner"
                or type(attestation["checkedAtUnixMs"]) is not int
                or attestation["checkedAtUnixMs"] > now_unix_ms
            ):
                return False
            if type(challenge) is not str or not hmac.compare_digest(
                attestation["challenge"],
                challenge,
            ):
                return False
            root = _validate_windows_path(str(ledger_root), "ledger root", file=False)
            record_path = _validate_windows_path(
                str(ledger_record_path),
                "ledger record path",
                file=True,
            )
            if not hmac.compare_digest(attestation["ledgerRoot"], root):
                return False
            if not hmac.compare_digest(attestation["ledgerRecordPath"], record_path):
                return False
            if not record_path.startswith(root + "\\"):
                return False
            if type(attestation["rootIdentity"]) is not dict or set(
                attestation["rootIdentity"]
            ) != ROOT_IDENTITY_KEYS:
                return False
            for key in ROOT_IDENTITY_KEYS:
                if (
                    type(attestation["rootIdentity"][key]) is not int
                    or not 1 <= attestation["rootIdentity"][key] <= (1 << 63) - 1
                ):
                    return False
            for key in ("daclSha256", "ownerSidSha256"):
                if (
                    type(attestation[key]) is not str
                    or SHA256_RE.fullmatch(attestation[key]) is None
                ):
                    return False

            native_api = self._native_api or _FilesystemNativeApi()
            root_identity = native_api.file_identity(root, directory=True)
            if root_identity != attestation["rootIdentity"]:
                return False
            if not native_api.record_is_regular_child(root, record_path):
                return False
            dacl_sha256, owner_sid_sha256, dacl_protected = (
                native_api.security_hashes(root)
            )
            if dacl_protected is not True:
                return False
            if not hmac.compare_digest(dacl_sha256, attestation["daclSha256"]):
                return False
            if not hmac.compare_digest(owner_sid_sha256, attestation["ownerSidSha256"]):
                return False
            if not hmac.compare_digest(
                native_api.current_user_sid_sha256(),
                owner_sid_sha256,
            ):
                return False
        except (OSError, SchemaError, VerificationError, TypeError, ValueError):
            return False
        return True


@dataclass(frozen=True)
class VerificationContext:
    challenge: str
    issued_at_unix_ms: int
    now_unix_ms: int
    expires_at_unix_ms: int
    source: EvidenceSource
    expected_emitter: dict[str, Any]
    pipe_client: dict[str, Any]
    expected_target: dict[str, Any]
    pipe_first_instance: bool
    pipe_remote_clients_rejected: bool
    pipe_acl_exact_user: bool
    filesystem_authority_attestation: dict[str, Any] | None = None


@dataclass(frozen=True)
class VerificationVerdict:
    status: str
    parser_crypto_valid: bool
    runtime_receipt_accepted: bool
    full_c4_success: bool
    point_delta: int
    receipt_frame_sha256: str


def _normalized_process(process: dict[str, Any]) -> dict[str, Any]:
    normalized = copy.deepcopy(process)
    normalized["executablePath"] = normalized["executablePath"].casefold()
    return normalized


def _process_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    return hmac.compare_digest(
        canonical_json(_normalized_process(left)),
        canonical_json(_normalized_process(right)),
    )


def _target_identity(target: dict[str, Any]) -> dict[str, Any]:
    return {key: target[key] for key in TARGET_KEYS if key != "bindingSha256"}


def _target_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    normalized_left = _target_identity(left)
    normalized_right = _target_identity(right)
    normalized_left["executablePath"] = normalized_left["executablePath"].casefold()
    normalized_right["executablePath"] = normalized_right["executablePath"].casefold()
    return hmac.compare_digest(
        canonical_json(normalized_left),
        canonical_json(normalized_right),
    )


def _validate_filesystem_attestation(
    attestation: Any,
    context: VerificationContext,
) -> dict[str, Any]:
    if type(attestation) is not dict or set(attestation) != FILESYSTEM_ATTESTATION_KEYS:
        raise VerificationError("filesystem attestation fields are not exact")
    if (
        attestation["schema"] != "osl.c4.filesystem-authority-attestation"
        or type(attestation["schema"]) is not str
        or attestation["version"] != 3
        or type(attestation["version"]) is not int
        or attestation["authoritySource"] != "native_windows_owner"
        or type(attestation["authoritySource"]) is not str
    ):
        raise VerificationError("filesystem attestation identity is invalid")
    if (
        type(attestation["challenge"]) is not str
        or not hmac.compare_digest(attestation["challenge"], context.challenge)
    ):
        raise VerificationError("filesystem attestation challenge is invalid")
    checked = attestation["checkedAtUnixMs"]
    if type(checked) is not int or not (
        context.issued_at_unix_ms <= checked <= context.now_unix_ms
    ):
        raise VerificationError("filesystem attestation time is invalid")
    root = _validate_windows_path(
        attestation["ledgerRoot"],
        "filesystem attestation root",
        file=False,
    )
    record_path = _validate_windows_path(
        attestation["ledgerRecordPath"],
        "filesystem attestation record path",
        file=True,
    )
    if not record_path.startswith(root + "\\"):
        raise VerificationError("filesystem attestation record escapes its ledger root")
    identity = attestation["rootIdentity"]
    if type(identity) is not dict or set(identity) != ROOT_IDENTITY_KEYS:
        raise VerificationError("filesystem root identity fields are not exact")
    for key in ROOT_IDENTITY_KEYS:
        if (
            type(identity[key]) is not int
            or not 1 <= identity[key] <= (1 << 63) - 1
        ):
            raise VerificationError("filesystem root identity is invalid")
    for key in ("daclSha256", "ownerSidSha256"):
        if (
            type(attestation[key]) is not str
            or SHA256_RE.fullmatch(attestation[key]) is None
        ):
            raise VerificationError(f"filesystem attestation {key} is invalid")
    return attestation


def _validate_windows_path(value: Any, label: str, *, file: bool) -> str:
    pattern = WINDOWS_FILE_RE if file else WINDOWS_DIRECTORY_RE
    if (
        type(value) is not str
        or not value.isascii()
        or not value.isprintable()
        or pattern.fullmatch(value) is None
        or "/" in value
        or "\\\\" in value
    ):
        raise VerificationError(f"{label} is invalid")
    parts = value[3:].split("\\")
    if not parts or any(
        not part
        or part in {".", ".."}
        or part.endswith((".", " "))
        or part.split(".", 1)[0].upper() in WINDOWS_RESERVED_NAMES
        for part in parts
    ):
        raise VerificationError(f"{label} is not lexically canonical")
    return value


def _validate_context(context: VerificationContext) -> None:
    if type(context) is not VerificationContext:
        raise VerificationError("verification context is invalid")
    if type(context.challenge) is not str or SHA256_RE.fullmatch(context.challenge) is None:
        raise VerificationError("expected challenge is invalid")
    if context.challenge == "0" * 64:
        raise VerificationError("zero expected challenge is forbidden")
    for label, value in (
        ("issued_at_unix_ms", context.issued_at_unix_ms),
        ("now_unix_ms", context.now_unix_ms),
        ("expires_at_unix_ms", context.expires_at_unix_ms),
    ):
        if type(value) is not int or not 0 <= value <= (1 << 63) - 1:
            raise VerificationError(f"{label} is invalid")
    if not (
        context.issued_at_unix_ms
        <= context.now_unix_ms
        < context.expires_at_unix_ms
        <= context.issued_at_unix_ms + 60_000
    ):
        raise VerificationError("challenge lifetime is invalid")
    if context.source not in ("synthetic", "runtime_named_pipe"):
        raise VerificationError("evidence source is unknown")
    if type(context.expected_emitter) is not dict or set(
        context.expected_emitter
    ) != PROCESS_KEYS:
        raise VerificationError("expected emitter fields are not exact")
    if type(context.pipe_client) is not dict or set(context.pipe_client) != PROCESS_KEYS:
        raise VerificationError("pipe client fields are not exact")
    if type(context.expected_target) is not dict or set(
        context.expected_target
    ) != TARGET_KEYS:
        raise VerificationError("expected target fields are not exact")
    try:
        validate_process_binding(context.expected_emitter, "expectedEmitter")
        validate_process_binding(context.pipe_client, "pipeClient")
        validate_target(context.expected_target)
    except SchemaError as error:
        raise VerificationError("independent process binding is invalid") from error
    for label, value in (
        ("pipe_first_instance", context.pipe_first_instance),
        ("pipe_remote_clients_rejected", context.pipe_remote_clients_rejected),
        ("pipe_acl_exact_user", context.pipe_acl_exact_user),
    ):
        if type(value) is not bool:
            raise VerificationError(f"{label} must be a literal boolean")
    if context.source == "synthetic":
        if context.filesystem_authority_attestation is not None:
            raise VerificationError("synthetic evidence may not carry native authority")
    else:
        if not (
            context.pipe_first_instance is True
            and context.pipe_remote_clients_rejected is True
            and context.pipe_acl_exact_user is True
        ):
            raise VerificationError(
                "runtime pipe protections were not independently proven"
            )
        _validate_filesystem_attestation(
            context.filesystem_authority_attestation,
            context,
        )


def verify_receipt(
    encoded: bytes,
    context: VerificationContext,
    *,
    ledger: OneShotLedger | None = None,
    filesystem_authority_verifier: FilesystemAuthorityVerifier | None = None,
) -> VerificationVerdict:
    """Validate one raw frame.

    Synthetic input can prove only parser/digest behavior. Even a byte-perfect
    synthetic receipt is never returned as runtime or full-C4 success.
    """

    _validate_context(context)
    if type(encoded) is not bytes or not encoded or len(encoded) > MAX_RECEIPT_BYTES:
        raise VerificationError("receipt frame length is invalid")
    try:
        receipt = parse_receipt(encoded)
    except ValueError as error:
        raise VerificationError(str(error)) from error

    if not hmac.compare_digest(receipt["challenge"], context.challenge):
        raise VerificationError("receipt challenge does not match the issued challenge")
    emitted = receipt["emittedAtUnixMs"]
    if not context.issued_at_unix_ms <= emitted <= context.now_unix_ms:
        raise VerificationError("receipt was emitted outside the challenge lifetime")
    monotonic_duration = receipt["monotonicStagesMs"]["receiptEmitted"]
    if monotonic_duration > emitted - context.issued_at_unix_ms:
        raise VerificationError("native monotonic duration exceeds elapsed wall time")
    if not _process_equal(receipt["emitter"], context.expected_emitter):
        raise VerificationError("receipt emitter does not match the expected executable")
    if not _process_equal(context.pipe_client, context.expected_emitter):
        raise VerificationError("named-pipe client is not the expected emitter process")
    if not _process_equal(receipt["emitter"], context.pipe_client):
        raise VerificationError("receipt emitter is not the OS-observed pipe client")
    if not _target_equal(receipt["target"], context.expected_target):
        raise VerificationError("Discord target does not match the independent target binding")

    raw_digest = sha256_hex(encoded)
    if context.source == "synthetic":
        if ledger is not None or filesystem_authority_verifier is not None:
            raise VerificationError("synthetic evidence may not carry runtime authority")
        return VerificationVerdict(
            status="synthetic-parser-crypto-valid",
            parser_crypto_valid=True,
            runtime_receipt_accepted=False,
            full_c4_success=False,
            point_delta=0,
            receipt_frame_sha256=raw_digest,
        )

    if ledger is None:
        raise VerificationError("runtime receipt requires the one-shot authority ledger")
    if not ledger.recovery_complete:
        raise VerificationError("runtime ledger recovery was not completed")
    try:
        ledger_record = ledger.read(context.challenge)
    except LedgerError as error:
        raise VerificationError("runtime ledger challenge could not be bound") from error
    if (
        ledger_record["issuedAtUnixMs"] != context.issued_at_unix_ms
        or ledger_record["expiresAtUnixMs"] != context.expires_at_unix_ms
    ):
        raise VerificationError("runtime request window differs from the ledger")
    if ledger_record["state"] != "connected":
        raise VerificationError("runtime ledger challenge is not connected")
    if ledger_record["updatedAtUnixMs"] > context.now_unix_ms:
        raise VerificationError("runtime ledger transition is in the future")
    try:
        expected_pipe_binding = pipe_binding_digest(context.pipe_client)
    except LedgerError as error:
        raise VerificationError("runtime pipe binding is invalid") from error
    if not hmac.compare_digest(
        ledger_record["pipeBindingSha256"] or "",
        expected_pipe_binding,
    ):
        raise VerificationError("runtime request pipe differs from the ledger")
    if filesystem_authority_verifier is None:
        raise VerificationError(
            "runtime receipt requires a native filesystem-authority verifier"
        )
    attestation = _validate_filesystem_attestation(
        context.filesystem_authority_attestation,
        context,
    )
    record_path = ledger.record_path(context.challenge)
    if attestation["ledgerRoot"] != str(ledger.root):
        raise VerificationError("filesystem attestation names another ledger root")
    if attestation["ledgerRecordPath"] != str(record_path):
        raise VerificationError("filesystem attestation names another request path")
    if attestation["checkedAtUnixMs"] < emitted:
        raise VerificationError("filesystem authority predates receipt emission")
    try:
        authority_proven = (
            filesystem_authority_verifier.verify_filesystem_authority(
                ledger_root=ledger.root,
                ledger_record_path=record_path,
                attestation=copy.deepcopy(attestation),
                challenge=context.challenge,
                expected_emitter=copy.deepcopy(context.expected_emitter),
                now_unix_ms=context.now_unix_ms,
            )
        )
    except Exception as error:
        raise VerificationError("native filesystem authority could not be proven") from error
    if type(authority_proven) is not bool or authority_proven is not True:
        raise VerificationError("native filesystem authority was not proven")
    try:
        ledger.consume(
            context.challenge,
            context.pipe_client,
            raw_digest,
            context.now_unix_ms,
        )
    except LedgerError as error:
        raise VerificationError(str(error)) from error
    return VerificationVerdict(
        status="runtime-native-receipt-valid",
        parser_crypto_valid=True,
        runtime_receipt_accepted=True,
        # Screenshot, transcript and cleanup evidence remain separate C4 gates.
        full_c4_success=False,
        point_delta=0,
        receipt_frame_sha256=raw_digest,
    )
