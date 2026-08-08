#!/usr/bin/env node
/**
 * TASK 1509: the checkout health gate for the Download page.
 *
 * TASK 1508 (lane m) restored the Pro purchase control only when the checkout
 * handoff is healthy. This module is the page-load side of that gate: a load
 * of the Download page consults checkout health *at load time*, and a load
 * made while checkout health has failed is refused outright — it may not fall
 * back to, re-serve, or otherwise leak the purchase control or the payment
 * address from an earlier healthy load, and it does not count as a render.
 */
import { readFileSync } from "node:fs";

export const CHECKOUT_UNHEALTHY = "checkout unhealthy";

const PRO_HEADING = "Download Pro";
/** A payment address: any checkout URL, or a bare checkout session token. */
const PAYMENT_ADDRESS = /https?:\/\/[^\s"'<>]*checkout[^\s"'<>]*|(?<![\w-])checkout-[a-z0-9]+(?:-[a-z0-9]+)*/gi;
/** A purchase control: anything the reader can press to start a payment. */
const PURCHASE_CONTROL = /<button\b|data-purchase-control="1"/gi;

export function countPaymentAddresses(html) {
  return (html.match(PAYMENT_ADDRESS) ?? []).length;
}

export function countPurchaseControls(html) {
  return (html.match(PURCHASE_CONTROL) ?? []).length;
}

/** Turn a checkout-handoff probe into a health result. Anything that is not a
 *  live https checkout address is failed, never "probably fine". */
export function checkoutHealth(probe) {
  if (!probe || probe.ok !== true) {
    return { state: "failed", reason: probe?.reason ?? "checkout handoff did not report healthy" };
  }
  const url = probe.url;
  if (typeof url !== "string" || !/^https:\/\/[^\s"'<>]+$/.test(url)) {
    return { state: "failed", reason: "checkout handoff returned no usable https checkout address" };
  }
  return { state: "healthy", url };
}

export function renderProSection(health) {
  if (health.state !== "healthy") throw new Error(`renderProSection requires healthy checkout, got ${health.state}`);
  return [
    `    <section id="download-pro" aria-labelledby="download-pro-heading" data-checkout-health="healthy">`,
    `      <h2 id="download-pro-heading">${PRO_HEADING}</h2>`,
    `      <p>OSL Hub Pro is 5 dollars for one month, started when you enter the code, with no renewal.</p>`,
    `      <a class="purchase-control" data-purchase-control="1" href="${health.url}">Buy ${PRO_HEADING}</a>`,
    `    </section>`,
  ].join("\n");
}

export function renderProRefusal(reason) {
  return [
    `    <section id="download-pro" aria-labelledby="download-pro-heading" data-checkout-health="failed">`,
    `      <h2 id="download-pro-heading">Pro purchase unavailable</h2>`,
    `      <p data-refusal="${CHECKOUT_UNHEALTHY}">Refused: ${CHECKOUT_UNHEALTHY}. ${reason}</p>`,
    `    </section>`,
  ].join("\n");
}

function withSection(basePage, section) {
  const marker = "  </main>";
  if (!basePage.includes(marker)) throw new Error("base page has no </main> to place the Pro section before");
  return basePage.replace(marker, `${section}\n${marker}`);
}

/**
 * A Download page loader that keeps the last healthy render and a render
 * count. `load` re-reads checkout health every time; only a healthy load
 * renders the page, counts, and is saved.
 */
export function createDownloadPageLoader({ basePage }) {
  let renders = 0;
  let saved = null;

  return {
    renderCount: () => renders,
    savedRender: () => (saved === null ? null : { ...saved }),
    load(probe) {
      const health = checkoutHealth(probe);
      if (health.state !== "healthy") {
        const html = withSection(basePage, renderProRefusal(health.reason));
        return {
          ok: false,
          refusal: CHECKOUT_UNHEALTHY,
          reason: health.reason,
          html,
          purchaseControls: countPurchaseControls(html),
          paymentAddresses: countPaymentAddresses(html),
          renderCount: renders,
        };
      }
      const html = withSection(basePage, renderProSection(health));
      renders += 1;
      saved = {
        html,
        url: health.url,
        renderCount: renders,
        purchaseControls: countPurchaseControls(html),
        paymentAddresses: countPaymentAddresses(html),
      };
      return { ok: true, html, url: health.url, ...saved };
    },
  };
}

/**
 * Drive one loader through the whole task: read the count, load healthy, then
 * change *only* checkout health to failed and load again, then re-read what
 * was saved. Returns the observations, so the same run can be replayed against
 * a deliberately broken loader.
 */
export function runCheckoutHealthGate(loader, { healthyProbe, failedProbe }) {
  const countBefore = loader.renderCount();
  const healthy = loader.load(healthyProbe);
  const countAfterHealthy = loader.renderCount();
  const failed = loader.load(failedProbe);
  const savedAfterFailure = loader.savedRender();
  return { countBefore, healthy, countAfterHealthy, failed, savedAfterFailure, countAfterFailure: loader.renderCount() };
}

/** The finish line as data: every violation the run produced, empty if clean. */
export function gateViolations(run, { sessionToken }) {
  const violations = [];
  const names = (html) => html.includes(PRO_HEADING) && html.includes(sessionToken);

  if (run.countBefore !== 0) violations.push(`render count before the healthy load is ${run.countBefore}, not 0`);
  if (run.healthy.ok !== true) violations.push("the healthy load was not served");
  if (run.countAfterHealthy !== 1) violations.push(`render count after the healthy load is ${run.countAfterHealthy}, not 1`);
  if (!names(run.healthy.html ?? "")) violations.push(`the healthy page does not name both ${PRO_HEADING} and ${sessionToken}`);

  if (run.failed.ok !== false) violations.push("the failed load was served instead of refused");
  if (run.failed.refusal !== CHECKOUT_UNHEALTHY) violations.push(`the failed load was refused as ${run.failed.refusal}, not ${CHECKOUT_UNHEALTHY}`);
  if (run.failed.purchaseControls !== 0) violations.push(`the refused load shows ${run.failed.purchaseControls} purchase controls, not 0`);
  if (run.failed.paymentAddresses !== 0) violations.push(`the refused load shows ${run.failed.paymentAddresses} payment addresses, not 0`);

  const saved = run.savedAfterFailure;
  if (saved === null) violations.push("the saved healthy result was discarded by the failed load");
  else {
    if (!names(saved.html ?? "")) violations.push(`the saved healthy result no longer names both ${PRO_HEADING} and ${sessionToken}`);
    if (saved.renderCount !== 1) violations.push(`the saved healthy result count is ${saved.renderCount}, not 1`);
  }
  if (run.countAfterFailure !== 1) violations.push(`the render count after the failed load is ${run.countAfterFailure}, not 1`);
  return violations;
}

export function readDownloadPage(url = new URL("../docs/download.html", import.meta.url)) {
  return readFileSync(url, "utf8");
}
