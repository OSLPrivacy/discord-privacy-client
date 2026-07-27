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
    authority_snapshot_sha256: string;
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
  d1_readback_sha256: string;
  r2_readback_sha256: string;
  quota_readback_sha256: string;
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
  if (value === null || typeof value !== "object") return JSON.stringify(value);
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
  if (
    !producer
    || producer.role !== role
    || envelope.key_epoch !== producer.key_epoch
    || !SHA256_RE.test(producer.account_sha256)
    || positiveInt(producer.key_epoch, `${label} producer key epoch`)
      !== envelope.key_epoch
    || positiveInt(producer.valid_from_ms, `${label} producer valid-from`) > nowMs
    || positiveInt(producer.valid_through_ms, `${label} producer valid-through`)
      < nowMs
    || nowMs < producer.valid_from_ms
    || nowMs > producer.valid_through_ms
    || (producer.revoked_at_ms !== null && nowMs >= producer.revoked_at_ms)
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
  nowMs: number,
): void {
  if (
    !CHALLENGE_RE.test(challenge.challenge_id)
    || evidence.challenge_id !== challenge.challenge_id
    || evidence.sequence !== challenge.sequence
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

async function validateStateReadback(
  encoded: unknown,
  digest: unknown,
  probeId: string,
  kind: D2ProbeKind,
  component: "d1" | "r2" | "quota",
  expectedState: string,
  label: string,
): Promise<void> {
  const value = await parseRawJson<Record<string, unknown>>(
    encoded,
    digest,
    label,
  );
  strictObject(
    value,
    ["format", "probe_id", "kind", "component", "state"],
    label,
  );
  if (
    value.format !== "osl.cipher-store.d2-raw-state-readback.v1"
    || value.probe_id !== probeId
    || value.kind !== kind
    || value.component !== component
    || value.state !== expectedState
  ) {
    fail(`${label} does not prove the expected post-operation state`);
  }
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
  const nowMs = authority.nowMs();
  const challenge = await authority.loadChallenge(evidence.challenge_id);
  if (!challenge) fail("challenge is not present in durable authority");
  validateChallenge(challenge, evidence, nowMs);
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
    || event.scheduled_at_ms < challenge.issued_at_ms
    || event.observed_at_ms < event.scheduled_at_ms
    || event.observed_at_ms > challenge.expires_at_ms
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
      || probe.observed_at_ms < event.observed_at_ms
      || probe.observed_at_ms > challenge.expires_at_ms
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
      || readback.observed_at_ms < probe.probe.observed_at_ms
      || readback.observed_at_ms > challenge.expires_at_ms
      || readbackKinds.has(readback.kind)
    ) {
      fail(`readback ${index} is stale, fabricated, or not independent`);
    }
    await validateStateReadback(
      readback.d1_readback_base64url,
      readback.d1_readback_sha256,
      readback.probe_id,
      readback.kind,
      "d1",
      expected.d1_state,
      `readback ${index} D1 bytes`,
    );
    await validateStateReadback(
      readback.r2_readback_base64url,
      readback.r2_readback_sha256,
      readback.probe_id,
      readback.kind,
      "r2",
      expected.r2_state,
      `readback ${index} R2 bytes`,
    );
    await validateStateReadback(
      readback.quota_readback_base64url,
      readback.quota_readback_sha256,
      readback.probe_id,
      readback.kind,
      "quota",
      expected.quota_state,
      `readback ${index} quota bytes`,
    );
    const anchor = await authority.loadPostOperationReadback(readback.probe_id);
    if (
      !anchor
      || anchor.probe_id !== readback.probe_id
      || anchor.kind !== readback.kind
      || anchor.transcript_bytes_sha256 !== readback.transcript_bytes_sha256
      || anchor.d1_readback_sha256 !== readback.d1_readback_sha256
      || anchor.r2_readback_sha256 !== readback.r2_readback_sha256
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

  const evidenceSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson(evidence)),
  );
  const authoritySnapshotSha256 = await sha256Hex(
    new TextEncoder().encode(canonicalJson({
      active,
      event: eventIdentity,
      readbacks: readbackAnchors.sort(
        (left, right) => left.probe_id.localeCompare(right.probe_id),
      ),
    })),
  );
  if (authority.nowMs() > challenge.expires_at_ms) {
    fail("challenge expired while evidence was being verified");
  }
  if (!await authority.consumeChallenge({
    challenge_id: challenge.challenge_id,
    sequence: challenge.sequence,
    evidence_sha256: evidenceSha256,
    authority_snapshot_sha256: authoritySnapshotSha256,
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
