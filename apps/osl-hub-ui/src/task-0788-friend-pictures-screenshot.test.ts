/**
 * TASK 0788 - a Linux screenshot of the Friend pictures screen.
 *
 * Same CDP-over-headless-Chrome harness as TASK 0367
 * (`task-0367-pro-ready-screenshot.test.ts`): render the screen's real markup
 * and CSS, capture it with `Page.captureScreenshot`, and read facts back from
 * the PNG bytes rather than trust the markup alone. The finish line is that
 * the capture shows a picture, a coloured-initial fallback, and every
 * control -- own-picture, hide pictures, the three layouts and Reset -- so
 * this test locates each of those regions in the rendered page and asserts
 * they are both present and actually drawn on (more than a flat block of one
 * colour).
 */
import { createHash } from "node:crypto";
import { existsSync, globSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, type ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";
import { inflateSync } from "node:zlib";
import { afterEach, describe, expect, it, vi } from "vitest";

import { renderFriendPicturesScreen } from "./friend-pictures-screen";
import {
  FRIEND_PICTURES_SCREEN_FRIENDS,
  FRIEND_PICTURES_SCREEN_SETTINGS,
  FRIEND_PICTURES_SCREEN_WINDOW,
} from "./friend-pictures-screen-data";

const WINDOW_SIZE = FRIEND_PICTURES_SCREEN_WINDOW;
const SCREENSHOT_PATH = path.resolve("screenshots/task-0788-friend-pictures.png");
const CAPTURE_BUDGET_MS = 60_000;

class CDPClient {
  private nextId = 1;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();

  constructor(private readonly ws: WebSocket) {
    ws.addEventListener("message", (event) => this.onMessage(event));
  }

  private onMessage(event: MessageEvent): void {
    const message = JSON.parse(String(event.data));
    if (message.id === undefined) return;
    const pending = this.pending.get(message.id);
    if (!pending) return;
    this.pending.delete(message.id);
    if (message.error) pending.reject(new Error(message.error.message));
    else pending.resolve(message.result);
  }

  send<T = any>(method: string, params: Record<string, unknown> = {}, sessionId?: string): Promise<T> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      const payload: Record<string, unknown> = { id, method, params };
      if (sessionId) payload.sessionId = sessionId;
      this.ws.send(JSON.stringify(payload));
    });
  }

  once(method: string, predicate: (sessionId?: string) => boolean = () => true): Promise<void> {
    return new Promise((resolve) => {
      const listener = (event: MessageEvent) => {
        const message = JSON.parse(String(event.data));
        if (message.method !== method) return;
        if (!predicate(message.sessionId)) return;
        this.ws.removeEventListener("message", listener);
        resolve();
      };
      this.ws.addEventListener("message", listener);
    });
  }
}

function locateChrome(): string {
  const candidates = [
    process.env.OSL_CHROME,
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    ...globSync(`${os.homedir()}/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell`).sort().reverse(),
    ...globSync(`${os.homedir()}/.cache/ms-playwright/chromium-*/chrome-linux*/chrome`).sort().reverse(),
  ].filter(Boolean) as string[];
  const chrome = candidates.find((candidate) => existsSync(candidate));
  if (!chrome) throw new Error("no Chrome/Chromium binary found; set OSL_CHROME");
  return chrome;
}

async function connectToChrome(chromeChild: ChildProcessByStdio<null, null, Readable>): Promise<{ cdp: CDPClient; ws: WebSocket }> {
  let buffer = "";
  const wsUrl = await new Promise<string>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for Chrome CDP")), 15_000);
    chromeChild.stderr.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      const match = buffer.match(/DevTools listening on (ws:\/\/\S+)/);
      if (!match) return;
      clearTimeout(timer);
      resolve(match[1]);
    });
    chromeChild.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`Chrome exited before CDP was ready (${code})`));
    });
  });
  const cdpPort = new URL(wsUrl).port;
  const versionInfo = await fetch(`http://127.0.0.1:${cdpPort}/json/version`).then((response) => response.json());
  const ws = new WebSocket(versionInfo.webSocketDebuggerUrl || wsUrl);
  await new Promise<void>((resolve, reject) => {
    ws.addEventListener("open", () => resolve());
    ws.addEventListener("error", () => reject(new Error("WebSocket connection failed")));
  });
  return { cdp: new CDPClient(ws), ws };
}

function screenshotHtml(markup: string): string {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const screenStyles = readFileSync(new URL("./friend-pictures-screen.css", import.meta.url), "utf8");
  const css = (styles + screenStyles).replaceAll("</style", "<\\/style");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"/><title>TASK 0788 Friend pictures</title><style>${css}</style><style>html,body{width:100%;height:100%;margin:0;background:#080c0d;padding:14px;box-sizing:border-box}</style></head><body>${markup}</body></html>`;
}

function sha256(buffer: Buffer): string {
  return createHash("sha256").update(buffer).digest("hex");
}

