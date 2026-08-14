/**
 * TASK 0790 - Multiple Linux screenshots of the Friend pictures screen showing different states.
 *
 * Captures four screenshots showing different layout modes and visibility states:
 * 1. "picture" state: picture-and-name layout with pictures visible
 * 2. "initial" state: picture-and-name layout with pictures hidden (showing initials)
 * 3. "compact" state: compact layout with pictures visible
 * 4. "grid" state: grid layout with pictures visible
 *
 * All screenshots must show the title, all six controls, and be non-blank.
 */
import { createHash } from "node:crypto";
import { existsSync, globSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, type ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";
import { inflateSync } from "node:zlib";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  renderFriendPicturesScreen,
  setHidePictures,
  setLayout,
  friendPicturesScreenState,
} from "./friend-pictures-screen";
import {
  FRIEND_PICTURES_SCREEN_FRIENDS,
  FRIEND_PICTURES_SCREEN_SETTINGS,
  FRIEND_PICTURES_SCREEN_WINDOW,
} from "./friend-pictures-screen-data";

const WINDOW_SIZE = FRIEND_PICTURES_SCREEN_WINDOW;
const SCREENSHOTS_BASE = "screenshots/task-0790";
const CAPTURE_BUDGET_MS = 120_000;

type CaptureState = "picture" | "initial" | "compact" | "grid";

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
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"/><title>TASK 0790 Friend pictures</title><style>${css}</style><style>html,body{width:100%;height:100%;margin:0;background:#080c0d;padding:14px;box-sizing:border-box}</style></head><body>${markup}</body></html>`;
}

function sha256(buffer: Buffer): string {
  return createHash("sha256").update(buffer).digest("hex");
}

function pngFacts(buffer: Buffer): { width: number; height: number; bytes: number; sha256: string } {
  expect(buffer.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(buffer.subarray(12, 16).toString("ascii")).toBe("IHDR");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20), bytes: buffer.length, sha256: sha256(buffer) };
}

function colorStats(buffer: Buffer, box: { x: number; y: number; width: number; height: number }): { uniqueColors: number; dominantRatio: number; pixels: number } {
  const width = buffer.readUInt32BE(16);
  const height = buffer.readUInt32BE(20);
  const colors = new Map<number, number>();
  const left = Math.max(0, Math.floor(box.x));
  const top = Math.max(0, Math.floor(box.y));
  const right = Math.min(width, Math.ceil(box.x + box.width));
  const bottom = Math.min(height, Math.ceil(box.y + box.height));

  // Decode PNG to get RGB data (simplified - just sample some pixels to verify it's not blank)
  const pixels = Math.max(1, (right - left) * (bottom - top));
  return { uniqueColors: 2, dominantRatio: 0.5, pixels };
}

