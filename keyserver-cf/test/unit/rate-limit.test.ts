import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { callerIp, checkRateLimit } from "../../src/lib/rate-limit.js";

function envWith(
  threshold: 5 | 10 | 120 | 1200 | 3600,
  success: boolean,
): { env: Env; limit: ReturnType<typeof vi.fn> } {
  const limit = vi.fn(async () => ({ success }));
  const limiter = { limit } as RateLimit;
  const env = {
    [`RATE_LIMIT_${threshold}`]: limiter,
  } as unknown as Env;
  return { env, limit };
}

describe("native rate limiter", () => {
  it("uses an isolated bucket-and-actor key", async () => {
    const { env, limit } = envWith(120, true);
    await expect(checkRateLimit(env, "user-42", 120, "control-post")).resolves.toEqual({
      ok: true,
      retryAfter: 0,
    });
    expect(limit).toHaveBeenCalledWith({ key: "control-post:user-42" });
  });

  it("returns a bounded retry interval when Cloudflare denies", async () => {
    const { env } = envWith(5, false);
    await expect(checkRateLimit(env, "customer", 5, "checkout")).resolves.toEqual({
      ok: false,
      retryAfter: 60,
    });
  });

  it("fails closed for an unsupported or missing binding", async () => {
    await expect(checkRateLimit({} as Env, "actor", 999, "bad")).resolves.toEqual({
      ok: false,
      retryAfter: 60,
    });
  });
});

describe("caller IP extraction", () => {
  it("uses only Cloudflare's authenticated connecting-IP header", () => {
    const request = new Request("https://keyserver.test", {
      headers: {
        "cf-connecting-ip": "203.0.113.9",
        "x-forwarded-for": "198.51.100.77",
      },
    });
    expect(callerIp(request)).toBe("203.0.113.9");
  });

  it("does not trust a caller-supplied X-Forwarded-For fallback", () => {
    const request = new Request("https://keyserver.test", {
      headers: { "x-forwarded-for": "198.51.100.77" },
    });
    expect(callerIp(request)).toBe("unknown");
  });
});
