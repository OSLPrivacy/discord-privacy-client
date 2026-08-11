#!/usr/bin/env python3
"""TASK 6581 fixed semantic oracle for TASK 6580 backup disclosures."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


AXES = (
    "provider",
    "region",
    "account",
    "control-plane",
    "administrator",
    "credential",
    "key-authority",
)

AXIS_COPY = {
    "provider": "provider",
    "region": "region",
    "account": "account",
    "control-plane": "control plane",
    "administrator": "administrator",
    "credential": "credential",
    "key-authority": "key authority",
}

AXIS_DATA = {
    "provider": {"hosted-d1", "hosted-r2", "hosted-secrets"},
    "region": {"hosted-d1", "hosted-r2"},
    "account": {"hosted-d1", "hosted-r2", "hosted-secrets"},
    "control-plane": {"hosted-d1", "hosted-r2", "hosted-secrets"},
    "administrator": {"hosted-d1", "hosted-r2", "hosted-secrets"},
    "credential": {"hosted-d1", "hosted-r2", "hosted-secrets"},
    "key-authority": {"hosted-d1", "hosted-r2", "hosted-secrets"},
}

AXIS_EDGES = {
    "provider": "Cloudflare-live->Cloudflare-Time-Travel",
    "region": "WNAM-live->no-separate-recovery-region",
    "account": "production-account->same-account-recovery",
    "control-plane": "Cloudflare-control-plane->same-control-plane-recovery",
    "administrator": "super-administrator->same-administrator-recovery",
    "credential": "destructive-OAuth-credential->no-separate-recovery-credential",
    "key-authority": "Cloudflare-managed-keys->no-separate-recovery-key-authority",
}

SURFACES = {
    "app-account-backup": ("local-account", "src-tauri/assets/settings_window.html"),
    "app-uninstall-backup": ("local-uninstall", "apps/osl-hub/nsis/osl-uninstall-hooks.nsh"),
    "site-terms": ("hosted", "docs/terms.html"),
    "help-readme": ("hosted", "README.md"),
    "help-onboarding": ("local-account", "docs/ONBOARDING.md"),
    "help-boundary": ("hosted", "docs/backup-and-disaster-recovery.md"),
    "operator-cipher-store": ("hosted", "cipher-store-cf/DEPLOY.md"),
    "operator-keyserver": ("hosted", "keyserver-cf/DEPLOY.md"),
    "operator-uninstall-contract": ("local-uninstall", "docs/release/burn-and-uninstall-contract.md"),
}

HOSTED_DATA = {
    "site-terms": (("d1", "identit", "wrapped key"), ("r2", "ciphertext"), ("service secrets",)),
    "help-readme": (("d1", "identit", "wrapped key"), ("r2", "ciphertext"), ("service secrets",)),
    "help-boundary": (("d1", "identit", "wrapped key"), ("r2", "ciphertext"), ("service secrets",)),
    "operator-cipher-store": (("d1", "relay", "attachment"), ("r2", "ciphertext")),
    "operator-keyserver": (("d1", "identit", "wrapped key"), ("r2", "ciphertext"), ("service secrets",)),
}

OWNER_REASON = "genuine disaster isolation is wanted but not funded or operated for this release"

MUTANTS = (
    *(f"hide-axis-{axis}" for axis in AXES),
    *(f"remove-surface-{surface}" for surface in SURFACES),
    "omit-loss-hosted-d1",
    "omit-loss-hosted-r2",
    "claim-off-site",
    "claim-independent",
    "claim-disaster-isolated",
    "promise-recovery",
    "forged-smaller-inventory",
    "remove-task-6582",
    "defer-deletion",
)

FORBIDDEN = {
    "off-site backup protection is active": (
        "same-provider-live->same-provider-recovery",
        "hosted-d1,hosted-r2,hosted-secrets",
        "shared-domain-no-off-site-isolation",
        "off-site-protection-claimed",
    ),
    "independent backup protection is active": (
        "shared-seven-axis-live->shared-seven-axis-recovery",
        "hosted-d1,hosted-r2,hosted-secrets",
        "shared-domain-no-independent-copy",
        "independent-protection-claimed",
    ),
    "disaster-isolated protection is active": (
        "shared-seven-axis-live->shared-seven-axis-recovery",
        "hosted-d1,hosted-r2,hosted-secrets",
        "isolated-axes=0",
        "disaster-isolation-claimed",
    ),
    "recovery is guaranteed after shared-domain failure": (
        "shared-domain-failure->same-domain-recovery-loss",
        "hosted-d1,hosted-r2,hosted-secrets",
        "recovery-not-guaranteed-after-co-failure",
        "co-failure-recovery-guaranteed",
    ),
}

CONFIG_FACTS = {
    "cipher-store-cf/wrangler.toml": (
        "database_id = \"be3d31f1-f6b4-4d6e-8ede-74514950b9e2\"",
        "bucket_name = \"osl-cipher-attachments-prod\"",
        "bucket_name = \"osl-cipher-payloads-prod\"",
    ),
    "keyserver-cf/wrangler.toml": (
        "database_id = \"1de837cd-3bf6-4d33-be82-12d358523600\"",
        "bucket_name = \"osl-retained-archive-payloads-prod\"",
    ),
}


class GateError(RuntimeError):
    pass


def normalized(value: str) -> str:
    value = value.replace("**", "").replace("`", "").replace('"', "").replace("+", "").replace("-", " ")
    return re.sub(r"\s+", " ", value).strip().lower()


def axis_absence_diagnostic(axis: str) -> str:
    return (
        f"absent axis starvation axis={axis} edge={AXIS_EDGES[axis]} "
        f"affected_data={','.join(sorted(AXIS_DATA[axis]))} "
        "expected=shared-axis-disclosed actual=axis-hidden"
    )


def require_file(root: Path, relative: str, label: str) -> str:
    path = root / relative
    if not path.is_file():
        raise GateError(f"absent surface starvation={label} path={relative}")
    return path.read_text(encoding="utf-8")


def require_exact_keys(record: dict, expected: set[str], label: str) -> None:
    actual = set(record)
    if actual != expected:
        raise GateError(f"forged inventory {label}: expected={sorted(expected)} actual={sorted(actual)}")


def validate_measurement(root: Path, oracle: dict) -> None:
    measurement = oracle.get("measurement")
    if not isinstance(measurement, dict):
        raise GateError("forged inventory measurement missing")

    for relative, needles in CONFIG_FACTS.items():
        text = require_file(root, relative, f"configuration:{relative}")
        for needle in needles:
            if needle not in text:
                raise GateError(f"configuration graph mismatch path={relative} missing={needle}")

    provider = measurement.get("providerApi", {})
    if provider.get("provider") != "Cloudflare" or provider.get("accountId") != "3490960a72d5f0c1ecf90490ccfbf7f1":
        raise GateError("provider/account graph mismatch expected=Cloudflare/3490960a72d5f0c1ecf90490ccfbf7f1")
    expected_d1 = {
        ("osl-cipher-store-prod", "be3d31f1-f6b4-4d6e-8ede-74514950b9e2", "WNAM", "disabled", True),
        ("osl-keyserver-prod", "1de837cd-3bf6-4d33-be82-12d358523600", "WNAM", "disabled", True),
    }
    actual_d1 = {
        (row.get("name"), row.get("id"), row.get("location"), row.get("readReplication"), row.get("timeTravelBookmarkObserved"))
        for row in provider.get("d1", []) if isinstance(row, dict)
    }
    if actual_d1 != expected_d1:
        raise GateError(f"provider D1/Time Travel graph mismatch affected_data=hosted-d1 actual={sorted(actual_d1, key=str)}")
    expected_r2 = {"osl-cipher-attachments-prod", "osl-cipher-payloads-prod", "osl-retained-archive-payloads-prod"}
    actual_r2 = {row.get("name") for row in provider.get("r2", []) if row.get("location") == "WNAM" and row.get("recoveryCopyObserved") is False}
    if actual_r2 != expected_r2 or provider.get("recoveryR2BucketsObserved") != 0:
        raise GateError(f"provider R2 graph mismatch affected_data=hosted-r2 actual={sorted(actual_r2)}")

    iam = measurement.get("iam", {})
    if (iam.get("acceptedMembers"), iam.get("superAdministrators"), iam.get("twoFactorAuthenticationEnabled"), iam.get("separateRecoveryAdministratorObserved")) != (1, 1, False, False):
        raise GateError("IAM graph mismatch axis=administrator affected_data=hosted-d1,hosted-r2,hosted-secrets")
    credentials = measurement.get("credentials", {})
    if not (credentials.get("workersWrite") is True and credentials.get("d1Write") is True and credentials.get("separateRecoveryCredentialObserved") is False):
        raise GateError("credential graph mismatch axis=credential affected_data=hosted-d1,hosted-r2,hosted-secrets")
    keys = measurement.get("keyAuthority", {})
    if not (keys.get("customerKmsBindingObserved") is False and keys.get("r2Keys") == "Cloudflare-managed" and keys.get("separateRecoveryKeyAuthorityObserved") is False):
        raise GateError("key graph mismatch axis=key-authority affected_data=hosted-d1,hosted-r2,hosted-secrets")


def validate_axes(oracle: dict) -> None:
    rows = oracle.get("axes")
    if not isinstance(rows, list):
        raise GateError("axis inventory absent")
    mapped = {row.get("id"): row for row in rows if isinstance(row, dict)}
    for axis in AXES:
        if axis not in mapped:
            raise GateError(axis_absence_diagnostic(axis))
        row = mapped[axis]
        if row.get("isolation") != 0:
            raise GateError(f"false isolation axis={axis} expected=0 actual={row.get('isolation')}")
        affected = set(row.get("affectedDataClasses", []))
        if affected != AXIS_DATA[axis]:
            raise GateError(f"omitted loss axis={axis} affected_data={','.join(sorted(AXIS_DATA[axis] - affected)) or 'unexpected-extra'}")
        if not row.get("live") or not row.get("recovery"):
            raise GateError(f"forged smaller inventory axis={axis} live/recovery edge absent")
    if set(mapped) != set(AXES):
        raise GateError(f"unexpected axis inventory expected={list(AXES)} actual={sorted(mapped)}")
    if oracle.get("guaranteedRecoveryClaims") != 0:
        raise GateError(f"false guaranteed recovery count expected=0 actual={oracle.get('guaranteedRecoveryClaims')}")


def validate_surfaces(root: Path, oracle: dict) -> dict[str, str]:
    rows = oracle.get("boundarySurfaces")
    if not isinstance(rows, list):
        raise GateError("absent surface inventory")
    actual = {row.get("id"): (row.get("kind"), row.get("path")) for row in rows if isinstance(row, dict)}
    if actual != SURFACES:
        missing = sorted(set(SURFACES) - set(actual))
        raise GateError(f"absent surface starvation={missing[0] if missing else 'inventory-mismatch'}")

    texts: dict[str, str] = {}
    for surface, (kind, relative) in SURFACES.items():
        text = normalized(require_file(root, relative, surface))
        texts[surface] = text
        for axis, needle in AXIS_COPY.items():
            if needle not in text:
                raise GateError(f"surface={surface} omitted axis={axis} affected_data={kind}")
        if not re.search(r"(?:isolation[^.]{0,80}\b0\b|\b0\b[^.]{0,80}isolation)", text):
            raise GateError(f"surface={surface} absent isolation=0 claim")
        if not re.search(r"(?:guarantee[^.]{0,80}\b0\b|\b0\b[^.]{0,80}guarantee|not guaranteed)", text):
            raise GateError(f"surface={surface} absent guaranteed-recovery=0 claim")
        if "failure" not in text or "lose" not in text:
            raise GateError(f"surface={surface} omitted incident-to-loss meaning affected_data={kind}")
        if "task 6582" not in text or OWNER_REASON not in text:
            raise GateError(
                f"surface={surface} missing deferred task=6582 edge=owner-ruling-T7->task-6582 "
                "affected_data=all-backup-loss-boundaries expected=deferred-work-disclosed "
                "actual=deferred-work-reference-missing"
            )

        if kind == "local-account":
            for needle in ("keys", "contacts", "settings", "message history"):
                if needle not in text:
                    raise GateError(f"surface={surface} omitted loss data_class=local-account-backup missing={needle}")
        elif kind == "local-uninstall":
            for needle in ("identity", "local message"):
                if needle not in text:
                    raise GateError(f"surface={surface} omitted loss data_class=local-uninstall-backup missing={needle}")
        else:
            for alternatives in HOSTED_DATA[surface]:
                if not all(needle in text for needle in alternatives):
                    data_class = "hosted-r2" if "r2" in alternatives else "hosted-secrets" if "service secrets" in alternatives else "hosted-d1"
                    edge = (
                        "provider-R2-serving-copy->no-recovery-R2-copy"
                        if data_class == "hosted-r2"
                        else "provider-D1-live->provider-D1-Time-Travel"
                    )
                    raise GateError(
                        f"surface={surface} omitted loss data_class={data_class} edge={edge} "
                        f"affected_data={data_class} expected=possible-loss-disclosed "
                        f"actual=loss-omitted missing={','.join(alternatives)}"
                    )

    combined = " ".join(texts.values())
    for phrase, (edge, affected_data, expected, actual) in FORBIDDEN.items():
        if normalized(phrase) in combined:
            raise GateError(
                f"false words={phrase} edge={edge} affected_data={affected_data} "
                f"expected={expected} actual={actual}"
            )
    return texts


def validate_deletion(root: Path, oracle: dict) -> None:
    rows = oracle.get("deletionAndErasureObligations")
    if not isinstance(rows, list):
        raise GateError("wrongly deferred obligation inventory absent")
    expected_ids = {
        "remote-delete-failure-retry",
        "provider-offline-local-destruction",
        "r2-versioning-not-a-hidden-copy",
        "serving-bucket-deletion-scope",
        "retention-delete-claim-not-eligible",
        "uninstall-decline-removes-backup",
    }
    actual_ids = {row.get("id") for row in rows if isinstance(row, dict)}
    if actual_ids != expected_ids:
        raise GateError(f"wrongly deferred obligation starvation missing={sorted(expected_ids - actual_ids)}")
    for row in rows:
        text = normalized(require_file(root, row["path"], f"obligation:{row['id']}"))
        if normalized(row["required"]) not in text:
            raise GateError(
                f"wrongly deferred obligation={row['id']} "
                "edge=remote-backup-object->deletion-retry affected_data=remote-backup-object "
                f"expected=deletion-erasure-active actual=deletion-erasure-deferred path={row['path']}"
            )


def validate(root: Path, oracle_path: Path) -> None:
    try:
        oracle = json.loads(oracle_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise GateError(f"failure-domain oracle unavailable: {error}") from error
    if oracle.get("schema") != "osl.task-6580.backup-failure-domain-oracle.v1":
        raise GateError("failure-domain oracle schema mismatch")
    validate_measurement(root, oracle)
    if oracle.get("candidateDisclosureDerivation") == "regenerated-from-false-inventory":
        raise GateError(
            "attack=self-derived-copy edge=false-inventory->regenerated-disclosure "
            "affected_data=hosted-d1,hosted-r2,hosted-secrets "
            "expected=copy-checked-against-fixed-measurement actual=copy-derived-from-false-inventory"
        )
    validate_axes(oracle)
    validate_surfaces(root, oracle)
    deferred = oracle.get("ownerDeferredWork", {})
    if deferred != {"task": 6582, "ruling": "T7", "reason": OWNER_REASON}:
        raise GateError(f"missing deferred task=6582 reason={OWNER_REASON}")
    if tuple(oracle.get("requiredMutants", ())) != MUTANTS:
        expected = set(MUTANTS)
        actual = set(oracle.get("requiredMutants", ()))
        raise GateError(f"absent mutant starvation={sorted(expected - actual)[0] if expected - actual else 'ordering-or-extra-mutant'}")
    validate_deletion(root, oracle)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    default_root = Path(__file__).resolve().parents[2]
    parser.add_argument("--root", type=Path, default=default_root)
    parser.add_argument("--oracle", type=Path)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    oracle = args.oracle or root / "contracts/task-6580-backup-failure-domain-oracle.json"
    try:
        validate(root, oracle)
    except GateError as error:
        print(f"TASK6581 FAIL {error}", file=sys.stderr)
        return 1
    print(
        "TASK6581 PASS provider_iam_reconciliation=matched provider=Cloudflare accounts=1 locations=1 d1=2 "
        "time_travel_recovery=2 r2_payload_buckets=3 recovery_r2_buckets=0 "
        "administrators=1 recovery_administrators=0 destructive_credentials_observed=1 "
        "recovery_credentials=0 customer_kms=0 axes=7 shared_axes=7 isolated_axes=0 "
        "data_classes=5 surfaces=9 app_surfaces=2 site_surfaces=1 help_surfaces=3 "
        "operator_surfaces=3 isolation_claims=0 guaranteed_recovery_claims=0 "
        "deletion_erasure_obligations=6"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
