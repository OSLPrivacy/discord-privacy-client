import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  canonicalSenderFilterFloorGetBytes,
} from "../../src/lib/canonical.js";
import {
  registerTestUser,
  signEd25519,
} from "./helpers.js";

let sequence = 0;

function userId(label: string): string {
  return `floor-${label}-${Date.now().toString(36)}-${sequence++}`;
}

async function signedFloorUrl(
  recipientId: string,
  signingKey: CryptoKey,
  options: {
    requestId?: string;
    timestampMs?: number;
    signedRequestId?: string;
  } = {},
): Promise<string> {
  const timestampMs = options.timestampMs ?? Date.now();
  const requestId = options.requestId ?? "A".repeat(43);
  const signedRequestId = options.signedRequestId ?? requestId;
  const signature = await signEd25519(
    signingKey,
    canonicalSenderFilterFloorGetBytes({
      user_id: recipientId,
      timestamp_ms: timestampMs,
      request_id: signedRequestId,
    }),
  );
  return (
    `http://test/v1/sender-filter-capability-floor/${encodeURIComponent(recipientId)}` +
    `?ts=${timestampMs}&request_id=${encodeURIComponent(requestId)}` +
    `&sig=${encodeURIComponent(signature)}`
  );
}

describe("D1-backed sender-filter capability floor", () => {
  it("implement sender_filter_capability_floor (migration 0032) worker write", async () => {
    const recipientId = userId("migration0032");
    const identity = await registerTestUser(SELF, recipientId);
    const timestampMs = Date.now();
    const requestId = "W".repeat(43);

    const response = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId,
        timestampMs,
      }),
    );
    expect(response.status, await response.clone().text()).toBe(200);
    const body = await response.json() as {
      identity_anchor_sha256: string;
      capability_version: number;
      monotonic_version: number;
      first_observed_at_ms: number;
      request_timestamp_ms: number;
      request_id: string;
    };
    expect(body).toMatchObject({
      capability_version: 1,
      monotonic_version: 1,
      first_observed_at_ms: timestampMs,
      request_timestamp_ms: timestampMs,
      request_id: requestId,
    });
    expect(body.identity_anchor_sha256).toMatch(/^[0-9a-f]{64}$/);

    const stored = await env.DB.prepare(
      `SELECT identity_anchor_sha256,
              capability_version,
              monotonic_version,
              first_observed_at_ms
         FROM sender_filter_capability_floors
        WHERE identity_anchor_sha256 = ?`,
    ).bind(body.identity_anchor_sha256).first<{
      identity_anchor_sha256: string;
      capability_version: number;
      monotonic_version: number;
      first_observed_at_ms: number;
    }>();
    expect(stored).toEqual({
      identity_anchor_sha256: body.identity_anchor_sha256,
      capability_version: 1,
      monotonic_version: 1,
      first_observed_at_ms: timestampMs,
    });
  });

  it("test/integration/sender-filter-capability-floor.test.ts", async () => {
    const recipientId = userId("exact-name");
    const identity = await registerTestUser(SELF, recipientId);
    const firstRequestId = "G".repeat(43);
    const first = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: firstRequestId,
      }),
    );
    expect(first.status, await first.clone().text()).toBe(200);
    const firstBody = await first.json() as Record<string, unknown>;
    expect(firstBody).toMatchObject({
      format: "osl.keyserver.sender-filter-capability-floor.v3",
      recipient_user_id: recipientId,
      capability_version: 1,
      monotonic_version: 1,
      request_id: firstRequestId,
    });
    expect(firstBody.identity_anchor_sha256).toMatch(/^[0-9a-f]{64}$/);

    const row = await env.DB.prepare(
      `SELECT capability_version, monotonic_version, first_observed_at_ms
         FROM sender_filter_capability_floors
        WHERE identity_anchor_sha256 = ?`,
    ).bind(firstBody.identity_anchor_sha256).first<{
      capability_version: number;
      monotonic_version: number;
      first_observed_at_ms: number;
    }>();
    expect(row).toEqual({
      capability_version: 1,
      monotonic_version: 1,
      first_observed_at_ms: firstBody.first_observed_at_ms,
    });

    const floorCountBeforeRefusal = await env.DB.prepare(
      `SELECT COUNT(*) AS count
         FROM sender_filter_capability_floors`,
    ).first<{ count: number }>();

    await env.DB.prepare(
      `DELETE FROM worker_schema_capabilities
        WHERE capability = 'control_inbox_sender_disposition'`,
    ).run();
    try {
      const refusedId = userId("no-authority");
      const refusedIdentity = await registerTestUser(SELF, refusedId);
      const refused = await SELF.fetch(
        await signedFloorUrl(refusedId, refusedIdentity.signingKey, {
          requestId: "H".repeat(43),
        }),
      );
      expect(refused.status).toBe(503);
      expect(await env.DB.prepare(
        `SELECT COUNT(*) AS count
           FROM sender_filter_capability_floors`,
      ).first<{ count: number }>()).toEqual(floorCountBeforeRefusal);
    } finally {
      await env.DB.prepare(
        `INSERT INTO worker_schema_capabilities (capability, version)
         VALUES ('control_inbox_sender_disposition', 1)`,
      ).run();
    }
  });

  it("creates a nonempty authority record and preserves it across duplicate requests", async () => {
    const recipientId = userId("positive");
    const identity = await registerTestUser(SELF, recipientId);
    const firstRequestId = "A".repeat(43);
    const first = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: firstRequestId,
      }),
    );
    expect(first.status).toBe(200);
    const firstBody = await first.json() as Record<string, unknown>;
    expect(firstBody).toMatchObject({
      format: "osl.keyserver.sender-filter-capability-floor.v3",
      recipient_user_id: recipientId,
      capability_version: 1,
      monotonic_version: 1,
      request_id: firstRequestId,
    });
    expect(firstBody.identity_anchor_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(firstBody.first_observed_at_ms).toEqual(
      firstBody.request_timestamp_ms,
    );

    const secondRequestId = "B".repeat(43);
    const second = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: secondRequestId,
      }),
    );
    expect(second.status).toBe(200);
    const secondBody = await second.json() as Record<string, unknown>;
    expect(secondBody).toMatchObject({
      identity_anchor_sha256: firstBody.identity_anchor_sha256,
      capability_version: 1,
      monotonic_version: 1,
      first_observed_at_ms: firstBody.first_observed_at_ms,
      request_id: secondRequestId,
    });

    const count = await env.DB.prepare(
      `SELECT COUNT(*) AS count
         FROM sender_filter_capability_floors
        WHERE identity_anchor_sha256 = ?`,
    ).bind(firstBody.identity_anchor_sha256).first<{ count: number }>();
    expect(count?.count).toBe(1);
  });

  it("refuses stale, stripped, mismatched, and replay-bound requests", async () => {
    const recipientId = userId("negative");
    const identity = await registerTestUser(SELF, recipientId);
    const stale = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        timestampMs: Date.now() - 10 * 60 * 1000,
      }),
    );
    expect(stale.status).toBe(400);

    const stripped = new URL(
      await signedFloorUrl(recipientId, identity.signingKey),
    );
    stripped.searchParams.delete("sig");
    expect((await SELF.fetch(stripped)).status).toBe(400);

    const mismatch = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: "C".repeat(43),
        signedRequestId: "D".repeat(43),
      }),
    );
    expect(mismatch.status).toBe(401);

    const replayed = new URL(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: "E".repeat(43),
      }),
    );
    replayed.searchParams.set("request_id", "F".repeat(43));
    expect((await SELF.fetch(replayed)).status).toBe(401);
  });

  it("database guards survive delete/tamper attempts and a stateless restart", async () => {
    const recipientId = userId("durable");
    const identity = await registerTestUser(SELF, recipientId);
    const first = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey),
    );
    expect(first.status).toBe(200);
    const firstBody = await first.json() as {
      identity_anchor_sha256: string;
      first_observed_at_ms: number;
    };

    await expect(
      env.DB.prepare(
        `DELETE FROM sender_filter_capability_floors
          WHERE identity_anchor_sha256 = ?`,
      ).bind(firstBody.identity_anchor_sha256).run(),
    ).rejects.toThrow(/cannot be deleted/);
    await expect(
      env.DB.prepare(
        `UPDATE sender_filter_capability_floors
            SET capability_version = 0
          WHERE identity_anchor_sha256 = ?`,
      ).bind(firstBody.identity_anchor_sha256).run(),
    ).rejects.toThrow();

    // A new request models a fresh Worker/client process. No in-memory or
    // caller-file bit participates; D1 returns the original monotonic record.
    const afterRestart = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: "R".repeat(43),
      }),
    );
    expect(afterRestart.status).toBe(200);
    expect(await afterRestart.json()).toMatchObject({
      identity_anchor_sha256: firstBody.identity_anchor_sha256,
      capability_version: 1,
      monotonic_version: 1,
      first_observed_at_ms: firstBody.first_observed_at_ms,
      request_id: "R".repeat(43),
    });
  });

  it("rechecks live migration authority instead of accepting cached readiness", async () => {
    const recipientId = userId("marker");
    const identity = await registerTestUser(SELF, recipientId);
    expect(
      (await SELF.fetch(
        await signedFloorUrl(recipientId, identity.signingKey),
      )).status,
    ).toBe(200);

    await env.DB.prepare(
      `DELETE FROM worker_schema_capabilities
        WHERE capability = 'control_inbox_sender_disposition'`,
    ).run();
    const absent = await SELF.fetch(
      await signedFloorUrl(recipientId, identity.signingKey, {
        requestId: "M".repeat(43),
      }),
    );
    expect(absent.status).toBe(503);

    await env.DB.prepare(
      `INSERT INTO worker_schema_capabilities (capability, version)
       VALUES ('control_inbox_sender_disposition', 1)`,
    ).run();
    expect(
      (await SELF.fetch(
        await signedFloorUrl(recipientId, identity.signingKey, {
          requestId: "N".repeat(43),
        }),
      )).status,
    ).toBe(200);
  });
});
