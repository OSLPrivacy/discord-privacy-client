import assert from "node:assert/strict";
import test from "node:test";
import {
  CHECKOUT_UNHEALTHY,
  countPaymentAddresses,
  countPurchaseControls,
  createDownloadPageLoader,
  gateViolations,
  readDownloadPage,
  runCheckoutHealthGate,
} from "./download-pro-page.mjs";

const SESSION_TOKEN = "checkout-maple";
const CHECKOUT_URL = `https://checkout.oslprivacy.com/session/${SESSION_TOKEN}`;
/** The only thing that changes between the two loads is checkout health. */
const HEALTHY_PROBE = { ok: true, url: CHECKOUT_URL };
const FAILED_PROBE = { ok: false, reason: "checkout session endpoint returned 503" };

const basePage = readDownloadPage();

test("TASK 1509 loading Download with checkout health healthy, then failed, holds the gate", () => {
  const run = runCheckoutHealthGate(createDownloadPageLoader({ basePage }), {
    healthyProbe: HEALTHY_PROBE,
    failedProbe: FAILED_PROBE,
  });

  // healthy: count 0 before, 1 after, page names Download Pro and checkout-maple
  assert.equal(run.countBefore, 0);
  assert.equal(run.countAfterHealthy, 1);
  assert.equal(run.healthy.ok, true);
  assert.ok(run.healthy.html.includes("Download Pro"));
  assert.ok(run.healthy.html.includes(SESSION_TOKEN));
  assert.equal(countPurchaseControls(run.healthy.html), 1);
  assert.equal(countPaymentAddresses(run.healthy.html), 1);

  // failed: refused as checkout unhealthy, 0 purchase controls, 0 payment addresses
  assert.equal(run.failed.ok, false);
  assert.equal(run.failed.refusal, CHECKOUT_UNHEALTHY);
  assert.equal(run.failed.purchaseControls, 0);
  assert.equal(run.failed.paymentAddresses, 0);
  assert.equal(countPurchaseControls(run.failed.html), 0);
  assert.equal(countPaymentAddresses(run.failed.html), 0);
  assert.doesNotMatch(run.failed.html, /checkout-maple/);
  assert.doesNotMatch(run.failed.html, /https:\/\//);
  assert.match(run.failed.html, /Refused: checkout unhealthy/);

  // nothing but checkout health changed: both loads rendered the same base page
  const stripPro = (html) => html.replace(/ {4}<section id="download-pro"[\s\S]*?<\/section>\n/, "");
  assert.equal(stripPro(run.failed.html), basePage);
  assert.equal(stripPro(run.healthy.html), basePage);

  // the saved healthy result survives the failed load unchanged
  assert.notEqual(run.savedAfterFailure, null);
  assert.ok(run.savedAfterFailure.html.includes("Download Pro"));
  assert.ok(run.savedAfterFailure.html.includes(SESSION_TOKEN));
  assert.equal(run.savedAfterFailure.renderCount, 1);
  assert.equal(run.savedAfterFailure.html, run.healthy.html);
  assert.equal(run.countAfterFailure, 1);

  assert.deepEqual(gateViolations(run, { sessionToken: SESSION_TOKEN }), []);

  console.log(
    `task1509 count_before=${run.countBefore} count_after_healthy=${run.countAfterHealthy} ` +
      `healthy_names_pro=${run.healthy.html.includes("Download Pro")} healthy_names_session=${run.healthy.html.includes(SESSION_TOKEN)} ` +
      `failed_refusal="${run.failed.refusal}" failed_purchase_controls=${run.failed.purchaseControls} ` +
      `failed_payment_addresses=${run.failed.paymentAddresses} ` +
      `saved_names_pro=${run.savedAfterFailure.html.includes("Download Pro")} ` +
      `saved_names_session=${run.savedAfterFailure.html.includes(SESSION_TOKEN)} ` +
      `saved_count=${run.savedAfterFailure.renderCount} violations=${gateViolations(run, { sessionToken: SESSION_TOKEN }).length}`,
  );
});

test("TASK 1509 the saved healthy result cannot be mutated through the handle it hands out", () => {
  const loader = createDownloadPageLoader({ basePage });
  loader.load(HEALTHY_PROBE);
  const stolen = loader.savedRender();
  stolen.html = "<h2>Download Pro</h2>";
  stolen.renderCount = 99;
  loader.load(FAILED_PROBE);
  assert.equal(loader.savedRender().renderCount, 1);
  assert.ok(loader.savedRender().html.includes(SESSION_TOKEN));
});

test("TASK 1509 an ok probe without a usable https checkout address is failed, not healthy", () => {
  const loader = createDownloadPageLoader({ basePage });
  for (const probe of [{ ok: true }, { ok: true, url: "http://checkout.example/session/checkout-maple" }, { ok: true, url: "" }]) {
    const result = loader.load(probe);
    assert.equal(result.ok, false);
    assert.equal(result.refusal, CHECKOUT_UNHEALTHY);
    assert.equal(result.purchaseControls, 0);
    assert.equal(result.paymentAddresses, 0);
  }
  assert.equal(loader.renderCount(), 0);
});

/** A loader that refuses in words but re-serves the cached healthy page. */
function staleCacheLoader() {
  const real = createDownloadPageLoader({ basePage });
  return {
    renderCount: real.renderCount,
    savedRender: real.savedRender,
    load(probe) {
      const result = real.load(probe);
      const saved = real.savedRender();
      if (result.ok === false && saved !== null) {
        return { ...result, html: saved.html, purchaseControls: countPurchaseControls(saved.html), paymentAddresses: countPaymentAddresses(saved.html) };
      }
      return result;
    },
  };
}

/** A loader that counts a refused load as a render and re-saves it. */
function countsRefusalsLoader() {
  const real = createDownloadPageLoader({ basePage });
  let extra = 0;
  return {
    renderCount: () => real.renderCount() + extra,
    savedRender: () => {
      const saved = real.savedRender();
      return saved === null ? null : { ...saved, renderCount: saved.renderCount + extra };
    },
    load(probe) {
      const result = real.load(probe);
      if (result.ok === false) extra += 1;
      return result;
    },
  };
}

test("TASK 1509 sabotage: a stale cached page served after checkout health fails is caught", () => {
  const run = runCheckoutHealthGate(staleCacheLoader(), { healthyProbe: HEALTHY_PROBE, failedProbe: FAILED_PROBE });
  assert.equal(run.failed.purchaseControls, 1);
  assert.equal(run.failed.paymentAddresses, 1);
  assert.ok(run.failed.html.includes(SESSION_TOKEN));
  const violations = gateViolations(run, { sessionToken: SESSION_TOKEN });
  assert.deepEqual(violations, [
    "the refused load shows 1 purchase controls, not 0",
    "the refused load shows 1 payment addresses, not 0",
  ]);
  console.log(`task1509 sabotage_stale_cache violations=${violations.length} :: ${violations.join(" | ")}`);
});

test("TASK 1509 sabotage: counting the refused load as a render is caught", () => {
  const run = runCheckoutHealthGate(countsRefusalsLoader(), { healthyProbe: HEALTHY_PROBE, failedProbe: FAILED_PROBE });
  const violations = gateViolations(run, { sessionToken: SESSION_TOKEN });
  assert.deepEqual(violations, [
    "the saved healthy result count is 2, not 1",
    "the render count after the failed load is 2, not 1",
  ]);
  console.log(`task1509 sabotage_counts_refusals violations=${violations.length} :: ${violations.join(" | ")}`);
});
