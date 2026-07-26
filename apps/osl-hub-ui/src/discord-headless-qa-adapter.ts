import { invoke } from "@tauri-apps/api/core";
import {
  parseNativeDiscordOverlayPrepared,
  type NativeDiscordOverlayPrepared,
} from "./overlay-state";

export interface DiscordHeadlessQaPoll {
  openedCount: number;
  pendingViewOnceCount: number;
  acknowledgmentCount: number;
  fetched: number;
}

function parseDiscordHeadlessQaPoll(value: unknown): DiscordHeadlessQaPoll | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = ["openedCount", "pendingViewOnceCount", "acknowledgmentCount", "fetched"] as const;
  if (Object.keys(record).length !== keys.length
    || keys.some((key) => !Number.isSafeInteger(record[key]) || Number(record[key]) < 0)) return null;
  return {
    openedCount: Number(record.openedCount),
    pendingViewOnceCount: Number(record.pendingViewOnceCount),
    acknowledgmentCount: Number(record.acknowledgmentCount),
    fetched: Number(record.fetched),
  };
}

/** Disposable compile-gated QA only; both commands accept zero renderer input. */
export async function runNativeDiscordHeadlessQa(): Promise<NativeDiscordOverlayPrepared | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseNativeDiscordOverlayPrepared(await invoke<unknown>("run_native_discord_headless_qa"));
  } catch {
    return null;
  }
}

export async function pollNativeDiscordHeadlessQa(): Promise<DiscordHeadlessQaPoll | null> {
  if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") return null;
  try {
    return parseDiscordHeadlessQaPoll(await invoke<unknown>("poll_native_discord_headless_qa"));
  } catch {
    return null;
  }
}
