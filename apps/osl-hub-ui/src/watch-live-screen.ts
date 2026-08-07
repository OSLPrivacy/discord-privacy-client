import type { WebsiteLiveRunProgress } from "./adapters";

export interface WatchLiveScreenSnapshot {
  readonly runId: string;
  readonly visibleProgressCount: number;
  readonly activeAccount: string;
  readonly currentConversation: string;
}

type PersistedProgressStorage = Pick<Storage, "getItem" | "setItem">;

const WATCH_LIVE_STORAGE_KEY = "osl.watchLive.progress.v1";

interface StoredWatchLiveProgress {
  runId: string;
  visibleProgressCount: number;
}

function readStoredProgress(storage: PersistedProgressStorage): StoredWatchLiveProgress | null {
  try {
    const raw = storage.getItem(WATCH_LIVE_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null) return null;
    const { runId, visibleProgressCount } = parsed as Record<string, unknown>;
    if (typeof runId !== "string" || runId.length === 0) return null;
    if (!Number.isSafeInteger(visibleProgressCount) || (visibleProgressCount as number) < 0) return null;
    return { runId, visibleProgressCount: visibleProgressCount as number };
  } catch {
    return null;
  }
}

function writeStoredProgress(storage: PersistedProgressStorage, value: StoredWatchLiveProgress): void {
  try {
    storage.setItem(WATCH_LIVE_STORAGE_KEY, JSON.stringify(value));
  } catch {
    // A storage write failure (quota, private mode) just means the count
    // will not survive a reopen; the live count in this instance still works.
  }
}

/**
 * Quiet progress line for the watch-live screen: the active account, the
 * current conversation, and a visible progress count that only advances when
 * the underlying run has actually moved, not on every poll.
 *
 * The count is persisted per run id so reopening the same run shows the same
 * count instead of resetting to zero, while a new run id starts fresh.
 */
export class WatchLiveScreen {
  private visibleProgressCount: number;
  private lastProgressKey: string | null = null;
  private activeAccount = "";
  private currentConversation = "";

  constructor(private readonly runId: string, private readonly storage: PersistedProgressStorage) {
    const stored = readStoredProgress(storage);
    this.visibleProgressCount = stored && stored.runId === runId ? stored.visibleProgressCount : 0;
    writeStoredProgress(storage, { runId, visibleProgressCount: this.visibleProgressCount });
  }

  snapshot(): WatchLiveScreenSnapshot {
    return Object.freeze({
      runId: this.runId,
      visibleProgressCount: this.visibleProgressCount,
      activeAccount: this.activeAccount,
      currentConversation: this.currentConversation,
    });
  }

  applyProgress(progress: WebsiteLiveRunProgress): WatchLiveScreenSnapshot {
    this.activeAccount = progress.activeAccount;
    this.currentConversation = progress.currentPlace;
    const key = JSON.stringify(progress);
    if (key !== this.lastProgressKey) {
      this.lastProgressKey = key;
      this.visibleProgressCount += 1;
      writeStoredProgress(this.storage, { runId: this.runId, visibleProgressCount: this.visibleProgressCount });
    }
    return this.snapshot();
  }
}

export function watchLiveScreenMarkup(snapshot: WatchLiveScreenSnapshot): string {
  const account = snapshot.activeAccount.length > 0 ? snapshot.activeAccount : "—";
  const conversation = snapshot.currentConversation.length > 0 ? snapshot.currentConversation : "—";
  return (
    `<div class="watch-live-screen" data-run-id="${escapeMarkupAttribute(snapshot.runId)}">` +
    `<p class="watch-live-progress-line" role="status">Progress: ${snapshot.visibleProgressCount}</p>` +
    `<p class="watch-live-account">Account: ${escapeMarkupText(account)}</p>` +
    `<p class="watch-live-conversation">Conversation: ${escapeMarkupText(conversation)}</p>` +
    `</div>`
  );
}

function escapeMarkupText(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function escapeMarkupAttribute(value: string): string {
  return escapeMarkupText(value).replace(/"/g, "&quot;");
}
