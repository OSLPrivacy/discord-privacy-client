// D-273 / D-274 — the Space-event lane under owner decision D15.
//
// D15: "delete on ACKNOWLEDGED receipt, never on transmission." That rule was
// already live in this same worker for the wrapped-key lane
// (test/integration/wrapped-key-reservation.test.ts, src/lib/db.ts
// `fetchWrappedKeyAuthenticated`), and `handleSpaceEventDrain` did the exact
// opposite: it DELETEd every row it returned, before the response was
// serialized. A dropped response destroyed the only copy of a membership
// event and no retry could recover it.
//
// D-274: `expires_at` on `space_event_queue` was a read filter only. The
// `scheduled()` handler swept six other tables and nothing swept this one, so
// expired ciphertext was retained in storage while invisible to every reader.
//
// WHICH WAY THE DESIGN ERRS: toward DUPLICATE DELIVERY. An unacknowledged
// event becomes visible again at lease expiry and is delivered again. That is
// asserted here, deliberately, as a property and not tolerated as an accident.
//
// Every assertion reads the real `space_event_queue` rows out of the D1 the
// Worker just wrote to. None inspects source text.
import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../../src/index.js";
import type { Env } from "../../src/env.js";
import {
  LEASE_SECONDS,
  MAX_EVENT_LIFETIME_SECONDS,
} from "../../src/endpoints/space-events.js";
import {
  SPACE_EVENT_SWEEP_MAX_ROWS,
  sweepExpiredSpaceEvents,
} from "../../src/lib/space-event-sweep.js";

const TAG_BYTES = 32;
const testDb = (env as unknown as { DB: D1Database }).DB;
const workerEnv = env as unknown as Env;
const ctx = {} as ExecutionContext;

let tagCounter = 0x40;
/** A distinct 32-byte tag per spec — D1 is isolated per file, not per test. */
function freshTag(): string {
  tagCounter += 1;
  const bytes = new Uint8Array(TAG_BYTES).fill(tagCounter);
  return btoa(String.fromCharCode(...bytes));
}

function b64(fill: number, length: number): string {
  return btoa(String.fromCharCode(...new Uint8Array(length).fill(fill)));
}

function tagBytes(tagB64: string): Uint8Array {
  return Uint8Array.from(atob(tagB64), (c) => c.charCodeAt(0));
}

interface StoredEvent {
  id: Uint8Array;
  expires_at: number;
  lease_until: number;
}

async function storedFor(tagB64: string): Promise<StoredEvent[]> {
  const rows = await testDb
    .prepare(
      "SELECT id, expires_at, lease_until FROM space_event_queue WHERE recipient_tag = ? ORDER BY created_at",
    )
    .bind(tagBytes(tagB64))
    .all<StoredEvent>();
  return rows.results ?? [];
}

async function enqueue(
  tagB64: string,
  ciphertext: string,
  ttlSeconds = 3600,
): Promise<Response> {
  return await SELF.fetch("http://test/v1/space-events", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      recipient_tag: tagB64,
      ciphertext,
      expires_at: Math.floor(Date.now() / 1000) + ttlSeconds,
    }),
  });
}

/**
 * D-260/OPEN-4: the drain is `POST /v1/space-events/drain` with the tag in the
 * body. It is NOT a `GET`, and `GET /v1/space-events/:tag` no longer exists --
 * `drainRequest` is the only transport these specs have.
 */
async function drainRequest(tagB64: string): Promise<Response> {
  return await SELF.fetch("http://test/v1/space-events/drain", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ recipient_tag: tagB64 }),
  });
}

async function drain(
  tagB64: string,
): Promise<{ events: { event_id: string; ciphertext: string }[] }> {
  const res = await drainRequest(tagB64);
  expect(res.status).toBe(200);
  return (await res.json()) as {
    events: { event_id: string; ciphertext: string }[];
  };
}

async function ack(tagB64: string, eventIds: string[]): Promise<Response> {
  return await SELF.fetch("http://test/v1/space-events/ack", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ recipient_tag: tagB64, event_ids: eventIds }),
  });
}

