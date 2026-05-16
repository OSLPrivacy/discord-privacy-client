import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const BASE = "http://test/v1/update-manifest";

describe("GET /v1/update-manifest", () => {
  it("older client (0.0.0) → 200 manifest pointing at the 0.0.1 installer", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      version: string;
      notes: string;
      pub_date: string;
      platforms: Record<string, { signature: string; url: string }>;
    };
    expect(body.version).toBe("0.0.1");
    expect(typeof body.notes).toBe("string");
    expect(Number.isNaN(Date.parse(body.pub_date))).toBe(false);
    const platform = body.platforms["windows-x86_64"];
    expect(platform).toBeDefined();
    expect(platform!.url).toBe(
      "https://installers.oslprivacy.com/osl-privacy-0.0.1.msi",
    );
    // G3.2 fills the signature; empty placeholder for now.
    expect(platform!.signature).toBe("");
  });

  it("up-to-date client (0.0.1) → 204 No Content", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.1`);
    expect(res.status).toBe(204);
    expect(await res.text()).toBe("");
  });

  it("newer client (9.9.9) → 204 No Content", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/9.9.9`);
    expect(res.status).toBe(204);
  });

  it("malformed version → 400", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/not-a-version`);
    expect(res.status).toBe(400);
  });

  it("carries the CORS origin header (mirrors checkout pattern)", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0`);
    expect(res.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://oslprivacy.com",
    );
  });

  it("OPTIONS preflight → 204 with GET allowed", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0`, {
      method: "OPTIONS",
    });
    expect(res.status).toBe(204);
    expect(res.headers.get("Access-Control-Allow-Methods")).toBe(
      "GET, OPTIONS",
    );
  });
});
