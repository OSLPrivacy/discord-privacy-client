import { describe, expect, it, vi } from "vitest";
import { parseWhatsAppCoverInsertionSetting, parseWhatsAppSendTrigger, prepareWhatsAppSelectedCover, whatsappPreparedSendReport } from "./whatsapp-send";

const carrier = () => ({ provider: "whatsapp", status: "readyForExplicitPlacement", coverText: "ordinary-looking protected carrier", expiresAt: 1_800_000_000, personToPersonE2ee: true, contextBindingSha256: "a".repeat(64), automaticPlacement: false, realMessageSent: false });

describe("WhatsApp selected-cover send controls", () => {
  it("prepares each named trigger without posting and reports its separate insertion setting", async () => {
    const cases = [["Enter", "insert-on-send", "Insert on send"], ["Enter x2", "type-naturally", "Type naturally"], ["Clipboard", "insert-on-send", "Insert on send"]] as const;
    for (const [triggerName, insertionName, insertionLabel] of cases) {
      const prepare = vi.fn().mockResolvedValue(carrier());
      const prepared = await prepareWhatsAppSelectedCover(parseWhatsAppSendTrigger(triggerName), parseWhatsAppCoverInsertionSetting(insertionName), prepare);
      expect(prepare).toHaveBeenCalledOnce();
      expect(prepared).toMatchObject({ trigger: triggerName, coverInsertion: insertionName, posted: false });
      expect(whatsappPreparedSendReport(prepared)).toBe(`Prepared cover via ${triggerName}. Cover insertion: ${insertionLabel}. Nothing was posted.`);
      console.info(`TASK1072_PREPARED trigger=${triggerName} posted=${prepared.posted} cover_insertion=${insertionLabel}`);
    }
  });

  it("refuses unnamed triggers and insertion settings", () => {
    for (const trigger of ["Manual", "Instant", "Match typing"]) expect(() => parseWhatsAppSendTrigger(trigger)).toThrow();
    expect(() => parseWhatsAppCoverInsertionSetting("automatic")).toThrow();
  });
});
