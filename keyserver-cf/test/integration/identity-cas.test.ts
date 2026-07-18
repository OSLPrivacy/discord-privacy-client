import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { canonicalUnregisterBytes } from "../../src/lib/canonical.js";
import {
  rotateUserKeys,
  unregisterUserIfCurrent,
  upsertPrekeyBundleAuthenticated,
} from "../../src/lib/db.js";
import {
  generateEd25519Pair,
  registerTestUser,
  signedRegisterBody,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_SIGNATURE_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

let seq = 0;
const userId = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${seq++}`;

describe("identity mutation compare-and-swap", () => {
  it("rotation updates only the identity key that was verified", async () => {
    const id = userId("rotate-cas");
    const owner = await registerTestUser(SELF, id);
    const next = await generateEd25519Pair();
    const wrongExpected = await generateEd25519Pair();
    const result = await rotateUserKeys(
      env.DB,
      {
        user_id: id,
        ik_x25519_pub: STUB_X25519_PUB_B64,
        ik_ed25519_pub: next.publicKeyB64,
        ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
        ik_x25519_signature: STUB_SIGNATURE_B64,
        ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
      },
      wrongExpected.publicKeyB64,
    );
    expect(result).toBeNull();
    const row = await env.DB.prepare(
      "SELECT ik_ed25519_pub FROM users WHERE user_id = ?",
    )
      .bind(id)
      .first<{ ik_ed25519_pub: string }>();
    expect(row?.ik_ed25519_pub).toBe(owner.publicKeyB64);
  });

  it("prekey replenish writes only under the identity key that was verified", async () => {
    const id = userId("replenish-cas");
    const owner = await registerTestUser(SELF, id);
    const replacement = await generateEd25519Pair();
    await env.DB.prepare("UPDATE users SET ik_ed25519_pub = ? WHERE user_id = ?")
      .bind(replacement.publicKeyB64, id)
      .run();

    const result = await upsertPrekeyBundleAuthenticated(
      env.DB,
      id,
      null,
      [{ id: 77, pub_b64: STUB_X25519_PUB_B64 }],
      new Uint8Array(32).fill(0x77),
      owner.publicKeyB64,
      Math.floor(Date.now() / 1000) + 600,
    );
    expect(result).toBe("stale_identity");
    const row = await env.DB
      .prepare("SELECT COUNT(*) AS count FROM opk_pool WHERE user_id = ?")
      .bind(id)
      .first<{ count: number }>();
    expect(row?.count).toBe(0);
  });

  it("unregister mismatch preserves both parent and child rows", async () => {
    const id = userId("unregister-cas");
    await registerTestUser(SELF, id);
    const wrongExpected = await generateEd25519Pair();
    await env.DB.prepare(
      `INSERT INTO wrapped_keys
         (content_id, content_type, sender_id, recipient_id, session_version,
          share_index, wrapped_share_blob, blob_version, single_use,
          expires_at, created_at)
       VALUES (?, 'text', ?, ?, 1, 0, 'blob', 1, 0, ?, ?)`,
    )
      .bind(
        userId("content"),
        id,
        id,
        new Date(Date.now() + 60_000).toISOString(),
        new Date().toISOString(),
      )
      .run();
    await env.DB.prepare(
      `INSERT INTO consuming_get_receipts
         (requester_id, request_digest, recipient_id, target_id, expires_at)
       VALUES (?, ?, 'other-recipient', 'target', ?)`,
    )
      .bind(
        id,
        new Uint8Array(32).fill(9),
        Math.floor(Date.now() / 1000) + 3600,
      )
      .run();

    expect(
      await unregisterUserIfCurrent(
        env.DB,
        id,
        wrongExpected.publicKeyB64,
        new Uint8Array(32).fill(0x41),
        Math.floor(Date.now() / 1000) + 600,
      ),
    ).toBe("stale_identity");
    const users = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM users WHERE user_id = ?",
    )
      .bind(id)
      .first<{ count: number }>();
    const wrapped = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM wrapped_keys WHERE sender_id = ?",
    )
      .bind(id)
      .first<{ count: number }>();
    const receipts = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM consuming_get_receipts WHERE requester_id = ?",
    )
      .bind(id)
      .first<{ count: number }>();
    expect(users?.count).toBe(1);
    expect(wrapped?.count).toBe(1);
    expect(receipts?.count).toBe(1);
  });
});

describe("DELETE /v1/pubkeys/:user_id", () => {
  it("rejects malformed base64 without an internal error", async () => {
    const id = userId("unregister-invalid");
    await registerTestUser(SELF, id);
    const res = await SELF.fetch(`http://test/v1/pubkeys/${id}`, {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ signature_b64: "A", timestamp_ms: Date.now() }),
    });
    expect(res.status).toBe(400);
  });

  it("deletes child rows before the user, including control inbox data", async () => {
    const id = userId("unregister");
    const owner = await registerTestUser(SELF, id);
    const now = Math.floor(Date.now() / 1000);
    await env.DB.batch([
      env.DB
        .prepare(
          `INSERT INTO prekey_bundles
             (user_id, spk_pub, spk_signature, spk_rotated_at)
           VALUES (?, 'spk', 'sig', 'now')`,
        )
        .bind(id),
      env.DB
        .prepare(
          "INSERT INTO opk_pool (user_id, opk_id, opk_pub) VALUES (?, 1, 'opk')",
        )
        .bind(id),
      env.DB
        .prepare(
          `INSERT INTO wrapped_keys
             (content_id, content_type, sender_id, recipient_id, session_version,
              share_index, wrapped_share_blob, blob_version, single_use,
              expires_at, created_at)
           VALUES (?, 'text', ?, ?, 1, 0, 'blob', 1, 0, ?, ?)`,
        )
        .bind(
          userId("content"),
          id,
          id,
          new Date(Date.now() + 60_000).toISOString(),
          new Date().toISOString(),
        ),
      env.DB
        .prepare(
          `INSERT INTO control_inbox
             (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
           VALUES (?, ?, ?, 'scope', ?, ?, ?)`,
        )
        .bind(
          new Uint8Array(16).fill(1),
          id,
          id,
          new Uint8Array([1]),
          now + 3600,
          now,
        ),
      env.DB
        .prepare(
          `INSERT INTO control_inbox_requests
             (sender_id, request_digest, inbox_id, recipient_id, expires_at)
           VALUES (?, ?, ?, ?, ?)`,
        )
        .bind(
          id,
          new Uint8Array(32).fill(2),
          new Uint8Array(16).fill(1),
          id,
          now + 3600,
        ),
      env.DB
        .prepare(
          `INSERT INTO consuming_get_receipts
             (requester_id, request_digest, recipient_id, target_id, expires_at)
           VALUES (?, ?, 'other-recipient', 'target-as-requester', ?)`,
        )
        .bind(id, new Uint8Array(32).fill(3), now + 3600),
      env.DB
        .prepare(
          `INSERT INTO consuming_get_receipts
             (requester_id, request_digest, recipient_id, target_id, expires_at)
           VALUES ('other-requester', ?, ?, 'target-as-recipient', ?)`,
        )
        .bind(new Uint8Array(32).fill(4), id, now + 3600),
      env.DB
        .prepare(
          `INSERT INTO consuming_get_receipts
             (requester_id, request_digest, recipient_id, target_id, expires_at)
           VALUES ('unrelated-requester', ?, 'unrelated-recipient', 'unrelated', ?)`,
        )
        .bind(new Uint8Array(32).fill(5), now + 3600),
    ]);

    const timestampMs = Date.now();
    const signature = await signEd25519(
      owner.signingKey,
      canonicalUnregisterBytes({ user_id: id, timestamp_ms: timestampMs }),
    );
    const res = await SELF.fetch(`http://test/v1/pubkeys/${id}`, {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        signature_b64: signature,
        timestamp_ms: timestampMs,
      }),
    });
    expect(res.status).toBe(200);

    for (const [table, where] of [
      ["users", "user_id"],
      ["prekey_bundles", "user_id"],
      ["opk_pool", "user_id"],
      ["wrapped_keys", "sender_id"],
      ["control_inbox", "sender_id"],
    ] as const) {
      const row = await env.DB.prepare(
        `SELECT COUNT(*) AS count FROM ${table} WHERE ${where} = ?`,
      )
        .bind(id)
        .first<{ count: number }>();
      expect(row?.count, table).toBe(0);
    }
    const retainedReplayReceipts = await env.DB.prepare(
      `SELECT
         (SELECT COUNT(*) FROM control_inbox_requests WHERE sender_id = ?) +
         (SELECT COUNT(*) FROM consuming_get_receipts
           WHERE requester_id = ? OR recipient_id = ?) AS count`,
    )
      .bind(id, id, id)
      .first<{ count: number }>();
    expect(retainedReplayReceipts?.count).toBe(3);
    const unrelatedReceipt = await env.DB.prepare(
      `SELECT COUNT(*) AS count FROM consuming_get_receipts
        WHERE requester_id = 'unrelated-requester'`,
    ).first<{ count: number }>();
    expect(unrelatedReceipt?.count).toBe(1);
  });

  it("rejects replay after the same identity re-registers", async () => {
    const id = userId("unregister-replay");
    const owner = await registerTestUser(SELF, id);
    const timestampMs = Date.now();
    const signature = await signEd25519(
      owner.signingKey,
      canonicalUnregisterBytes({ user_id: id, timestamp_ms: timestampMs }),
    );
    const request = () =>
      SELF.fetch(`http://test/v1/pubkeys/${id}`, {
        method: "DELETE",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          signature_b64: signature,
          timestamp_ms: timestampMs,
        }),
      });

    expect((await request()).status).toBe(200);
    const restored = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(await signedRegisterBody(id, owner)),
    });
    expect(restored.status).toBe(201);

    const replay = await request();
    expect(replay.status).toBe(409);
    expect(((await replay.json()) as { error: string }).error).toMatch(
      /already used/,
    );
    const stillRegistered = await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM users WHERE user_id = ?",
    )
      .bind(id)
      .first<{ count: number }>();
    expect(stillRegistered?.count).toBe(1);
  });
});
