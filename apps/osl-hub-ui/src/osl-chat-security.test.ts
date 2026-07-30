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
    expect(open).toContain("if (context.scopeApproved)");
  });

  it("does not publish a newly opened chat when capture protection is refused", () => {
    const open = functionBody("openOslChat", "approveOslChat");
    const captureRefusal = open.indexOf("if (!captureReady || epoch !== oslChatOperationEpoch)");
    expect(captureRefusal).toBeGreaterThanOrEqual(0);
    expect(open.indexOf("activeOslChatPersonId = personId")).toBeGreaterThan(captureRefusal);
    expect(open.indexOf('route = "osl-chat"')).toBeGreaterThan(captureRefusal);
    expect(open.indexOf("oslChatUnread.delete(personId)")).toBeGreaterThan(captureRefusal);
    expect(open).not.toContain("activeOslChatPersonId !== personId");
  });

  it("also enforces capture resistance at the native history IPC boundary", () => {
    const start = nativeSource.indexOf("async fn list_osl_chat_history");
    const end = nativeSource.indexOf("async fn select_osl_chat_attachment", start + 1);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const historyCommand = nativeSource.slice(start, end);
    expect(historyCommand.indexOf("screenshot::apply_to_window")).toBeLessThan(historyCommand.indexOf("broker::load_osl_chat_history"));
    // The literal `ScreenshotProtection::On` used to be inlined at the call
    // site; it is now read through `active_osl_capture_protection()`, which
    // is cfg-gated rather than a fixed argument. That helper still resolves
    // to `On` for every real build -- only the disposable, never-shipped
    // `discord-qa-shell` feature (Cargo.toml: "production binaries cannot
    // enter the passwordless path") flips it to `Off`, so the shipped
    // capture-resistance guarantee is unchanged.
    expect(historyCommand).toContain("active_osl_capture_protection()");
    const productionCaptureProtection = nativeSource.slice(
      nativeSource.indexOf('#[cfg(not(feature = "discord-qa-shell"))]\nfn active_osl_capture_protection'),
      nativeSource.indexOf("#[cfg(feature = \"discord-qa-shell\")]\nfn qa_discord_overlay_stage"),
    );
    expect(productionCaptureProtection).toContain("runtime::ScreenshotProtection::On");
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
    expect(source).toContain('state: "sent" as const');
    expect(source).not.toContain('state: "delivered" as const');
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
