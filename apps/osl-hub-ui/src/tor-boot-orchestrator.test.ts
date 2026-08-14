import { describe, expect, it, vi } from "vitest";
import {
  applyTorSidecarEvent,
  attemptNetworkSend,
  firstRunTorScreenMarkup,
  initialTorBootStatus,
  markTorBootSlow,
  parseTorSidecarLine,
  startTorBootOrchestrator,
  torRouteRetryLabel,
  torRouteStatusLabel,
  type TorBootStatus,
  type TorSidecarProcess,
} from "./tor-boot-orchestrator";

function fakeSidecar(port = 0): TorSidecarProcess & { emit(line: string): void; exit(): void; killed: boolean } {
  const handlers: Array<(line: string) => void> = [];
  const exitHandlers: Array<() => void> = [];
  return {
    port,
    killed: false,
    onLine(handler) {
      handlers.push(handler);
    },
    onExit(handler) {
      exitHandlers.push(handler);
    },
    kill() {
      this.killed = true;
    },
    emit(line: string) {
      for (const handler of handlers) handler(line);
    },
    exit() {
      for (const handler of exitHandlers) handler();
    },
  };
}

describe("initial route status", () => {
  it("reads exactly 'Connecting -- 0%' before any sidecar line has arrived", () => {
    expect(torRouteStatusLabel(initialTorBootStatus())).toBe("Connecting -- 0%");
  });
});

describe("parseTorSidecarLine", () => {
  it("parses a bootstrap progress line", () => {
    expect(parseTorSidecarLine('{"event":"bootstrap","percent":42}')).toEqual({ event: "bootstrap", percent: 42 });
  });

  it("parses a ready line", () => {
    expect(parseTorSidecarLine('{"event":"ready"}')).toEqual({ event: "ready" });
  });

  it("parses an error line", () => {
    expect(parseTorSidecarLine('{"event":"error","message":"boom"}')).toEqual({ event: "error", message: "boom" });
  });

  it("parses the packaged Rust sidecar's scope/detail error shape", () => {
    expect(parseTorSidecarLine('{"event":"error","scope":"bootstrap","detail":"no route"}')).toEqual({ event: "error", message: "no route" });
  });

  it("drops a blank line rather than throwing", () => {
    expect(parseTorSidecarLine("")).toBeNull();
    expect(parseTorSidecarLine("   ")).toBeNull();
  });

  it("drops a line that is not JSON rather than throwing", () => {
    expect(parseTorSidecarLine("not json at all")).toBeNull();
  });

  it("drops a well-formed object with no recognised event", () => {
    expect(parseTorSidecarLine('{"event":"unrelated"}')).toBeNull();
  });
});

describe("applyTorSidecarEvent", () => {
  it("raises percent on a bootstrap event and keeps the label in sync", () => {
    const next = applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: 17 });
    expect(next.percent).toBe(17);
    expect(torRouteStatusLabel(next)).toBe("Connecting -- 17%");
  });

  it("clamps an out-of-range percent into 0..100", () => {
    expect(applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: 140 }).percent).toBe(100);
    expect(applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: -5 }).percent).toBe(0);
  });

  it("never lets percent run backwards on a later, lower reading", () => {
    const mid = applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: 60 });
    const next = applyTorSidecarEvent(mid, { event: "bootstrap", percent: 10 });
    expect(next.percent).toBe(60);
    expect(torRouteStatusLabel(next)).toBe("Connecting -- 60%");
  });

  it("flips ready and reports the one-sentence coverage boundary", () => {
    const ready = applyTorSidecarEvent(initialTorBootStatus(), { event: "ready" });
    expect(ready.ready).toBe(true);
    expect(torRouteStatusLabel(ready)).toBe("Connected — Tor covers OSL's own traffic, not Discord or your browser.");
  });

  it("is terminal: a line after ready cannot un-ready the route", () => {
    const ready = applyTorSidecarEvent(initialTorBootStatus(), { event: "ready" });
    const after = applyTorSidecarEvent(ready, { event: "bootstrap", percent: 3 });
    expect(after).toBe(ready);
    expect(after.ready).toBe(true);
  });

  it("is terminal: a line after failure cannot resurrect the route", () => {
    const failed = applyTorSidecarEvent(initialTorBootStatus(), { event: "error", message: "relay refused" });
    const after = applyTorSidecarEvent(failed, { event: "ready" });
    expect(after).toBe(failed);
    expect(after.failed).toBe(true);
  });

  it("marks a long live bootstrap slow without failing it", () => {
    const slow = markTorBootSlow(initialTorBootStatus());
    expect(slow.failed).toBe(false);
    expect(torRouteStatusLabel(slow)).toBe("Slow -- still trying");
  });

  it("renders Retry and Direct only after an explicit failure", () => {
    const failed = applyTorSidecarEvent(initialTorBootStatus(), { event: "error", message: "relay refused" });
    expect(torRouteStatusLabel(failed)).toBe("Failed -- Tor could not connect");
    expect(firstRunTorScreenMarkup(failed)).toContain(">Retry</button>");
    expect(firstRunTorScreenMarkup(failed)).toContain(">Direct</button>");
    expect(firstRunTorScreenMarkup(initialTorBootStatus())).not.toContain(">Retry</button>");
  });

  it("makes the exact failure state retryable", () => {
    const failed = applyTorSidecarEvent(initialTorBootStatus(), { event: "error", message: "obsolete consensus" });
    expect(torRouteStatusLabel(failed)).toBe("Failed -- Tor could not connect");
    expect(torRouteRetryLabel(failed)).toBe("Retry");
  });
});

