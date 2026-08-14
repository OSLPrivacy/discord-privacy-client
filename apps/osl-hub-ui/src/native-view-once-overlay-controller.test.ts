import { describe, expect, it, vi } from "vitest";

const { revealNativeDiscordOverlayViewOnce } = vi.hoisted(() => ({
  revealNativeDiscordOverlayViewOnce: vi.fn(),
}));
vi.mock("./native-overlay-adapter", () => ({ revealNativeDiscordOverlayViewOnce }));

import { createNativeViewOnceOverlayController } from "./native-view-once-overlay-controller";

describe("native view-once overlay controller", () => {
  it("routes Play through one reveal claim and lets X close the claimed display", async () => {
    const messageId = "peer-abcdef0123456789abcdef0123456789";
    revealNativeDiscordOverlayViewOnce.mockResolvedValue({
      messageId,
      plaintext: "one server claim",
      viewOnceConsumed: true,
      displayDurationSeconds: 15,
    });
    const closed = vi.fn();
    const controller = createNativeViewOnceOverlayController({
      messageId,
      initialDurationSeconds: 15,
      durationChoices: [{ seconds: 5 }, { seconds: 15 }],
      now: () => 0,
      onClose: closed,
    });

    await Promise.all([controller.play(), controller.play()]);
    expect(revealNativeDiscordOverlayViewOnce).toHaveBeenCalledOnce();
    expect(revealNativeDiscordOverlayViewOnce).toHaveBeenCalledWith(messageId);
    expect(controller.snapshot().displaySession?.durationSeconds).toBe(15);

    controller.close();
    await controller.play();
    console.info(`TASK0565_NATIVE_REVEAL_COMMAND_CLAIMS=${revealNativeDiscordOverlayViewOnce.mock.calls.length}`);
    console.info(`TASK0565_NATIVE_DISPLAY_CLOSED=${controller.snapshot().phase === "closed"}`);
    expect(controller.snapshot().phase).toBe("closed");
    expect(revealNativeDiscordOverlayViewOnce).toHaveBeenCalledOnce();
    expect(closed).toHaveBeenCalledOnce();
  });
});
