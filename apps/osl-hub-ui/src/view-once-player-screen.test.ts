import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import {
  EMPTY_VIEW_ONCE_PLAYER_SCREEN,
  closeViewOnceItem,
  openViewOnceItem,
  tickViewOnceItem,
  viewOncePlayerScreenMarkup,
} from "./view-once-player-screen";

// TASK 3161: read from the SAME words file the backend serves.
const SCREEN_WORDS_URL = new URL("../../../crates/ipc/src/screen_words/", import.meta.url);
const EN_WORDS = JSON.parse(readFileSync(new URL("en.json", SCREEN_WORDS_URL), "utf8")).view_once_player;
const ES_WORDS = JSON.parse(readFileSync(new URL("es.json", SCREEN_WORDS_URL), "utf8")).view_once_player;

describe("view-once player screen", () => {
  it("renders nothing playable in the empty state", () => {
    const markup = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN, EN_WORDS);
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
    const markup = viewOncePlayerScreenMarkup(model, EN_WORDS);

    expect(markup).toContain('data-vop-state="populated"');
    expect(markup).toContain('data-vop-item="msg-play"');
    expect(markup).toContain('data-vop-item-state="play"');
    expect(markup).toMatch(/data-vop-play[^>]*>&#9654;/u);
    expect(markup).toContain('data-vop-item="msg-open"');
    expect(markup).toContain('data-vop-item-state="open"');
    expect(markup).toMatch(/data-vop-countdown>10</u);
    expect(markup).toMatch(/data-vop-close[^>]*>&times;/u);
    expect(markup).toContain('aria-label="Play view once message"');
    expect(markup).toContain('aria-label="Closes in 10 seconds"');
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

    const markup = viewOncePlayerScreenMarkup({ play: null, open: closed }, EN_WORDS);
    expect(markup).toMatch(/data-vop-countdown>0</u);
  });

  it("the populated markup differs from the empty-state markup", () => {
    const empty = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN, EN_WORDS);
    const populated = viewOncePlayerScreenMarkup(
      {
        play: { id: "msg-play" },
        open: openViewOnceItem("msg-open", 5_000, { now: () => 0, onClose: vi.fn() }),
      },
      EN_WORDS,
    );
    expect(populated).not.toBe(empty);
  });

  it("TASK 3161: shows different words for the SAME model once the language changes, no restart", () => {
    const english = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN, EN_WORDS);
    const spanish = viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN, ES_WORDS);
    expect(english).toContain("No view-once messages to show.");
    expect(spanish).toContain("No hay mensajes de una sola vista para mostrar.");
    expect(english).not.toBe(spanish);
  });

  it("TASK 3161: a screen word missing from the language file breaks loudly, not blankly", () => {
    expect(() => viewOncePlayerScreenMarkup(EMPTY_VIEW_ONCE_PLAYER_SCREEN, {})).toThrow(
      /missing view once player screen word/u,
    );
  });
});
