import { execFileSync } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import {
  lstat,
  mkdir,
  open,
  readFile,
  rename,
  rmdir,
} from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import {
  DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT,
  DEPLOYMENT_TRANSITIONS,
} from "./deployment-evidence-receipt-contract.mjs";
import { canonicalJson, sha256 } from "./readiness-artifact-contract.mjs";

export const DEPLOYMENT_EVIDENCE_VERIFIER_STATE_FORMAT =
  "osl.keyserver.deployment-evidence-verifier-state.v2";
export const DEPLOYMENT_EVIDENCE_CHALLENGE_LIFETIME_MS = 10 * 60_000;

const MAX_RECEIPT_BYTES = 1024 * 1024;
const DEFAULT_STATE_DIRECTORY = path.join(
  homedir(),
  ".local",
  "state",
  "osl-keyserver-verifier",
  "deployment-evidence-v2",
);
const STATE_FIELDS = Object.freeze([
  "current_artifact",
  "current_deployment_id",
  "current_worker_version",
  "format",
  "last_producer_run_id",
  "pending_challenge",
  "producer_identity",
  "producer_key_id",
  "receipt_sha256",
  "seen_deployment_ids",
  "seen_worker_versions",
  "sequence",
  "state_epoch",
]);
const CHALLENGE_REQUEST_FIELDS = Object.freeze([
  "archiveId",
  "artifact",
  "artifactBundleSha256",
  "expectedCommit",
  "producerIdentity",
  "producerKeyId",
]);

