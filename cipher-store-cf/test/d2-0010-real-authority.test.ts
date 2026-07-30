/**
 * D2 real-boundary authority proof.
 *
 * These tests run inside @cloudflare/vitest-pool-workers. `env.DB` and
 * `env.ATTACHMENTS` are the actual Workerd D1/R2 bindings, not the Node SQLite
 * adapter or a stream-accepting bucket double.
 */

import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  D2_ADMISSION_PRODUCERS,
  consumeD2AdmissionChallengeInD1,
  loadD2AdmissionChallengeFromD1,
  runD2AdmissionChallengeConsumeInD1,
  type D2ChallengeConsumption,
  verifyD2AuthoritativeProductionAdmission,
} from "../scripts/d2-0010-authoritative-admission.js";
import {
  loadD2ExactReadbackFromBindings,
} from "../scripts/d2-0010-workerd-authority.js";

const CHALLENGE_ID = "1".repeat(64);
const EVIDENCE_SHA256 = "2".repeat(64);
const TRANSCRIPT_SHA256 = "3".repeat(64);
const SNAPSHOT_SHA256 = "4".repeat(64);
const ACCOUNT_SHA256 = "5".repeat(64);
const PROBE_ID = "6".repeat(32);
const FETCH_TOKEN_SHA256 = "7".repeat(64);
const ISSUED_AT_MS = 1_900_000_000_000;
const EXPIRES_AT_MS = ISSUED_AT_MS + 300_000;
const CONSUMED_AT_MS = ISSUED_AT_MS + 10_000;

async function createAuthorityTables(): Promise<void> {
  for (const sql of [
    `CREATE TABLE d2_admission_authority_state (
       singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
       authority_snapshot_sha256 TEXT NOT NULL
     ) STRICT`,
    `CREATE TABLE d2_admission_challenges (
       challenge_id TEXT PRIMARY KEY,
       sequence INTEGER NOT NULL UNIQUE CHECK(sequence > 0),
       issued_at_ms INTEGER NOT NULL CHECK(issued_at_ms > 0),
       expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > issued_at_ms),
       expected_evidence_sha256 TEXT NOT NULL,
       expected_transcript_root_sha256 TEXT NOT NULL,
       expected_authority_snapshot_sha256 TEXT NOT NULL,
       consumed_at_ms INTEGER,
       consumed_evidence_sha256 TEXT,
       consumed_transcript_root_sha256 TEXT,
       consumed_authority_snapshot_sha256 TEXT,
       CHECK(
         (consumed_at_ms IS NULL
          AND consumed_evidence_sha256 IS NULL
          AND consumed_transcript_root_sha256 IS NULL
          AND consumed_authority_snapshot_sha256 IS NULL)
         OR
         (consumed_at_ms IS NOT NULL
          AND consumed_evidence_sha256 IS NOT NULL
          AND consumed_transcript_root_sha256 IS NOT NULL
          AND consumed_authority_snapshot_sha256 IS NOT NULL)
       )
     ) STRICT`,
    `CREATE TABLE d2_admission_resource_readbacks (
       probe_id TEXT PRIMARY KEY,
       kind TEXT NOT NULL,
       transcript_bytes_sha256 TEXT NOT NULL,
       account_sha256 TEXT NOT NULL,
       attachment_id TEXT NOT NULL,
       object_key TEXT NOT NULL,
       row_version INTEGER NOT NULL CHECK(row_version > 0),
       object_version TEXT NOT NULL,
       etag TEXT NOT NULL,
       size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
       sha256 TEXT NOT NULL
     ) STRICT`,
    `CREATE TABLE d2_admission_quota_readbacks (
       account_sha256 TEXT PRIMARY KEY,
       counter_version INTEGER NOT NULL CHECK(counter_version > 0),
       reservation_rows INTEGER NOT NULL CHECK(reservation_rows >= 0),
       reservation_bytes INTEGER NOT NULL CHECK(reservation_bytes >= 0),
       content_rows INTEGER NOT NULL CHECK(content_rows >= 0),
       content_bytes INTEGER NOT NULL CHECK(content_bytes >= 0)
     ) STRICT`,
  ]) {
    const result = await env.DB.prepare(sql).run();
    expect(result.success).toBe(true);
  }
}

