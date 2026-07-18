import assert from "node:assert/strict";
import test from "node:test";
import { BASE_STATE, PRESETS, buildLogoSvg, cssVariables, logoMetrics, safeRandomState, sanitizeState, validatePaths } from "../src/model.js";
import { isAllowedHost, resolveRoute } from "../server.mjs";

const fixture = [
  { d: "M0 0 C10 0 20 10 20 20 C10 20 0 10 0 0 Z", transform: "translate(1,2)" },
  { d: "M2 2 C8 2 14 8 14 14 C8 14 2 8 2 2 Z", transform: "translate(3,4)" },
];

test("state stays inside reviewed safe bounds", () => {
  const state = sanitizeState({ size: 9999, padding: -9, stroke: 80, color: "red", shield: -999, glow: 90, shadow: -2, wordmarkSpacing: 400, context: "remote" });
  assert.equal(state.size, 560);
  assert.equal(state.padding, 0);
  assert.equal(state.stroke, 10);
  assert.equal(state.color, BASE_STATE.color);
  assert.equal(state.shield, -100);
  assert.equal(state.glow, 40);
  assert.equal(state.shadow, 0);
  assert.equal(state.wordmarkSpacing, 100);
  assert.equal(state.context, "dark");
});

test("all required presets produce finite exact SVGs", () => {
  assert.deepEqual(Object.keys(PRESETS), ["homepage", "desktop", "website", "favicon"]);
  for (const [name, state] of Object.entries(PRESETS)) {
    const svg = buildLogoSvg(state, fixture);
    const metrics = logoMetrics(state);
    assert.match(svg, /^<svg xmlns=/);
    assert.match(svg, new RegExp(`width="${metrics.width}" height="${metrics.height}"`));
    assert.equal(svg.includes("<text "), state.wordmark, name);
    assert.doesNotMatch(svg, /NaN|Infinity|<script|<foreignObject|<image|onload=/i);
  }
});

test("random-safe variants never leave the design envelope", () => {
  for (let index = 0; index < 30; index += 1) {
    const state = safeRandomState(() => (index % 29) / 28);
    assert.ok(state.size >= 48 && state.size <= 560);
    assert.ok(state.shield >= -100 && state.shield <= 100);
    assert.match(state.color, /^#[0-9a-f]{6}$/);
    assert.doesNotThrow(() => buildLogoSvg(state, fixture));
  }
});

test("source path contract rejects added or malformed artwork", () => {
  assert.throws(() => validatePaths([fixture[0]]));
  assert.throws(() => validatePaths([...fixture, fixture[0]]));
  assert.throws(() => validatePaths([{ d: "x", transform: "" }, fixture[1]]));
});

test("CSS export contains only bounded local design variables", () => {
  const css = cssVariables(PRESETS.website);
  assert.match(css, /--osl-logo-color: #06b6d4/);
  assert.match(css, /--osl-logo-size: 160px/);
  assert.doesNotMatch(css, /url\(|@import|https?:/);
});

test("localhost server exposes only explicit lab assets", () => {
  assert.ok(resolveRoute("/"));
  assert.ok(resolveRoute("/source/logo-mark.svg"));
  assert.equal(resolveRoute("/../../.git/config"), null);
  assert.equal(resolveRoute("/%2e%2e/%2e%2e/.env"), null);
  assert.equal(resolveRoute("/unknown"), null);
  assert.equal(isAllowedHost("127.0.0.1:4177"), true);
  assert.equal(isAllowedHost("localhost:4177"), true);
  assert.equal(isAllowedHost("example.com"), false);
  assert.equal(isAllowedHost("127.0.0.1.example.com"), false);
});
