// D-260: the two `/v1/space-events` routes were dispatched above `const method`
// in `dispatch`, i.e. ahead of every gate the router applies before a caller may
// send. These tests hold each of the three bypasses shut, and hold the lane's
// actual behaviour (enqueue, then a consuming drain) unchanged by the move.
import { SELF } from "cloudflare:test";
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
 * touched at all. `prepare` is the instrument for "did a row get read or
 * deleted": the drain's only database access goes through it.
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

  it("refuses the DESTRUCTIVE drain on the public-GET bucket, deleting nothing", async () => {
    const { env, publicGetLimit, mutationLimit, prepare } = envWith({
      publicGetAllowed: false,
      mutationAllowed: true,
    });

    const res = await worker.fetch(
      new Request(
        `https://keyserver.test/v1/space-events/${encodeURIComponent(b64(0x00, TAG_BYTES))}`,
        { headers: { "cf-connecting-ip": "203.0.113.91" } },
      ),
      env,
      ctx,
    );

    expect(res.status).toBe(429);
    expect(res.headers.get("retry-after")).toBe("60");
    expect(await res.json()).toEqual({ error: "rate_limited" });
    expect(publicGetLimit).toHaveBeenCalledWith({
      key: "public-get-ingress:203.0.113.91",
    });
    expect(mutationLimit).not.toHaveBeenCalled();
    // The whole point: `handleSpaceEventDrain` DELETEs every row it returns.
    // A throttled drain must not have reached the queue at all.
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

    const drain = await SELF.fetch(
      `http://test/v1/space-events/${encodeURIComponent(tag)}`,
    );
    expect(drain.status).toBe(200);
    expect(await drain.json()).toEqual({ events: [{ ciphertext }] });

    // The drain consumes: a second call returns nothing.
    const again = await SELF.fetch(
      `http://test/v1/space-events/${encodeURIComponent(tag)}`,
    );
    expect(again.status).toBe(200);
    expect(await again.json()).toEqual({ events: [] });
  });
});
