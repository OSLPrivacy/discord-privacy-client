import {
  canonicalJson,
  sha256,
} from "./readiness-artifact-contract.mjs";
import {
  TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS,
  verifyDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-contract.mjs";
import {
  UNPROVISIONED_DEPLOYMENT_EVIDENCE_VERIFIER_STORE,
  requireProvisionedDeploymentEvidenceVerifierStore,
  validateDeploymentEvidenceVerifierSnapshot,
} from "./deployment-evidence-verifier-store.mjs";

export const SENDER_FILTER_ROLLOUT_FORMAT =
  "osl.keyserver.sender-filter-rollout-contract.v2";
export const SENDER_FILTER_PLAN_FORMAT =
  "osl.keyserver.sender-filter-rollout-plan.v2";
export const SENDER_FILTER_PHASE_RECEIPT_FORMAT =
  "osl.keyserver.sender-filter-phase-receipt.v3";
export const SENDER_FILTER_CAPABILITY =
  "control_inbox_sender_disposition";
export const SENDER_FILTER_CAPABILITY_VERSION = 1;
export const SENDER_FILTER_MAX_PAGE = 64;

export const ROLLOUT_WORKERS = Object.freeze([
  "legacy",
  "artifact-a",
  "artifact-b",
]);
export const ROLLOUT_SCHEMAS = Object.freeze(["pre-0031", "0031"]);
export const ROLLOUT_CLIENTS = Object.freeze(["legacy", "sender-filter"]);
export const ROLLOUT_SOURCE_PATHS = Object.freeze([
  "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql",
  "keyserver-cf/src/index.ts",
  "keyserver-cf/src/endpoints/control-inbox.ts",
  "keyserver-cf/src/endpoints/healthz.ts",
  "keyserver-cf/src/readiness/bridge/control-inbox.ts",
  "keyserver-cf/src/readiness/bridge/healthz.ts",
  "keyserver-cf/src/lib/canonical.ts",
  "crates/keystore/src/client.rs",
  "crates/keystore/src/sender_filter_rollout.rs",
  "apps/osl-hub/src/broker.rs",
]);

export const ROLLOUT_CALL_SITES = Object.freeze({
  broker_boundary:
    "apps/osl-hub/src/broker.rs::fetch_peer_control_inbox",
  client_boundary:
    "crates/keystore/src/client.rs::KeyServerClient::get_control_inbox_compatible_from",
  migration:
    "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql",
  worker_health:
    "keyserver-cf/src/endpoints/healthz.ts::handleHealthz",
  worker_inbox:
    "keyserver-cf/src/endpoints/control-inbox.ts::handleControlInboxGetInner",
});

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

function requireChoice(value, choices, label) {
  if (!choices.includes(value)) {
    throw new Error(`${label} is not exact`);
  }
}

function validateRows(value) {
  if (!Array.isArray(value)) {
    throw new Error("rollout inbox rows must be an array");
  }
  return value.map((entryValue) => {
    const entry = requireObject(entryValue, "rollout inbox row");
    requireExactKeys(entry, ["id", "sender_id"], "rollout inbox row");
    if (
      typeof entry.id !== "string" ||
      entry.id.length === 0 ||
      typeof entry.sender_id !== "string" ||
      entry.sender_id.length === 0
    ) {
      throw new Error("rollout inbox row is empty");
    }
    return entry;
  });
}

export function workerHealth(worker, schema) {
  requireChoice(worker, ROLLOUT_WORKERS, "rollout Worker");
  requireChoice(schema, ROLLOUT_SCHEMAS, "rollout schema");
  if (worker === "legacy") {
    return { status: 200, body: { ok: true } };
  }
  if (worker === "artifact-a") {
    return {
      status: 200,
      body: {
        ok: true,
        readiness_artifact: "A-pre-0031-bridge",
      },
    };
  }
  if (schema === "pre-0031") {
    return {
      status: 503,
      body: {
        ok: false,
        capabilities: {
          [SENDER_FILTER_CAPABILITY]: 0,
        },
      },
    };
  }
  return {
    status: 200,
    body: {
      ok: true,
      capabilities: {
        [SENDER_FILTER_CAPABILITY]: SENDER_FILTER_CAPABILITY_VERSION,
      },
    },
  };
}

export function classifyCapability(probeValue, capabilityPreviouslyObserved) {
  const probe = requireObject(probeValue, "capability probe");
  requireExactKeys(probe, ["body", "status"], "capability probe");
  if (typeof capabilityPreviouslyObserved !== "boolean") {
    throw new Error("capability memory must be boolean");
  }
  if (probe.status === 503) {
    return { mode: "refuse", reason: "worker-or-schema-unavailable" };
  }
  if (probe.status !== 200) {
    return { mode: "refuse", reason: "capability-status-mismatch" };
  }
  const body = requireObject(probe.body, "capability body");
  if (
    body.ok === true &&
    body.readiness_artifact === "A-pre-0031-bridge" &&
    Object.keys(body).length === 2
  ) {
    return { mode: "refuse", reason: "artifact-a-transition" };
  }
  if (body.ok === true && Object.keys(body).length === 1) {
    if (capabilityPreviouslyObserved) {
      return { mode: "refuse", reason: "capability-downgrade" };
    }
    return { mode: "legacy", reason: "capability-not-yet-advertised" };
  }
  if (
    body.ok === true &&
    body.capabilities &&
    typeof body.capabilities === "object" &&
    !Array.isArray(body.capabilities) &&
    Object.keys(body).length === 2 &&
    Object.keys(body.capabilities).length === 1 &&
    body.capabilities[SENDER_FILTER_CAPABILITY] ===
      SENDER_FILTER_CAPABILITY_VERSION
  ) {
    return { mode: "filtered", reason: null };
  }
  return { mode: "refuse", reason: "capability-shape-or-version-mismatch" };
}

function validateRequest(value) {
  const request = requireObject(value, "sender-filter request");
  requireExactKeys(
    request,
    ["sender_param", "signature_valid", "signed_sender"],
    "sender-filter request",
  );
  if (typeof request.signature_valid !== "boolean") {
    throw new Error("request signature validity must be boolean");
  }
  for (const field of ["sender_param", "signed_sender"]) {
    if (
      request[field] !== null &&
      (typeof request[field] !== "string" || request[field].length === 0)
    ) {
      throw new Error(`request ${field} must be null or nonempty`);
    }
  }
  return request;
}

function validProtocolSender(value) {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 256 &&
    !/[\u0000-\u001f\u007f]/.test(value)
  );
}

