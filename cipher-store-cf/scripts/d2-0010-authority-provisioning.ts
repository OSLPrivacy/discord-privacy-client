/**
 * Offline-only D2 authority provisioning preflight.
 *
 * This module never generates private keys, mutates the shipping registry,
 * talks to Cloudflare, or authorizes production. It produces a canonical,
 * unsigned review plan and verifies a later four-party review receipt into a
 * registry candidate that still requires an explicit source import.
 */

import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import {
  D2_ADMISSION_PRODUCERS,
  type D2ActiveDeployment,
  type D2AdmissionProducer,
  type D2AdmissionRole,
  type D2ProviderEventIdentity,
} from "./d2-0010-authoritative-admission.js";
import {
  D2_DATABASE_ID,
  D2_R2_BUCKET,
  D2_RELEASE_COMMIT,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
} from "./d2-0010-release-contract.js";

export const D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT =
  "osl.cipher-store.d2-authority-provisioning-request.v1";
export const D2_AUTHORITY_UNSIGNED_PLAN_FORMAT =
  "osl.cipher-store.d2-authority-unsigned-plan.v1";
export const D2_AUTHORITY_REVIEW_RECEIPT_FORMAT =
  "osl.cipher-store.d2-authority-review-receipt.v1";
export const D2_AUTHORITY_REGISTRY_CANDIDATE_FORMAT =
  "osl.cipher-store.d2-authority-registry-candidate.v1";
export const D2_AUTHORITY_READBACK_ROOTS_FORMAT =
  "osl.cipher-store.d2-authority-readback-roots.v1";
export const D2_AUTHORITY_SIGNING_DOMAIN =
  "osl.cipher-store.d2-authority-plan-signature.v1";
export const D2_AUTHORITY_MAX_PLAN_LIFETIME_MS = 24 * 60 * 60 * 1000;
export const D2_AUTHORITY_MAX_KEY_LIFETIME_MS = 366 * 24 * 60 * 60 * 1000;

export const D2_AUTHORITY_ROLES: readonly D2AdmissionRole[] = Object.freeze([
  "deployment-anchor",
  "provider-event-anchor",
  "probe-transcript",
  "independent-readback",
]);

export interface D2AuthorityContractPin {
  commit_sha: string;
  tree_sha: string;
  contract_sha256: string;
}

export interface D2AuthorityReadbackRoots {
  format: typeof D2_AUTHORITY_READBACK_ROOTS_FORMAT;
  observed_at_ms: number;
  worker_version_id: string;
  d1_sha256: string;
  r2_sha256: string;
  quota_sha256: string;
}

export interface D2AuthorityProvisioningRequest {
  format: typeof D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT;
  created_at_ms: number;
  expires_at_ms: number;
  account_sha256: string;
  admission_contract: D2AuthorityContractPin;
  producers: Array<D2AdmissionProducer & { producer_id: string }>;
  active_deployment: D2ActiveDeployment;
  provider_event_identity: D2ProviderEventIdentity;
  readback_roots: D2AuthorityReadbackRoots;
  admitted_product_client: D2AuthorityContractPin;
}

export interface D2AuthoritySigningRequest {
  producer_id: string;
  role: D2AdmissionRole;
  key_epoch: number;
  algorithm: "Ed25519";
  signature_domain: typeof D2_AUTHORITY_SIGNING_DOMAIN;
  plan_sha256: string;
  required_review_fields: [
    "review_id",
    "review_artifact_sha256",
    "reviewed_at_ms",
    "expires_at_ms",
    "decision",
  ];
}

export interface D2AuthorityUnsignedPlan {
  format: typeof D2_AUTHORITY_UNSIGNED_PLAN_FORMAT;
  created_at_ms: number;
  expires_at_ms: number;
  production_authorized: false;
  shipping_registry_mutated: false;
  request: D2AuthorityProvisioningRequest;
  plan_sha256: string;
  signing_requests: D2AuthoritySigningRequest[];
  signatures: [];
}

export interface D2AuthorityReviewSignature {
  producer_id: string;
  role: D2AdmissionRole;
  key_epoch: number;
  algorithm: "Ed25519";
  signature_base64url: string;
}

export interface D2AuthorityReviewReceipt {
  format: typeof D2_AUTHORITY_REVIEW_RECEIPT_FORMAT;
  plan_sha256: string;
  review_id: string;
  review_artifact_sha256: string;
  reviewed_at_ms: number;
  expires_at_ms: number;
  decision: "approve-registry-import-candidate";
  signatures: D2AuthorityReviewSignature[];
}

