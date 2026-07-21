import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("first-party OSL Chats integration", () => {
  it("keeps chat plaintext behind capture resistance and the fixed OSL context", () => {
    const open = source.slice(source.indexOf("async function openOslChat"), source.indexOf("async function approveOslChat"));
    const refresh = source.slice(source.indexOf("async function refreshOslChat"), source.indexOf("async function sendOslChat"));
    expect(open.indexOf("setScreenshotProtection(true)")).toBeLessThan(open.indexOf("listOslChatHistory()"));
    expect(refresh.indexOf("setScreenshotProtection(true)")).toBeLessThan(refresh.indexOf("openOslChatText()"));
    expect(source).toContain('activateOslChatContext(personId)');
  });

  it("persists only unread metadata and never stores message bodies in localStorage", () => {
    expect(source).toContain("osl-chat-unread-v1");
    expect(source).toContain("persistOslChatUnread()");
    expect(source).not.toMatch(/localStorage\.setItem\([^\n]*(?:plaintext|\.body)/u);
    expect(source).toContain("async function syncOslChatsInBackground()");
    expect(source).toContain("function scheduleOslChatBackgroundSync");
    expect(source).not.toContain("setInterval(");
    expect(source).toContain("person.safetyNumberVerified && !person.pendingKeyChange");
    expect(source).toContain("if (!context.scopeApproved) continue");
  });

  it("uses the established encrypted route for view-once without persisting it to history", () => {
    expect(source).toContain("prepareOslChatText(draft, oslChatViewOnce)");
    expect(source).toContain('message.state === "opened"');
    expect(source).toContain("[...durableMessages, ...queuedViewOnce].slice(-200)");
    expect(source).toContain('filter((message) => message.state !== "opened")');
    expect(source.match(/discardOpenedOslChatMessages\(\)/gu)?.length).toBeGreaterThanOrEqual(3);
  });

  it("uses dedicated first-party attachment commands rather than provider attachment IPC", () => {
    expect(source).toContain("selectOslChatAttachment(oslChatViewOnce)");
    expect(source).toContain("listOslChatAttachments()");
    expect(source).toContain("openOslChatAttachment(attachmentId)");
    expect(source).toContain("Other supported files open temporarily in their Windows viewer, which may allow capture.");
  });

  it("makes preview hiding Pro-only and exposes exact per-friend enable/revoke controls", () => {
    expect(source).toContain('licenseState.access === "pro" || licenseState.access === "offlineGrace"');
    expect(source).toContain('id="osl-chat-preview-toggle"');
    expect(source).toContain('id="osl-chat-permission-toggle"');
    expect(source).toContain('setActiveHubFriendPermission(context.contextToken, context.personId, next, false)');
  });

  it("labels provider server capabilities as unavailable instead of faking support", () => {
    expect(source).toContain('id: "osl-servers", name: "Servers", state: "Roadmap", available: true');
    expect(source).toContain('["Discord servers", "Not available yet"]');
    expect(source).toContain('["Telegram groups and channels", "Not available yet"]');
    expect(source).toContain('["Signal groups", "Not available yet"]');
    expect(source).toContain('["Snapchat groups", "Not available yet"]');
  });
});
