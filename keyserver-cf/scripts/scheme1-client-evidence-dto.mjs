export const CROSS_LANGUAGE_CASE_IDS = Object.freeze([
  "canonical-fixture-json",
  "full-bundle-root-current-registration-signatures",
  "rn-capability-authentication",
  "opk-owner-proof",
  "replenish-idempotent-response-replay",
  "consuming-fetch-response",
  "shipping-scheme1-register-fetch-replenish-reachability",
]);

export const RESTART_CASE_IDS = Object.freeze([
  "scheme-pin-survives-process-restart",
  "generation-batch-pin-survives-process-restart",
]);

export const DOWNGRADE_CASE_IDS = Object.freeze([
  "stripped-scheme",
  "stripped-root-proof",
  "noncanonical-signature",
  "legacy-after-scheme1-pin",
  "lower-generation",
  "different-batch-same-generation",
]);

export const SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT =
  "osl.rust-client.scheme1-client-evidence.v1";
export const SCHEME1_CROSS_LANGUAGE_RECEIPT_FORMAT =
  "osl.rust-client.scheme1-cross-language-receipt.v1";
export const SCHEME1_RESTART_RECEIPT_FORMAT =
  "osl.rust-client.scheme1-restart-receipt.v1";
export const SCHEME1_DOWNGRADE_RECEIPT_FORMAT =
  "osl.rust-client.scheme1-downgrade-receipt.v1";

const TOP_LEVEL_FIELDS = Object.freeze([
  "format",
  "client",
  "contract",
  "run",
  "cross_language_receipt",
  "restart_receipt",
  "downgrade_receipt",
]);

const CLIENT_FIELDS = Object.freeze(["commit", "repository_tree"]);
const CONTRACT_FIELDS = Object.freeze([
  "server_commit",
  "server_repository_tree",
  "server_keyserver_tree",
  "descriptor_sha256",
  "fixture_sha256",
  "fixture_path",
]);
const RUN_FIELDS = Object.freeze([
  "runner_name",
  "runner_binary_sha256",
  "command_argv",
  "started_at",
  "finished_at",
  "exit_code",
  "test_count",
]);
const CROSS_RECEIPT_FIELDS = Object.freeze([
  "format",
  "receipt_sha256",
  "case_count",
  "cases",
]);
const RESTART_RECEIPT_FIELDS = Object.freeze([
  "format",
  "receipt_sha256",
  "pre_process_sha256",
  "post_process_sha256",
  "state_sha256",
  "identity_scheme",
  "lifecycle_generation",
  "generation_batch",
  "cases",
]);
const DOWNGRADE_RECEIPT_FIELDS = Object.freeze([
  "format",
  "receipt_sha256",
  "cases",
]);

const HEX40 = /^[0-9a-f]{40}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const PADDED_BASE64_32_BYTES = /^[A-Za-z0-9+/]{43}=$/;

export function canonicalJson(value) {
  return JSON.stringify(canonicalize(value));
}

export function parseScheme1ClientEvidencePayload(value) {
  assertPlainObject(value, "payload");
  assertExactKeys(value, TOP_LEVEL_FIELDS, "payload");
  assertLiteral(
    value.format,
    SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT,
    "payload.format",
  );

  return {
    format: value.format,
    client: parseClient(value.client),
    contract: parseContract(value.contract),
    run: parseRun(value.run),
    cross_language_receipt: parseCrossLanguageReceipt(
      value.cross_language_receipt,
    ),
    restart_receipt: parseRestartReceipt(value.restart_receipt),
    downgrade_receipt: parseDowngradeReceipt(value.downgrade_receipt),
  };
}

function parseClient(value) {
  assertPlainObject(value, "payload.client");
  assertExactKeys(value, CLIENT_FIELDS, "payload.client");

  return {
    commit: assertHex(value.commit, 40, "payload.client.commit"),
    repository_tree: assertHex(
      value.repository_tree,
      40,
      "payload.client.repository_tree",
    ),
  };
}

function parseContract(value) {
  assertPlainObject(value, "payload.contract");
  assertExactKeys(value, CONTRACT_FIELDS, "payload.contract");

  return {
    server_commit: assertHex(
      value.server_commit,
      40,
      "payload.contract.server_commit",
    ),
    server_repository_tree: assertHex(
      value.server_repository_tree,
      40,
      "payload.contract.server_repository_tree",
    ),
    server_keyserver_tree: assertHex(
      value.server_keyserver_tree,
      40,
      "payload.contract.server_keyserver_tree",
    ),
    descriptor_sha256: assertHex(
      value.descriptor_sha256,
      64,
      "payload.contract.descriptor_sha256",
    ),
    fixture_sha256: assertHex(
      value.fixture_sha256,
      64,
      "payload.contract.fixture_sha256",
    ),
    fixture_path: assertNonEmptyString(
      value.fixture_path,
      "payload.contract.fixture_path",
    ),
  };
}

