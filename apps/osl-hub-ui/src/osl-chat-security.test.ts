import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const nativeSource = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const brokerSource = readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");

function functionBody(name: string, nextName: string): string {
  const start = source.indexOf(`async function ${name}`);
  const end = source.indexOf(`async function ${nextName}`, start + 1);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("first-party OSL Chat plaintext boundary", () => {
  it("enables capture resistance before requesting encrypted-at-rest history", () => {
    const open = functionBody("openOslChat", "approveOslChat");
    expect(open.indexOf("await setScreenshotProtection(true)")).toBeLessThan(open.indexOf("await listOslChatHistory()"));
    expect(open).toContain("if (resolvedContext.scopeApproved)");
  });

  it("also enforces capture resistance at the native history IPC boundary", () => {
    const start = nativeSource.indexOf("async fn list_osl_chat_history");
    const end = nativeSource.indexOf("async fn select_osl_chat_attachment", start + 1);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const historyCommand = nativeSource.slice(start, end);
    expect(historyCommand.indexOf("screenshot::apply_to_window")).toBeLessThan(historyCommand.indexOf("broker::load_osl_chat_history"));
    expect(historyCommand).toContain("ScreenshotProtection::On");
  });

  it("requires exact friend approval and decrypted-display policy before native history reads", () => {
    const start = brokerSource.indexOf("pub fn load_osl_chat_history");
    const end = brokerSource.indexOf("pub fn begin_native_overlay_attachment", start + 1);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const historyLoad = brokerSource.slice(start, end);
    expect(historyLoad.indexOf("decrypt_display_enabled")).toBeLessThan(historyLoad.indexOf("cmd_osl_load_channel_history"));
    expect(historyLoad.indexOf("require_manual_peer_scope_approved")).toBeLessThan(historyLoad.indexOf("cmd_osl_load_channel_history"));
  });

  it("serializes native context revocation with in-flight account operations", () => {
    const start = nativeSource.indexOf("async fn close_osl_chat_context");
    const end = nativeSource.indexOf("async fn prepare_peer_prose_text", start + 1);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const closeCommand = nativeSource.slice(start, end);
    expect(closeCommand.indexOf("session.transition.lock().await")).toBeLessThan(closeCommand.indexOf("broker.clear_osl_chat_context()"));
  });

  it("does not overwrite the user's capture preference when enforcing sender policy", () => {
    const refresh = functionBody("refreshOslChat", "sendOslChat");
    expect(refresh.indexOf("await setScreenshotProtection(true)")).toBeLessThan(refresh.indexOf("await openOslChatText()"));
    expect(refresh).not.toContain("windowCaptureEnabled = false");
  });

  it("keeps destructive receive navigation locked and reports conservative receipts", () => {
    expect(source).toContain('id="osl-chat-back" type="button" ${oslChatBusy ? "disabled" : ""}');
    expect(source).toContain('state: "sent"');
    expect(source).not.toContain('state: "delivered"');
  });

  it("escapes decrypted friend previews before inserting Home markup", () => {
    expect(source).toContain("<small>${escapeHtml(chatState)}</small>");
  });

  it("revokes native chat authority before any route can leave OSL Chat", () => {
    expect(source).toContain('if (route === "osl-chat")');
    expect(source).toContain('if (!(await closeOslChatContext()))');
    const close = functionBody("closeOslChat", "submitFriendCode");
    expect(close.indexOf("await closeOslChatContext()")).toBeLessThan(close.indexOf("resetOslChatUiState(false)"));
  });
});
