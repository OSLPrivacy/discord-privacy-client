import { webcrypto } from "node:crypto";
import {
  D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT,
  D2_AUTHORITY_REVIEW_RECEIPT_FORMAT,
  D2_AUTHORITY_ROLES,
  D2_AUTHORITY_SIGNING_DOMAIN,
} from "../../scripts/d2-0010-authority-provisioning.js";
import {
  D2_DATABASE_ID,
  D2_R2_BUCKET,
  D2_RELEASE_COMMIT,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
} from "../../scripts/d2-0010-release-contract.js";

export const D2_AUTHORITY_FIXTURE_NOW_MS = 2_000_000_000_000;
export const D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256 = "a".repeat(64);
export const D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID =
  "11111111-1111-4111-8111-111111111111";

export const D2_AUTHORITY_FIXTURE_ROLES = D2_AUTHORITY_ROLES;

export type D2AuthorityFixtureRole =
  typeof D2_AUTHORITY_FIXTURE_ROLES[number];

export interface D2AuthorityFixtureProducer {
  producer_id: string;
  role: D2AuthorityFixtureRole;
  public_key_raw_base64url: string;
  account_sha256: string;
  key_epoch: number;
  valid_from_ms: number;
  valid_through_ms: number;
  revoked_at_ms: number | null;
}

export interface D2AuthorityProvisioningFixture {
  request: {
    format: string;
    created_at_ms: number;
    expires_at_ms: number;
    account_sha256: string;
    admission_contract: {
      commit_sha: string;
      tree_sha: string;
      contract_sha256: string;
    };
    producers: D2AuthorityFixtureProducer[];
    active_deployment: {
      account_sha256: string;
      worker_version_id: string;
      traffic_percentage: number;
      activated_at_ms: number;
      source: {
        commit_sha: string;
        tree_sha: string;
        manifest_sha256: string;
      };
      d1_database_id: string;
      migration_observed_at_ms: number;
      r2_bucket_name: string;
    };
    provider_event_identity: {
      event_id: string;
      event_sha256: string;
      worker_version_id: string;
      observed_at_ms: number;
    };
    readback_roots: {
      format: string;
      observed_at_ms: number;
      worker_version_id: string;
      d1_sha256: string;
      r2_sha256: string;
      quota_sha256: string;
    };
    admitted_product_client: {
      commit_sha: string;
      tree_sha: string;
      contract_sha256: string;
    };
  };
  privateKeysByProducerId: ReadonlyMap<string, webcrypto.CryptoKey>;
}

interface FixtureSigningRequest {
  producer_id: string;
  role: D2AuthorityFixtureRole;
  key_epoch: number;
  algorithm: "Ed25519";
  signature_domain: string;
  plan_sha256: string;
  required_review_fields: readonly string[];
}

interface FixtureUnsignedPlan {
  plan_sha256: string;
  expires_at_ms: number;
  signing_requests: readonly FixtureSigningRequest[];
}

export interface D2AuthorityReviewedReceiptFixture {
  format: string;
  plan_sha256: string;
  review_id: string;
  review_artifact_sha256: string;
  reviewed_at_ms: number;
  expires_at_ms: number;
  decision: "approve-registry-import-candidate";
  signatures: Array<{
    producer_id: string;
    role: D2AuthorityFixtureRole;
    key_epoch: number;
    algorithm: "Ed25519";
    signature_base64url: string;
  }>;
}

