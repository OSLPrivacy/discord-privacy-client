import {
  createHash,
  createPublicKey,
  verify as verifySignature,
} from "node:crypto";
import {
  canonicalJson,
  CROSS_LANGUAGE_CASE_IDS,
  DOWNGRADE_CASE_IDS,
  parseScheme1ClientEvidencePayload,
  RESTART_CASE_IDS,
  SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT,
} from "./scheme1-client-evidence-dto.mjs";

export const SCHEME1_FROZEN_SERVER_CONTRACT = Object.freeze({
  commit: "6cb2b5a275b3676869d2a69fc10a8b6544dbc9e1",
  repository_tree: "9283ded794ce13f51a278ecbc17b6d11e7d71a7c",
  keyserver_tree: "148eb20bc2f9e75356af8b119943496e56f72d89",
  descriptor_sha256:
    "8041c9c14f841935c6b42e74829e39747e6b8915bf4c929c80ff1d3e189dffaa",
  fixture_sha256:
    "8cebfb7bd94a3614178d6cce1af8ed8d36d776978ac57fdc5f407b6e0ce97a6f",
  fixture_path:
    "keyserver-cf/test/fixtures/scheme1-contract-vectors.json",
});
export const SCHEME1_RUST_CLIENT_CONTRACT_BINDING = Object.freeze({
  server_commit: SCHEME1_FROZEN_SERVER_CONTRACT.commit,
  server_repository_tree: SCHEME1_FROZEN_SERVER_CONTRACT.repository_tree,
  server_keyserver_tree: SCHEME1_FROZEN_SERVER_CONTRACT.keyserver_tree,
  descriptor_sha256: SCHEME1_FROZEN_SERVER_CONTRACT.descriptor_sha256,
  fixture_sha256: SCHEME1_FROZEN_SERVER_CONTRACT.fixture_sha256,
  fixture_path: SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path,
});

export const SCHEME1_CLIENT_EVIDENCE_FORMAT =
  SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT;
export const SCHEME1_CLIENT_EVIDENCE_ENVELOPE_FORMAT =
  "osl.keyserver.scheme1-client-evidence-envelope.v1";
export const SCHEME1_CLIENT_EVIDENCE_DOMAIN =
  "OSL-KEYSERVER-SCHEME1-CLIENT-EVIDENCE-v1\u0000";
export const SCHEME1_CLIENT_PREFLIGHT_FORMAT =
  "osl.keyserver.scheme1-client-deployment-preflight.v1";
export const SCHEME1_PREFLIGHT_ACTIONS = Object.freeze([
  "migrate-0033-0034",
  "activate-scheme1-worker",
]);
export const SCHEME1_CLIENT_EVIDENCE_MAX_AGE_MS = 15 * 60 * 1000;
export const SCHEME1_CLIENT_RUN_MAX_DURATION_MS = 2 * 60 * 60 * 1000;
export const SCHEME1_CLIENT_MINIMUM_TEST_COUNT =
  CROSS_LANGUAGE_CASE_IDS.length +
  RESTART_CASE_IDS.length +
  DOWNGRADE_CASE_IDS.length;

export const SCHEME1_FROZEN_RUST_CLIENT_PRODUCER_KEY_ID =
  "osl-rust-client-scheme1-frozen-20260727";

function freezeTrustedProducers(producers) {
  for (const producer of Object.values(producers)) {
    Object.freeze(producer);
  }
  return Object.freeze(producers);
}

export const TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS =
  freezeTrustedProducers({
    [SCHEME1_FROZEN_RUST_CLIENT_PRODUCER_KEY_ID]: {
      identity: "osl://scheme1-rust-client/frozen-shipping/2026-07-27",
      minimum_sequence: 1,
      public_key_spki_b64:
        "MCowBQYDK2VwAyEArqLJqLipE68NG6DRgdUTUlPduDx4S/b0rVkHER2tH6s=",
      key_epoch: 1,
    },
  });

