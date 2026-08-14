#!/usr/bin/env node

import { mkdirSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);
const OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-5038-overlay-strip.png");
const WIDTH = 736;
const COMPOSER_HEIGHT = 58;
const STRIP_HEIGHT = 44;
const CAPTURE_WIDTH = 1200;
const CAPTURE_HEIGHT = 760;
const OVERLAY_LEFT = Math.floor((CAPTURE_WIDTH - WIDTH) / 2);
const OVERLAY_TOP = CAPTURE_HEIGHT - STRIP_HEIGHT - COMPOSER_HEIGHT - 36;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function pngDimensions(png) {
  assert(png.subarray(1, 4).toString("ascii") === "PNG", "TASK5038 capture is not PNG");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

const server = await createViteServer({
  root: APP_ROOT,
  server: { host: "127.0.0.1", port: 0, strictPort: false },
  logLevel: "error",
});
await server.listen();
const address = server.httpServer.address();
if (!address || typeof address === "string") throw new Error("Vite did not expose a port");

const chrome = await launchChrome();
const page = await chrome.openPage();
try {
  // Capture a real Discord Web window first. The local overlay is rendered in
  // a second navigation against these pixels so the evidence contains real
  // Discord chrome without needing a stored account or a synthetic fixture.
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: CAPTURE_WIDTH,
    height: CAPTURE_HEIGHT,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await page.navigate("https://discord.com/app");
  await new Promise((resolve) => setTimeout(resolve, 8_000));
  const discordRaw = await page.evaluate(`JSON.stringify({
    href: location.href,
    origin: location.origin,
    hostname: location.hostname,
    title: document.title,
    readyState: document.readyState,
    hasLoginSurface: document.body.innerText.includes("Welcome back!")
  })`);
  const discord = JSON.parse(discordRaw);
  assert(discord.hostname === "discord.com", `TASK5038 expected a real Discord window, got ${discordRaw}`);
  assert(discord.title === "Discord", `TASK5038 Discord window title mismatch: ${discordRaw}`);
  assert(discord.readyState === "complete", `TASK5038 Discord window did not finish loading: ${discordRaw}`);
  // Discord's anonymous login screen includes a short-lived QR credential.
  // It proves nothing about this task and must not be persisted in evidence.
  const redactedQrSurfaces = Number(await page.evaluate(`(() => {
    let count = 0;
    for (const qr of document.querySelectorAll('[role="img"][aria-label*="QR code"]')) {
      qr.style.visibility = "hidden";
      count += 1;
    }
    return count;
  })()`));
  assert(redactedQrSurfaces >= 1, "TASK5038 expected Discord's anonymous QR surface to redact");
  const discordPng = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
  const discordPngFacts = pngDimensions(discordPng);
  assert(discordPngFacts.width === CAPTURE_WIDTH && discordPngFacts.height === CAPTURE_HEIGHT,
    "TASK5038 Discord capture dimensions are wrong");
  assert(discordPng.length > 20_000, "TASK5038 real Discord window capture is blank");
  const discordSha256 = createHash("sha256").update(discordPng).digest("hex");

  await page.navigate(`http://127.0.0.1:${address.port}/overlay.html`);
  await new Promise((resolve) => setTimeout(resolve, 300));
  const discordBackground = `data:image/png;base64,${discordPng.toString("base64")}`;
  await page.evaluate(`(() => {
    document.documentElement.style.background = ${JSON.stringify(`url("${discordBackground}") center / cover no-repeat`)};
    document.body.style.background = "transparent";
    document.documentElement.dataset.nativeComposerCapture = "true";
    document.documentElement.style.setProperty("--osl-native-composer-aspect-ratio", "${WIDTH} / ${COMPOSER_HEIGHT}");
    const shell = document.querySelector(".overlay-shell");
    Object.assign(shell.style, {
      position: "fixed",
      left: "${OVERLAY_LEFT}px",
      top: "${OVERLAY_TOP}px",
      width: "${WIDTH}px",
      height: "${STRIP_HEIGHT + COMPOSER_HEIGHT}px",
    });
    ${process.env.OSL_TASK5038_HIDE_BAND === "1" ? 'document.querySelector("#osl-strip").style.display = "none";' : ""}
    return "ready";
  })()`);

  const raw = await page.evaluate(`(() => {
    const band = document.querySelector("#osl-strip");
    const strip = band.querySelector("[data-osl-strip]");
    const left = [...strip.querySelectorAll(".osl-strip__cluster--left [data-chip]")].map((node) => node.dataset.chip);
    const right = [...strip.querySelectorAll(".osl-strip__cluster--right [data-chip]")].map((node) => node.dataset.chip);
    const windowControls = [...strip.querySelectorAll("[data-window-control]")].map((node) => node.dataset.windowControl);
    const rect = band.getBoundingClientRect();
    return JSON.stringify({
      display: getComputedStyle(band).display,
      height: rect.height,
      top: rect.top,
      bottom: rect.bottom,
      logoCount: strip.querySelectorAll('[data-chip="home"]').length,
      nonLogoControlCount: strip.querySelectorAll('[data-chip]:not([data-chip="home"]), [data-window-control]').length,
      left,
      right,
      windowControls,
    });
  })()`);
  const facts = JSON.parse(raw);
  assert(facts.display !== "none", "TASK5038 carrier band is hidden");
  assert(facts.height === STRIP_HEIGHT, `TASK5038 expected 44px band, got ${facts.height}px`);
  assert(facts.top === OVERLAY_TOP && facts.bottom === OVERLAY_TOP + STRIP_HEIGHT,
    `TASK5038 band is not the overlay's top 44px: ${raw}`);
  assert(facts.logoCount === 1, `TASK5038 expected one logo, got ${facts.logoCount}`);
  assert(facts.nonLogoControlCount === 11, `TASK5038 expected eleven controls, got ${facts.nonLogoControlCount}`);
  assert(JSON.stringify(facts.left) === JSON.stringify(["home", "plan", "quick", "burn"]), `TASK5038 left cluster mismatch: ${raw}`);
  assert(JSON.stringify(facts.right) === JSON.stringify(["whitelist", "timer", "once", "lock", "eye"]), `TASK5038 right cluster mismatch: ${raw}`);
  assert(JSON.stringify(facts.windowControls) === JSON.stringify(["minimise", "maximise", "close"]), `TASK5038 window controls mismatch: ${raw}`);

  const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
  const pngFacts = pngDimensions(png);
  assert(pngFacts.width === CAPTURE_WIDTH && pngFacts.height === CAPTURE_HEIGHT, "TASK5038 capture dimensions are wrong");
  assert(png.length > discordPng.length / 2, "TASK5038 composed Discord/overlay capture is blank");
  mkdirSync(path.dirname(OUTPUT), { recursive: true });
  writeFileSync(OUTPUT, png);
  console.log(`TASK5038_DISCORD origin=${discord.origin} url=${discord.href} title=${discord.title} login_surface=${discord.hasLoginSurface} qr_surfaces_redacted=${redactedQrSurfaces} sha256=${discordSha256}`);
  console.log(`TASK5038_OVERLAY band_height=${facts.height} logo_count=${facts.logoCount} control_count=${facts.nonLogoControlCount}`);
  console.log(`TASK5038_OVERLAY left=${facts.left.join(",")} right=${facts.right.join(",")} window_controls=${facts.windowControls.join(",")}`);
  console.log(`TASK5038_OVERLAY capture=${OUTPUT} width=${pngFacts.width} height=${pngFacts.height} bytes=${png.length}`);
} finally {
  await page.close();
  await chrome.close();
  await server.close();
}
