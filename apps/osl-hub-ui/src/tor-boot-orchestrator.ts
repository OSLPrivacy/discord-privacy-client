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
  readonly slow: boolean;
  /** A failed sidecar is recoverable without restarting the hub. */
  readonly retryAvailable: boolean;
  readonly percent: number;
  readonly errorMessage: string | null;
}

/** A long bootstrap is still a live bootstrap. It changes the explanation on
 * screen, but it is deliberately not a deadline and cannot fail the route. */
export const TOR_SLOW_AFTER_MS = 45_000;

export function initialTorBootStatus(): TorBootStatus {
  return { ready: false, failed: false, slow: false, retryAvailable: false, percent: 0, errorMessage: null };
}

/**
 * The route status shown on screen while the app is usable but Tor is not.
 * Pinned to this exact string at 0%: onboarding-tor.ts already owns the route
 * *choice* UI, this owns the *connecting* state the chosen route passes
 * through before Send may be pressed.
 */
export function torRouteStatusLabel(status: TorBootStatus): string {
  if (status.failed) return "Failed -- Tor could not connect";
  if (status.ready) return "Connected — Tor covers OSL's own traffic, not Discord or your browser.";
  if (status.slow) return "Slow -- still trying";
  return `Connecting -- ${status.percent}%`;
}

/** The first-run status surface. Retry and Direct are intentionally absent
 * until the sidecar reports a real error: a merely slow bootstrap remains a
 * live attempt, including a cold start that needs 75 seconds. */
export function firstRunTorScreenMarkup(status: TorBootStatus): string {
  const label = torRouteStatusLabel(status);
  const successCopy = status.ready
    ? `<p class="tor-first-run-scope">Use Tor covers OSL's own traffic and nothing else. Discord, Telegram and the browser are separate processes with their own sockets.</p>`
    : "";
  const actions = status.failed
    ? `<div class="tor-first-run-actions"><button type="button" data-tor-retry>Retry</button><button type="button" data-tor-direct>Direct</button></div>`
    : status.ready
      ? `<div class="tor-first-run-actions"><button type="button" data-tor-connected-continue>Continue</button></div>`
      : "";
  return `<section class="tor-first-run" aria-labelledby="tor-first-run-heading">
    <h1 id="tor-first-run-heading">Connecting with Tor</h1>
    <p class="tor-first-run-status" role="status" aria-live="polite">${label}</p>
    ${successCopy}
    ${actions}
  </section>`;
}

/** The renderer uses this to expose the recovery action only after a failure. */
export function torRouteRetryLabel(status: TorBootStatus): string | null {
  return status.retryAvailable ? "Retry" : null;
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
  if (record.event === "error") {
    if (typeof record.message === "string") return { event: "error", message: record.message };
    // The packaged Rust sidecar names failures with scope + detail. Accepting
    // that real wire shape prevents an actual bootstrap failure becoming a
    // forever-connecting screen.
    if (typeof record.detail === "string") return { event: "error", message: record.detail };
  }
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
  if (event.event === "ready") return { ready: true, failed: false, slow: false, retryAvailable: false, percent: 100, errorMessage: null };
  return { ...status, failed: true, retryAvailable: true, errorMessage: event.message };
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
  /** Called when the child exits without the hub asking it to stop. */
  onExit?(handler: () => void): void;
  /** Sidecar-owned SOCKS port; retry must obtain a fresh one. */
  readonly port?: number;
  kill(): void;
}

export interface TorBootOrchestratorHost {
  /** Allowed to run before the sidecar is spawned. Must not be awaited on
   * anything Tor-related: it is what makes the window visible. */
  paint(): void;
  /** Not called until after `paint()` returns. */
  spawnSidecar(): TorSidecarProcess;
  onStatus(status: TorBootStatus): void;
  /**
   * The compose surface supplies these two hooks. Capturing happens at the
   * failure boundary and restoring happens before retry, so a route redraw
   * cannot turn a Tor failure into lost draft text.
   */
  captureDraft?(): string;
  restoreDraft?(draft: string): void;
}

export interface TorBootOrchestratorHandle {
  status(): TorBootStatus;
  /** Start a new contained sidecar after a Tor-only failure. */
  retry(): void;
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
  let sidecar: TorSidecarProcess | null = null;
  let stopped = false;
  let draftAtFailure: string | null = null;
  let slowTimer: ReturnType<typeof setTimeout> | null = null;

  const publish = (next: TorBootStatus): void => {
    if (next === status) return;
    status = next;
    if ((status.ready || status.failed) && slowTimer !== null) clearTimeout(slowTimer);
    host.onStatus(status);
  };

  const armSlowTimer = (): void => {
    if (slowTimer !== null) clearTimeout(slowTimer);
    slowTimer = setTimeout(() => publish(markTorBootSlow(status)), TOR_SLOW_AFTER_MS);
    // A status timer must not keep a Node fixture/test process alive after its
    // sidecar is gone. Browser timers are numeric and simply skip this branch.
    if (typeof slowTimer === "object" && "unref" in slowTimer) slowTimer.unref();
  };

  const preserveDraft = (): void => {
    if (host.captureDraft === undefined) return;
    draftAtFailure = host.captureDraft();
    host.restoreDraft?.(draftAtFailure);
  };

  const failSidecar = (process: TorSidecarProcess, message: string): void => {
    // An exit from a previous sidecar after Retry is stale information. It
    // cannot be allowed to fail the new route.
    if (stopped || sidecar !== process || status.failed) return;
    preserveDraft();
    publish({ ready: false, failed: true, slow: status.slow, retryAvailable: true, percent: status.percent, errorMessage: message });
  };

  const spawn = (): void => {
    const process = host.spawnSidecar();
    sidecar = process;
    process.onLine((line) => {
      const event = parseTorSidecarLine(line);
      if (event === null || stopped || sidecar !== process) return;
      if (event.event === "error") {
        failSidecar(process, event.message);
        return;
      }
      publish(applyTorSidecarEvent(status, event));
    });
    process.onExit?.(() => failSidecar(process, "Tor sidecar exited"));
    armSlowTimer();
  };

  host.paint();
  host.onStatus(status);
  spawn();

  return {
    status: () => status,
    retry: () => {
      if (!status.retryAvailable || stopped) return;
      // This is deliberately before spawn: the new sidecar cannot cause a
      // compose redraw to observe an empty editor, even for synchronous hosts.
      if (draftAtFailure !== null) host.restoreDraft?.(draftAtFailure);
      sidecar?.kill();
      status = initialTorBootStatus();
      host.onStatus(status);
      spawn();
    },
    stop: () => {
      stopped = true;
      if (slowTimer !== null) clearTimeout(slowTimer);
      sidecar?.kill();
    },
  };
}