export const SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS = Object.freeze([
  "keyserver-cf/migrations/0033_canonical_identity_rollout_authority.sql",
  "keyserver-cf/migrations/0034_scheme1_prekey_owner_proofs.sql",
  "keyserver-cf/src/index.ts",
  "keyserver-cf/src/endpoints/canonical-identity.ts",
  "keyserver-cf/src/endpoints/prekey-bundle.ts",
  "keyserver-cf/src/endpoints/pubkeys.ts",
  "keyserver-cf/src/endpoints/register.ts",
  "keyserver-cf/src/endpoints/sender-filter-rollout-root.ts",
  "keyserver-cf/src/lib/db.ts",
  "keyserver-cf/src/lib/identity-authority.ts",
  "keyserver-cf/src/lib/prekey-owner-proof.ts",
  SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path,
]);

export const SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS = Object.freeze([
  "keyserver-cf/DEPLOY.md",
  "keyserver-cf/package.json",
  ...SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS,
  "keyserver-cf/scripts/refuse-unadmitted-production-action.mjs",
  "keyserver-cf/scripts/scheme1-client-evidence-dto.mjs",
  "keyserver-cf/scripts/scheme1-client-evidence-test-fixture.mjs",
  "keyserver-cf/scripts/scheme1-client-admission-contract.mjs",
  "keyserver-cf/scripts/scheme1-client-admission.test.ts",
  "keyserver-cf/scripts/scheme1-client-admission-mutant-catalog.mjs",
  "keyserver-cf/scripts/scheme1-client-admission-mutants.mjs",
  "keyserver-cf/scripts/create-scheme1-client-preflight.mjs",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function object(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function exact(value, fields, label) {
  const item = object(value, label);
  const actual = Object.keys(item).sort();
  const expected = [...fields].sort();
  if (
    actual.length !== expected.length ||
    actual.some((field, index) => field !== expected[index])
  ) {
    throw new Error(`${label} fields are not exact`);
  }
  return item;
}

function nonempty(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be nonempty`);
  }
  return value;
}

function gitObject(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{40}$/u.test(value) ||
    value === "0".repeat(40)
  ) {
    throw new Error(`${label} must be a nonzero full Git object`);
  }
  return value;
}

function digest(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/u.test(value) ||
    value === "0".repeat(64)
  ) {
    throw new Error(`${label} must be a nonzero SHA-256`);
  }
  return value;
}

function positiveInteger(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive safe integer`);
  }
  return value;
}

function timestamp(value, label) {
  const parsed = typeof value === "string" ? Date.parse(value) : NaN;
  if (
    !Number.isFinite(parsed) ||
    new Date(parsed).toISOString() !== value
  ) {
    throw new Error(`${label} must be a canonical ISO timestamp`);
  }
  return parsed;
}

function uuid(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u
      .test(value)
  ) {
    throw new Error(`${label} must be a lowercase UUID`);
  }
  return value;
}

function canonicalBase64(value, label) {
  if (typeof value !== "string") {
    throw new Error(`${label} must be canonical base64`);
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.length === 0 || decoded.toString("base64") !== value) {
    throw new Error(`${label} must be canonical base64`);
  }
  return decoded;
}

function sourceBytes(value, label) {
  if (Buffer.isBuffer(value)) return value;
  if (value instanceof Uint8Array) return Buffer.from(value);
  if (typeof value === "string") return Buffer.from(value);
  throw new Error(`${label} is not bytes`);
}

function exactAnchor(value, label) {
  const anchor = exact(
    value,
    ["commit", "keyserver_tree", "repository_tree"],
    label,
  );
  return {
    commit: gitObject(anchor.commit, `${label} commit`),
    repository_tree: gitObject(
      anchor.repository_tree,
      `${label} repository tree`,
    ),
    keyserver_tree: gitObject(
      anchor.keyserver_tree,
      `${label} keyserver tree`,
    ),
  };
}

