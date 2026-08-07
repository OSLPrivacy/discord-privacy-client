/**
 * TASK 1349 - the view-once player: the play button, the open item's
 * countdown, and its X close action.
 *
 * The countdown itself is `createViewOnceTimer` from `view-once-timer.ts`
 * (already gated 1346 for backwards-clock and manual/deadline-race safety);
 * this module only turns that timer's snapshots into markup for an unopened
 * ("play") item and an opened item that is actively counting down.
 */
import { createViewOnceTimer, type MonotonicNow, type ViewOnceTimer, type ViewOnceTimerSnapshot } from "./view-once-timer";

export interface ViewOncePlayItem {
  readonly id: string;
}

export interface ViewOnceOpenItem {
  readonly id: string;
  readonly timer: ViewOnceTimer;
  readonly snapshot: ViewOnceTimerSnapshot;
}

export interface ViewOncePlayerScreenModel {
  readonly play: ViewOncePlayItem | null;
  readonly open: ViewOnceOpenItem | null;
}

export const EMPTY_VIEW_ONCE_PLAYER_SCREEN: ViewOncePlayerScreenModel = { play: null, open: null };

/** Opens a view-once item: starts its non-extendable countdown running now. */
export function openViewOnceItem(
  id: string,
  lifetimeMs: number,
  options: { now?: MonotonicNow; onClose(): void },
): ViewOnceOpenItem {
  const timer = createViewOnceTimer({ lifetimeMs, now: options.now, onClose: options.onClose });
  return { id, timer, snapshot: timer.tick() };
}

/** Re-samples an open item's countdown; returns a new snapshot, same timer. */
export function tickViewOnceItem(item: ViewOnceOpenItem): ViewOnceOpenItem {
  return { ...item, snapshot: item.timer.tick() };
}

/** The X close action: ends the display early, independent of the deadline. */
export function closeViewOnceItem(item: ViewOnceOpenItem): ViewOnceOpenItem {
  return { ...item, snapshot: item.timer.close() };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (char) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char] as string
  ));
}

function playItemMarkup(item: ViewOncePlayItem): string {
  return `<article class="vop-item vop-item-play" data-vop-item="${escapeHtml(item.id)}" data-vop-item-state="play">
      <button class="vop-play" type="button" aria-label="Play view once message" data-vop-play>&#9654;</button>
      <p class="vop-item-caption">Tap to view once</p>
    </article>`;
}

function openItemMarkup(item: ViewOnceOpenItem): string {
  const seconds = item.snapshot.closed ? 0 : item.snapshot.remainingSeconds;
  return `<article class="vop-item vop-item-open" data-vop-item="${escapeHtml(item.id)}" data-vop-item-state="open">
      <div class="vop-open-media" aria-hidden="true"></div>
      <div class="vop-countdown" role="timer" aria-label="Closes in ${seconds} seconds" data-vop-countdown>${seconds}</div>
      <button class="vop-close" type="button" aria-label="Close view once message" data-vop-close>&times;</button>
    </article>`;
}

/**
 * The screen is empty when both an unopened item and an open item are absent.
 * The populated fixture keeps both on screen at once: one item waiting to be
 * played, one already open and counting down, so a single capture proves the
 * play state, the open item, its countdown, and its X close action together.
 */
export function viewOncePlayerScreenMarkup(model: ViewOncePlayerScreenModel): string {
  if (!model.play && !model.open) {
    return `<section class="view-once-player-screen vop-empty" data-vop-state="empty">
      <p class="vop-empty-copy">No view-once messages to show.</p>
    </section>`;
  }
  const items = [
    model.play ? playItemMarkup(model.play) : "",
    model.open ? openItemMarkup(model.open) : "",
  ].join("");
  return `<section class="view-once-player-screen vop-populated" data-vop-state="populated">${items}</section>`;
}
