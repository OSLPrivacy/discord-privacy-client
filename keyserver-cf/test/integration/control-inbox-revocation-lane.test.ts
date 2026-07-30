import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { canonicalControlInboxPostBytes } from "../../src/lib/canonical.js";
import { base64Encode, registerTestUser, signEd25519 } from "./helpers.js";

/**
 * The bilateral-burn revocation lane.
 *
 * The defect these tests pin down: `evictOldestPending` silently DELETEs the
 * oldest undelivered rows at the per-pair cap of 32, so a burn queued to an
 * offline peer used to be destroyed by the sender's own next 32 messages to that
 * same peer -- with no error and no notice to either side. A burn must never fail
 * silently, so revocation rows are never evictable; the POST is refused (507)
 * instead, and the lane collapses a retry for the same (scope, epoch).
 *
 * NOT DEPLOYED. See migration 0027.
 */

let seq = 0;
const userId = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${seq++}`;

const hex64 = (n: number) => n.toString(16).padStart(64, "0");

async function signedPost(args: {
  senderId: string;
  recipientId: string;
  signingKey: CryptoKey;
  scopeId?: string;
  kind?: string;
  collapseKey?: string;
  /** Sign these values instead, to model tampering in transit. */
  signAs?: { kind?: string; collapseKey?: string };
  bundleLabel?: string;
  timestampMs?: number;
}): Promise<Record<string, unknown>> {
  const timestampMs = args.timestampMs ?? Date.now();
  const scopeId = args.scopeId ?? `scope-${seq++}`;
  const bundle = new TextEncoder().encode(
    args.bundleLabel ?? `bundle-${seq++}`,
  );
  const bundleHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bundle),
  );
  // When `signAs` is present it REPLACES the signed values outright, including
  // with `undefined`; a `??` fallback here would silently make "signed as a
  // revocation, presented as ordinary" impossible to express.
  const signedKind = args.signAs ? args.signAs.kind : args.kind;
  const signedCollapse = args.signAs ? args.signAs.collapseKey : args.collapseKey;
  const signature_b64 = await signEd25519(
    args.signingKey,
    canonicalControlInboxPostBytes({
      sender_id: args.senderId,
      recipient_id: args.recipientId,
      scope_id: scopeId,
      timestamp_ms: timestampMs,
      bundle_sha256: bundleHash,
      kind: signedKind,
      collapse_key: signedCollapse,
    }),
  );
  const body: Record<string, unknown> = {
    sender_id: args.senderId,
    recipient_id: args.recipientId,
    scope_id: scopeId,
    timestamp_ms: timestampMs,
    bundle_b64: base64Encode(bundle),
    signature_b64,
  };
  if (args.kind !== undefined) body.kind = args.kind;
  if (args.collapseKey !== undefined) body.collapse_key = args.collapseKey;
  return body;
}

async function post(body: Record<string, unknown>): Promise<Response> {
  return SELF.fetch("http://test/v1/control-inbox", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function laneCount(recipientId: string, kind: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM control_inbox
      WHERE recipient_id = ? AND kind = ?`,
  )
    .bind(recipientId, kind)
    .first<{ count: number }>();
  return row?.count ?? 0;
}

async function pair() {
  const senderId = userId("burn-sender");
  const recipientId = userId("burn-recipient");
  const sender = await registerTestUser(SELF, senderId);
  const recipient = await registerTestUser(SELF, recipientId);
  return {
    senderId,
    recipientId,
    signingKey: sender.signingKey,
    // Registering the same user_id twice with a fresh key is a 403 rotation
    // refusal, so a drain must reuse the key from this registration.
    recipientSigningKey: recipient.signingKey,
  };
}