export function validateFrozenScheme1Fixture(value) {
  const bytes = sourceBytes(value, "frozen scheme-1 fixture");
  if (sha256(bytes) !== SCHEME1_FROZEN_SERVER_CONTRACT.fixture_sha256) {
    throw new Error("frozen scheme-1 fixture digest mismatch");
  }
  let fixture;
  try {
    fixture = JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("frozen scheme-1 fixture is not JSON");
  }
  const descriptor = exact(
    object(fixture, "frozen scheme-1 fixture").contract_descriptor,
    ["sha256_hex", "utf8"],
    "frozen scheme-1 descriptor",
  );
  if (
    descriptor.sha256_hex !==
      SCHEME1_FROZEN_SERVER_CONTRACT.descriptor_sha256 ||
    sha256(Buffer.from(nonempty(descriptor.utf8, "scheme-1 descriptor"))) !==
      SCHEME1_FROZEN_SERVER_CONTRACT.descriptor_sha256
  ) {
    throw new Error("frozen scheme-1 descriptor mismatch");
  }
  const request = object(
    object(fixture.identity_registration, "identity fixture").request,
    "identity registration request",
  );
  const response = object(
    object(fixture.replenish, "replenish fixture").expected_commit_response,
    "replenish expected response",
  );
  if (
    request.identity_scheme !== 1 ||
    request.identity_bundle_version !== 1 ||
    response.identity_scheme !== 1 ||
    response.identity_bundle_version !== 1 ||
    response.protocol_version !== 2 ||
    response.lifecycle_version !== 2 ||
    response.result !== "scheme1_replenish_committed"
  ) {
    throw new Error("frozen scheme-1 fixture tags are not exact");
  }
  const rustStorageVersion = object(
    object(fixture.version_separation, "fixture version separation")
      .rust_private_identity_blob_version,
    "Rust private identity blob version",
  );
  if (rustStorageVersion.normative_for_this_fixture !== false) {
    throw new Error("Rust private identity blob version became normative");
  }
  return Object.freeze({
    bytes: bytes.length,
    sha256: SCHEME1_FROZEN_SERVER_CONTRACT.fixture_sha256,
    descriptor_sha256: descriptor.sha256_hex,
  });
}

export function validateScheme1ClientPreflightSourceClosure(fileValues) {
  const files = exact(
    fileValues,
    SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS,
    "scheme-1 client preflight source closure",
  );
  const result = SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS.map((sourcePath) => {
    const bytes = sourceBytes(files[sourcePath], sourcePath);
    if (bytes.length === 0) {
      throw new Error(`scheme-1 client preflight source is empty: ${sourcePath}`);
    }
    return Object.freeze({
      path: sourcePath,
      bytes: bytes.length,
      sha256: sha256(bytes),
    });
  });
  const fixture = files[SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path];
  validateFrozenScheme1Fixture(fixture);
  return Object.freeze(result);
}

export function validateFrozenScheme1SourceContract(
  frozenFileValues,
  deploymentFileValues,
) {
  const frozen = exact(
    frozenFileValues,
    SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS,
    "frozen scheme-1 source contract",
  );
  object(deploymentFileValues, "scheme-1 deployment source files");
  const result = SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS.map((sourcePath) => {
    const frozenBytes = sourceBytes(frozen[sourcePath], sourcePath);
    const deploymentBytes = sourceBytes(
      deploymentFileValues[sourcePath],
      `deployment ${sourcePath}`,
    );
    const frozenSha256 = sha256(frozenBytes);
    // SCHEME1_FROZEN_SOURCE_CONTRACT_MUTATION_BEGIN
    if (
      frozenBytes.length === 0 ||
      deploymentBytes.length === 0 ||
      sha256(deploymentBytes) !== frozenSha256
    ) {
      throw new Error(
        `scheme-1 frozen source contract drift: ${sourcePath}`,
      );
    }
    // SCHEME1_FROZEN_SOURCE_CONTRACT_MUTATION_END
    return Object.freeze({
      path: sourcePath,
      bytes: frozenBytes.length,
      sha256: frozenSha256,
    });
  });
  validateFrozenScheme1Fixture(
    frozen[SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path],
  );
  return Object.freeze(result);
}

