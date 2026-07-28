/**
 * D2 production admission v3.
 *
 * This module deliberately has no production authority configured. A future
 * operator must wire a reviewed, code-owned durable authority before the
 * production entry point can authorize anything. Tests may inject an authority
 * through the explicitly test-only function.
 */

import {
  D2_DATABASE_ID,
  D2_PROBE_FORMAT,
  D2_PROBE_KINDS,
  D2_R2_BUCKET,
  D2_RELEASE_COMMIT,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
  type D2ProbeKind,
  type D2ProductionProbe,
  validateD2ProbeOutcomeForAdmission,
} from "./d2-0010-release-contract.js";

export const D2_ADMISSION_FORMAT =
  "osl.cipher-store.d2-production-admission.v3";
export const D2_ADMISSION_STATEMENT_FORMAT =
  "osl.cipher-store.d2-production-admission-statement.v1";
export const D2_ADMISSION_RECEIPT_FORMAT =
  "osl.cipher-store.d2-production-admission-receipt.v3";
export const D2_ADMISSION_MAX_LIFETIME_MS = 5 * 60 * 1000;

export type D2AdmissionRole =
  | "deployment-anchor"
  | "provider-event-anchor"
  | "probe-transcript"
  | "independent-readback";

export interface D2AdmissionProducer {
  public_key_raw_base64url: string;
  account_sha256: string;
  role: D2AdmissionRole;
  key_epoch: number;
  valid_from_ms: number;
  valid_through_ms: number;
  revoked_at_ms: number | null;
}

export const D2_ADMISSION_PRODUCERS: Readonly<
  Record<string, D2AdmissionProducer>
> = Object.freeze({});

export interface D2AdmissionChallenge {
  challenge_id: string;
  sequence: number;
  issued_at_ms: number;
  expires_at_ms: number;
  expected_evidence_sha256: string;
  expected_transcript_root_sha256: string;
  expected_authority_snapshot_sha256: string;
}

export interface D2ActiveDeployment {
  account_sha256: string;
  worker_version_id: string;
  traffic_percentage: 100;
  activated_at_ms: number;
  source: {
    commit_sha: typeof D2_RELEASE_COMMIT;
    tree_sha: typeof D2_RELEASE_TREE;
    manifest_sha256: typeof D2_RELEASE_SOURCE_SHA256;
  };
  d1_database_id: typeof D2_DATABASE_ID;
  migration_observed_at_ms: number;
  r2_bucket_name: typeof D2_R2_BUCKET;
}

export interface D2ProviderEvent {
  event_id: string;
  event_type: "scheduled";
  invocation_source: "cloudflare-provider";
  worker_version_id: string;
  cron: "*/5 * * * *";
  scheduled_at_ms: number;
  observed_at_ms: number;
  marker: "[attachment-sweep-cycle] complete";
}

export interface D2ProviderEventIdentity {
  event_id: string;
  event_sha256: string;
  worker_version_id: string;
  observed_at_ms: number;
}

export interface D2AdmissionAuthority {
  nowMs(): number;
  loadChallenge(challengeId: string): Promise<D2AdmissionChallenge | null>;
  loadActiveDeployment(): Promise<D2ActiveDeployment | null>;
  loadProviderEventIdentity(
    eventId: string,
  ): Promise<D2ProviderEventIdentity | null>;
  loadPostOperationReadback(
    probeId: string,
  ): Promise<D2PostOperationReadbackAnchor | null>;
  consumeChallenge(input: {
    challenge_id: string;
    sequence: number;
    evidence_sha256: string;
    transcript_root_sha256: string;
    authority_snapshot_sha256: string;
    consumed_at_ms: number;
    expires_at_ms: number;
  }): Promise<boolean>;
}

export interface D2AdmissionStatement<T> {
  format: typeof D2_ADMISSION_STATEMENT_FORMAT;
  producer_id: string;
  key_epoch: number;
  challenge_id: string;
  sequence: number;
  algorithm: "Ed25519";
  payload: T;
  signature_base64url: string;
}

export interface D2RawDeploymentAnchor {
  format: "osl.cipher-store.d2-raw-deployment-anchor.v1";
  observed_at_ms: number;
  export_base64url: string;
  export_sha256: string;
}

export interface D2RawEventAnchor {
  format: "osl.cipher-store.d2-raw-provider-event.v1";
  event_base64url: string;
  event_sha256: string;
}

export interface D2ProbeTranscript {
  format: "osl.cipher-store.d2-probe-transcript.v1";
  transcript_base64url: string;
  transcript_bytes_sha256: string;
}

export interface D2IndependentReadback {
  format: "osl.cipher-store.d2-independent-readback.v1";
  probe_id: string;
  kind: D2ProbeKind;
  observed_at_ms: number;
  transcript_bytes_sha256: string;
  d1_readback_base64url: string;
  d1_readback_sha256: string;
  r2_readback_base64url: string;
  r2_readback_sha256: string;
  quota_readback_base64url: string;
  quota_readback_sha256: string;
}

export interface D2PostOperationReadbackAnchor {
  probe_id: string;
  kind: D2ProbeKind;
  transcript_bytes_sha256: string;
  d1_readback_base64url: string;
  d1_readback_sha256: string;
  r2_readback_base64url: string;
  r2_readback_sha256: string;
  quota_readback_base64url: string;
  quota_readback_sha256: string;
}