describe("control-inbox revocation lane", () => {
  it("accepts a revocation and records it in its own lane", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const res = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: hex64(1),
      }),
    );
    expect(res.status).toBe(201);
    expect(await laneCount(recipientId, "revocation")).toBe(1);
    expect(await laneCount(recipientId, "")).toBe(0);
  });

  it("keeps the ordinary lane byte-identical for a client that sends no kind", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const res = await post(
      await signedPost({ senderId, recipientId, signingKey }),
    );
    expect(res.status).toBe(201);
    expect(await laneCount(recipientId, "")).toBe(1);
    expect(await laneCount(recipientId, "revocation")).toBe(0);
  });

  it("refuses a revocation whose lane label was stripped in transit", async () => {
    // Signed as a revocation, presented as ordinary traffic. If this were
    // accepted, an attacker could demote every burn into the evictable lane and
    // the sender would never know.
    const { senderId, recipientId, signingKey } = await pair();
    const res = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        signAs: { kind: "revocation", collapseKey: hex64(2) },
      }),
    );
    expect(res.status).toBe(401);
    expect(await laneCount(recipientId, "")).toBe(0);
  });

  it("refuses a revocation label added to a request that did not sign it", async () => {
    // The mirror image: an ordinary message promoted into the non-evictable lane.
    const { senderId, recipientId, signingKey } = await pair();
    const res = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: hex64(3),
        signAs: { kind: undefined, collapseKey: undefined },
      }),
    );
    expect(res.status).toBe(401);
    expect(await laneCount(recipientId, "revocation")).toBe(0);
  });

  it("refuses an unrecognised lane rather than defaulting to one", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const res = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "urgent",
        collapseKey: hex64(4),
      }),
    );
    expect(res.status).toBe(400);
  });

  it("requires a collapse key for a revocation and forbids one otherwise", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const missing = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
      }),
    );
    expect(missing.status).toBe(400);

    const stray = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        collapseKey: hex64(5),
      }),
    );
    expect(stray.status).toBe(400);

    const malformed = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: "NOT-HEX",
      }),
    );
    expect(malformed.status).toBe(400);
  });

  it("collapses a second burn for the same scope and epoch instead of appending", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const scopeId = `scope-collapse-${seq++}`;
    const collapseKey = hex64(6);
    const first = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        scopeId,
        kind: "revocation",
        collapseKey,
        bundleLabel: "burn-v1",
      }),
    );
    expect(first.status).toBe(201);
    const second = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        scopeId,
        kind: "revocation",
        collapseKey,
        bundleLabel: "burn-v2",
      }),
    );
    expect(second.status).toBe(201);
    expect(await laneCount(recipientId, "revocation")).toBe(1);

    // The surviving row carries the newer bundle: both assert the same epoch, so
    // the later one supersedes.
    const row = await env.DB.prepare(
      `SELECT bundle FROM control_inbox
        WHERE recipient_id = ? AND collapse_key = ?`,
    )
      .bind(recipientId, collapseKey)
      .first<{ bundle: unknown }>();
    const bytes = Array.isArray(row?.bundle)
      ? Uint8Array.from(row!.bundle as number[])
      : new Uint8Array(row!.bundle as ArrayBuffer);
    expect(new TextDecoder().decode(bytes)).toBe("burn-v2");

    // A different epoch is a different burn and appends.
    const other = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        scopeId,
        kind: "revocation",
        collapseKey: hex64(7),
        bundleLabel: "burn-next-epoch",
      }),
    );
    expect(other.status).toBe(201);
    expect(await laneCount(recipientId, "revocation")).toBe(2);
  });

  it("refuses with 507 when the per-pair revocation lane is full, and evicts nothing", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    for (let i = 0; i < 8; i++) {
      const res = await post(
        await signedPost({
          senderId,
          recipientId,
          signingKey,
          kind: "revocation",
          collapseKey: hex64(100 + i),
        }),
      );
      expect(res.status).toBe(201);
    }
    expect(await laneCount(recipientId, "revocation")).toBe(8);

    const overflow = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: hex64(999),
      }),
    );
    expect(overflow.status).toBe(507);
    const payload = (await overflow.json()) as Record<string, unknown>;
    expect(payload.error).toBe("revocation_lane_full");
    expect(payload.scope).toBe("sender_recipient");
    expect(overflow.headers.get("retry-after")).toBeTruthy();

    // The refusal is the whole point: nothing was dropped to make room.
    expect(await laneCount(recipientId, "revocation")).toBe(8);
  });

  it("still collapses onto an existing row when the lane is otherwise full", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    const scopeId = `scope-full-collapse-${seq++}`;
    const collapseKey = hex64(200);
    expect(
      (
        await post(
          await signedPost({
            senderId,
            recipientId,
            signingKey,
            scopeId,
            kind: "revocation",
            collapseKey,
            bundleLabel: "first",
          }),
        )
      ).status,
    ).toBe(201);
    for (let i = 1; i < 8; i++) {
      expect(
        (
          await post(
            await signedPost({
              senderId,
              recipientId,
              signingKey,
              kind: "revocation",
              collapseKey: hex64(200 + i),
            }),
          )
        ).status,
      ).toBe(201);
    }
    expect(await laneCount(recipientId, "revocation")).toBe(8);
    // A retry of an already-queued burn must never be refused for want of room:
    // it needs none.
    const retry = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        scopeId,
        kind: "revocation",
        collapseKey,
        bundleLabel: "retried",
      }),
    );
    expect(retry.status).toBe(201);
    expect(await laneCount(recipientId, "revocation")).toBe(8);
  });

  it("never evicts a queued revocation to make room for ordinary traffic", async () => {
    // This is the exact scenario that used to lose a burn: a revocation queued to
    // an offline peer, followed by the sender's own next 32+ messages.
    const { senderId, recipientId, signingKey } = await pair();
    const burn = await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: hex64(300),
        bundleLabel: "the-burn",
      }),
    );
    expect(burn.status).toBe(201);

    for (let i = 0; i < 40; i++) {
      const res = await post(
        await signedPost({
          senderId,
          recipientId,
          signingKey,
          bundleLabel: `ordinary-${i}`,
        }),
      );
      expect(res.status).toBe(201);
    }

    // The ordinary lane is capped and evicting, exactly as before.
    expect(await laneCount(recipientId, "")).toBeLessThanOrEqual(32);
    // The burn survived all of it.
    expect(await laneCount(recipientId, "revocation")).toBe(1);
    const row = await env.DB.prepare(
      `SELECT bundle FROM control_inbox
        WHERE recipient_id = ? AND kind = 'revocation'`,
    )
      .bind(recipientId)
      .first<{ bundle: unknown }>();
    const bytes = Array.isArray(row?.bundle)
      ? Uint8Array.from(row!.bundle as number[])
      : new Uint8Array(row!.bundle as ArrayBuffer);
    expect(new TextDecoder().decode(bytes)).toBe("the-burn");
  }, 20_000);

  it("does not let queued burns reduce an ordinary conversation's headroom", async () => {
    const { senderId, recipientId, signingKey } = await pair();
    for (let i = 0; i < 8; i++) {
      expect(
        (
          await post(
            await signedPost({
              senderId,
              recipientId,
              signingKey,
              kind: "revocation",
              collapseKey: hex64(400 + i),
            }),
          )
        ).status,
      ).toBe(201);
    }
    // A full revocation lane must not make ordinary sends fail or be counted
    // against.
    for (let i = 0; i < 32; i++) {
      expect(
        (
          await post(
            await signedPost({
              senderId,
              recipientId,
              signingKey,
              bundleLabel: `content-${i}`,
            }),
          )
        ).status,
      ).toBe(201);
    }
    expect(await laneCount(recipientId, "")).toBe(32);
    expect(await laneCount(recipientId, "revocation")).toBe(8);
  }, 20_000);

  it("reports the lane on the drain so a client can route without decrypting", async () => {
    const { senderId, recipientId, signingKey, recipientSigningKey } =
      await pair();
    await post(
      await signedPost({
        senderId,
        recipientId,
        signingKey,
        kind: "revocation",
        collapseKey: hex64(500),
      }),
    );
    await post(await signedPost({ senderId, recipientId, signingKey }));

    const ts = Date.now();
    const sig = await signEd25519(
      recipientSigningKey,
      (await import("../../src/lib/canonical.js")).canonicalControlInboxGetBytes(
        { user_id: recipientId, timestamp_ms: ts },
      ),
    );
    const res = await SELF.fetch(
      `http://test/v1/control-inbox/${recipientId}?ts=${ts}&sig=${encodeURIComponent(sig)}`,
    );
    expect(res.status).toBe(200);
    const body = (await res.json()) as { items: Array<{ kind: string }> };
    const kinds = body.items.map((item) => item.kind).sort();
    expect(kinds).toEqual(["", "revocation"]);
  });
});
