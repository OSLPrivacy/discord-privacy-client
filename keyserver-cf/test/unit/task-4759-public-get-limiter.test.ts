import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { callerIp, checkRateLimit } from "../../src/lib/rate-limit.js";

const REQUEST_COUNT = 1199;
const SOURCE = new URL("../../src/index.ts", import.meta.url);

function publicGetThresholdFromSource(): number {
  const source = readFileSync(SOURCE, "utf8");
  const match = source.match(
    /\bPUBLIC_GET_INGRESS_MAX_PER_MINUTE\s*=\s*(\d+)\s*;/,
  );
  if (!match) throw new Error("PUBLIC_GET_INGRESS_MAX_PER_MINUTE was not found");
  return Number(match[1]);
}

function rateLimitBinding(): {
  binding: RateLimit;
  limit: ReturnType<typeof vi.fn>;
} {
  const limit = vi.fn(async () => ({ success: true }));
  return { binding: { limit } as RateLimit, limit };
}

function envWithLimiters(): {
  env: Env;
  limits: Record<5 | 10 | 120 | 1200 | 3600, ReturnType<typeof vi.fn>>;
} {
  const limit5 = rateLimitBinding();
  const limit10 = rateLimitBinding();
  const limit120 = rateLimitBinding();
  const limit1200 = rateLimitBinding();
  const limit3600 = rateLimitBinding();

  return {
    env: {
      RATE_LIMIT_5: limit5.binding,
      RATE_LIMIT_10: limit10.binding,
      RATE_LIMIT_120: limit120.binding,
      RATE_LIMIT_1200: limit1200.binding,
      RATE_LIMIT_3600: limit3600.binding,
      SELECTOR_MANIFEST_JSON: JSON.stringify({ schema: "task-4759" }),
    } as unknown as Env,
    limits: {
      5: limit5.limit,
      10: limit10.limit,
      120: limit120.limit,
      1200: limit1200.limit,
      3600: limit3600.limit,
    },
  };
}

describe("task 4759 public GET native limiter number", () => {
  it("keeps the public GET gate on the supported 1200/minute native binding", async () => {
    const publicGetThreshold = publicGetThresholdFromSource();

    const { env, limits } = envWithLimiters();

    for (let requestNumber = 1; requestNumber <= REQUEST_COUNT; requestNumber += 1) {
      const request = new Request(
        `https://keyserver.test/v1/selector-manifest?n=${requestNumber}`,
        {
          headers: { "cf-connecting-ip": "203.0.113.119" },
        },
      );
      const decision = await checkRateLimit(
        env,
        callerIp(request),
        publicGetThreshold,
        "public-get-ingress",
      );

      if (!decision.ok) {
        const threshold = publicGetThresholdFromSource();
        throw new Error(
          `request number ${requestNumber} was refused; public GET limiter asked for ${threshold}; ` +
            `unsupported native rate-limit threshold: ${threshold}`,
        );
      }

      expect(decision, `request number ${requestNumber}`).toEqual({
        ok: true,
        retryAfter: 0,
      });
    }

    expect(publicGetThreshold).toBe(1200);
    expect(limits[1200]).toHaveBeenCalledTimes(REQUEST_COUNT);
    expect(limits[1200]).toHaveBeenLastCalledWith({
      key: "public-get-ingress:203.0.113.119",
    });
    expect(limits[5]).not.toHaveBeenCalled();
    expect(limits[10]).not.toHaveBeenCalled();
    expect(limits[120]).not.toHaveBeenCalled();
    expect(limits[3600]).not.toHaveBeenCalled();
    console.log("request 1199 succeeding");
  });
});
