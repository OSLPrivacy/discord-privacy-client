/** UI-only network route vocabulary shared with the onboarding choice. */
export type UiNetworkRoute = "tor" | "direct" | null;

/**
 * Attachments at or above this size get explicit Tor pacing copy while active.
 * Ten MiB is large enough that coarse 25% native progress still represents a
 * meaningful byte count, without calling an ordinary photo "large".
 */
export const LARGE_TOR_ATTACHMENT_BYTES = 10 * 1024 * 1024;

export const TOR_SLOW_ATTACHMENT_LABEL = "Slow -- still sending";
export const TOR_KEYSERVER_WAIT_LABEL = "Checking keys over Tor";
export const TOR_KEYSERVER_WAIT_DELAY_MS = 10_000;

/** This release has no voice client, so this flag must never gate a control. */
export const VOICE_CLIENT_SHIPS_THIS_RELEASE = false;

export interface DirectVoiceCallChoice {
  id: "direct";
  label: "Direct";
  disclosure: string;
}

/**
 * Contract for the future per-call exception sheet. It is intentionally data,
 * not markup: no call or voice control ships until a voice client exists.
 */
export const directVoiceCallChoice = (): DirectVoiceCallChoice => ({
  id: "direct",
  label: "Direct",
  disclosure: "This call will connect directly and may reveal your network path to the voice server.",
});

export function attachmentBytesSent(totalBytes: number, progressPercent: number): number {
  if (!Number.isSafeInteger(totalBytes) || totalBytes <= 0) return 0;
  if (!Number.isInteger(progressPercent) || progressPercent <= 0) return 0;
  if (progressPercent >= 100) return totalBytes;
  return Math.floor((totalBytes * progressPercent) / 100);
}

export function isSlowActiveTorAttachment(
  route: UiNetworkRoute,
  totalBytes: number,
  stage: string,
  progressPercent: number,
): boolean {
  return route === "tor"
    && totalBytes >= LARGE_TOR_ATTACHMENT_BYTES
    && (stage === "uploading" || stage === "delivering")
    && progressPercent < 100;
}

export interface TorKeyserverPollingOptions {
  setTimer?: typeof globalThis.setTimeout;
  clearTimer?: typeof globalThis.clearTimeout;
}

/**
 * Put a delayed, live status around the real key lookup promise. Fast checks
 * remain quiet. A Tor check still pending at ten seconds announces itself and
 * always retracts that announcement when the promise settles or rejects.
 */
export async function withTorKeyserverPolling<T>(
  poll: () => Promise<T>,
  route: UiNetworkRoute,
  setStatus: (status: string | null) => void,
  options: TorKeyserverPollingOptions = {},
): Promise<T> {
  const setTimer = options.setTimer ?? globalThis.setTimeout;
  const clearTimer = options.clearTimer ?? globalThis.clearTimeout;
  let noticeShown = false;
  const timer = route === "tor"
    ? setTimer(() => {
      noticeShown = true;
      setStatus(TOR_KEYSERVER_WAIT_LABEL);
    }, TOR_KEYSERVER_WAIT_DELAY_MS)
    : null;

  try {
    return await poll();
  } finally {
    if (timer !== null) clearTimer(timer);
    if (noticeShown) setStatus(null);
  }
}
