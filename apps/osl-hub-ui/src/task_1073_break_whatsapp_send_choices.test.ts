import { describe, expect, it, vi } from "vitest";
import {
  parseWhatsAppCoverInsertionSetting,
  parseWhatsAppSendTrigger,
  prepareWhatsAppSelectedCover,
  type WhatsAppPreparedSend,
  type WhatsAppSendTrigger,
} from "./whatsapp-send";

const marker = "whatsapp-send-1073";
const carrier = () => ({
  provider: "whatsapp",
  status: "readyForExplicitPlacement",
  coverText: marker,
  expiresAt: 1_800_000_000,
  personToPersonE2ee: true,
  contextBindingSha256: "a".repeat(64),
  automaticPlacement: false,
  realMessageSent: false,
});

describe("TASK 1073 break WhatsApp send choices", () => {
  it("keeps one prepared cover while every named invalid choice is refused", async () => {
    const prepare = vi.fn().mockResolvedValue(carrier());
    const preparedCovers: WhatsAppPreparedSend[] = [];
    const insertion = parseWhatsAppCoverInsertionSetting("insert-on-send");
    const good = await prepareWhatsAppSelectedCover(
      parseWhatsAppSendTrigger("Enter"),
      insertion,
      { privateText: marker, foundBox: true },
      prepare,
    );
    preparedCovers.push(good);

    expect(prepare).toHaveBeenCalledTimes(1);
    expect(preparedCovers).toHaveLength(1);
    expect(preparedCovers[0]?.carrier.coverText).toBe(marker);
    console.info(`TASK1073_GOOD private_text=${marker} prepared_count=${preparedCovers.length} cover=${preparedCovers[0]?.carrier.coverText}`);

    const choices: readonly WhatsAppSendTrigger[] = ["Enter", "Enter x2", "Clipboard"];
    for (const choice of choices) {
      await expect(prepareWhatsAppSelectedCover(
        choice,
        insertion,
        { privateText: "", foundBox: true },
        prepare,
      )).rejects.toThrow(`WhatsApp ${choice} refused: empty-text`);
      expect(prepare).toHaveBeenCalledTimes(1);
      console.info(`TASK1073_REFUSED trigger=${choice} reason=empty-text prepare_count=${prepare.mock.calls.length}`);

      await expect(prepareWhatsAppSelectedCover(
        choice,
        insertion,
        { privateText: marker, foundBox: false },
        prepare,
      )).rejects.toThrow(`WhatsApp ${choice} refused: missing-box`);
      expect(prepare).toHaveBeenCalledTimes(1);
      console.info(`TASK1073_REFUSED trigger=${choice} reason=missing-box prepare_count=${prepare.mock.calls.length}`);
    }

    for (const retired of ["Manual", "Instant", "Match typing"] as const) {
      expect(() => parseWhatsAppSendTrigger(retired)).toThrow(retired);
      console.info(`TASK1073_RETIRED trigger=${retired} refused_name=${retired}`);
    }

    expect(preparedCovers).toEqual([good]);
    expect(preparedCovers).toHaveLength(1);
    expect(prepare).toHaveBeenCalledTimes(1);
    console.info(`TASK1073_UNCHANGED private_text=${marker} prepared_count=${preparedCovers.length} cover=${preparedCovers[0]?.carrier.coverText}`);
  });
});