export function evaluateWorkerRequest({
  worker,
  schema,
  request: requestValue,
  rows: rowsValue,
}) {
  requireChoice(worker, ROLLOUT_WORKERS, "rollout Worker");
  requireChoice(schema, ROLLOUT_SCHEMAS, "rollout schema");
  const request = validateRequest(requestValue);
  const rows = validateRows(rowsValue);

  if (worker === "artifact-a") {
    return {
      status: 503,
      items: [],
      filtered_sender_id: null,
      refusal: "artifact-a-transition",
    };
  }
  if (worker === "artifact-b" && schema === "pre-0031") {
    return {
      status: 503,
      items: [],
      filtered_sender_id: null,
      refusal: "migration-0031-absent",
    };
  }
  if (worker === "legacy") {
    // The historical Worker neither parses nor signs `sender`. A request
    // signed with the new canonical component is therefore 401, while an
    // unknown query parameter appended to a valid legacy signature is ignored.
    if (!request.signature_valid || request.signed_sender !== null) {
      return {
        status: 401,
        items: [],
        filtered_sender_id: null,
        refusal: "sender-signature-mismatch",
      };
    }
    return {
      status: 200,
      items: rows.slice(0, SENDER_FILTER_MAX_PAGE),
      filtered_sender_id: null,
      refusal: null,
    };
  }
  if (
    request.sender_param !== null &&
    !validProtocolSender(request.sender_param)
  ) {
    return {
      status: 400,
      items: [],
      filtered_sender_id: null,
      refusal: "malformed-sender",
    };
  }

  const expectedSignedSender = request.sender_param;
  if (
    !request.signature_valid ||
    request.signed_sender !== expectedSignedSender
  ) {
    return {
      status: 401,
      items: [],
      filtered_sender_id: null,
      refusal: "sender-signature-mismatch",
    };
  }

  if (request.sender_param === null) {
    return {
      status: 200,
      items: rows.slice(0, SENDER_FILTER_MAX_PAGE),
      filtered_sender_id: null,
      refusal: null,
    };
  }
  return {
    status: 200,
    items: rows
      .filter((row) => row.sender_id === request.sender_param)
      .slice(0, SENDER_FILTER_MAX_PAGE),
    filtered_sender_id: request.sender_param,
    refusal: null,
  };
}

export function requestForClient({
  client,
  senderId,
  capabilityProbe,
  capabilityPreviouslyObserved = false,
}) {
  requireChoice(client, ROLLOUT_CLIENTS, "rollout client");
  if (!validProtocolSender(senderId)) {
    throw new Error("selected sender is malformed");
  }
  if (client === "legacy") {
    return {
      mode: "legacy",
      request: {
        sender_param: null,
        signed_sender: null,
        signature_valid: true,
      },
      refusal: null,
    };
  }
  const capability = classifyCapability(
    capabilityProbe,
    capabilityPreviouslyObserved,
  );
  if (capability.mode === "refuse") {
    return { mode: "refuse", request: null, refusal: capability.reason };
  }
  if (capability.mode === "legacy") {
    return {
      mode: "legacy",
      request: {
        sender_param: null,
        signed_sender: null,
        signature_valid: true,
      },
      refusal: null,
    };
  }
  return {
    mode: "filtered",
    request: {
      sender_param: senderId,
      signed_sender: senderId,
      signature_valid: true,
    },
    refusal: null,
  };
}

export function consumeClientResponse({
  mode,
  senderId,
  response,
}) {
  requireChoice(mode, ["legacy", "filtered"], "client drain mode");
  if (!validProtocolSender(senderId)) {
    throw new Error("selected sender is malformed");
  }
  if (!response || response.status !== 200 || !Array.isArray(response.items)) {
    return {
      accepted: false,
      fail_closed: true,
      reason: "drain-status-mismatch",
      active_sender_reachable: false,
      cross_sender_leakage: false,
    };
  }
  if (mode === "filtered") {
    if (response.filtered_sender_id !== senderId) {
      return {
        accepted: false,
        fail_closed: true,
        reason: "filtered-sender-echo-mismatch",
        active_sender_reachable: false,
        cross_sender_leakage: false,
      };
    }
    if (
      response.items.some((row) => row.sender_id !== senderId)
    ) {
      return {
        accepted: false,
        fail_closed: true,
        reason: "cross-sender-filter-response",
        active_sender_reachable: false,
        cross_sender_leakage: false,
      };
    }
  }
  return {
    accepted: true,
    fail_closed: false,
    reason: null,
    active_sender_reachable: response.items.some(
      (row) => row.sender_id === senderId,
    ),
    // An unfiltered legacy drain intentionally returns the recipient's whole
    // inbox. Cross-sender leakage is only a filtered-response violation.
    cross_sender_leakage: false,
  };
}

