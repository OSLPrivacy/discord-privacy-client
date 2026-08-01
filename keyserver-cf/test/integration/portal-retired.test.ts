import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("POST /v1/billing-portal-session", () => {
  it("returns 410 with an honest retirement message", async () => {
    const response = await SELF.fetch("http://test/v1/billing-portal-session", {
      method: "POST",
    });

    expect(response.status).toBe(410);
    await expect(response.json()).resolves.toEqual({
      error: "billing portal is no longer available",
    });
  });
});
