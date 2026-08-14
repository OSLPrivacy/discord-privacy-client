import assert from "node:assert/strict";
import test from "node:test";
import { checkoutHandoffHealth, renderDownloadProButton } from "./download-pro-button.mjs";

test("TASK 1508 the healthy fixture opens checkout exactly 1 time", () => {
  const health = checkoutHandoffHealth({ ok: true, url: "https://checkout.oslprivacy.com/session/checkout-maple" });
  const view = renderDownloadProButton(health, () => {});
  assert.equal(view.buttonCount, 1);
  assert.match(view.html, /Download Pro/);
  assert.match(view.html, /checkout-maple/);

  let opens = 0;
  const opened = renderDownloadProButton(health, () => { opens += 1; });
  opened.open();
  assert.equal(opens, 1);
});

test("TASK 1508 the unhealthy health check shows 0 checkout buttons with its reason", () => {
  const health = checkoutHandoffHealth({ ok: false, reason: "checkout session endpoint returned 503" });
  const view = renderDownloadProButton(health, () => {
    throw new Error("must not open checkout when unhealthy");
  });
  assert.equal(view.buttonCount, 0);
  assert.doesNotMatch(view.html, /<button/);
  assert.match(view.html, /checkout session endpoint returned 503/);
});

test("TASK 1508 sabotage: an unhealthy handoff must not be able to open checkout", () => {
  const health = checkoutHandoffHealth({ ok: false, reason: "STRIPE_SECRET_KEY not configured" });
  assert.equal(health.healthy, false);
  assert.equal(health.url, null);
  const view = renderDownloadProButton(health, () => {
    throw new Error("must not open checkout when unhealthy");
  });
  assert.equal(view.open, undefined);
});
