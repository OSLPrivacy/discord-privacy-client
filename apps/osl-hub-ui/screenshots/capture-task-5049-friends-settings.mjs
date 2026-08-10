#!/usr/bin/env node

import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);
const OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-5049-friends-settings.png");
const VIEWPORT = { width: 1280, height: 1350 };

const server = await createViteServer({
  root: APP_ROOT,
  server: { host: "127.0.0.1", port: 0, strictPort: false },
  logLevel: "error",
});
await server.listen();
const address = server.httpServer.address();
if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP port");

const chrome = await launchChrome();
const page = await chrome.openPage();
try {
  await page.send("Emulation.setDeviceMetricsOverride", { ...VIEWPORT, deviceScaleFactor: 1, mobile: false });
  await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
  await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-5049-friends-settings-fixture.html`);
  const ready = await page.evaluate("document.documentElement.dataset.fixtureReady");
  if (ready !== "task-5049") throw new Error("friends Settings fixture did not become ready");
  const facts = JSON.parse(await page.evaluate(`JSON.stringify({
    lists: document.querySelectorAll("[data-friends-list]").length,
    accountRows: document.querySelectorAll("[data-visible-account]").length,
    discordRows: [...document.querySelectorAll("[data-visible-account]")].filter((row) => row.dataset.visibleAccount.startsWith("discord:")).length,
    selectedRules: document.querySelectorAll(".friends-settings-policy input:checked").length,
    mirrorChecked: document.querySelector('[data-friend-request-setting="autoMirrorNewFriends"]').checked,
    text: document.body.innerText.replace(/\\s+/g, " ").trim(),
  })`));
  if (facts.lists !== 2 || facts.accountRows !== 3 || facts.discordRows !== 2 || facts.selectedRules !== 2 || facts.mirrorChecked) {
    throw new Error(`surface facts refused: ${JSON.stringify(facts)}`);
  }
  for (const text of ["OSL friends", "OSL Chats contacts", "Who can add you", "Friend requests", "Accounts friends can see"]) {
    if (!facts.text.includes(text)) throw new Error(`visible surface is missing ${text}`);
  }
  const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
  const image = { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
  if (image.width !== VIEWPORT.width || image.height !== VIEWPORT.height || png.length < 20_000) {
    throw new Error(`capture is blank or wrong-sized: ${JSON.stringify({ ...image, bytes: png.length })}`);
  }
  mkdirSync(path.dirname(OUTPUT), { recursive: true });
  writeFileSync(OUTPUT, png);
  console.log(`TASK5049_CAPTURE lists=${facts.lists} account_rows=${facts.accountRows} discord_rows=${facts.discordRows} rules=${facts.selectedRules} mirror=${facts.mirrorChecked} png=${image.width}x${image.height} bytes=${png.length}`);
  console.log(`TASK5049_CAPTURE_PATH ${OUTPUT}`);
} finally {
  await page.close();
  await chrome.close();
  await server.close();
}
