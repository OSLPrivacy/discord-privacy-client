import { webcrypto } from "node:crypto";
import { beforeAll, describe, expect, it } from "vitest";
import {
  D2_ADMISSION_FORMAT,
  D2_ADMISSION_PRODUCERS,
  D2_ADMISSION_STATEMENT_FORMAT,
  type D2ActiveDeployment,
  type D2AdmissionAuthority,
  type D2AdmissionChallenge,
  type D2AdmissionProducer,
  type D2AdmissionRole,
  type D2AdmissionStatement,
  type D2AuthoritativeAdmissionEvidence,
  type D2IndependentReadback,
  type D2PostOperationReadbackAnchor,
  type D2ProbeTranscript,
  type D2ProviderEvent,
  type D2ProviderEventIdentity,
  type D2RawDeploymentAnchor,
  type D2RawEventAnchor,
  verifyD2AuthoritativeAdmissionForTestsOnly,
  verifyD2AuthoritativeProductionAdmission,
} from "../scripts/d2-0010-authoritative-admission.js";
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
} from "../scripts/d2-0010-release-contract.js";

const NOW = 10_000_000;
const ACCOUNT = "a".repeat(64);
const VERSION = "11111111-1111-4111-8111-111111111111";
const CHALLENGE_ID = "c".repeat(64);
const roles: D2AdmissionRole[] = [
  "deployment-anchor",
  "provider-event-anchor",
  "probe-transcript",
  "independent-readback",
];
const keys = new Map<D2AdmissionRole, webcrypto.CryptoKey>();
let registry: Readonly<Record<string, D2AdmissionProducer>>;

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const object = value as Record<string, unknown>;
  return `{${Object.keys(object).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(object[key])}`
  )).join(",")}}`;
}

function bytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(canonicalJson(value));
}

async function digest(value: Uint8Array): Promise<string> {
  return Buffer.from(
    await webcrypto.subtle.digest("SHA-256", value),
  ).toString("hex");
}

async function raw(value: unknown) {
  const encoded = bytes(value);
  return {
    base64url: Buffer.from(encoded).toString("base64url"),
    sha256: await digest(encoded),
  };
}

const challenge: D2AdmissionChallenge = {
  challenge_id: CHALLENGE_ID,
  sequence: 7,
  issued_at_ms: NOW - 60_000,
  expires_at_ms: NOW + 60_000,
};

const active: D2ActiveDeployment = {
  account_sha256: ACCOUNT,
  worker_version_id: VERSION,
  traffic_percentage: 100,
  activated_at_ms: NOW - 300_000,
  source: {
    commit_sha: D2_RELEASE_COMMIT,
    tree_sha: D2_RELEASE_TREE,
    manifest_sha256: D2_RELEASE_SOURCE_SHA256,
  },
  d1_database_id: D2_DATABASE_ID,
  migration_observed_at_ms: NOW - 400_000,
  r2_bucket_name: D2_R2_BUCKET,
};

class MemoryAuthority implements D2AdmissionAuthority {
  consumed = false;
  current = NOW;
  durableChallenge: D2AdmissionChallenge | null = structuredClone(challenge);
  deployment: D2ActiveDeployment | null = structuredClone(active);

  nowMs() {
    return this.current;
  }
  async loadChallenge(id: string) {
    return this.durableChallenge?.challenge_id === id
      ? structuredClone(this.durableChallenge)
      : null;
  }
  async loadActiveDeployment() {
    return this.deployment ? structuredClone(this.deployment) : null;
  }
  async loadProviderEventIdentity(id: string) {
    const event = providerEvent();
    if (id !== event.event_id) return null;
    const encoded = await raw(event);
    return {
      event_id: event.event_id,
      event_sha256: encoded.sha256,
      worker_version_id: event.worker_version_id,
      observed_at_ms: event.observed_at_ms,
    } satisfies D2ProviderEventIdentity;
  }
  async loadPostOperationReadback(probeId: string) {
    const index = Number.parseInt(probeId[0]!, 16) - 1;
    const kind = D2_PROBE_KINDS[index];
    if (!kind || probeId !== (index + 1).toString(16).repeat(32)) return null;
    const probe = await productionProbe(kind, index);
    const transcript = await raw(probe);
    const readbacks = await rawStateReadbacks(probeId, kind);
    return {
      probe_id: probeId,
      kind,
      transcript_bytes_sha256: transcript.sha256,
      d1_readback_sha256: readbacks.d1.sha256,
      r2_readback_sha256: readbacks.r2.sha256,
      quota_readback_sha256: readbacks.quota.sha256,
    } satisfies D2PostOperationReadbackAnchor;
  }
  async consumeChallenge(input: {
    challenge_id: string;
    sequence: number;
    evidence_sha256: string;
    authority_snapshot_sha256: string;
    expires_at_ms: number;
  }) {
    await Promise.resolve();
    const eventIdentity = await this.loadProviderEventIdentity("e".repeat(32));
    const readbacks = await Promise.all(
      D2_PROBE_KINDS.map((_, index) => (
        this.loadPostOperationReadback((index + 1).toString(16).repeat(32))
      )),
    );
    const expectedSnapshot = this.deployment && eventIdentity
      && readbacks.every((value) => value !== null)
      ? await digest(bytes({
        active: this.deployment,
        event: eventIdentity,
        readbacks,
      }))
      : null;
    if (
      this.consumed
      || input.challenge_id !== this.durableChallenge?.challenge_id
      || input.sequence !== this.durableChallenge.sequence
      || input.expires_at_ms !== this.durableChallenge.expires_at_ms
      || this.current > input.expires_at_ms
      || !/^[0-9a-f]{64}$/.test(input.evidence_sha256)
      || input.authority_snapshot_sha256 !== expectedSnapshot
    ) return false;
    this.consumed = true;
    return true;
  }
}

beforeAll(async () => {
  const entries: Record<string, D2AdmissionProducer> = {};
  for (const [index, role] of roles.entries()) {
    const pair = await webcrypto.subtle.generateKey(
      "Ed25519",
      true,
      ["sign", "verify"],
    ) as webcrypto.CryptoKeyPair;
    keys.set(role, pair.privateKey);
    entries[`producer-${index}`] = {
      public_key_raw_base64url: Buffer.from(
        await webcrypto.subtle.exportKey("raw", pair.publicKey),
      ).toString("base64url"),
      account_sha256: ACCOUNT,
      role,
      key_epoch: 3,
      valid_from_ms: NOW - 1_000_000,
      valid_through_ms: NOW + 1_000_000,
      revoked_at_ms: null,
    };
  }
  registry = Object.freeze(entries);
});

async function sign<T>(
  role: D2AdmissionRole,
  payload: T,
): Promise<D2AdmissionStatement<T>> {
  const index = roles.indexOf(role);
  const producerId = `producer-${index}`;
  const unsigned = {
    producer_id: producerId,
    key_epoch: 3,
    challenge_id: CHALLENGE_ID,
    sequence: challenge.sequence,
    payload,
  };
  const message = new TextEncoder().encode(
    `${D2_ADMISSION_STATEMENT_FORMAT}\0${canonicalJson(unsigned)}`,
  );
  return {
    format: D2_ADMISSION_STATEMENT_FORMAT,
    ...unsigned,
    algorithm: "Ed25519",
    signature_base64url: Buffer.from(
      await webcrypto.subtle.sign("Ed25519", keys.get(role)!, message),
    ).toString("base64url"),
  };
}

function outcome(kind: D2ProbeKind, probeIds: string[]) {
  if (kind === "legacy-no-object") return {
    count: 1, created_after_0009_cutoff: true, unlineaged: true,
    state_before: "completing", expected_size_bytes: 2,
    head_before: "absent", abort_outcome: "succeeded",
    post_abort_head: "absent", row_after: "removed", quota_after: "released",
  };
  if (kind === "exact-size") return {
    count: 1, expected_size_bytes: 2, observed_size_bytes: 2,
    head_before: "exact", abort_calls: 0, delete_calls: 0,
    row_after: "ready", object_after: "retained", quota_after: "retained",
  };
  if (kind === "wrong-size") return {
    count: 1, expected_size_bytes: 2, observed_size_bytes: 3,
    abort_order: 1, delete_order: 2, absence_cas_order: 3,
    post_delete_head: "absent", row_after: "removed", quota_after: "released",
  };
  if (kind === "no-such-upload") return {
    count: 1, discriminator: "code", value: "NoSuchUpload",
    post_abort_head: "absent", decision: "resume-idempotently",
  };
  if (kind === "unknown-abort") return {
    count: 1, discriminator: "code", value: "InternalError",
    decision: "retain", delete_calls: 0, metadata_after: "retained",
    quota_after: "retained",
  };
  if (kind === "crash-retry") return {
    count: 1, absence_fence_persisted: true,
    lease_version_increased: true, final_cleanup: "completed",
  };
  if (kind === "cleanup") return {
    count: 1, covered_probe_ids: probeIds.slice(0, 6),
    created_rows: 6, created_objects: 3, remaining_rows: 0,
    remaining_objects: 0, remaining_multipart_uploads: 0,
  };
  return {
    count: 1,
    requested_target_version_id: "22222222-2222-4222-8222-222222222222",
    target_source_digest_sha256: "b".repeat(64),
    decision: "refused", reason: "migration-0010-before-worker",
    mutations_performed: 0,
  };
}

function states(kind: D2ProbeKind) {
  if (kind === "exact-size") return {
    d1_state: "ready", r2_state: "exact", quota_state: "retained",
  } as const;
  if (kind === "unknown-abort") return {
    d1_state: "retained", r2_state: "unknown", quota_state: "retained",
  } as const;
  if (kind === "rollback-refusal") return {
    d1_state: "unchanged", r2_state: "unchanged", quota_state: "unchanged",
  } as const;
  return {
    d1_state: "absent", r2_state: "absent", quota_state: "released",
  } as const;
}

function providerEvent(): D2ProviderEvent {
  return {
    event_id: "e".repeat(32),
    event_type: "scheduled",
    invocation_source: "cloudflare-provider",
    worker_version_id: VERSION,
    cron: "*/5 * * * *",
    scheduled_at_ms: NOW - 20_000,
    observed_at_ms: NOW - 19_000,
    marker: "[attachment-sweep-cycle] complete",
  };
}

async function productionProbe(
  kind: D2ProbeKind,
  index: number,
): Promise<D2ProductionProbe> {
  const probeIds = D2_PROBE_KINDS.map(
    (_, probeIndex) => (probeIndex + 1).toString(16).repeat(32),
  );
  const probeBase = {
    format: D2_PROBE_FORMAT as typeof D2_PROBE_FORMAT,
    environment: "production" as const,
    probe_id: probeIds[index]!,
    kind,
    observed_at_ms: NOW - 10_000 + index,
    transcript_sha256: "",
    binding: {
      source: active.source,
      account_sha256: ACCOUNT,
      database_id: D2_DATABASE_ID,
      worker_version_id: VERSION,
      r2_bucket_name: D2_R2_BUCKET,
    },
    outcome: outcome(kind, probeIds),
  };
  return {
    ...probeBase,
    transcript_sha256: await digest(bytes(probeBase)),
  };
}

async function rawStateReadbacks(probeId: string, kind: D2ProbeKind) {
  const expected = states(kind);
  const make = async (
    component: "d1" | "r2" | "quota",
    state: string,
  ) => raw({
    format: "osl.cipher-store.d2-raw-state-readback.v1",
    probe_id: probeId,
    kind,
    component,
    state,
  });
  return {
    d1: await make("d1", expected.d1_state),
    r2: await make("r2", expected.r2_state),
    quota: await make("quota", expected.quota_state),
  };
}

async function evidence(): Promise<D2AuthoritativeAdmissionEvidence> {
  const deploymentRaw = await raw(active);
  const deployment: D2RawDeploymentAnchor = {
    format: "osl.cipher-store.d2-raw-deployment-anchor.v1",
    observed_at_ms: NOW - 30_000,
    export_base64url: deploymentRaw.base64url,
    export_sha256: deploymentRaw.sha256,
  };
  const event = providerEvent();
  const eventRaw = await raw(event);
  const eventAnchor: D2RawEventAnchor = {
    format: "osl.cipher-store.d2-raw-provider-event.v1",
    event_base64url: eventRaw.base64url,
    event_sha256: eventRaw.sha256,
  };
  const probeIds = D2_PROBE_KINDS.map(
    (_, index) => (index + 1).toString(16).repeat(32),
  );
  const probes: D2AdmissionStatement<D2ProbeTranscript>[] = [];
  const readbacks: D2AdmissionStatement<D2IndependentReadback>[] = [];
  for (const [index, kind] of D2_PROBE_KINDS.entries()) {
    const probe = await productionProbe(kind, index);
    const transcript = await raw(probe);
    const stateReadbacks = await rawStateReadbacks(probe.probe_id, kind);
    const transcriptPayload: D2ProbeTranscript = {
      format: "osl.cipher-store.d2-probe-transcript.v1",
      transcript_base64url: transcript.base64url,
      transcript_bytes_sha256: transcript.sha256,
    };
    probes.push(await sign("probe-transcript", transcriptPayload));
    readbacks.push(await sign("independent-readback", {
      format: "osl.cipher-store.d2-independent-readback.v1",
      probe_id: probe.probe_id,
      kind,
      observed_at_ms: NOW - 5_000 + index,
      transcript_bytes_sha256: transcript.sha256,
      d1_readback_base64url: stateReadbacks.d1.base64url,
      d1_readback_sha256: stateReadbacks.d1.sha256,
      r2_readback_base64url: stateReadbacks.r2.base64url,
      r2_readback_sha256: stateReadbacks.r2.sha256,
      quota_readback_base64url: stateReadbacks.quota.base64url,
      quota_readback_sha256: stateReadbacks.quota.sha256,
    }));
  }
  return {
    format: D2_ADMISSION_FORMAT,
    challenge_id: CHALLENGE_ID,
    sequence: challenge.sequence,
    deployment: await sign("deployment-anchor", deployment),
    provider_event: await sign("provider-event-anchor", eventAnchor),
    probes,
    readbacks,
  };
}

describe("D2 authoritative single-use production admission", () => {
  it("keeps production fail-closed without provisioned authority", async () => {
    expect(D2_ADMISSION_PRODUCERS).toEqual({});
    await expect(verifyD2AuthoritativeProductionAdmission({}))
      .rejects.toThrow(/not provisioned/);
  });

  it("accepts a complete test-only packet once and rejects replay", async () => {
    const value = await evidence();
    const authority = new MemoryAuthority();
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(value, registry, authority),
    ).resolves.toMatchObject({
      verdict: "single-use-authority-contract-valid",
      production_authorized: false,
      challenge_id: CHALLENGE_ID,
      sequence: 7,
      worker_version_id: VERSION,
    });
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(value, registry, authority),
    ).rejects.toThrow(/consumed/);
  });

  it("refuses expired challenges and revoked or stale producer epochs", async () => {
    const expired = new MemoryAuthority();
    expired.current = challenge.expires_at_ms + 1;
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        await evidence(),
        registry,
        expired,
      ),
    ).rejects.toThrow(/expired/);

    const revoked = structuredClone(registry) as Record<string, D2AdmissionProducer>;
    revoked["producer-0"]!.revoked_at_ms = NOW;
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        await evidence(),
        revoked,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/revoked/);
  });

  it("refuses a co-mutated signed Worker version against active authority", async () => {
    const value = await evidence();
    const rawDeployment = structuredClone(active);
    rawDeployment.worker_version_id =
      "33333333-3333-4333-8333-333333333333";
    const encoded = await raw(rawDeployment);
    value.deployment = await sign("deployment-anchor", {
      format: "osl.cipher-store.d2-raw-deployment-anchor.v1",
      observed_at_ms: NOW - 30_000,
      export_base64url: encoded.base64url,
      export_sha256: encoded.sha256,
    });
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        value,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/disagree/);
  });

  it("refuses relabelled manual events and event-byte digest drift", async () => {
    const manual = await evidence();
    const event: D2ProviderEvent = {
      event_id: "e".repeat(32),
      event_type: "scheduled",
      invocation_source: "cloudflare-provider",
      worker_version_id: VERSION,
      cron: "*/5 * * * *",
      scheduled_at_ms: NOW - 20_000,
      observed_at_ms: NOW - 19_000,
      marker: "[attachment-sweep-cycle] complete",
    };
    const encoded = await raw({ ...event, invocation_source: "manual" });
    manual.provider_event = await sign("provider-event-anchor", {
      format: "osl.cipher-store.d2-raw-provider-event.v1",
      event_base64url: encoded.base64url,
      event_sha256: encoded.sha256,
    });
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        manual,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/natural cron/);

    const drift = await evidence();
    drift.provider_event.payload.event_sha256 = "f".repeat(64);
    drift.provider_event = await sign(
      "provider-event-anchor",
      drift.provider_event.payload,
    );
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        drift,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/digest/);

    const unanchored = new MemoryAuthority();
    unanchored.loadProviderEventIdentity = async () => null;
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        await evidence(),
        registry,
        unanchored,
      ),
    ).rejects.toThrow(/independent event authority/);
  });

  it("refuses fabricated transcript bytes and missing independent readback", async () => {
    const fabricated = await evidence();
    fabricated.probes[0]!.payload.transcript_bytes_sha256 = "f".repeat(64);
    fabricated.probes[0] = await sign(
      "probe-transcript",
      fabricated.probes[0]!.payload,
    );
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        fabricated,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/digest/);

    const semanticDrift = await evidence();
    const payload = semanticDrift.probes[0]!.payload;
    const decoded = JSON.parse(
      Buffer.from(payload.transcript_base64url, "base64url").toString(),
    ) as D2ProductionProbe;
    decoded.observed_at_ms += 1;
    const changed = await raw(decoded);
    payload.transcript_base64url = changed.base64url;
    payload.transcript_bytes_sha256 = changed.sha256;
    semanticDrift.probes[0] = await sign("probe-transcript", payload);
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        semanticDrift,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/semantic transcript digest/);

    const missing = await evidence();
    missing.readbacks.pop();
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        missing,
        registry,
        new MemoryAuthority(),
      ),
    ).rejects.toThrow(/every probe/);

    const unanchored = new MemoryAuthority();
    unanchored.loadPostOperationReadback = async () => null;
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        await evidence(),
        registry,
        unanchored,
      ),
    ).rejects.toThrow(/independent readback authority/);
  });

  it("allows only one winner under concurrent replay", async () => {
    const value = await evidence();
    const authority = new MemoryAuthority();
    const results = await Promise.allSettled([
      verifyD2AuthoritativeAdmissionForTestsOnly(value, registry, authority),
      verifyD2AuthoritativeAdmissionForTestsOnly(value, registry, authority),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
  });

  it("fences an active-authority change before receipt consumption", async () => {
    const authority = new MemoryAuthority();
    const consume = authority.consumeChallenge.bind(authority);
    authority.consumeChallenge = async (input) => {
      authority.deployment!.worker_version_id =
        "44444444-4444-4444-8444-444444444444";
      return consume(input);
    };
    await expect(
      verifyD2AuthoritativeAdmissionForTestsOnly(
        await evidence(),
        registry,
        authority,
      ),
    ).rejects.toThrow(/consumed/);
    expect(authority.consumed).toBe(false);
  });
});
