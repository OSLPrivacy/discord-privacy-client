import fs from "node:fs";
import { describe, expect, it } from "vitest";

const native = fs.readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const broker = fs.readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");
const qaIdentity = fs.readFileSync(new URL("../../osl-hub/src/discord_qa_identity.rs", import.meta.url), "utf8");
const adapter = fs.readFileSync(new URL("./discord-headless-qa-adapter.ts", import.meta.url), "utf8");
const renderer = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const permissions = fs.readFileSync(new URL("../../osl-hub/permissions/hub.toml", import.meta.url), "utf8");
const capability = fs.readFileSync(new URL("../../osl-hub/capabilities/hub.json", import.meta.url), "utf8");

function nativeCommand(name: string, nextName: string): string {
  const start = native.indexOf(`async fn ${name}`);
  const end = native.indexOf(`async fn ${nextName}`, start + 1);
  expect(start, `${name} must be registered in the native command module`).toBeGreaterThan(-1);
  expect(end, `${nextName} must follow ${name} so its boundary is auditable`).toBeGreaterThan(start);
  return native.slice(start, end);
}

function nativeParameters(command: string): string {
  const match = command.match(/^async fn \w+\(([\s\S]*?)\)\s*->/u);
  expect(match, "native QA command must have an auditable typed signature").not.toBeNull();
  return match?.[1] ?? "";
}

