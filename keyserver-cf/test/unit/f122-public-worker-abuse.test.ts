import { describe, expect, it, vi } from "vitest";
import worker from "../../src/index.js";
import type { Env } from "../../src/env.js";

function rateLimitBinding(success: boolean): {
  binding: RateLimit;
  limit: ReturnType<typeof vi.fn>;
} {
  const limit = vi.fn(async () => ({ success }));
  return { binding: { limit } as RateLimit, limit };
}

function envWith(args: {
  publicGetAllowed: boolean;
  mutationAllowed?: boolean;
}): {
  env: Env;
  publicGetLimit: ReturnType<typeof vi.fn>;
  mutationLimit: ReturnType<typeof vi.fn>;
  prepare: ReturnType<typeof vi.fn>;
} {
  const allow = rateLimitBinding(true);
  const publicGet = rateLimitBinding(args.publicGetAllowed);
  const mutation = rateLimitBinding(args.mutationAllowed ?? true);
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

describe("f122 public Worker abuse prevention", () => {
  it("rate-limits public GET ingress before endpoint or database work", async () => {
    const { env, publicGetLimit, mutationLimit, prepare } = envWith({
      publicGetAllowed: false,
    });

    const response = await worker.fetch(
      new Request("https://keyserver.test/v1/pubkeys/osl1_target", {
        headers: { "cf-connecting-ip": "203.0.113.80" },
      }),
      env,
      ctx,
    );

    expect(response.status).toBe(429);
    expect(response.headers.get("retry-after")).toBe("60");
    expect(await response.json()).toEqual({ error: "rate_limited" });
    expect(publicGetLimit).toHaveBeenCalledWith({
      key: "public-get-ingress:203.0.113.80",
    });
    expect(mutationLimit).not.toHaveBeenCalled();
    expect(prepare).not.toHaveBeenCalled();
  });

  it("keeps mutation ingress on the mutation bucket instead of the public GET bucket", async () => {
    const { env, publicGetLimit, mutationLimit, prepare } = envWith({
      publicGetAllowed: false,
      mutationAllowed: false,
    });

    const response = await worker.fetch(
      new Request("https://keyserver.test/v1/register", {
        method: "POST",
        headers: {
          "cf-connecting-ip": "203.0.113.81",
          "content-type": "application/json",
        },
        body: "{}",
      }),
      env,
      ctx,
    );

    expect(response.status).toBe(429);
    expect(publicGetLimit).not.toHaveBeenCalled();
    expect(mutationLimit).toHaveBeenCalledWith({
      key: "mutation-ingress:203.0.113.81",
    });
    expect(prepare).not.toHaveBeenCalled();
  });
});
