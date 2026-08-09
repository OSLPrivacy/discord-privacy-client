#!/usr/bin/env node

// capture-all-routes -- photograph EVERY screen of the OSL hub UI, for real.
//
// A design audit received 140 "captures" of this app; three were hand-written
// SVG drawn inside test files, and the rest came from lane worktrees that no
// longer exist. This harness is the single trustworthy answer to "what does
// the app look like": it boots the REAL app (vite dev server + headless
// Chromium over CDP), drives the app's own render path onto every route, and
// writes one PNG per screen plus a manifest of measured pixel facts.
//
// Honesty rules:
//   - The route list is DERIVED from src/main.ts at run time (the `Route` and
//     `OnboardingRoute` union types), so it cannot silently go stale.
//   - A capture with fewer than MIN_DISTINCT_COLORS distinct colours is
//     recorded as FAILED and the script exits non-zero. Blankness is judged
//     by COUNTING COLOURS, never by brightness -- this app is near-black by
//     design (#080c0d), so brightness thresholds cannot tell "rendered" from
//     "didn't".
//   - A route that cannot render is reported as FAILED with the real error.
//     Nothing is ever drawn by hand to fill a gap.
//
// THEME IS PINNED TO DARK, deliberately and twice. The app's default theme is
// "system" once any OSL state exists (src/theme-preference.ts), and none of
// the older capture scripts pinned it -- so the same screen photographed
// differently depending on the host machine's OS theme. Here:
//   1. localStorage `osl-hub-theme` is set to "dark" BEFORE the app module
//      evaluates (Page.addScriptToEvaluateOnNewDocument), so the app itself
//      resolves to dark, and
//   2. CDP `Emulation.setEmulatedMedia` forces `prefers-color-scheme: dark`,
//      so even "system" styling resolves identically on every machine.
// The harness then VERIFIES `data-theme="dark"` on <html> before each shot.

import { mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { parsePng } from "./png-facts.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const MAIN_TS = path.join(APP_ROOT, "src", "main.ts");
const OUT_DIR = path.join(SCRIPT_DIR, "all-routes");
const MANIFEST_PATH = path.join(OUT_DIR, "manifest.json");

/** One fixed window for every shot so screens are directly comparable. */
export const WINDOW = { width: 1280, height: 800 };

/** Fewer distinct colours than this means the page did not really render. */
export const MIN_DISTINCT_COLORS = 12;

/** Pixel sampling stride for colour statistics (every 4th pixel each axis). */
const SAMPLE_STRIDE = 4;

// ---------------------------------------------------------------------------
// Route enumeration -- read the unions straight out of src/main.ts so the
// screen list tracks the source and cannot rot.
// ---------------------------------------------------------------------------

export function unionMembers(source, declaration, typeName) {
  const match = source.match(new RegExp(`${declaration}\\s*=([^;]+);`));
  if (!match) throw new Error(`could not find \`${declaration} = ...\` in src/main.ts`);
  const members = [...match[1].matchAll(/"([^"]+)"/g)].map((hit) => hit[1]);
  if (members.length === 0) throw new Error(`the ${typeName} union in src/main.ts has no string members`);
  return members;
}

export function enumerateScreens(mainTsSource) {
  const routes = unionMembers(mainTsSource, "export type Route", "Route");
  const onboardingRoutes = unionMembers(mainTsSource, "type OnboardingRoute", "OnboardingRoute");
  const screens = [];
  for (const route of routes) {
    // "onboarding" as a bare top-level route is covered by its sub-routes below.
    if (route === "onboarding") continue;
    screens.push({ name: route, kind: "workspace", route });
  }
  for (const sub of onboardingRoutes) {
    screens.push({ name: `onboarding-${sub}`, kind: "onboarding", route: "onboarding", onboardingRoute: sub });
  }
  return { routes, onboardingRoutes, screens };
}

// ---------------------------------------------------------------------------
// In-page scripts
// ---------------------------------------------------------------------------

/**
 * Runs before ANY page script. Pins the theme choice the app will read at
 * module-init time, and stubs enough of Tauri that the web UI boots without a
 * native shell (copied from capture-home-top-bar.mjs, the proven pattern).
 */
const NEW_DOCUMENT_SCRIPT = `
  (() => {
    try { localStorage.setItem("osl-hub-theme", "dark"); } catch {}
    let nextCallback = 1;
    const callbacks = {};
    window.__TAURI_INTERNALS__ = {
      callbacks,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback(callback, once = false) {
        const id = nextCallback++;
        callbacks[id] = { callback, once };
        return id;
      },
      unregisterCallback(id) { delete callbacks[id]; },
      runCallback(id, args) {
        const entry = callbacks[id];
        if (!entry) return;
        entry.callback(args);
        if (entry.once) delete callbacks[id];
      },
      convertFileSrc(filePath) { return filePath; },
      invoke(cmd) {
        if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
        if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
        return Promise.reject(new Error("capture-all-routes Tauri stub refused " + cmd));
      },
    };
  })();
`;

/** Enough app state that every screen has real content to paint. */
const BASE_STATE = {
  coreReady: true,
  bootstrapStatus: "ready",
  storageMethod: "tpm-pcp",
  licenseAccess: "free",
  servicesChecked: true,
  notificationsEnabled: true,
  services: [
    {
      id: "signal", displayName: "Signal", sidebarGlyph: "SI", sidebarOrder: 0,
      category: "consumer", launchState: "available",
      supportsNativePreview: true, supportsProtectedPreview: true,
      accounts: [{ id: "acct-signal", label: "Alma Reed", handle: "+15550130324" }],
    },
    {
      id: "discord", displayName: "Discord", sidebarGlyph: "DI", sidebarOrder: 1,
      category: "consumer", launchState: "available",
      supportsNativePreview: true, supportsProtectedPreview: true,
      accounts: [{ id: "acct-discord", label: "Miles Chen", handle: "miles.fixed.0324" }],
    },
  ],
  hubPeople: [
    { personId: "p-1", alias: "Rose", safetyNumberVerified: false },
    { personId: "p-2", alias: "Sam", safetyNumberVerified: true, pendingKeyChange: true },
  ],
  hubIdentities: [{ slotId: "slot-1", label: "OSL Profile", oslUserId: "OSLUSER-1", active: true }],
  appNotifications: [{ id: "n-1", title: "Key change", detail: "Rose changed keys", createdAt: "Now" }],
  recoveryBundle: {
    userId: "OSLUSER-1",
    identityPhrase: "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
    passwordPhrase: "atlas broom cedar dusk ember flint grove honest iris kettle lunar mint",
  },
};

const BOOT = `(async () => {
  const ui = await import("/src/main.ts");
  globalThis.__oslCaptureAll = ui;
  // Let any in-flight async boot work settle before captures start, so a late
  // boot render cannot repaint over a captured screen.
  await new Promise((resolve) => setTimeout(resolve, 500));
  return "booted";
})()`;

function showScreenExpression(screen) {
  const patch = { ...BASE_STATE };
  if (screen.kind === "onboarding") {
    patch.route = "onboarding";
    patch.onboardingRoute = screen.onboardingRoute;
    patch.onboardingComplete = false;
  } else {
    patch.route = screen.route;
    patch.onboardingComplete = true;
  }
  // The "service" screen only renders with an active service; reset() cannot
  // set one, so the app's own test hook synthesises a minimal Discord service
  // (route becomes "service" and activeService is populated).
  const serviceSetup = screen.route === "service"
    ? `ui.__oslHubUiTest.renderServiceHeader("discord");`
    : "";
  return `(async () => {
    const ui = globalThis.__oslCaptureAll;
    ui.__oslHubUiTest.reset(${JSON.stringify(patch)});
    ${serviceSetup}
    ui.__oslHubUiTest.flushRenderForTest();
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    // Let entrance motion finish before photographing: .view-enter staggers
    // section animations with \`both\` fill, so a capture taken two frames in
    // photographs later sections at opacity 0 — the Email row "vanished" from
    // every Home capture this way. Only finite animations are awaited
    // (spinners never finish), and a 1.5s cap keeps a stuck animation from
    // hanging the run.
    await Promise.race([
      Promise.all(document.getAnimations()
        .filter((animation) => {
          const timing = animation.effect?.getTiming?.();
          return timing && timing.iterations !== Infinity;
        })
        .map((animation) => animation.finished.catch(() => {}))),
      new Promise((resolve) => setTimeout(resolve, 1500)),
    ]);
    return {
      theme: document.documentElement.dataset.theme ?? null,
      themeChoice: document.documentElement.dataset.themeChoice ?? null,
      headingText: (document.querySelector("#route-heading, h1, h2")?.textContent ?? "").replace(/\\s+/g, " ").trim().slice(0, 120),
      bodyTextLength: document.body.innerText.replace(/\\s+/g, " ").trim().length,
    };
  })()`;
}

// ---------------------------------------------------------------------------
// Pixel facts
// ---------------------------------------------------------------------------

/** Distinct-colour count and dominant colour, sampled every 4th pixel. */
export function pixelFacts(pngBuffer) {
  const png = parsePng(pngBuffer);
  const counts = new Map();
  for (let y = 0; y < png.height; y += SAMPLE_STRIDE) {
    for (let x = 0; x < png.width; x += SAMPLE_STRIDE) {
      const offset = (y * png.width + x) * 4;
      const key = (png.pixels[offset] << 16) | (png.pixels[offset + 1] << 8) | png.pixels[offset + 2];
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  let dominantKey = 0;
  let dominantCount = -1;
  let sampled = 0;
  for (const [key, count] of counts) {
    sampled += count;
    if (count > dominantCount) {
      dominantKey = key;
      dominantCount = count;
    }
  }
  return {
    width: png.width,
    height: png.height,
    distinctColors: counts.size,
    dominantBackground: `#${dominantKey.toString(16).padStart(6, "0")}`,
    dominantShare: Math.round((dominantCount / sampled) * 1000) / 1000,
  };
}

// ---------------------------------------------------------------------------
// Capture loop
// ---------------------------------------------------------------------------

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description
      || result.exceptionDetails.text
      || "page evaluation failed");
  }
  return result.result.value;
}

