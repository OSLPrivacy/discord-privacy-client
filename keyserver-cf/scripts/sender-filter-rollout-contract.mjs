export const SENDER_FILTER_ROLLOUT_FORMAT =
  "osl.keyserver.sender-filter-rollout-contract.v1";
export const SENDER_FILTER_PLAN_FORMAT =
  "osl.keyserver.sender-filter-rollout-plan.v1";
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

  const expectedSignedSender =
    worker === "legacy" ? null : request.sender_param;
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

  if (worker === "legacy" || request.sender_param === null) {
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
  sourceRows,
}) {
  requireChoice(mode, ["legacy", "filtered"], "client drain mode");
  if (!validProtocolSender(senderId)) {
    throw new Error("selected sender is malformed");
  }
  const rows = validateRows(sourceRows);
  const selectedRows = rows.filter((row) => row.sender_id === senderId);
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
    if (selectedRows.length > 0 && response.items.length === 0) {
      return {
        accepted: false,
        fail_closed: true,
        reason: "filtered-positive-starved",
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
    sourceRows: rows,
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

function validateProbe(value, label, expectedFields) {
  const probe = requireObject(value, label);
  requireExactKeys(probe, expectedFields, label);
  return probe;
}

export function admitSenderFilterRolloutPlan({
  worker,
  schema,
  traffic,
  capabilityProbe,
  legacyProbe,
  filteredProbe,
}) {
  requireChoice(worker, ROLLOUT_WORKERS, "rollout Worker");
  requireChoice(schema, ROLLOUT_SCHEMAS, "rollout schema");
  requireChoice(traffic, ["active", "quiesced"], "rollout traffic state");
  const legacy = validateProbe(
    legacyProbe,
    "legacy continuity probe",
    ["item_count", "status"],
  );
  const filtered = validateProbe(
    filteredProbe,
    "filtered isolation probe",
    ["cross_sender_count", "echo", "item_count", "status"],
  );
  const reasons = [];
  const actualHealth = workerHealth(worker, schema);
  if (JSON.stringify(capabilityProbe) !== JSON.stringify(actualHealth)) {
    reasons.push("capability-probe-mismatch");
  }
  if (
    legacy.status !== 200 ||
    !Number.isSafeInteger(legacy.item_count) ||
    legacy.item_count <= 0
  ) {
    reasons.push("legacy-inbox-continuity-unproved");
  }
  if (worker === "artifact-a" && traffic !== "quiesced") {
    reasons.push("artifact-a-requires-quiesced-traffic");
  }
  if (worker === "artifact-b" && schema === "pre-0031") {
    reasons.push("worker-first-artifact-b-refused");
  }
  if (worker === "artifact-b" && schema === "0031") {
    if (
      filtered.status !== 200 ||
      filtered.echo !== "sender-positive" ||
      !Number.isSafeInteger(filtered.item_count) ||
      filtered.item_count <= 0 ||
      filtered.cross_sender_count !== 0
    ) {
      reasons.push("filtered-isolation-unproved");
    }
  } else {
    const expectedRefusalStatus = worker === "legacy" ? 401 : 503;
    if (
      filtered.status !== expectedRefusalStatus ||
      filtered.echo !== null ||
      filtered.item_count !== 0 ||
      filtered.cross_sender_count !== 0
    ) {
      reasons.push("filtered-probe-present-before-artifact-b");
    }
  }

  let nextSelection = "none";
  if (reasons.length === 0) {
    if (worker === "legacy" && schema === "pre-0031") {
      nextSelection =
        traffic === "quiesced" ? "artifact-a" : "quiesce-traffic";
    } else if (worker === "artifact-a" && schema === "pre-0031") {
      nextSelection = "migrations-0030-0031";
    } else if (worker === "artifact-a" && schema === "0031") {
      nextSelection = "artifact-b";
    } else if (worker === "legacy" && schema === "0031") {
      nextSelection = "artifact-b";
    } else if (worker === "artifact-b" && schema === "0031") {
      nextSelection = "stable-compatible";
    }
  }
  return {
    format: SENDER_FILTER_PLAN_FORMAT,
    plan_admitted: reasons.length === 0,
    direct_deploy_permitted: false,
    execution_authorized: false,
    next_selection: nextSelection,
    reasons,
  };
}

export function validateRolloutSourceClosure(filesValue) {
  const files = requireObject(filesValue, "rollout source closure");
  const expected = {
    "keyserver-cf/src/endpoints/control-inbox.ts": [
      "rawSender !== null && !isProtocolId(rawSender)",
      "sender_id: senderFilter",
      "WHERE recipient_id = ? AND sender_id = ?",
      "filtered_sender_id: senderFilter",
      "The unfiltered form is unchanged and still supported",
    ],
    "keyserver-cf/src/endpoints/healthz.ts": [
      "control_inbox_sender_disposition: controlInboxSenderDisposition ? 1 : 0",
      "controlInboxSenderDisposition ? undefined : { status: 503 }",
    ],
    "keyserver-cf/src/readiness/bridge/control-inbox.ts": [
      "return serviceUnavailable(BRIDGE_UNAVAILABLE)",
    ],
    "keyserver-cf/src/readiness/bridge/healthz.ts": [
      'readiness_artifact: "A-pre-0031-bridge"',
    ],
    "keyserver-cf/src/lib/canonical.ts": [
      "sender_id?: string | null",
      "parts.push(lpString(args.sender_id))",
    ],
  };
  requireExactKeys(files, Object.keys(expected), "rollout source closure");
  for (const [sourcePath, requiredText] of Object.entries(expected)) {
    const source = files[sourcePath];
    if (typeof source !== "string" || source.length === 0) {
      throw new Error(`rollout source is empty: ${sourcePath}`);
    }
    for (const text of requiredText) {
      if (!source.includes(text)) {
        throw new Error(
          `rollout source contract missing ${JSON.stringify(text)}: ${sourcePath}`,
        );
      }
    }
  }
  return true;
}
