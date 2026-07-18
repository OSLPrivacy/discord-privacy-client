import { afterEach, describe, expect, it, vi } from "vitest";
import { NativeDeadlineError, withNativeDeadline } from "./native-deadline";

afterEach(() => vi.useRealTimers());

describe("native host invocation deadline", () => {
  it("returns a native result completed before the deadline", async () => {
    await expect(withNativeDeadline(Promise.resolve("ready"), "open service", 25)).resolves.toBe("ready");
  });

  it("rejects boundedly and ignores completion from the old intent", async () => {
    vi.useFakeTimers();
    let resolveNative: ((value: string) => void) | undefined;
    const native = new Promise<string>((resolve) => { resolveNative = resolve; });
    let visibleState = "opening";
    const request = withNativeDeadline(native, "open service", 15_000)
      .then(() => { visibleState = "stale-ready"; })
      .catch((failure: unknown) => {
        expect(failure).toBeInstanceOf(NativeDeadlineError);
        visibleState = "timed-out";
      });

    await vi.advanceTimersByTimeAsync(15_000);
    await request;
    expect(visibleState).toBe("timed-out");

    visibleState = "newer-navigation";
    resolveNative?.("late-ready");
    await Promise.resolve();
    expect(visibleState).toBe("newer-navigation");
  });

  it("preserves a native rejection that arrives before the deadline", async () => {
    const failure = new Error("native failed closed");
    await expect(withNativeDeadline(Promise.reject(failure), "close service", 25)).rejects.toBe(failure);
  });
});
