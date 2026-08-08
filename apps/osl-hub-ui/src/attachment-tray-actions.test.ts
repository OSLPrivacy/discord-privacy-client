import { describe, expect, it } from "vitest";

import { createAttachmentTrayActions } from "./attachment-tray-actions";
import type { AttachmentTrayCard } from "./attachment-tray-screen";

const pictureCard: AttachmentTrayCard = {
  removableId: "aa11",
  name: "vacation.png",
  type: "image/png",
  size: 240_000,
  previewDataUrl: "data:image/png;base64,AAAA",
};

const documentCard: AttachmentTrayCard = {
  removableId: "bb22",
  name: "Quarterly Report.pdf",
  type: "application/pdf",
  size: 4_096,
  previewDataUrl: null,
};

describe("TASK 0626 connect tray screen actions", () => {
  it("removes one card by removable id via a direct command, leaving the other card", () => {
    const actions = createAttachmentTrayActions([pictureCard, documentCard]);

    actions.removeCard(pictureCard.removableId);

    const cards = actions.getCards();
    console.log(
      `TASK0626_REMOVE cards_before=2 removed_id=${pictureCard.removableId} cards_after=${cards.length} remaining_id=${cards[0]?.removableId ?? "<none>"}`,
    );
    expect(cards).toHaveLength(1);
    expect(cards[0].removableId).toBe(documentCard.removableId);
  });

  it("is a no-op when the removable id is not present in the tray", () => {
    const actions = createAttachmentTrayActions([pictureCard]);

    actions.removeCard("not-a-real-id");

    expect(actions.getCards()).toHaveLength(1);
  });

  it("reports Send disabled while an attachment check is running, and re-enabled once it ends", () => {
    const actions = createAttachmentTrayActions([]);
    const observed: boolean[] = [];
    actions.subscribe((state) => observed.push(state.checkingCount > 0));

    expect(actions.isSendDisabled()).toBe(false);
    console.log(`TASK0626_SEND_STATE phase=idle disabled=${actions.isSendDisabled()}`);

    actions.beginChecking();
    console.log(`TASK0626_SEND_STATE phase=checking disabled=${actions.isSendDisabled()}`);
    expect(actions.isSendDisabled()).toBe(true);

    actions.endChecking();
    console.log(`TASK0626_SEND_STATE phase=checked disabled=${actions.isSendDisabled()}`);
    expect(actions.isSendDisabled()).toBe(false);

    expect(observed).toEqual([true, false]);
  });

  it("keeps Send disabled while any of several overlapping checks is still running", () => {
    const actions = createAttachmentTrayActions([]);

    actions.beginChecking();
    actions.beginChecking();
    expect(actions.isSendDisabled()).toBe(true);

    actions.endChecking();
    console.log(`TASK0626_SEND_STATE phase=one-of-two-finished disabled=${actions.isSendDisabled()}`);
    expect(actions.isSendDisabled()).toBe(true);

    actions.endChecking();
    console.log(`TASK0626_SEND_STATE phase=both-finished disabled=${actions.isSendDisabled()}`);
    expect(actions.isSendDisabled()).toBe(false);
  });

  it("never lets endChecking underflow the counter below zero", () => {
    const actions = createAttachmentTrayActions([]);

    actions.endChecking();

    expect(actions.isSendDisabled()).toBe(false);
    expect(actions.getState().checkingCount).toBe(0);
  });
});
