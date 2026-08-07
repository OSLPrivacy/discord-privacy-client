import { describe, expect, it, vi } from "vitest";
import {
  EMPTY_VIEW_ONCE_PLAYER_SCREEN,
  closeViewOnceItem,
  openViewOnceItem,
  tickViewOnceItem,
  viewOncePlayerScreenMarkup,
} from "./view-once-player-screen";

describe("view-once player screen", () => {
  it("renders nothing playable in the empty state", () => {
    const markup = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN);
    expect(markup).toContain('data-vop-state="empty"');
    expect(markup).not.toContain("data-vop-play");
    expect(markup).not.toContain("data-vop-close");
    expect(markup).not.toContain("data-vop-countdown");
  });

  it("shows the play state, the open item, its countdown, and its X close action together", () => {
    let now = 0;
    const onClose = vi.fn();
    const open = openViewOnceItem("msg-open", 10_000, { now: () => now, onClose });
    const model = { play: { id: "msg-play" }, open };
    const markup = viewOncePlayerScreenMarkup(model);

    expect(markup).toContain('data-vop-state="populated"');
    expect(markup).toContain('data-vop-item="msg-play"');
    expect(markup).toContain('data-vop-item-state="play"');
    expect(markup).toMatch(/data-vop-play[^>]*>&#9654;/u);
    expect(markup).toContain('data-vop-item="msg-open"');
    expect(markup).toContain('data-vop-item-state="open"');
    expect(markup).toMatch(/data-vop-countdown>10</u);
    expect(markup).toMatch(/data-vop-close[^>]*>&times;/u);
  });

  it("counts down without extending on a backwards clock sample, matching the underlying timer", () => {
    let now = 0;
    let open = openViewOnceItem("msg-open", 5_000, { now: () => now, onClose: vi.fn() });
    now = 2_000;
    open = tickViewOnceItem(open);
    expect(open.snapshot.remainingSeconds).toBe(3);

    now = 1_000;
    open = tickViewOnceItem(open);
    expect(open.snapshot.remainingSeconds).toBe(3);
  });

  it("the X close action ends the display early and fires onClose exactly once", () => {
    const onClose = vi.fn();
    const open = openViewOnceItem("msg-open", 5_000, { now: () => 0, onClose });
    const closed = closeViewOnceItem(open);
    expect(closed.snapshot.closed).toBe(true);
    expect(closed.snapshot.remainingSeconds).toBe(0);
    expect(onClose).toHaveBeenCalledOnce();

    const markup = viewOncePlayerScreenMarkup({ play: null, open: closed });
    expect(markup).toMatch(/data-vop-countdown>0</u);
  });

  it("the populated markup differs from the empty-state markup", () => {
    const empty = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN);
    const populated = viewOncePlayerScreenMarkup({
      play: { id: "msg-play" },
      open: openViewOnceItem("msg-open", 5_000, { now: () => 0, onClose: vi.fn() }),
    });
    expect(populated).not.toBe(empty);
  });
});