function parseRun(value) {
  assertPlainObject(value, "payload.run");
  assertExactKeys(value, RUN_FIELDS, "payload.run");

  const startedAt = assertParseableDate(value.started_at, "payload.run.started_at");
  const finishedAt = assertParseableDate(
    value.finished_at,
    "payload.run.finished_at",
  );
  if (finishedAt < startedAt) {
    fail("payload.run.finished_at", "must be greater than or equal to started_at");
  }

  if (value.exit_code !== 0) {
    fail("payload.run.exit_code", "must be 0");
  }

  return {
    runner_name: assertNonEmptyString(
      value.runner_name,
      "payload.run.runner_name",
    ),
    runner_binary_sha256: assertHex(
      value.runner_binary_sha256,
      64,
      "payload.run.runner_binary_sha256",
    ),
    command_argv: assertNonEmptyStringArray(
      value.command_argv,
      "payload.run.command_argv",
    ),
    started_at: value.started_at,
    finished_at: value.finished_at,
    exit_code: value.exit_code,
    test_count: assertPositiveInteger(
      value.test_count,
      "payload.run.test_count",
    ),
  };
}

function parseCrossLanguageReceipt(value) {
  assertPlainObject(value, "payload.cross_language_receipt");
  assertExactKeys(
    value,
    CROSS_RECEIPT_FIELDS,
    "payload.cross_language_receipt",
  );
  assertLiteral(
    value.format,
    SCHEME1_CROSS_LANGUAGE_RECEIPT_FORMAT,
    "payload.cross_language_receipt.format",
  );
  if (value.case_count !== CROSS_LANGUAGE_CASE_IDS.length) {
    fail(
      "payload.cross_language_receipt.case_count",
      `must be ${CROSS_LANGUAGE_CASE_IDS.length}`,
    );
  }

  return {
    format: value.format,
    receipt_sha256: assertHex(
      value.receipt_sha256,
      64,
      "payload.cross_language_receipt.receipt_sha256",
    ),
    case_count: value.case_count,
    cases: normalizeWitnessCases({
      cases: value.cases,
      expectedIds: CROSS_LANGUAGE_CASE_IDS,
      witnessField: "witness_sha256",
      path: "payload.cross_language_receipt.cases",
    }),
  };
}

function parseRestartReceipt(value) {
  assertPlainObject(value, "payload.restart_receipt");
  assertExactKeys(value, RESTART_RECEIPT_FIELDS, "payload.restart_receipt");
  assertLiteral(
    value.format,
    SCHEME1_RESTART_RECEIPT_FORMAT,
    "payload.restart_receipt.format",
  );
  const preProcessSha256 = assertHex(
    value.pre_process_sha256,
    64,
    "payload.restart_receipt.pre_process_sha256",
  );
  const postProcessSha256 = assertHex(
    value.post_process_sha256,
    64,
    "payload.restart_receipt.post_process_sha256",
  );
  if (preProcessSha256 === postProcessSha256) {
    fail(
      "payload.restart_receipt.post_process_sha256",
      "must differ from pre_process_sha256",
    );
  }
  if (value.identity_scheme !== 1) {
    fail("payload.restart_receipt.identity_scheme", "must be 1");
  }

  return {
    format: value.format,
    receipt_sha256: assertHex(
      value.receipt_sha256,
      64,
      "payload.restart_receipt.receipt_sha256",
    ),
    pre_process_sha256: preProcessSha256,
    post_process_sha256: postProcessSha256,
    state_sha256: assertHex(
      value.state_sha256,
      64,
      "payload.restart_receipt.state_sha256",
    ),
    identity_scheme: value.identity_scheme,
    lifecycle_generation: assertPositiveInteger(
      value.lifecycle_generation,
      "payload.restart_receipt.lifecycle_generation",
    ),
    generation_batch: assertCanonicalPaddedBase64Batch(
      value.generation_batch,
      "payload.restart_receipt.generation_batch",
    ),
    cases: normalizeWitnessCases({
      cases: value.cases,
      expectedIds: RESTART_CASE_IDS,
      witnessField: "witness_sha256",
      path: "payload.restart_receipt.cases",
    }),
  };
}

function parseDowngradeReceipt(value) {
  assertPlainObject(value, "payload.downgrade_receipt");
  assertExactKeys(value, DOWNGRADE_RECEIPT_FIELDS, "payload.downgrade_receipt");
  assertLiteral(
    value.format,
    SCHEME1_DOWNGRADE_RECEIPT_FORMAT,
    "payload.downgrade_receipt.format",
  );

  return {
    format: value.format,
    receipt_sha256: assertHex(
      value.receipt_sha256,
      64,
      "payload.downgrade_receipt.receipt_sha256",
    ),
    cases: normalizeWitnessCases({
      cases: value.cases,
      expectedIds: DOWNGRADE_CASE_IDS,
      witnessField: "witness",
      disposition: "refused",
      path: "payload.downgrade_receipt.cases",
    }),
  };
}

