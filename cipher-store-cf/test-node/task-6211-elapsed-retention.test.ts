import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import {
  REQUIRED_MUTATIONS,
  signableEvidence,
  verifyEvidence,
} from "../scripts/task-6211-elapsed-retention.mjs";

const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const publicPem = publicKey.export({ type: "spki", format: "pem" }).toString();
const checker = resolve(process.cwd(), "scripts/task-6211-elapsed-retention.mjs");

function runCli(evidence: any) {
  const directory = mkdtempSync(join(tmpdir(), "osl-task-6211-check-"));
  const evidencePath = join(directory, "evidence.json");
  const keyPath = join(directory, "observer.pem");
  try {
    writeFileSync(evidencePath, JSON.stringify(evidence));
    writeFileSync(keyPath, publicPem);
    const result = spawnSync(process.execPath, [checker, "--evidence", evidencePath, "--observer-key", keyPath], {
      cwd: process.cwd(), encoding: "utf8",
    });
    return { status: result.status, output: `${result.stdout}${result.stderr}` };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function offsets(final: number) {
  return Array.from({ length: final / 3600 + 1 }, (_, index) => index * 3600);
}

function tierEvidence(tier: "Free" | "Pro", final: number, byte: string) {
  const schedule = offsets(final);
  const start = 1_000_000_000_000n + (tier === "Pro" ? 10_000_000_000_000n : 0n);
  const object = tier === "Free" ? "1".repeat(32) : "2".repeat(32);
  const hash = byte.repeat(64);
  const generation = `r2-generation-${tier.toLowerCase()}-fresh`;
  return {
    tier,
    object_id: object,
    bytes_sha256: hash,
    provider_generation: generation,
    upload_receipt: `signed-production-upload-${tier}`,
    upload_receipt_signature: `receipt-signature-${tier}`,
    start: { wall_unix_ms: 2_000_000_000_000, monotonic_ns: start.toString() },
    schedule_offsets_seconds: schedule,
    first_unavailable_elapsed_seconds: null,
    probes: schedule.map((scheduled) => ({
      scheduled_offset_seconds: scheduled,
      monotonic_ns: (start + BigInt(scheduled) * 1_000_000_000n).toString(),
      status: 200,
      shipping_read_route: true,
      sha256: hash,
      provider_generation: generation,
      provider_audit: {
        history_complete: true,
        generation,
        delete: false,
        replace: false,
        restore: false,
        availability_gap: false,
      },
    })),
  };
}

function signed(value: any) {
  const evidence = structuredClone(value);
  evidence.observer_signature = sign(null, signableEvidence(evidence), privateKey).toString("base64");
  return evidence;
}

function positive() {
  const Free = tierEvidence("Free", 6 * 24 * 3600, "a");
  const Pro = tierEvidence("Pro", 29 * 24 * 3600, "b");
  const scheduleBody = JSON.stringify({ Free: Free.schedule_offsets_seconds, Pro: Pro.schedule_offsets_seconds });
  // The production checker canonicalizes this exact two-key object; these keys
  // are already in lexical order and both values contain only integers.
  const commitment = createHash("sha256").update(scheduleBody).digest("hex");
  return signed({
    format: "osl.task6211.elapsed-retention.v1",
    campaign_kind: "positive",
    origin: "deployed-production",
    relay: { shipping: true, url: "https://ciphers.oslprivacy.com", deployment_version: "production-version" },
    shipping_client: {
      route: "CipherStoreClient::upload_attachment_file/fetch_attachment_to_writer",
      binary_sha256: "c".repeat(64),
    },
    storage: { provider: "Cloudflare R2", production: true },
    schedule: {
      origin: "external-observer-precommit",
      committed_before_upload: true,
      max_gap_seconds: 3600,
      commitment_sha256: commitment,
    },
    time_sources: {
      wall: { kind: "external-signed-wall", proof_digest: "wall-proof" },
      monotonic: {
        kind: "kernel-clock-boottime", continuous: true, relay_settable: false,
        storage_settable: false, boot_id: "outside-control-plane-boot",
      },
    },
    tiers: { Free, Pro },
    boundary_mutation_receipts: REQUIRED_MUTATIONS.map((entry) => {
      const [tier, kind] = entry.split(":");
      return { tier, kind, exit_code: 1, control_status: 200, discarded: true, failure: `tier=${tier} detected` };
    }),
  });
}

function mutation(kind: string, tier: "Free" | "Pro") {
  const base: any = {
    format: "osl.task6211.elapsed-retention.v1",
    campaign_kind: kind,
    kind,
    tier,
    object_id: tier === "Free" ? "3".repeat(32) : "4".repeat(32),
    provider_generation: `${tier}-generation-1`,
    throwaway_deployed: true,
    discarded: true,
    control: { status: 200, hash_match: true },
  };
  if (kind === "premature-clock-advance") {
    base.reported_elapsed_seconds = tier === "Free" ? 518_400 : 2_505_600;
    base.independent_elapsed_seconds = 3_600;
  } else {
    base.audit_event = kind === "early-delete" ? "DeleteObject" : "DeleteObject+PutObject";
    base.missing_interval_seconds = 1_800;
    if (kind === "delete-reupload-identical") base.replacement_generation = `${tier}-generation-2`;
  }
  return signed(base);
}

describe("TASK 6211 fail-closed actual elapsed retention checker", () => {
  it("accepts only a complete signed production-shaped campaign", () => {
    const evidence = positive();
    const result = verifyEvidence(evidence, publicPem);
    expect(result).toMatchObject({
      free: { probes: 145, elapsed: 518_400 },
      pro: { probes: 697, elapsed: 2_505_600 },
      mutations: 6,
    });
    const cli = runCli(evidence);
    expect(cli.status).toBe(0);
    expect(cli.output).toContain("Free probes=145 elapsed=518400; Pro probes=697 elapsed=2505600; mutations=6");
    console.log("TASK6211_CHECK Free_probes=145 Free_elapsed=518400 Pro_probes=697 Pro_elapsed=2505600 max_gap=3600 mutations=6");
  });

  it.each([
    ["premature-clock-advance", "Free", "reported versus independently measured elapsed time"],
    ["premature-clock-advance", "Pro", "reported versus independently measured elapsed time"],
    ["early-delete", "Free", "audit_event=DeleteObject"],
    ["early-delete", "Pro", "audit_event=DeleteObject"],
    ["delete-reupload-identical", "Free", "replacement_generation=Free-generation-2"],
    ["delete-reupload-identical", "Pro", "replacement_generation=Pro-generation-2"],
  ])("rejects deployed %s for %s while its control remains readable", (kind, tier, message) => {
    const result = runCli(mutation(kind, tier as "Free" | "Pro"));
    expect(result.status).toBe(1);
    expect(result.output).toContain(message);
    expect(result.output).toContain(`tier=${tier}`);
    expect(result.output).toContain("control_status=200");
    console.log(`TASK6211_RED tier=${tier} kind=${kind} exit=1 control_status=200`);
  });

  it.each([
    ["Free tier", (e: any) => { delete e.tiers.Free; }, "Free tier starved"],
    ["Pro endpoint", (e: any) => { e.tiers.Pro.probes.length = 2; e.tiers.Pro.schedule_offsets_seconds.length = 2; }, "actual-duration endpoint"],
    ["independent clock", (e: any) => { delete e.time_sources.monotonic; }, "independent monotonic"],
    ["immutable generation", (e: any) => { e.tiers.Free.probes[2].provider_generation = "replacement"; }, "immutable generation changed"],
    ["scheduled read", (e: any) => { e.tiers.Free.probes.pop(); }, "scheduled read starved"],
    ["audit history", (e: any) => { e.tiers.Pro.probes[2].provider_audit.history_complete = false; }, "provider audit-history gap"],
    ["boundary mutation", (e: any) => { e.boundary_mutation_receipts.pop(); }, "boundary mutation Pro:delete-reupload-identical starved"],
  ])("names a starved %s", (_name, mutate, message) => {
    const evidence: any = positive();
    delete evidence.observer_signature;
    mutate(evidence);
    evidence.schedule.commitment_sha256 = createHash("sha256").update(JSON.stringify({
      Free: evidence.tiers?.Free?.schedule_offsets_seconds,
      Pro: evidence.tiers?.Pro?.schedule_offsets_seconds,
    })).digest("hex");
    const resigned = signed(evidence);
    expect(() => verifyEvidence(resigned, publicPem)).toThrow(message);
  });
});
