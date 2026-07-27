import { createHash } from "node:crypto";

export const SENDER_FILTER_EVIDENCE_FORMAT =
  "osl.keyserver.sender-filter-live-evidence.v1";
export const SENDER_FILTER_RECEIPT_FORMAT =
  "osl.keyserver.sender-filter-deployment-admission.v1";
export const SENDER_FILTER_MAX_EVIDENCE_AGE_MS = 120_000;
export const SENDER_FILTER_MAX_CAPTURE_DURATION_MS = 120_000;
export const SENDER_FILTER_CLOCK_SKEW_MS = 10_000;

export const SENDER_FILTER_LIVE_NULL_FIXTURE =
  "keyserver-cf/scripts/fixtures/sender-filter-live-null.json";

export const SENDER_FILTER_SOURCE_FILES = Object.freeze({
  "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql":
    Object.freeze({
      role: "migration-0031",
      sha256:
        "2b44b32a0eeaa20da1ed91bb4af2f862227a88bd8c25ee148aaf1e69fc3a86db",
    }),
  "keyserver-cf/migrations/0032_sender_filter_capability_floor.sql":
    Object.freeze({
      role: "migration-0032-capability-floor-authority",
      sha256:
        "862ad79d48b9712199c8cee2958f7be392e483b1ddc98177b1ca1115b6ce06c5",
    }),
  "keyserver-cf/migrations/0033_canonical_identity_rollout_authority.sql":
    Object.freeze({
      role: "migration-0033-canonical-rollout-authority",
      sha256:
        "5660f5be340247fc0e4ac4caf48f333523856dc225352db8d01c12e75c4f32df",
    }),
  "keyserver-cf/src/index.ts": Object.freeze({
    role: "worker-route-registration",
    sha256:
      "ba2e504235dce21677b18676b6c7f7841ed424ac654faa96f2b92f5ac38e7200",
  }),
  "keyserver-cf/src/endpoints/register.ts": Object.freeze({
    role: "shipping-canonical-identity-registration-caller",
    sha256:
      "b602c6d91fa748ef86bf114bec89935879920e74663bc1f0ad17122f4fd518bb",
  }),
  "keyserver-cf/src/endpoints/pubkeys.ts": Object.freeze({
    role: "shipping-full-bundle-proof-response",
    sha256:
      "a8642d796e0ee8e209a0292caca5913478920aad77c726887c1dc6307f1a2c14",
  }),
  "keyserver-cf/src/endpoints/canonical-identity.ts": Object.freeze({
    role: "canonical-identity-proof-and-monotonic-cas",
    sha256:
      "4ded4413982e271c9b7f16691250f84b47cbe58900d9c8ceebff5266cef8d753",
  }),
  "keyserver-cf/src/endpoints/sender-filter-rollout-root.ts":
    Object.freeze({
      role: "shipping-rollout-root-genesis-and-cas",
      sha256:
        "a5b438ab067e2553e2773eb7bca04c1ab06aa770bed201cb126a63ab4d396379",
    }),
  "keyserver-cf/src/endpoints/control-inbox.ts": Object.freeze({
    role: "signed-sender-filter-route",
    sha256:
      "23a58e917ada33c07995d9ae933e6e0e92cf0892cd16201b6c25a04d3324301f",
  }),
  "keyserver-cf/src/endpoints/healthz.ts": Object.freeze({
    role: "capability-route",
    sha256:
      "8328a29f38669343f24b88a2848066eb03d5fd397b8aece36695323576950d0c",
  }),
  "keyserver-cf/src/endpoints/sender-filter-capability-floor.ts":
    Object.freeze({
      role: "identity-authenticated-capability-floor-authority",
      sha256:
        "3f5b7c78d828655344a5b94fa2a6d6ab4ba99509f61457b27dc59630f462a2b5",
    }),
  "keyserver-cf/src/lib/canonical.ts": Object.freeze({
    role: "signed-filter-canonical-bytes",
    sha256:
        "618ace3df4d9494f780179034bb1e3253a5a1cade77750a1731c7965a9ec3d86",
  }),
  "keyserver-cf/src/lib/identity-authority.ts": Object.freeze({
      role: "canonical-opaque-id-and-full-bundle-proof",
      sha256:
        "d53a6b4fe61b52aafe7dd2699ea469ac5ee2ee4d21cad7d03e11adf40e954909",
  }),
  "keyserver-cf/src/lib/control-inbox-sweep.ts": Object.freeze({
    role: "capability-schema-projection",
    sha256:
      "2e2263a4b2d37ae33fb0638028e99068dd0216fc420446b93986a98db331173a",
  }),
  "keyserver-cf/test/integration/control-inbox-sender-filter.test.ts":
    Object.freeze({
      role: "nonempty-route-behavior-fixture",
      sha256:
        "2dc27d68fdb9c838d07ecccfc28b51f269ba5525f55b71685f324bef7636d462",
    }),
  "keyserver-cf/test/integration/sender-filter-capability-floor.test.ts":
    Object.freeze({
      role: "nonempty-authority-and-anti-reset-fixture",
      sha256:
        "a515a84f6ea2e56be0a8fcfda557938f7c359ecd70c07ad62e92a688a209b9d8",
    }),
  "keyserver-cf/test/integration/canonical-identity-rollout.test.ts":
    Object.freeze({
      role: "real-worker-d1-canonical-authority-fixture",
      sha256:
        "68a18f4f6f5f67f9292c01b0d81e52f766fae2e8bfa87f7897c92d22fa74c9ac",
    }),
  "keyserver-cf/scripts/provision-sender-filter-rollout-genesis.mjs":
    Object.freeze({
      role: "shipping-d1-admin-genesis-provisioning-interface",
      sha256:
        "ea361582dad96c677907eb1c5ebb64c7adfde7fd3d998b2d8fc5501eee6ffd42",
    }),
  "keyserver-cf/scripts/canonical-identity-rollout.test.ts":
    Object.freeze({
      role: "nonempty-identity-authority-and-cas-fixture",
      sha256:
        "b8387092656aada4a1da45e681817a9ceac4ddcf460a7d735bebd2e574af66c3",
    }),
});

