// Throwaway probe for TASK 0819 — deleted before commit.
import { beforeAll, describe, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  mocks.invoke.mockResolvedValue(undefined);
  mocks.listen.mockResolvedValue(() => undefined);
  mocks.getCurrentWindow.mockReturnValue({ onFocusChanged: vi.fn().mockResolvedValue(() => undefined) });
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

function visibleText(markup: string): string {
  return markup.replace(/<[^>]+>/gu, " ").replace(/&nbsp;/gu, " ").replace(/&amp;/gu, "&").replace(/\s+/gu, " ").trim();
}

describe("probe", () => {
  it("dumps home shell facts", () => {
    ui.__oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
    const shell = ui.__oslHubUiTest.renderRouteShell("home");
    const barStart = shell.indexOf(`class="home-header home-command-bar"`);
    const bar = shell.slice(barStart, shell.indexOf("</header>", barStart));
    ui.__oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
    const content = ui.__oslHubUiTest.renderWorkspaceContent("home");
    const barText = visibleText(bar);
    const contentText = visibleText(content);
    console.log("BAR TEXT:", JSON.stringify(barText));
    console.log("BAR WORDS:", barText.split(/\s+/).filter(Boolean).length);
    const h1 = /<h1\b[^>]*>([\s\S]*?)<\/h1>/iu.exec(content);
    console.log("H1:", JSON.stringify(h1 ? visibleText(h1[1]) : null));
    console.log("CONTENT WORDS:", contentText.split(/\s+/).filter(Boolean).length);
    console.log("CONTENT TEXT:", JSON.stringify(contentText).slice(0, 3000));
    for (const word of ["Home", "Search", "Settings", "Friends", "Messages", "Profile", "Notifications"]) {
      const pat = new RegExp(`(?<![\\p{L}\\p{N}])${word}(?![\\p{L}\\p{N}])`, "iu");
      console.log(`WORD ${word}: bar=${pat.test(barText)} content=${pat.test(contentText)}`);
    }
    const families: Array<[string, RegExp]> = [
      ["deep-simplicity", /\b(?:keyservers?|ratchets?|browser profiles?|provider adapters?|protocol state|storage layout|automation internals|transport plumbing|service-adapter mechanics)\b/iu],
      ["scary", /\b(?:attacks?|attackers?|adversar(?:y|ies)|malicious|hackers?|hacked|breach(?:es|ed)?|threats?|eavesdrops?|eavesdropping|intercepts?|intercepted|impersonat(?:e|es|ed|ion)|spoof(?:s|ed|ing)?|compromis(?:e|es|ed)|man-in-the-middle|MITM)\b/iu],
      ["technical", /\b(?:cryptograph(?:y|ic)|encrypt(?:s|ed|ion)?|decrypt(?:s|ed|ion)?|cipher(?:text)?|plaintext|handshakes?|fingerprints?|public keys?|private keys?|key exchange|hashe?s?|nonces?|entropy|protocols?|certificates?|metadata|payloads?|X3DH|PQXDH|TOFU|SAS)\b/iu],
      ["overclaim", /\b(?:military[- ]grade|bank[- ]level|unhackable|uncrackable|NSA[- ]proof|100% secure|absolutely secure|totally secure|complete privacy|total privacy)\b/iu],
    ];
    for (const [family, pat] of families) {
      console.log(`BANNED ${family}: bar=${JSON.stringify(pat.exec(barText)?.[0] ?? null)} content=${JSON.stringify(pat.exec(contentText)?.[0] ?? null)}`);
    }
  });
});