function normalizeWitnessCases({
  cases,
  expectedIds,
  witnessField,
  disposition,
  path,
}) {
  if (!Array.isArray(cases)) {
    fail(path, "must be an array");
  }
  if (cases.length !== expectedIds.length) {
    fail(path, `must contain exactly ${expectedIds.length} cases`);
  }

  const expected = new Set(expectedIds);
  const seenIds = new Set();
  const seenWitnesses = new Set();
  const byId = new Map();
  const caseFields =
    disposition === undefined
      ? Object.freeze(["id", witnessField])
      : Object.freeze(["id", "disposition", witnessField]);

  for (let index = 0; index < cases.length; index += 1) {
    const testCase = cases[index];
    const casePath = `${path}[${index}]`;
    assertPlainObject(testCase, casePath);
    assertExactKeys(testCase, caseFields, casePath);

    const id = assertNonEmptyString(testCase.id, `${casePath}.id`);
    if (!expected.has(id)) {
      fail(`${casePath}.id`, "is not an expected case id");
    }
    if (seenIds.has(id)) {
      fail(`${casePath}.id`, "must be unique");
    }
    seenIds.add(id);

    if (disposition !== undefined) {
      assertLiteral(testCase.disposition, disposition, `${casePath}.disposition`);
    }

    const witness = assertHex(
      testCase[witnessField],
      64,
      `${casePath}.${witnessField}`,
    );
    if (seenWitnesses.has(witness)) {
      fail(`${casePath}.${witnessField}`, "must be unique");
    }
    seenWitnesses.add(witness);

    byId.set(
      id,
      disposition === undefined
        ? { id, [witnessField]: witness }
        : { id, disposition: testCase.disposition, [witnessField]: witness },
    );
  }

  for (const id of expectedIds) {
    if (!byId.has(id)) {
      fail(path, `is missing case id ${id}`);
    }
  }

  return expectedIds.map((id) => byId.get(id));
}

function canonicalize(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      fail("value", "must not contain non-finite numbers");
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => canonicalize(item));
  }
  if (isPlainObject(value)) {
    const normalized = {};
    for (const key of Object.keys(value).sort()) {
      const item = value[key];
      if (item === undefined) {
        fail(`value.${key}`, "must not be undefined");
      }
      normalized[key] = canonicalize(item);
    }
    return normalized;
  }
  fail("value", "must contain only JSON-compatible values");
}

function assertPlainObject(value, path) {
  if (!isPlainObject(value)) {
    fail(path, "must be a plain object");
  }
}

function isPlainObject(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function assertExactKeys(value, expectedKeys, path) {
  const symbols = Object.getOwnPropertySymbols(value);
  if (symbols.length > 0) {
    fail(path, "must not contain symbol keys");
  }

  const expected = new Set(expectedKeys);
  for (const key of Object.keys(value)) {
    if (!expected.has(key)) {
      fail(`${path}.${key}`, "is not an expected field");
    }
  }
  for (const key of expectedKeys) {
    if (!Object.prototype.hasOwnProperty.call(value, key)) {
      fail(`${path}.${key}`, "is required");
    }
  }
}

function assertLiteral(value, expected, path) {
  if (value !== expected) {
    fail(path, `must be ${JSON.stringify(expected)}`);
  }
  return value;
}

function assertNonEmptyString(value, path) {
  if (typeof value !== "string" || value.length === 0) {
    fail(path, "must be a nonempty string");
  }
  return value;
}

function assertNonEmptyStringArray(value, path) {
  if (!Array.isArray(value) || value.length === 0) {
    fail(path, "must be a nonempty array");
  }
  return value.map((item, index) =>
    assertNonEmptyString(item, `${path}[${index}]`),
  );
}

function assertPositiveInteger(value, path) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    fail(path, "must be a positive integer");
  }
  return value;
}

function assertParseableDate(value, path) {
  assertNonEmptyString(value, path);
  const time = Date.parse(value);
  if (!Number.isFinite(time)) {
    fail(path, "must be parseable as a date");
  }
  return time;
}

function assertHex(value, length, path) {
  assertNonEmptyString(value, path);
  const pattern = length === 40 ? HEX40 : HEX64;
  if (!pattern.test(value)) {
    fail(path, `must be lowercase hex length ${length}`);
  }
  if (/^0+$/.test(value)) {
    fail(path, "must be nonzero");
  }
  return value;
}

function assertCanonicalPaddedBase64Batch(value, path) {
  assertNonEmptyString(value, path);
  if (!PADDED_BASE64_32_BYTES.test(value)) {
    fail(path, "must be canonical padded base64 for exactly 32 bytes");
  }

  const bytes = Buffer.from(value, "base64");
  if (
    bytes.length !== 32 ||
    bytes.every((byte) => byte === 0) ||
    bytes.toString("base64") !== value
  ) {
    fail(path, "must be canonical padded base64 for exactly 32 bytes");
  }
  return value;
}

function fail(path, message) {
  throw new TypeError(`${path}: ${message}`);
}
