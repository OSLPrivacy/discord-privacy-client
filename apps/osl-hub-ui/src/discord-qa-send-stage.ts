import { invoke } from "@tauri-apps/api/core";

/**
 * Disposable Discord QA only. The protected composer runs inside a WebView with
 * no filesystem authority, so the keyboard and gesture hops of a send are
 * otherwise invisible: a refused Enter looks exactly like an Enter that never
 * arrived. These labels let one QA run name the exact hop that stopped.
 *
 * Every entry is a fixed compile-time constant. No draft text, carrier text,
 * hash, measurement, identifier, or error detail is ever passed here, and the
 * native command re-validates the label against its own allowlist before
 * appending it.
 */
export const DISCORD_QA_SEND_STAGES = [
  "renderer_keydown_observed",
  "renderer_enter_recognised",
  "renderer_enter_refocused_draft",
  "renderer_double_enter_handoff_keydown",
  "renderer_double_enter_handoff_keyup",
  "renderer_send_refused_not_ready",
  "renderer_send_refused_busy",
  "renderer_send_refused_empty_draft",
  "renderer_send_refused_too_large",
  "renderer_send_started",
  "renderer_send_refused_state_unavailable",
  "renderer_send_refused_marker_unavailable",
  "renderer_send_refused_covertext_off",
  "renderer_send_command_invoked",
  "renderer_send_command_rejected",
  "renderer_send_command_accepted",
  "renderer_send_refused_invalid_response",
  "renderer_send_failed",
  "renderer_send_complete",
] as const;

export type DiscordQaSendStage = (typeof DISCORD_QA_SEND_STAGES)[number];

/**
 * Fire-and-forget: recording evidence must never change, delay, or fail a send.
 */
export function recordDiscordQaSendStage(stage: DiscordQaSendStage): void {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return;
  if (!DISCORD_QA_SEND_STAGES.includes(stage)) return;
  void invoke<null>("record_native_discord_qa_send_stage", { stage }).catch(() => undefined);
}
