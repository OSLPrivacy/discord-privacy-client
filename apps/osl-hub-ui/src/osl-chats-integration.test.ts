import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const runtimeSource = readFileSync(new URL("./osl-chat-runtime.ts", import.meta.url), "utf8");
const serversViewSource = readFileSync(new URL("./osl-servers-view.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

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
    // The delivery loop itself moved to ./osl-chat-runtime (T14-A0). Its
    // behaviour — route independence, the verified-friend gate, the approved-scope
    // gate, the rotating roster — is asserted behaviourally in
    // ./osl-chat-delivery.test.ts, not as source text.
    //
    // What stays here is the one property that is only meaningful as a
    // whole-file invariant: the chat cadence self-reschedules with a trailing
    // setTimeout (never overlaps, never drifts) rather than a setInterval, and
    // neither the main window nor the delivery runtime owns a repeating timer of
    // any kind for a future change to quietly attach chat work to.
    expect(source).not.toContain("setInterval(");
    expect(runtimeSource).not.toContain("setInterval(");
    expect(source).toContain("person.safetyNumberVerified && !person.pendingKeyChange");
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

  it("uses local per-friend muting without interrupting encrypted receipt", () => {
    expect(source).toContain("osl-chat-muted-people-v1");
    expect(source).toContain('id="osl-chat-mute-toggle"');
    expect(source).toContain("Messages still arrive without creating a local alert.");
    expect(source).toContain("notificationChatActivity && !oslChatMutedPeople.has(personId)");
    expect(source).toContain('data-osl-chat-unmute="${escapeHtml(personId)}"');
  });

  it("keeps migrated chat preference persistence out of plaintext localStorage", () => {
    expect(source).toContain("osl-chat-previews-visible-v1");
    expect(source).toContain("osl-chat-muted-people-v1");
    expect(source).toContain("osl-chat-unread-v1");
    expect(source).toContain("persistSensitiveOslChatJson(oslChatPreviewStorageKey");
    expect(source).toContain("persistSensitiveOslChatJson(oslChatMutedStorageKey");
    expect(source).toContain("persistSensitiveOslChatJson(oslChatUnreadStorageKey");
    expect(source).not.toMatch(/localStorage\.setItem\(\s*oslChat(?:Preview|Muted|Unread)StorageKey/u);
    expect(source).not.toMatch(/localStorage\.setItem\(\s*["']osl-chat-(?:previews-visible|muted-people|unread)-v1/u);
    expect(source).toContain("storage.removeItem(oslChatPreviewStorageKey)");
    expect(source).toContain("storage.removeItem(oslChatMutedStorageKey)");
    expect(source).toContain("storage.removeItem(oslChatUnreadStorageKey)");
  });

  it("separates real chat and security activity controls", () => {
    expect(source).toContain('id="notification-chat-activity"');
    expect(source).toContain('id="notification-security-activity"');
    expect(source).toContain("function visibleAppNotifications()");
    expect(source).toContain('item.detail === "New encrypted message" ? notificationChatActivity : notificationSecurityActivity');
  });

  it("owns the OSL Chat notification settings surface and keeps previews local", () => {
    const content = functionSource("notificationSettingsContent", "oslChatNotificationSettings");
    const chatSettings = functionSource("oslChatNotificationSettings", "visibleAppNotifications");
    const binding = functionSource("bindWorkspace", "ttlSeconds");

    expect(content).toContain("oslChatNotificationSettings()");
    expect(chatSettings).toContain('class="settings-list osl-chat-notification-settings"');
    expect(chatSettings).toContain('id="notification-chat-activity"');
    expect(chatSettings).toContain('id="osl-chat-preview-toggle"');
    expect(chatSettings).toContain("Hide message previews on this device.");
    expect(chatSettings).not.toContain("Preview hiding is available with Pro.");
    expect(chatSettings).toContain('data-osl-chat-unmute="${escapeHtml(personId)}"');
    expect(chatSettings).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
    expect(binding).toContain("persistOslChatPreviewVisibility()");
    expect(binding).not.toMatch(/localStorage\.setItem\(\s*oslChatPreviewStorageKey/u);
  });

  it("labels provider server capabilities as unavailable instead of faking support", () => {
    expect(source).toContain('if (route === "osl-servers") return oslServersContent()');
    expect(source).toContain('import { oslServersViewMarkup } from "./osl-servers-view"');
    expect(serversViewSource).toContain('["Discord servers", "Not available yet"]');
    expect(serversViewSource).toContain('["Telegram groups and channels", "Not available yet"]');
    expect(serversViewSource).toContain('["Signal groups", "Not available yet"]');
    expect(serversViewSource).toContain('["Snapchat groups", "Not available yet"]');
  });
});
