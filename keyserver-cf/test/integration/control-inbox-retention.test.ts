import { env, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  canonicalControlInboxGetBytes,
  canonicalControlInboxPostBytes,
} from "../../src/lib/canonical.js";
import {
  CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
  CONTROL_INBOX_DISABLED_RETRY_SECONDS,
  reconcileControlInboxSenderStates,
  sweepExpiredControlInboxRows,
} from "../../src/lib/control-inbox-sweep.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

let sequence = 0;
const uid = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${sequence++}`;

interface DeliveryRow {
  bundle: unknown;
  expires_at: number;
  delivery_status: string;
  delivery_reason: string | null;
  delivery_attempts: number;
  sender_disabled_first_seen_at: number | null;
  delivery_next_retry_at: number | null;
  delivery_retain_until: number | null;
}

function randomBytes(size: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(size));
}

function asBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  throw new Error("D1 did not return blob bytes");
}

async function insertRow(
  recipientId: string,
  senderId: string,
  bundle: Uint8Array,
  expiresAt: number,
  createdAt: number,
): Promise<Uint8Array> {
  const id = randomBytes(16);
  await env.DB.prepare(
    `INSERT INTO control_inbox
       (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
  ).bind(
    id,
    recipientId,
    senderId,
    `scope-${sequence++}`,
    bundle,
    expiresAt,
    createdAt,
  ).run();
  return id;
}

async function insertQuarantinedRow(
  recipientId: string,
  senderId: string,
  bundle: Uint8Array,
  expiresAt: number,
  createdAt: number,
  firstSeenAt: number,
  retainUntil: number,
): Promise<Uint8Array> {
  const id = randomBytes(16);
  await env.DB.prepare(
    `INSERT INTO control_inbox
       (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at,
        delivery_status, delivery_reason, delivery_attempts,
        sender_disabled_first_seen_at, delivery_next_retry_at,
        delivery_retain_until)
     VALUES (?, ?, ?, ?, ?, ?, ?, 'quarantined',
             'sender_lookup_retry_exhausted', 3, ?, NULL, ?)`,
  ).bind(
    id,
    recipientId,
    senderId,
    `quarantined-scope-${sequence++}`,
    bundle,
    expiresAt,
    createdAt,
    firstSeenAt,
    retainUntil,
  ).run();
  return id;
}

function hexToBytes(hex: string): Uint8Array {
  if (!/^[0-9a-f]{32}$/.test(hex)) throw new Error("invalid inbox id");
  const bytes = new Uint8Array(16);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

async function postAuthenticatedResponse(
  recipientId: string,
  senderId: string,
  senderSigningKey: CryptoKey,
  bundle: Uint8Array,
): Promise<Response> {
  const timestamp = Date.now();
  const bundleHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bundle),
  );
  const scopeId = `authenticated-scope-${sequence++}`;
  const signature = await signEd25519(
    senderSigningKey,
    canonicalControlInboxPostBytes({
      sender_id: senderId,
      recipient_id: recipientId,
      scope_id: scopeId,
      timestamp_ms: timestamp,
      bundle_sha256: bundleHash,
    }),
  );
  return SELF.fetch("http://test/v1/control-inbox", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      sender_id: senderId,
      recipient_id: recipientId,
      scope_id: scopeId,
      timestamp_ms: timestamp,
      bundle_b64: base64Encode(bundle),
      signature_b64: signature,
    }),
  });
}

async function postAuthenticatedRow(
  recipientId: string,
  senderId: string,
  senderSigningKey: CryptoKey,
  bundle: Uint8Array,
): Promise<{ id: Uint8Array; expiresAt: number }> {
  const response = await postAuthenticatedResponse(
    recipientId,
    senderId,
    senderSigningKey,
    bundle,
  );
  expect(response.status).toBe(201);
  const body = await response.json() as { id: string; expires_at: number };
  return { id: hexToBytes(body.id), expiresAt: body.expires_at };
}

async function deliveryRow(id: Uint8Array): Promise<DeliveryRow | null> {
  return env.DB.prepare(
    `SELECT bundle,
            expires_at,
            delivery_status,
            delivery_reason,
            delivery_attempts,
            sender_disabled_first_seen_at,
            delivery_next_retry_at,
            delivery_retain_until
       FROM control_inbox
      WHERE id = ?`,
  ).bind(id).first<DeliveryRow>();
}

