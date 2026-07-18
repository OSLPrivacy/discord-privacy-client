import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("mutation request bounds", () => {
  it("rejects an oversized streamed body before endpoint JSON parsing", async () => {
    const body = new Uint8Array(1024 * 1024 + 1);
    const res = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body,
    });
    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({ error: "request body too large" });
  });

  it("marks JSON errors as non-cacheable and non-sniffable", async () => {
    const res = await SELF.fetch("http://test/v1/register", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{}",
    });
    expect(res.headers.get("cache-control")).toBe("no-store");
    expect(res.headers.get("x-content-type-options")).toBe("nosniff");
    expect(res.headers.get("referrer-policy")).toBe("no-referrer");
  });
});