export const SENDER_FILTER_ROUTE_CONTRACT = Object.freeze({
  method: "GET",
  path: "/v1/control-inbox/:user_id",
  signed_query_component: "sender",
  response_filter_echo: "filtered_sender_id",
  response_disposition: "filtered_sender_delivery",
  health_path: "/v1/healthz",
  health_capability: "control_inbox_sender_disposition",
  health_capability_version: 1,
  migration: "0031_control_inbox_sender_retention.sql",
  authority_path:
    "/v1/internal/sender-filter-rollout-root/{provision,advance}",
  authority_migration:
    "0033_canonical_identity_rollout_authority.sql",
});

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    throw new Error(`${label} fields are not exact`);
  }
}

function requireGitObject(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/.test(value)) {
    throw new Error(`${label} must be a full lowercase Git object id`);
  }
}

function requireIso(value, label) {
  if (typeof value !== "string" || !Number.isFinite(Date.parse(value))) {
    throw new Error(`${label} must be an ISO timestamp`);
  }
  return Date.parse(value);
}

function requireUuid(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      value,
    )
  ) {
    throw new Error(`${label} must be a lowercase UUID`);
  }
}

function sourceBytes(value, path) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof Uint8Array) return Buffer.from(value);
  if (typeof value === "string") return Buffer.from(value);
  throw new Error(`source file is not bytes: ${path}`);
}

export function validateSenderFilterSourceClosure(fileValues) {
  const files = requireObject(fileValues, "sender-filter source closure");
  const paths = Object.keys(SENDER_FILTER_SOURCE_FILES);
  requireExactKeys(files, paths, "sender-filter source closure");
  return paths.map((path) => {
    const bytes = sourceBytes(files[path], path);
    if (bytes.length === 0) {
      throw new Error(`sender-filter source file is empty: ${path}`);
    }
    const definition = SENDER_FILTER_SOURCE_FILES[path];
    const digest = sha256(bytes);
    if (digest !== definition.sha256) {
      throw new Error(`sender-filter source hash mismatch: ${path}`);
    }
    return {
      path,
      role: definition.role,
      bytes: bytes.length,
      sha256: digest,
    };
  });
}