describe("headless Discord P2P QA contract", () => {
  it("keeps send and poll compile-gated, main-window-only, and free of renderer routing inputs", () => {
    expect(native).toMatch(
      /#\[cfg\(feature = "discord-qa-shell"\)\]\s*#\[tauri::command\]\s*async fn run_native_discord_headless_qa/u,
    );
    expect(native).toMatch(
      /#\[cfg\(feature = "discord-qa-shell"\)\]\s*#\[tauri::command\]\s*async fn poll_native_discord_headless_qa/u,
    );

    const send = nativeCommand("run_native_discord_headless_qa", "poll_native_discord_headless_qa");
    // The overlay-session-survives-lock-off work inserted a new command,
    // `rehydrate_native_discord_overlay_history`, directly after poll and
    // before `open_native_discord_overlay_text`. It is legitimately
    // overlay-only (it checks `caller.label() != OVERLAY_LABEL`), so bounding
    // `poll` on the next real neighbour keeps this test pinned to exactly the
    // poll command's own body instead of also swallowing that command's
    // OVERLAY_LABEL check.
    const poll = nativeCommand("poll_native_discord_headless_qa", "rehydrate_native_discord_overlay_history");
    const forbiddenRendererInputs =
      /\b(peer|person|friend|text|plaintext|message|path|url|uri|endpoint|host|account|context|token|timeout|delay|interval)\w*\s*:/iu;

    expect(nativeParameters(send)).not.toMatch(forbiddenRendererInputs);
    expect(nativeParameters(poll)).not.toMatch(forbiddenRendererInputs);
    expect(send).toContain('if caller.label() != "main"');
    expect(poll).toContain('if caller.label() != "main"');
    expect(send).not.toContain("OVERLAY_LABEL");
    expect(poll).not.toContain("OVERLAY_LABEL");

    expect(permissions).toContain('commands.allow = ["run_native_discord_headless_qa"]');
    expect(permissions).toContain('commands.allow = ["poll_native_discord_headless_qa"]');
    expect(capability).toContain('"allow-run-native-discord-headless-qa"');
    expect(capability).toContain('"allow-poll-native-discord-headless-qa"');
  });

  it("derives exactly one stable verified friend natively and sends only the fixed probe", () => {
    const send = nativeCommand("run_native_discord_headless_qa", "poll_native_discord_headless_qa");

    expect(send).toContain("discord_qa_identity::verified_pairing_person_id");
    expect(send).toContain("discord_qa_identity::pairing_root_dir");
    expect(send).not.toContain("keystore::osl_config_dir()");
    expect(send).toContain("security::manual_peer_binding");
    expect(send).toContain("activate_owned_native_manual_peer_context");
    const pairedPeer = qaIdentity.slice(
      qaIdentity.indexOf("pub fn verified_pairing_person_id"),
      qaIdentity.indexOf("fn encode_public_offer"),
    );
    expect(pairedPeer).toContain("matching.len() != 1");
    expect(pairedPeer).toContain("!person.safety_number_verified");
    expect(pairedPeer).toContain("person.pending_key_change");
    expect(pairedPeer).toContain("status.peer_offer_sha256 != hex_digest(&peer_bytes)");
    expect(pairedPeer).toContain("person.person_id == status.peer_person_id");
    expect(pairedPeer).toContain("person.osl_user_id != status.peer_osl_user_id");
    expect(pairedPeer).toContain("person.safety_number != status.peer_safety_number");
    expect(native).toContain('const QA_PROBE_PLAINTEXT: &str = "OSL Discord QA probe";');
    expect(send).toContain("QA_PROBE_PLAINTEXT");
    expect(send).toContain("wait_for_registered_transport(");
    expect(send).toContain("prepare_native_discord_overlay_text");
    expect(send).toContain("record_outbound(");
    for (const phase of ["host", "pairing", "binding", "activation", "permission", "context", "post"]) {
      expect(send).toContain(`run_headless_discord_qa_phase(&registration, "${phase}"`);
    }
    expect(send).toContain('run_headless_discord_qa_phase(&registration, "pre_send_drain"');
    expect(send).toContain("broker::drain_native_discord_overlay_text(");
    expect(send).toContain("discord_qa_inbound_receipt::record_poll(");
    for (const phase of [
      "scope",
      "verified_peer",
      "keyserver_client",
      "recipient_registration",
      "record",
      "encrypt",
    ]) {
      expect(broker).toContain(`"${phase}"`);
    }
    expect(broker).toContain("record_headless_post_control_stage(");
    expect(broker).toContain("client.post_control_inbox(");
    // The passwordless local-receipt bypass used to be gated on the literal QA
    // probe string; it is now gated on the disposable two-VM QA account
    // namespace itself. That is still narrow in the way that matters: the
    // whole call site only exists under `discord-qa-shell`, which Cargo.toml
    // documents as "deliberately a native build feature ... production
    // binaries cannot enter the passwordless path from JavaScript or an
    // environment variable" -- so a shipped build, or any renderer-controlled
    // input, can never reach this branch.
    expect(broker).toContain(
      "let allow_device_bound_qa_receipt_key = native_discord_qa_receipt_context(&context);",
    );
    expect(broker).toContain(
      'fn native_discord_qa_receipt_context(context: &HubConversationContext) -> bool {\n    context.service_id == "discord" && context.account_id.starts_with("native-discord-")',
    );
    expect(broker).toContain('#[cfg(not(feature = "discord-qa-shell"))]');
    expect(broker).toContain("let allow_device_bound_qa_receipt_key = false");
    expect(broker).toContain("local_protected_identity_for_receipt(");
    expect(qaIdentity).toContain("require_installed_device_bound_storage_key");
    expect(qaIdentity).toContain("installed device key does not match its sealed record");
  });

  it("exposes no-argument QA adapters only in the disposable QA build", () => {
    expect(adapter).toMatch(
      /export async function runNativeDiscordHeadlessQa\(\)[\s\S]*?VITE_OSL_DISCORD_QA_SHELL !== "1"[\s\S]*?invoke<unknown>\("run_native_discord_headless_qa"\)/u,
    );
    expect(adapter).toMatch(
      /export async function pollNativeDiscordHeadlessQa\(\)[\s\S]*?VITE_OSL_DISCORD_QA_SHELL !== "1"[\s\S]*?invoke<unknown>\("poll_native_discord_headless_qa"\)/u,
    );
    expect(adapter).not.toMatch(/invoke<unknown>\("run_native_discord_headless_qa",\s*\{/u);
    expect(adapter).not.toMatch(/invoke<unknown>\("poll_native_discord_headless_qa",\s*\{/u);
  });

  it("starts the non-blocking visual attempt after authenticated send without waiting for peer launch", () => {
    expect(renderer).toContain("async function startDiscordQaVisualOverlayAttempt()");
    expect(renderer).toContain("const qaProbe = await runNativeDiscordHeadlessQa()");
    expect(renderer).toContain("void startDiscordQaVisualOverlayAttempt()");
    expect(renderer).toContain("await pollNativeDiscordHeadlessQa()");
    expect(renderer).not.toContain("await startDiscordQaVisualOverlayAttempt()");

    const oneClickStart = renderer.indexOf("async function runDiscordQaOneClick()");
    const headlessSend = renderer.indexOf("const qaProbe = await runNativeDiscordHeadlessQa()", oneClickStart);
    const visualAttempt = renderer.indexOf("void startDiscordQaVisualOverlayAttempt()", oneClickStart);
    const headlessPoll = renderer.indexOf("await pollNativeDiscordHeadlessQa()", oneClickStart);
    expect(oneClickStart).toBeGreaterThan(-1);
    expect(headlessSend).toBeGreaterThan(oneClickStart);
    expect(visualAttempt).toBeGreaterThan(headlessSend);
    expect(headlessPoll).toBeGreaterThan(visualAttempt);
  });
});