export interface D2RawD1ResourceReadback {
  format: "osl.cipher-store.d2-raw-d1-resource-readback.v2";
  probe_id: string;
  kind: D2ProbeKind;
  database_id: typeof D2_DATABASE_ID;
  attachment_id: string;
  object_key: string;
  observation: "absent" | "ready" | "retained" | "unchanged";
  row_state: "ready" | "completing" | null;
  row_version: number | null;
  expected_size_bytes: number | null;
  row_sha256: string | null;
}

export interface D2RawR2ResourceReadback {
  format: "osl.cipher-store.d2-raw-r2-resource-readback.v2";
  probe_id: string;
  kind: D2ProbeKind;
  bucket_name: typeof D2_R2_BUCKET;
  object_key: string;
  observation: "absent" | "exact" | "unknown" | "unchanged";
  object_version: string | null;
  etag: string | null;
  size_bytes: number | null;
  sha256: string | null;
}

export interface D2RawQuotaResourceReadback {
  format: "osl.cipher-store.d2-raw-quota-resource-readback.v2";
  probe_id: string;
  kind: D2ProbeKind;
  account_sha256: string;
  observation: "released" | "retained" | "unchanged";
  counter_version: number;
  reservation_rows: number;
  reservation_bytes: number;
  content_rows: number;
  content_bytes: number;
  counters_sha256: string;
}

export interface D2AuthoritativeAdmissionEvidence {
  format: typeof D2_ADMISSION_FORMAT;
  challenge_id: string;
  sequence: number;
  deployment: D2AdmissionStatement<D2RawDeploymentAnchor>;
  provider_event: D2AdmissionStatement<D2RawEventAnchor>;
  probes: D2AdmissionStatement<D2ProbeTranscript>[];
  readbacks: D2AdmissionStatement<D2IndependentReadback>[];
}

export interface D2AuthoritativeAdmissionReceipt {
  format: typeof D2_ADMISSION_RECEIPT_FORMAT;
  verdict: "single-use-authority-contract-valid";
  production_authorized: false;
  challenge_id: string;
  sequence: number;
  evidence_sha256: string;
  transcript_root_sha256: string;
  authority_snapshot_sha256: string;
  worker_version_id: string;
  witness_kinds: D2ProbeKind[];
}

const SHA256_RE = /^[0-9a-f]{64}$/;
const CHALLENGE_RE = /^[0-9a-f]{64}$/;
const PROBE_ID_RE = /^[0-9a-f]{32}$/;
const EVENT_ID_RE = /^[0-9a-f]{32,128}$/;
const BASE64URL_RE = /^[A-Za-z0-9_-]+$/;

function fail(message: string): never {
  throw new Error(`D2 production admission v3: ${message}`);
}

function canonicalJson(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("canonical input contains a non-finite number");
    return JSON.stringify(value);
  }
  if (typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value !== "object") {
    fail("canonical input contains a non-JSON value");
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

function strictObject(value: unknown, keys: readonly string[], label: string) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  const object = value as Record<string, unknown>;
  const actual = Object.keys(object).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length
    || actual.some((key, index) => key !== expected[index])
  ) {
    fail(`${label} has unexpected or missing fields`);
  }
  return object;
}

function positiveInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) <= 0) {
    fail(`${label} must be a positive safe integer`);
  }
  return value as number;
}

