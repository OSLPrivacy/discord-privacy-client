import { describe, expect, it, vi } from "vitest";
import { FrameRenderScheduler } from "./render-scheduler";

describe("FrameRenderScheduler", () => {
  it("collapses repeated requests into one frame and one commit", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    let nextHandle = 1;
    const commit = vi.fn();
    const scheduler = new FrameRenderScheduler(
      (callback) => {
        const handle = nextHandle++;
        callbacks.set(handle, callback);
        return handle;
      },
      (handle) => callbacks.delete(handle),
      commit,
    );

    scheduler.request();
    scheduler.request();
    scheduler.request();

    expect(callbacks.size).toBe(1);
    expect(commit).not.toHaveBeenCalled();
    callbacks.values().next().value?.(0);
    expect(commit).toHaveBeenCalledTimes(1);
  });

  it("keeps a 500-interaction burst to one render commit", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    const commit = vi.fn();
    const scheduler = new FrameRenderScheduler(
      (callback) => {
        callbacks.set(1, callback);
        return 1;
      },
      (handle) => callbacks.delete(handle),
      commit,
    );

    for (let interaction = 0; interaction < 500; interaction += 1) scheduler.request();

    expect(callbacks.size).toBe(1);
    callbacks.get(1)?.(0);
    expect(commit).toHaveBeenCalledTimes(1);
  });

  it("allows one new commit after the pending frame completes", () => {
    const callbacks: FrameRequestCallback[] = [];
    const commit = vi.fn();
    const scheduler = new FrameRenderScheduler(
      (callback) => {
        callbacks.push(callback);
        return callbacks.length;
      },
      vi.fn(),
      commit,
    );

    scheduler.request();
    callbacks[0](0);
    scheduler.request();
    callbacks[1](16);

    expect(commit).toHaveBeenCalledTimes(2);
  });

  it("flushes synchronously without leaving the queued frame alive", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    const cancel = vi.fn((handle: number) => callbacks.delete(handle));
    const commit = vi.fn();
    const scheduler = new FrameRenderScheduler(
      (callback) => {
        callbacks.set(7, callback);
        return 7;
      },
      cancel,
      commit,
    );

    scheduler.request();
    scheduler.flush();

    expect(cancel).toHaveBeenCalledWith(7);
    expect(callbacks.size).toBe(0);
    expect(commit).toHaveBeenCalledTimes(1);
  });
});