function signedEnvelopeFields(envelope) {
  return {
    format: envelope.format,
    producer_key_id: envelope.producer_key_id,
    producer_key_epoch: envelope.producer_key_epoch,
    producer_sequence: envelope.producer_sequence,
    issued_at: envelope.issued_at,
    expires_at: envelope.expires_at,
    challenge_nonce: envelope.challenge_nonce,
    payload: envelope.payload,
    payload_sha256: envelope.payload_sha256,
  };
}

function receiptWithoutDigest(receipt) {
  const { receipt_sha256: _digest, ...payload } = receipt;
  return payload;
}

function validateEmbeddedReceiptDigests(payload) {
  const receipts = [
    ["cross-language", payload.cross_language_receipt],
    ["restart", payload.restart_receipt],
    ["downgrade", payload.downgrade_receipt],
  ];
  const receiptDigests = new Set();
  for (const [label, receipt] of receipts) {
    const expected = sha256(
      Buffer.from(canonicalJson(receiptWithoutDigest(receipt))),
    );
    if (receipt.receipt_sha256 !== expected) {
      throw new Error(`scheme-1 ${label} receipt digest mismatch`);
    }
    if (receiptDigests.has(expected)) {
      throw new Error("scheme-1 embedded receipt digests must be distinct");
    }
    receiptDigests.add(expected);
  }

  const witnesses = [
    payload.run.runner_binary_sha256,
    ...payload.cross_language_receipt.cases.map(
      (testCase) => testCase.witness_sha256,
    ),
    payload.restart_receipt.pre_process_sha256,
    payload.restart_receipt.post_process_sha256,
    payload.restart_receipt.state_sha256,
    ...payload.restart_receipt.cases.map(
      (testCase) => testCase.witness_sha256,
    ),
    ...payload.downgrade_receipt.cases.map(
      (testCase) => testCase.witness,
    ),
  ];
  if (new Set(witnesses).size !== witnesses.length) {
    throw new Error(
      "scheme-1 client evidence reuses a witness across receipt classes",
    );
  }
}

