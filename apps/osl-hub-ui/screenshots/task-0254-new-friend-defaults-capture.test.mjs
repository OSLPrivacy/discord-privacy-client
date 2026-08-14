import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const FIXTURE_PAGE = "screenshots/task-0253-new-friend-default-save-fixture.html";
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-0254-new-friend-defaults-saved-1280x800.png");
const SAVED = Object.freeze({
  "account-reach": { value: "all_shared_chats", label: "All shared chats" },
  "auto-whitelist": { value: "only_if_a_friend", label: "Only if a friend" },
  "verification-warnings": { value: "never", label: "Never warn" },
});

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  }
  return result.result.value;
}

async function waitReady(page) {
  await evaluate(page, `new Promise((resolve, reject) => {
    const deadline = Date.now() + 15_000;
    const tick = () => {
      if (document.querySelector("#fixture")?.dataset.task0253State === "ready") return resolve();
      if (Date.now() > deadline) return reject(new Error("fixture did not become ready"));
      setTimeout(tick, 20);
    };
    tick();
  })`);
}

function pngSize(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "screenshot is a PNG");
  assert.equal(bytes.subarray(12, 16).toString("ascii"), "IHDR", "screenshot has an IHDR");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

test("TASK 0254 captures the three saved new-friend defaults in the fixed Linux window", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await vite.listen();
  const address = vite.httpServer?.address();
  assert.ok(address && typeof address !== "string", "Vite did not expose a TCP port");
  const linuxScreen = await vite.ssrLoadModule("/src/linux-onboarding-screen-data.ts");
  const windowSize = linuxScreen.LINUX_ONBOARDING_SCREEN_WINDOW;
  assert.deepEqual(windowSize, { width: 1280, height: 800 });
  const url = `http://127.0.0.1:${address.port}/${FIXTURE_PAGE}`;
  const chrome = await launchChrome();
  const page = await chrome.openPage();

  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      ...windowSize,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    await waitReady(page);
    await evaluate(page, "localStorage.clear(); location.reload(); true");
    await waitReady(page);

    const saved = await evaluate(page, `(async () => {
      const choices = ${JSON.stringify(SAVED)};
      for (const [control, choice] of Object.entries(choices)) {
        const radio = document.querySelector('[data-new-friend-control="' + control + '"] input[value="' + choice.value + '"]');
        if (!radio) throw new Error('missing choice ' + control + ':' + choice.value);
        radio.click();
      }
      document.querySelector('[data-new-friend-action="save-defaults"]').click();
      const deadline = Date.now() + 15_000;
      while (!document.querySelector("#fixture")?.dataset.task0253Saved) {
        if (Date.now() > deadline) throw new Error("Save default did not complete");
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      return Object.fromEntries(Object.entries(choices).map(([control, choice]) => {
        const input = document.querySelector('[data-new-friend-control="' + control + '"] input:checked');
        const label = input?.closest("label")?.innerText.trim() ?? "";
        const rect = input?.closest("label")?.getBoundingClientRect();
        return [control, { value: input?.value ?? "", label, rect: rect && { x: rect.x, y: rect.y, width: rect.width, height: rect.height } }];
      }));
    })()`);

    for (const [control, expected] of Object.entries(SAVED)) {
      assert.equal(saved[control].value, expected.value, `${control} saved value`);
      assert.equal(saved[control].label, expected.label, `${control} saved visible label`);
      const rect = saved[control].rect;
      assert.ok(rect && rect.width > 0 && rect.height > 0, `${control} saved label has a visible rectangle`);
      assert.ok(rect.x >= 0 && rect.y >= 0 && rect.x + rect.width <= windowSize.width && rect.y + rect.height <= windowSize.height, `${control} saved label fits in the fixed window`);
    }

    await page.send("Accessibility.enable");
    const tree = await page.send("Accessibility.getFullAXTree");
    const names = tree.nodes
      .map((node) => typeof node.name?.value === "string" ? node.name.value.trim() : "")
      .filter(Boolean);
    for (const { label } of Object.values(SAVED)) {
      assert.ok(names.includes(label), `screen tree includes saved label: ${label}`);
    }

    const png = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const dimensions = pngSize(png);
    assert.deepEqual(dimensions, windowSize, "screenshot uses the fixed Linux window size");
    const sha256 = createHash("sha256").update(png).digest("hex");
    console.log(`TASK0254_PNG=${PNG_PATH}`);
    console.log(`TASK0254_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0254_SAVED account_reach=${saved["account-reach"].value} label=${saved["account-reach"].label}`);
    console.log(`TASK0254_SAVED auto_whitelist=${saved["auto-whitelist"].value} label=${saved["auto-whitelist"].label}`);
    console.log(`TASK0254_SAVED verification_warnings=${saved["verification-warnings"].value} label=${saved["verification-warnings"].label}`);
    console.log(`TASK0254_SCREEN_TREE=${Object.values(SAVED).map(({ label }) => label).join("|")}`);
    console.log(`TASK0254_PNG_SHA256=${sha256}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
});
