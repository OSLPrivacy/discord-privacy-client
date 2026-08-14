import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  addPickedWhatsAppAttachmentFiles,
  invokeWhatsAppAttachmentSendControl,
  whatsappAttachmentSendControlAvailable,
  type WhatsAppAttachmentFile,
  type WhatsAppAttachmentTray,
} from "./whatsapp-attachment-tray";

type InvalidInputId = "17-files" | "over-8-MB" | "folder";
type LimitsFixture = Readonly<{
  exactValidCard: WhatsAppAttachmentFile;
  invalidAttachmentInputs: readonly Readonly<{
    id: InvalidInputId;
    files: readonly WhatsAppAttachmentFile[];
  }>[];
}>;

const defaultFixture = new URL("./fixtures/task-1085-whatsapp-attachment-limits.json", import.meta.url);

function selectedFixture(): { fixture: LimitsFixture; source: string } {
  const override = process.env.TASK_1085_ATTACHMENT_FIXTURE;
  const source = override ? resolve(override) : defaultFixture.pathname;
  return {
    fixture: JSON.parse(readFileSync(source, "utf8")) as LimitsFixture,
    source,
  };
}

describe("TASK 1085 - break WhatsApp attachment limits", () => {
  it("refuses every named invalid input and only enables send for the exact valid-card fixture", () => {
    const { fixture, source } = selectedFixture();
    const expectedInvalidIds: readonly InvalidInputId[] = ["17-files", "over-8-MB", "folder"];
    const actualInvalidIds = fixture.invalidAttachmentInputs.map(({ id }) => id);
    console.log(`TASK1085 fixture=${source} invalid_input_ids=${actualInvalidIds.join(",") || "none"}`);
    expect(actualInvalidIds).toEqual(expectedInvalidIds);

    const empty: WhatsAppAttachmentTray = { cards: [], rejected: [] };
    expect(whatsappAttachmentSendControlAvailable(empty)).toBe(false);
    let directInvokeRefusal = "";
    try {
      invokeWhatsAppAttachmentSendControl(empty);
    } catch (error) {
      directInvokeRefusal = error instanceof Error ? error.message : String(error);
    }
    expect(directInvokeRefusal).toContain("unavailable without a valid card");
    console.log(`TASK1085 send_available_before=false direct_invoke_refused=true message=${directInvokeRefusal}`);

    let availabilityTransitions = 0;
    let previousAvailability = false;
    for (const invalid of fixture.invalidAttachmentInputs) {
      const attempted = addPickedWhatsAppAttachmentFiles(empty, invalid.files);
      const available = whatsappAttachmentSendControlAvailable(attempted);
      if (available !== previousAvailability) availabilityTransitions += 1;
      previousAvailability = available;

      expect(attempted.cards, `${invalid.id} created a valid card`).toEqual([]);
      expect(attempted.rejected, `${invalid.id} was not refused`).toHaveLength(1);
      console.log(`TASK1085 refused=${invalid.id} attempted_files=${invalid.files.length} cards=${attempted.cards.length} send_available=${available}`);
    }

    const validTray = addPickedWhatsAppAttachmentFiles(empty, [fixture.exactValidCard]);
    const availableAfterValid = whatsappAttachmentSendControlAvailable(validTray);
    if (availableAfterValid !== previousAvailability) availabilityTransitions += 1;
    expect(availableAfterValid).toBe(true);
    expect(validTray.cards).toEqual([{
      name: "whatsapp-valid-card-1085.bin",
      size: 8 * 1024 * 1024,
      sizeLabel: "8.0 MiB",
      state: "unsent",
    }]);
    console.log(`TASK1085 exact_valid_card=${validTray.cards[0]?.name} bytes=${validTray.cards[0]?.size} state=${validTray.cards[0]?.state} send_available_after=true`);

    const originalCards = validTray.cards.map((card) => ({ ...card }));
    for (const invalid of fixture.invalidAttachmentInputs) {
      const attempted = addPickedWhatsAppAttachmentFiles(validTray, invalid.files);
      expect(attempted.cards, `${invalid.id} changed the valid unsent card`).toEqual(originalCards);
      expect(attempted.rejected).toHaveLength(1);
      expect(whatsappAttachmentSendControlAvailable(attempted)).toBe(true);
    }

    const sendReceipt = invokeWhatsAppAttachmentSendControl(validTray);
    expect(sendReceipt.realMessageSent).toBe(false);
    expect(sendReceipt.cards).toEqual(originalCards);
    expect(sendReceipt.cards.every(({ state }) => state === "unsent")).toBe(true);
    expect(availabilityTransitions).toBe(1);
    console.log(`TASK1085 refused_count=${fixture.invalidAttachmentInputs.length} final_valid_cards=${sendReceipt.cards.length} final_unsent_cards=${sendReceipt.cards.filter(({ state }) => state === "unsent").length} availability_transitions=${availabilityTransitions} real_message_sent=${sendReceipt.realMessageSent}`);
  });
});
