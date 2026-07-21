import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import { rateLimit } from "../src/lib/rate-limit.js";

const secret = "s".repeat(48);

function envWith(namespace: Partial<KVNamespace>): Env {
  return {
    DB: {} as D1Database,
    ATTACHMENTS: {} as R2Bucket,
    RATE_LIMIT: namespace as KVNamespace,
    RATE_LIMIT_HASH_KEY: secret,
  };
}

describe("cipher-store rate limiter failure policy", () => {
  it("fails closed for anonymous writes when KV is unavailable", async () => {
    const env = envWith({ get: vi.fn().mockRejectedValue(new Error("down")) });
    await expect(rateLimit(env, "203.0.113.1", "upload")).resolves.toEqual({
      allowed: false,
      remaining: 0,
    });
    await expect(rateLimit(env, "203.0.113.1", "delete")).resolves.toEqual({
      allowed: false,
      remaining: 0,
    });
    await expect(rateLimit(env, "203.0.113.1", "attachment-upload")).resolves.toEqual({
      allowed: false,
      remaining: 0,
    });
  });

  it("keeps ciphertext reads available when KV is unavailable", async () => {
    const env = envWith({ get: vi.fn().mockRejectedValue(new Error("down")) });
    await expect(rateLimit(env, "203.0.113.1", "fetch")).resolves.toEqual({
      allowed: true,
      remaining: 0,
    });
    await expect(rateLimit(env, "203.0.113.1", "attachment-fetch")).resolves.toEqual({
      allowed: true,
      remaining: 0,
    });
  });

  it("never writes a raw or plain-hashed IP into the KV key", async () => {
    const put = vi.fn().mockResolvedValue(undefined);
    const env = envWith({ get: vi.fn().mockResolvedValue(null), put });
    await rateLimit(env, "203.0.113.77", "upload");
    const storedKey = String(put.mock.calls[0]?.[0]);
    expect(storedKey).not.toContain("203.0.113.77");
    expect(storedKey).toMatch(/^rl:upload:\d+:[0-9a-f]{32}$/);
  });
});
