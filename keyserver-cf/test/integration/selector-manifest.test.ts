import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("GET /v1/selector-manifest", () => {
  it("503s when SELECTOR_MANIFEST_JSON is unset (default test env)", async () => {
    // The test vitest config sets it to "".
    expect(env.SELECTOR_MANIFEST_JSON ?? "").toBe("");
    const res = await SELF.fetch("http://test/v1/selector-manifest");
    expect(res.status).toBe(503);
    const j = (await res.json()) as { error: string };
    expect(j.error).toMatch(/not configured/);
  });
});
