import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const OUTPUT = join(ROOT, "screenshots", "evidence", "task-0651-throttled-upload-progress-tray-card.png");
const WINDOW = Object.freeze({ width: 900, height: 700 });

function pngDimensions(png) {
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

async function loadTrayModule() {
  const outfile = join("/tmp", `task-0651-upload-progress-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `export { acceptDroppedFilesIntoTray, attachmentTrayMarkup, createOslChatAttachmentTray, setOslChatAttachmentUploadProgress } from ${JSON.stringify(join(ROOT, "src", "chat-attachment-drop.ts"))};`,
      resolveDir: ROOT,
      sourcefile: "task-0651-entry.mjs",
      loader: "js",
    },
    bundle: true,
    format: "esm",
    platform: "node",
    outfile,
    logLevel: "silent",
  });
  return import(`file://${outfile}`);
}

function documentFor(markup) {
  return `<!doctype html><html lang="en"><meta charset="utf-8"><title>Throttled attachment upload</title><style>
    :root { color-scheme: dark; font-family: Inter, system-ui, sans-serif; background:#071011; color:#edf8f2; }
    * { box-sizing:border-box; }
    body { min-height:100vh; margin:0; display:grid; place-items:center; background:radial-gradient(circle at 16% 12%,#173c35 0,#071011 46%); }
    main { width:min(620px,calc(100vw - 64px)); padding:28px; border:1px solid #31564c; border-radius:18px; background:#0d1b1a; box-shadow:0 20px 55px #0009; }
    h1 { margin:0 0 6px; font-size:22px; }
    .hint { margin:0 0 22px; color:#a6c3b6; font-size:14px; }
    .osl-chat-drop-tray { display:grid; gap:12px; margin:0; padding:0; list-style:none; }
    .osl-chat-drop-card { display:grid; gap:8px; padding:18px; border:1px solid #4b846c; border-radius:12px; background:#102723; }
    .osl-chat-drop-card strong { font-size:17px; }
    .osl-chat-drop-card small { color:#cce3d6; font-size:14px; }
    .osl-chat-drop-upload-progress { padding:10px 12px; border-left:4px solid #70d59c; border-radius:5px; background:#153a30; color:#f1fff6 !important; font-variant-numeric:tabular-nums; }
  </style><body><main aria-label="OSL Chats attachment tray"><h1>Encrypted attachments</h1><p class="hint">Throttled upload in progress</p>${markup}</main></body></html>`;
}

test("TASK 0651 captures the tray card while a throttled upload is in progress", async () => {
  mkdirSync(dirname(OUTPUT), { recursive: true });
  const trayModule = await loadTrayModule();
  const tray = trayModule.createOslChatAttachmentTray();
  trayModule.acceptDroppedFilesIntoTray(tray, [{ name: "throttled-37.bin", size: 37 }]);
  const trayId = tray.attachments[0]?.trayId;
  assert.ok(trayId, "throttled fixture did not create a tray card");

  const throttledRead = new Promise((resolve) => setTimeout(() => {
    trayModule.setOslChatAttachmentUploadProgress(tray, trayId, { uploadedBytes: 17, totalBytes: 37, completedPieces: 17 });
    resolve();
  }, 50));
  await throttledRead;

  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`data:text/html;charset=utf-8,${encodeURIComponent(documentFor(trayModule.attachmentTrayMarkup(tray)))}`);
    const visible = await page.evaluate(`(() => {
      const card = document.querySelector('[data-osl-chat-drop-card]');
      const progress = document.querySelector('.osl-chat-drop-upload-progress');
      if (!(card instanceof HTMLElement) || !(progress instanceof HTMLElement)) throw new Error('missing progress tray card');
      const rect = progress.getBoundingClientRect();
      return {
        filename: card.querySelector('strong')?.textContent,
        text: progress.textContent?.replace(/\\s+/gu, ' ').trim(),
        uploaded: progress.getAttribute('data-osl-chat-uploaded-bytes'),
        total: progress.getAttribute('data-osl-chat-total-bytes'),
        pieces: progress.getAttribute('data-osl-chat-completed-pieces'),
        visible: getComputedStyle(progress).display !== 'none' && rect.width > 0 && rect.height > 0,
        width: Math.round(rect.width), height: Math.round(rect.height),
      };
    })()`);
    assert.deepEqual({ uploaded: visible.uploaded, total: visible.total, pieces: visible.pieces }, { uploaded: "17", total: "37", pieces: "17" });
    assert.equal(visible.filename, "throttled-37.bin");
    assert.equal(visible.text, "Uploaded 17 of 37 bytes · 17 completed pieces");
    assert.equal(visible.visible, true);
    assert.ok(visible.width > 0 && visible.height > 0, "progress surface was not visibly laid out");
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(OUTPUT, png);
    assert.deepEqual(pngDimensions(png), WINDOW);
    assert.ok(png.length > 10_000, `screenshot unexpectedly small: ${png.length}`);
    console.log(`TASK0651_SCREENSHOT ${OUTPUT}`);
    console.log(`TASK0651_WINDOW ${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0651_THROTTLED uploaded=${visible.uploaded} total=${visible.total} completed_pieces=${visible.pieces}`);
    console.log(`TASK0651_VISIBLE filename=${visible.filename} progress_visible=${visible.visible} progress_bounds=${visible.width}x${visible.height}`);
    console.log(`TASK0651_PNG_BYTES ${png.length} SHA256 ${createHash("sha256").update(png).digest("hex")}`);
  } finally {
    await page.close();
    await chrome.close();
  }
}, { timeout: 30_000 });