export interface D2AuthorityRegistryCandidate {
  format: typeof D2_AUTHORITY_REGISTRY_CANDIDATE_FORMAT;
  plan_sha256: string;
  review_id: string;
  review_artifact_sha256: string;
  production_authorized: false;
  shipping_registry_mutated: false;
  requires_source_import: true;
  producers: Readonly<Record<string, D2AdmissionProducer>>;
}

const SHA256_RE = /^[0-9a-f]{64}$/;
const GIT_OID_RE = /^[0-9a-f]{40}$/;
const PRODUCER_ID_RE = /^[a-z0-9](?:[a-z0-9-]{1,62}[a-z0-9])?$/;
const EVENT_ID_RE = /^[0-9a-f]{32,128}$/;
const WORKER_VERSION_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const BASE64URL_RE = /^[A-Za-z0-9_-]+$/;
const textEncoder = new TextEncoder();

function fail(message: string): never {
  throw new Error(`D2 authority provisioning: ${message}`);
}

function canonicalJson(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string" || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("canonical input contains a non-finite number");
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (typeof value !== "object") {
    fail("canonical input contains a non-JSON value");
  }
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

function strictObject(
  value: unknown,
  keys: readonly string[],
  label: string,
): Record<string, unknown> {
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

function sha256(value: unknown, label: string): string {
  if (typeof value !== "string" || !SHA256_RE.test(value)) {
    fail(`${label} must be a lowercase SHA-256 digest`);
  }
  return value;
}

function gitOid(value: unknown, label: string): string {
  if (typeof value !== "string" || !GIT_OID_RE.test(value)) {
    fail(`${label} must be a lowercase 40-character Git object ID`);
  }
  return value;
}

function strictBase64url(value: unknown, label: string): Uint8Array {
  if (
    typeof value !== "string"
    || !BASE64URL_RE.test(value)
    || value.includes("=")
  ) {
    fail(`${label} must be unpadded base64url`);
  }
  const bytes = new Uint8Array(Buffer.from(value, "base64url"));
  if (Buffer.from(bytes).toString("base64url") !== value) {
    fail(`${label} is not canonical base64url`);
  }
  return bytes;
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  return Buffer.from(
    await crypto.subtle.digest("SHA-256", bytes),
  ).toString("hex");
}

function validateContractPin(
  value: unknown,
  label: string,
): D2AuthorityContractPin {
  const pin = strictObject(
    value,
    ["commit_sha", "tree_sha", "contract_sha256"],
    label,
  );
  return {
    commit_sha: gitOid(pin.commit_sha, `${label} commit`),
    tree_sha: gitOid(pin.tree_sha, `${label} tree`),
    contract_sha256: sha256(pin.contract_sha256, `${label} contract`),
  };
}

function validateActiveDeployment(
  value: unknown,
  accountSha256: string,
): D2ActiveDeployment {
  const deployment = strictObject(value, [
    "account_sha256",
    "worker_version_id",
    "traffic_percentage",
    "activated_at_ms",
    "source",
    "d1_database_id",
    "migration_observed_at_ms",
    "r2_bucket_name",
  ], "active deployment");
  const source = strictObject(
    deployment.source,
    ["commit_sha", "tree_sha", "manifest_sha256"],
    "active deployment source",
  );
  const activatedAt = positiveInt(
    deployment.activated_at_ms,
    "active deployment activation",
  );
  const migrationAt = positiveInt(
    deployment.migration_observed_at_ms,
    "active deployment migration observation",
  );
  if (
    deployment.account_sha256 !== accountSha256
    || typeof deployment.worker_version_id !== "string"
    || !WORKER_VERSION_RE.test(deployment.worker_version_id)
    || deployment.traffic_percentage !== 100
    || source.commit_sha !== D2_RELEASE_COMMIT
    || source.tree_sha !== D2_RELEASE_TREE
    || source.manifest_sha256 !== D2_RELEASE_SOURCE_SHA256
    || deployment.d1_database_id !== D2_DATABASE_ID
    || deployment.r2_bucket_name !== D2_R2_BUCKET
    || migrationAt >= activatedAt
  ) {
    fail("active deployment/traffic snapshot does not bind the reviewed release");
  }
  return {
    account_sha256: accountSha256,
    worker_version_id: deployment.worker_version_id as string,
    traffic_percentage: 100,
    activated_at_ms: activatedAt,
    source: {
      commit_sha: D2_RELEASE_COMMIT,
      tree_sha: D2_RELEASE_TREE,
      manifest_sha256: D2_RELEASE_SOURCE_SHA256,
    },
    d1_database_id: D2_DATABASE_ID,
    migration_observed_at_ms: migrationAt,
    r2_bucket_name: D2_R2_BUCKET,
  };
}

function validateProviderEventIdentity(
  value: unknown,
  deployment: D2ActiveDeployment,
  createdAtMs: number,
): D2ProviderEventIdentity {
  const identity = strictObject(value, [
    "event_id",
    "event_sha256",
    "worker_version_id",
    "observed_at_ms",
  ], "provider event identity");
  const observedAt = positiveInt(
    identity.observed_at_ms,
    "provider event observation",
  );
  if (
    typeof identity.event_id !== "string"
    || !EVENT_ID_RE.test(identity.event_id)
    || typeof identity.worker_version_id !== "string"
    || identity.worker_version_id !== deployment.worker_version_id
    || observedAt < deployment.activated_at_ms
    || observedAt > createdAtMs
  ) {
    fail("provider event identity is stale or bound to the wrong deployment");
  }
  return {
    event_id: identity.event_id,
    event_sha256: sha256(identity.event_sha256, "provider event digest"),
    worker_version_id: identity.worker_version_id,
    observed_at_ms: observedAt,
  };
}

function validateReadbackRoots(
  value: unknown,
  deployment: D2ActiveDeployment,
  providerEvent: D2ProviderEventIdentity,
  createdAtMs: number,
): D2AuthorityReadbackRoots {
  const roots = strictObject(value, [
    "format",
    "observed_at_ms",
    "worker_version_id",
    "d1_sha256",
    "r2_sha256",
    "quota_sha256",
  ], "readback roots");
  const observedAt = positiveInt(
    roots.observed_at_ms,
    "readback roots observation",
  );
  const digests = [
    sha256(roots.d1_sha256, "D1 readback root"),
    sha256(roots.r2_sha256, "R2 readback root"),
    sha256(roots.quota_sha256, "quota readback root"),
  ];
  if (
    roots.format !== D2_AUTHORITY_READBACK_ROOTS_FORMAT
    || roots.worker_version_id !== deployment.worker_version_id
    || observedAt < providerEvent.observed_at_ms
    || observedAt > createdAtMs
    || new Set(digests).size !== digests.length
  ) {
    fail("D1/R2/quota readback roots are stale, duplicated, or mismatched");
  }
  return {
    format: D2_AUTHORITY_READBACK_ROOTS_FORMAT,
    observed_at_ms: observedAt,
    worker_version_id: deployment.worker_version_id,
    d1_sha256: digests[0]!,
    r2_sha256: digests[1]!,
    quota_sha256: digests[2]!,
  };
}

function validateProducers(
  value: unknown,
  accountSha256: string,
  createdAtMs: number,
  expiresAtMs: number,
): Array<D2AdmissionProducer & { producer_id: string }> {
  if (!Array.isArray(value) || value.length !== D2_AUTHORITY_ROLES.length) {
    fail("exactly four role-separated producers are required");
  }
  const producerIds = new Set<string>();
  const roles = new Set<D2AdmissionRole>();
  const publicKeys = new Set<string>();
  const producers = value.map((item, index) => {
    const producer = strictObject(item, [
      "producer_id",
      "public_key_raw_base64url",
      "account_sha256",
      "role",
      "key_epoch",
      "valid_from_ms",
      "valid_through_ms",
      "revoked_at_ms",
    ], `producer ${index}`);
    if (
      typeof producer.producer_id !== "string"
      || !PRODUCER_ID_RE.test(producer.producer_id)
      || producer.account_sha256 !== accountSha256
      || !D2_AUTHORITY_ROLES.includes(producer.role as D2AdmissionRole)
    ) {
      fail(`producer ${index} identity, account, or role is invalid`);
    }
    const key = strictBase64url(
      producer.public_key_raw_base64url,
      `producer ${index} public key`,
    );
    if (key.byteLength !== 32) {
      fail(`producer ${index} Ed25519 public key must be 32 bytes`);
    }
    const epoch = positiveInt(producer.key_epoch, `producer ${index} epoch`);
    const validFrom = positiveInt(
      producer.valid_from_ms,
      `producer ${index} valid-from`,
    );
    const validThrough = positiveInt(
      producer.valid_through_ms,
      `producer ${index} valid-through`,
    );
    const revokedAt = producer.revoked_at_ms === null
      ? null
      : positiveInt(producer.revoked_at_ms, `producer ${index} revocation`);
    if (
      validFrom > createdAtMs
      || validThrough < expiresAtMs
      || validThrough <= validFrom
      || validThrough - validFrom > D2_AUTHORITY_MAX_KEY_LIFETIME_MS
      || (revokedAt !== null && revokedAt <= expiresAtMs)
    ) {
      fail(`producer ${index} epoch is stale, expired, or revoked`);
    }
    if (
      producerIds.has(producer.producer_id)
      || roles.has(producer.role as D2AdmissionRole)
      || publicKeys.has(producer.public_key_raw_base64url as string)
    ) {
      fail("producer IDs, roles, and public keys must all be distinct");
    }
    producerIds.add(producer.producer_id);
    roles.add(producer.role as D2AdmissionRole);
    publicKeys.add(producer.public_key_raw_base64url as string);
    return {
      producer_id: producer.producer_id,
      public_key_raw_base64url:
        producer.public_key_raw_base64url as string,
      account_sha256: accountSha256,
      role: producer.role as D2AdmissionRole,
      key_epoch: epoch,
      valid_from_ms: validFrom,
      valid_through_ms: validThrough,
      revoked_at_ms: revokedAt,
    };
  });
  if (D2_AUTHORITY_ROLES.some((role) => !roles.has(role))) {
    fail("every authority role must appear exactly once");
  }
  return producers.sort(
    (left, right) => (
      D2_AUTHORITY_ROLES.indexOf(left.role)
      - D2_AUTHORITY_ROLES.indexOf(right.role)
    ),
  );
}

function validateProvisioningRequest(
  input: unknown,
  nowMs: number,
): D2AuthorityProvisioningRequest {
  positiveInt(nowMs, "current time");
  const request = strictObject(input, [
    "format",
    "created_at_ms",
    "expires_at_ms",
    "account_sha256",
    "admission_contract",
    "producers",
    "active_deployment",
    "provider_event_identity",
    "readback_roots",
    "admitted_product_client",
  ], "provisioning request");
  if (request.format !== D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT) {
    fail("provisioning request format is invalid");
  }
  const createdAt = positiveInt(request.created_at_ms, "plan creation");
  const expiresAt = positiveInt(request.expires_at_ms, "plan expiry");
  if (
    createdAt > nowMs
    || expiresAt <= nowMs
    || expiresAt <= createdAt
    || expiresAt - createdAt > D2_AUTHORITY_MAX_PLAN_LIFETIME_MS
  ) {
    fail("provisioning request is stale, expired, or overlong");
  }
  const accountSha256 = sha256(request.account_sha256, "account digest");
  const admissionContract = validateContractPin(
    request.admission_contract,
    "admission contract",
  );
  const admittedProductClient = validateContractPin(
    request.admitted_product_client,
    "admitted product/client",
  );
  const deployment = validateActiveDeployment(
    request.active_deployment,
    accountSha256,
  );
  if (deployment.activated_at_ms > createdAt) {
    fail("active deployment observation postdates the plan");
  }
  const providerEvent = validateProviderEventIdentity(
    request.provider_event_identity,
    deployment,
    createdAt,
  );
  const roots = validateReadbackRoots(
    request.readback_roots,
    deployment,
    providerEvent,
    createdAt,
  );
  const producers = validateProducers(
    request.producers,
    accountSha256,
    createdAt,
    expiresAt,
  );
  return {
    format: D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT,
    created_at_ms: createdAt,
    expires_at_ms: expiresAt,
    account_sha256: accountSha256,
    admission_contract: admissionContract,
    producers,
    active_deployment: deployment,
    provider_event_identity: providerEvent,
    readback_roots: roots,
    admitted_product_client: admittedProductClient,
  };
}

function assertShippingRegistryEmpty(): void {
  if (Object.keys(D2_ADMISSION_PRODUCERS).length !== 0) {
    fail(
      "shipping registry is not empty; this offline package cannot activate it",
    );
  }
}

function reviewSigningMessage(
  planSha256: string,
  receipt: {
    review_id: string;
    review_artifact_sha256: string;
    reviewed_at_ms: number;
    expires_at_ms: number;
    decision: "approve-registry-import-candidate";
  },
  producer: D2AdmissionProducer & { producer_id: string },
): Uint8Array {
  const reviewBinding = canonicalJson({
    plan_sha256: planSha256,
    ...receipt,
  });
  return textEncoder.encode(
    `${D2_AUTHORITY_SIGNING_DOMAIN}\0${reviewBinding}\0`
    + `${producer.producer_id}\0${producer.role}\0${producer.key_epoch}`,
  );
}

export async function buildUnsignedD2AuthorityPlan(
  input: unknown,
  nowMs: number,
): Promise<D2AuthorityUnsignedPlan> {
  assertShippingRegistryEmpty();
  const request = validateProvisioningRequest(input, nowMs);
  const body: Omit<
    D2AuthorityUnsignedPlan,
    "plan_sha256" | "signing_requests" | "signatures"
  > = {
    format: D2_AUTHORITY_UNSIGNED_PLAN_FORMAT,
    created_at_ms: request.created_at_ms,
    expires_at_ms: request.expires_at_ms,
    production_authorized: false as const,
    shipping_registry_mutated: false as const,
    request,
  };
  const planSha256 = await sha256Hex(
    textEncoder.encode(canonicalJson(body)),
  );
  const signingRequests = request.producers.map(
    (producer): D2AuthoritySigningRequest => ({
      producer_id: producer.producer_id,
      role: producer.role,
      key_epoch: producer.key_epoch,
      algorithm: "Ed25519",
      signature_domain: D2_AUTHORITY_SIGNING_DOMAIN,
      plan_sha256: planSha256,
      required_review_fields: [
        "review_id",
        "review_artifact_sha256",
        "reviewed_at_ms",
        "expires_at_ms",
        "decision",
      ],
    }),
  );
  return {
    ...body,
    plan_sha256: planSha256,
    signing_requests: signingRequests,
    signatures: [],
  };
}

async function validateUnsignedPlan(
  value: unknown,
  nowMs: number,
): Promise<D2AuthorityUnsignedPlan> {
  const plan = strictObject(value, [
    "format",
    "created_at_ms",
    "expires_at_ms",
    "production_authorized",
    "shipping_registry_mutated",
    "request",
    "plan_sha256",
    "signing_requests",
    "signatures",
  ], "unsigned plan");
  if (
    plan.format !== D2_AUTHORITY_UNSIGNED_PLAN_FORMAT
    || plan.production_authorized !== false
    || plan.shipping_registry_mutated !== false
    || !Array.isArray(plan.signatures)
    || plan.signatures.length !== 0
  ) {
    fail("unsigned plan is malformed or already contains signatures");
  }
  const rebuilt = await buildUnsignedD2AuthorityPlan(plan.request, nowMs);
  if (canonicalJson(rebuilt) !== canonicalJson(plan)) {
    fail("unsigned plan digest or signing requests do not match its request");
  }
  return rebuilt;
}

export async function verifyAndImportReviewedD2AuthorityReceipt(
  planInput: unknown,
  receiptInput: unknown,
  nowMs: number,
): Promise<D2AuthorityRegistryCandidate> {
  assertShippingRegistryEmpty();
  const plan = await validateUnsignedPlan(planInput, nowMs);
  const receipt = strictObject(receiptInput, [
    "format",
    "plan_sha256",
    "review_id",
    "review_artifact_sha256",
    "reviewed_at_ms",
    "expires_at_ms",
    "decision",
    "signatures",
  ], "review receipt");
  const reviewedAt = positiveInt(receipt.reviewed_at_ms, "review time");
  const receiptExpires = positiveInt(receipt.expires_at_ms, "receipt expiry");
  if (
    receipt.format !== D2_AUTHORITY_REVIEW_RECEIPT_FORMAT
    || receipt.plan_sha256 !== plan.plan_sha256
    || receipt.decision !== "approve-registry-import-candidate"
    || reviewedAt < plan.created_at_ms
    || reviewedAt > nowMs
    || receiptExpires !== plan.expires_at_ms
    || receiptExpires <= nowMs
  ) {
    fail("review receipt is stale, expired, or bound to the wrong plan");
  }
  const reviewId = sha256(receipt.review_id, "review ID");
  const reviewArtifactSha256 = sha256(
    receipt.review_artifact_sha256,
    "review artifact",
  );
  const reviewBinding = {
    review_id: reviewId,
    review_artifact_sha256: reviewArtifactSha256,
    reviewed_at_ms: reviewedAt,
    expires_at_ms: receiptExpires,
    decision: "approve-registry-import-candidate" as const,
  };
  if (
    !Array.isArray(receipt.signatures)
    || receipt.signatures.length !== D2_AUTHORITY_ROLES.length
  ) {
    fail("review receipt requires exactly four separate signatures");
  }
  const producers = new Map(
    plan.request.producers.map((producer) => [producer.producer_id, producer]),
  );
  const seenProducerIds = new Set<string>();
  const seenRoles = new Set<D2AdmissionRole>();
  for (const [index, value] of receipt.signatures.entries()) {
    const signature = strictObject(value, [
      "producer_id",
      "role",
      "key_epoch",
      "algorithm",
      "signature_base64url",
    ], `review signature ${index}`);
    if (
      typeof signature.producer_id !== "string"
      || !D2_AUTHORITY_ROLES.includes(signature.role as D2AdmissionRole)
      || signature.algorithm !== "Ed25519"
    ) {
      fail(`review signature ${index} identity or role is invalid`);
    }
    const producer = producers.get(signature.producer_id);
    if (
      !producer
      || signature.role !== producer.role
      || signature.key_epoch !== producer.key_epoch
      || producer.valid_from_ms > reviewedAt
      || producer.valid_through_ms < receiptExpires
      || (
        producer.revoked_at_ms !== null
        && producer.revoked_at_ms <= receiptExpires
      )
      || seenProducerIds.has(producer.producer_id)
      || seenRoles.has(producer.role)
    ) {
      fail(`review signature ${index} is duplicate, stale, or wrong-role`);
    }
    const signatureBytes = strictBase64url(
      signature.signature_base64url,
      `review signature ${index}`,
    );
    if (signatureBytes.byteLength !== 64) {
      fail(`review signature ${index} must be a 64-byte Ed25519 signature`);
    }
    let publicKey: CryptoKey;
    try {
      publicKey = await crypto.subtle.importKey(
        "raw",
        strictBase64url(
          producer.public_key_raw_base64url,
          `producer ${producer.producer_id} public key`,
        ),
        { name: "Ed25519" },
        false,
        ["verify"],
      );
    } catch {
      fail(`producer ${producer.producer_id} public key cannot be imported`);
    }
    if (!await crypto.subtle.verify(
      "Ed25519",
      publicKey,
      signatureBytes,
      reviewSigningMessage(plan.plan_sha256, reviewBinding, producer),
    )) {
      fail(`review signature ${index} is invalid`);
    }
    seenProducerIds.add(producer.producer_id);
    seenRoles.add(producer.role);
  }
  if (
    seenProducerIds.size !== D2_AUTHORITY_ROLES.length
    || D2_AUTHORITY_ROLES.some((role) => !seenRoles.has(role))
  ) {
    fail("review receipt does not cover all four authority roles");
  }
  const registry = Object.fromEntries(plan.request.producers.map(
    ({ producer_id: producerId, ...producer }) => [
      producerId,
      Object.freeze(producer),
    ],
  ));
  return {
    format: D2_AUTHORITY_REGISTRY_CANDIDATE_FORMAT,
    plan_sha256: plan.plan_sha256,
    review_id: reviewId,
    review_artifact_sha256: reviewArtifactSha256,
    production_authorized: false,
    shipping_registry_mutated: false,
    requires_source_import: true,
    producers: Object.freeze(registry),
  };
}

async function runCli(argv: string[]): Promise<void> {
  if (
    argv.length !== 2
    || argv[0] !== "--input"
    || typeof argv[1] !== "string"
  ) {
    console.error(
      "usage: node scripts/d2-0010-authority-provisioning.ts "
      + "--input <public-request.json>",
    );
    process.exitCode = 2;
    return;
  }
  let input: unknown;
  try {
    input = JSON.parse(await readFile(argv[1], "utf8"));
  } catch {
    fail("input must be a readable JSON file");
  }
  const plan = await buildUnsignedD2AuthorityPlan(input, Date.now());
  process.stdout.write(`${JSON.stringify(plan, null, 2)}\n`);
}

const entry = process.argv[1];
if (entry && import.meta.url === pathToFileURL(entry).href) {
  runCli(process.argv.slice(2)).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