function pngFacts(buffer: Buffer): { width: number; height: number; bytes: number; sha256: string } {
  expect(buffer.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(buffer.subarray(12, 16).toString("ascii")).toBe("IHDR");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20), bytes: buffer.length, sha256: sha256(buffer) };
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

function decodePngRgb(buffer: Buffer): { width: number; height: number; rgb: Uint8Array } {
  const width = buffer.readUInt32BE(16);
  const height = buffer.readUInt32BE(20);
  const bitDepth = buffer[24];
  const colorType = buffer[25];
  if (bitDepth !== 8 || (colorType !== 2 && colorType !== 6)) throw new Error(`unsupported PNG format bitDepth=${bitDepth} colorType=${colorType}`);
  const bytesPerPixel = colorType === 6 ? 4 : 3;
  const stride = width * bytesPerPixel;
  const chunks: Buffer[] = [];
  for (let offset = 8; offset < buffer.length;) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    if (type === "IDAT") chunks.push(buffer.subarray(offset + 8, offset + 8 + length));
    offset += 12 + length;
  }
  const inflated = inflateSync(Buffer.concat(chunks));
  const rgb = new Uint8Array(width * height * 3);
  let source = 0;
  let target = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[source++];
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= bytesPerPixel ? row[x - bytesPerPixel] : 0;
      const up = previous[x];
      const upperLeft = x >= bytesPerPixel ? previous[x - bytesPerPixel] : 0;
      if (filter === 1) row[x] = (row[x] + left) & 255;
      else if (filter === 2) row[x] = (row[x] + up) & 255;
      else if (filter === 3) row[x] = (row[x] + Math.floor((left + up) / 2)) & 255;
      else if (filter === 4) row[x] = (row[x] + paeth(left, up, upperLeft)) & 255;
      else if (filter !== 0) throw new Error(`unsupported PNG filter ${filter}`);
    }
    for (let x = 0; x < width; x += 1) {
      const i = x * bytesPerPixel;
      rgb[target++] = row[i];
      rgb[target++] = row[i + 1];
      rgb[target++] = row[i + 2];
    }
    previous = row;
  }
  return { width, height, rgb };
}

function colorStats(image: { width: number; height: number; rgb: Uint8Array }, box: { x: number; y: number; width: number; height: number }): { uniqueColors: number; dominantRatio: number; pixels: number } {
  const colors = new Map<number, number>();
  const left = Math.max(0, Math.floor(box.x));
  const top = Math.max(0, Math.floor(box.y));
  const right = Math.min(image.width, Math.ceil(box.x + box.width));
  const bottom = Math.min(image.height, Math.ceil(box.y + box.height));
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const i = (y * image.width + x) * 3;
      const color = (image.rgb[i] << 16) | (image.rgb[i + 1] << 8) | image.rgb[i + 2];
      colors.set(color, (colors.get(color) ?? 0) + 1);
    }
  }
  const pixels = Math.max(1, (right - left) * (bottom - top));
  return { uniqueColors: colors.size, dominantRatio: Math.max(...colors.values()) / pixels, pixels };
}

