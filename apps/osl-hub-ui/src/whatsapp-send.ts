import { parseWhatsAppPreparedCarrier, type WhatsAppPreparedCarrier } from "./whatsapp-overlay-prepare";

/** The only deliberate actions that may prepare a WhatsApp carrier. */
export type WhatsAppSendTrigger = "Enter" | "Enter x2" | "Clipboard";
export type WhatsAppCoverInsertionSetting = "insert-on-send" | "type-naturally";

export interface WhatsAppPreparedSend {
  trigger: WhatsAppSendTrigger;
  carrier: WhatsAppPreparedCarrier;
  coverInsertion: WhatsAppCoverInsertionSetting;
  posted: false;
}

const triggers: readonly WhatsAppSendTrigger[] = ["Enter", "Enter x2", "Clipboard"];
const insertionSettings: readonly WhatsAppCoverInsertionSetting[] = ["insert-on-send", "type-naturally"];

export const whatsappCoverInsertionLabel = (setting: WhatsAppCoverInsertionSetting): string =>
  setting === "insert-on-send" ? "Insert on send" : "Type naturally";

export function parseWhatsAppSendTrigger(value: string): WhatsAppSendTrigger {
  if (!triggers.includes(value as WhatsAppSendTrigger)) throw new Error("unsupported WhatsApp preparation trigger");
  return value as WhatsAppSendTrigger;
}

export function parseWhatsAppCoverInsertionSetting(value: string): WhatsAppCoverInsertionSetting {
  if (!insertionSettings.includes(value as WhatsAppCoverInsertionSetting)) throw new Error("unsupported WhatsApp cover insertion setting");
  return value as WhatsAppCoverInsertionSetting;
}

/** Preparation deliberately has no provider-write capability. */
export async function prepareWhatsAppSelectedCover(
  trigger: WhatsAppSendTrigger,
  coverInsertion: WhatsAppCoverInsertionSetting,
  prepare: () => Promise<unknown>,
): Promise<WhatsAppPreparedSend> {
  return { trigger, carrier: parseWhatsAppPreparedCarrier(await prepare()), coverInsertion, posted: false };
}

export function whatsappPreparedSendReport(prepared: WhatsAppPreparedSend): string {
  return `Prepared cover via ${prepared.trigger}. Cover insertion: ${whatsappCoverInsertionLabel(prepared.coverInsertion)}. Nothing was posted.`;
}