function git(repoRoot, args, encoding = "utf8") {
  return execFileSync("git", ["-C", repoRoot, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function exactKeys(value, fields, label) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    canonicalJson(Object.keys(value).sort()) !==
      canonicalJson([...fields].sort())
  ) {
    throw new Error(`${label} fields are not exact`);
  }
  return value;
}

function requireSha(value, label) {
  if (
    typeof value !== "string" ||
    !/^[0-9a-f]{64}$/.test(value) ||
    value === "0".repeat(64)
  ) {
    throw new Error(`${label} must be a nonzero SHA-256`);
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

function requireIdentity(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be nonempty`);
  }
}

function requireUniqueUuidHistory(value, current, label) {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    new Set(value).size !== value.length ||
    !value.includes(current)
  ) {
    throw new Error(`${label} is empty, duplicated, or missing current identity`);
  }
  value.forEach((entry) => requireUuid(entry, label));
}

function validateVerifierState(value) {
  const state = exactKeys(
    value,
    STATE_FIELDS,
    "deployment evidence verifier state",
  );
  if (state.format !== DEPLOYMENT_EVIDENCE_VERIFIER_STATE_FORMAT) {
    throw new Error("deployment evidence verifier state format mismatch");
  }
  requireIdentity(state.producer_key_id, "verifier producer key id");
  requireIdentity(state.producer_identity, "verifier producer identity");
  if (
    !Number.isSafeInteger(state.sequence) ||
    state.sequence <= 0 ||
    !Number.isSafeInteger(state.state_epoch) ||
    state.state_epoch <= 0
  ) {
    throw new Error("deployment evidence verifier lineage is not initialized");
  }
  requireSha(state.receipt_sha256, "verifier receipt anchor");
  requireUuid(state.last_producer_run_id, "verifier last producer run id");
  if (!["legacy", "A", "B"].includes(state.current_artifact)) {
    throw new Error("verifier current artifact is invalid");
  }
  requireUuid(state.current_worker_version, "verifier current Worker version");
  requireUuid(state.current_deployment_id, "verifier current deployment id");
  requireUniqueUuidHistory(
    state.seen_worker_versions,
    state.current_worker_version,
    "verifier Worker version history",
  );
  requireUniqueUuidHistory(
    state.seen_deployment_ids,
    state.current_deployment_id,
    "verifier deployment id history",
  );
  if (
    state.pending_challenge !== null &&
    (!state.pending_challenge ||
      typeof state.pending_challenge !== "object" ||
      Array.isArray(state.pending_challenge))
  ) {
    throw new Error("verifier pending challenge is invalid");
  }
  return state;
}

function stateDirectory(options) {
  // This override is intentionally test-named and is never exposed by the CLI.
  // Receipts cannot choose it. Production uses only the verifier-owned path.
  return options?.testStateDirectory ?? DEFAULT_STATE_DIRECTORY;
}

export function deploymentEvidenceVerifierStatePath(
  producerKeyId,
  options = {},
) {
  requireIdentity(producerKeyId, "producer key id");
  return path.join(
    stateDirectory(options),
    `${sha256(Buffer.from(producerKeyId))}.json`,
  );
}

async function secureStateDirectory(directory) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const info = await lstat(directory);
  if (
    !info.isDirectory() ||
    info.isSymbolicLink() ||
    (info.mode & 0o077) !== 0
  ) {
    throw new Error("deployment evidence verifier state is not private");
  }
}

async function readExistingState(producerKeyId, options = {}) {
  const directory = stateDirectory(options);
  await secureStateDirectory(directory);
  const statePath = deploymentEvidenceVerifierStatePath(
    producerKeyId,
    options,
  );
  let info;
  try {
    info = await lstat(statePath);
  } catch (error) {
    if (error && typeof error === "object" && error.code === "ENOENT") {
      throw new Error(
        "durable deployment evidence verifier state is missing; genesis is forbidden",
      );
    }
    throw error;
  }
  if (
    !info.isFile() ||
    info.isSymbolicLink() ||
    (info.mode & 0o077) !== 0
  ) {
    throw new Error(
      "deployment evidence verifier state is not a private regular file",
    );
  }
  let parsed;
  try {
    parsed = JSON.parse(await readFile(statePath, "utf8"));
  } catch {
    throw new Error("deployment evidence verifier state is not valid JSON");
  }
  return {
    directory,
    statePath,
    state: validateVerifierState(parsed),
  };
}

async function withProducerLock(producerKeyId, options, action) {
  const directory = stateDirectory(options);
  await secureStateDirectory(directory);
  const lockPath = path.join(
    directory,
    `${sha256(Buffer.from(producerKeyId))}.lock`,
  );
  try {
    await mkdir(lockPath, { mode: 0o700 });
  } catch (error) {
    if (error && typeof error === "object" && error.code === "EEXIST") {
      throw new Error("deployment evidence verifier state is busy");
    }
    throw error;
  }
  try {
    return await action();
  } finally {
    await rmdir(lockPath);
  }
}

async function writeState(statePath, directory, next, runId) {
  const temporaryPath = path.join(
    directory,
    `${path.basename(statePath, ".json")}.${runId}.tmp`,
  );
  const handle = await open(temporaryPath, "wx", 0o600);
  try {
    await handle.writeFile(`${canonicalJson(next)}\n`);
    await handle.sync();
  } finally {
    await handle.close();
  }
  await rename(temporaryPath, statePath);
  const directoryHandle = await open(directory, "r");
  try {
    await directoryHandle.sync();
  } finally {
    await directoryHandle.close();
  }
}

export function loadCommittedMigrationClosure(
  repoRoot,
  expectedCommit,
  gitRun = git,
) {
  const output = gitRun(repoRoot, [
    "ls-tree",
    "-r",
    "--name-only",
    expectedCommit,
    "--",
    "keyserver-cf/migrations",
  ]);
  const paths = output.trim().split("\n").filter(Boolean).sort();
  if (paths.length === 0) {
    throw new Error("committed migration closure is empty");
  }
  const expectedPrefix = "keyserver-cf/migrations/";
  return paths.map((sourcePath) => {
    if (
      !sourcePath.startsWith(expectedPrefix) ||
      !/^\d{4}_[a-z0-9_]+\.sql$/.test(sourcePath.slice(expectedPrefix.length))
    ) {
      throw new Error(`committed migration path is invalid: ${sourcePath}`);
    }
    const bytes = gitRun(
      repoRoot,
      ["show", `${expectedCommit}:${sourcePath}`],
      null,
    );
    if (!Buffer.isBuffer(bytes) || bytes.byteLength === 0) {
      throw new Error(`committed migration is empty: ${sourcePath}`);
    }
    return {
      name: sourcePath.slice(expectedPrefix.length),
      sha256: sha256(bytes),
    };
  });
}

export async function readDeploymentEvidenceReceipt(receiptPath) {
  if (!path.isAbsolute(receiptPath)) {
    throw new Error("deployment evidence receipt path must be absolute");
  }
  const info = await lstat(receiptPath);
  if (!info.isFile() || info.isSymbolicLink()) {
    throw new Error("deployment evidence receipt is not a regular file");
  }
  if (info.size <= 0 || info.size > MAX_RECEIPT_BYTES) {
    throw new Error("deployment evidence receipt is empty or oversized");
  }
  const bytes = await readFile(receiptPath);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("deployment evidence receipt is not JSON");
  }
}

export async function issueDeploymentEvidenceChallenge(
  requestValue,
  {
    testStateDirectory,
    nowMs = Date.now(),
    randomBytesFn = randomBytes,
    randomUuidFn = randomUUID,
  } = {},
) {
  const request = exactKeys(
    requestValue,
    CHALLENGE_REQUEST_FIELDS,
    "deployment evidence challenge request",
  );
  requireIdentity(request.producerKeyId, "challenge producer key id");
  requireIdentity(request.producerIdentity, "challenge producer identity");
  if (
    typeof request.expectedCommit !== "string" ||
    !/^[0-9a-f]{40}$/.test(request.expectedCommit) ||
    request.expectedCommit === "0".repeat(40)
  ) {
    throw new Error("challenge expected commit is invalid");
  }
  requireSha(request.archiveId, "challenge archive");
  requireSha(request.artifactBundleSha256, "challenge artifact bundle");
  if (!["A", "B"].includes(request.artifact)) {
    throw new Error("challenge artifact is invalid");
  }
  const options = { testStateDirectory };
  return withProducerLock(request.producerKeyId, options, async () => {
    const { directory, statePath, state } = await readExistingState(
      request.producerKeyId,
      options,
    );
    if (
      state.producer_identity !== request.producerIdentity ||
      state.producer_key_id !== request.producerKeyId
    ) {
      throw new Error("challenge producer does not match durable verifier state");
    }
    if (state.pending_challenge !== null) {
      throw new Error("durable verifier state already has a pending challenge");
    }
    const permittedTransition =
      DEPLOYMENT_TRANSITIONS[
        `${state.current_artifact}:${request.artifact}`
      ];
    if (!permittedTransition) {
      throw new Error("requested deployment transition is not permitted");
    }
    const challengeId = randomUuidFn();
    requireUuid(challengeId, "generated challenge id");
    const nonce = randomBytesFn(32);
    if (!Buffer.isBuffer(nonce) || nonce.byteLength !== 32) {
      throw new Error("generated challenge nonce is invalid");
    }
    const challenge = {
      format: DEPLOYMENT_EVIDENCE_CHALLENGE_FORMAT,
      challenge_id: challengeId,
      nonce_b64: nonce.toString("base64"),
      issued_at: new Date(nowMs).toISOString(),
      expires_at: new Date(
        nowMs + DEPLOYMENT_EVIDENCE_CHALLENGE_LIFETIME_MS,
      ).toISOString(),
      producer_key_id: state.producer_key_id,
      producer_identity: state.producer_identity,
      previous_sequence: state.sequence,
      previous_receipt_sha256: state.receipt_sha256,
      previous_worker_version: state.current_worker_version,
      previous_deployment_id: state.current_deployment_id,
      previous_artifact: state.current_artifact,
      permitted_transition: permittedTransition,
      expected_commit: request.expectedCommit,
      archive_id: request.archiveId,
      artifact: request.artifact,
      artifact_bundle_sha256: request.artifactBundleSha256,
    };
    const next = {
      ...state,
      state_epoch: state.state_epoch + 1,
      pending_challenge: challenge,
    };
    await writeState(statePath, directory, next, challengeId);
    return challenge;
  });
}

export async function loadDeploymentEvidenceVerifierChallenge(
  producerKeyId,
  { testStateDirectory, nowMs = Date.now() } = {},
) {
  const { state } = await readExistingState(producerKeyId, {
    testStateDirectory,
  });
  const challenge = state.pending_challenge;
  if (challenge === null) {
    throw new Error("durable verifier state has no pending challenge");
  }
  const expiresAt = Date.parse(challenge.expires_at);
  if (!Number.isFinite(expiresAt) || expiresAt < nowMs) {
    throw new Error("durable verifier challenge is stale");
  }
  return structuredClone(challenge);
}

export async function consumeDeploymentEvidenceOnce(
  verified,
  { testStateDirectory } = {},
) {
  const options = { testStateDirectory };
  return withProducerLock(verified.producer_key_id, options, async () => {
    const { directory, statePath, state } = await readExistingState(
      verified.producer_key_id,
      options,
    );
    const payload = verified.payload;
    if (
      state.producer_key_id !== verified.producer_key_id ||
      state.producer_identity !== verified.producer_identity
    ) {
      throw new Error("receipt producer does not match durable verifier state");
    }
    if (
      state.pending_challenge === null ||
      canonicalJson(state.pending_challenge) !== canonicalJson(payload.challenge)
    ) {
      throw new Error("receipt challenge is absent, forged, stale, or replayed");
    }
    if (
      payload.producer_sequence !== state.sequence + 1 ||
      payload.previous_receipt_sha256 !== state.receipt_sha256
    ) {
      throw new Error("deployment evidence receipt is replayed or out of chain");
    }
    const transition = payload.transition;
    if (
      transition.previous_artifact !== state.current_artifact ||
      transition.previous_worker_version !== state.current_worker_version ||
      transition.previous_deployment_id !== state.current_deployment_id
    ) {
      throw new Error("receipt predecessor does not match durable verifier state");
    }
    if (
      state.seen_worker_versions.includes(transition.current_worker_version) ||
      state.seen_deployment_ids.includes(transition.current_deployment_id)
    ) {
      throw new Error("deployment evidence Worker rollback or replay refused");
    }
    const expectedTransition =
      DEPLOYMENT_TRANSITIONS[
        `${state.current_artifact}:${transition.current_artifact}`
      ];
    if (
      !expectedTransition ||
      transition.permitted_transition !== expectedTransition
    ) {
      throw new Error("deployment evidence transition is not permitted");
    }
    const next = {
      ...state,
      state_epoch: state.state_epoch + 1,
      sequence: payload.producer_sequence,
      receipt_sha256: verified.receipt_sha256,
      last_producer_run_id: payload.producer_run_id,
      current_artifact: transition.current_artifact,
      current_worker_version: transition.current_worker_version,
      current_deployment_id: transition.current_deployment_id,
      seen_worker_versions: [
        ...state.seen_worker_versions,
        transition.current_worker_version,
      ],
      seen_deployment_ids: [
        ...state.seen_deployment_ids,
        transition.current_deployment_id,
      ],
      pending_challenge: null,
    };
    await writeState(
      statePath,
      directory,
      next,
      payload.producer_run_id,
    );
    return next;
  });
}
