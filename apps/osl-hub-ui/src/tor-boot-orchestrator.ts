/**
 * TASK 4909 - the window must be allowed to paint before Tor is ready.
 *
 * Tor bootstrap (crates/osl-tor-sidecar per TASK 4905) can run well past the
 * boot-shell's first frame -- a cold circuit build has taken over 45 seconds
 * in fixture capture (TASK 4903). This module is the seam between "the window
 * exists" and "the network route is up": `startTorBootOrchestrator` calls
 * `host.paint()` before it ever spawns the sidecar, so painting can never be
 * made to wait on a socket, a relay handshake, or a directory fetch. Nothing
 * that reaches the network runs until the sidecar has reported "ready", which
 * `attemptNetworkSend` enforces at the one call site every Send action must
 * pass through.
 */

/** One parsed line of the sidecar's newline-delimited JSON status stream. */
export type TorSidecarEvent =
  | { readonly event: "bootstrap"; readonly percent: number; readonly bridgeInUse?: boolean }
  | { readonly event: "ready"; readonly bridgeInUse?: boolean }
  | { readonly event: "error"; readonly message: string };

export interface TorBootStatus {
  readonly ready: boolean;
  readonly failed: boolean;
  readonly slow: boolean;
  readonly percent: number;
  readonly errorMessage: string | null;
  readonly bridgeInUse?: boolean;
}

/** A long bootstrap is still a live bootstrap. It changes the explanation on
 * screen, but it is deliberately not a deadline and cannot fail the route. */
export const TOR_SLOW_AFTER_MS = 45_000;

export function initialTorBootStatus(): TorBootStatus {
  return { ready: false, failed: false, slow: false, percent: 0, errorMessage: null, bridgeInUse: false };
}

/**
 * The route status shown on screen while the app is usable but Tor is not.
 * Pinned to this exact string at 0%: onboarding-tor.ts already owns the route
 * *choice* UI, this owns the *connecting* state the chosen route passes
 * through before Send may be pressed.
 */
export function torRouteStatusLabel(status: TorBootStatus): string {
  if (status.failed) return "Failed -- Tor could not connect";
  if (status.ready) return "Connected";
  if (status.slow) return "Slow -- still trying";
  return `Connecting -- ${status.percent}%`;
}

/** The first-run status surface. Retry and Direct are intentionally absent
 * until the sidecar reports a real error: a merely slow bootstrap remains a
 * live attempt, including a cold start that needs 75 seconds. */
export function firstRunTorScreenMarkup(status: TorBootStatus): string {
  const label = torRouteStatusLabel(status);
  const actions = status.failed
    ? `<div class="tor-first-run-actions"><button type="button" data-tor-retry>Retry</button><button type="button" data-tor-direct>Direct</button></div>`
    : "";
  return `<section class="tor-first-run" aria-labelledby="tor-first-run-heading">
    <h1 id="tor-first-run-heading">Connecting with Tor</h1>
    <p class="tor-first-run-status" role="status" aria-live="polite">${label}</p>
    ${actions}
  </section>`;
}

/** Malformed or unrecognised lines are dropped rather than thrown, so one bad
 * line from the sidecar cannot crash the host that is reading its stdout. */
export function parseTorSidecarLine(line: string): TorSidecarEvent | null {
  const trimmed = line.trim();
  if (trimmed.length === 0) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const record = parsed as Record<string, unknown>;
  if (record.event === "bootstrap" && typeof record.percent === "number" && Number.isFinite(record.percent)) {
    return record.bridge_in_use === true
      ? { event: "bootstrap", percent: record.percent, bridgeInUse: true }
      : { event: "bootstrap", percent: record.percent };
  }
  if (record.event === "ready") return record.bridge_in_use === true ? { event: "ready", bridgeInUse: true } : { event: "ready" };
  if (record.event === "error") {
    const message = typeof record.message === "string" ? record.message : record.detail;
    if (typeof message === "string") return { event: "error", message };
  }
  return null;
}

/** Once ready or failed the status is terminal: a stray late line (the
 * sidecar racing its own shutdown) cannot un-ready a connected route. */
export function applyTorSidecarEvent(status: TorBootStatus, event: TorSidecarEvent): TorBootStatus {
  if (status.ready || status.failed) return status;
  if (event.event === "bootstrap") {
    const clamped = Math.min(100, Math.max(0, Math.trunc(event.percent)));
    return { ...status, percent: clamped, bridgeInUse: event.bridgeInUse ?? status.bridgeInUse };
  }
  if (event.event === "ready") return { ready: true, failed: false, slow: false, percent: 100, errorMessage: null, bridgeInUse: event.bridgeInUse ?? status.bridgeInUse };
  return { ...status, failed: true, errorMessage: event.message };
}

/** Mark a still-running attempt as slow without fabricating either progress or
 * failure. Terminal states ignore the timer if it races a sidecar line. */
export function markTorBootSlow(status: TorBootStatus): TorBootStatus {
  if (status.ready || status.failed || status.slow) return status;
  return { ...status, slow: true };
}

export interface NetworkSendAttempt {
  readonly sent: boolean;
  readonly reason: "not-ready" | "route-failed" | null;
}

/**
 * The one gate every Send action must pass through. `performSend` is not
 * invoked at all -- not invoked-and-discarded -- when the route is not ready,
 * so a caller that counts its own network writes sees zero of them for every
 * attempt made before `status.ready`.
 */
export function attemptNetworkSend(status: TorBootStatus, performSend: () => void): NetworkSendAttempt {
  if (status.failed) return { sent: false, reason: "route-failed" };
  if (!status.ready) return { sent: false, reason: "not-ready" };
  performSend();
  return { sent: true, reason: null };
}

export interface TorSidecarProcess {
  onLine(handler: (line: string) => void): void;
  kill(): void;
}

export interface TorBootOrchestratorHost {
  /** Allowed to run before the sidecar is spawned. Must not be awaited on
   * anything Tor-related: it is what makes the window visible. */
  paint(): void;
  /** Not called until after `paint()` returns. */
  spawnSidecar(): TorSidecarProcess;
  onStatus(status: TorBootStatus): void;
}

export interface TorBootOrchestratorHandle {
  status(): TorBootStatus;
  stop(): void;
}

/**
 * Paint first, spawn second: the ordering below is the entire fix. Moving the
 * `spawnSidecar()` call ahead of `host.paint()` is exactly the bug this task
 * exists to prevent, so it is written once, here, rather than at each call
 * site that wires a real window and a real sidecar together.
 */
export function startTorBootOrchestrator(host: TorBootOrchestratorHost): TorBootOrchestratorHandle {
  let status = initialTorBootStatus();
  host.paint();
  host.onStatus(status);

  const sidecar = host.spawnSidecar();
  const slowTimer = setTimeout(() => {
    const next = markTorBootSlow(status);
    if (next === status) return;
    status = next;
    host.onStatus(status);
  }, TOR_SLOW_AFTER_MS);
  // A status timer must not keep a Node fixture/test process alive after its
  // sidecar is gone. Browser timers are numeric and simply skip this branch.
  if (typeof slowTimer === "object" && "unref" in slowTimer) slowTimer.unref();
  sidecar.onLine((line) => {
    const event = parseTorSidecarLine(line);
    if (event === null) return;
    const next = applyTorSidecarEvent(status, event);
    if (next === status) return;
    status = next;
    if (status.ready || status.failed) clearTimeout(slowTimer);
    host.onStatus(status);
  });

  return {
    status: () => status,
    stop: () => {
      clearTimeout(slowTimer);
      sidecar.kill();
    },
  };
}
