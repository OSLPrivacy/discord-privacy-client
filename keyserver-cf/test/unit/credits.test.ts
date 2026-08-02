import { describe, expect, it, vi } from "vitest";
import { handleCreditSpend } from "../../src/endpoints/credits.js";
import type { Env } from "../../src/env.js";

describe("T13-TF6 privacy-preserving credit metering boundary", () => {
  it("creates no per-request record and fails closed until anonymous replay-safe spend exists", async () => {
    const prepare = vi.fn(() => {
      throw new Error("credit spending must not create a D1 usage record");
    });
    const response = await handleCreditSpend(
      new Request("https://keyserver.test/v1/credits/spend", {
        method: "POST",
        body: JSON.stringify({ bearer: "opaque-unlinkable-token" }),
      }),
      { DB: { prepare } } as unknown as Env,
    );

    expect(response.status).toBe(503);
    expect(await response.text()).toContain("anonymous replay-safe credit metering");
    expect(prepare).not.toHaveBeenCalled();
  });

  it("rejects identity fields before a future meter could observe them", async () => {
    const response = await handleCreditSpend(
      new Request("https://keyserver.test/v1/credits/spend", {
        method: "POST",
        body: JSON.stringify({ bearer: "opaque", user_id: "must-not-be-metered" }),
      }),
      {} as Env,
    );

    expect(response.status).toBe(400);
  });
});