/** Expire every live lease for a tag, as the clock would. */
async function expireLeases(tagB64: string): Promise<void> {
  await testDb
    .prepare(
      "UPDATE space_event_queue SET lease_until = 1 WHERE recipient_tag = ? AND lease_until > 0",
    )
    .bind(tagBytes(tagB64))
    .run();
}

function hourlyCron(): ScheduledController {
  return {
    cron: "0 * * * *",
    scheduledTime: Date.now(),
    noRetry() {},
  } as unknown as ScheduledController;
}

describe("D-273 the drain never destroys on transmission", () => {
  it("keeps the event when the response is dropped, and redelivers it", async () => {
    // THE SHIPPED DEFECT, stated as its consequence: before the fix the row
    // was gone the instant the drain returned, so this dropped response was
    // unrecoverable membership-event loss.
    const tag = freshTag();
    const ciphertext = b64(0x07, 48);
    expect((await enqueue(tag, ciphertext)).status).toBe(202);

    const res = await drainRequest(tag);
    expect(res.status).toBe(200);

    // The row is STILL THERE at the instant the response reaches the socket.
    const afterDrain = await storedFor(tag);
    expect(afterDrain).toHaveLength(1);
    expect(afterDrain[0]!.lease_until).toBeGreaterThan(0);

    // The response never arrives.
    await res.body?.cancel();

    // Recovery is real, not theoretical: once the lease lapses the same event
    // is delivered again, byte for byte.
    await expireLeases(tag);
    const retry = await drain(tag);
    expect(retry.events).toHaveLength(1);
    expect(retry.events[0]!.ciphertext).toBe(ciphertext);
    expect(await storedFor(tag)).toHaveLength(1);
  });

  it("consumes on the drain: a second call returns nothing (T21-C1)", async () => {
    // T21-C1 says the drain "returns and consumes". The lease is what keeps
    // that true while the destruction waits for an ack; the amended clause
    // changed the drain's METHOD, not this property.
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x08, 16))).status).toBe(202);

    expect((await drain(tag)).events).toHaveLength(1);
    expect((await drain(tag)).events).toHaveLength(0);
    // Consumed from the reader's view, NOT destroyed.
    expect(await storedFor(tag)).toHaveLength(1);
  });

  it("errs toward DUPLICATE delivery: an unacknowledged event comes back", async () => {
    const tag = freshTag();
    const ciphertext = b64(0x09, 24);
    expect((await enqueue(tag, ciphertext)).status).toBe(202);

    const first = await drain(tag);
    expect(first.events).toHaveLength(1);
    await expireLeases(tag);
    const second = await drain(tag);
    expect(second.events).toHaveLength(1);
    expect(second.events[0]!.ciphertext).toBe(ciphertext);
    expect(second.events[0]!.event_id).toBe(first.events[0]!.event_id);
    await expireLeases(tag);
    expect((await drain(tag)).events).toHaveLength(1);
  });

  it("deletes only on acknowledgement, and only what was acknowledged", async () => {
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x0a, 8))).status).toBe(202);
    expect((await enqueue(tag, b64(0x0b, 8))).status).toBe(202);

    const delivered = await drain(tag);
    expect(delivered.events).toHaveLength(2);
    expect(await storedFor(tag)).toHaveLength(2);

    const res = await ack(tag, [delivered.events[0]!.event_id]);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ acknowledged: true });

    // Exactly the acknowledged row left storage.
    const left = await storedFor(tag);
    expect(left).toHaveLength(1);
    const survivorId = Array.from(left[0]!.id, (b) =>
      b.toString(16).padStart(2, "0"),
    ).join("");
    expect(survivorId).toBe(delivered.events[1]!.event_id);

    // And the unacknowledged one is still deliverable.
    await expireLeases(tag);
    const again = await drain(tag);
    expect(again.events).toHaveLength(1);
    expect(again.events[0]!.event_id).toBe(delivered.events[1]!.event_id);
  });

  it("refuses an acknowledgement carrying a different recipient tag", async () => {
    // The tag is the whole capability. A leaked event id must not let anyone
    // else destroy the row — the same scoping the control-inbox delete uses.
    const tag = freshTag();
    const other = freshTag();
    expect((await enqueue(tag, b64(0x0c, 8))).status).toBe(202);
    const delivered = await drain(tag);
    expect(delivered.events).toHaveLength(1);

    const res = await ack(other, [delivered.events[0]!.event_id]);
    expect(res.status).toBe(200);
    expect(await storedFor(tag)).toHaveLength(1);
  });

  it("refuses to destroy an event that was never delivered", async () => {
    // `lease_until > 0` is the predicate: nothing can be acknowledged that the
    // relay never handed out, so an ack can never race a delivery into loss.
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x0d, 8))).status).toBe(202);
    const stored = await storedFor(tag);
    expect(stored).toHaveLength(1);
    expect(stored[0]!.lease_until).toBe(0);
    const undeliveredId = Array.from(stored[0]!.id, (b) =>
      b.toString(16).padStart(2, "0"),
    ).join("");

    const res = await ack(tag, [undeliveredId]);
    expect(res.status).toBe(200);
    expect(await storedFor(tag)).toHaveLength(1);
  });

  it("rejects a malformed acknowledgement with one indistinguishable 400", async () => {
    const tag = freshTag();
    const bad = [
      { recipient_tag: b64(0x01, 31), event_ids: ["00".repeat(16)] },
      { recipient_tag: tag, event_ids: [] },
      { recipient_tag: tag, event_ids: ["nothex".padEnd(32, "0")] },
      { recipient_tag: tag, event_ids: ["00".repeat(15)] },
      { recipient_tag: tag, event_ids: new Array(65).fill("00".repeat(16)) },
      { recipient_tag: tag, event_ids: "00".repeat(16) },
    ];
    for (const body of bad) {
      const res = await SELF.fetch("http://test/v1/space-events/ack", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      expect(res.status).toBe(400);
      expect(await res.json()).toEqual({
        error: "invalid Space event acknowledgement",
      });
    }
  });

  it("holds the lease at exactly LEASE_SECONDS ahead of the drain", async () => {
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x0e, 8))).status).toBe(202);
    const before = Math.floor(Date.now() / 1000);
    await drain(tag);
    const after = Math.floor(Date.now() / 1000);
    const stored = await storedFor(tag);
    expect(stored[0]!.lease_until).toBeGreaterThanOrEqual(before + LEASE_SECONDS);
    expect(stored[0]!.lease_until).toBeLessThanOrEqual(after + LEASE_SECONDS);
  });
});

