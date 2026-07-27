import { createHash, webcrypto } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import {
  D2_CRON,
  D2_CYCLE_MARKER,
  D2_DATABASE_ID,
  D2_DATABASE_NAME,
  D2_LOCAL_EVIDENCE_FORMAT,
  D2_MIGRATION_0010_SHA256,
  D2_PROBE_FORMAT,
  D2_PROBE_KINDS,
  D2_PRODUCTION_EVIDENCE_FORMAT,
  D2_R2_BUCKET,
  D2_RECOVERY_MARKER,
  D2_RELEASE_COMMIT,
  D2_RELEASE_SOURCE_FILES,
  D2_RELEASE_SOURCE_SHA256,
  D2_RELEASE_TREE,
  D2_REQUIRED_MIGRATIONS,
  D2_SIGNED_STATEMENT_FORMAT,
  D2_TRUSTED_PRODUCERS,
  D2_WORKER_NAME,
  type D2LocalEvidence,
  type D2ProbeKind,
  type D2ProductionDeploymentObservation,
  type D2ProductionEvidence,
  type D2ProductionProbe,
  type D2SignedStatement,
  type D2TrustedProducer,
  verifyD2Migration0010LocalEvidence,
  verifyD2Migration0010ProductionRelease,
  verifyD2ProductionContractForTestsOnly,
} from "../scripts/d2-0010-release-contract.js";

const ACCOUNT_SHA256 = "a".repeat(64);
const VERSION_ID = "11111111-1111-4111-8111-111111111111";
const ROLLBACK_VERSION_ID = "22222222-2222-4222-8222-222222222222";
const PRODUCER_ID = "test-authoritative-producer";
const PROJECT_ROOT = fileURLToPath(new URL("../", import.meta.url));
let privateKey: webcrypto.CryptoKey;
let registry: Readonly<Record<string, D2TrustedProducer>>;

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

function base64url(bytes: ArrayBuffer): string {
  return Buffer.from(bytes).toString("base64url");
}

async function sign<T>(payload: T): Promise<D2SignedStatement<T>> {
  const message = new TextEncoder().encode(
    `${D2_SIGNED_STATEMENT_FORMAT}\0${canonicalJson(payload)}`,
  );
  return {
    format: D2_SIGNED_STATEMENT_FORMAT,
    producer_id: PRODUCER_ID,
    algorithm: "Ed25519",
    payload,
    signature_base64url: base64url(
      await webcrypto.subtle.sign("Ed25519", privateKey, message),
    ),
  };
}

beforeAll(async () => {
  const pair = await webcrypto.subtle.generateKey(
    "Ed25519",
    true,
    ["sign", "verify"],
  ) as webcrypto.CryptoKeyPair;
  privateKey = pair.privateKey;
  registry = Object.freeze({
    [PRODUCER_ID]: {
      public_key_raw_base64url: base64url(
        await webcrypto.subtle.exportKey("raw", pair.publicKey),
      ),
      account_sha256: ACCOUNT_SHA256,
      roles: ["cloudflare-provider", "production-probe"],
    },
  });
});

function source() {
  return {
    commit_sha: D2_RELEASE_COMMIT,
    tree_sha: D2_RELEASE_TREE,
    manifest_sha256: D2_RELEASE_SOURCE_SHA256,
  };
}

function provider(): D2ProductionDeploymentObservation {
  return {
    format: "osl.cipher-store.d2-provider-observation.v1",
    environment: "production",
    observation_source: "cloudflare-authoritative-export",
    source: source(),
    account_sha256: ACCOUNT_SHA256,
    d1: {
      database_id: D2_DATABASE_ID,
      database_name: D2_DATABASE_NAME,
      observed_at_ms: 1_000,
      applied_migrations: [...D2_REQUIRED_MIGRATIONS],
      migration_0010_sha256: D2_MIGRATION_0010_SHA256,
      recovery_marker_format: D2_RECOVERY_MARKER,
      max_claims_per_cycle: 100,
      claim_columns: ["claim_origin", "storage_fence_state"],
    },
    worker: {
      worker_name: D2_WORKER_NAME,
      version_id: VERSION_ID,
      activated_at_ms: 2_000,
      traffic_percentage: 100,
      source: source(),
      handlers: ["fetch", "scheduled"],
      d1_binding: "DB",
      d1_database_id: D2_DATABASE_ID,
      r2_binding: "ATTACHMENTS",
      r2_bucket_name: D2_R2_BUCKET,
    },
    r2: { binding: "ATTACHMENTS", bucket_name: D2_R2_BUCKET },
    cron: {
      configured: D2_CRON,
      natural_trigger_observation: "observed",
      observation_source: "cloudflare-provider-event",
      worker_version_id: VERSION_ID,
      scheduled_time_ms: 3_000,
      event_time_ms: 3_010,
      outcome: "ok",
      marker: D2_CYCLE_MARKER,
    },
  };
}

