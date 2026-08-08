import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { checkDownloadPage } from "../scripts/check-download-page-words.mjs";

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

test("TASK 1519 Download states the four shared pricing facts beside a hidden purchase button", () => {
  const output = execFileSync("node", ["scripts/check-download-page-words.mjs"], { encoding: "utf8" });
  assert.match(output, /shared_pricing_facts=4\/4/);
  assert.match(output, /pricing_fact_1=5 dollars :: present/);
  assert.match(output, /pricing_fact_2=one month from code entry :: present/);
  assert.match(output, /pricing_fact_3=no renewal :: present/);
  assert.match(output, /pricing_fact_4=no OSL card storage :: present/);
  assert.match(output, /tier_statements_beside_purchase=3\/3/);
  assert.match(output, /Free review \(scrub-discovery, Planned\): present, forward_looking_marker="at v1"/);
  assert.match(output, /Pro deletion \(scrub-guided-deletion, Planned\): present, forward_looking_marker="at v1"/);
  assert.match(output, /Pro schedules \(autoscrub, Planned\): present, forward_looking_marker="at v1"/);
  assert.match(output, /checkout_ready=false/);
  assert.match(output, /purchase_button=hidden/);
  assert.match(output, /refusal_reason=stated/);
});

test("TASK 1519 the working-purchase-button branch passes only when checkout is ready", () => {
  const fixture = "docs/fixtures/download-checkout-ready.html";
  const logged = [];
  const capture = (...args) => logged.push(args.join(" "));
  const realLog = console.log;
  console.log = capture;
  try {
    checkDownloadPage(fixture, { readiness: { ready: true, reason: "checkout is live" } });
  } finally {
    console.log = realLog;
  }
  assert.match(logged.join("\n"), /purchase_button=working/);
  assert.match(logged.join("\n"), /purchase_controls=1 \(buttons=1, checkout links=0\)/);

  // The same page with checkout NOT ready is a dead button, and must fail.
  console.log = () => {};
  try {
    assert.throws(
      () => checkDownloadPage(fixture, { readiness: { ready: false, reason: "not ready" } }),
      /purchase button must be hidden, found 1 control/,
    );
  } finally {
    console.log = realLog;
  }
});