export function evaluateVersionSkewScenario({
  worker,
  schema,
  client,
  senderId,
  rows,
  capabilityProbe = workerHealth(worker, schema),
  capabilityPreviouslyObserved = false,
}) {
  const clientRequest = requestForClient({
    client,
    senderId,
    capabilityProbe,
    capabilityPreviouslyObserved,
  });
  if (clientRequest.mode === "refuse") {
    return {
      format: SENDER_FILTER_ROLLOUT_FORMAT,
      worker,
      schema,
      client,
      request_mode: "none",
      server_status: null,
      accepted: false,
      fail_closed: true,
      refusal: clientRequest.refusal,
      active_sender_reachable: false,
      cross_sender_leakage: false,
    };
  }
  const response = evaluateWorkerRequest({
    worker,
    schema,
    request: clientRequest.request,
    rows,
  });
  const disposition = consumeClientResponse({
    mode: clientRequest.mode,
    senderId,
    response,
  });
  return {
    format: SENDER_FILTER_ROLLOUT_FORMAT,
    worker,
    schema,
    client,
    request_mode: clientRequest.mode,
    server_status: response.status,
    accepted: disposition.accepted,
    fail_closed: disposition.fail_closed,
    refusal: disposition.reason ?? response.refusal,
    active_sender_reachable: disposition.active_sender_reachable,
    cross_sender_leakage: disposition.cross_sender_leakage,
  };
}

export const VERSION_SKEW_MATRIX = Object.freeze(
  ROLLOUT_SCHEMAS.flatMap((schema) =>
    ROLLOUT_WORKERS.flatMap((worker) =>
      ROLLOUT_CLIENTS.map((client) =>
        Object.freeze({ schema, worker, client }),
      ),
    ),
  ),
);

const PHASES = Object.freeze({
  "legacy-pre-0031": {
    worker: "legacy",
    schema: "pre-0031",
    capability: ["legacy", 200, null],
    legacy: ["legacy", 200, "positive"],
    filtered: ["none", null, "empty"],
  },
  "legacy-0031": {
    worker: "legacy",
    schema: "0031",
    capability: ["legacy", 200, null],
    legacy: ["legacy", 200, "positive"],
    filtered: ["none", null, "empty"],
  },
  "artifact-a-pre-0031": {
    worker: "artifact-a",
    schema: "pre-0031",
    capability: ["transitional", 200, 0],
    legacy: ["none", null, "empty"],
    filtered: ["none", null, "empty"],
  },
  "artifact-a-0031": {
    worker: "artifact-a",
    schema: "0031",
    capability: ["transitional", 200, 0],
    legacy: ["none", null, "empty"],
    filtered: ["none", null, "empty"],
  },
  "artifact-b-pre-0031": {
    worker: "artifact-b",
    schema: "pre-0031",
    capability: ["unavailable", 503, 0],
    legacy: ["none", null, "empty"],
    filtered: ["none", null, "empty"],
  },
  "artifact-b-0031": {
    worker: "artifact-b",
    schema: "0031",
    capability: ["filtered", 200, 1],
    legacy: ["legacy", 200, "positive"],
    filtered: ["filtered", 200, "positive"],
  },
});

const PHASE_RECEIPT_FIELDS = Object.freeze([
  "applied_schema_fingerprint_sha256",
  "archive_id",
  "artifact",
  "call_sites",
  "captured_at",
  "database_environment",
  "database_id",
  "deployment_id",
  "expected_commit",
  "format",
  "migration_0031_sha256",
  "phase",
  "producer_identity",
  "producer_key_id",
  "producer_receipt_sha256",
  "producer_sequence",
  "route_observation_sha256",
  "source_closure_sha256",
  "verifier_administrator_identity",
  "verifier_database_id",
  "verifier_monotonic_version",
  "worker_version",
]);

function requireSha(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/.test(value) ||
    value === "0".repeat(64)
  ) {
    throw new Error(`${label} must be a nonzero SHA-256`);
  }
}

