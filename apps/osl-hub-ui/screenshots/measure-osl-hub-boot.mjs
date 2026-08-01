#!/usr/bin/env node

/**
 * Measure the browser-visible portion of OSL boot without adding a browser
 * dependency.  The Tauri window handle is measured separately by
 * measure-osl-hub-startup.ps1; this script begins when the rendered document
 * becomes visible to the engine and reports the four intervals that follow.
 *
 * Usage:
 *   node screenshots/measure-osl-hub-boot.mjs --url http://127.0.0.1:4173/
 *
 * Pass a URL served by the built UI (or a debug WebView endpoint).  The page
 * must ultimately render #route-heading; a timeout is an intentional failure,
 * because a timeline ending at a loading screen is not a boot measurement.
 */

import { fileURLToPath } from "node:url";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const MARKS = [
  "windowVisibleMs",
  "firstPaintMs",
  "firstContentfulPaintMs",
  "bootstrapAssignmentMs",
  "routePaintedMs",
];

export function bootIntervals(timeline) {
  const values = MARKS.map((mark) => timeline[mark]);
  if (!values.every((value) => Number.isFinite(value) && value >= 0)) {
    throw new Error("boot timeline is missing a finite, non-negative mark");
  }
  for (let index = 1; index < values.length; index += 1) {
    if (values[index] < values[index - 1]) {
      throw new Error(`${MARKS[index]} preceded ${MARKS[index - 1]}`);
    }
  }
  return {
    window_visible_to_first_paint_ms: values[1] - values[0],
    first_paint_to_first_contentful_paint_ms: values[2] - values[1],
    first_contentful_paint_to_bootstrap_assignment_ms: values[3] - values[2],
    bootstrap_assignment_to_route_painted_ms: values[4] - values[3],
  };
}

export function emittedBootTimeline(timeline) {
  return {
    marks_ms: Object.fromEntries(MARKS.map((mark) => [mark, timeline[mark]])),
    intervals_ms: bootIntervals(timeline),
  };
}

function parseArgs(args) {
  const urlIndex = args.indexOf("--url");
  if (urlIndex === -1 || !args[urlIndex + 1]) {
    throw new Error("usage: node screenshots/measure-osl-hub-boot.mjs --url <built-ui-url>");
  }
  const timeoutIndex = args.indexOf("--timeout-ms");
  const timeoutMs = timeoutIndex === -1 ? 15_000 : Number(args[timeoutIndex + 1]);
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new Error("--timeout-ms must be a positive number");
  return { url: args[urlIndex + 1], timeoutMs };
}

// Installed before navigation, so this observes runtime behaviour rather than
// inspecting main.ts.  The innerHTML setter identifies bootstrap's first write
// to #app and the later route render; two animation frames after the route DOM
// exists is the first frame in which it can be painted.
function installTimelineProbe() {
  const timeline = window.__oslBootTimeline = {
    windowVisibleMs: 0,
    firstPaintMs: null,
    firstContentfulPaintMs: null,
    bootstrapAssignmentMs: null,
    routePaintedMs: null,
  };
  const now = () => performance.now();
  new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      if (entry.name === "first-paint" && timeline.firstPaintMs === null) timeline.firstPaintMs = entry.startTime;
      if (entry.name === "first-contentful-paint" && timeline.firstContentfulPaintMs === null) timeline.firstContentfulPaintMs = entry.startTime;
    }
  }).observe({ type: "paint", buffered: true });

  const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, "innerHTML");
  if (!descriptor?.get || !descriptor.set) throw new Error("innerHTML descriptor is unavailable");
  Object.defineProperty(Element.prototype, "innerHTML", {
    configurable: descriptor.configurable,
    enumerable: descriptor.enumerable,
    get: descriptor.get,
    set(value) {
      descriptor.set.call(this, value);
      if (this.id !== "app") return;
      if (timeline.bootstrapAssignmentMs === null) {
        timeline.bootstrapAssignmentMs = now();
        return;
      }
      if (timeline.routePaintedMs !== null || !this.querySelector("#route-heading")) return;
      requestAnimationFrame(() => requestAnimationFrame(() => {
        if (timeline.routePaintedMs === null) timeline.routePaintedMs = now();
      }));
    },
  });
}

export async function measureBootTimeline({ url, timeoutMs }) {
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: `(${installTimelineProbe.toString()})()` });
    await page.navigate(url, { timeoutMs });
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const timeline = await page.evaluate("window.__oslBootTimeline");
      if (timeline?.firstPaintMs !== null && timeline?.firstContentfulPaintMs !== null
        && timeline?.bootstrapAssignmentMs !== null && timeline?.routePaintedMs !== null) {
        return emittedBootTimeline(timeline);
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error("timed out waiting for first paint, bootstrap assignment, and route paint");
  } finally {
    await page.close();
    await chrome.close();
  }
}

async function main() {
  const result = await measureBootTimeline(parseArgs(process.argv.slice(2)));
  console.log(JSON.stringify(result));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`measure-osl-hub-boot: ${error.stack || error.message}`);
    process.exitCode = 1;
  });
}