function validateDeliveryCounts(value) {
  const counts = requireObject(value, "filtered sender delivery");
  requireExactKeys(
    counts,
    ["live", "quarantined", "retired", "retryable"],
    "filtered sender delivery",
  );
  for (const [name, count] of Object.entries(counts)) {
    if (!Number.isSafeInteger(count) || count < 0) {
      throw new Error(`filtered sender delivery count is invalid: ${name}`);
    }
  }
  return counts;
}

function validateRouteProbe(value, reasons) {
  if (value === null) {
    reasons.push("signed-sender-filter-route-missing");
    return null;
  }
  const route = requireObject(value, "sender-filter route probe");
  requireExactKeys(
    route,
    [
      "filtered_sender_delivery",
      "filtered_sender_id",
      "items",
      "requested_sender_id",
      "signed_sender_id",
      "status",
    ],
    "sender-filter route probe",
  );
  if (route.status !== 200) reasons.push("sender-filter-route-status-mismatch");
  for (const name of [
    "requested_sender_id",
    "signed_sender_id",
    "filtered_sender_id",
  ]) {
    if (typeof route[name] !== "string" || route[name].length === 0) {
      throw new Error(`sender-filter route ${name} is empty`);
    }
  }
  if (
    route.requested_sender_id !== route.signed_sender_id ||
    route.requested_sender_id !== route.filtered_sender_id
  ) {
    reasons.push("sender-filter-route-echo-mismatch");
  }
  if (!Array.isArray(route.items)) {
    throw new Error("sender-filter route items must be an array");
  }
  if (route.items.length === 0) {
    reasons.push("sender-filter-route-positive-starved");
  }
  for (const itemValue of route.items) {
    const item = requireObject(itemValue, "sender-filter route item");
    requireExactKeys(
      item,
      ["bundle_b64", "sender_id"],
      "sender-filter route item",
    );
    if (
      item.sender_id !== route.requested_sender_id ||
      typeof item.bundle_b64 !== "string" ||
      item.bundle_b64.length === 0
    ) {
      reasons.push("sender-filter-route-item-mismatch");
    }
  }
  const counts = validateDeliveryCounts(route.filtered_sender_delivery);
  if (counts.live < route.items.length) {
    reasons.push("sender-filter-route-disposition-mismatch");
  }
  return route;
}

function validateHealthProbe(value, reasons) {
  if (value === null) {
    reasons.push("health-capability-missing");
    return null;
  }
  const health = requireObject(value, "health capability probe");
  requireExactKeys(
    health,
    ["capability", "ok", "status"],
    "health capability probe",
  );
  if (
    health.status !== 200 ||
    health.ok !== true ||
    health.capability !== 1
  ) {
    reasons.push("health-capability-mismatch");
  }
  return health;
}