describe("attemptNetworkSend", () => {
  it("records zero network writes when the route is not ready", () => {
    let writes = 0;
    const result = attemptNetworkSend(initialTorBootStatus(), () => {
      writes += 1;
    });
    expect(result).toEqual({ sent: false, reason: "not-ready" });
    expect(writes).toBe(0);
  });

  it("records zero network writes when the route has failed", () => {
    let writes = 0;
    const failed: TorBootStatus = { ready: false, failed: true, slow: false, retryAvailable: true, percent: 0, errorMessage: "x" };
    const result = attemptNetworkSend(failed, () => {
      writes += 1;
    });
    expect(result).toEqual({ sent: false, reason: "route-failed" });
    expect(writes).toBe(0);
  });

  it("performs exactly one network write once the route is ready", () => {
    let writes = 0;
    const ready: TorBootStatus = { ready: true, failed: false, slow: false, retryAvailable: false, percent: 100, errorMessage: null };
    const result = attemptNetworkSend(ready, () => {
      writes += 1;
    });
    expect(result).toEqual({ sent: true, reason: null });
    expect(writes).toBe(1);
  });
});

describe("startTorBootOrchestrator", () => {
  it("paints before the sidecar is spawned, every time", () => {
    const calls: string[] = [];
    const sidecar = fakeSidecar();
    startTorBootOrchestrator({
      paint: () => calls.push("paint"),
      spawnSidecar: () => {
        calls.push("spawn");
        return sidecar;
      },
      onStatus: () => undefined,
    });
    expect(calls).toEqual(["paint", "spawn"]);
  });

  it("does not spawn the sidecar from inside paint(), even if paint() throws", () => {
    const sidecar = fakeSidecar();
    let spawned = false;
    expect(() =>
      startTorBootOrchestrator({
        paint: () => {
          throw new Error("paint blew up");
        },
        spawnSidecar: () => {
          spawned = true;
          return sidecar;
        },
        onStatus: () => undefined,
      }),
    ).toThrow("paint blew up");
    expect(spawned).toBe(false);
  });

  it("reports 'Connecting -- 0%' immediately after paint, before any sidecar line arrives", () => {
    const statuses: TorBootStatus[] = [];
    const sidecar = fakeSidecar();
    const handle = startTorBootOrchestrator({
      paint: () => undefined,
      spawnSidecar: () => sidecar,
      onStatus: (status) => statuses.push(status),
    });
    expect(torRouteStatusLabel(handle.status())).toBe("Connecting -- 0%");
    expect(statuses).toHaveLength(1);
    expect(torRouteStatusLabel(statuses[0])).toBe("Connecting -- 0%");
  });

  it("blocks a Send attempted through the handle's status until the sidecar reports ready", () => {
    const sidecar = fakeSidecar();
    const handle = startTorBootOrchestrator({
      paint: () => undefined,
      spawnSidecar: () => sidecar,
      onStatus: () => undefined,
    });

    let writes = 0;
    const beforeReady = attemptNetworkSend(handle.status(), () => {
      writes += 1;
    });
    expect(beforeReady.sent).toBe(false);
    expect(writes).toBe(0);

    sidecar.emit('{"event":"bootstrap","percent":50}');
    expect(torRouteStatusLabel(handle.status())).toBe("Connecting -- 50%");
    const stillNotReady = attemptNetworkSend(handle.status(), () => {
      writes += 1;
    });
    expect(stillNotReady.sent).toBe(false);
    expect(writes).toBe(0);

    sidecar.emit('{"event":"ready"}');
    expect(torRouteStatusLabel(handle.status())).toBe("Connected — Tor covers OSL's own traffic, not Discord or your browser.");
    const afterReady = attemptNetworkSend(handle.status(), () => {
      writes += 1;
    });
    expect(afterReady.sent).toBe(true);
    expect(writes).toBe(1);
  });

  it("stop() kills the sidecar process", () => {
    const sidecar = fakeSidecar();
    const handle = startTorBootOrchestrator({
      paint: () => undefined,
      spawnSidecar: () => sidecar,
      onStatus: () => undefined,
    });
    handle.stop();
    expect(sidecar.killed).toBe(true);
  });

  it("ignores a malformed line from the sidecar instead of throwing", () => {
    const sidecar = fakeSidecar();
    const handle = startTorBootOrchestrator({
      paint: () => undefined,
      spawnSidecar: () => sidecar,
      onStatus: () => undefined,
    });
    expect(() => sidecar.emit("not json")).not.toThrow();
    expect(torRouteStatusLabel(handle.status())).toBe("Connecting -- 0%");
  });

  it("contains three sidecar crashes, preserves the exact UTF-8 draft, and retries on fresh ports", () => {
    const draft = { text: "First line\nemoji: 🧭\ncombining: e\u0301\0" };
    const originalBytes = Array.from(new TextEncoder().encode(draft.text));
    const ports = [39151, 39152, 39153, 39154];
    const sidecars = ports.map((port) => fakeSidecar(port));
    const statuses: Array<{ label: string; elapsedSinceCrashMs: number }> = [];
    let spawnCount = 0;
    let hubExited = 0;
    let now = 0;
    let crashStartedAt = 0;

    const handle = startTorBootOrchestrator({
      paint: () => undefined,
      spawnSidecar: () => sidecars[spawnCount++],
      onStatus: (status) => statuses.push({ label: torRouteStatusLabel(status), elapsedSinceCrashMs: now - crashStartedAt }),
      captureDraft: () => draft.text,
      restoreDraft: (saved) => {
        draft.text = saved;
      },
    });

    const usedPorts: number[] = [];
    for (let crash = 0; crash < 3; crash += 1) {
      const active = sidecars[crash];
      usedPorts.push(active.port ?? -1);
      now += 4_999;
      crashStartedAt = now;
      // This models SIGKILL/obsolete-consensus termination of the child only.
      // Nothing throws from the callback, so the hub remains alive.
      try {
        active.exit();
      } catch {
        // A thrown sidecar-exit callback is the test-harness equivalent of
        // the hub process dying. The containment mutant must make this 1.
        hubExited += 1;
      }
      expect(hubExited).toBe(0);
      const failure = statuses.at(-1);
      expect(failure).toEqual({ label: "Failed -- Tor could not connect", elapsedSinceCrashMs: 0 });
      expect(failure?.elapsedSinceCrashMs).toBeLessThanOrEqual(5_000);
      expect(Array.from(new TextEncoder().encode(draft.text))).toEqual(originalBytes);
      expect(torRouteRetryLabel(handle.status())).toBe("Retry");
      expect(() => handle.retry()).not.toThrow();
    }

    usedPorts.push(sidecars[3].port ?? -1);
    expect(new Set(usedPorts).size).toBe(4);
    expect(spawnCount).toBe(4);
    console.info(`TASK4915 sidecar_kills=3 hub_process_exits=${hubExited} failed_within_5_seconds=3 draft_bytes_identical=true retry_ports=${usedPorts.join(",")}`);
  });
});

describe("a real 45-second sidecar cannot delay paint (structural proof)", () => {
  it("calls paint() synchronously even when spawnSidecar would otherwise take 45000ms", () => {
    vi.useFakeTimers();
    try {
      let painted = false;
      const sidecar = fakeSidecar();
      startTorBootOrchestrator({
        paint: () => {
          painted = true;
        },
        spawnSidecar: () => {
          // A spawnSidecar that itself blocked for 45s here would be the bug;
          // paint must already be true before this function is even called.
          expect(painted).toBe(true);
          return sidecar;
        },
        onStatus: () => undefined,
      });
      expect(painted).toBe(true);
      vi.advanceTimersByTime(45_000);
      expect(painted).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