const outcomes: Record<D2ProbeKind, Record<string, unknown>> = {
  "legacy-no-object": {
    count: 1,
    created_after_0009_cutoff: true,
    unlineaged: true,
    state_before: "completing",
    expected_size_bytes: 2,
    head_before: "absent",
    abort_outcome: "succeeded",
    post_abort_head: "absent",
    row_after: "removed",
    quota_after: "released",
  },
  "exact-size": {
    count: 1,
    expected_size_bytes: 2,
    observed_size_bytes: 2,
    head_before: "exact",
    abort_calls: 0,
    delete_calls: 0,
    row_after: "ready",
    object_after: "retained",
    quota_after: "retained",
  },
  "wrong-size": {
    count: 1,
    expected_size_bytes: 2,
    observed_size_bytes: 3,
    abort_order: 1,
    delete_order: 2,
    absence_cas_order: 3,
    post_delete_head: "absent",
    row_after: "removed",
    quota_after: "released",
  },
  "no-such-upload": {
    count: 1,
    discriminator: "code",
    value: "NoSuchUpload",
    post_abort_head: "absent",
    decision: "resume-idempotently",
  },
  "unknown-abort": {
    count: 1,
    discriminator: "code",
    value: "InternalError",
    decision: "retain",
    delete_calls: 0,
    metadata_after: "retained",
    quota_after: "retained",
  },
  "crash-retry": {
    count: 1,
    absence_fence_persisted: true,
    lease_version_increased: true,
    final_cleanup: "completed",
  },
  cleanup: {},
  "rollback-refusal": {
    count: 1,
    requested_target_version_id: ROLLBACK_VERSION_ID,
    target_source_digest_sha256: "b".repeat(64),
    decision: "refused",
    reason: "migration-0010-before-worker",
    mutations_performed: 0,
  },
};

function probe(kind: D2ProbeKind, index: number): D2ProductionProbe {
  const probeIds = D2_PROBE_KINDS.map((_, item) => (item + 1).toString(16).repeat(32));
  const outcome = kind === "cleanup"
    ? {
      count: 1,
      covered_probe_ids: probeIds.slice(0, 6),
      created_rows: 6,
      created_objects: 3,
      remaining_rows: 0,
      remaining_objects: 0,
      remaining_multipart_uploads: 0,
    }
    : structuredClone(outcomes[kind]);
  return {
    format: D2_PROBE_FORMAT,
    environment: "production",
    probe_id: probeIds[index]!,
    kind,
    observed_at_ms: 4_000 + index,
    transcript_sha256: "89abcdef"[index]!.repeat(64),
    binding: {
      source: source(),
      account_sha256: ACCOUNT_SHA256,
      database_id: D2_DATABASE_ID,
      worker_version_id: VERSION_ID,
      r2_bucket_name: D2_R2_BUCKET,
    },
    outcome,
  };
}

async function evidence(): Promise<D2ProductionEvidence> {
  return {
    format: D2_PRODUCTION_EVIDENCE_FORMAT,
    environment: "production",
    provider_observation: await sign(provider()),
    probe_receipts: await Promise.all(
      D2_PROBE_KINDS.map((kind, index) => sign(probe(kind, index))),
    ),
  };
}

function localEvidence(): D2LocalEvidence {
  return {
    format: D2_LOCAL_EVIDENCE_FORMAT,
    environment: "local",
    source: source(),
    runtime: {
      engine: "workerd",
      invocation: "manual",
      scheduled_callback_observed: true,
      natural_cron_observation: "unknown",
      marker: D2_CYCLE_MARKER,
    },
    migration: {
      applied_migrations: [...D2_REQUIRED_MIGRATIONS],
      migration_0010_sha256: D2_MIGRATION_0010_SHA256,
      recovery_marker: {
        format: D2_RECOVERY_MARKER,
        max_claims_per_cycle: 100,
      },
      claim_columns: ["claim_origin", "storage_fence_state"],
    },
    witnesses: {
      legacy_no_object: structuredClone(outcomes["legacy-no-object"]) as never,
      exact_size: structuredClone(outcomes["exact-size"]) as never,
      wrong_size: structuredClone(outcomes["wrong-size"]) as never,
    },
    cleanup: {
      created_rows: 3,
      created_objects: 2,
      remaining_rows: 0,
      remaining_objects: 0,
    },
  };
}