export async function captureAllRoutes() {
  const { routes, onboardingRoutes, screens } = enumerateScreens(readFileSync(MAIN_TS, "utf8"));
  mkdirSync(OUT_DIR, { recursive: true });

  // node_modules may be a symlink into another checkout; vite's fs allow-list
  // works on REAL paths, so without this the @fontsource woff2 files are
  // blocked and every screenshot silently renders in fallback fonts.
  const vite = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: {
      host: "127.0.0.1",
      port: 0,
      fs: { allow: [APP_ROOT, realpathSync(path.join(APP_ROOT, "node_modules"))] },
    },
  });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/`;
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${WINDOW.width},${WINDOW.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  const entries = [];
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: NEW_DOCUMENT_SCRIPT });
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    // Deterministic rendering: dark colour scheme regardless of host machine,
    // and no motion so screenshots never catch a transition mid-flight.
    await page.send("Emulation.setEmulatedMedia", {
      features: [
        { name: "prefers-color-scheme", value: "dark" },
        { name: "prefers-reduced-motion", value: "reduce" },
      ],
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    const booted = await evaluate(page, BOOT);
    if (booted !== "booted") throw new Error(`the app did not boot: ${booted}`);

    for (const screen of screens) {
      const outputPath = path.join(OUT_DIR, `${screen.name}.png`);
      const entry = {
        route: screen.name,
        kind: screen.kind,
        appRoute: screen.route,
        onboardingRoute: screen.onboardingRoute ?? null,
        outputPath,
        status: "FAILED",
        reason: null,
        bytes: null,
        width: null,
        height: null,
        distinctColors: null,
        dominantBackground: null,
        theme: null,
        heading: null,
      };
      try {
        const shown = await evaluate(page, showScreenExpression(screen));
        entry.theme = shown.theme;
        entry.heading = shown.headingText;
        if (shown.theme !== "dark") {
          throw new Error(`theme is not pinned: data-theme="${shown.theme}" (choice "${shown.themeChoice}")`);
        }
        // A CRASHED SCREEN IS NOT A SCREEN.
        //
        // On 2026-08-08 the Settings route threw on every render and painted
        // the "OSL paused this view" error boundary. That view uses the correct
        // background and accent, so it passed both the distinct-colour blank
        // check below AND a separate palette-conformance audit, and was shown to
        // the owner in a design review as a conformant screen. Colour statistics
        // cannot tell a rendered page from a caught exception.
        //
        // The boundary now marks itself, so refuse the capture outright: a
        // missing screenshot is an honest result; a photograph of a crash
        // presented as a screen is not.
        const crashed = await evaluate(
          page,
          `(() => !!document.querySelector('[data-osl-render-recovery]'))()`,
        );
        if (crashed) {
          throw new Error(
            "screen rendered the error boundary (data-osl-render-recovery): it crashed, so there is nothing to photograph",
          );
        }
        const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
        writeFileSync(outputPath, png);
        const facts = pixelFacts(png);
        entry.bytes = png.length;
        entry.width = facts.width;
        entry.height = facts.height;
        entry.distinctColors = facts.distinctColors;
        entry.dominantBackground = facts.dominantBackground;
        entry.dominantShare = facts.dominantShare;
        if (facts.width !== WINDOW.width || facts.height !== WINDOW.height) {
          entry.reason = `wrong dimensions ${facts.width}x${facts.height}, expected ${WINDOW.width}x${WINDOW.height}`;
        } else if (facts.distinctColors < MIN_DISTINCT_COLORS) {
          entry.reason = `blank or near-blank render: ${facts.distinctColors} distinct colours (< ${MIN_DISTINCT_COLORS})`;
        } else {
          entry.status = "OK";
        }
      } catch (error) {
        entry.reason = error.message;
      }
      entries.push(entry);
      const label = entry.status === "OK" ? "ok    " : "FAILED";
      console.log(`${label} ${screen.name.padEnd(28)} colors=${entry.distinctColors ?? "-"} bg=${entry.dominantBackground ?? "-"}${entry.reason ? ` -- ${entry.reason}` : ""}`);
    }
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }

  const failed = entries.filter((entry) => entry.status !== "OK");
  const manifest = {
    generatedAt: new Date().toISOString(),
    window: WINDOW,
    theme: "dark (pinned; see header comment)",
    minDistinctColors: MIN_DISTINCT_COLORS,
    sampleStride: SAMPLE_STRIDE,
    routesFromSource: routes,
    onboardingRoutesFromSource: onboardingRoutes,
    captured: entries.length - failed.length,
    failed: failed.length,
    screens: entries,
  };
  writeFileSync(MANIFEST_PATH, `${JSON.stringify(manifest, null, 2)}\n`);
  return { manifest, entries, failed };
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureAllRoutes().then(({ manifest, entries, failed }) => {
    const backgrounds = new Map();
    for (const entry of entries) {
      if (entry.dominantBackground) backgrounds.set(entry.dominantBackground, (backgrounds.get(entry.dominantBackground) ?? 0) + 1);
    }
    console.log("");
    console.log(`screens captured: ${manifest.captured}/${entries.length}`);
    console.log(`screens FAILED:   ${failed.length}${failed.length ? ` (${failed.map((entry) => entry.route).join(", ")})` : ""}`);
    console.log(`dominant backgrounds: ${[...backgrounds.entries()].sort((a, b) => b[1] - a[1]).map(([color, count]) => `${color} x${count}`).join(", ")}`);
    console.log(`manifest: ${MANIFEST_PATH}`);
    process.exit(failed.length ? 1 : 0);
  }).catch((error) => {
    console.error(`capture-all-routes: fatal: ${error.stack || error.message}`);
    process.exit(1);
  });
}
