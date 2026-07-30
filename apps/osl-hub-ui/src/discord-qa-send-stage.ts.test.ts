import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { DISCORD_QA_SEND_STAGES } from "./discord-qa-send-stage";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("discord QA send stages", () => {
  it("d4 qualifies Double Enter handoff stages on renderer and native allowlist", () => {
    expect(new Set(DISCORD_QA_SEND_STAGES).size).toBe(DISCORD_QA_SEND_STAGES.length);
    expect(DISCORD_QA_SEND_STAGES).toContain("renderer_double_enter_handoff_keydown");
    expect(DISCORD_QA_SEND_STAGES).toContain("renderer_double_enter_handoff_keyup");

    const native = readRelative("../../osl-hub/src/main.rs");
    const overlay = readRelative("./overlay.ts");
    const allowlist = native.slice(
      native.indexOf("const QA_RENDERER_SEND_STAGES"),
      native.indexOf("const QA_ATOMIC_SEND_MARKER"),
    );
    const atomicSend = native.slice(
      native.indexOf("async fn send_native_discord_qa_atomic_text"),
      native.indexOf("async fn send_native_discord_qa_probe"),
    );

    expect(allowlist).toContain("const QA_RENDERER_SEND_STAGES: [&str; 19]");
    expect(allowlist).toContain('"renderer_double_enter_handoff_keydown"');
    expect(allowlist).toContain('"renderer_double_enter_handoff_keyup"');
    expect(atomicSend).toContain(".protected_send_outcome(carrier.placed, carrier.enter_sent)");
    expect(atomicSend).toContain("carrier_outcome == DiscordProtectedSendOutcome::Sent");
    expect(overlay).toContain('sendMode.value === "double"');
    expect(overlay).toContain("let qaDoubleEnterHandoffKeyDown = false;");
    expect(overlay).toContain("qaDoubleEnterHandoffKeyDown = true;");
    expect(overlay).toContain('recordDiscordQaSendStage("renderer_double_enter_handoff_keydown")');
    expect(overlay).toContain("!qaDoubleEnterHandoffKeyDown");
    expect(overlay).toContain('recordDiscordQaSendStage("renderer_double_enter_handoff_keyup")');
    expect(overlay).toContain("event.stopPropagation();");
    expect(overlay.indexOf("sendGesture.keydown(keyboardGesture(event))")).toBeLessThan(
      overlay.indexOf("sendGesture.keyup(keyboardGesture(event))"),
    );
  });
});
