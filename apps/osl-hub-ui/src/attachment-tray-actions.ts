/**
 * TASK 0626 - connect tray screen actions.
 *
 * Wires TASK 0625's attachment tray screen (`attachment-tray-screen.ts`) to
 * real state changes: removing a card's `removableId` from the tray, and
 * disabling the Send button for as long as at least one attachment is still
 * being checked (admitted into the tray via TASK 0046/0622's size/type
 * checks). `checkingCount` is a counter rather than a boolean so overlapping
 * checks (e.g. two files dropped at once) can't let one finishing check
 * re-enable Send while another is still running.
 */
import type { AttachmentTrayCard } from "./attachment-tray-screen";
import { attachmentTrayScreenMarkup, bindAttachmentTrayScreen } from "./attachment-tray-screen";

export interface AttachmentTrayActionsState {
  cards: readonly AttachmentTrayCard[];
  checkingCount: number;
}

export interface AttachmentTrayActions {
  getState(): AttachmentTrayActionsState;
  getCards(): readonly AttachmentTrayCard[];
  /** Appends checked records admitted by a picker, paste, or drop intake. */
  addCards(cards: readonly AttachmentTrayCard[]): void;
  /** Removes one card by its tray `removableId`. No-op if not present. */
  removeCard(removableId: string): void;
  /** Marks the start of one in-flight attachment check. */
  beginChecking(): void;
  /** Marks the end of one in-flight attachment check. */
  endChecking(): void;
  /** True while at least one attachment check is in flight. */
  isSendDisabled(): boolean;
  subscribe(listener: (state: AttachmentTrayActionsState) => void): () => void;
}

export function createAttachmentTrayActions(initialCards: readonly AttachmentTrayCard[] = []): AttachmentTrayActions {
  let cards: readonly AttachmentTrayCard[] = [...initialCards];
  let checkingCount = 0;
  const listeners = new Set<(state: AttachmentTrayActionsState) => void>();

  const state = (): AttachmentTrayActionsState => ({ cards, checkingCount });
  const notify = (): void => {
    const snapshot = state();
    listeners.forEach((listener) => listener(snapshot));
  };

  return {
    getState: state,
    getCards: () => cards,
    addCards(nextCards) {
      if (nextCards.length === 0) return;
      cards = [...cards, ...nextCards];
      notify();
    },
    removeCard(removableId) {
      const next = cards.filter((card) => card.removableId !== removableId);
      if (next.length === cards.length) return;
      cards = next;
      notify();
    },
    beginChecking() {
      checkingCount += 1;
      notify();
    },
    endChecking() {
      checkingCount = Math.max(0, checkingCount - 1);
      notify();
    },
    isSendDisabled: () => checkingCount > 0,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

/** Keeps `root`'s tray markup and `sendButton`'s disabled state in sync with `actions`. */
export function bindAttachmentTrayActions(
  root: HTMLElement,
  sendButton: HTMLButtonElement,
  actions: AttachmentTrayActions,
): () => void {
  const render = (): void => {
    root.innerHTML = attachmentTrayScreenMarkup(actions.getCards());
    bindAttachmentTrayScreen(root, (removableId) => actions.removeCard(removableId));
    const disabled = actions.isSendDisabled();
    sendButton.disabled = disabled;
    sendButton.setAttribute("aria-disabled", disabled ? "true" : "false");
  };
  render();
  return actions.subscribe(render);
}