export function evaluateSenderFilterLiveEvidence(
  evidenceValue,
  expectedCommit,
  nowMs = Date.now(),
) {
  requireGitObject(expectedCommit, "expected worker commit");
  if (!Number.isFinite(nowMs)) throw new Error("current time is invalid");
  const evidence = requireObject(evidenceValue, "sender-filter live evidence");
  requireExactKeys(
    evidence,
    [
      "active_worker_version",
      "capability_table_exists",
      "captured_finished_at",
      "captured_started_at",
      "control_inbox_sender_disposition",
      "database",
      "environment",
      "evidence_tier",
      "format",
      "health_probe",
      "migration_0031_present",
      "route_probe",
      "worker_commit",
    ],
    "sender-filter live evidence",
  );
  if (evidence.format !== SENDER_FILTER_EVIDENCE_FORMAT) {
    throw new Error("sender-filter live evidence format is not exact");
  }
  if (evidence.database !== "osl-keyserver-prod") {
    throw new Error("sender-filter evidence database is not production");
  }
  if (evidence.environment !== "production") {
    throw new Error("sender-filter evidence environment is not production");
  }
  if (
    evidence.evidence_tier !== "verified-live-read-only" &&
    evidence.evidence_tier !== "verified-live-read-only-historical"
  ) {
    throw new Error("sender-filter evidence tier is not exact");
  }
  requireUuid(evidence.active_worker_version, "active worker version");
  if (
    evidence.worker_commit !== null &&
    (typeof evidence.worker_commit !== "string" ||
      !/^[0-9a-f]{40}$/.test(evidence.worker_commit))
  ) {
    throw new Error("live worker commit must be null or a full Git commit");
  }
  if (typeof evidence.migration_0031_present !== "boolean") {
    throw new Error("migration 0031 presence must be boolean");
  }
  if (
    evidence.capability_table_exists !== 0 &&
    evidence.capability_table_exists !== 1
  ) {
    throw new Error("capability table presence must be exactly 0 or 1");
  }
  if (
    evidence.control_inbox_sender_disposition !== null &&
    evidence.control_inbox_sender_disposition !== 1
  ) {
    throw new Error("disposition marker must be exactly null or 1");
  }

  const started = requireIso(
    evidence.captured_started_at,
    "capture start",
  );
  const finished = requireIso(
    evidence.captured_finished_at,
    "capture finish",
  );
  if (
    finished < started ||
    finished - started > SENDER_FILTER_MAX_CAPTURE_DURATION_MS
  ) {
    throw new Error("sender-filter capture duration is invalid");
  }

  const reasons = [];
  if (evidence.worker_commit === null) {
    reasons.push("live-worker-commit-unmapped");
  } else if (evidence.worker_commit !== expectedCommit) {
    reasons.push("live-worker-commit-mismatch");
  }
  if (nowMs - finished > SENDER_FILTER_MAX_EVIDENCE_AGE_MS) {
    reasons.push("live-evidence-stale");
  }
  if (finished > nowMs + SENDER_FILTER_CLOCK_SKEW_MS) {
    reasons.push("live-evidence-from-future");
  }
  if (!evidence.migration_0031_present) {
    reasons.push("migration-0031-absent");
  }
  if (evidence.capability_table_exists !== 1) {
    reasons.push("capability-table-absent");
  }
  if (evidence.control_inbox_sender_disposition !== 1) {
    reasons.push("live-disposition-null");
  }
  if (
    evidence.capability_table_exists === 0 &&
    evidence.control_inbox_sender_disposition !== null
  ) {
    throw new Error("disposition marker exists while capability table is absent");
  }
  validateHealthProbe(evidence.health_probe, reasons);
  validateRouteProbe(evidence.route_probe, reasons);

  return {
    would_admit_with_trusted_live_evidence: reasons.length === 0,
    refusal_reasons: reasons,
  };
}

export function createSenderFilterDeploymentReceipt({
  anchor,
  fileValues,
  liveEvidence,
  nowMs = Date.now(),
}) {
  const exactAnchor = requireObject(anchor, "sender-filter source anchor");
  requireExactKeys(
    exactAnchor,
    ["commit", "keyserver_tree", "repository_tree"],
    "sender-filter source anchor",
  );
  requireGitObject(exactAnchor.commit, "source commit");
  requireGitObject(exactAnchor.repository_tree, "repository tree");
  requireGitObject(exactAnchor.keyserver_tree, "keyserver tree");
  const files = validateSenderFilterSourceClosure(fileValues);
  const evaluation = evaluateSenderFilterLiveEvidence(
    liveEvidence,
    exactAnchor.commit,
    nowMs,
  );
  const payload = {
    format: SENDER_FILTER_RECEIPT_FORMAT,
    admission_scope: "source-only-non-authorizing",
    source_contract_admitted: true,
    deployment_admitted: false,
    generated_at: new Date(nowMs).toISOString(),
    source: {
      ...exactAnchor,
      files,
    },
    route_contract: SENDER_FILTER_ROUTE_CONTRACT,
    live_evidence: liveEvidence,
    ...evaluation,
  };
  return {
    ...payload,
    payload_sha256: sha256(Buffer.from(JSON.stringify(payload))),
  };
}
