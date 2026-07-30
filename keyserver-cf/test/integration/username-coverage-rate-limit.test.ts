import { describe, expect, it, vi } from "vitest";
import { handleRegister } from "../../src/endpoints/register.js";
import type { Env } from "../../src/env.js";

function envWith(limit: ReturnType<typeof vi.fn>): Env {
  return {
    RATE_LIMIT_5: { limit } as unknown as RateLimit,
  } as unknown as Env;
}

function registerRequest(body: unknown): Request {
  return new Request("http://test/v1/register", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "cf-connecting-ip": "198.51.100.42",
      "x-forwarded-for": "203.0.113.99",
    },
    body: JSON.stringify(body),
  });
}

function minimallyShapedRegisterBody(userId: string): Record<string, unknown> {
  return {
    user_id: userId,
    ik_x25519_pub: "x",
    ik_ed25519_pub: "x",
    ik_mlkem768_pub: "x",
    registration_sig: "x",
  };
}

describe("public registration abuse prevention", () => {
  it("Rate-limit and abuse-prevention on the public Worker endpoint", async () => {
    const ipDenied = vi.fn(async () => ({ success: false }));
    const deniedByIp = await handleRegister(
      registerRequest("not even json object"),
      envWith(ipDenied),
    );
    expect(deniedByIp.status).toBe(429);
    expect(deniedByIp.headers.get("retry-after")).toBe("60");
    expect(ipDenied).toHaveBeenCalledWith({ key: "register-ip:198.51.100.42" });

    const perUserDenied = vi.fn(async ({ key }: { key: string }) => ({
      success: key.startsWith("register-ip:"),
    }));
    const deniedByUser = await handleRegister(
      registerRequest(minimallyShapedRegisterBody("public-username-target")),
      envWith(perUserDenied),
    );
    expect(deniedByUser.status).toBe(429);
    expect(perUserDenied).toHaveBeenCalledWith({
      key: "register-ip:198.51.100.42",
    });
    expect(perUserDenied).toHaveBeenCalledWith({
      key: "rlreg:public-username-target",
    });

    const snowflakeLimit = vi.fn(async () => ({ success: true }));
    const snowflake = await handleRegister(
      registerRequest(minimallyShapedRegisterBody("900000000000000001")),
      envWith(snowflakeLimit),
    );
    expect(snowflake.status).toBe(400);
    await expect(snowflake.json()).resolves.toEqual({
      error: "Discord identifiers are not OSL identities",
    });
    expect(snowflakeLimit).toHaveBeenCalledTimes(1);
    expect(snowflakeLimit).not.toHaveBeenCalledWith({
      key: "rlreg:900000000000000001",
    });
  });
});