export function validateScheme1ClientEvidenceEnvelope(
  value,
  {
    expectedClientCommit,
    expectedClientTree,
    expectedChallenge,
    nowMs = Date.now(),
    trustedProducers = TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS,
  },
) {
  if (!Number.isFinite(nowMs)) {
    throw new Error("scheme-1 client evidence current time is invalid");
  }
  const envelope = exact(
    value,
    [
      "challenge_nonce",
      "expires_at",
      "format",
      "issued_at",
      "payload",
      "payload_sha256",
      "producer_key_epoch",
      "producer_key_id",
      "producer_sequence",
      "signature_b64",
    ],
    "scheme-1 client evidence envelope",
  );
  if (envelope.format !== SCHEME1_CLIENT_EVIDENCE_ENVELOPE_FORMAT) {
    throw new Error("scheme-1 client evidence envelope format mismatch");
  }
  if (uuid(envelope.challenge_nonce, "scheme-1 client challenge") !==
      expectedChallenge) {
    throw new Error("scheme-1 client evidence challenge mismatch");
  }
  const issuedAt = timestamp(envelope.issued_at, "client evidence issue time");
  const expiresAt = timestamp(
    envelope.expires_at,
    "client evidence expiration",
  );
  if (
    expiresAt <= issuedAt ||
    expiresAt - issuedAt > SCHEME1_CLIENT_EVIDENCE_MAX_AGE_MS ||
    nowMs < issuedAt ||
    nowMs >= expiresAt
  ) {
    throw new Error("scheme-1 client evidence is stale or not yet valid");
  }
  const producerKeyId = nonempty(
    envelope.producer_key_id,
    "scheme-1 client producer key id",
  );
  const producer = exact(
    trustedProducers[producerKeyId],
    [
      "identity",
      "minimum_sequence",
      "public_key_spki_b64",
      "key_epoch",
    ],
    "trusted scheme-1 client evidence producer",
  );
  const keyEpoch = positiveInteger(
    envelope.producer_key_epoch,
    "scheme-1 client producer key epoch",
  );
  const sequence = positiveInteger(
    envelope.producer_sequence,
    "scheme-1 client producer sequence",
  );
  const trustedKeyEpoch = positiveInteger(
    producer.key_epoch,
    "trusted scheme-1 client producer key epoch",
  );
  const minimumSequence = positiveInteger(
    producer.minimum_sequence,
    "trusted scheme-1 client producer minimum sequence",
  );
  if (keyEpoch !== trustedKeyEpoch || sequence < minimumSequence) {
    throw new Error("scheme-1 client producer epoch or sequence is stale");
  }
  nonempty(producer.identity, "scheme-1 client producer identity");
  digest(envelope.payload_sha256, "scheme-1 client evidence payload");
  const payload = parseScheme1ClientEvidencePayload(envelope.payload);
  if (canonicalJson(envelope.payload) !== canonicalJson(payload)) {
    throw new Error("scheme-1 client evidence payload is not canonical");
  }
  if (
    payload.format !== SCHEME1_CLIENT_EVIDENCE_FORMAT ||
    payload.client.commit !== expectedClientCommit ||
    payload.client.repository_tree !== expectedClientTree
  ) {
    throw new Error("scheme-1 client evidence object mismatch");
  }
  for (const [field, expected] of Object.entries(
    SCHEME1_RUST_CLIENT_CONTRACT_BINDING,
  )) {
    if (payload.contract[field] !== expected) {
      throw new Error(`scheme-1 client contract ${field} mismatch`);
    }
  }
  if (payload.run.test_count < SCHEME1_CLIENT_MINIMUM_TEST_COUNT) {
    throw new Error("scheme-1 client evidence test count is incomplete");
  }
  // SCHEME1_CLIENT_RECEIPT_DIGEST_MUTATION_BEGIN
  validateEmbeddedReceiptDigests(payload);
  // SCHEME1_CLIENT_RECEIPT_DIGEST_MUTATION_END
  const runStarted = timestamp(payload.run.started_at, "client run start");
  const runFinished = timestamp(payload.run.finished_at, "client run finish");
  if (
    runFinished - runStarted > SCHEME1_CLIENT_RUN_MAX_DURATION_MS ||
    runFinished > issuedAt
  ) {
    throw new Error("scheme-1 client run timing is invalid");
  }
  const payloadBytes = Buffer.from(canonicalJson(payload));
  if (sha256(payloadBytes) !== envelope.payload_sha256) {
    throw new Error("scheme-1 client evidence payload digest mismatch");
  }
  const publicKey = createPublicKey({
    key: canonicalBase64(
      producer.public_key_spki_b64,
      "scheme-1 client producer public key",
    ),
    format: "der",
    type: "spki",
  });
  if (
    publicKey.asymmetricKeyType !== "ed25519" ||
    publicKey
      .export({ format: "der", type: "spki" })
      .toString("base64") !== producer.public_key_spki_b64
  ) {
    throw new Error(
      "scheme-1 client producer key is not canonical Ed25519 SPKI",
    );
  }
  const signature = canonicalBase64(
    envelope.signature_b64,
    "scheme-1 client evidence signature",
  );
  const signedBytes = Buffer.from(canonicalJson(signedEnvelopeFields(envelope)));
  // SCHEME1_CLIENT_SIGNATURE_MUTATION_BEGIN
  if (
    signature.length !== 64 ||
    !verifySignature(
      null,
      Buffer.concat([
        Buffer.from(SCHEME1_CLIENT_EVIDENCE_DOMAIN),
        signedBytes,
      ]),
      publicKey,
      signature,
    )
  ) {
    throw new Error("scheme-1 client evidence signature is invalid");
  }
  // SCHEME1_CLIENT_SIGNATURE_MUTATION_END
  return Object.freeze({
    envelope,
    payload,
    producer_identity: producer.identity,
  });
}

