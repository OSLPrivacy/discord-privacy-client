import {
  canonicalJson,
  sha256,
} from "./readiness-artifact-contract.mjs";

export const SENDER_FILTER_ROLLOUT_FORMAT =
  "osl.keyserver.sender-filter-rollout-contract.v2";
export const SENDER_FILTER_PLAN_FORMAT =
  "osl.keyserver.sender-filter-rollout-plan.v2";
export const SENDER_FILTER_PHASE_RECEIPT_FORMAT =
  "osl.keyserver.sender-filter-phase-receipt.v2";
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
  "call_sites",
  "capability_probe",
  "captured_at",
  "client_commit",
  "deployment_id",
  "expected_commit",
  "filtered_probe",
  "format",
  "legacy_probe",
  "migration_0031_sha256",
  "phase",
  "receipt_sha256",
  "source_closure_sha256",
  "traffic",
  "worker_commit",
  "worker_version",
]);
const CAPABILITY_PROBE_FIELDS = Object.freeze([
  "client_commit",
  "deployment_id",
  "mode",
  "phase",
  "status",
  "version",
  "worker_commit",
  "worker_version",
]);
const INBOX_PROBE_FIELDS = Object.freeze([
  "client_commit",
  "cross_sender_count",
  "deployment_id",
  "echo",
  "item_count",
  "phase",
  "request_mode",
  "status",
  "worker_commit",
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

function receiptDigest(receipt) {
  const payload = { ...receipt };
  delete payload.receipt_sha256;
  return sha256(Buffer.from(canonicalJson(payload)));
}

function validateBoundProbe(probeValue, fields, receipt, label) {
  const probe = requireObject(probeValue, label);
  requireExactKeys(probe, fields, label);
  if (
    probe.phase !== receipt.phase ||
    probe.client_commit !== receipt.client_commit ||
    probe.worker_commit !== receipt.worker_commit ||
    probe.worker_version !== receipt.worker_version ||
    probe.deployment_id !== receipt.deployment_id
  ) {
    throw new Error(`${label} is not bound to the receipt phase`);
  }
  return probe;
}

export function createSenderFilterPhaseReceipt(value) {
  const receipt = structuredClone(value);
  receipt.format = SENDER_FILTER_PHASE_RECEIPT_FORMAT;
  receipt.receipt_sha256 = receiptDigest(receipt);
  return receipt;
}

export function verifySenderFilterPhaseReceipt(
  receiptValue,
  sourceClosure,
  nowMs,
) {
  const receipt = requireObject(receiptValue, "rollout phase receipt");
  requireExactKeys(
    receipt,
    PHASE_RECEIPT_FIELDS,
    "rollout phase receipt",
  );
  if (receipt.format !== SENDER_FILTER_PHASE_RECEIPT_FORMAT) {
    throw new Error("rollout phase receipt format mismatch");
  }
  const phase = PHASES[receipt.phase];
  if (!phase) throw new Error("rollout phase is not exact");
  requireCommit(receipt.expected_commit, "rollout expected commit");
  requireCommit(receipt.worker_commit, "rollout Worker commit");
  requireCommit(receipt.client_commit, "rollout client commit");
  requireSha(receipt.migration_0031_sha256, "migration 0031 digest");
  requireSha(receipt.source_closure_sha256, "source closure digest");
  requireSha(receipt.receipt_sha256, "phase receipt digest");
  requireUuid(receipt.worker_version, "rollout Worker version");
  requireUuid(receipt.deployment_id, "rollout deployment id");
  requireChoice(receipt.traffic, ["active", "quiesced"], "rollout traffic");
  if (
    canonicalJson(receipt.call_sites) !== canonicalJson(ROLLOUT_CALL_SITES)
  ) {
    throw new Error("rollout call-site binding mismatch");
  }
  if (
    receipt.source_closure_sha256 !== sourceClosure.source_closure_sha256 ||
    receipt.migration_0031_sha256 !==
      sourceClosure.file_sha256[
        "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
      ]
  ) {
    throw new Error("rollout source or migration closure mismatch");
  }
  if (receipt.receipt_sha256 !== receiptDigest(receipt)) {
    throw new Error("rollout phase receipt digest mismatch");
  }
  const captured = Date.parse(receipt.captured_at);
  if (
    !Number.isFinite(captured) ||
    Math.abs(captured - nowMs) > 120_000
  ) {
    throw new Error("rollout phase receipt is stale or future-dated");
  }

  const capability = validateBoundProbe(
    receipt.capability_probe,
    CAPABILITY_PROBE_FIELDS,
    receipt,
    "rollout capability probe",
  );
  const legacy = validateBoundProbe(
    receipt.legacy_probe,
    INBOX_PROBE_FIELDS,
    receipt,
    "rollout legacy probe",
  );
  const filtered = validateBoundProbe(
    receipt.filtered_probe,
    INBOX_PROBE_FIELDS,
    receipt,
    "rollout filtered probe",
  );
  if (
    capability.mode !== phase.capability[0] ||
    capability.status !== phase.capability[1] ||
    capability.version !== phase.capability[2]
  ) {
    throw new Error("rollout capability probe phase mismatch");
  }
  for (const [probe, expected, label] of [
    [legacy, phase.legacy, "legacy"],
    [filtered, phase.filtered, "filtered"],
  ]) {
    const [requestMode, status, population] = expected;
    if (
      probe.request_mode !== requestMode ||
      probe.status !== status ||
      !Number.isSafeInteger(probe.item_count) ||
      !Number.isSafeInteger(probe.cross_sender_count) ||
      probe.item_count < 0 ||
      probe.cross_sender_count < 0 ||
      (population === "positive" && probe.item_count <= 0) ||
      (population === "empty" &&
        (probe.item_count !== 0 || probe.cross_sender_count !== 0))
    ) {
      throw new Error(`rollout ${label} probe phase mismatch`);
    }
  }
  if (
    filtered.request_mode === "filtered" &&
    (filtered.echo !== "sender-positive" ||
      filtered.cross_sender_count !== 0)
  ) {
    throw new Error("rollout filtered isolation probe mismatch");
  }
  if (
    filtered.request_mode !== "filtered" &&
    filtered.echo !== null
  ) {
    throw new Error("rollout non-filtered phase carried a sender echo");
  }
  return Object.freeze(structuredClone(receipt));
}

export function admitSenderFilterRolloutPlan({
  phaseReceipt,
  sourceClosure,
  nowMs,
}) {
  const receipt = verifySenderFilterPhaseReceipt(
    phaseReceipt,
    sourceClosure,
    nowMs,
  );
  const phase = PHASES[receipt.phase];
  const reasons = [];
  if (phase.worker === "artifact-a" && receipt.traffic !== "quiesced") {
    reasons.push("artifact-a-requires-quiesced-traffic");
  }
  if (
    phase.worker === "artifact-b" &&
    phase.schema === "pre-0031"
  ) {
    reasons.push("worker-first-artifact-b-refused");
  }
  let nextSelection = "none";
  if (reasons.length === 0) {
    if (receipt.phase === "legacy-pre-0031") {
      nextSelection =
        receipt.traffic === "quiesced"
          ? "artifact-a"
          : "quiesce-traffic";
    } else if (receipt.phase === "legacy-0031") {
      nextSelection = "artifact-b";
    } else if (receipt.phase === "artifact-a-pre-0031") {
      nextSelection = "migrations-0030-0031";
    } else if (receipt.phase === "artifact-a-0031") {
      nextSelection = "artifact-b";
    } else if (receipt.phase === "artifact-b-0031") {
      nextSelection = "stable-compatible";
    }
  }
  return {
    format: SENDER_FILTER_PLAN_FORMAT,
    plan_admitted: reasons.length === 0,
    direct_deploy_permitted: false,
    execution_authorized: false,
    next_selection: nextSelection,
    phase_receipt_sha256: receipt.receipt_sha256,
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

function normalizedSql(source) {
  return source
    .replace(/--[^\n]*/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase();
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

  const migration = normalizedSql(
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
    if (!migration.includes(sql)) {
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
  requireCode(
    files["crates/keystore/src/client.rs"],
    "crates/keystore/src/client.rs",
    [
      /get_control_inbox_compatible_from\s*\(/,
      /load_sender_filter_capability_floor\s*\(\s*identity\s*\)/,
      /probe_control_inbox_sender_filter_capability\s*\(\s*\)/,
      /record_sender_filter_capability_floor\s*\(/,
      /get_control_inbox_from\s*\(\s*identity\s*,\s*sender_id\s*\)/,
      /get_control_inbox\s*\(\s*identity\s*\)/,
    ],
  );
  const floorProduction = files[
    "crates/keystore/src/sender_filter_rollout.rs"
  ].split("#[cfg(test)]")[0];
  requireCode(
    floorProduction,
    "crates/keystore/src/sender_filter_rollout.rs",
    [
      /osl_config_dir\s*\(\s*\)/,
      /create_new\s*\(\s*true\s*\)/,
      /crypto\s*::\s*ed25519\s*::\s*sign\s*\(/,
      /crypto\s*::\s*ed25519\s*::\s*verify\s*\(/,
      /SenderFilterCapabilityFloor\s*::\s*Version1/,
    ],
  );
  if (
    /\b(?:remove_file|remove_dir|remove_dir_all|truncate|set_len)\s*\(/.test(
      stripCommentsAndStrings(floorProduction),
    )
  ) {
    throw new Error("sender-filter capability floor exposes a lowering path");
  }
  requireCode(
    files["apps/osl-hub/src/broker.rs"],
    "apps/osl-hub/src/broker.rs",
    [
      /fetch_peer_control_inbox\s*\(/,
      /get_control_inbox_compatible_from\s*\(\s*identity\s*,\s*peer_osl_user_id\s*\)/,
    ],
  );

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
