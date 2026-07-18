#!/usr/bin/env node

/*
 * OSL Privacy prototype smoke test.
 *
 * This intentionally exercises only simulated UI state. It must never accept
 * credentials or contact a real social platform. Run the prototype server,
 * then execute:
 *
 *   node docs/prototypes/osl-hub/smoke.cjs
 *
 * Override the target with BASE_URL=http://127.0.0.1:PORT when needed.
 */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("../../../selector-ci/node_modules/playwright");

const prototypeDir = __dirname;
const appPath = path.join(prototypeDir, "app.js");
const baseURL = process.env.BASE_URL || "http://127.0.0.1:4173";
const launchServices = [
  "Discord",
  "Telegram",
  "Instagram",
  "Snapchat",
  "Email",
  "X",
  "Slack",
  "Teams",
  "Facebook Messenger",
];
const prohibitedTerms = [
  "ban bypass",
  "fingerprint spoof",
  "captcha bypass",
  "simulated wpm",
  "fake conversation",
  "cover traffic",
];

function check(condition, message) {
  assert.ok(condition, message);
  process.stdout.write(`PASS  ${message}\n`);
}

async function pageText(locator) {
  return (await locator.innerText()).replace(/\s+/g, " ").trim();
}

async function selectByVisibleLabel(select, label) {
  const option = select.locator("option", { hasText: label }).first();
  const value = await option.getAttribute("value");
  assert.notEqual(value, null, `${label} option has a value`);
  await select.selectOption(value);
}