function receiptPayload(receipt) {
  const { payload_sha256: _digest, ...payload } = receipt;
  return payload;
}

export function createScheme1ClientDeploymentPreflight({
  action,
  deploymentAnchor,
  clientEvidence,
  expectedClientCommit,
  expectedClientTree,
  expectedChallenge,
  fileValues,
  frozenFixtureBytes,
  frozenContractFileValues,
  nowMs = Date.now(),
  trustedProducers = TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS,
}) {
  if (!SCHEME1_PREFLIGHT_ACTIONS.includes(action)) {
    throw new Error("scheme-1 preflight action is unsupported");
  }
  const deploymentSource = exactAnchor(
    deploymentAnchor,
    "scheme-1 deployment source",
  );
  gitObject(expectedClientCommit, "scheme-1 client commit");
  gitObject(expectedClientTree, "scheme-1 client repository tree");
  validateFrozenScheme1Fixture(frozenFixtureBytes);
  const sourceFiles = validateScheme1ClientPreflightSourceClosure(fileValues);
  const frozenContractFiles = validateFrozenScheme1SourceContract(
    frozenContractFileValues,
    fileValues,
  );
  const verified = validateScheme1ClientEvidenceEnvelope(clientEvidence, {
    expectedClientCommit,
    expectedClientTree,
    expectedChallenge,
    nowMs,
    trustedProducers,
  });
  const expiresAt = Math.min(
    Date.parse(verified.envelope.expires_at),
    nowMs + SCHEME1_CLIENT_EVIDENCE_MAX_AGE_MS,
  );
  const payload = {
    format: SCHEME1_CLIENT_PREFLIGHT_FORMAT,
    action,
    client_contract_admitted: true,
    execution_authorized: false,
    migration_execution_authorized: false,
    worker_activation_authorized: false,
    issued_at: new Date(nowMs).toISOString(),
    expires_at: new Date(expiresAt).toISOString(),
    challenge_nonce: expectedChallenge,
    deployment_source: deploymentSource,
    frozen_server_contract: { ...SCHEME1_FROZEN_SERVER_CONTRACT },
    client: { ...verified.payload.client },
    producer_key_id: verified.envelope.producer_key_id,
    producer_key_epoch: verified.envelope.producer_key_epoch,
    producer_sequence: verified.envelope.producer_sequence,
    producer_identity: verified.producer_identity,
    client_evidence: verified.envelope,
    frozen_contract_files: frozenContractFiles,
    source_files: sourceFiles,
  };
  return Object.freeze({
    ...payload,
    payload_sha256: sha256(Buffer.from(canonicalJson(payload))),
  });
}

