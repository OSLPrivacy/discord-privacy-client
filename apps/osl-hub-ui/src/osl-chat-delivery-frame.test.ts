import { describe, expect, it, vi } from "vitest";
import { attachOslChatFrameDelivery } from "./osl-chat-delivery";

describe("OSL Chat realtime delivery", () => {
  it("is driven only by a persistent-frame wakeup, never a timer", async () => {
    let wakeup: (() => void) | undefined;
    const source = { onWakeup: vi.fn(async (callback: () => void) => {
      wakeup = callback;
      return () => {};
    }) };
    const delivery = { sync: vi.fn(async () => {}) };

    await attachOslChatFrameDelivery(source, delivery);
    expect(delivery.sync).not.toHaveBeenCalled();
    wakeup?.();
    await Promise.resolve();
    expect(delivery.sync).toHaveBeenCalledTimes(1);
    expect(source.onWakeup).toHaveBeenCalledTimes(1);
  });
});