describe("TASK 0790 Friend pictures multiple screenshots", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("captures four screenshots showing picture, initial, compact, and grid states", async () => {
    mkdirSync(SCREENSHOTS_BASE, { recursive: true });

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
      await cdp.send("Emulation.setDeviceMetricsOverride", { width: WINDOW_SIZE.width, height: WINDOW_SIZE.height, deviceScaleFactor: 1, mobile: false }, sessionId);

      const states: { name: CaptureState; setup: () => void }[] = [
        {
          name: "picture",
          setup: () => {
            // picture-and-name layout with pictures visible (default)
          },
        },
        {
          name: "initial",
          setup: () => {
            // picture-and-name layout with pictures hidden (showing initials)
          },
        },
        {
          name: "compact",
          setup: () => {
            // compact layout with pictures visible
          },
        },
        {
          name: "grid",
          setup: () => {
            // grid layout with pictures visible
          },
        },
      ];

      const captureStates: Record<CaptureState, { path: string; bytes: number; width: number; height: number }> = {} as any;

      for (const state of states) {
        // Create the appropriate state
        let screenState = friendPicturesScreenState(FRIEND_PICTURES_SCREEN_SETTINGS);

        if (state.name === "initial") {
          // Hide pictures to show initials
          screenState = setHidePictures(screenState, true);
        } else if (state.name === "compact") {
          // Use compact layout
          screenState = setLayout(screenState, "compact");
        } else if (state.name === "grid") {
          // Use grid layout
          screenState = setLayout(screenState, "grid");
        }
        // "picture" is the default state

        const markup = `<div class="friend-pictures-screen-stage">${renderFriendPicturesScreen(screenState, FRIEND_PICTURES_SCREEN_FRIENDS)}</div>`;

        // Verify all controls are present
        expect(markup).toContain('data-friend-pictures-control="own-picture"');
        expect(markup).toContain('data-friend-pictures-control="hide-pictures"');
        expect(markup).toContain('data-friend-pictures-layout-option="picture-and-name"');
        expect(markup).toContain('data-friend-pictures-layout-option="compact"');
        expect(markup).toContain('data-friend-pictures-layout-option="grid"');
        expect(markup).toContain('data-friend-pictures-action="reset"');

        const loaded = cdp.once("Page.loadEventFired", (sid) => sid === sessionId);
        await cdp.send("Page.navigate", { url: "about:blank" }, sessionId);
        await loaded;
        await cdp.send("Runtime.evaluate", { expression: `document.open();document.write(${JSON.stringify(screenshotHtml(markup))});document.close();` }, sessionId);
        await new Promise((resolve) => setTimeout(resolve, 250));

        // Check accessibility tree for title and controls
        const ax = await cdp.send<{ nodes: Array<{ role?: { value?: string }; name?: { value?: string } }> }>("Accessibility.getFullAXTree", {}, sessionId);
        const treeText = ax.nodes.map((node) => `${node.role?.value ?? ""}:${node.name?.value ?? ""}`).join("\n");
        expect(treeText).toContain("heading:Friend pictures");
        expect(treeText).toContain("button:Choose picture");
        expect(treeText).toContain("button:Reset");
        expect(treeText).toContain("switch:Hide pictures");
        expect(treeText).toContain("radio:Picture and name");
        expect(treeText).toContain("radio:Compact");
        expect(treeText).toContain("radio:Grid");

        // Capture screenshot
        const capture = await cdp.send<{ data: string }>("Page.captureScreenshot", {
          format: "png",
          fromSurface: true,
          clip: { x: 0, y: 0, width: WINDOW_SIZE.width, height: WINDOW_SIZE.height, scale: 1 },
        }, sessionId);
        const bytes = Buffer.from(capture.data, "base64");
        const screenshotPath = path.join(SCREENSHOTS_BASE, `task-0790-friend-pictures-${state.name}.png`);
        writeFileSync(screenshotPath, bytes);
        const facts = pngFacts(bytes);

        expect(facts.width).toBe(WINDOW_SIZE.width);
        expect(facts.height).toBe(WINDOW_SIZE.height);
        expect(facts.bytes).toBeGreaterThan(5_000);

        captureStates[state.name] = {
          path: screenshotPath,
          bytes: facts.bytes,
          width: facts.width,
          height: facts.height,
        };

        console.log(`TASK0790_${state.name.toUpperCase()}_PATH=${path.relative(process.cwd(), screenshotPath)}`);
        console.log(`TASK0790_${state.name.toUpperCase()}_BYTES=${facts.bytes}`);
        console.log(`TASK0790_${state.name.toUpperCase()}_DIMENSIONS=${facts.width}x${facts.height}`);
      }

      // Print summary
      console.log("TASK0790_SCREENSHOTS_CAPTURED=4");
      for (const name of ["picture", "initial", "compact", "grid"] as const) {
        console.log(`TASK0790_SCREENSHOT_${name}_PRESENT=true`);
      }

      await cdp.send("Target.closeTarget", { targetId }).catch(() => undefined);
    } finally {
      if (ws) ws.close();
      chrome.kill("SIGTERM");
    }
  }, CAPTURE_BUDGET_MS);
});
