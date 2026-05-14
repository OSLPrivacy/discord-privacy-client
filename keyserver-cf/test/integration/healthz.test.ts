import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("GET /v1/healthz", () => {
  it("returns {ok:true} without auth", async () => {
    const res = await SELF.fetch("http://test/v1/healthz");
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({ ok: true });
  });

  it("404s for unknown paths", async () => {
    const res = await SELF.fetch("http://test/nope");
    expect(res.status).toBe(404);
  });
});