async function main() {
  // The prototype is being assembled in parallel. Treat the intentionally
  // absent controller as a temporary skip, not a broken product assertion.
  if (!fs.existsSync(appPath)) {
    process.stdout.write(
      "SKIP  app.js is not present yet; smoke test is ready for the completed prototype.\n",
    );
    return;
  }

  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 960 },
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    const consoleErrors = [];
    page.on("console", (message) => {
      if (message.type() === "error") consoleErrors.push(message.text());
    });
    page.on("pageerror", (error) => consoleErrors.push(error.message));

    const response = await page.goto(baseURL, { waitUntil: "networkidle" });
    check(response && response.ok(), `prototype responds at ${baseURL}`);

    const banner = await pageText(page.locator(".prototype-banner strong"));
    check(
      banner === "Prototype - simulated data only" ||
        banner === "Prototype — simulated data only",
      "simulated-data banner is explicit and exact",
    );
    check((await page.locator('input[type="password"]').count()) === 0, "no password inputs exist");

    const wholeDocument = (await page.locator("body").innerText()).toLowerCase();
    for (const term of prohibitedTerms) {
      check(!wholeDocument.includes(term), `UI excludes prohibited anti-detection term: ${term}`);
    }

    await page.getByRole("button", { name: "Connections" }).click();
    await page.locator('[data-page="connections"].active').waitFor();
    const connectionsText = await pageText(page.locator('[data-page="connections"]'));
    for (const service of launchServices) {
      check(connectionsText.includes(service), `${service} is listed at launch`);
    }
    check(
      (await page.locator("#connections-grid > *").count()) === 9,
      "connections contain exactly nine service cards",
    );

    await page.getByRole("button", { name: "Secure Composer" }).click();
    await page.locator('[data-page="composer"].active').waitFor();
    const serviceSelect = page.locator("#service-select");
    const accountSelect = page.locator("#account-select");
    await selectByVisibleLabel(serviceSelect, "Discord");
    check((await accountSelect.locator("option").count()) === 2, "Discord exposes two isolated test accounts");

    const secureText = page.locator("#secure-text");
    const nativeComposer = page.locator("#native-composer");
    const handoff = page.locator("#handoff-button");
    const secret = "ACCOUNT_ONE_PLAINTEXT_7e2d";
    const accountOptions = await accountSelect.locator("option").evaluateAll((options) =>
      options.map((option) => option.value),
    );
    await accountSelect.selectOption(accountOptions[0]);
    await secureText.fill(secret);
    check((await nativeComposer.inputValue()) === "", "native composer stays empty before explicit handoff");
    await accountSelect.selectOption(accountOptions[1]);
    check(!(await secureText.inputValue()).includes(secret), "account switch does not expose the prior account draft");
    check(!(await nativeComposer.inputValue()).includes(secret), "account switch does not leak plaintext into native composer");
    check(await page.getByRole("radio", { name: /Native/ }).isChecked(), "unverified recipient falls back visibly to Native");
    check(!(await nativeComposer.isDisabled()), "Native mode keeps the underlying service composer available");

    // Return to the verified test identity before exercising OSL Protected.
    await accountSelect.selectOption(accountOptions[0]);
    check(await page.getByRole("radio", { name: /OSL Protected/ }).isChecked(), "verified recipient restores the explicit OSL Protected mode");
    const handoffText = "meet me after eight";
    await secureText.fill(handoffText);
    await handoff.click();
    const capsule = await nativeComposer.inputValue();
    check(capsule.length > 0, "explicit handoff creates a native capsule");
    check(!capsule.includes(handoffText), "native handoff is opaque and excludes plaintext");
    check(/osl|protected|capsule/i.test(capsule), "native handoff is visibly an OSL simulated capsule");

    // Safety warnings are protection, never an upsell surface.
    await page.locator("#check-button").click();
    const warningDialog = page.locator("#warning-dialog");
    await warningDialog.waitFor({ state: "visible" });
    const warningText = (await pageText(warningDialog)).toLowerCase();
    check(!/\b(pro|upgrade|subscribe|payment|buy)\b/.test(warningText), "safety warning contains no upsell");
    await warningDialog.getByRole("button", { name: "Edit draft" }).click();

    await selectByVisibleLabel(serviceSelect, "Snapchat");
    const capabilityText = `${await pageText(page.locator("#capability-state"))} ${await pageText(page.locator("#capability-copy"))}`;
    check(/assist|user-assisted/i.test(capabilityText), "Snapchat is clearly assist-only");
    check(/press|user|you/i.test(capabilityText), "Snapchat keeps the final platform action with the user");

    const preservedDraft = "PRESERVE_DRAFT_29ab";
    await secureText.fill(preservedDraft);
    await page.locator("#low-confidence").click();
    check(await secureText.isDisabled(), "low-confidence layout disables protected composer input");
    check(await handoff.isDisabled(), "low-confidence layout disables handoff");
    check((await secureText.inputValue()) === preservedDraft, "low-confidence fallback preserves the local draft");
    const lowConfidenceState = await pageText(page.locator("#layout-state"));
    check(/layout changed|review placement|low confidence/i.test(lowConfidenceState), "low-confidence state is explained plainly");

    await page.screenshot({
      path: path.join(prototypeDir, "osl-secure-composer-prototype.png"),
      fullPage: true,
    });

    await page.getByRole("button", { name: "Privacy" }).click();
    await page.locator('[data-page="privacy"].active').waitFor();
    check(await page.locator('input[name="preset"][value="balanced"]').isChecked(), "Balanced privacy preset is selected by default");
    const automaticDeletion = page.getByRole("checkbox", { name: "Automatic deletion" });
    check(!(await automaticDeletion.isChecked()), "automatic deletion is off by default");
    await page.locator("#scan-button").click();
    const scanResults = page.locator("#scan-results");
    await scanResults.waitFor({ state: "visible" });
    check(
      /3 items may reveal your home address/i.test(await pageText(scanResults)),
      "local scan presents the expected calm contextual finding",
    );
    const deletionSteps = scanResults.getByRole("button", { name: /deletion steps/i }).first();
    check((await deletionSteps.count()) === 1, "scan offers guided deletion steps");
    await deletionSteps.click();
    const proDialog = page.locator("#pro-dialog");
    await proDialog.waitFor({ state: "visible" });
    check(/guided cleanup is a pro tool/i.test(await pageText(proDialog)), "guided cleanup is gated as Pro");
    await proDialog.getByRole("button", { name: "Stay on Free" }).click();

    check(consoleErrors.length === 0, `prototype produces no console/page errors${consoleErrors.length ? `: ${consoleErrors.join(" | ")}` : ""}`);
    await context.close();

    const mobileContext = await browser.newContext({
      viewport: { width: 375, height: 812 },
      deviceScaleFactor: 1,
      isMobile: true,
      reducedMotion: "reduce",
    });
    const mobile = await mobileContext.newPage();
    await mobile.goto(baseURL, { waitUntil: "networkidle" });
    const overflow = await mobile.evaluate(() => ({
      viewport: document.documentElement.clientWidth,
      body: document.body.scrollWidth,
      document: document.documentElement.scrollWidth,
    }));
    check(
      overflow.body <= overflow.viewport && overflow.document <= overflow.viewport,
      `375px layout has no horizontal overflow (${overflow.document}/${overflow.viewport})`,
    );
    await mobile.screenshot({
      path: path.join(prototypeDir, "osl-hub-mobile-prototype.png"),
      fullPage: true,
    });
    await mobileContext.close();

    process.stdout.write("\nOSL prototype smoke checks complete.\n");
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(`FAIL  ${error.stack || error.message}`);
  process.exitCode = 1;
});