describe("TASK 0788 Friend pictures screenshot", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("captures every control, a picture and a coloured fallback on a Linux render", async () => {
    const state = { saved: FRIEND_PICTURES_SCREEN_SETTINGS, draft: FRIEND_PICTURES_SCREEN_SETTINGS };
    const markup = `<div class="friend-pictures-screen-stage">${renderFriendPicturesScreen(state, FRIEND_PICTURES_SCREEN_FRIENDS)}</div>`;
    expect(markup).toContain('data-friend-pictures-control="own-picture"');
    expect(markup).toContain('data-friend-pictures-control="hide-pictures"');
    expect(markup).toContain('data-friend-pictures-layout-option="picture-and-name"');
    expect(markup).toContain('data-friend-pictures-layout-option="compact"');
    expect(markup).toContain('data-friend-pictures-layout-option="grid"');
    expect(markup).toContain('data-friend-pictures-action="reset"');

    const chrome = spawn(locateChrome(), [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      "about:blank",
    ], { stdio: ["ignore", "ignore", "pipe"] });

    let ws: WebSocket | undefined;
    try {
      const connection = await connectToChrome(chrome);
      ws = connection.ws;
      const { cdp } = connection;
      const { targetId } = await cdp.send<{ targetId: string }>("Target.createTarget", { url: "about:blank" });
      const { sessionId } = await cdp.send<{ sessionId: string }>("Target.attachToTarget", { targetId, flatten: true });
      await cdp.send("Page.enable", {}, sessionId);
      await cdp.send("Runtime.enable", {}, sessionId);
      await cdp.send("Accessibility.enable", {}, sessionId);
      await cdp.send("Emulation.setDeviceMetricsOverride", { width: WINDOW_SIZE.width, height: WINDOW_SIZE.height, deviceScaleFactor: 1, mobile: false }, sessionId);
      const loaded = cdp.once("Page.loadEventFired", (sid) => sid === sessionId);
      await cdp.send("Page.navigate", { url: "about:blank" }, sessionId);
      await loaded;
      await cdp.send("Runtime.evaluate", { expression: `document.open();document.write(${JSON.stringify(screenshotHtml(markup))});document.close();` }, sessionId);
      await new Promise((resolve) => setTimeout(resolve, 250));

      const ax = await cdp.send<{ nodes: Array<{ role?: { value?: string }; name?: { value?: string } }> }>("Accessibility.getFullAXTree", {}, sessionId);
      const treeText = ax.nodes.map((node) => `${node.role?.value ?? ""}:${node.name?.value ?? ""}`).join("\n");
      expect(treeText).toContain("heading:Friend pictures");
      expect(treeText).toContain("button:Choose picture");
      expect(treeText).toContain("button:Reset");
      expect(treeText).toContain("switch:Hide pictures");
      expect(treeText).toContain("radio:Picture and name");
      expect(treeText).toContain("radio:Compact");
      expect(treeText).toContain("radio:Grid");

      const boxes = await cdp.send<{ result: { value: Record<string, { x: number; y: number; width: number; height: number }> } }>("Runtime.evaluate", {
        expression: `(() => {
          function box(selector) {
            const el = document.querySelector(selector);
            if (!el) throw new Error("missing " + selector);
            const rect = el.getBoundingClientRect();
            return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
          }
          return {
            ownPicture: box('[data-friend-pictures-control="own-picture"] img.friend-picture'),
            hidePictures: box('[data-friend-pictures-control="hide-pictures"]'),
            layout: box('[data-friend-pictures-control="layout"]'),
            resetButton: box('[data-friend-pictures-action="reset"]'),
            saveButton: box('[data-friend-pictures-action="save"]'),
            previewPicture: box('.friend-pictures-preview img.friend-picture'),
            previewFallback: box('.friend-pictures-preview .friend-picture-fallback'),
          };
        })()`,
        returnByValue: true,
      }, sessionId);
      for (const [name, box] of Object.entries(boxes.result.value)) {
        expect(box.width, `${name} width`).toBeGreaterThan(8);
        expect(box.height, `${name} height`).toBeGreaterThan(8);
      }

      const capture = await cdp.send<{ data: string }>("Page.captureScreenshot", {
        format: "png",
        fromSurface: true,
        clip: { x: 0, y: 0, width: WINDOW_SIZE.width, height: WINDOW_SIZE.height, scale: 1 },
      }, sessionId);
      const bytes = Buffer.from(capture.data, "base64");
      mkdirSync(path.dirname(SCREENSHOT_PATH), { recursive: true });
      writeFileSync(SCREENSHOT_PATH, bytes);
      const facts = pngFacts(bytes);
      expect(facts.width).toBe(WINDOW_SIZE.width);
      expect(facts.height).toBe(WINDOW_SIZE.height);
      expect(facts.bytes).toBeGreaterThan(5_000);

      const decoded = decodePngRgb(bytes);
      const whole = colorStats(decoded, { x: 0, y: 0, width: decoded.width, height: decoded.height });
      expect(whole.uniqueColors).toBeGreaterThan(32);
      expect(whole.dominantRatio).toBeLessThan(0.98);

      const regionStats: Record<string, { uniqueColors: number; pixels: number }> = {};
      for (const [name, box] of Object.entries(boxes.result.value)) {
        const stats = colorStats(decoded, box);
        regionStats[name] = { uniqueColors: stats.uniqueColors, pixels: stats.pixels };
        expect(stats.uniqueColors, `${name} region drawn on`).toBeGreaterThan(1);
      }

      // The preview fallback must actually be the coloured circle, not a flat
      // background-coloured rectangle: it needs a second colour for the letter.
      expect(regionStats.previewFallback.uniqueColors).toBeGreaterThan(1);
      expect(regionStats.previewPicture.uniqueColors).toBeGreaterThan(1);

      console.log(`TASK0788_WINDOW_SIZE=${WINDOW_SIZE.width}x${WINDOW_SIZE.height}`);
      console.log(`TASK0788_IMAGE_PATH=${path.relative(process.cwd(), SCREENSHOT_PATH)}`);
      console.log(`TASK0788_PNG_DIMENSIONS=${facts.width}x${facts.height}`);
      console.log(`TASK0788_PNG_BYTES=${facts.bytes}`);
      console.log(`TASK0788_PNG_SHA256=${facts.sha256}`);
      console.log(`TASK0788_PNG_UNIQUE_COLORS=${whole.uniqueColors}`);
      console.log(`TASK0788_PNG_DOMINANT_RATIO=${whole.dominantRatio.toFixed(4)}`);
      for (const [name, stats] of Object.entries(regionStats)) {
        console.log(`TASK0788_REGION_${name}=unique_colors=${stats.uniqueColors} pixels=${stats.pixels}`);
      }
      await cdp.send("Target.closeTarget", { targetId }).catch(() => undefined);
    } finally {
      if (ws) ws.close();
      chrome.kill("SIGTERM");
    }
  }, CAPTURE_BUDGET_MS);
});