export async function makeValidD2AuthorityProvisioningFixture(
  nowMs = D2_AUTHORITY_FIXTURE_NOW_MS,
): Promise<D2AuthorityProvisioningFixture> {
  const privateKeysByProducerId = new Map<string, webcrypto.CryptoKey>();
  const producers: D2AuthorityFixtureProducer[] = [];

  for (const [index, role] of D2_AUTHORITY_FIXTURE_ROLES.entries()) {
    const keyPair = await webcrypto.subtle.generateKey(
      "Ed25519",
      true,
      ["sign", "verify"],
    ) as webcrypto.CryptoKeyPair;
    const producerId = `offline-${role}-${index + 1}`;
    privateKeysByProducerId.set(producerId, keyPair.privateKey);
    producers.push({
      producer_id: producerId,
      role,
      public_key_raw_base64url: Buffer.from(
        await webcrypto.subtle.exportKey("raw", keyPair.publicKey),
      ).toString("base64url"),
      account_sha256: D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256,
      key_epoch: index + 1,
      valid_from_ms: nowMs - 86_400_000,
      valid_through_ms: nowMs + 86_400_000,
      revoked_at_ms: null,
    });
  }

  const admittedProductClient = {
    commit_sha: "b".repeat(40),
    tree_sha: "c".repeat(40),
    contract_sha256: "d".repeat(64),
  };

  return {
    request: {
      format: D2_AUTHORITY_PROVISIONING_REQUEST_FORMAT,
      created_at_ms: nowMs,
      expires_at_ms: nowMs + 300_000,
      account_sha256: D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256,
      admission_contract: {
        commit_sha: "9".repeat(40),
        tree_sha: "8".repeat(40),
        contract_sha256: "6".repeat(64),
      },
      producers,
      active_deployment: {
        account_sha256: D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256,
        worker_version_id: D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID,
        traffic_percentage: 100,
        activated_at_ms: nowMs - 60_000,
        source: {
          commit_sha: D2_RELEASE_COMMIT,
          tree_sha: D2_RELEASE_TREE,
          manifest_sha256: D2_RELEASE_SOURCE_SHA256,
        },
        d1_database_id: D2_DATABASE_ID,
        migration_observed_at_ms: nowMs - 70_000,
        r2_bucket_name: D2_R2_BUCKET,
      },
      provider_event_identity: {
        event_id: "e".repeat(32),
        event_sha256: "f".repeat(64),
        worker_version_id: D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID,
        observed_at_ms: nowMs - 49_000,
      },
      readback_roots: {
        format: "osl.cipher-store.d2-authority-readback-roots.v1",
        observed_at_ms: nowMs - 40_000,
        worker_version_id: D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID,
        d1_sha256: "1".repeat(64),
        r2_sha256: "2".repeat(64),
        quota_sha256: "3".repeat(64),
      },
      admitted_product_client: admittedProductClient,
    },
    privateKeysByProducerId,
  };
}

export function cloneD2AuthorityFixtureRequest(
  fixture: D2AuthorityProvisioningFixture,
): D2AuthorityProvisioningFixture["request"] {
  return structuredClone(fixture.request);
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

export async function signD2AuthorityPlanForFixture(
  plan: FixtureUnsignedPlan,
  privateKeysByProducerId: ReadonlyMap<string, webcrypto.CryptoKey>,
  reviewedAtMs = D2_AUTHORITY_FIXTURE_NOW_MS + 1_000,
): Promise<D2AuthorityReviewedReceiptFixture> {
  const receiptBody = {
    plan_sha256: plan.plan_sha256,
    review_id: "7".repeat(64),
    review_artifact_sha256: "8".repeat(64),
    reviewed_at_ms: reviewedAtMs,
    expires_at_ms: plan.expires_at_ms,
    decision: "approve-registry-import-candidate" as const,
  };
  const signatures = [];
  for (const request of plan.signing_requests) {
    const privateKey = privateKeysByProducerId.get(request.producer_id);
    if (!privateKey) {
      throw new Error(`missing fixture private key for ${request.producer_id}`);
    }
    signatures.push({
      producer_id: request.producer_id,
      role: request.role,
      key_epoch: request.key_epoch,
      algorithm: "Ed25519" as const,
      signature_base64url: Buffer.from(
        await webcrypto.subtle.sign(
          "Ed25519",
          privateKey,
          new TextEncoder().encode(
            `${D2_AUTHORITY_SIGNING_DOMAIN}\0${canonicalJson(receiptBody)}\0`
            + `${request.producer_id}\0${request.role}\0${request.key_epoch}`,
          ),
        ),
      ).toString("base64url"),
    });
  }
  return {
    format: D2_AUTHORITY_REVIEW_RECEIPT_FORMAT,
    ...receiptBody,
    signatures,
  };
}
