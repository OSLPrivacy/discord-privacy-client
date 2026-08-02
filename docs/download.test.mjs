import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import test from "node:test";

const page = readFileSync(new URL("./download.html", import.meta.url), "utf8");

test("T11-T13 states the NSIS installer and unsigned beta status without claiming an available download", () => {
  execFileSync("node", ["scripts/check-claims.mjs"], { stdio: "pipe" });
  assert.match(page, /NSIS <code>\.exe<\/code> installer/);
  assert.match(page, /not code signed yet/i);
  assert.match(page, /Coming soon/i);
  assert.doesNotMatch(page, /href=["']https?:\/\/[^"']+\.exe/i);
});

test("T11-T13 sabotage: a signed-installer claim is forbidden until a live proof exists", () => {
  const dishonest = page.replace("not code signed yet", "a signed installer");
  assert.doesNotMatch(dishonest, /not code signed yet/i);
  assert.match(dishonest, /signed installer/i);
});