async function seedChallenge(): Promise<D2ChallengeConsumption> {
  await createAuthorityTables();
  const state = await env.DB.prepare(
    `INSERT INTO d2_admission_authority_state
       (singleton_id, authority_snapshot_sha256)
     VALUES (1, ?)`,
  ).bind(SNAPSHOT_SHA256).run();
  expect(state.meta.changes).toBe(1);
  const challenge = await env.DB.prepare(
    `INSERT INTO d2_admission_challenges (
       challenge_id, sequence, issued_at_ms, expires_at_ms,
       expected_evidence_sha256, expected_transcript_root_sha256,
       expected_authority_snapshot_sha256
     ) VALUES (?, 1, ?, ?, ?, ?, ?)`,
  ).bind(
    CHALLENGE_ID,
    ISSUED_AT_MS,
    EXPIRES_AT_MS,
    EVIDENCE_SHA256,
    TRANSCRIPT_SHA256,
    SNAPSHOT_SHA256,
  ).run();
  expect(challenge.meta.changes).toBe(1);
  return {
    challenge_id: CHALLENGE_ID,
    sequence: 1,
    evidence_sha256: EVIDENCE_SHA256,
    transcript_root_sha256: TRANSCRIPT_SHA256,
    authority_snapshot_sha256: SNAPSHOT_SHA256,
    consumed_at_ms: CONSUMED_AT_MS,
    expires_at_ms: EXPIRES_AT_MS,
  };
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function seedExactResource(bytes: Uint8Array): Promise<{
  version: string;
  etag: string;
  digest: string;
}> {
  await createAuthorityTables();
  const key = `attachments/${PROBE_ID}`;
  const object = await env.ATTACHMENTS.put(key, bytes);
  expect(object).not.toBeNull();
  if (!object) throw new Error("real R2 put returned no object metadata");
  const digest = await sha256Hex(bytes);
  const row = await env.DB.prepare(
    `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, 'ready', NULL)`,
  ).bind(
    PROBE_ID,
    key,
    bytes.byteLength,
    1_900_003_600,
    1_900_003_600,
    1_900_000_000,
    FETCH_TOKEN_SHA256,
  ).run();
  expect(row.meta.changes).toBe(1);
  const receipt = await env.DB.prepare(
    `INSERT INTO d2_admission_resource_readbacks (
       probe_id, kind, transcript_bytes_sha256, account_sha256,
       attachment_id, object_key, row_version, object_version, etag,
       size_bytes, sha256
     ) VALUES (?, 'exact-size', ?, ?, ?, ?, 1, ?, ?, ?, ?)`,
  ).bind(
    PROBE_ID,
    TRANSCRIPT_SHA256,
    ACCOUNT_SHA256,
    PROBE_ID,
    key,
    object.version,
    object.etag,
    bytes.byteLength,
    digest,
  ).run();
  expect(receipt.meta.changes).toBe(1);
  const quota = await env.DB.prepare(
    `INSERT INTO d2_admission_quota_readbacks (
       account_sha256, counter_version, reservation_rows, reservation_bytes,
       content_rows, content_bytes
     ) VALUES (?, 1, 0, 0, 1, ?)`,
  ).bind(ACCOUNT_SHA256, bytes.byteLength).run();
  expect(quota.meta.changes).toBe(1);
  return { version: object.version, etag: object.etag, digest };
}

describe("D2 real Workerd authority boundary", () => {
  it("keeps production admission empty and fail-closed", async () => {
    expect(Object.keys(D2_ADMISSION_PRODUCERS)).toEqual([]);
    await expect(verifyD2AuthoritativeProductionAdmission({}))
      .rejects.toThrow("production authority is not provisioned");
  });

  it("uses real D1 meta.changes for one CAS winner and durable replay refusal", async () => {
    const input = await seedChallenge();
    const loaded = await loadD2AdmissionChallengeFromD1(env.DB, CHALLENGE_ID);
    expect(loaded).toEqual({
      challenge_id: CHALLENGE_ID,
      sequence: 1,
      issued_at_ms: ISSUED_AT_MS,
      expires_at_ms: EXPIRES_AT_MS,
      expected_evidence_sha256: EVIDENCE_SHA256,
      expected_transcript_root_sha256: TRANSCRIPT_SHA256,
      expected_authority_snapshot_sha256: SNAPSHOT_SHA256,
    });

    const results = await Promise.all([
      runD2AdmissionChallengeConsumeInD1(env.DB, input),
      runD2AdmissionChallengeConsumeInD1(env.DB, input),
    ]);
    expect(results.every((result) => result.success)).toBe(true);
    expect(results.map((result) => Number(result.meta.changes)).sort())
      .toEqual([0, 1]);

    const stored = await env.DB.prepare(
      `SELECT consumed_at_ms, consumed_evidence_sha256,
              consumed_transcript_root_sha256,
              consumed_authority_snapshot_sha256
         FROM d2_admission_challenges
        WHERE challenge_id = ?`,
    ).bind(CHALLENGE_ID).first<Record<string, unknown>>();
    expect(stored).toEqual({
      consumed_at_ms: CONSUMED_AT_MS,
      consumed_evidence_sha256: EVIDENCE_SHA256,
      consumed_transcript_root_sha256: TRANSCRIPT_SHA256,
      consumed_authority_snapshot_sha256: SNAPSHOT_SHA256,
    });

    // A newly constructed consumer has no process-local consumed flag to carry
    // over. The same bound D1 state is the only replay authority after restart.
    const restartedConsumer = {
      consume: (value: D2ChallengeConsumption) =>
        consumeD2AdmissionChallengeInD1(env.DB, value),
    };
    await expect(restartedConsumer.consume(input)).resolves.toBe(false);
    const replay = await runD2AdmissionChallengeConsumeInD1(env.DB, input);
    expect(replay.success).toBe(true);
    expect(replay.meta.changes).toBe(0);
  });

  it("loads a nonempty exact-byte witness from actual bound D1 and R2", async () => {
    const bytes = new TextEncoder().encode(
      "D2 real Workerd nonempty ciphertext witness",
    );
    const expected = await seedExactResource(bytes);
    const readback = await loadD2ExactReadbackFromBindings(
      env.DB,
      env.ATTACHMENTS,
      PROBE_ID,
    );
    expect(readback).not.toBeNull();
    expect([...readback!.bytes]).toEqual([...bytes]);
    expect(readback!.object).toEqual({
      key: `attachments/${PROBE_ID}`,
      version: expected.version,
      etag: expected.etag,
      size: bytes.byteLength,
      sha256: expected.digest,
    });
    expect(readback!.raw.d1.expected_size_bytes).toBe(bytes.byteLength);
    expect(readback!.raw.d1.row_sha256).toBe(expected.digest);
    expect(readback!.raw.r2.size_bytes).toBe(bytes.byteLength);
    expect(readback!.raw.r2.sha256).toBe(expected.digest);
    expect(readback!.raw.d1.row_sha256).toBe(readback!.raw.r2.sha256);
    expect(readback!.raw.quota).toMatchObject({
      account_sha256: ACCOUNT_SHA256,
      counter_version: 1,
      reservation_rows: 0,
      reservation_bytes: 0,
      content_rows: 1,
      content_bytes: bytes.byteLength,
    });
    expect(readback!.anchor.d1_readback_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(readback!.anchor.r2_readback_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(readback!.anchor.quota_readback_sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it("refuses real R2 replacement despite a retained D1 receipt", async () => {
    const bytes = new TextEncoder().encode("original authority bytes");
    await seedExactResource(bytes);
    await env.ATTACHMENTS.put(
      `attachments/${PROBE_ID}`,
      new TextEncoder().encode("same receipt, replaced object"),
    );
    await expect(loadD2ExactReadbackFromBindings(
      env.DB,
      env.ATTACHMENTS,
      PROBE_ID,
    )).rejects.toThrow("live R2 key/version/etag/size/digest");
  });

  it("refuses shipping D1 row drift despite a retained authority receipt", async () => {
    const bytes = new TextEncoder().encode("D1 equality authority bytes");
    await seedExactResource(bytes);
    await env.DB.prepare(
      "UPDATE attachment_objects SET size_bytes = size_bytes + 1 WHERE id = ?",
    ).bind(PROBE_ID).run();
    await expect(loadD2ExactReadbackFromBindings(
      env.DB,
      env.ATTACHMENTS,
      PROBE_ID,
    )).rejects.toThrow("shipping D1 attachment row");
  });

  it("refuses a durable receipt relabelled to a different bucket key", async () => {
    const bytes = new TextEncoder().encode("bucket identity authority bytes");
    await seedExactResource(bytes);
    await env.DB.prepare(
      "UPDATE d2_admission_resource_readbacks SET object_key = ? WHERE probe_id = ?",
    ).bind(`attachments/${"a".repeat(32)}`, PROBE_ID).run();
    await expect(loadD2ExactReadbackFromBindings(
      env.DB,
      env.ATTACHMENTS,
      PROBE_ID,
    )).rejects.toThrow("durable receipt identifies the wrong resource");
  });

  it("refuses live quota drift despite a retained counter receipt", async () => {
    const bytes = new TextEncoder().encode("quota authority bytes");
    await seedExactResource(bytes);
    await env.DB.prepare(
      `INSERT INTO attachment_objects
         (id, object_key, size_bytes, expires_at, content_expires_at,
          created_at, fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, 1, ?, ?, ?, ?, 'ready', NULL)`,
    ).bind(
      "8".repeat(32),
      `attachments/${"8".repeat(32)}`,
      1_900_003_600,
      1_900_003_600,
      1_900_000_000,
      "9".repeat(64),
    ).run();
    await expect(loadD2ExactReadbackFromBindings(
      env.DB,
      env.ATTACHMENTS,
      PROBE_ID,
    )).rejects.toThrow("live quota aggregates");
  });
});