function decodeBase64url(value: unknown, label: string): Uint8Array {
  if (
    typeof value !== "string"
    || !BASE64URL_RE.test(value)
    || value.includes("=")
  ) {
    fail(`${label} is not strict base64url`);
  }
  return new Uint8Array(Buffer.from(value, "base64url"));
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

function parseRawJson<T>(
  encoded: unknown,
  expectedDigest: unknown,
  label: string,
): Promise<T> {
  const bytes = decodeBase64url(encoded, `${label} bytes`);
  return sha256Hex(bytes).then((digest) => {
    if (
      typeof expectedDigest !== "string"
      || !SHA256_RE.test(expectedDigest)
      || digest !== expectedDigest
    ) {
      fail(`${label} digest does not bind its bytes`);
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(new TextDecoder().decode(bytes));
    } catch {
      fail(`${label} bytes are not JSON`);
    }
    if (canonicalJson(parsed) !== new TextDecoder().decode(bytes)) {
      fail(`${label} bytes are not canonical JSON`);
    }
    return parsed as T;
  });
}

async function verifyStatement<T>(
  value: unknown,
  registry: Readonly<Record<string, D2AdmissionProducer>>,
  challenge: D2AdmissionChallenge,
  role: D2AdmissionRole,
  nowMs: number,
  label: string,
): Promise<{
  statement: D2AdmissionStatement<T>;
  producerId: string;
  producer: D2AdmissionProducer;
}> {
  const currentTime = positiveInt(nowMs, `${label} current time`);
  const envelope = strictObject(value, [
    "format",
    "producer_id",
    "key_epoch",
    "challenge_id",
    "sequence",
    "algorithm",
    "payload",
    "signature_base64url",
  ], label);
  if (
    envelope.format !== D2_ADMISSION_STATEMENT_FORMAT
    || envelope.algorithm !== "Ed25519"
    || typeof envelope.producer_id !== "string"
    || envelope.challenge_id !== challenge.challenge_id
    || envelope.sequence !== challenge.sequence
  ) {
    fail(`${label} envelope is stale or malformed`);
  }
  const producer = registry[envelope.producer_id];
  const validFrom = producer
    ? positiveInt(producer.valid_from_ms, `${label} producer valid-from`)
    : 0;
  const validThrough = producer
    ? positiveInt(producer.valid_through_ms, `${label} producer valid-through`)
    : 0;
  const revokedAt = !producer || producer.revoked_at_ms === null
    ? null
    : positiveInt(producer.revoked_at_ms, `${label} producer revocation`);
  if (
    !producer
    || producer.role !== role
    || envelope.key_epoch !== producer.key_epoch
    || !SHA256_RE.test(producer.account_sha256)
    || positiveInt(producer.key_epoch, `${label} producer key epoch`)
      !== envelope.key_epoch
    || validFrom > currentTime
    || validThrough < currentTime
    || (revokedAt !== null && currentTime >= revokedAt)
  ) {
    fail(`${label} producer epoch is unknown, stale, or revoked`);
  }
  const publicKey = decodeBase64url(
    producer.public_key_raw_base64url,
    `${label} public key`,
  );
  const signature = decodeBase64url(
    envelope.signature_base64url,
    `${label} signature`,
  );
  if (publicKey.byteLength !== 32 || signature.byteLength !== 64) {
    fail(`${label} Ed25519 material has the wrong length`);
  }
  const signed = canonicalJson({
    producer_id: envelope.producer_id,
    key_epoch: envelope.key_epoch,
    challenge_id: envelope.challenge_id,
    sequence: envelope.sequence,
    payload: envelope.payload,
  });
  const key = await crypto.subtle.importKey(
    "raw",
    publicKey,
    { name: "Ed25519" },
    false,
    ["verify"],
  );
  if (!await crypto.subtle.verify(
    "Ed25519",
    key,
    signature,
    new TextEncoder().encode(`${D2_ADMISSION_STATEMENT_FORMAT}\0${signed}`),
  )) {
    fail(`${label} signature is invalid`);
  }
  return {
    statement: envelope as unknown as D2AdmissionStatement<T>,
    producerId: envelope.producer_id,
    producer,
  };
}

async function validateProbeTranscriptIdentity(
  probe: D2ProductionProbe,
  label: string,
): Promise<void> {
  if (
    typeof probe.transcript_sha256 !== "string"
    || !SHA256_RE.test(probe.transcript_sha256)
  ) {
    fail(`${label} semantic transcript digest is malformed`);
  }
  const expected = await sha256Hex(new TextEncoder().encode(canonicalJson({
    ...probe,
    transcript_sha256: "",
  })));
  if (probe.transcript_sha256 !== expected) {
    fail(`${label} semantic transcript digest does not bind its fields`);
  }
}

function validateChallenge(
  challenge: D2AdmissionChallenge,
  evidence: D2AuthoritativeAdmissionEvidence,
  evidenceSha256: string,
  nowMs: number,
): void {
  if (
    !CHALLENGE_RE.test(challenge.challenge_id)
    || evidence.challenge_id !== challenge.challenge_id
    || evidence.sequence !== challenge.sequence
    || !SHA256_RE.test(challenge.expected_evidence_sha256)
    || !SHA256_RE.test(challenge.expected_transcript_root_sha256)
    || !SHA256_RE.test(challenge.expected_authority_snapshot_sha256)
    || challenge.expected_evidence_sha256 !== evidenceSha256
    || positiveInt(challenge.sequence, "challenge sequence") !== evidence.sequence
    || positiveInt(challenge.issued_at_ms, "challenge issue time") > nowMs
    || positiveInt(challenge.expires_at_ms, "challenge expiry") < nowMs
    || challenge.expires_at_ms - challenge.issued_at_ms
      > D2_ADMISSION_MAX_LIFETIME_MS
  ) {
    fail("challenge is absent, stale, expired, or not the durable next sequence");
  }
}

function validateActiveDeployment(value: unknown): D2ActiveDeployment {
  const deployment = strictObject(value, [
    "account_sha256",
    "worker_version_id",
    "traffic_percentage",
    "activated_at_ms",
    "source",
    "d1_database_id",
    "migration_observed_at_ms",
    "r2_bucket_name",
  ], "active deployment export");
  const source = strictObject(
    deployment.source,
    ["commit_sha", "tree_sha", "manifest_sha256"],
    "active deployment source",
  );
  if (
    typeof deployment.account_sha256 !== "string"
    || !SHA256_RE.test(deployment.account_sha256)
    || typeof deployment.worker_version_id !== "string"
    || deployment.traffic_percentage !== 100
    || deployment.d1_database_id !== D2_DATABASE_ID
    || deployment.r2_bucket_name !== D2_R2_BUCKET
    || source.commit_sha !== D2_RELEASE_COMMIT
    || source.tree_sha !== D2_RELEASE_TREE
    || source.manifest_sha256 !== D2_RELEASE_SOURCE_SHA256
    || positiveInt(deployment.migration_observed_at_ms, "migration readback")
      >= positiveInt(deployment.activated_at_ms, "deployment activation")
  ) {
    fail("active deployment export does not match the reviewed release");
  }
  return deployment as unknown as D2ActiveDeployment;
}

function readbackStates(kind: D2ProbeKind) {
  if (kind === "exact-size") {
    return { d1_state: "ready", r2_state: "exact", quota_state: "retained" };
  }
  if (kind === "unknown-abort") {
    return {
      d1_state: "retained",
      r2_state: "unknown",
      quota_state: "retained",
    };
  }
  if (kind === "rollback-refusal") {
    return {
      d1_state: "unchanged",
      r2_state: "unchanged",
      quota_state: "unchanged",
    };
  }
  return { d1_state: "absent", r2_state: "absent", quota_state: "released" };
}

interface D2ChallengeRow {
  challenge_id: string;
  sequence: number;
  issued_at_ms: number;
  expires_at_ms: number;
  expected_evidence_sha256: string;
  expected_transcript_root_sha256: string;
  expected_authority_snapshot_sha256: string;
}

export async function loadD2AdmissionChallengeFromD1(
  db: D1Database,
  challengeId: string,
): Promise<D2AdmissionChallenge | null> {
  if (!CHALLENGE_RE.test(challengeId)) {
    fail("D1 challenge ID is malformed");
  }
  const row = await db.prepare(
    `SELECT challenge_id, sequence, issued_at_ms, expires_at_ms,
            expected_evidence_sha256, expected_transcript_root_sha256,
            expected_authority_snapshot_sha256
       FROM d2_admission_challenges
      WHERE challenge_id = ?`,
  ).bind(challengeId).first<D2ChallengeRow>();
  if (!row) return null;
  return {
    challenge_id: row.challenge_id,
    sequence: positiveInt(row.sequence, "D1 challenge sequence"),
    issued_at_ms: positiveInt(row.issued_at_ms, "D1 challenge issue time"),
    expires_at_ms: positiveInt(row.expires_at_ms, "D1 challenge expiry"),
    expected_evidence_sha256: sha256Digest(
      row.expected_evidence_sha256,
      "D1 expected evidence",
    ),
    expected_transcript_root_sha256: sha256Digest(
      row.expected_transcript_root_sha256,
      "D1 expected transcript root",
    ),
    expected_authority_snapshot_sha256: sha256Digest(
      row.expected_authority_snapshot_sha256,
      "D1 expected authority snapshot",
    ),
  };
}

export interface D2ChallengeConsumption {
  challenge_id: string;
  sequence: number;
  evidence_sha256: string;
  transcript_root_sha256: string;
  authority_snapshot_sha256: string;
  consumed_at_ms: number;
  expires_at_ms: number;
}

export const D2_CONSUME_CHALLENGE_SQL =
  `UPDATE d2_admission_challenges
      SET consumed_at_ms = ?,
          consumed_evidence_sha256 = ?,
          consumed_transcript_root_sha256 = ?,
          consumed_authority_snapshot_sha256 = ?
    WHERE challenge_id = ?
      AND sequence = ?
      AND expires_at_ms = ?
      AND expected_evidence_sha256 = ?
      AND expected_transcript_root_sha256 = ?
      AND expected_authority_snapshot_sha256 = ?
      AND EXISTS (
        SELECT 1
          FROM d2_admission_authority_state
         WHERE singleton_id = 1
           AND authority_snapshot_sha256 = ?
      )
      AND consumed_at_ms IS NULL
      AND expires_at_ms >= ?`;

/**
 * Execute the durable single-use challenge CAS and return D1's raw result.
 *
 * The boolean wrapper below deliberately derives admission from the real
 * binding's `meta.changes` value. Exposing this narrow result is what lets the
 * Workerd contract test prove that the exact conditional UPDATE reports one
 * winner and zero losers; no adapter is permitted to invent that metadata.
 */
export async function runD2AdmissionChallengeConsumeInD1(
  db: D1Database,
  input: D2ChallengeConsumption,
): Promise<D1Result> {
  if (
    !CHALLENGE_RE.test(input.challenge_id)
    || positiveInt(input.sequence, "D1 consume sequence") !== input.sequence
    || positiveInt(input.consumed_at_ms, "D1 consume time")
      !== input.consumed_at_ms
    || positiveInt(input.expires_at_ms, "D1 consume expiry")
      !== input.expires_at_ms
    || input.consumed_at_ms > input.expires_at_ms
  ) {
    fail("D1 challenge consumption input is stale or malformed");
  }
  const evidenceSha256 = sha256Digest(
    input.evidence_sha256,
    "D1 consumed evidence",
  );
  const transcriptRootSha256 = sha256Digest(
    input.transcript_root_sha256,
    "D1 consumed transcript root",
  );
  const authoritySnapshotSha256 = sha256Digest(
    input.authority_snapshot_sha256,
    "D1 consumed authority snapshot",
  );
  return db.prepare(D2_CONSUME_CHALLENGE_SQL).bind(
    input.consumed_at_ms,
    evidenceSha256,
    transcriptRootSha256,
    authoritySnapshotSha256,
    input.challenge_id,
    input.sequence,
    input.expires_at_ms,
    evidenceSha256,
    transcriptRootSha256,
    authoritySnapshotSha256,
    authoritySnapshotSha256,
    input.consumed_at_ms,
  ).run();
}

export async function consumeD2AdmissionChallengeInD1(
  db: D1Database,
  input: D2ChallengeConsumption,
): Promise<boolean> {
  const result = await runD2AdmissionChallengeConsumeInD1(db, input);
  return result.success === true && Number(result.meta.changes ?? 0) === 1;
}

function sha256Digest(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256_RE.test(value)) {
    fail(`${label} must be a lowercase SHA-256 digest`);
  }
  return value;
}

