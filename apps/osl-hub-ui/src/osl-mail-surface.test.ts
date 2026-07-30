import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { oslMailStage, oslMailStages } from "./desktop-service-policy";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("OSL Mail surface", () => {
  const scope = functionSource("mailComposerEncryptionScope", "activeHomeAppName");

  it("Show external email encryption scope before send", () => {
    expect(scope).toContain('app?.serviceId !== "email"');
    expect(scope).toContain("app.provider === null");
    expect(scope).toContain('data-mail-composer-encryption-scope="${app.id}"');
    expect(scope).toContain('role="note"');
    expect(scope).toContain('aria-label="Email protection scope"');
    expect(scope).toContain("Before you send");
    expect(scope).toContain("Ordinary external email is not OSL end-to-end encrypted.");
    expect(scope).toContain("Use OSL Chat for verified friends.");
    expect(scope).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);

    const service = functionSource("serviceContent", "activeNativeApp");
    expect(service.indexOf("${mailScope}")).toBeGreaterThanOrEqual(0);
    expect(service.indexOf("${mailScope}")).toBeLessThan(service.indexOf('id="embedded-service-setup"'));

    const header = functionSource("trustedHeader", "homeHeader");
    expect(header).toContain("mailComposerEncryptionScope(activeHomeApp())");
    expect(header.indexOf("${mailScope}")).toBeLessThan(header.indexOf("${localProtection}"));

    const guide = functionSource("serviceGuideContent", "settingsContent");
    expect(guide).toContain("mailComposerEncryptionScope(selectedApp ?? activeHomeApp())");
    expect(guide.indexOf("${mailScope}")).toBeLessThan(guide.indexOf("<footer"));
    expect(styles).toContain(".mail-composer-encryption-scope");
  });

  it("models OSL Mail product stages as a client-protection-first contract", () => {
    expect(oslMailStages.map((stage) => stage.id)).toEqual(["stageA", "stageB", "stageC"]);
    expect(oslMailStage("stageA")).toMatchObject({
      availability: "available",
      externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
    });
    expect(oslMailStage("stageA").boundary).toContain("explicitly authorizes");
    expect(oslMailStage("stageA").excludes).toContain("OSL-operated mailbox");
    expect(oslMailStage("stageB").availability).toBe("comingLater");
    expect(oslMailStage("stageC").availability).toBe("comingLater");
    expect(JSON.stringify(oslMailStages)).not.toMatch(/ordinary external email is OSL end-to-end encrypted|universal encrypted delivery available/i);
  });
});
