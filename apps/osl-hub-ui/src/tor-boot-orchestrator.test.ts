import { describe, expect, it, vi } from "vitest";
import {
  applyTorSidecarEvent,
  attemptNetworkSend,
  firstRunTorScreenMarkup,
  initialTorBootStatus,
  markTorBootSlow,
  parseTorSidecarLine,
  startTorBootOrchestrator,
  torRouteStatusLabel,
  type TorBootStatus,
  type TorSidecarProcess,
} from "./tor-boot-orchestrator";

function fakeSidecar(): TorSidecarProcess & { emit(line: string): void; killed: boolean } {
  const handlers: Array<(line: string) => void> = [];
  return {
    killed: false,
    onLine(handler) {
      handlers.push(handler);
    },
    kill() {
      this.killed = true;
    },
    emit(line: string) {
      for (const handler of handlers) handler(line);
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

  it("copies the last bootstrap reading even when it is lower", () => {
    const mid = applyTorSidecarEvent(initialTorBootStatus(), { event: "bootstrap", percent: 60 });
    const next = applyTorSidecarEvent(mid, { event: "bootstrap", percent: 10 });
    expect(next.percent).toBe(10);
    expect(torRouteStatusLabel(next)).toBe("Connecting -- 10%");
  });

  it("flips ready and reports 'Connected'", () => {
    const ready = applyTorSidecarEvent(initialTorBootStatus(), { event: "ready" });
    expect(ready.ready).toBe(true);
    expect(torRouteStatusLabel(ready)).toBe("Connected");
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
    const failed: TorBootStatus = { ready: false, failed: true, slow: false, percent: 0, errorMessage: "x" };
    const result = attemptNetworkSend(failed, () => {
      writes += 1;
    });
    expect(result).toEqual({ sent: false, reason: "route-failed" });
    expect(writes).toBe(0);
  });

  it("performs exactly one network write once the route is ready", () => {
    let writes = 0;
    const ready: TorBootStatus = { ready: true, failed: false, slow: false, percent: 100, errorMessage: null };
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
    expect(torRouteStatusLabel(handle.status())).toBe("Connected");
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
