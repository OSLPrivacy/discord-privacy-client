import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { oslMailStage, oslMailStages } from "./desktop-service-policy";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

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

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("OSL Mail surface", () => {
  const scope = functionSource("mailComposerEncryptionScope", "oslMailContent");

  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Show external email encryption scope before send", () => {
    expect(scope).toContain('app?.serviceId !== "email"');
    expect(scope).toContain("app.provider === null");
    expect(scope).toContain('data-mail-composer-encryption-scope="${app.id}"');
    expect(scope).toContain('role="note"');
    expect(scope).toContain('aria-label="Email protection scope"');
    expect(scope).toContain("Before you send");
    expect(scope).toContain("Ordinary external email uses the mail provider's delivery path.");
    expect(scope).toContain("Use OSL Chat for verified friends.");
    expect(scope).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);

    const service = functionSource("serviceContent", "activeNativeApp");
    expect(service.indexOf("${mailScope}")).toBeGreaterThanOrEqual(0);
    expect(service.indexOf("${mailScope}")).toBeLessThan(service.indexOf('id="embedded-service-setup"'));

    const header = functionSource("trustedHeader", "homeHeader");
    expect(header).toContain("mailComposerEncryptionScope(activeHomeApp())");
    expect(header.indexOf("${mailScope}")).toBeLessThan(header.indexOf("${localProtection}"));

    const guide = functionSource("serviceGuideContent", "settingsContent");
    expect(guide).toContain("mailComposerProtectionNote(selectedApp ?? activeHomeApp())");
    expect(guide.indexOf("${mailNote}")).toBeLessThan(guide.indexOf("<footer"));
    expect(styles).toContain(".mail-composer-encryption-scope");
  });

  it("renders OSL Mail as unavailable until its desktop bridge exists", async () => {
    const { __oslHubUiTest, oslMailStageAContent } = await loadUi();
    __oslHubUiTest.reset({ route: "inbox" });

    const card = oslMailStageAContent();
    const html = __oslHubUiTest.renderWorkspaceContent("inbox");

    for (const rendered of [card, html]) {
      expect(rendered).toContain('data-inbox-osl-surface="mail"');
      expect(rendered).toContain('data-osl-mail-stage-a="unavailable"');
      expect(rendered).toContain('data-osl-mail-protection="private-client"');
      expect(rendered).toContain('data-mailbox-operations="refused"');
      expect(rendered).toContain('data-osl-mailbox-stage-c-gate="stage-c-coming-later"');
      expect(rendered).toContain("<small>Private client protection</small>");
      expect(rendered).toContain("OSL Mail client protection is unavailable until its desktop bridge exists.");
      expect(rendered).not.toMatch(/ordinary external email is OSL end-to-end encrypted|universal encrypted delivery|silent mailbox import|auto.?retry|retry automatically/i);
      expect(rendered).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    }
    expect(html).toContain('class="inbox-surface-card unavailable" data-inbox-osl-surface="mail"');
  });

  it("models OSL Mail product stages without claiming its missing bridge", () => {
    expect(oslMailStages.map((stage) => stage.id)).toEqual(["stageA", "stageB", "stageC"]);
    expect(oslMailStage("stageA")).toMatchObject({
      availability: "comingLater",
      externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
    });
    expect(oslMailStage("stageA").boundary).toContain("desktop bridge exists");
    expect(oslMailStage("stageA").excludes).toContain("OSL-operated mailbox");
    expect(oslMailStage("stageB").availability).toBe("comingLater");
    expect(oslMailStage("stageC").availability).toBe("comingLater");
    expect(JSON.stringify(oslMailStages)).not.toMatch(/ordinary external email is OSL end-to-end encrypted|universal encrypted delivery available/i);
  });
});
