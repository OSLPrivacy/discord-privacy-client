import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

export type UpdateStatus =
  | { state: "unavailable" }
  | { state: "checking" }
  | { state: "upToDate"; current: string }
  | { state: "available"; current: string; next: string; notes: string }
  | { state: "installing"; current: string; next: string; notes: string }
  | { state: "error" };

export function parseUpdateCheck(raw: unknown): UpdateStatus {
  if (!isRecord(raw) || typeof raw.status !== "string") return { state: "error" };
  if (raw.status === "up_to_date" && hasExactKeys(raw, ["status", "current"]) && isVersion(raw.current)) {
    return { state: "upToDate", current: raw.current };
  }
  if (
    raw.status === "update_available"
    && hasExactKeys(raw, ["status", "current", "next", "notes"])
    && isVersion(raw.current)
    && isVersion(raw.next)
    && isPlainText(raw.notes, 2_000)
  ) return { state: "available", current: raw.current, next: raw.next, notes: raw.notes };
  if (raw.status === "error" && hasExactKeys(raw, ["status"])) return { state: "error" };
  return { state: "error" };
}

export async function checkHubForUpdates(): Promise<UpdateStatus> {
  if (!isTauriRuntime()) return { state: "unavailable" };
  try { return parseUpdateCheck(await invoke<unknown>("check_hub_for_updates")); }
  catch { return { state: "unavailable" }; }
}

export async function installHubUpdate(expectedVersion: string): Promise<"noUpdate" | "error"> {
  if (!isTauriRuntime() || !isVersion(expectedVersion)) return "error";
  try {
    const raw = await invoke<unknown>("install_hub_update", { expectedVersion });
    if (isRecord(raw) && hasExactKeys(raw, ["status"]) && raw.status === "no_update") return "noUpdate";
    return "error";
  } catch { return "error"; }
}

export async function openHubReleasesPage(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try { await invoke("open_hub_releases_page"); return true; }
  catch { return false; }
}

function isVersion(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 64 && /^[0-9A-Za-z][0-9A-Za-z.+-]*$/.test(value);
}

function isPlainText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length <= max && !/[<>\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && actual.every((key) => keys.includes(key));
}
