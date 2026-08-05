// D-260: the two `/v1/space-events` routes were dispatched above `const method`
// in `dispatch`, i.e. ahead of every gate the router applies before a caller may
// send. These tests hold each of the three bypasses shut, and hold the lane's
// actual behaviour (enqueue, then a consuming drain) unchanged by the move.
//
// D-273 later changed HOW the drain consumes -- it leases rather than deletes,
// and `POST /v1/space-events/ack` is what destroys a row. Nothing about the
// ingress ordering these tests hold changed, and no assertion here was
// relaxed for it; see d273-space-event-ack.test.ts.
import { SELF, env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import worker from "../../src/index.js";
import type { Env } from "../../src/env.js";

const TAG_BYTES = 32;

/** base64 of `length` bytes all equal to `fill` — path-safe for these values. */
function b64(fill: number, length: number): string {
  return btoa(String.fromCharCode(...new Uint8Array(length).fill(fill)));
}

function rateLimitBinding(success: boolean) {
  const limit = vi.fn(async () => ({ success }));
  return { binding: { limit } as RateLimit, limit };
}

/**
 * A worker env whose limiters answer as instructed and whose DB throws if it is
 * touched at all. `prepare` is the instrument for "did a row get read, leased
 * or deleted": the drain's only database access goes through it.
 */
function envWith(args: { publicGetAllowed: boolean; mutationAllowed: boolean }) {
  const allow = rateLimitBinding(true);
  const publicGet = rateLimitBinding(args.publicGetAllowed);
  const mutation = rateLimitBinding(args.mutationAllowed);
  const prepare = vi.fn(() => {
    throw new Error("DB should not be reached by this test");
  });
  const env = {
    DB: { prepare } as unknown as D1Database,
    RATE_LIMIT_5: allow.binding,
    RATE_LIMIT_10: allow.binding,
    RATE_LIMIT_120: allow.binding,
    RATE_LIMIT_1200: publicGet.binding,
    RATE_LIMIT_3600: mutation.binding,
  } as Env;
  return {
    env,
    publicGetLimit: publicGet.limit,
    mutationLimit: mutation.limit,
    prepare,
  };
}

const ctx = {} as ExecutionContext;

describe("D-260 /v1/space-events sits behind the ingress gates", () => {
  it("bounds the POST body before it is read, not after it is parsed", async () => {
    // Valid JSON, over MAX_MUTATION_BODY_BYTES. Before the fix this was read to
    // the end and JSON.parsed, and only the envelope check rejected it — the
    // 400 below was the *symptom*. A 413 can only come from bufferRequestBody,
    // which decides before the body is consumed.
    const oversize = JSON.stringify({ pad: "a".repeat(1024 * 1024) });
    expect(oversize.length).toBeGreaterThan(1024 * 1024);

    const res = await SELF.fetch("http://test/v1/space-events", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: oversize,
    });

    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({ error: "request body too large" });
    // Specifically NOT the handler's own post-parse rejection.
    expect(res.status).not.toBe(400);
  });

  it("still lets an under-limit body reach the handler's own 64 KiB ciphertext cap", async () => {
    // Under 1 MiB, so buffering admits it; the envelope's ciphertext is over the
    // handler's 64 KiB cap, so the handler rejects it. This proves the coarse
    // bound did not swallow the fine one, and that the route is still reachable.
    const res = await SELF.fetch("http://test/v1/space-events", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        recipient_tag: b64(0x02, TAG_BYTES),
        ciphertext: b64(0x03, 64 * 1024 + 1),
        expires_at: Math.floor(Date.now() / 1000) + 3600,
      }),
    });

    expect(res.status).toBe(400);
    expect(await res.json()).toMatchObject({
      error: "invalid opaque Space event envelope",
    });
  });

  it("refuses the POST on the mutation-ingress bucket, before the database", async () => {
    const { env, mutationLimit, publicGetLimit, prepare } = envWith({
      publicGetAllowed: true,
      mutationAllowed: false,
    });

    const res = await worker.fetch(
      new Request("https://keyserver.test/v1/space-events", {
        method: "POST",
        headers: {
          "cf-connecting-ip": "203.0.113.90",
          "content-type": "application/json",
        },
        body: JSON.stringify({
          recipient_tag: b64(0x04, TAG_BYTES),
          ciphertext: b64(0x05, 32),
          expires_at: Math.floor(Date.now() / 1000) + 3600,
        }),
      }),
      env,
      ctx,
    );

    expect(res.status).toBe(429);
    expect(res.headers.get("retry-after")).toBe("60");
    expect(await res.json()).toEqual({ error: "rate_limited" });
    expect(mutationLimit).toHaveBeenCalledWith({
      key: "mutation-ingress:203.0.113.90",
    });
    // A mutation must not be charged to, or admitted by, the public-GET bucket.
    expect(publicGetLimit).not.toHaveBeenCalled();
    expect(prepare).not.toHaveBeenCalled();
  });

  // D-273 renamed this spec: the drain no longer deletes, it leases. D-260/
  // OPEN-4 then moved it off `GET /v1/space-events/:tag` onto
  // `POST /v1/space-events/drain`. The property asserted is the SAME one and
  // the bound is the SAME number -- a throttled drain must not reach the queue
  // at all, and it must still be refused on the 1200/min public-GET bucket.
  //
  // THE FLOOR IS THE POINT OF THIS SPEC. The mutation-ingress limit is 3600/min
  // (index.ts) and the public-GET limit is 1200/min, so registering the drain
  // as an ordinary POST and stopping there would have TRIPLED the budget for
  // probing the 32-byte tag space. The route is charged to both, and this
  // asserts the binding one is still the 1200 bucket, under the same key.
  it("refuses the drain on the public-GET bucket, touching the queue not at all", async () => {
    const { env, publicGetLimit, mutationLimit, prepare } = envWith({
      publicGetAllowed: false,
      mutationAllowed: true,
    });

    const res = await worker.fetch(
      new Request("https://keyserver.test/v1/space-events/drain", {
        method: "POST",
        headers: {
          "cf-connecting-ip": "203.0.113.91",
          "content-type": "application/json",
        },
        body: JSON.stringify({ recipient_tag: b64(0x00, TAG_BYTES) }),
      }),
      env,
      ctx,
    );

    expect(res.status).toBe(429);
    expect(res.headers.get("retry-after")).toBe("60");
    expect(await res.json()).toEqual({ error: "rate_limited" });
    expect(publicGetLimit).toHaveBeenCalledWith({
      key: "public-get-ingress:203.0.113.91",
    });
    // The mutation gate ran and ADMITTED it; the public-GET bucket is what
    // refused. That is the floor being preserved rather than merely present.
    expect(mutationLimit).toHaveBeenCalledWith({
      key: "mutation-ingress:203.0.113.91",
    });
    // The whole point: the drain both reads and writes the queue (D-273: it
    // leases every row it returns; before that it DELETEd them).
    // A throttled drain must not have reached the queue at all.
    expect(prepare).not.toHaveBeenCalled();
  });

  // D-260/OPEN-4: the removed method, asserted by its effect. A replayed `GET`
  // must not be able to lease a recipient's events away from it.
  it("has no GET drain to bypass anything: the old route is gone", async () => {
    const { env, publicGetLimit, prepare } = envWith({
      publicGetAllowed: true,
      mutationAllowed: true,
    });

    const res = await worker.fetch(
      new Request(
        `https://keyserver.test/v1/space-events/${encodeURIComponent(b64(0x00, TAG_BYTES))}`,
        { headers: { "cf-connecting-ip": "203.0.113.92" } },
      ),
      env,
      ctx,
    );

    expect(res.status).toBe(404);
    // It is still gated -- the removal did not hoist anything above the limit --
    // and it reaches no database at all.
    expect(publicGetLimit).toHaveBeenCalledWith({
      key: "public-get-ingress:203.0.113.92",
    });
    expect(prepare).not.toHaveBeenCalled();
  });

  it("still distributes Space events end to end once behind the gates", async () => {
    const tag = b64(0x01, TAG_BYTES);
    const ciphertext = b64(0x07, 48);

    const post = await SELF.fetch("http://test/v1/space-events", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        recipient_tag: tag,
        ciphertext,
        expires_at: Math.floor(Date.now() / 1000) + 3600,
      }),
    });
    expect(post.status).toBe(202);
    expect(await post.json()).toEqual({ accepted: true });

    // D-273 added `event_id` to each event: the acknowledgement half of D15
    // has to name what it is acknowledging. The assertion is not relaxed for
    // it -- `toEqual` still pins the response exactly, and the id is pinned to
    // the actual `space_event_queue` row id read out of D1 rather than waved
    // through with `expect.any(String)`.
    const queued = await (env as unknown as { DB: D1Database }).DB.prepare(
      "SELECT id FROM space_event_queue WHERE recipient_tag = ?",
    )
      .bind(Uint8Array.from(atob(tag), (c) => c.charCodeAt(0)))
      .first<{ id: Uint8Array }>();
    const expectedEventId = Array.from(queued!.id, (b) =>
      b.toString(16).padStart(2, "0"),
    ).join("");
    expect(expectedEventId).toHaveLength(32);

    // D-260/OPEN-4: over `POST /v1/space-events/drain`, tag in the body.
    const drainBody = JSON.stringify({ recipient_tag: tag });
    const drain = await SELF.fetch("http://test/v1/space-events/drain", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: drainBody,
    });
    expect(drain.status).toBe(200);
    expect(await drain.json()).toEqual({
      events: [{ event_id: expectedEventId, ciphertext }],
    });

    // The drain consumes: a second call returns nothing.
    const again = await SELF.fetch("http://test/v1/space-events/drain", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: drainBody,
    });
    expect(again.status).toBe(200);
    expect(await again.json()).toEqual({ events: [] });
  });
});
