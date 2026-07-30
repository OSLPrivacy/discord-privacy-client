import { canonicalJson } from "./readiness-artifact-contract.mjs";

export const DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT =
  "osl.keyserver.deployment-evidence-verifier-store.v1";
export const DEPLOYMENT_EVIDENCE_VERIFIER_ADMINISTRATION_DOMAIN =
  "independent-release-verifier";

export const DEPLOYMENT_EVIDENCE_VERIFIER_SELECT_SQL = `SELECT
  state_version,
  state_json
FROM deployment_evidence_verifier_state
WHERE producer_key_id = ?`;

export const DEPLOYMENT_EVIDENCE_VERIFIER_CAS_SQL = `UPDATE deployment_evidence_verifier_state
SET state_version = ?,
    state_json = ?,
    updated_at = ?
WHERE producer_key_id = ?
  AND state_version = ?`;

// No production D1 binding or administrative identity is present in source.
// The selector receives this value by default and therefore fails closed.
export const UNPROVISIONED_DEPLOYMENT_EVIDENCE_VERIFIER_STORE =
  Object.freeze({
    format: DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT,
    provisioned: false,
  });

function requireNonemptyString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be nonempty`);
  }
}

function requireUuid(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      value,
    )
  ) {
    throw new Error(`${label} must be a UUID`);
  }
}

function changes(result) {
  return result?.meta?.changes;
}

export function requireProvisionedDeploymentEvidenceVerifierStore(value) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    value.format !== DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT ||
    value.provisioned !== true ||
    value.administration_domain !==
      DEPLOYMENT_EVIDENCE_VERIFIER_ADMINISTRATION_DOMAIN ||
    value.environment !== "production" ||
    typeof value.readCurrent !== "function" ||
    typeof value.compareAndSwap !== "function"
  ) {
    throw new Error(
      "independently administered deployment verifier store is unprovisioned",
    );
  }
  requireUuid(value.database_id, "verifier store database id");
  requireNonemptyString(
    value.administrator_identity,
    "verifier store administrator identity",
  );
  return value;
}

export function validateDeploymentEvidenceVerifierSnapshot(
  value,
  producerKeyId,
) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).sort().join(",") !==
      "monotonic_version,state".split(",").sort().join(",") ||
    !Number.isSafeInteger(value.monotonic_version) ||
    value.monotonic_version <= 0 ||
    !value.state ||
    typeof value.state !== "object" ||
    Array.isArray(value.state) ||
    value.state.producer_key_id !== producerKeyId ||
    value.state.state_epoch !== value.monotonic_version
  ) {
    throw new Error(
      "deployment verifier store returned an invalid monotonic snapshot",
    );
  }
  return value;
}

export async function compareAndSwapDeploymentEvidenceVerifierState(
  storeValue,
  producerKeyId,
  snapshot,
  nextState,
) {
  const store = requireProvisionedDeploymentEvidenceVerifierStore(storeValue);
  validateDeploymentEvidenceVerifierSnapshot(snapshot, producerKeyId);
  const nextVersion = snapshot.monotonic_version + 1;
  if (
    nextState.state_epoch !== nextVersion ||
    nextState.producer_key_id !== producerKeyId
  ) {
    throw new Error("deployment verifier next state is not monotonic");
  }
  const result = await store.compareAndSwap({
    expected_monotonic_version: snapshot.monotonic_version,
    next_monotonic_version: nextVersion,
    next_state: structuredClone(nextState),
    producer_key_id: producerKeyId,
  });
  if (
    !result ||
    typeof result !== "object" ||
    Array.isArray(result) ||
    Object.keys(result).sort().join(",") !==
      "applied,observed_monotonic_version".split(",").sort().join(",") ||
    typeof result.applied !== "boolean" ||
    !Number.isSafeInteger(result.observed_monotonic_version) ||
    result.observed_monotonic_version <= 0
  ) {
    throw new Error("deployment verifier store returned an invalid CAS result");
  }
  if (
    result.applied !== true ||
    result.observed_monotonic_version !== nextVersion
  ) {
    throw new Error(
      "deployment verifier state changed concurrently; compare-and-swap refused",
    );
  }
  return nextVersion;
}

/**
 * Adapt one independently administered D1-style database binding.
 *
 * This exposes no INSERT, DELETE, reset, recovery, or genesis operation. The
 * administrator must provision the table and initial lineage out of band. A
 * single conditional UPDATE is the transaction boundary: exactly one caller
 * can advance a given state_version.
 */
export function createD1DeploymentEvidenceVerifierStore(
  database,
  {
    administratorIdentity,
    databaseId,
    environment,
  },
) {
  if (
    !database ||
    typeof database.prepare !== "function" ||
    environment !== "production"
  ) {
    throw new Error("independent verifier D1 binding is invalid");
  }
  requireUuid(databaseId, "verifier store database id");
  requireNonemptyString(
    administratorIdentity,
    "verifier store administrator identity",
  );
  return Object.freeze({
    format: DEPLOYMENT_EVIDENCE_VERIFIER_STORE_FORMAT,
    provisioned: true,
    administration_domain:
      DEPLOYMENT_EVIDENCE_VERIFIER_ADMINISTRATION_DOMAIN,
    administrator_identity: administratorIdentity,
    database_id: databaseId,
    environment,
    async readCurrent(producerKeyId) {
      requireNonemptyString(producerKeyId, "verifier producer key id");
      const row = await database
        .prepare(DEPLOYMENT_EVIDENCE_VERIFIER_SELECT_SQL)
        .bind(producerKeyId)
        .first();
      if (row === null) return null;
      if (
        !row ||
        !Number.isSafeInteger(row.state_version) ||
        row.state_version <= 0 ||
        typeof row.state_json !== "string" ||
        row.state_json.length === 0
      ) {
        throw new Error("verifier D1 row is malformed");
      }
      let state;
      try {
        state = JSON.parse(row.state_json);
      } catch {
        throw new Error("verifier D1 state is not JSON");
      }
      if (canonicalJson(state) !== row.state_json) {
        throw new Error("verifier D1 state is not canonical JSON");
      }
      return {
        monotonic_version: row.state_version,
        state,
      };
    },
    async compareAndSwap({
      expected_monotonic_version,
      next_monotonic_version,
      next_state,
      producer_key_id,
    }) {
      if (
        !Number.isSafeInteger(expected_monotonic_version) ||
        expected_monotonic_version <= 0 ||
        next_monotonic_version !== expected_monotonic_version + 1
      ) {
        throw new Error("verifier D1 CAS version is invalid");
      }
      const result = await database
        .prepare(DEPLOYMENT_EVIDENCE_VERIFIER_CAS_SQL)
        .bind(
          next_monotonic_version,
          canonicalJson(next_state),
          new Date().toISOString(),
          producer_key_id,
          expected_monotonic_version,
        )
        .run();
      if (changes(result) === 1) {
        return {
          applied: true,
          observed_monotonic_version: next_monotonic_version,
        };
      }
      const current = await this.readCurrent(producer_key_id);
      return {
        applied: false,
        observed_monotonic_version:
          current?.monotonic_version ?? expected_monotonic_version,
      };
    },
  });
}
