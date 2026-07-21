import { describe, expect, it } from "vitest";
import { OverlaySendGesture, type OverlayEnterGesture } from "./overlay-send-gesture";

const enter = (now: number, overrides: Partial<OverlayEnterGesture> = {}): OverlayEnterGesture => ({
  key: "Enter", shiftKey: false, repeat: false, isTrusted: true, isComposing: false, now, ...overrides,
});

describe("native overlay send gestures", () => {
  it("keeps button mode and Shift+Enter multiline-only", () => {
    const gesture = new OverlaySendGesture();
    expect(gesture.keydown(enter(1))).toBe("none");
    gesture.setMode("single");
    expect(gesture.keydown(enter(2, { shiftKey: true }))).toBe("none");
  });

  it("requires distinct trusted double-enter down/up gestures", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10))).toBe("none");
    expect(gesture.keydown(enter(11, { repeat: true }))).toBe("none");
    expect(gesture.keyup(enter(12))).toBe("armed");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("send");
  });

  it("ignores synthetic input and expires without sending", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    expect(gesture.keydown(enter(10, { isTrusted: false }))).toBe("none");
    expect(gesture.keyup(enter(11, { isTrusted: false }))).toBe("none");
    expect(gesture.keydown(enter(20))).toBe("none");
    expect(gesture.keyup(enter(21))).toBe("armed");
    expect(gesture.expire(1_221)).toBe(true);
    expect(gesture.keydown(enter(1_222))).toBe("none");
    expect(gesture.keyup(enter(1_223))).toBe("armed");
  });

  it("resets its armed state whenever mode changes", () => {
    const gesture = new OverlaySendGesture();
    gesture.setMode("double");
    gesture.keydown(enter(10));
    expect(gesture.keyup(enter(11))).toBe("armed");
    gesture.setMode("single");
    gesture.setMode("double");
    gesture.keydown(enter(12));
    expect(gesture.keyup(enter(13))).toBe("armed");
  });
});
