import { describe, expect, it, vi } from "vitest";
import { createViewOnceOverlayController } from "./view-once-overlay-controller";

describe("view-once overlay controller", () => {
  it("direct UI commands claim once, apply the chosen duration to the display session, and let X close without another claim", async () => {
    let now = 1_000;
    let claimCount = 0;
    const displayClosed = vi.fn();
    const controller = createViewOnceOverlayController({
      messageId: "peer-0123456789abcdef0123456789abcdef",
      initialDurationSeconds: 30,
      durationChoices: [{ seconds: 5 }, { seconds: 30 }],
      now: () => now,
      claim: async (messageId) => {
        claimCount += 1;
        return {
          messageId,
          content: { kind: "text", text: "claimed once" },
          displayDurationSeconds: 30,
        };
      },
      onClose: displayClosed,
    });

    expect(controller.chooseDuration(5)).toBe(true);
    await Promise.all([controller.play(), controller.play()]);
    const open = controller.snapshot();
    expect(open.phase).toBe("displaying");
    expect(open.displaySession?.durationSeconds).toBe(5);
    expect(claimCount).toBe(1);

    controller.close(); // The screen's X command.
    now += 10_000;
    expect(controller.snapshot().phase).toBe("closed");
    await controller.play();

    console.info(`TASK0565_UI_SELECTED_DURATION_SECONDS=${open.selectedDurationSeconds}`);
    console.info(`TASK0565_UI_DISPLAY_SESSION_SECONDS=${open.displaySession?.durationSeconds}`);
    console.info(`TASK0565_UI_SERVER_CLAIMS=${claimCount}`);
    console.info(`TASK0565_UI_CLOSED=${controller.snapshot().phase === "closed"}`);
    expect(claimCount).toBe(1);
    expect(displayClosed).toHaveBeenCalledOnce();
  });

  it("never allows a local duration choice to outlive the server-claimed duration", async () => {
    const controller = createViewOnceOverlayController({
      messageId: "peer-fedcba98765432100123456789abcdef",
      initialDurationSeconds: 15,
      durationChoices: [{ seconds: 15 }, { seconds: 30 }],
      now: () => 0,
      claim: async (messageId) => ({
        messageId,
        content: { kind: "text", text: "server duration wins" },
        displayDurationSeconds: 15,
      }),
    });

    expect(controller.chooseDuration(30)).toBe(true);
    await controller.play();
    expect(controller.snapshot().displaySession?.durationSeconds).toBe(15);
    controller.close();
  });
});