describe("D-274 expired Space events leave storage", () => {
  async function insertExpired(tagB64: string, count: number): Promise<void> {
    const expired = Math.floor(Date.now() / 1000) - 3600;
    for (let i = 0; i < count; i += 1) {
      await testDb
        .prepare(
          "INSERT INTO space_event_queue (id, recipient_tag, ciphertext, expires_at, created_at, lease_until) VALUES (?, ?, ?, ?, ?, 0)",
        )
        .bind(
          crypto.getRandomValues(new Uint8Array(16)),
          tagBytes(tagB64),
          new Uint8Array(8).fill(i & 0xff),
          expired,
          expired - 60,
        )
        .run();
    }
  }

  it("deletes expired rows on the hourly cron, unacknowledged or not", async () => {
    const tag = freshTag();
    await insertExpired(tag, 5);
    expect(await storedFor(tag)).toHaveLength(5);

    // Invisible to readers even before the sweep — that was the whole problem:
    // invisible is not the same as gone.
    expect((await drain(tag)).events).toHaveLength(0);

    await worker.scheduled!(hourlyCron(), workerEnv, ctx);
    expect(await storedFor(tag)).toHaveLength(0);
  });

  it("leaves live rows alone", async () => {
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x0f, 8))).status).toBe(202);
    await worker.scheduled!(hourlyCron(), workerEnv, ctx);
    expect(await storedFor(tag)).toHaveLength(1);
  });

  it("removes a drained-but-never-acknowledged row once it expires", async () => {
    // The backstop for D-273's lease: erring toward duplicate delivery must not
    // become erring toward unbounded retention.
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x10, 8))).status).toBe(202);
    expect((await drain(tag)).events).toHaveLength(1);
    await testDb
      .prepare(
        "UPDATE space_event_queue SET expires_at = ? WHERE recipient_tag = ?",
      )
      .bind(Math.floor(Date.now() / 1000) - 1, tagBytes(tag))
      .run();

    await worker.scheduled!(hourlyCron(), workerEnv, ctx);
    expect(await storedFor(tag)).toHaveLength(0);
  });

  it("reports what it could NOT remove instead of silently truncating", async () => {
    // D-254's residue-overflow lesson. The sweep is bounded, so it MUST be
    // able to say the bound was reached; a bounded sweep that reports success
    // reads exactly like a finished one.
    const tag = freshTag();
    const expired = Math.floor(Date.now() / 1000) - 3600;
    await insertExpired(tag, 3);

    // Sweep with a `now` that predates the rows: nothing is removable, so the
    // reported figures must be zero-deleted and non-zero-remaining is proved
    // separately below rather than inferred.
    const noop = await sweepExpiredSpaceEvents(testDb, expired - 120);
    expect(noop).toEqual({ deleted: 0, remaining: 0, boundReached: false });
    expect(await storedFor(tag)).toHaveLength(3);

    const swept = await sweepExpiredSpaceEvents(testDb);
    expect(swept.deleted).toBeGreaterThanOrEqual(3);
    expect(swept.remaining).toBe(0);
    expect(swept.boundReached).toBe(false);
    expect(await storedFor(tag)).toHaveLength(0);
  });

  it("bounds one run and reports the residue, proved by starving the bound", async () => {
    // The residue path is EXECUTED, not described. Five expired rows against a
    // bound of two (one row per batch, two batches) must delete two, leave
    // three, and SAY so.
    const tag = freshTag();
    await insertExpired(tag, 5);
    const now = Math.floor(Date.now() / 1000);

    const starved = await sweepExpiredSpaceEvents(testDb, now, {
      batchSize: 1,
      maxBatches: 2,
    });
    expect(starved).toEqual({ deleted: 2, remaining: 3, boundReached: true });
    expect(await storedFor(tag)).toHaveLength(3);

    // The shipped bound is large enough to finish this backlog, so the same
    // call with production's constants completes and reports no residue.
    expect(SPACE_EVENT_SWEEP_MAX_ROWS).toBeGreaterThanOrEqual(3);
    const finished = await sweepExpiredSpaceEvents(testDb, now);
    expect(finished).toEqual({ deleted: 3, remaining: 0, boundReached: false });
    expect(await storedFor(tag)).toHaveLength(0);
  });

  it("makes the cron say what it swept, so an inert sweep is visible", async () => {
    // The sweep is only useful if the cron actually calls it and reports. This
    // asserts the real line the handler writes, so deleting the cron branch
    // fails here as well as on the row count.
    const tag = freshTag();
    await insertExpired(tag, 2);
    const logs: string[] = [];
    const originalLog = console.log;
    console.log = (...args: unknown[]) => {
      logs.push(args.map(String).join(" "));
    };
    try {
      await worker.scheduled!(hourlyCron(), workerEnv, ctx);
    } finally {
      console.log = originalLog;
    }
    expect(
      logs.some((line) => /space event sweep deleted \d+ expired row\(s\)/.test(line)),
    ).toBe(true);
    expect(await storedFor(tag)).toHaveLength(0);
  });

  it("bounds how far ahead a sender may push expires_at", async () => {
    // A sweep over an unbounded `expires_at` is not a retention policy: one
    // POST could pin ciphertext in the relay past any horizon. Same 7-day
    // ceiling the wrapped-key lane puts on itself.
    const tag = freshTag();
    const overLimit = await enqueue(
      tag,
      b64(0x11, 8),
      MAX_EVENT_LIFETIME_SECONDS + 600,
    );
    expect(overLimit.status).toBe(400);
    // The SAME message every other envelope violation returns.
    expect(await overLimit.json()).toEqual({
      error: "invalid opaque Space event envelope",
    });
    expect(await storedFor(tag)).toHaveLength(0);

    const atLimit = await enqueue(
      tag,
      b64(0x12, 8),
      MAX_EVENT_LIFETIME_SECONDS - 60,
    );
    expect(atLimit.status).toBe(202);
    expect(await storedFor(tag)).toHaveLength(1);
  });
});

