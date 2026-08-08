#!/usr/bin/env node
/** TASK 1508: restore the Download Pro button only when its checkout handoff is healthy. */

export function checkoutHandoffHealth(fixture) {
  if (fixture && fixture.ok) {
    if (typeof fixture.url !== "string" || !fixture.url) {
      throw new Error("a healthy checkout fixture must include a url");
    }
    return { healthy: true, url: fixture.url, reason: null };
  }
  const reason = (fixture && fixture.reason) || "checkout handoff unavailable";
  return { healthy: false, url: null, reason };
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);
}

/**
 * Renders the Download Pro purchase control from a checkout handoff health
 * result. Unhealthy handoffs render zero buttons plus the refusal reason;
 * healthy handoffs render exactly one button wired to `openCheckout`.
 */
export function renderDownloadProButton(health, openCheckout) {
  if (!health.healthy) {
    return {
      html: `<p class="checkout-unavailable" data-checkout-buttons="0">Pro purchase is unavailable: ${escapeHtml(health.reason)}</p>`,
      buttonCount: 0,
      reason: health.reason,
    };
  }
  return {
    html: `<button type="button" class="button primary" id="download-pro-button" data-checkout-buttons="1" data-checkout-url="${escapeHtml(health.url)}">Download Pro</button>`,
    buttonCount: 1,
    open: () => openCheckout(health.url),
  };
}

if (import.meta.main) {
  console.log("Usage: import { checkoutHandoffHealth, renderDownloadProButton } from \"./download-pro-button.mjs\"");
}
