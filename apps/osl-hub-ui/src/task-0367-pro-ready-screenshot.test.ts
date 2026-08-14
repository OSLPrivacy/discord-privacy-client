import { createHash } from "node:crypto";
import { existsSync, globSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, type ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";
import { inflateSync } from "node:zlib";
import { afterEach, describe, expect, it, vi } from "vitest";

const WINDOW_SIZE = { width: 520, height: 720 } as const;
const SCREENSHOT_PATH = path.resolve("screenshots/task-0367-pro-ready.png");
const MODULE_RELOAD_BUDGET_MS = 60_000;

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

class CDPClient {
  private nextId = 1;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  private listeners = new Map<string, Set<(params: unknown, sessionId?: string) => void>>();

  constructor(private readonly ws: WebSocket) {
    ws.addEventListener("message", (event) => this.onMessage(event));
  }

  private onMessage(event: MessageEvent): void {
    const message = JSON.parse(String(event.data));
    if (message.id !== undefined) {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
      return;
    }
    if (!message.method) return;
    const listeners = this.listeners.get(message.method);
    if (listeners) for (const listener of listeners) listener(message.params, message.sessionId);
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

  on(method: string, listener: (params: unknown, sessionId?: string) => void): () => void {
    if (!this.listeners.has(method)) this.listeners.set(method, new Set());
    this.listeners.get(method)!.add(listener);
    return () => this.listeners.get(method)?.delete(listener);
  }

  once(method: string, predicate: (params: unknown, sessionId?: string) => boolean = () => true): Promise<unknown> {
    return new Promise((resolve) => {
      const off = this.on(method, (params, sessionId) => {
        if (!predicate(params, sessionId)) return;
        off();
        resolve(params);
      });
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

function stubMainGlobals(): void {
  vi.resetModules();
  vi.stubEnv("VITE_OSL_DISCORD_QA_SHELL", "0");
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({ querySelector: () => null, querySelectorAll: () => [], innerHTML: "" })),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
    activeElement: null,
  });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
}

async function proReadyMarkup(): Promise<string> {
  stubMainGlobals();
  const { __oslHubUiTest } = await import("./main");
  __oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "pro", licenseAccess: "pro", coreReady: true });
  return __oslHubUiTest.renderOnboardingShellForTest("pro");
}

function screenshotHtml(markup: string): string {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"/><title>TASK 0367 Pro ready</title><style>${styles.replaceAll("</style", "<\\/style")}</style><style>html,body,#app{width:100%;height:100%;margin:0;background:#080c0d;overflow:hidden}.app-frame{width:100%;height:100%}</style></head><body><div id="app">${markup}</div></body></html>`;
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

function colorStats(image: { width: number; height: number; rgb: Uint8Array }, box?: { x: number; y: number; width: number; height: number }): { uniqueColors: number; dominantRatio: number; pixels: number } {
  const colors = new Map<number, number>();
  const left = Math.max(0, Math.floor(box?.x ?? 0));
  const top = Math.max(0, Math.floor(box?.y ?? 0));
  const right = Math.min(image.width, Math.ceil((box?.x ?? 0) + (box?.width ?? image.width)));
  const bottom = Math.min(image.height, Math.ceil((box?.y ?? 0) + (box?.height ?? image.height)));
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

describe("TASK 0367 Pro ready screenshot", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("captures the fixed active-Pro onboarding screen with readable controls", async () => {
    const markup = await proReadyMarkup();
    expect(markup).toContain(">Pro is ready</h1>");
    expect(markup).toContain(">Continue</button>");
    expect(markup).toContain(">Back</button>");

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
      const loaded = cdp.once("Page.loadEventFired", (_params, sid) => sid === sessionId);
      await cdp.send("Page.navigate", { url: "about:blank" }, sessionId);
      await loaded;
      await cdp.send("Runtime.evaluate", { expression: `document.open();document.write(${JSON.stringify(screenshotHtml(markup))});document.close();` }, sessionId);
      await cdp.send("Runtime.evaluate", {
        expression: `(() => {
          const nav = document.querySelector(".onboarding-nav");
          const back = document.querySelector("#onboarding-back");
          const primaryRow = [...document.querySelectorAll(".onboarding-panel .setup-footer.onboarding-actions")].find((row) => row !== nav);
          if (nav && back && primaryRow) { primaryRow.prepend(back); nav.remove(); }
        })()`,
      }, sessionId);
      await new Promise((resolve) => setTimeout(resolve, 250));

      const ax = await cdp.send<{ nodes: Array<{ role?: { value?: string }; name?: { value?: string } }> }>("Accessibility.getFullAXTree", {}, sessionId);
      const treeItems = ax.nodes.map((node) => `${node.role?.value ?? ""}:${node.name?.value ?? ""}`);
      const treeText = treeItems.join("\n");
      expect(treeText).toContain("heading:Pro is ready");
      expect(treeText).toContain("button:Continue");
      expect(treeText).toContain("button:Back");

      const boxes = await cdp.send<{ result: { value: Record<string, { x: number; y: number; width: number; height: number }> } }>("Runtime.evaluate", {
        expression: `(() => {
          function box(selector) {
            const rect = document.querySelector(selector).getBoundingClientRect();
            return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
          }
          return { title: box("#route-heading"), continue: box("button.primary"), back: box("#onboarding-back") };
        })()`,
        returnByValue: true,
      }, sessionId);
      for (const [name, box] of Object.entries(boxes.result.value)) {
        expect(box.width, `${name} width`).toBeGreaterThan(20);
        expect(box.height, `${name} height`).toBeGreaterThan(20);
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
      const whole = colorStats(decoded);
      expect(whole.uniqueColors).toBeGreaterThan(32);
      expect(whole.dominantRatio).toBeLessThan(0.98);

      const titleRegion = colorStats(decoded, boxes.result.value.title);
      const continueRegion = colorStats(decoded, boxes.result.value.continue);
      const backRegion = colorStats(decoded, boxes.result.value.back);
      expect(titleRegion.uniqueColors).toBeGreaterThan(8);
      expect(continueRegion.uniqueColors).toBeGreaterThan(8);
      expect(backRegion.uniqueColors).toBeGreaterThan(8);

      console.log(`TASK0367_WINDOW_SIZE=${WINDOW_SIZE.width}x${WINDOW_SIZE.height}`);
      console.log(`TASK0367_SCREEN_TREE=heading:Pro is ready|button:Continue|button:Back`);
      console.log(`TASK0367_IMAGE_PATH=${path.relative(process.cwd(), SCREENSHOT_PATH)}`);
      console.log(`TASK0367_PNG_DIMENSIONS=${facts.width}x${facts.height}`);
      console.log(`TASK0367_PNG_BYTES=${facts.bytes}`);
      console.log(`TASK0367_PNG_SHA256=${facts.sha256}`);
      console.log(`TASK0367_PNG_UNIQUE_COLORS=${whole.uniqueColors}`);
      console.log(`TASK0367_PNG_DOMINANT_RATIO=${whole.dominantRatio.toFixed(4)}`);
      console.log(`TASK0367_IMAGE_REGION_TITLE=Pro is ready unique_colors=${titleRegion.uniqueColors} pixels=${titleRegion.pixels}`);
      console.log(`TASK0367_IMAGE_REGION_CONTINUE=Continue unique_colors=${continueRegion.uniqueColors} pixels=${continueRegion.pixels}`);
      console.log(`TASK0367_IMAGE_REGION_BACK=Back unique_colors=${backRegion.uniqueColors} pixels=${backRegion.pixels}`);
      await cdp.send("Target.closeTarget", { targetId }).catch(() => undefined);
    } finally {
      if (ws) ws.close();
      chrome.kill("SIGTERM");
    }
  }, MODULE_RELOAD_BUDGET_MS);
});
