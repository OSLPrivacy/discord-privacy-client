import { describe, expect, it, vi } from "vitest";

import type { Env } from "../../src/env.js";
import { checkAdminToken } from "../../src/lib/auth.js";

function env(tokens: Partial<Pick<Env, "OSL_KEYSERVER_ADMIN_TOKEN">>): Env {
  return tokens as Env;
}

describe("bearer authorization", () => {
  it("fails closed when a deployment omits a token", async () => {
    const log = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const request = new Request("https://keyserver.test/v1/wrapped-keys");

    expect((await checkAdminToken(request, env({})))?.status).toBe(503);
    log.mockRestore();
  });

  it("rejects a missing or wrong configured bearer", async () => {
    const log = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const configured = env({ OSL_KEYSERVER_ADMIN_TOKEN: "expected" });
    expect(
      (await checkAdminToken(
        new Request("https://keyserver.test/v1/wrapped-keys"),
        configured,
      ))?.status,
    ).toBe(401);
    expect(
      (await checkAdminToken(
        new Request("https://keyserver.test/v1/wrapped-keys", {
          headers: { authorization: "Bearer wrong" },
        }),
        configured,
      ))?.status,
    ).toBe(401);
    log.mockRestore();
  });

  it("accepts the exact configured bearer", async () => {
    const request = new Request("https://keyserver.test/v1/wrapped-keys", {
      headers: { authorization: "Bearer expected" },
    });
    expect(
      await checkAdminToken(
        request,
        env({ OSL_KEYSERVER_ADMIN_TOKEN: "expected" }),
      ),
    ).toBeNull();
  });
});
