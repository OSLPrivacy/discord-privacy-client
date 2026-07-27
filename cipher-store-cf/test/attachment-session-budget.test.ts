/// HIGH-1 regression suite — "sixteen empty multipart reservations exhaust the
/// global attachment quota for seven days"
/// (docs/security/osl-audit-2026-07-26-codex.md, the second High finding).
///
/// These run against a real SQLite database with the real migrations applied,
/// so the conditional-INSERT admission predicates are genuinely exercised. The
/// bounds below are written as literals rather than imported from
/// `src/lib/attachment-limits.ts` on purpose: this file pins the *promised
/// behaviour*, so loosening a constant has to fail here rather than silently
/// travel with the import.

import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleAttachmentComplete,
  handleAttachmentPartUpload,
  handleAttachmentSessionCreate,
  handleAttachmentUpload,
} from "../src/endpoints/attachment.js";
import { memoryR2, migratedD1 } from "./helpers/d1.js";

const HALF_GIB = 512 * 1024 * 1024;
const SEVEN_DAYS = 604800;
/// The longest an *incomplete* session may hold reserved capacity.
const MAX_INCOMPLETE_HOLD_SECONDS = 15 * 60;

function randomToken(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  let hex = "";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

function harness() {
  const db = migratedD1();
  const r2 = memoryR2();
  const env = {
    DB: db.d1,
    ATTACHMENTS: r2.bucket,
    RATE_LIMIT: {} as KVNamespace,
    RATE_LIMIT_HASH_KEY: "s".repeat(48),
  } as Env;
  return { db, r2, env };
}

function sessionRequest(sizeBytes: number, token = randomToken(), ttl = SEVEN_DAYS): Request {
  return new Request("https://cipher.test/v1/attachment/session", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": String(ttl),
      "x-osl-fetch-token": token,
      "x-osl-size-bytes": String(sizeBytes),
      "content-length": "0",
    },
  });
}

function directRequest(bytes: Uint8Array): Request {
  return new Request("https://cipher.test/v1/attachment", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": String(SEVEN_DAYS),
      "x-osl-fetch-token": randomToken(),
      "content-length": String(bytes.byteLength),
    },
    body: bytes,
  });
}

describe("attachment session admission (HIGH-1)", () => {
  it("bodyless multipart reservations cannot deny service to real uploads", async () => {
    const { db, env } = harness();

    // The audit's exact attack: sixteen POSTs, each declaring 512 MiB and a
    // seven-day TTL, with no part body ever uploaded.
    let admitted = 0;
    for (let attempt = 0; attempt < 16; attempt++) {
      const response = await handleAttachmentSessionCreate(sessionRequest(HALF_GIB), env);
      if (response.status === 201) admitted++;
    }

    // Whatever the reservation policy admits, an ordinary small attachment from
    // an unrelated caller must still be storable. Before the fix this is a
    // `503 storage_capacity`, and stays one for seven days.
    const direct = await handleAttachmentUpload(directRequest(new Uint8Array([1, 2, 3, 4])), env);
    expect(direct.status).toBe(201);

    // The reserved bytes must not have swallowed the global budget.
    const reserved = db.count(
      "SELECT COALESCE(SUM(size_bytes), 0) FROM attachment_objects WHERE state <> 'ready'",
    );
    expect(reserved).toBeLessThan(8 * 1024 * 1024 * 1024);
    expect(admitted).toBeLessThanOrEqual(16);
  });

  it("holds an unfinished reservation for minutes, not for the content TTL", async () => {
    const { db, env } = harness();

    const created = await handleAttachmentSessionCreate(sessionRequest(HALF_GIB), env);
    expect(created.status).toBe(201);

    const row = db.raw
      .prepare("SELECT created_at, expires_at FROM attachment_objects LIMIT 1")
      .get() as { created_at: number; expires_at: number };

    // Before the fix this is 604800 — one bodyless request parks half a gigabyte
    // of the global budget for a week.
    expect(row.expires_at - row.created_at).toBeLessThanOrEqual(MAX_INCOMPLETE_HOLD_SECONDS);
  });

  it("starts the caller's content TTL only once the upload completes", async () => {
    const { db, env } = harness();
    const token = randomToken();
    const declared = 1024;

    const createdResponse = await handleAttachmentSessionCreate(
      sessionRequest(declared, token),
      env,
    );
    expect(createdResponse.status).toBe(201);
    const session = (await createdResponse.json()) as { id: string; expires_at: number };

    const part = await handleAttachmentPartUpload(
      new Request(`https://cipher.test/v1/attachment/${session.id}/part/1`, {
        method: "PUT",
        headers: { "x-osl-fetch-token": token, "content-length": String(declared) },
        body: new Uint8Array(declared).fill(7),
      }),
      env,
      session.id,
      1,
    );
    expect(part.status).toBe(201);

    const completedResponse = await handleAttachmentComplete(
      new Request(`https://cipher.test/v1/attachment/${session.id}/complete`, {
        method: "POST",
        headers: { "x-osl-fetch-token": token, "content-length": "0" },
      }),
      env,
      session.id,
    );
    expect(completedResponse.status).toBe(201);
    const completed = (await completedResponse.json()) as { id: string; expires_at: number };

    // Wire contract with the shipping Rust client
    // (crates/ipc/src/cipher_store_client.rs): the completion receipt must
    // report exactly the expiry the session receipt promised, or the client
    // treats the upload as a mismatch and deletes it.
    expect(completed.expires_at).toBe(session.expires_at);

    const row = db.raw
      .prepare("SELECT created_at, expires_at, state FROM attachment_objects WHERE id = ?")
      .get(session.id) as { created_at: number; expires_at: number; state: string };
    expect(row.state).toBe("ready");
    // ...and only now does the row actually hold the seven-day content TTL.
    expect(row.expires_at - row.created_at).toBeGreaterThan(MAX_INCOMPLETE_HOLD_SECONDS);
    expect(row.expires_at).toBe(completed.expires_at);
  });

  it("reclaims an abandoned reservation without touching completed content", async () => {
    const { db, env } = harness();
    const token = randomToken();

    const createdResponse = await handleAttachmentSessionCreate(sessionRequest(HALF_GIB, token), env);
    expect(createdResponse.status).toBe(201);
    const session = (await createdResponse.json()) as { id: string };

    // Fast-forward past the incomplete-session deadline.
    db.exec(
      "UPDATE attachment_objects SET expires_at = ? WHERE id = ?",
      Math.floor(Date.now() / 1000) - 1,
      session.id,
    );

    const { sweepExpiredAttachments } = await import("../src/lib/sweep.js");
    const swept = await sweepExpiredAttachments(env);
    expect(swept).toBe(1);
    expect(db.count("SELECT COUNT(*) FROM attachment_objects")).toBe(0);
  });
});
