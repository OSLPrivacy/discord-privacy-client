import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("inbox primary action", () => {
  const action = functionSource("inboxPrimaryAction", "openHomeModule");
  const launcher = functionSource("openHomeModule", "oslChatTimestamp");

  it("opens a private conversation only with a verified stable friend", () => {
    expect(action).toContain("hubPeople.find");
    // The trust condition moved into the shared peerIsVerified predicate
    // (adapters.ts) when T5-U3 wired it up. Assert BOTH halves so the property
    // is still proven end to end: the caller must gate on the predicate, AND
    // the predicate must still mean verified-and-not-key-changed. Asserting
    // only the call would pass even if the predicate were gutted.
    expect(action).toContain("hubPeople.find(peerIsVerified)");
    expect(action).toContain("void openOslChat(first.personId)");
    expect(action.indexOf("peerIsVerified")).toBeLessThan(action.indexOf("openOslChat"));
    const adapters = readFileSync(fileURLToPath(new URL("./adapters.ts", import.meta.url)), "utf8");
    const pStart = adapters.indexOf("export function peerIsVerified");
    expect(pStart, "peerIsVerified should exist").toBeGreaterThanOrEqual(0);
    const predicate = adapters.slice(pStart, adapters.indexOf("\n}", pStart));
    expect(predicate).toContain("person.safetyNumberVerified && !person.pendingKeyChange");
  });

  it("refuses to invent a chat when no verified friend is available", () => {
    expect(action).toMatch(/if \(first\) \{/);
    expect(action).toContain('route = "home"');
    expect(action).toContain("activeOslChatPersonId = null");
    expect(action).toContain("friendsDialogOpen = true");
    expect(action).toMatch(/friendsDialogOpen = true;[\s\S]*?render\(\);/u);
  });

  it("routes the Home OSL Chat tile through the Inbox primary action", () => {
    expect(launcher).toMatch(/if \(id === "osl-chats"\) \{\s*inboxPrimaryAction\(\);/u);
  });

  it("keeps the user-facing action in plain conversation terms", () => {
    expect(action).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
    expect(launcher).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
  });
});