function requireCommit(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{40}$/.test(value) ||
    value === "0".repeat(40)
  ) {
    throw new Error(`${label} must be a nonzero full commit`);
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

function requireConsumedVerifierState(
  verified,
  verifierStoreValue,
) {
  const store =
    requireProvisionedDeploymentEvidenceVerifierStore(verifierStoreValue);
  return store.readCurrent(verified.producer_key_id).then((snapshot) => {
    if (snapshot === null) {
      throw new Error(
        "authenticated rollout receipt has no transactional verifier lineage",
      );
    }
    validateDeploymentEvidenceVerifierSnapshot(
      snapshot,
      verified.producer_key_id,
    );
    const state = snapshot.state;
    const transition = verified.payload.transition;
    if (
      state.pending_challenge !== null ||
      state.producer_identity !== verified.producer_identity ||
      state.sequence !== verified.payload.producer_sequence ||
      state.receipt_sha256 !== verified.receipt_sha256 ||
      state.current_artifact !== transition.current_artifact ||
      state.current_worker_version !== transition.current_worker_version ||
      state.current_deployment_id !== transition.current_deployment_id
    ) {
      throw new Error(
        "rollout receipt is not the consumed head of verifier-administered lineage",
      );
    }
    return { snapshot, store };
  });
}

function validateDerivedPhaseReceipt(receiptValue) {
  const receipt = requireObject(receiptValue, "derived rollout phase receipt");
  requireExactKeys(
    receipt,
    PHASE_RECEIPT_FIELDS,
    "derived rollout phase receipt",
  );
  if (receipt.format !== SENDER_FILTER_PHASE_RECEIPT_FORMAT) {
    throw new Error("derived rollout phase receipt format mismatch");
  }
  requireCommit(receipt.expected_commit, "rollout expected commit");
  requireSha(receipt.archive_id, "rollout archive");
  requireSha(receipt.migration_0031_sha256, "migration 0031 digest");
  requireSha(receipt.source_closure_sha256, "source closure digest");
  requireSha(
    receipt.producer_receipt_sha256,
    "authenticated producer receipt digest",
  );
  requireSha(
    receipt.route_observation_sha256,
    "authenticated route observation digest",
  );
  requireSha(
    receipt.applied_schema_fingerprint_sha256,
    "authenticated schema fingerprint",
  );
  requireUuid(receipt.worker_version, "rollout Worker version");
  requireUuid(receipt.deployment_id, "rollout deployment id");
  requireUuid(receipt.database_id, "rollout D1 database id");
  requireUuid(receipt.verifier_database_id, "rollout verifier database id");
  if (
    !Number.isSafeInteger(receipt.producer_sequence) ||
    receipt.producer_sequence <= 0 ||
    !Number.isSafeInteger(receipt.verifier_monotonic_version) ||
    receipt.verifier_monotonic_version <= 0
  ) {
    throw new Error("rollout producer or verifier sequence is invalid");
  }
  for (const [value, label] of [
    [receipt.producer_key_id, "rollout producer key id"],
    [receipt.producer_identity, "rollout producer identity"],
    [
      receipt.verifier_administrator_identity,
      "rollout verifier administrator identity",
    ],
  ]) {
    if (typeof value !== "string" || value.length === 0) {
      throw new Error(`${label} must be nonempty`);
    }
  }
  if (
    receipt.database_environment !== "production" ||
    !["A", "B"].includes(receipt.artifact) ||
    !PHASES[receipt.phase] ||
    canonicalJson(receipt.call_sites) !== canonicalJson(ROLLOUT_CALL_SITES)
  ) {
    throw new Error("derived rollout phase identity is not exact");
  }
  return Object.freeze(structuredClone(receipt));
}

export async function deriveAuthenticatedSenderFilterPhaseReceipt(
  optionsValue,
) {
  const options = requireObject(
    optionsValue,
    "sender-filter phase derivation",
  );
  requireExactKeys(
    options,
    [
      "deploymentExpectation",
      "nowMs",
      "producerReceipt",
      "sourceFiles",
    ],
    "sender-filter phase derivation",
  );
  const {
    producerReceipt,
    deploymentExpectation,
    sourceFiles,
    nowMs,
  } = options;
  const sourceClosure = validateRolloutSourceClosure(sourceFiles);
  const verified = verifyDeploymentEvidenceReceipt(
    producerReceipt,
    deploymentExpectation,
    {
      trustedProducers: TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS,
      nowMs,
    },
  );
  const { snapshot, store } = await requireConsumedVerifierState(
    verified,
    UNPROVISIONED_DEPLOYMENT_EVIDENCE_VERIFIER_STORE,
  );
  const payload = verified.payload;
  const migration0031 = deploymentExpectation.expectedMigrations.find(
    (entry) =>
      entry.name === "0031_control_inbox_sender_retention.sql",
  );
  if (
    !migration0031 ||
    migration0031.sha256 !==
      sourceClosure.file_sha256[
        "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
      ]
  ) {
    throw new Error(
      "authenticated rollout migration does not match the source closure",
    );
  }
  const phase =
    payload.artifact === "B"
      ? "artifact-b-0031"
      : "artifact-a-pre-0031";
  return validateDerivedPhaseReceipt({
    format: SENDER_FILTER_PHASE_RECEIPT_FORMAT,
    phase,
    artifact: payload.artifact,
    expected_commit: payload.expected_commit,
    archive_id: payload.archive_id,
    migration_0031_sha256: migration0031.sha256,
    source_closure_sha256: sourceClosure.source_closure_sha256,
    call_sites: ROLLOUT_CALL_SITES,
    captured_at: payload.timestamps.issued_at,
    worker_version: payload.worker.version_id,
    deployment_id: payload.worker.deployment_id,
    database_id: payload.database.id,
    database_environment: payload.database.environment,
    applied_schema_fingerprint_sha256:
      payload.database.schema_fingerprint_sha256,
    route_observation_sha256:
      payload.worker.sender_filter_route.response_sha256,
    producer_key_id: verified.producer_key_id,
    producer_identity: verified.producer_identity,
    producer_sequence: payload.producer_sequence,
    producer_receipt_sha256: verified.receipt_sha256,
    verifier_database_id: store.database_id,
    verifier_administrator_identity: store.administrator_identity,
    verifier_monotonic_version: snapshot.monotonic_version,
  });
}

export async function admitSenderFilterRolloutPlan(optionsValue) {
  const options = requireObject(
    optionsValue,
    "sender-filter rollout admission",
  );
  requireExactKeys(
    options,
    [
      "deploymentExpectation",
      "nowMs",
      "producerReceipt",
      "sourceFiles",
    ],
    "sender-filter rollout admission",
  );
  const receipt = await deriveAuthenticatedSenderFilterPhaseReceipt({
    producerReceipt: options.producerReceipt,
    deploymentExpectation: options.deploymentExpectation,
    sourceFiles: options.sourceFiles,
    nowMs: options.nowMs,
  });
  const phase = PHASES[receipt.phase];
  const reasons = [];
  if (phase.worker === "artifact-a") {
    reasons.push("artifact-a-traffic-quiescence-is-not-authenticated");
  }
  if (
    phase.worker === "artifact-b" &&
    phase.schema === "pre-0031"
  ) {
    reasons.push("worker-first-artifact-b-refused");
  }
  let nextSelection = "none";
  if (reasons.length === 0) {
    if (receipt.phase === "artifact-b-0031") {
      nextSelection = "stable-compatible";
    }
  }
  return {
    format: SENDER_FILTER_PLAN_FORMAT,
    plan_admitted: reasons.length === 0,
    direct_deploy_permitted: false,
    execution_authorized: false,
    next_selection: nextSelection,
    phase_receipt_sha256: receipt.producer_receipt_sha256,
    source_closure_sha256: receipt.source_closure_sha256,
    reasons,
  };
}

function stripCommentsAndStrings(source) {
  let output = "";
  let state = "code";
  let quote = "";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (state === "line") {
      if (char === "\n") {
        state = "code";
        output += "\n";
      } else {
        output += " ";
      }
    } else if (state === "block") {
      if (char === "*" && next === "/") {
        output += "  ";
        index += 1;
        state = "code";
      } else {
        output += char === "\n" ? "\n" : " ";
      }
    } else if (state === "string") {
      if (char === "\\") {
        output += "  ";
        index += 1;
      } else if (char === quote) {
        output += " ";
        state = "code";
      } else {
        output += char === "\n" ? "\n" : " ";
      }
    } else if (char === "/" && next === "/") {
      output += "  ";
      index += 1;
      state = "line";
    } else if (char === "/" && next === "*") {
      output += "  ";
      index += 1;
      state = "block";
    } else if (
      char === "'" &&
      /[A-Za-z_]/.test(next ?? "") &&
      source[index + 2] !== "'"
    ) {
      // Rust lifetime (`&'a T`, `'static`) rather than a char literal.
      output += char;
    } else if (char === "'" || char === '"' || char === "`") {
      quote = char;
      output += " ";
      state = "string";
    } else {
      output += char;
    }
  }
  return output.replace(/\s+/g, " ");
}

function stripComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\/\/[^\n]*/g, " ");
}

function requireCode(source, path, patterns) {
  const code = stripCommentsAndStrings(source);
  for (const pattern of patterns) {
    if (!pattern.test(code)) {
      throw new Error(
        `rollout semantic source contract missing ${pattern}: ${path}`,
      );
    }
  }
}

function executableSqlStatements(source) {
  const statements = [];
  let current = "";
  let state = "code";
  let quote = "";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (state === "line-comment") {
      if (char === "\n") {
        state = "code";
        current += " ";
      }
    } else if (state === "block-comment") {
      if (char === "*" && next === "/") {
        state = "code";
        index += 1;
        current += " ";
      }
    } else if (state === "quoted") {
      current += char;
      if (char === quote) {
        if (next === quote) {
          current += next;
          index += 1;
        } else {
          state = "code";
        }
      }
    } else if (char === "-" && next === "-") {
      state = "line-comment";
      index += 1;
    } else if (char === "/" && next === "*") {
      state = "block-comment";
      index += 1;
    } else if (char === "'" || char === '"' || char === "`") {
      state = "quoted";
      quote = char;
      current += char;
    } else if (char === ";") {
      const statement = current.replace(/\s+/g, " ").trim().toLowerCase();
      if (statement.length > 0) statements.push(statement);
      current = "";
    } else {
      current += char;
    }
  }
  if (state === "block-comment" || state === "quoted") {
    throw new Error("migration 0031 has unterminated SQL syntax");
  }
  const tail = current.replace(/\s+/g, " ").trim().toLowerCase();
  if (tail.length > 0) statements.push(tail);
  return statements;
}

