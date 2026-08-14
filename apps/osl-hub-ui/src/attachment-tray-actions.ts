import type { AttachmentTrayCard } from "./attachment-tray-screen";

export interface AttachmentTrayActionsState {
  cards: readonly AttachmentTrayCard[];
  checkingCount: number;
}

export interface AttachmentTrayActions {
  getState(): AttachmentTrayActionsState;
  getCards(): readonly AttachmentTrayCard[];
  /** Appends checked records admitted by a picker, paste, or drop intake. */
  addCards(cards: readonly AttachmentTrayCard[]): void;
  removeCard(removableId: string): void;
  beginChecking(): void;
  endChecking(): void;
  isSendDisabled(): boolean;
}

export function createAttachmentTrayActions(initialCards: readonly AttachmentTrayCard[] = []): AttachmentTrayActions {
  let cards: readonly AttachmentTrayCard[] = [...initialCards];
  let checkingCount = 0;
  return {
    getState: () => ({ cards, checkingCount }),
    getCards: () => cards,
    addCards(nextCards) {
      if (!nextCards.length) return;
      cards = [...cards, ...nextCards];
    },
    removeCard(removableId) { cards = cards.filter((card) => card.removableId !== removableId); },
    beginChecking() { checkingCount += 1; },
    endChecking() { checkingCount = Math.max(0, checkingCount - 1); },
    isSendDisabled: () => checkingCount > 0,
  };
}