async function filteredDrain(
  recipientId: string,
  recipientSigningKey: CryptoKey,
  senderId: string | null,
): Promise<{
  status: number;
  body: {
    items?: Array<Record<string, unknown>>;
    filtered_sender_delivery?: Record<string, unknown>;
  };
}> {
  const timestamp = Date.now();
  const signature = await signEd25519(
    recipientSigningKey,
    canonicalControlInboxGetBytes({
      user_id: recipientId,
      timestamp_ms: timestamp,
      sender_id: senderId,
    }),
  );
  let url =
    `http://test/v1/control-inbox/${encodeURIComponent(recipientId)}` +
    `?ts=${timestamp}&sig=${encodeURIComponent(signature)}`;
  if (senderId !== null) {
    url += `&sender=${encodeURIComponent(senderId)}`;
  }
  const response = await SELF.fetch(url);
  return {
    status: response.status,
    body: await response.json() as {
      items?: Array<Record<string, unknown>>;
      filtered_sender_delivery?: Record<string, unknown>;
    },
  };
}

async function disableLookup(userId: string): Promise<void> {
  await env.DB.prepare(
    "UPDATE users SET identity_lookup_enabled = 0 WHERE user_id = ?",
  ).bind(userId).run();
}

describe("control-inbox disabled-sender retention", () => {
  it("keeps an enabled sender live and exposes exact payload plus live state", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("live-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("live-sender");
    await registerTestUser(SELF, senderId);
    const payload = randomBytes(97);
    const id = await insertRow(
      recipientId,
      senderId,
      payload,
      now + 3600,
      now,
    );

    expect(await reconcileControlInboxSenderStates(env.DB, now)).toEqual({
      examined: 0,
      reenabled: 0,
      retryable: 0,
      quarantined: 0,
      retired: 0,
    });
    expect(asBytes((await deliveryRow(id))?.bundle)).toEqual(payload);

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      senderId,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items).toHaveLength(1);
    expect(drained.body.items?.[0]?.bundle_b64).toBe(base64Encode(payload));
    expect(drained.body.filtered_sender_delivery).toEqual({
      live: 1,
      retryable: 0,
      quarantined: 0,
      retired: 0,
    });
  });

  it("retries a disabled opaque sender on schedule, then quarantines without changing payload", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("retry-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("retry-sender");
    const sender = await registerTestUser(SELF, senderId);
    const payload = randomBytes(111);
    const posted = await postAuthenticatedRow(
      recipientId,
      senderId,
      sender.signingKey,
      payload,
    );
    const id = posted.id;
    await disableLookup(senderId);

    expect(await reconcileControlInboxSenderStates(env.DB, now)).toEqual({
      examined: 1,
      reenabled: 0,
      retryable: 1,
      quarantined: 0,
      retired: 0,
    });
    expect((await deliveryRow(id))?.delivery_attempts).toBe(1);

    // Calling early is not another attempt.
    expect(
      await reconcileControlInboxSenderStates(
        env.DB,
        now + CONTROL_INBOX_DISABLED_RETRY_SECONDS - 1,
      ),
    ).toEqual({
      examined: 0,
      reenabled: 0,
      retryable: 0,
      quarantined: 0,
      retired: 0,
    });

    expect(
      await reconcileControlInboxSenderStates(
        env.DB,
        now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
      ),
    ).toMatchObject({ retryable: 1, quarantined: 0 });
    expect(
      await reconcileControlInboxSenderStates(
        env.DB,
        now + 2 * CONTROL_INBOX_DISABLED_RETRY_SECONDS,
      ),
    ).toMatchObject({ retryable: 0, quarantined: 1 });

    const row = await deliveryRow(id);
    expect(row).toMatchObject({
      delivery_status: "quarantined",
      delivery_reason: "sender_lookup_retry_exhausted",
      delivery_attempts: 3,
      sender_disabled_first_seen_at: now,
      delivery_next_retry_at: null,
      delivery_retain_until:
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
    });
    expect(asBytes(row?.bundle)).toEqual(payload);

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      senderId,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items).toEqual([]);
    expect(drained.body.filtered_sender_delivery).toEqual({
      live: 0,
      retryable: 0,
      quarantined: 1,
      retired: 0,
    });
    const responseText = JSON.stringify(drained.body);
    expect(responseText).not.toContain("sender_lookup_retry_exhausted");
    expect(responseText).not.toContain(String(now));
    expect(responseText).not.toContain(base64Encode(payload));
  });

  it("restores a re-enabled sender and gives retained bytes a drain window", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("reenabled-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("reenabled-sender");
    await registerTestUser(SELF, senderId);
    await disableLookup(senderId);
    const payload = randomBytes(83);
    const id = await insertRow(
      recipientId,
      senderId,
      payload,
      now - 1,
      now - 120,
    );

    expect(
      (await reconcileControlInboxSenderStates(env.DB, now)).retryable,
    ).toBe(1);
    await env.DB.prepare(
      "UPDATE users SET identity_lookup_enabled = 1 WHERE user_id = ?",
    ).bind(senderId).run();
    expect(
      (await reconcileControlInboxSenderStates(env.DB, now + 1)).reenabled,
    ).toBe(1);

    const row = await deliveryRow(id);
    expect(row).toMatchObject({
      delivery_status: "live",
      delivery_reason: null,
      delivery_attempts: 0,
      sender_disabled_first_seen_at: null,
      delivery_next_retry_at: null,
      delivery_retain_until: null,
      expires_at: now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
    });
    expect(asBytes(row?.bundle)).toEqual(payload);

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      senderId,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items?.[0]?.bundle_b64).toBe(base64Encode(payload));
    expect(drained.body.filtered_sender_delivery).toEqual({
      live: 1,
      retryable: 0,
      quarantined: 0,
      retired: 0,
    });
  });

  it("retires a Discord snowflake immediately with a recorded reason", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("snowflake-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = "900000000000000001";
    const payload = randomBytes(71);
    const id = await insertRow(
      recipientId,
      senderId,
      payload,
      now + 60,
      now - 60,
    );

    expect(await reconcileControlInboxSenderStates(env.DB, now)).toMatchObject({
      examined: 1,
      retryable: 0,
      quarantined: 0,
      retired: 1,
    });
    const row = await deliveryRow(id);
    expect(row).toMatchObject({
      delivery_status: "retired",
      delivery_reason: "sender_discord_snowflake",
      delivery_attempts: 0,
      sender_disabled_first_seen_at: now,
    });
    expect(asBytes(row?.bundle)).toEqual(payload);

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      senderId,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items).toEqual([]);
    expect(drained.body.filtered_sender_delivery).toEqual({
      live: 0,
      retryable: 0,
      quarantined: 0,
      retired: 1,
    });
  });

  it("keeps a snowflake retired even if a legacy users row says lookup is enabled", async () => {
    const now = Math.floor(Date.now() / 1000);
    const senderId = "900000000000000002";
    await env.DB.prepare(
      `INSERT INTO users
         (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub,
          ik_x25519_signature, registered_at, identity_lookup_enabled)
       VALUES (?, 'x', 'ed', 'mlkem', 'sig',
               '2026-01-01T00:00:00Z', 1)`,
    ).bind(senderId).run();
    const id = await insertRow(
      uid("enabled-snowflake-recipient"),
      senderId,
      randomBytes(37),
      now + 60,
      now,
    );

    expect(await reconcileControlInboxSenderStates(env.DB, now)).toMatchObject({
      examined: 1,
      reenabled: 0,
      retired: 1,
    });
    expect((await deliveryRow(id))?.delivery_status).toBe("retired");
    expect(
      await reconcileControlInboxSenderStates(env.DB, now + 1),
    ).toMatchObject({ examined: 0, reenabled: 0, retired: 0 });
    await expect(
      env.DB.prepare(
        `UPDATE control_inbox
            SET delivery_status = 'live',
                delivery_reason = NULL,
                delivery_attempts = 0,
                sender_disabled_first_seen_at = NULL,
                delivery_next_retry_at = NULL,
                delivery_retain_until = NULL
          WHERE id = ?`,
      ).bind(id).run(),
    ).rejects.toThrow(/lookup|transition/);
    expect((await deliveryRow(id))?.delivery_status).toBe("retired");
  });

  it("bounds malformed senders through retry to quarantine and never widens an unfiltered drain", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("malformed-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = "malformed\nsender";
    const payload = randomBytes(63);
    const id = await insertRow(
      recipientId,
      senderId,
      payload,
      now + 60,
      now - 60,
    );

    await reconcileControlInboxSenderStates(env.DB, now);
    await reconcileControlInboxSenderStates(
      env.DB,
      now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
    );
    await reconcileControlInboxSenderStates(
      env.DB,
      now + 2 * CONTROL_INBOX_DISABLED_RETRY_SECONDS,
    );

    const row = await deliveryRow(id);
    expect(row).toMatchObject({
      delivery_status: "quarantined",
      delivery_reason: "sender_identifier_malformed",
      delivery_attempts: 3,
      sender_disabled_first_seen_at: now,
    });
    expect(asBytes(row?.bundle)).toEqual(payload);

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      null,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items).toEqual([]);
    expect(drained.body.filtered_sender_delivery).toBeUndefined();
  });

  it("reports mixed state as counts while returning only the live payload", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("mixed-recipient");
    const recipient = await registerTestUser(SELF, recipientId);
    const senderId = uid("mixed-sender");
    await registerTestUser(SELF, senderId);
    const livePayload = randomBytes(51);
    const hiddenPayload = randomBytes(53);
    await insertRow(
      recipientId,
      senderId,
      livePayload,
      now + 3600,
      now,
    );
    const hiddenId = await insertRow(
      recipientId,
      senderId,
      hiddenPayload,
      now + 3600,
      now + 1,
    );
    await disableLookup(senderId);
    await env.DB.prepare(
      `UPDATE control_inbox
          SET delivery_status = 'retryable',
              delivery_reason = 'sender_lookup_disabled',
              delivery_attempts = 1,
              sender_disabled_first_seen_at = ?,
              delivery_next_retry_at = ?,
              delivery_retain_until = ?
        WHERE id = ?`,
    ).bind(
      now,
      now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
      now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
      hiddenId,
    ).run();
    await env.DB.prepare(
      "UPDATE users SET identity_lookup_enabled = 1 WHERE user_id = ?",
    ).bind(senderId).run();

    const drained = await filteredDrain(
      recipientId,
      recipient.signingKey,
      senderId,
    );
    expect(drained.status).toBe(200);
    expect(drained.body.items).toHaveLength(1);
    expect(drained.body.items?.[0]?.bundle_b64).toBe(base64Encode(livePayload));
    expect(drained.body.filtered_sender_delivery).toEqual({
      live: 1,
      retryable: 1,
      quarantined: 0,
      retired: 0,
    });
    expect(JSON.stringify(drained.body)).not.toContain(base64Encode(hiddenPayload));
  });

  it("refuses at the physical quota without evicting retained bytes", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("quota-recipient");
    await registerTestUser(SELF, recipientId);
    const senderId = uid("quota-sender");
    const sender = await registerTestUser(SELF, senderId);
    const statements: D1PreparedStatement[] = [];
    for (let index = 0; index < 512; index += 1) {
      const heldSender =
        `held-quota-${sequence}-${Math.floor(index / 32)}`;
      statements.push(
        env.DB.prepare(
          `INSERT INTO control_inbox
             (id, recipient_id, sender_id, scope_id, bundle, expires_at,
              created_at, delivery_status, delivery_reason, delivery_attempts,
              sender_disabled_first_seen_at, delivery_next_retry_at,
              delivery_retain_until)
           VALUES (?, ?, ?, ?, ?, ?, ?, 'quarantined',
                   'sender_lookup_retry_exhausted', 3, ?, NULL, ?)`,
        ).bind(
          randomBytes(16),
          recipientId,
          heldSender,
          `held-${index}`,
          randomBytes(24),
          now + 3600,
          now - 120 + index,
          now - 60,
          now - 60 + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
        ),
      );
    }
    for (let offset = 0; offset < statements.length; offset += 80) {
      await env.DB.batch(statements.slice(offset, offset + 80));
    }

    const response = await postAuthenticatedResponse(
      recipientId,
      senderId,
      sender.signingKey,
      randomBytes(41),
    );
    expect(response.status).toBe(429);
    expect(await response.json()).toMatchObject({
      error: "recipient_inbox_full",
      scope: "recipient",
    });
    const counts = await env.DB.prepare(
      `SELECT COUNT(*) AS rows,
              SUM(delivery_status = 'live') AS live,
              SUM(delivery_status = 'quarantined') AS quarantined
         FROM control_inbox
        WHERE recipient_id = ?`,
    ).bind(recipientId).first<{
      rows: number;
      live: number;
      quarantined: number;
    }>();
    expect(counts).toEqual({ rows: 512, live: 0, quarantined: 512 });
  });

  it("refuses a physically full sender lane without deleting quarantine", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("pair-quota-recipient");
    await registerTestUser(SELF, recipientId);
    const senderId = uid("pair-quota-sender");
    const sender = await registerTestUser(SELF, senderId);
    await disableLookup(senderId);
    const payloads: Uint8Array[] = [];
    for (let index = 0; index < 32; index += 1) {
      const payload = randomBytes(29);
      payloads.push(payload);
      await insertQuarantinedRow(
        recipientId,
        senderId,
        payload,
        now + 3600,
        now + index,
        now,
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
      );
    }
    await env.DB.prepare(
      "UPDATE users SET identity_lookup_enabled = 1 WHERE user_id = ?",
    ).bind(senderId).run();

    const response = await postAuthenticatedResponse(
      recipientId,
      senderId,
      sender.signingKey,
      randomBytes(31),
    );
    expect(response.status).toBe(429);
    expect(await response.json()).toMatchObject({
      error: "recipient_inbox_full",
      scope: "sender_recipient",
    });
    const retained = await env.DB.prepare(
      `SELECT bundle FROM control_inbox
        WHERE recipient_id = ? AND sender_id = ?
        ORDER BY created_at`,
    ).bind(recipientId, senderId).all<{ bundle: unknown }>();
    expect(retained.results).toHaveLength(32);
    expect(
      retained.results?.map((row) => asBytes(row.bundle)),
    ).toEqual(payloads);
  });

  it("retains expired disabled bytes through quarantine, then deletes only after the recorded hold", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("hold-recipient");
    const senderId = uid("hold-sender");
    await registerTestUser(SELF, senderId);
    await disableLookup(senderId);
    const payload = randomBytes(79);
    const id = await insertRow(
      recipientId,
      senderId,
      payload,
      now - 1,
      now - 120,
    );

    const first = await sweepExpiredControlInboxRows(env.DB, now);
    expect(first.inboxRows).toBe(0);
    expect(first.senderStates.retryable).toBe(1);
    expect(asBytes((await deliveryRow(id))?.bundle)).toEqual(payload);

    await sweepExpiredControlInboxRows(
      env.DB,
      now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
    );
    await sweepExpiredControlInboxRows(
      env.DB,
      now + 2 * CONTROL_INBOX_DISABLED_RETRY_SECONDS,
    );
    expect((await deliveryRow(id))?.delivery_status).toBe("quarantined");

    const agedId = await insertQuarantinedRow(
      recipientId,
      senderId,
      randomBytes(43),
      now - 1,
      now - 120,
      1,
      1 + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
    );
    const final = await sweepExpiredControlInboxRows(
      env.DB,
      now + 2 * CONTROL_INBOX_DISABLED_RETRY_SECONDS + 1,
    );
    expect(final.inboxRows).toBeGreaterThanOrEqual(1);
    expect(final.inboxRows).toBeLessThanOrEqual(100);
    expect(await deliveryRow(agedId)).toBeNull();
    expect((await deliveryRow(id))?.delivery_status).toBe("quarantined");
  });

  it("advances an overdue retry before recording quarantine and cleanup", async () => {
    const now = Math.floor(Date.now() / 1000);
    const recipientId = uid("downtime-recipient");
    const senderId = uid("downtime-sender");
    await registerTestUser(SELF, senderId);
    await disableLookup(senderId);
    const firstSeen =
      now - CONTROL_INBOX_DISABLED_RETENTION_SECONDS - 3600;
    const retainUntil =
      firstSeen + CONTROL_INBOX_DISABLED_RETENTION_SECONDS;
    const id = randomBytes(16);
    const payload = randomBytes(47);
    await env.DB.prepare(
      `INSERT INTO control_inbox
         (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at,
          delivery_status, delivery_reason, delivery_attempts,
          sender_disabled_first_seen_at, delivery_next_retry_at,
          delivery_retain_until)
       VALUES (?, ?, ?, ?, ?, ?, ?, 'retryable',
               'sender_lookup_disabled', 1, ?, ?, ?)`,
    ).bind(
      id,
      recipientId,
      senderId,
      "downtime-scope",
      payload,
      now - 1,
      firstSeen,
      firstSeen,
      firstSeen + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
      retainUntil,
    ).run();

    const secondAttempt = await sweepExpiredControlInboxRows(env.DB, now);
    expect(secondAttempt.senderStates).toMatchObject({
      retryable: 1,
      quarantined: 0,
    });
    expect(secondAttempt.inboxRows).toBe(0);
    expect(await deliveryRow(id)).toMatchObject({
      delivery_status: "retryable",
      delivery_attempts: 2,
      delivery_retain_until: retainUntil,
    });
    expect(asBytes((await deliveryRow(id))?.bundle)).toEqual(payload);

    const quarantinedThenCleaned = await sweepExpiredControlInboxRows(
      env.DB,
      now + 1,
    );
    expect(quarantinedThenCleaned.senderStates.quarantined).toBe(1);
    expect(quarantinedThenCleaned.inboxRows).toBe(1);
    expect(await deliveryRow(id)).toBeNull();
  });

  it("refuses inconsistent status metadata at the D1 boundary", async () => {
    const now = Math.floor(Date.now() / 1000);
    const id = await insertRow(
      uid("guard-recipient"),
      uid("guard-sender"),
      randomBytes(32),
      now + 60,
      now,
    );
    await expect(
      env.DB.prepare(
        `UPDATE control_inbox
            SET delivery_status = 'quarantined',
                delivery_reason = NULL,
                delivery_attempts = 0
          WHERE id = ?`,
      ).bind(id).run(),
    ).rejects.toThrow(/delivery state is inconsistent/);
    expect((await deliveryRow(id))?.delivery_status).toBe("live");
  });

  it("binds legal transitions to lookup truth and the snowflake shape", async () => {
    const now = Math.floor(Date.now() / 1000);
    const senderId = uid("truth-sender");
    await registerTestUser(SELF, senderId);
    const id = await insertRow(
      uid("truth-recipient"),
      senderId,
      randomBytes(32),
      now + 60,
      now,
    );
    const retryUpdate = env.DB.prepare(
      `UPDATE control_inbox
          SET delivery_status = 'retryable',
              delivery_reason = 'sender_lookup_disabled',
              delivery_attempts = 1,
              sender_disabled_first_seen_at = ?,
              delivery_next_retry_at = ?,
              delivery_retain_until = ?
        WHERE id = ?`,
    );
    await expect(
      retryUpdate.bind(
        now,
        now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
        id,
      ).run(),
    ).rejects.toThrow(/lookup-enabled|contradicts lookup/);

    await disableLookup(senderId);
    await expect(
      env.DB.prepare(
        `UPDATE control_inbox
            SET delivery_status = 'quarantined',
                delivery_reason = 'sender_lookup_retry_exhausted',
                delivery_attempts = 3,
                sender_disabled_first_seen_at = ?,
                delivery_next_retry_at = NULL,
                delivery_retain_until = ?
          WHERE id = ?`,
      ).bind(
        now,
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
        id,
      ).run(),
    ).rejects.toThrow(/transition is invalid/);
    await retryUpdate.bind(
      now,
      now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
      now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
      id,
    ).run();
    await expect(
      env.DB.prepare(
        `UPDATE control_inbox
            SET delivery_retain_until = delivery_retain_until + 1
          WHERE id = ?`,
      ).bind(id).run(),
    ).rejects.toThrow(/delivery state is inconsistent/);
    await expect(
      env.DB.prepare(
        `UPDATE control_inbox
            SET delivery_status = 'retired',
                delivery_reason = 'sender_discord_snowflake',
                delivery_attempts = 0,
                sender_disabled_first_seen_at = ?,
                delivery_next_retry_at = NULL,
                delivery_retain_until = ?
          WHERE id = ?`,
      ).bind(
        now,
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
        id,
      ).run(),
    ).rejects.toThrow(/transition is invalid|delivery state is inconsistent/);

    const snowflakeId = await insertRow(
      uid("truth-snowflake-recipient"),
      "900000000000000001",
      randomBytes(32),
      now + 60,
      now,
    );
    await expect(
      retryUpdate.bind(
        now,
        now + CONTROL_INBOX_DISABLED_RETRY_SECONDS,
        now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
        snowflakeId,
      ).run(),
    ).rejects.toThrow(/delivery state is inconsistent/);
    await env.DB.prepare(
      `UPDATE control_inbox
          SET delivery_status = 'retired',
              delivery_reason = 'sender_discord_snowflake',
              delivery_attempts = 0,
              sender_disabled_first_seen_at = ?,
              delivery_next_retry_at = NULL,
              delivery_retain_until = ?
        WHERE id = ?`,
    ).bind(
      now,
      now + CONTROL_INBOX_DISABLED_RETENTION_SECONDS,
      snowflakeId,
    ).run();
    expect((await deliveryRow(snowflakeId))?.delivery_status).toBe("retired");
  });
});