async function validateStateReadback(
  encoded: unknown,
  digest: unknown,
  probeId: string,
  kind: D2ProbeKind,
  component: "d1" | "r2" | "quota",
  expectedState: string,
  accountSha256: string,
  label: string,
): Promise<Record<string, unknown>> {
  const value = await parseRawJson<Record<string, unknown>>(
    encoded,
    digest,
    label,
  );
  const objectKey = `attachments/${probeId}`;
  if (component === "d1") {
    strictObject(value, [
      "format",
      "probe_id",
      "kind",
      "database_id",
      "attachment_id",
      "object_key",
      "observation",
      "row_state",
      "row_version",
      "expected_size_bytes",
      "row_sha256",
    ], label);
    if (
      value.format !== "osl.cipher-store.d2-raw-d1-resource-readback.v2"
      || value.probe_id !== probeId
      || value.kind !== kind
      || value.database_id !== D2_DATABASE_ID
      || value.attachment_id !== probeId
      || value.object_key !== objectKey
      || value.observation !== expectedState
    ) {
      fail(`${label} does not identify the expected D1 resource`);
    }
    if (expectedState === "absent") {
      if (
        value.row_state !== null
        || value.row_version !== null
        || value.expected_size_bytes !== null
        || value.row_sha256 !== null
      ) {
        fail(`${label} claims absent D1 state with retained row metadata`);
      }
      return value;
    }
    if (expectedState === "unchanged") {
      const allNull = [
        value.row_state,
        value.row_version,
        value.expected_size_bytes,
        value.row_sha256,
      ].every((item) => item === null);
      if (!allNull) fail(`${label} unchanged D1 sentinel is inconsistent`);
      return value;
    }
    if (
      value.row_state !== (expectedState === "ready" ? "ready" : "completing")
      || positiveInt(value.row_version, `${label} row version`) < 1
      || positiveInt(value.expected_size_bytes, `${label} expected size`) < 1
      || typeof value.row_sha256 !== "string"
      || !SHA256_RE.test(value.row_sha256)
    ) {
      fail(`${label} lacks exact D1 row/version/size/digest state`);
    }
    return value;
  }
  if (component === "r2") {
    strictObject(value, [
      "format",
      "probe_id",
      "kind",
      "bucket_name",
      "object_key",
      "observation",
      "object_version",
      "etag",
      "size_bytes",
      "sha256",
    ], label);
    if (
      value.format !== "osl.cipher-store.d2-raw-r2-resource-readback.v2"
      || value.probe_id !== probeId
      || value.kind !== kind
      || value.bucket_name !== D2_R2_BUCKET
      || value.object_key !== objectKey
      || value.observation !== expectedState
    ) {
      fail(`${label} does not identify the expected R2 resource`);
    }
    if (expectedState === "exact") {
      if (
        typeof value.object_version !== "string"
        || value.object_version.length === 0
        || typeof value.etag !== "string"
        || value.etag.length === 0
        || positiveInt(value.size_bytes, `${label} object size`) < 1
        || typeof value.sha256 !== "string"
        || !SHA256_RE.test(value.sha256)
      ) {
        fail(`${label} lacks exact R2 version/etag/length/digest state`);
      }
      return value;
    }
    if (
      value.object_version !== null
      || value.etag !== null
      || value.size_bytes !== null
      || value.sha256 !== null
    ) {
      fail(`${label} non-exact R2 observation contains asserted object metadata`);
    }
    return value;
  }
  strictObject(value, [
    "format",
    "probe_id",
    "kind",
    "account_sha256",
    "observation",
    "counter_version",
    "reservation_rows",
    "reservation_bytes",
    "content_rows",
    "content_bytes",
    "counters_sha256",
  ], label);
  const counterVersion = positiveInt(
    value.counter_version,
    `${label} counter version`,
  );
  const counters = {
    counter_version: counterVersion,
    reservation_rows: nonNegativeInt(
      value.reservation_rows,
      `${label} reservation rows`,
    ),
    reservation_bytes: nonNegativeInt(
      value.reservation_bytes,
      `${label} reservation bytes`,
    ),
    content_rows: nonNegativeInt(value.content_rows, `${label} content rows`),
    content_bytes: nonNegativeInt(value.content_bytes, `${label} content bytes`),
  };
  const countersSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson(counters)),
  );
  if (
    value.format !== "osl.cipher-store.d2-raw-quota-resource-readback.v2"
    || value.probe_id !== probeId
    || value.kind !== kind
    || value.account_sha256 !== accountSha256
    || value.observation !== expectedState
    || value.counters_sha256 !== countersSha256
  ) {
    fail(`${label} lacks exact quota identity/version/counter digest state`);
  }
  const total = counters.reservation_rows
    + counters.reservation_bytes
    + counters.content_rows
    + counters.content_bytes;
  if (
    (expectedState === "released" && total !== 0)
    || (expectedState === "retained" && total === 0)
  ) {
    fail(`${label} quota counters contradict their observation`);
  }
  return value;
}

