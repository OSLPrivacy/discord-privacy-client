import { describe, expect, it } from "vitest";
import {
  CoalescedRealignment,
  NATIVE_REALIGNMENT_HEARTBEAT_MS,
  NATIVE_REALIGNMENT_PACING_MS,
  NativeCallGate,
} from "./native-realignment";

interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
}

function deferred<T = void>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Run every already-queued microtask. */
function settle(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

describe("CoalescedRealignment", () => {
  it("drops intermediate requests instead of queueing one pass per event", async () => {
    // The defect this exists for: a caption drag raises hundreds of move events
    // a second and each one used to be able to reach the backend.
    const gate = deferred();
    const positions: number[] = [];
    let position = 0;
    const coalescer = new CoalescedRealignment(async () => {
      positions.push(position);
      await gate.promise;
    }, { pacingMs: 0 });

    void coalescer.request();
    await settle();
    expect(coalescer.passes).toBe(1);

    // 200 further move events arrive while that first pass is still in flight.
    for (let event = 0; event < 200; event += 1) {
      position += 1;
      void coalescer.request();
    }
    await settle();

    // Not one of them started a pass, and none of them is waiting in a queue.
    expect(coalescer.passes).toBe(1);
    expect(coalescer.dropped).toBe(200);
    expect(coalescer.trailingPassOwed).toBe(true);

    gate.resolve();
    await settle();

    // Exactly one trailing pass, and it read the LAST position, not the first
    // of the 200 that were dropped.
    expect(coalescer.passes).toBe(2);
    expect(positions).toEqual([0, 200]);
  });

  it("always runs a trailing pass so the resting position is never lost", async () => {
    const seen: number[] = [];
    const gates: Array<Deferred<void>> = [];
    let position = 0;
    const coalescer = new CoalescedRealignment(async () => {
      seen.push(position);
      const gate = deferred();
      gates.push(gate);
      await gate.promise;
    }, { pacingMs: 0 });

    void coalescer.request();
    await settle();
    expect(seen).toEqual([0]);

    // The very last event of the drag lands while the pass is running -- the
    // worst case, because nothing will ever ask again.
    position = 42;
    void coalescer.request();
    await settle();
    expect(seen).toEqual([0]);

    gates[0].resolve();
    await settle();
    expect(seen).toEqual([0, 42]);

    gates[1].resolve();
    await settle();
    // And the chain then stops: no self-perpetuating loop.
    expect(coalescer.passes).toBe(2);
    expect(coalescer.active).toBe(false);
  });

  it("never runs two passes at once however many requests arrive", async () => {
    let inside = 0;
    let peak = 0;
    const gate = deferred();
    const coalescer = new CoalescedRealignment(async () => {
      inside += 1;
      peak = Math.max(peak, inside);
      await gate.promise;
      inside -= 1;
    }, { pacingMs: 0 });

    for (let event = 0; event < 500; event += 1) void coalescer.request();
    await settle();
    gate.resolve();
    await settle();

    expect(peak).toBe(1);
    expect(coalescer.peakConcurrentPasses).toBe(1);
    // 500 events, at most two passes: the running one and its trailing one.
    expect(coalescer.passes).toBeLessThanOrEqual(2);
  });

  it("paces consecutive passes so a sustained burst cannot saturate the loop", async () => {
    // Coalescing alone still lets the trailing pass re-enter with a zero gap
    // for as long as events keep arriving, which during a drag is the entire
    // drag. The pacing gap is what makes that a heartbeat instead.
    const waits: number[] = [];
    let events = 0;
    const coalescer = new CoalescedRealignment(async () => {
      // The drag is still going: every pass is followed by more move events.
      if (events < 5) {
        events += 1;
        void coalescer.request();
      }
      await Promise.resolve();
    }, {
      pacingMs: 120,
      wait: async (ms) => {
        waits.push(ms);
        await Promise.resolve();
      },
    });

    await coalescer.request();

    expect(coalescer.passes).toBe(6);
    // A gap before every re-entry, and none before the pass that ends the chain.
    expect(waits).toEqual([120, 120, 120, 120, 120]);
  });

  it("does not pace the pass that lands the resting position", async () => {
    const waits: number[] = [];
    const coalescer = new CoalescedRealignment(async () => {
      await Promise.resolve();
    }, {
      pacingMs: 120,
      wait: async (ms) => {
        waits.push(ms);
        await Promise.resolve();
      },
    });

    await coalescer.request();

    expect(coalescer.passes).toBe(1);
    expect(waits).toEqual([]);
  });

  it("still runs the trailing pass when a pass throws", async () => {
    // A companion that failed mid-drag must not strand the borrowed window at
    // an intermediate position.
    let attempts = 0;
    const coalescer = new CoalescedRealignment(async () => {
      attempts += 1;
      if (attempts === 1) {
        void coalescer.request();
        throw new Error("native host unavailable");
      }
      await Promise.resolve();
    }, { pacingMs: 0 });

    await expect(coalescer.request()).resolves.toBeUndefined();
    expect(attempts).toBe(2);
  });

  it("ships a pacing default that is invisible at rest and bounded under load", () => {
    expect(NATIVE_REALIGNMENT_PACING_MS).toBeGreaterThan(0);
    expect(NATIVE_REALIGNMENT_PACING_MS).toBeLessThanOrEqual(250);
  });

  it("realigns through a low-rate heartbeat when an animation frame is frozen", async () => {
    // This models a minimised/occluded WebView: scheduleNativeHostRealignment
    // armed the backstop, but its rAF callback is never delivered.
    const timers = new Map<number, () => void>();
    let nextTimer = 0;
    let passes = 0;
    const coalescer = new CoalescedRealignment(async () => {
      passes += 1;
    }, {
      heartbeatMs: 1_000,
      setHeartbeatTimer: (callback) => {
        nextTimer += 1;
        timers.set(nextTimer, callback);
        return nextTimer;
      },
      clearHeartbeatTimer: (timer) => {
        timers.delete(timer as number);
      },
    });

    coalescer.armHeartbeat();
    expect(timers.size).toBe(1);
    const firstBeat = timers.entries().next().value!;
    // A real timeout is removed by the platform before its callback runs.
    // Model that lifecycle explicitly in this injected scheduler.
    timers.delete(firstBeat[0]);
    firstBeat[1]();
    await settle();

    expect(passes).toBe(1);
    // It stays low-rate while rAF remains frozen, rather than becoming a busy
    // loop or a second, unbounded native-call path.
    expect(timers.size).toBe(1);
    coalescer.acknowledgeAnimationFrame();
    expect(timers.size).toBe(0);
  });

  it("keeps heartbeat requests inside the existing coalescing bound", async () => {
    const timers = new Map<number, () => void>();
    let nextTimer = 0;
    const gate = deferred();
    const coalescer = new CoalescedRealignment(async () => {
      await gate.promise;
    }, {
      pacingMs: 0,
      setHeartbeatTimer: (callback) => {
        nextTimer += 1;
        timers.set(nextTimer, callback);
        return nextTimer;
      },
      clearHeartbeatTimer: (timer) => {
        timers.delete(timer as number);
      },
    });

    coalescer.armHeartbeat();
    timers.values().next().value!();
    await settle();
    timers.values().next().value!();
    await settle();
    timers.values().next().value!();
    await settle();

    expect(coalescer.passes).toBe(1);
    expect(coalescer.trailingPassOwed).toBe(true);
    expect(coalescer.peakConcurrentPasses).toBe(1);
    coalescer.acknowledgeAnimationFrame();
    gate.resolve();
    await settle();
    expect(coalescer.passes).toBe(2);
  });

  it("uses a heartbeat interval that remains a backstop rather than polling", () => {
    expect(NATIVE_REALIGNMENT_HEARTBEAT_MS).toBeGreaterThanOrEqual(500);
  });
});

describe("NativeCallGate", () => {
  it("keeps at most one outstanding invoke per command", async () => {
    let invokes = 0;
    const gate = new NativeCallGate();
    const pending = deferred<string>();

    const calls = Array.from({ length: 50 }, () => gate.run("resize", () => {
      invokes += 1;
      return pending.promise;
    }));

    expect(invokes).toBe(1);
    expect(gate.size).toBe(1);
    pending.resolve("resized");
    expect(await Promise.all(calls)).toEqual(Array.from({ length: 50 }, () => "resized"));
    await settle();
    expect(gate.size).toBe(0);
  });

  it("cannot accumulate outstanding calls when a caller gives up waiting", async () => {
    // withNativeDeadline abandons a slow call; it cannot cancel it. Without the
    // gate, every abandoned pass would start another invoke of the same command
    // while the first was still running in the backend.
    let invokes = 0;
    const gate = new NativeCallGate();
    const stalled = deferred<string>();
    const start = (): Promise<string> => {
      invokes += 1;
      return stalled.promise;
    };

    for (let abandonedPass = 0; abandonedPass < 100; abandonedPass += 1) {
      void gate.run("resize", start).catch(() => undefined);
      await settle();
    }

    expect(invokes).toBe(1);
    expect(gate.size).toBe(1);
    stalled.resolve("resized");
    await settle();

    // Once the backend finally answers, the next request is a fresh call.
    void gate.run("resize", start).catch(() => undefined);
    expect(invokes).toBe(2);
  });

  it("releases the slot when a call rejects and keys commands independently", async () => {
    let resizes = 0;
    let focuses = 0;
    const gate = new NativeCallGate();

    await expect(gate.run("resize", () => {
      resizes += 1;
      return Promise.reject(new Error("native host unavailable"));
    })).rejects.toThrow("native host unavailable");
    await settle();
    expect(gate.size).toBe(0);

    const resize = deferred<string>();
    const focus = deferred<string>();
    void gate.run("resize", () => {
      resizes += 1;
      return resize.promise;
    });
    void gate.run("focus", () => {
      focuses += 1;
      return focus.promise;
    });
    expect(gate.size).toBe(2);
    expect(resizes).toBe(2);
    expect(focuses).toBe(1);
    resize.resolve("resized");
    focus.resolve("focused");
    await settle();
    expect(gate.size).toBe(0);
  });
});
