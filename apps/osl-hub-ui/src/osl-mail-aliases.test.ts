import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("OSL Mail aliases", () => {
  it("renders Stage B aliases and relay after client protection", () => {
    const { __oslHubUiTest, oslMailStageAContent, oslMailStageBContent } = ui;
    __oslHubUiTest.reset({ route: "inbox" });

    const stageA = oslMailStageAContent();
    const stageB = oslMailStageBContent();
    const html = __oslHubUiTest.renderWorkspaceContent("inbox");

    for (const rendered of [stageB, html]) {
      expect(rendered).toContain('data-osl-mail-stage-b="coming-later"');
      expect(rendered).toContain('data-osl-mail-stage-b-after="client-protection"');
      expect(rendered).toContain('data-osl-mail-aliases="refused"');
      expect(rendered).toContain('data-osl-mail-relay="refused"');
      expect(rendered).toContain("<strong>Aliases and relay</strong>");
      expect(rendered).toContain("<small>After client protection</small>");
      expect(rendered).toContain("Aliases and relay come after client protection");
      expect(rendered).toContain("abuse handling, deliverability, reply routing, account recovery, and support operations pass review");
      expect(rendered).toContain("External email remains ordinary email unless a supported encrypted path is selected before send.");
      expect(rendered).not.toMatch(/ordinary external email is OSL end-to-end encrypted|universal encrypted delivery|silent mailbox import|auto.?retry|retry automatically/i);
      expect(rendered).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    }

    expect(html.indexOf(stageA)).toBeGreaterThanOrEqual(0);
    expect(html.indexOf(stageB)).toBeGreaterThan(html.indexOf(stageA));
  });
});
