import { SELF } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import {
  RELEASE_SIGNATURES,
  signatureFor,
} from "../../src/endpoints/update-manifest.js";

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
    // G3.2: signature tracks RELEASE_SIGNATURES — auto-updates to the
    // real value once the operator pastes the .msi.sig contents.
    expect(platform!.signature).toBe(RELEASE_SIGNATURES["0.0.1"]);
  });

  it("manifest signature equals the current RELEASE_SIGNATURES[0.0.1] (auto-tracks operator update)", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0`);
    const body = (await res.json()) as {
      platforms: Record<string, { signature: string }>;
    };
    expect(body.platforms["windows-x86_64"]!.signature).toBe(
      RELEASE_SIGNATURES["0.0.1"],
    );
  });

  it("unknown version → warning path, signature falls back to empty string", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const sig = signatureFor("999.0.0");
    expect(sig).toBe("");
    expect(warn).toHaveBeenCalledOnce();
    expect(warn.mock.calls[0]![0]).toContain("MISSING signature");
    warn.mockRestore();
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

describe("GET /v1/update-manifest — channels (G3.3)", () => {
  it("?channel=beta returns a 200 manifest for 0.0.0", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0?channel=beta`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      version: string;
      platforms: Record<string, { url: string }>;
    };
    expect(body.version).toBe("0.0.1");
    expect(body.platforms["windows-x86_64"]!.url).toBe(
      "https://installers.oslprivacy.com/osl-privacy-0.0.1.msi",
    );
  });

  it("?channel=stable returns a 200 manifest for 0.0.0", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.0?channel=stable`);
    expect(res.status).toBe(200);
    expect(((await res.json()) as { version: string }).version).toBe("0.0.1");
  });

  it("?channel=garbage falls back to stable, still 200", async () => {
    const res = await SELF.fetch(
      `${BASE}/windows/x86_64/0.0.0?channel=please-give-me-secret-builds`,
    );
    expect(res.status).toBe(200);
    expect(((await res.json()) as { version: string }).version).toBe("0.0.1");
  });

  it("no channel param behaves as stable (back-compat)", async () => {
    type M = { version: string; platforms: Record<string, { url: string }> };
    const withParam = (await (
      await SELF.fetch(`${BASE}/windows/x86_64/0.0.0?channel=stable`)
    ).json()) as M;
    const without = (await (
      await SELF.fetch(`${BASE}/windows/x86_64/0.0.0`)
    ).json()) as M;
    expect(without.version).toBe(withParam.version);
    expect(without.platforms["windows-x86_64"]!.url).toBe(
      withParam.platforms["windows-x86_64"]!.url,
    );
  });

  it("beta channel: up-to-date client (0.0.1) still 204", async () => {
    const res = await SELF.fetch(`${BASE}/windows/x86_64/0.0.1?channel=beta`);
    expect(res.status).toBe(204);
  });
});
