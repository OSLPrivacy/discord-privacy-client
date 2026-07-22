import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("real text-only OSL Chats integration", () => {
  it("enables capture resistance before history and inbox plaintext", () => {
    const open = source.slice(source.indexOf("async function openOslChat"), source.indexOf("function commitOslChatBatch"));
    const refresh = source.slice(source.indexOf("async function refreshOslChat"), source.indexOf("async function sendOslChat"));
    expect(open.indexOf("setScreenshotProtection(true)")).toBeLessThan(open.indexOf("listOslChatHistory()"));
    expect(refresh.indexOf("setScreenshotProtection(true)")).toBeLessThan(refresh.indexOf("openOslChatText()"));
  });

  it("persists only unread and mute metadata and polls without setInterval", () => {
    expect(source).toContain("osl-chat-unread-v1");
    expect(source).toContain("osl-chat-muted-people-v1");
    expect(source).toContain("async function syncOslChatsInBackground");
    expect(source).toContain("function scheduleOslChatBackgroundSync");
    expect(source).not.toContain("setInterval(");
    expect(source).not.toMatch(/localStorage\.setItem\([^\n]*(?:plaintext|\.body)/u);
  });

  it("routes Home to real chat and excludes attachments and native Discord", () => {
    expect(source).toContain('if (id === "osl-chats")');
    expect(source).toContain('route = "osl-chat"');
    expect(source).toContain("prepareOslChatText(draft, oslChatViewOnce)");
    expect(source).toContain('message.state !== "opened"');
    expect(source).not.toMatch(/selectOslChatAttachment|openOslChatAttachment|activateNativeManualPeerContext/u);
  });

  it("supports global categories and per-friend mute, including right-click", () => {
    expect(source).toContain('id="notification-chat-activity"');
    expect(source).toContain('id="notification-security-activity"');
    expect(source).toContain('id="osl-chat-mute-toggle"');
    expect(source).toContain('addEventListener("contextmenu"');
    expect(source).toContain("notificationChatActivity && !oslChatMutedPeople.has(personId)");
  });

  it("uses one fail-closed navigation path for Home controls while a chat is active", () => {
    const navigation = source.slice(source.indexOf("async function navigateWorkspace"), source.indexOf("function bindWorkspace"));
    expect(navigation).toContain('if (route === "osl-chat")');
    expect(navigation.indexOf("closeOslChatContext()")).toBeLessThan(navigation.indexOf("route = requestedRoute"));
    expect(navigation).toContain('friendsDialogOpen = options.openFriends === true && route === "home"');
    expect(source).toContain('navigateWorkspace("home", { openFriends: true })');
    expect(source).toContain('navigateWorkspace("settings", { settingsTarget: "notifications" })');
    expect(source).not.toContain('data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"');
  });

  it("polls view-once as metadata and never exposes background view-once plaintext", () => {
    const commit = source.slice(source.indexOf("function commitOslChatBatch"), source.indexOf("async function syncOslChatsInBackground"));
    const background = source.slice(source.indexOf("async function syncOslChatsInBackground"), source.indexOf("async function toggleOslChatPermission"));
    expect(background).toContain("openOslChatText(false)");
    expect(commit).toContain("background && incoming.viewOnceConsumed");
    expect(commit).toContain("batch.pendingViewOnce");
    expect(commit.indexOf("background && incoming.viewOnceConsumed")).toBeLessThan(commit.indexOf("body: incoming.plaintext"));
    expect(source).toContain('preview: hasPendingViewOnce ? "View-once message"');
    expect(source).toContain("previewVisible: notificationPreviewContent");
    expect(background).not.toContain("filter((message) => message.state === \"opened\")");
  });
});
