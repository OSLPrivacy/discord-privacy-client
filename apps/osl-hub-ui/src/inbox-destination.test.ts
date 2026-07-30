import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Inbox destination content", () => {
  const inbox = functionSource("inboxDestinationContent", "oslChatContent");

  it("renders the conversation destination with the required filters", () => {
    expect(inbox).toContain('class="content-viewport inbox-destination"');
    expect(inbox).toContain("<h1>Conversations</h1>");
    expect(inbox).toContain('data-inbox-filter="${label.toLowerCase()}"');
    for (const filter of ['"All"', '"OSL"', '"Connected"', '"Requests"']) {
      expect(inbox).toContain(filter);
    }
  });

  it("shows OSL Chat, Circles, Mail, and connected account sections with honest protection labels", () => {
    expect(inbox).toContain("OSL Chat");
    expect(inbox).toContain("OSL Circles");
    expect(inbox).toContain("OSL Mail");
    expect(inbox).toContain("Encrypted for verified friends");
    expect(inbox).toContain("External recipients are not OSL E2EE");
    expect(inbox).toContain("External recipient, not OSL E2EE");
    expect(inbox).toContain("OSL overlay active when a conversation is verified");
    expect(inbox).toContain("connected accounts");
    expect(inbox).toContain("Connect an app from Home");
  });

  it("refuses unavailable protected conversation states instead of implying permission", () => {
    expect(inbox).toContain("refuses protected send when the conversation cannot be verified");
    expect(inbox).toContain("Verify a friend before starting an encrypted OSL chat.");
    expect(inbox).toContain("Verification needed before protected chat");
    expect(inbox).toContain("Security change needs review");
    expect(inbox).not.toMatch(/auto.?retry|retry automatically|silently send/i);
  });

  it("keeps implementation concepts out of visible Inbox copy", () => {
    const visibleCopy = inbox.replace(/\$\{[^}]+\}/g, "");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
  });
});
