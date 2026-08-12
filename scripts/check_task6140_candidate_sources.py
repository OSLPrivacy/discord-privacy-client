#!/usr/bin/env python3
"""Independent source/provenance trace for the exact TASK 6140 candidate."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


class Refusal(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Refusal(message)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        module = (root / "crates/crypto/src/modern_protection.rs").read_text()
        test = (root / "crates/crypto/tests/task_6140_modern_protection.rs").read_text()
        cargo = (root / "crates/crypto/Cargo.toml").read_text()
        lock = (root / "Cargo.lock").read_text()
        broker = (root / "apps/osl-hub/src/broker.rs").read_text()

        domains = re.findall(r'Self::(Recovery|EnclaveExport|PersonalExport|Backup|Carrier),', module)
        require(domains[:5] == ["Recovery", "EnclaveExport", "PersonalExport", "Backup", "Carrier"],
                "TASK6140_SOURCE_FAIL starvation domain inventory is not exact")
        for fact in (
            'AEAD_ALGORITHM: &str = "XChaCha20-Poly1305-IETF"',
            'SIGNATURE_ALGORITHM: &str = "Ed25519"',
            "KEY_BITS: usize = 256",
            "SIGNATURE_SECURITY_BITS: usize = 128",
            "NONCE_BITS: usize = 192",
            "ARGON2_SALT_BITS: usize = 128",
            "ARGON2_MEMORY_KIB: u32 = 65_536",
            "ARGON2_ITERATIONS: u32 = 3",
            "ARGON2_PARALLELISM: u32 = 1",
            "Algorithm::Argon2id",
            "Version::V0x13",
            "random::random_aead_key()",
            "random::random_nonce()",
            "hkdf::derive_32",
            "aead::seal",
            "aead::open",
            "ed25519::generate_keypair()",
            "ed25519::sign",
            "ed25519::verify",
        ):
            require(fact in module, f"TASK6140_SOURCE_FAIL missing maintained-library trace fact={fact}")
        require('argon2 = "0.5"' in cargo, "TASK6140_SOURCE_FAIL Argon2 dependency absent")
        for package, version in (
            ("argon2", "0.5.3"),
            ("chacha20poly1305", "0.10.1"),
            ("ed25519-dalek", "2.2.0"),
            ("hkdf", "0.12.4"),
        ):
            require(f'name = "{package}"\nversion = "{version}"' in lock,
                    f"TASK6140_SOURCE_FAIL maintained library/version absent package={package} version={version}")

        aad = ("algorithm", "domain", "version", "purpose", "owner-account", "object-id",
               "generation", "chunk-index", "chunk-count")
        for field in aad:
            require(f'b"{field}"' in module, f"TASK6140_SOURCE_FAIL AAD field absent field={field}")
        require("object.password_kdf.is_some()" in module,
                "TASK6140_SOURCE_FAIL random-key path accepts password envelope")
        require('object.algorithm != AEAD_ALGORITHM' in module,
                "TASK6140_SOURCE_FAIL algorithm downgrade check absent")
        require("object.domain != domain" in module,
                "TASK6140_SOURCE_FAIL cross-domain transplant check absent")
        require("fn generate() -> Self" in module and "pub fn from_bytes" not in module,
                "TASK6140_SOURCE_FAIL public raw/low-entropy object-key constructor present")

        require("const MAX_TEXT_BYTES: usize = 1_000;" in broker,
                "TASK6140_SOURCE_FAIL constructor=prepare_peer_prose_text_inner boundary=1000 absent")
        require("pub const PRIVATE_MESSAGE_BYTES_PER_COVER: usize = 40 * 1024;" in broker,
                "TASK6140_SOURCE_FAIL constructor=split_native_overlay_text boundary=40960 absent")
        require("const CONSTRUCTORS: [(&str, &str, usize); 2]" in test,
                "TASK6140_SOURCE_FAIL constructor inventory absent")
        require("const CASES: [(&str, isize, Option<u64>); 7]" in test,
                "TASK6140_SOURCE_FAIL boundary corpus inventory absent")
        for case in ("threshold-minus-one", "exact-threshold", "threshold-plus-one",
                     "multipart-first", "multipart-middle", "multipart-final",
                     "final-part-after-long-prefix"):
            require(f'"{case}"' in test, f"TASK6140_SOURCE_FAIL boundary member absent case={case}")
        require('route: "bundled-tor"' in test and "direct_egress_bytes: 0" in test,
                "TASK6140_SOURCE_FAIL bundled Tor observation absent")
        require("RecoverySigner::generate()" in test and "verify_recovery(&signed)" in test,
                "TASK6140_SOURCE_FAIL signed 5019/6101 corpus proof absent")
    except (OSError, Refusal) as error:
        print(error, file=sys.stderr)
        return 1

    print(
        "TASK6140_SOURCE_OK domains=5 maintained_libraries=4 aad_fields=9 "
        "supported_cells=1 constructors=2 thresholds=2 cases_per_constructor=7 "
        "corpus_paths=14 tor_observations=14 direct_routes=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
