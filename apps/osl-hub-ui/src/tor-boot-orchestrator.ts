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
  | { readonly event: "bootstrap"; readonly percent: number }
  | { readonly event: "ready" }
  | { readonly event: "error"; readonly message: string };

export interface TorBootStatus {
  readonly ready: boolean;
  readonly failed: boolean;
  readonly percent: number;
  readonly errorMessage: string | null;
}

export function initialTorBootStatus(): TorBootStatus {
  return { ready: false, failed: false, percent: 0, errorMessage: null };
}

/**
 * The route status shown on screen while the app is usable but Tor is not.
 * Pinned to this exact string at 0%: onboarding-tor.ts already owns the route
 * *choice* UI, this owns the *connecting* state the chosen route passes
 * through before Send may be pressed.
 */
export function torRouteStatusLabel(status: TorBootStatus): string {
  if (status.failed) return "Connection failed";
  if (status.ready) return "Connected";
  return `Connecting -- ${status.percent}%`;
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
    return { event: "bootstrap", percent: record.percent };
  }
  if (record.event === "ready") return { event: "ready" };
  if (record.event === "error" && typeof record.message === "string") return { event: "error", message: record.message };
  return null;
}

/** Once ready or failed the status is terminal: a stray late line (the
 * sidecar racing its own shutdown) cannot un-ready a connected route. */
export function applyTorSidecarEvent(status: TorBootStatus, event: TorSidecarEvent): TorBootStatus {
  if (status.ready || status.failed) return status;
  if (event.event === "bootstrap") {
    const clamped = Math.min(100, Math.max(0, Math.trunc(event.percent)));
    return { ...status, percent: Math.max(status.percent, clamped) };
  }
  if (event.event === "ready") return { ready: true, failed: false, percent: 100, errorMessage: null };
  return { ...status, failed: true, errorMessage: event.message };
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
  sidecar.onLine((line) => {
    const event = parseTorSidecarLine(line);
    if (event === null) return;
    const next = applyTorSidecarEvent(status, event);
    if (next === status) return;
    status = next;
    host.onStatus(status);
  });

  return {
    status: () => status,
    stop: () => sidecar.kill(),
  };
}