describe("D2 migration-0010 authoritative release contract", () => {
  it("keeps local Workerd evidence local and names only executed witnesses", () => {
    expect(verifyD2Migration0010LocalEvidence(localEvidence())).toEqual({
      format: "osl.cipher-store.d2-migration-0010-local-receipt.v2",
      verdict: "local-runtime-evidence-only",
      environment: "local",
      source: source(),
      invocation: "manual",
      natural_cron_observation: "unknown",
      executed_witnesses: [
        "legacy-no-object",
        "exact-size",
        "wrong-size",
      ],
      cleanup: { remaining_rows: 0, remaining_objects: 0 },
      production_authorized: false,
    });
  });

  it("refuses production fields, natural-cron claims, and fabricated sub-witnesses in local evidence", () => {
    const productionField = localEvidence() as D2LocalEvidence & {
      worker_version_id?: string;
    };
    productionField.worker_version_id = VERSION_ID;
    expect(() => verifyD2Migration0010LocalEvidence(productionField))
      .toThrow(/unexpected or missing fields/);

    const cron = localEvidence();
    (cron.runtime.natural_cron_observation as string) = "observed";
    expect(() => verifyD2Migration0010LocalEvidence(cron)).toThrow(/cron/);

    const fabricated = localEvidence() as D2LocalEvidence & {
      retry?: Record<string, unknown>;
    };
    fabricated.retry = { no_such_upload: { count: 1 } };
    expect(() => verifyD2Migration0010LocalEvidence(fabricated))
      .toThrow(/unexpected or missing fields/);
  });

  it("blocks production while the fixed trusted-producer registry is empty", async () => {
    expect(D2_TRUSTED_PRODUCERS).toEqual({});
    await expect(verifyD2Migration0010ProductionRelease(await evidence()))
      .rejects.toThrow(/v2 production admission is retired/);
    expect(verifyD2Migration0010ProductionRelease.length).toBe(1);
  });

  it("accepts the signed semantic contract only as a test-only result", async () => {
    await expect(
      verifyD2ProductionContractForTestsOnly(await evidence(), registry),
    ).resolves.toEqual({
      format: "osl.cipher-store.d2-production-contract-test-result.v1",
      verdict: "test-only-signature-contract-valid",
      production_authorized: false,
      witness_kinds: [...D2_PROBE_KINDS].sort(),
    });
  });

  it("refuses unsigned manual/synthetic/fabricated statement mutations", async () => {
    const manual = await evidence();
    manual.provider_observation.payload.cron.observation_source =
      "worker-callback" as never;
    await expect(verifyD2ProductionContractForTestsOnly(manual, registry))
      .rejects.toThrow(/signature/);

    const synthetic = await evidence();
    synthetic.provider_observation.payload.observation_source =
      "synthetic" as never;
    await expect(verifyD2ProductionContractForTestsOnly(synthetic, registry))
      .rejects.toThrow(/signature/);

    const fabricated = await evidence();
    fabricated.probe_receipts[0]!.payload.outcome.count = 0;
    await expect(verifyD2ProductionContractForTestsOnly(fabricated, registry))
      .rejects.toThrow(/signature/);
  });

  it("refuses signed manual callback and synthetic provider claims", async () => {
    const manual = await evidence();
    manual.provider_observation.payload.cron.observation_source =
      "worker-callback" as never;
    manual.provider_observation = await sign(manual.provider_observation.payload);
    await expect(verifyD2ProductionContractForTestsOnly(manual, registry))
      .rejects.toThrow(/natural scheduled trigger/);

    const synthetic = await evidence();
    synthetic.provider_observation.payload.observation_source =
      "synthetic" as never;
    synthetic.provider_observation = await sign(
      synthetic.provider_observation.payload,
    );
    await expect(verifyD2ProductionContractForTestsOnly(synthetic, registry))
      .rejects.toThrow(/synthetic/);
  });

  it("refuses a co-mutated caller account even when every statement is re-signed", async () => {
    const expectation = await evidence() as D2ProductionEvidence & {
      expected_account_sha256?: string;
    };
    expectation.expected_account_sha256 = "c".repeat(64);
    await expect(verifyD2ProductionContractForTestsOnly(expectation, registry))
      .rejects.toThrow(/unexpected or missing fields/);

    const value = await evidence();
    const other = "c".repeat(64);
    value.provider_observation.payload.account_sha256 = other;
    value.provider_observation = await sign(value.provider_observation.payload);
    value.probe_receipts = await Promise.all(value.probe_receipts.map(
      async (receipt) => {
        receipt.payload.binding.account_sha256 = other;
        return sign(receipt.payload);
      },
    ));
    await expect(verifyD2ProductionContractForTestsOnly(value, registry))
      .rejects.toThrow(/wrong account/);
  });

  it("refuses signed empty, duplicated, or fabricated probe receipts", async () => {
    const empty = await evidence();
    empty.probe_receipts[0]!.payload.outcome.count = 0;
    empty.probe_receipts[0] = await sign(empty.probe_receipts[0]!.payload);
    await expect(verifyD2ProductionContractForTestsOnly(empty, registry))
      .rejects.toThrow(/empty/);

    const duplicate = await evidence();
    duplicate.probe_receipts[1] = duplicate.probe_receipts[0]!;
    await expect(verifyD2ProductionContractForTestsOnly(duplicate, registry))
      .rejects.toThrow(/kinds, ids, or transcripts/);

    const cleanup = await evidence();
    cleanup.probe_receipts[6]!.payload.outcome.covered_probe_ids =
      (cleanup.probe_receipts[6]!.payload.outcome.covered_probe_ids as string[])
        .slice(1);
    cleanup.probe_receipts[6] = await sign(cleanup.probe_receipts[6]!.payload);
    await expect(verifyD2ProductionContractForTestsOnly(cleanup, registry))
      .rejects.toThrow(/cleanup/);
  });

  it("refuses migration ordering, source, traffic, bucket, and NoSuchUpload drift after valid signatures", async () => {
    const workerFirst = await evidence();
    workerFirst.provider_observation.payload.d1.observed_at_ms = 2_000;
    workerFirst.provider_observation = await sign(
      workerFirst.provider_observation.payload,
    );
    await expect(verifyD2ProductionContractForTestsOnly(workerFirst, registry))
      .rejects.toThrow(/fenced/);

    const traffic = await evidence();
    traffic.provider_observation.payload.worker.traffic_percentage = 99;
    traffic.provider_observation = await sign(traffic.provider_observation.payload);
    await expect(verifyD2ProductionContractForTestsOnly(traffic, registry))
      .rejects.toThrow(/traffic/);

    const timestamp = await evidence();
    timestamp.probe_receipts[0]!.payload.observed_at_ms = 1_999;
    timestamp.probe_receipts[0] = await sign(
      timestamp.probe_receipts[0]!.payload,
    );
    await expect(verifyD2ProductionContractForTestsOnly(timestamp, registry))
      .rejects.toThrow(/predates/);

    const version = await evidence();
    version.probe_receipts[0]!.payload.binding.worker_version_id =
      ROLLBACK_VERSION_ID;
    version.probe_receipts[0] = await sign(version.probe_receipts[0]!.payload);
    await expect(verifyD2ProductionContractForTestsOnly(version, registry))
      .rejects.toThrow(/authorized deployment/);

    const bucket = await evidence();
    bucket.provider_observation.payload.r2.bucket_name = "lookalike";
    bucket.provider_observation = await sign(bucket.provider_observation.payload);
    await expect(verifyD2ProductionContractForTestsOnly(bucket, registry))
      .rejects.toThrow(/R2 bucket/);

    const noSuch = await evidence();
    noSuch.probe_receipts[3]!.payload.outcome.value = "NoSuchUpload: maybe";
    noSuch.probe_receipts[3] = await sign(noSuch.probe_receipts[3]!.payload);
    await expect(verifyD2ProductionContractForTestsOnly(noSuch, registry))
      .rejects.toThrow(/widened/);

    const sourceDrift = await evidence();
    sourceDrift.provider_observation.payload.worker.source.manifest_sha256 =
      "e".repeat(64);
    sourceDrift.provider_observation = await sign(
      sourceDrift.provider_observation.payload,
    );
    await expect(verifyD2ProductionContractForTestsOnly(sourceDrift, registry))
      .rejects.toThrow(/exact reviewed/);
  });

  it("binds the exact recovery source and migration bytes", () => {
    const manifest = createHash("sha256");
    for (const relative of D2_RELEASE_SOURCE_FILES) {
      const bytes = readFileSync(
        fileURLToPath(new URL(`../${relative}`, import.meta.url)),
      );
      manifest.update(relative);
      manifest.update("\0");
      manifest.update(String(bytes.byteLength));
      manifest.update("\0");
      manifest.update(bytes);
    }
    expect(manifest.digest("hex")).toBe(D2_RELEASE_SOURCE_SHA256);
    const migration = readFileSync(
      fileURLToPath(
        new URL(
          "../migrations/0010_continuous_predecessor_recovery.sql",
          import.meta.url,
        ),
      ),
    );
    expect(createHash("sha256").update(migration).digest("hex")).toBe(
      D2_MIGRATION_0010_SHA256,
    );
    expect(PROJECT_ROOT.endsWith("cipher-store-cf/")).toBe(true);
  });
});