// D-260 / OPEN-4 — the owner's ruling on the drain's METHOD.
//
// `GET /v1/space-events/:tag` is REMOVED, not deprecated. It carried the lane's
// entire bearer capability -- the 32-byte tag -- in the request path, where
// every intermediary writes it to a default log (the D81 defect), while
// PERFORMING A WRITE: it leases every row it returns. A method that proxies,
// prefetchers and generic retry logic all treat as safe and repeatable must not
// do that. `POST /v1/space-events/drain` takes the tag in the body instead.
//
// These specs execute the route. None of them reads source text.
describe("D-260 the drain is a POST with the tag in the body", () => {
  it("runs the whole lifecycle over the new route: enqueue, drain, ack, empty", async () => {
    const tag = freshTag();
    const first = b64(0x21, 32);
    const second = b64(0x22, 16);
    expect((await enqueue(tag, first)).status).toBe(202);
    expect((await enqueue(tag, second)).status).toBe(202);
    expect(await storedFor(tag)).toHaveLength(2);

    // DRAIN — over POST /v1/space-events/drain, byte for byte what was queued.
    const drained = await drainRequest(tag);
    expect(drained.status).toBe(200);
    const body = (await drained.json()) as {
      events: { event_id: string; ciphertext: string }[];
    };
    expect(body.events.map((event) => event.ciphertext)).toEqual([first, second]);
    // Reserved, not destroyed: D15 still holds on the new method.
    expect(await storedFor(tag)).toHaveLength(2);
    for (const row of await storedFor(tag)) {
      expect(row.lease_until).toBeGreaterThan(0);
    }

    // ACK — and only now do the rows leave storage.
    const acked = await ack(
      tag,
      body.events.map((event) => event.event_id),
    );
    expect(acked.status).toBe(200);
    expect(await acked.json()).toEqual({ acknowledged: true });

    // EMPTY — in storage, and to a reader, including after every lease lapses.
    expect(await storedFor(tag)).toHaveLength(0);
    await expireLeases(tag);
    expect((await drain(tag)).events).toHaveLength(0);
  });

  it("has no GET drain left to replay: the old route is gone", async () => {
    // THE EFFECT OF THE REMOVAL, not its spelling. The event stays queued and
    // undisturbed -- an intermediary replaying the old URL can no longer lease
    // a recipient's events away from it.
    const tag = freshTag();
    expect((await enqueue(tag, b64(0x23, 8))).status).toBe(202);

    const res = await SELF.fetch(
      `http://test/v1/space-events/${encodeURIComponent(tag)}`,
    );
    expect(res.status).toBe(404);

    const stored = await storedFor(tag);
    expect(stored).toHaveLength(1);
    expect(stored[0]!.lease_until).toBe(0);
    // And the row is still deliverable over the route that replaced it.
    expect((await drain(tag)).events).toHaveLength(1);
  });

  it("refuses a malformed tag with the same single rejection, oracle intact", async () => {
    // Three shapes of wrong tag, one answer. A drain for a well-formed tag with
    // nothing queued still answers 200 with an empty list, so a caller cannot
    // learn whether any tag exists.
    for (const recipient_tag of [b64(0x24, 31), b64(0x24, 33), 12345]) {
      const res = await SELF.fetch("http://test/v1/space-events/drain", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ recipient_tag }),
      });
      expect(res.status).toBe(400);
      expect(await res.json()).toEqual({ error: "invalid recipient tag" });
    }

    const unused = await drainRequest(freshTag());
    expect(unused.status).toBe(200);
    expect(await unused.json()).toEqual({ events: [] });
  });
});
