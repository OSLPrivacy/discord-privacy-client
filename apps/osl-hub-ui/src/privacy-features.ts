import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

const maximumExternalUrlLength = 2_048;
const maximumLinksPerMessage = 8;

const linkGrabberDomains = [
  "2no.co",
  "blasze.com",
  "canarytokens.com",
  "grabify.link",
  "ipgrabber.ru",
  "iplogger.co",
  "iplogger.com",
  "iplogger.org",
  "iplogger.ru",
  "ps3cfw.com",
  "whatstheirip.com",
  "yip.su",
] as const;

export type ExternalLinkDecision =
  | { allowed: true; normalizedUrl: string; hostname: string }
  | { allowed: false; reason: "invalid" | "knownLinkGrabber" };

function parseExternalHttpUrl(value: string): URL | null {
  if (!value || value.length > maximumExternalUrlLength) return null;
  try {
    const parsed = new URL(value);
    if (!["http:", "https:"].includes(parsed.protocol) || parsed.username || parsed.password) return null;
    if (!parsed.hostname || parsed.hostname.length > 253) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function isKnownLinkGrabberHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase().replace(/\.$/u, "");
  return linkGrabberDomains.some((domain) => normalized === domain || normalized.endsWith(`.${domain}`));
}

export function checkExternalLink(value: string, blockKnownGrabbers: boolean): ExternalLinkDecision {
  const parsed = parseExternalHttpUrl(value);
  if (!parsed) return { allowed: false, reason: "invalid" };
  if (blockKnownGrabbers && isKnownLinkGrabberHostname(parsed.hostname)) {
    return { allowed: false, reason: "knownLinkGrabber" };
  }
  return { allowed: true, normalizedUrl: parsed.href, hostname: parsed.hostname };
}

export function externalHttpUrls(value: string): string[] {
  const candidates = value.match(/https?:\/\/[^\s<>"']+/giu) ?? [];
  const urls: string[] = [];
  for (const candidate of candidates) {
    const trimmed = candidate.replace(/[),.;!?]+$/u, "");
    const decision = checkExternalLink(trimmed, false);
    if (!decision.allowed || urls.includes(decision.normalizedUrl)) continue;
    urls.push(decision.normalizedUrl);
    if (urls.length === maximumLinksPerMessage) break;
  }
  return urls;
}

export async function openExternalLinkInDefaultBrowser(url: string): Promise<boolean> {
  if (!isTauriRuntime() || !checkExternalLink(url, false).allowed) return false;
  try {
    await invoke("open_external_link_in_default_browser", { url });
    return true;
  } catch {
    return false;
  }
}

export async function lockHubSession(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try {
    await invoke("lock_hub_session");
    return true;
  } catch {
    return false;
  }
}

export async function scheduleProtectedClipboardClear(timeoutSeconds: number): Promise<boolean> {
  if (!isTauriRuntime() || !Number.isSafeInteger(timeoutSeconds) || timeoutSeconds < 5 || timeoutSeconds > 300) return false;
  try {
    await invoke("schedule_protected_clipboard_clear", { timeoutSeconds });
    return true;
  } catch {
    return false;
  }
}