function nonNegativeInt(value: unknown, label: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    fail(`${label} must be a non-negative safe integer`);
  }
  return value as number;
}

async function validateAdmission(
  input: unknown,
  registry: Readonly<Record<string, D2AdmissionProducer>>,
  authority: D2AdmissionAuthority,
): Promise<D2AuthoritativeAdmissionReceipt> {
  if (Object.keys(registry).length === 0) {
    fail("no trusted producer epochs are configured");
  }
  const evidence = strictObject(input, [
    "format",
    "challenge_id",
    "sequence",
    "deployment",
    "provider_event",
    "probes",
    "readbacks",
  ], "admission evidence") as unknown as D2AuthoritativeAdmissionEvidence;
  if (
    evidence.format !== D2_ADMISSION_FORMAT
    || !CHALLENGE_RE.test(evidence.challenge_id)
  ) {
    fail("admission evidence format is invalid");
  }
  const evidenceSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson(evidence)),
  );
  const nowMs = positiveInt(authority.nowMs(), "authority current time");
  const challenge = await authority.loadChallenge(evidence.challenge_id);
  if (!challenge) fail("challenge is not present in durable authority");
  validateChallenge(challenge, evidence, evidenceSha256, nowMs);
  const active = await authority.loadActiveDeployment();
  if (!active) fail("active deployment has no independent authority anchor");
  validateActiveDeployment(active);

  const deploymentSigned = await verifyStatement<D2RawDeploymentAnchor>(
    evidence.deployment,
    registry,
    challenge,
    "deployment-anchor",
    nowMs,
    "deployment statement",
  );
  if (deploymentSigned.producer.account_sha256 !== active.account_sha256) {
    fail("deployment authority is not bound to the active account");
  }
  const deploymentPayload = strictObject(
    deploymentSigned.statement.payload,
    ["format", "observed_at_ms", "export_base64url", "export_sha256"],
    "deployment anchor",
  );
  if (
    deploymentPayload.format !== "osl.cipher-store.d2-raw-deployment-anchor.v1"
    || positiveInt(deploymentPayload.observed_at_ms, "deployment observation")
      < challenge.issued_at_ms
  ) {
    fail("deployment anchor is stale or malformed");
  }
  const exported = validateActiveDeployment(
    await parseRawJson(
      deploymentPayload.export_base64url,
      deploymentPayload.export_sha256,
      "deployment export",
    ),
  );
  if (canonicalJson(exported) !== canonicalJson(active)) {
    fail("signed deployment facts disagree with independent active state");
  }

  const eventSigned = await verifyStatement<D2RawEventAnchor>(
    evidence.provider_event,
    registry,
    challenge,
    "provider-event-anchor",
    nowMs,
    "provider event statement",
  );
  if (eventSigned.producer.account_sha256 !== active.account_sha256) {
    fail("provider-event authority is not bound to the active account");
  }
  const eventPayload = strictObject(
    eventSigned.statement.payload,
    ["format", "event_base64url", "event_sha256"],
    "provider event anchor",
  );
  if (eventPayload.format !== "osl.cipher-store.d2-raw-provider-event.v1") {
    fail("provider event anchor is malformed");
  }
  const event = await parseRawJson<D2ProviderEvent>(
    eventPayload.event_base64url,
    eventPayload.event_sha256,
    "provider event",
  );
  strictObject(event, [
    "event_id",
    "event_type",
    "invocation_source",
    "worker_version_id",
    "cron",
    "scheduled_at_ms",
    "observed_at_ms",
    "marker",
  ], "provider event");
  if (
    !EVENT_ID_RE.test(event.event_id)
    || event.event_type !== "scheduled"
    || event.invocation_source !== "cloudflare-provider"
    || event.worker_version_id !== active.worker_version_id
    || event.cron !== "*/5 * * * *"
    || event.marker !== "[attachment-sweep-cycle] complete"
  ) {
    fail("raw provider event does not prove the active natural cron");
  }
  const eventScheduledAt = positiveInt(
    event.scheduled_at_ms,
    "provider event scheduled time",
  );
  const eventObservedAt = positiveInt(
    event.observed_at_ms,
    "provider event observation",
  );
  if (
    eventScheduledAt < challenge.issued_at_ms
    || eventObservedAt < eventScheduledAt
    || eventObservedAt > challenge.expires_at_ms
  ) {
    fail("raw provider event does not prove the active natural cron");
  }
  const eventIdentity = await authority.loadProviderEventIdentity(event.event_id);
  if (
    !eventIdentity
    || eventIdentity.event_id !== event.event_id
    || eventIdentity.event_sha256 !== eventPayload.event_sha256
    || eventIdentity.worker_version_id !== active.worker_version_id
    || eventIdentity.observed_at_ms !== event.observed_at_ms
  ) {
    fail("provider event is not present in the independent event authority");
  }

  if (
    !Array.isArray(evidence.probes)
    || !Array.isArray(evidence.readbacks)
    || evidence.probes.length !== D2_PROBE_KINDS.length
    || evidence.readbacks.length !== D2_PROBE_KINDS.length
  ) {
    fail("every probe needs one transcript and one independent readback");
  }
  const probes = new Map<D2ProbeKind, {
    probe: D2ProductionProbe;
    digest: string;
  }>();
  const producerIds = new Set([
    deploymentSigned.producerId,
    eventSigned.producerId,
  ]);
  const producerKeys = new Set([
    deploymentSigned.producer.public_key_raw_base64url,
    eventSigned.producer.public_key_raw_base64url,
  ]);
  for (const [index, value] of evidence.probes.entries()) {
    const signed = await verifyStatement<D2ProbeTranscript>(
      value,
      registry,
      challenge,
      "probe-transcript",
      nowMs,
      `probe transcript ${index}`,
    );
    producerIds.add(signed.producerId);
    producerKeys.add(signed.producer.public_key_raw_base64url);
    const payload = strictObject(
      signed.statement.payload,
      ["format", "transcript_base64url", "transcript_bytes_sha256"],
      `probe transcript ${index}`,
    );
    if (signed.producer.account_sha256 !== active.account_sha256) {
      fail(`probe transcript ${index} authority is not bound to the active account`);
    }
    if (payload.format !== "osl.cipher-store.d2-probe-transcript.v1") {
      fail(`probe transcript ${index} is malformed`);
    }
    const probe = await parseRawJson<D2ProductionProbe>(
      payload.transcript_base64url,
      payload.transcript_bytes_sha256,
      `probe transcript ${index}`,
    );
    strictObject(probe, [
      "format",
      "environment",
      "probe_id",
      "kind",
      "observed_at_ms",
      "transcript_sha256",
      "binding",
      "outcome",
    ], `probe ${index}`);
    if (
      probe.format !== D2_PROBE_FORMAT
      || probe.environment !== "production"
      || !PROBE_ID_RE.test(probe.probe_id)
      || !D2_PROBE_KINDS.includes(probe.kind)
      || positiveInt(probe.observed_at_ms, `probe ${index} observation`)
        < eventObservedAt
      || positiveInt(probe.observed_at_ms, `probe ${index} observation`)
        > challenge.expires_at_ms
      || probes.has(probe.kind)
    ) {
      fail(`probe transcript ${index} is stale, duplicated, or mismatched`);
    }
    await validateProbeTranscriptIdentity(probe, `probe ${index}`);
    const binding = strictObject(probe.binding, [
      "source",
      "account_sha256",
      "database_id",
      "worker_version_id",
      "r2_bucket_name",
    ], `probe ${index} binding`);
    if (
      canonicalJson(binding.source) !== canonicalJson(active.source)
      || binding.account_sha256 !== active.account_sha256
      || binding.database_id !== D2_DATABASE_ID
      || binding.worker_version_id !== active.worker_version_id
      || binding.r2_bucket_name !== D2_R2_BUCKET
    ) {
      fail(`probe ${index} is not bound to independently active resources`);
    }
    validateD2ProbeOutcomeForAdmission(probe.kind, probe.outcome);
    probes.set(probe.kind, {
      probe,
      digest: String(payload.transcript_bytes_sha256),
    });
  }
  const transcriptRootSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson(
      [...probes.entries()]
        .map(([kind, value]) => ({
          kind,
          probe_id: value.probe.probe_id,
          transcript_bytes_sha256: value.digest,
        }))
        .sort((left, right) => left.kind.localeCompare(right.kind)),
    )),
  );
  if (transcriptRootSha256 !== challenge.expected_transcript_root_sha256) {
    fail("probe transcript root does not match the durable challenge");
  }

  const readbackKinds = new Set<D2ProbeKind>();
  const readbackAnchors: D2PostOperationReadbackAnchor[] = [];
  for (const [index, value] of evidence.readbacks.entries()) {
    const signed = await verifyStatement<D2IndependentReadback>(
      value,
      registry,
      challenge,
      "independent-readback",
      nowMs,
      `readback ${index}`,
    );
    producerIds.add(signed.producerId);
    producerKeys.add(signed.producer.public_key_raw_base64url);
    if (signed.producer.account_sha256 !== active.account_sha256) {
      fail(`readback ${index} authority is not bound to the active account`);
    }
    const readback = strictObject(signed.statement.payload, [
      "format",
      "probe_id",
      "kind",
      "observed_at_ms",
      "transcript_bytes_sha256",
      "d1_readback_base64url",
      "d1_readback_sha256",
      "r2_readback_base64url",
      "r2_readback_sha256",
      "quota_readback_base64url",
      "quota_readback_sha256",
    ], `readback ${index}`) as unknown as D2IndependentReadback;
    const probe = probes.get(readback.kind);
    const expected = readbackStates(readback.kind);
    if (
      readback.format !== "osl.cipher-store.d2-independent-readback.v1"
      || !probe
      || readback.probe_id !== probe.probe.probe_id
      || readback.transcript_bytes_sha256 !== probe.digest
      || positiveInt(readback.observed_at_ms, `readback ${index} observation`)
        < probe.probe.observed_at_ms
      || positiveInt(readback.observed_at_ms, `readback ${index} observation`)
        > challenge.expires_at_ms
      || readbackKinds.has(readback.kind)
    ) {
      fail(`readback ${index} is stale, fabricated, or not independent`);
    }
    const d1Readback = await validateStateReadback(
      readback.d1_readback_base64url,
      readback.d1_readback_sha256,
      readback.probe_id,
      readback.kind,
      "d1",
      expected.d1_state,
      active.account_sha256,
      `readback ${index} D1 bytes`,
    );
    const r2Readback = await validateStateReadback(
      readback.r2_readback_base64url,
      readback.r2_readback_sha256,
      readback.probe_id,
      readback.kind,
      "r2",
      expected.r2_state,
      active.account_sha256,
      `readback ${index} R2 bytes`,
    );
    await validateStateReadback(
      readback.quota_readback_base64url,
      readback.quota_readback_sha256,
      readback.probe_id,
      readback.kind,
      "quota",
      expected.quota_state,
      active.account_sha256,
      `readback ${index} quota bytes`,
    );
    if (
      d1Readback.expected_size_bytes !== null
      && r2Readback.size_bytes !== null
      && d1Readback.expected_size_bytes !== r2Readback.size_bytes
    ) {
      fail(`readback ${index} D1 and R2 lengths disagree`);
    }
    if (
      d1Readback.row_sha256 !== null
      && r2Readback.sha256 !== null
      && d1Readback.row_sha256 !== r2Readback.sha256
    ) {
      fail(`readback ${index} D1 and R2 digests disagree`);
    }
    const anchor = await authority.loadPostOperationReadback(readback.probe_id);
    if (
      !anchor
      || anchor.probe_id !== readback.probe_id
      || anchor.kind !== readback.kind
      || anchor.transcript_bytes_sha256 !== readback.transcript_bytes_sha256
      || anchor.d1_readback_base64url !== readback.d1_readback_base64url
      || anchor.d1_readback_sha256 !== readback.d1_readback_sha256
      || anchor.r2_readback_base64url !== readback.r2_readback_base64url
      || anchor.r2_readback_sha256 !== readback.r2_readback_sha256
      || anchor.quota_readback_base64url !== readback.quota_readback_base64url
      || anchor.quota_readback_sha256 !== readback.quota_readback_sha256
    ) {
      fail(`readback ${index} is absent from the independent readback authority`);
    }
    readbackAnchors.push(anchor);
    readbackKinds.add(readback.kind);
  }
  if (
    probes.size !== D2_PROBE_KINDS.length
    || readbackKinds.size !== D2_PROBE_KINDS.length
    || producerIds.size !== 4
    || producerKeys.size !== 4
  ) {
    fail(
      "admission requires four distinct producer keys and complete witnesses",
    );
  }

  const authoritySnapshotSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson({
      active,
      event: eventIdentity,
      readbacks: readbackAnchors.sort(
        (left, right) => left.probe_id.localeCompare(right.probe_id),
      ),
    })),
  );
  if (
    authoritySnapshotSha256
    !== challenge.expected_authority_snapshot_sha256
  ) {
    fail("authority snapshot does not match the durable challenge");
  }
  const consumeNowMs = positiveInt(
    authority.nowMs(),
    "authority consume time",
  );
  if (consumeNowMs > challenge.expires_at_ms) {
    fail("challenge expired while evidence was being verified");
  }
  if (!await authority.consumeChallenge({
    challenge_id: challenge.challenge_id,
    sequence: challenge.sequence,
    evidence_sha256: evidenceSha256,
    transcript_root_sha256: transcriptRootSha256,
    authority_snapshot_sha256: authoritySnapshotSha256,
    consumed_at_ms: consumeNowMs,
    expires_at_ms: challenge.expires_at_ms,
  })) {
    fail("challenge or receipt sequence was already consumed");
  }
  return {
    format: D2_ADMISSION_RECEIPT_FORMAT,
    verdict: "single-use-authority-contract-valid",
    production_authorized: false,
    challenge_id: challenge.challenge_id,
    sequence: challenge.sequence,
    evidence_sha256: evidenceSha256,
    transcript_root_sha256: transcriptRootSha256,
    authority_snapshot_sha256: authoritySnapshotSha256,
    worker_version_id: active.worker_version_id,
    witness_kinds: [...probes.keys()].sort(),
  };
}

export async function verifyD2AuthoritativeAdmissionForTestsOnly(
  input: unknown,
  registry: Readonly<Record<string, D2AdmissionProducer>>,
  authority: D2AdmissionAuthority,
): Promise<D2AuthoritativeAdmissionReceipt> {
  return validateAdmission(input, registry, authority);
}

export async function verifyD2AuthoritativeProductionAdmission(
  _input: unknown,
): Promise<never> {
  fail(
    "production authority is not provisioned; trusted epochs, durable challenge "
    + "storage, and independent provider readbacks are required",
  );
}
