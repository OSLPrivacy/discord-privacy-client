import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import worker from "../src/index.js";
import { landingSource, LANDING_BYTES } from "../src/lib/landing.js";

/// A landing request must never be able to reach storage. Any DB access
/// from this route would be an existence oracle by construction.
const noStorageEnv = {
  get DB(): never {
    throw new Error("the landing route must not touch storage");
  },
  get RATE_LIMIT(): never {
    throw new Error("the landing route must not touch the limiter");
  },
} as unknown as Env;

const ctx = {} as ExecutionContext;

function get(path: string): Request {
  return new Request("https://links.test" + path, { method: "GET" });
}

const IDS = [
  // Shapes a real link uses, plus every "should not exist" case.
  "0123456789abcdef0123456789abcdef",
  "ffffffffffffffffffffffffffffffff",
  "00000000000000000000000000000000",
  "not-a-real-id",
  "a",
  "%20weird%20",
  "0123456789abcdef0123456789abcdeg",
];

describe("view-once landing page", () => {
  it("is byte-identical for every id, including ids that never existed", async () => {
    const bodies: string[] = [];
    const lengths: string[] = [];
    const etags: string[] = [];
    for (const id of IDS) {
      const res = await worker.fetch(get("/v/" + id), noStorageEnv, ctx);
      expect(res.status).toBe(200);
      bodies.push(await res.text());
      lengths.push(res.headers.get("content-length") ?? "");
      etags.push(res.headers.get("etag") ?? "");
    }
    expect(new Set(bodies).size).toBe(1);
    expect(new Set(lengths).size).toBe(1);
    expect(new Set(etags).size).toBe(1);
    expect(lengths[0]).toBe(String(LANDING_BYTES.byteLength));
    expect(etags[0]).toMatch(/^"[0-9a-f]{32}"$/);
  });

  it("never embeds an id, so the page cannot vary with one", async () => {
    const res = await worker.fetch(
      get("/v/0123456789abcdef0123456789abcdef"),
      noStorageEnv,
      ctx,
    );
    const body = await res.text();
    expect(body).not.toContain("0123456789abcdef");
    // The page derives the id at runtime instead.
    expect(body).toContain("location.pathname");
  });

  it("serves the same page with and without a trailing slash", async () => {
    const a = await worker.fetch(get("/v/abc"), noStorageEnv, ctx);
    const b = await worker.fetch(get("/v/abc/"), noStorageEnv, ctx);
    expect(a.status).toBe(200);
    expect(b.status).toBe(200);
    expect(await a.text()).toBe(await b.text());
  });

  it("sends no-referrer and a CSP with no third-party subresource origin", async () => {
    const res = await worker.fetch(get("/v/abc"), noStorageEnv, ctx);
    expect(res.headers.get("referrer-policy")).toBe("no-referrer");
    const csp = res.headers.get("content-security-policy") ?? "";
    expect(csp).toContain("default-src 'self'");
    expect(csp).toContain("connect-src 'self'");
    expect(csp).toContain("frame-ancestors 'none'");
    expect(csp).toContain("base-uri 'none'");
    expect(csp).toContain("form-action 'none'");
    // Script and style are pinned by hash, never 'unsafe-inline'.
    expect(csp).toMatch(/script-src 'sha256-[A-Za-z0-9+/=]+'/);
    expect(csp).toMatch(/style-src 'sha256-[A-Za-z0-9+/=]+'/);
    expect(csp).not.toContain("unsafe-inline");
    expect(csp).not.toContain("unsafe-eval");
    // No host source anywhere: nothing may be loaded off-origin, so the
    // fragment has no outbound channel to leak through.
    expect(csp).not.toMatch(/https?:\/\//);
  });

  it("marks itself noindex and blocks robots at the site level", async () => {
    const res = await worker.fetch(get("/v/abc"), noStorageEnv, ctx);
    expect(res.headers.get("x-robots-tag")).toContain("noindex");
    const body = await res.text();
    expect(body).toContain('name="robots"');

    const robots = await worker.fetch(get("/robots.txt"), noStorageEnv, ctx);
    expect(robots.status).toBe(200);
    expect(await robots.text()).toBe("User-agent: *\nDisallow: /\n");
  });

  it("makes no third-party request and loads no external subresource", () => {
    const html = landingSource();
    expect(html).not.toMatch(/<script[^>]+src=/i);
    expect(html).not.toMatch(/<link[^>]+rel=["']?stylesheet/i);
    expect(html).not.toMatch(/<img\b/i);
    expect(html).not.toContain("//fonts.");
    expect(html).not.toContain("cdn.");
    // The only fetch targets are same-origin absolute paths.
    const fetchTargets = [...html.matchAll(/fetch\(\s*("[^"]*"|'[^']*')/g)].map(
      (m) => m[1] ?? "",
    );
    expect(fetchTargets.length).toBeGreaterThan(0);
    for (const target of fetchTargets) expect(target).toContain('"/v/"');
  });

  it("never creates an object URL or an image element for content", () => {
    const html = landingSource();
    expect(html).not.toContain("createObjectURL");
    expect(html).not.toContain("new Image(");
    expect(html).not.toContain("document.write");
    expect(html).toContain("createImageBitmap");
    expect(html).toContain("getContext(\"2d\")");
  });

  it("has a parseable inline script", () => {
    // The page is inert if the script has a syntax error, and the CSP
    // hash means a typo cannot be hot-fixed in the browser.
    const script = /<script>([\s\S]*)<\/script>/.exec(landingSource())?.[1];
    expect(script).toBeTruthy();
    expect(() => new Function(script!)).not.toThrow();
  });

  it("references only element ids the markup actually defines", () => {
    const html = landingSource();
    const referenced = [...html.matchAll(/byId\("([a-z]+)"\)/g)].map((m) => m[1]!);
    expect(referenced.length).toBeGreaterThan(0);
    for (const id of new Set(referenced)) {
      expect(html).toContain('id="' + id + '"');
    }
  });

  it("ships no DevTools detection", () => {
    const html = landingSource();
    // Deliberately absent: it is theatre, false-positives constantly,
    // and breaks accessibility tooling.
    for (const tell of [
      "devtool",
      "debugger",
      "outerWidth - window.innerWidth",
      "console.profile",
      "toString.call(console",
    ]) {
      expect(html.toLowerCase()).not.toContain(tell.toLowerCase());
    }
  });

  it("gates the reveal on a trusted gesture and sends the gesture header", () => {
    const html = landingSource();
    expect(html).toContain("e.isTrusted");
    expect(html).toContain('addEventListener("pointerdown", onReveal)');
    expect(html).toContain('addEventListener("keydown", onReveal)');
    expect(html).toContain('"x-osl-gesture": "1"');
    // The token travels in the POST body, never in a URL.
    expect(html).toContain('body: JSON.stringify({ t: token })');
  });

  it("reads the key from the fragment and strips it afterwards", () => {
    const html = landingSource();
    expect(html).toContain("location.hash");
    expect(html).toContain("history.replaceState");
    expect(html).toContain('crypto.subtle.importKey("raw"');
    expect(html).toContain('name: "AES-GCM"');
    // Zero JS crypto: no bundled cipher implementation.
    expect(html.toLowerCase()).not.toContain("forge");
    expect(html.toLowerCase()).not.toContain("sjcl");
    expect(html.toLowerCase()).not.toContain("cryptojs");
  });

  it("arms each deterrence layer that was promised", () => {
    const html = landingSource();
    expect(html).toContain('"visibilitychange"');
    expect(html).toContain('addEventListener("blur"');
    expect(html).toContain('e.key === "PrintScreen"');
    for (const suppressed of ["contextmenu", "dragstart", "selectstart", "copy"]) {
      expect(html).toContain('"' + suppressed + '"');
    }
  });

  it("keeps temporal slicing off by default and honours reduced motion", () => {
    const html = landingSource();
    // Checkbox ships unchecked.
    expect(html).toContain('<input type="checkbox" id="slice">');
    expect(html).not.toMatch(/id="slice"[^>]*checked/);
    expect(html).toContain('matchMedia("(prefers-reduced-motion: reduce)")');
    expect(html).toContain("sliceBox.disabled = true");
    // Explicitly labelled experimental, with a photosensitivity warning
    // shown before the first reveal.
    expect(html).toContain("experimental");
    expect(html).toContain("seizures");
    expect(html).toContain("window.confirm");
  });

  it("states plainly that this is deterrence, not screenshot prevention", () => {
    const html = landingSource();
    expect(html).toContain("deterrence, not prevention");
    expect(html).toContain("nothing here stops a screenshot, a screen recorder, or a phone camera");
    expect(html).toContain("Your screen isn't protected.");
    expect(html).toContain("the link dies after one view, or 60&nbsp;seconds, whichever is first");
  });

  it("uses no banned claim", () => {
    const html = landingSource().toLowerCase();
    for (const banned of [
      "screenshot-protected",
      "secure view",
      "cannot be saved",
      "disappears forever",
    ]) {
      expect(html).not.toContain(banned);
    }
  });

  it("publishes an abuse path and admits the accessibility cost", () => {
    const html = landingSource();
    expect(html).toContain("Report abuse: abuse@");
    expect(html).toContain("not readable by a screen reader");
  });
});
