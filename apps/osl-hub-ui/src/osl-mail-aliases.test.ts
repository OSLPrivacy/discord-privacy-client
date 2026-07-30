import { beforeEach, describe, expect, it, vi } from "vitest";

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("OSL Mail aliases", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders Stage B aliases and relay after client protection", async () => {
    const { __oslHubUiTest, oslMailStageAContent, oslMailStageBContent } = await loadUi();
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