export function validateScheme1ClientPreflightReceipt(
  value,
  {
    action,
    expectedDeploymentCommit,
    expectedDeploymentTree,
    expectedKeyserverTree,
    expectedClientCommit,
    expectedClientTree,
    expectedChallenge,
    fileValues,
    frozenContractFileValues,
    nowMs = Date.now(),
    trustedProducers = TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS,
  },
) {
  const receipt = exact(
    value,
    [
      "action",
      "challenge_nonce",
      "client",
      "client_contract_admitted",
      "client_evidence",
      "deployment_source",
      "execution_authorized",
      "expires_at",
      "format",
      "frozen_contract_files",
      "frozen_server_contract",
      "issued_at",
      "migration_execution_authorized",
      "payload_sha256",
      "producer_identity",
      "producer_key_epoch",
      "producer_key_id",
      "producer_sequence",
      "source_files",
      "worker_activation_authorized",
    ],
    "scheme-1 client preflight receipt",
  );
  if (
    receipt.format !== SCHEME1_CLIENT_PREFLIGHT_FORMAT ||
    receipt.action !== action ||
    receipt.client_contract_admitted !== true ||
    receipt.execution_authorized !== false ||
    receipt.migration_execution_authorized !== false ||
    receipt.worker_activation_authorized !== false
  ) {
    throw new Error("scheme-1 client preflight disposition mismatch");
  }
  const source = exactAnchor(
    receipt.deployment_source,
    "scheme-1 receipt deployment source",
  );
  if (
    source.commit !== expectedDeploymentCommit ||
    source.repository_tree !== expectedDeploymentTree ||
    source.keyserver_tree !== expectedKeyserverTree
  ) {
    throw new Error("scheme-1 client preflight deployment source mismatch");
  }
  const receiptClient = exact(
    receipt.client,
    ["commit", "repository_tree"],
    "scheme-1 receipt client",
  );
  if (
    receipt.challenge_nonce !== expectedChallenge ||
    gitObject(receiptClient.commit, "scheme-1 receipt client commit") !==
      expectedClientCommit ||
    gitObject(
      receiptClient.repository_tree,
      "scheme-1 receipt client repository tree",
    ) !== expectedClientTree ||
    canonicalJson(receipt.frozen_server_contract) !==
      canonicalJson(SCHEME1_FROZEN_SERVER_CONTRACT)
  ) {
    throw new Error("scheme-1 client preflight contract binding mismatch");
  }
  const issuedAt = timestamp(receipt.issued_at, "preflight receipt issue");
  const expiresAt = timestamp(receipt.expires_at, "preflight receipt expiry");
  if (
    expiresAt <= issuedAt ||
    expiresAt - issuedAt > SCHEME1_CLIENT_EVIDENCE_MAX_AGE_MS ||
    nowMs < issuedAt ||
    nowMs >= expiresAt
  ) {
    throw new Error("scheme-1 client preflight receipt is stale");
  }
  const verified = validateScheme1ClientEvidenceEnvelope(
    receipt.client_evidence,
    {
      expectedClientCommit,
      expectedClientTree,
      expectedChallenge,
      nowMs,
      trustedProducers,
    },
  );
  if (
    receipt.producer_key_id !== receipt.client_evidence.producer_key_id ||
    receipt.producer_key_epoch !== receipt.client_evidence.producer_key_epoch ||
    receipt.producer_sequence !== receipt.client_evidence.producer_sequence ||
    receipt.producer_identity !== verified.producer_identity
  ) {
    throw new Error("scheme-1 client evidence is not bound to receipt");
  }
  const exactSourceFiles = validateScheme1ClientPreflightSourceClosure(
    fileValues,
  );
  // SCHEME1_CLIENT_SOURCE_BINDING_MUTATION_BEGIN
  if (
    canonicalJson(receipt.source_files) !== canonicalJson(exactSourceFiles)
  ) {
    throw new Error(
      "scheme-1 client preflight source closure does not match exact bytes",
    );
  }
  const exactFrozenContractFiles = validateFrozenScheme1SourceContract(
    frozenContractFileValues,
    fileValues,
  );
  if (
    canonicalJson(receipt.frozen_contract_files) !==
      canonicalJson(exactFrozenContractFiles)
  ) {
    throw new Error(
      "scheme-1 client preflight frozen source contract mismatch",
    );
  }
  // SCHEME1_CLIENT_SOURCE_BINDING_MUTATION_END
  digest(receipt.payload_sha256, "scheme-1 client preflight receipt digest");
  if (
    sha256(Buffer.from(canonicalJson(receiptPayload(receipt)))) !==
      receipt.payload_sha256
  ) {
    throw new Error("scheme-1 client preflight receipt digest mismatch");
  }
  return receipt;
}
