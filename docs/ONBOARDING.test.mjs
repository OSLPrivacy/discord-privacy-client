import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const documentPath = new URL("./ONBOARDING.md", import.meta.url);
const document = readFileSync(documentPath, "utf8");

// This is the verified route inventory in the shipped onboarding renderer.
// Keep this list in lockstep with onboarding-sequence.ts when that shared route
// data lands; this test keeps the documentation honest in the meantime.
const builtRoutes = [
  "welcome", "create", "import", "unlock", "recovery", "pro", "privacy",
  "defaults", "sending", "cover", "passwords", "burnpass", "mullvad",
  "browser", "tutorial", "detected", "install", "apps", "decoy",
];

test("documents every built onboarding route", () => {
  const documentedRoutes = [...document.matchAll(/^\| `([a-z]+)` \|/gmu)]
    .map((match) => match[1])
    .sort();

  assert.deepEqual(documentedRoutes, [...builtRoutes].sort());
});

test("does not describe the retired Discord-login webview flow", () => {
  assert.doesNotMatch(document, /Discord login/u);
  assert.doesNotMatch(document, /`webview\//u);
});