function maskCodeTrivia(source) {
  const output = [...source];
  let state = "code";
  let quote = "";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    if (state === "line") {
      if (char === "\n") state = "code";
      else output[index] = " ";
    } else if (state === "block") {
      output[index] = char === "\n" ? "\n" : " ";
      if (char === "*" && next === "/") {
        output[index + 1] = " ";
        index += 1;
        state = "code";
      }
    } else if (state === "string") {
      output[index] = char === "\n" ? "\n" : " ";
      if (char === "\\") {
        output[index + 1] = " ";
        index += 1;
      } else if (char === quote) {
        state = "code";
      }
    } else if (char === "/" && next === "/") {
      output[index] = " ";
      output[index + 1] = " ";
      index += 1;
      state = "line";
    } else if (char === "/" && next === "*") {
      output[index] = " ";
      output[index + 1] = " ";
      index += 1;
      state = "block";
    } else if (char === "r" && /^(r#+")/.test(source.slice(index))) {
      const opener = source.slice(index).match(/^r(#+)"/);
      const terminator = `"${opener[1]}`;
      const end = source.indexOf(terminator, index + opener[0].length);
      if (end < 0) {
        throw new Error("rollout Rust source has an unterminated raw string");
      }
      const final = end + terminator.length;
      for (let cursor = index; cursor < final; cursor += 1) {
        output[cursor] = source[cursor] === "\n" ? "\n" : " ";
      }
      index = final - 1;
    } else if (
      char === "'" &&
      ((next === "\\" && source[index + 3] === "'") ||
        (next !== "\\" && source[index + 2] === "'"))
    ) {
      const final = next === "\\" ? index + 4 : index + 3;
      for (let cursor = index; cursor < final; cursor += 1) {
        output[cursor] = " ";
      }
      index = final - 1;
    } else if (
      char === "'" &&
      /[A-Za-z_]/.test(next ?? "") &&
      source[index + 2] !== "'"
    ) {
      continue;
    } else if (char === "'" || char === '"' || char === "`") {
      output[index] = " ";
      quote = char;
      state = "string";
    }
  }
  return output.join("");
}

function matchingDelimiter(source, start, open, close) {
  let depth = 0;
  for (let index = start; index < source.length; index += 1) {
    if (source[index] === open) depth += 1;
    if (source[index] === close) {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function rustFunctions(source) {
  const masked = maskCodeTrivia(source);
  const functions = new Map();
  const declaration =
    /\b(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_]\w*)\s*[<(]/g;
  for (const match of masked.matchAll(declaration)) {
    const name = match[1];
    const brace = masked.indexOf("{", match.index + match[0].length);
    if (brace < 0) continue;
    const end = matchingDelimiter(masked, brace, "{", "}");
    if (end < 0) {
      throw new Error(`rollout Rust function ${name} has unbalanced braces`);
    }
    functions.set(name, {
      body: masked.slice(brace + 1, end),
      source: source.slice(brace + 1, end),
    });
  }
  return functions;
}

function rustFunctionByName(source, name) {
  const declaration = new RegExp(`\\bfn\\s+${name}\\s*\\(`);
  const start = source.search(declaration);
  if (start < 0) return null;
  const tail = source.slice(start);
  const masked = maskCodeTrivia(tail);
  const brace = masked.indexOf("{");
  if (brace < 0) return null;
  const end = matchingDelimiter(masked, brace, "{", "}");
  if (end < 0) {
    throw new Error(`rollout Rust function ${name} has unbalanced braces`);
  }
  return {
    body: masked.slice(brace + 1, end),
    source: tail.slice(brace + 1, end),
  };
}

function functionCalls(body) {
  const calls = new Set();
  for (const match of body.matchAll(
    /(?:\.\s*|::\s*|\b)([A-Za-z_]\w*)\s*\(/g,
  )) {
    calls.add(match[1]);
  }
  return calls;
}

function reachableFunctions(functions, roots) {
  const reachable = new Set();
  const pending = [...roots];
  while (pending.length > 0) {
    const name = pending.pop();
    if (reachable.has(name) || !functions.has(name)) continue;
    reachable.add(name);
    for (const called of functionCalls(functions.get(name).body)) {
      if (!reachable.has(called)) pending.push(called);
    }
  }
  return reachable;
}

function requireShippingClientDataflow(source) {
  const functions = rustFunctions(source);
  const boundary = functions.get("get_control_inbox_compatible_from");
  if (!boundary) {
    throw new Error("shipping client compatibility function is absent");
  }
  const code = boundary.body.replace(/\s+/g, " ");
  const probeCalls =
    code.match(/\bprobe_control_inbox_sender_filter_capability\s*\(/g) ?? [];
  const floorLoads =
    code.match(/\bload_sender_filter_capability_floor\s*\(/g) ?? [];
  const floorRecords =
    code.match(/\brecord_sender_filter_capability_floor\s*\(/g) ?? [];
  if (
    probeCalls.length !== 1 ||
    floorLoads.length !== 2 ||
    floorRecords.length !== 1 ||
    !/let\s+capability\s*=\s*self\s*\.\s*probe_control_inbox_sender_filter_capability\s*\(\s*\)\s*\?\s*;\s*let\s+initial_floor\s*=\s*load_sender_filter_capability_floor\s*\(\s*identity\s*\)\s*\?\s*;\s*let\s+measured_floor\s*=\s*match\s*\(\s*capability\s*,\s*initial_floor\s*\)\s*\{\s*\(\s*ControlInboxSenderFilterCapability\s*::\s*Version1\s*,\s*SenderFilterCapabilityFloor\s*::\s*NeverObserved\s*,?\s*\)\s*=>\s*\{\s*record_sender_filter_capability_floor\s*\(\s*identity\s*,\s*unix_timestamp_ms\s*\(\s*\)\s*,?\s*\)\s*\?\s*;\s*load_sender_filter_capability_floor\s*\(\s*identity\s*\)\s*\?\s*\}\s*,?\s*\(\s*_\s*,\s*floor\s*\)\s*=>\s*floor\s*,?\s*\}\s*;\s*match\s*\(\s*capability\s*,\s*measured_floor\s*\)\s*\{/.test(
      code,
    )
  ) {
    throw new Error(
      "shipping client ignores, falsifies, or bypasses measured capability-floor dataflow",
    );
  }
  for (const branch of [
    /Version1\s*,\s*SenderFilterCapabilityFloor\s*::\s*Version1[\s\S]*get_control_inbox_from/,
    /Version1\s*,\s*SenderFilterCapabilityFloor\s*::\s*NeverObserved[\s\S]*Err\s*\(/,
    /Legacy\s*,\s*SenderFilterCapabilityFloor\s*::\s*NeverObserved[\s\S]*get_control_inbox\s*\(/,
    /Legacy\s*,\s*SenderFilterCapabilityFloor\s*::\s*Version1[\s\S]*Err\s*\(/,
  ]) {
    if (!branch.test(code)) {
      throw new Error("shipping client compatibility branch is incomplete");
    }
  }
}

function requireBrokerCallGraph(source) {
  const production = source.split("\n#[cfg(test)]\nmod tests")[0];
  const boundary = rustFunctionByName(
    production,
    "fetch_peer_control_inbox",
  );
  if (!boundary) {
    throw new Error("broker sender-filter boundary is absent");
  }
  const code = boundary.body.replace(/\s+/g, " ").trim();
  if (
    !/^client\s*\.\s*get_control_inbox_compatible_from\s*\(\s*identity\s*,\s*peer_osl_user_id\s*\)\s*$/.test(
      code,
    )
  ) {
    throw new Error(
      "broker sender-filter boundary has a dead branch or direct bypass",
    );
  }
  const calls =
    maskCodeTrivia(production).match(/\bfetch_peer_control_inbox\s*\(/g) ??
    [];
  if (calls.length < 2) {
    throw new Error("broker production drains do not reach the filtered boundary");
  }
}

function requireNonLowerableFloor(source) {
  const production = source.split("#[cfg(test)]")[0];
  const functions = rustFunctions(production);
  const reachable = reachableFunctions(functions, [
    "load_sender_filter_capability_floor",
    "record_sender_filter_capability_floor",
  ]);
  for (const required of [
    "load_sender_filter_capability_floor",
    "record_sender_filter_capability_floor",
    "load_from_paths",
    "write_receipt_atomically",
    "sync_parent",
  ]) {
    if (!reachable.has(required)) {
      throw new Error(`capability floor call graph does not reach ${required}`);
    }
  }
  const load = functions.get("load_from_paths").body.replace(/\s+/g, " ");
  if (
    !/\(\s*None\s*,\s*None\s*\)\s*=>\s*Err\s*\(/.test(load) ||
    /Ok\s*\(\s*SenderFilterCapabilityFloor\s*::\s*NeverObserved\s*\)/.test(
      load,
    ) ||
    !/\(\s*Some\s*\([^)]*\)\s*,\s*Some\s*\([^)]*\)\s*\)[\s\S]*Version1/.test(
      load,
    ) ||
    !/_\s*=>\s*Err\s*\(/.test(load)
  ) {
    throw new Error(
      "capability floor does not fail closed on one-sided identity-anchor absence",
    );
  }
  const atomic = functions
    .get("write_receipt_atomically")
    .body.replace(/\s+/g, " ");
  if (
    !/file\s*\.\s*sync_all\s*\(\s*\)\s*\?/.test(atomic) ||
    !/fs\s*::\s*rename\s*\(\s*&\s*temporary\s*,\s*path\s*\)\s*\?/.test(
      atomic,
    ) ||
    !/sync_parent\s*\(\s*path\s*\)\s*\?/.test(atomic)
  ) {
    throw new Error(
      "capability floor atomic replacement or parent durability is absent",
    );
  }
  const record = functions
    .get("record_sender_filter_capability_floor")
    .body.replace(/\s+/g, " ");
  if (
    !/write_receipt_atomically\s*\(\s*&\s*anchor_path[\s\S]*write_receipt_atomically\s*\(\s*&\s*local_path/.test(
      record,
    )
  ) {
    throw new Error("capability floor is not anchored before local publication");
  }
  const code = maskCodeTrivia(production);
  const forbidden = new Set([
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "set_len",
    "truncate",
  ]);
  const aliases = new Set(forbidden);
  let changed = true;
  while (changed) {
    changed = false;
    for (const match of code.matchAll(
      /\buse\s+(?:std\s*::\s*)?(?:fs\s*::\s*)?([A-Za-z_]\w*)\s+as\s+([A-Za-z_]\w*)/g,
    )) {
      if (aliases.has(match[1]) && !aliases.has(match[2])) {
        aliases.add(match[2]);
        changed = true;
      }
    }
    for (const match of code.matchAll(
      /\blet\s+([A-Za-z_]\w*)(?:\s*:[^=;]+)?\s*=\s*(?:std\s*::\s*)?(?:fs\s*::\s*)?([A-Za-z_]\w*)\s*;/g,
    )) {
      if (aliases.has(match[2]) && !aliases.has(match[1])) {
        aliases.add(match[1]);
        changed = true;
      }
    }
    for (const match of code.matchAll(
      /\b([A-Za-z_]\w*)\s+as\s+([A-Za-z_]\w*)/g,
    )) {
      if (aliases.has(match[1]) && !aliases.has(match[2])) {
        aliases.add(match[2]);
        changed = true;
      }
    }
  }
  const called = functionCalls(code);
  for (const alias of aliases) {
    if (called.has(alias)) {
      throw new Error(
        "sender-filter capability floor exposes an aliased lowering path",
      );
    }
  }
}

export function validateRolloutSourceClosure(filesValue) {
  const files = requireObject(filesValue, "rollout source closure");
  requireExactKeys(
    files,
    ROLLOUT_SOURCE_PATHS,
    "rollout source closure",
  );
  for (const sourcePath of ROLLOUT_SOURCE_PATHS) {
    if (
      typeof files[sourcePath] !== "string" ||
      files[sourcePath].length === 0
    ) {
      throw new Error(`rollout source is empty: ${sourcePath}`);
    }
  }

  const migration = executableSqlStatements(
    files[
      "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
    ],
  );
  for (const sql of [
    "alter table control_inbox add column delivery_status",
    "create table if not exists worker_schema_capabilities",
    "insert into worker_schema_capabilities (capability, version)",
    "create trigger control_inbox_retention_delete_guard",
  ]) {
    if (!migration.some((statement) => statement.includes(sql))) {
      throw new Error(`migration 0031 semantic contract missing ${sql}`);
    }
  }
  const uncommentedIndex = stripComments(files["keyserver-cf/src/index.ts"]);
  if (
    !uncommentedIndex.includes('path === "/v1/healthz"') ||
    !uncommentedIndex.includes(
      "handleControlInboxGet(request, env, inboxUserId)",
    )
  ) {
    throw new Error("Worker route entrypoint binding mismatch");
  }
  requireCode(
    files["keyserver-cf/src/index.ts"],
    "keyserver-cf/src/index.ts",
    [/handleHealthz\s*\(\s*env\s*\)/, /handleControlInboxGet\s*\(/],
  );
  requireCode(
    files["keyserver-cf/src/endpoints/healthz.ts"],
    "keyserver-cf/src/endpoints/healthz.ts",
    [
      /controlInboxDispositionSchemaReady\s*\(\s*env\s*\.\s*DB\s*\)/,
      /controlInboxSenderDisposition\s*\?\s*1\s*:\s*0/,
      /controlInboxSenderDisposition\s*\?\s*undefined\s*:\s*\{\s*status\s*:\s*503\s*\}/,
    ],
  );
  requireCode(
    files["keyserver-cf/src/endpoints/control-inbox.ts"],
    "keyserver-cf/src/endpoints/control-inbox.ts",
    [
      /rawSender\s*!==\s*null\s*&&\s*!isProtocolId\s*\(\s*rawSender\s*\)/,
      /sender_id\s*:\s*senderFilter/,
      /verifyEd25519\s*\(/,
      /senderFilter\s*===\s*null/,
      /filtered_sender_id\s*:\s*senderFilter/,
    ],
  );
  requireCode(
    files["keyserver-cf/src/lib/canonical.ts"],
    "keyserver-cf/src/lib/canonical.ts",
    [
      /args\s*\.\s*sender_id\s*!==\s*undefined\s*&&\s*args\s*\.\s*sender_id\s*!==\s*null/,
      /parts\s*\.\s*push\s*\(\s*lpString\s*\(\s*args\s*\.\s*sender_id\s*\)\s*\)/,
    ],
  );
  requireCode(
    files["keyserver-cf/src/readiness/bridge/control-inbox.ts"],
    "keyserver-cf/src/readiness/bridge/control-inbox.ts",
    [/handleControlInboxGet[\s\S]*serviceUnavailable\s*\(\s*BRIDGE_UNAVAILABLE\s*\)/],
  );
  const bridgeHealth = stripComments(
    files["keyserver-cf/src/readiness/bridge/healthz.ts"],
  );
  if (
    !bridgeHealth.includes(
      'readiness_artifact: "A-pre-0031-bridge"',
    )
  ) {
    throw new Error("Artifact A health marker binding mismatch");
  }
  requireShippingClientDataflow(files["crates/keystore/src/client.rs"]);
  const floorProduction = files[
    "crates/keystore/src/sender_filter_rollout.rs"
  ].split("#[cfg(test)]")[0];
  requireCode(
    floorProduction,
    "crates/keystore/src/sender_filter_rollout.rs",
    [
      /osl_config_dir\s*\(\s*\)/,
      /osl_base_dir\s*\(\s*\)/,
      /identity_anchor_sha256\s*\(\s*identity\s*\)/,
      /write_receipt_atomically\s*\(/,
      /sync_parent\s*\(\s*path\s*\)/,
      /crypto\s*::\s*ed25519\s*::\s*sign\s*\(/,
      /crypto\s*::\s*ed25519\s*::\s*verify\s*\(/,
      /SenderFilterCapabilityFloor\s*::\s*Version1/,
    ],
  );
  requireNonLowerableFloor(
    files["crates/keystore/src/sender_filter_rollout.rs"],
  );
  requireBrokerCallGraph(files["apps/osl-hub/src/broker.rs"]);

  const fileSha256 = Object.fromEntries(
    ROLLOUT_SOURCE_PATHS.map((sourcePath) => [
      sourcePath,
      sha256(Buffer.from(files[sourcePath])),
    ]),
  );
  return Object.freeze({
    file_sha256: Object.freeze(fileSha256),
    source_closure_sha256: sha256(
      Buffer.from(canonicalJson(fileSha256)),
    ),
  });
}
