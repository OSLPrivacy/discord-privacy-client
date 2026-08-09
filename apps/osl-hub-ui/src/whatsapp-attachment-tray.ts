export const WHATSAPP_ATTACHMENT_MAX_FILES = 16;
export const WHATSAPP_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export type WhatsAppAttachmentFile = Readonly<{
  name: string;
  size: number;
  /** Picker/drop adapters use this to keep directories out of file admission. */
  kind?: "file" | "folder";
}>;

export type WhatsAppAttachmentTrayCard = Readonly<{
  name: string;
  size: number;
  sizeLabel: string;
  state: "unsent";
}>;

export type WhatsAppAttachmentTray = Readonly<{
  cards: readonly WhatsAppAttachmentTrayCard[];
  rejected: readonly string[];
}>;

const emptyTray = (): WhatsAppAttachmentTray => ({ cards: [], rejected: [] });

function fileName(file: WhatsAppAttachmentFile): string | null {
  const name = file.name.trim();
  return name.length > 0 && name.length <= 255 ? name : null;
}

export function whatsappAttachmentSizeLabel(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(bytes < 1024 * 1024 ? 2 : 1)} MiB`;
}

/**
 * The picker and drop handlers deliberately call this same reducer. It stages
 * metadata only: accepted cards stay unsent until a separate send flow exists.
 */
export function addWhatsAppAttachmentFiles(
  current: WhatsAppAttachmentTray = emptyTray(),
  files: Iterable<WhatsAppAttachmentFile>,
): WhatsAppAttachmentTray {
  const incoming = [...files];
  if (current.cards.length + incoming.length > WHATSAPP_ATTACHMENT_MAX_FILES) {
    return {
      cards: current.cards,
      rejected: [`The attachment tray holds up to ${WHATSAPP_ATTACHMENT_MAX_FILES} files.`],
    };
  }
  if (incoming.some((file) => file.kind === "folder")) {
    return { cards: current.cards, rejected: ["Folders cannot be attached."] };
  }

  const admitted: WhatsAppAttachmentTrayCard[] = [];
  for (const file of incoming) {
    const name = fileName(file);
    if (!name || !Number.isSafeInteger(file.size) || file.size <= 0) {
      return { cards: current.cards, rejected: ["Choose a valid file."] };
    } else if (file.size > WHATSAPP_ATTACHMENT_MAX_BYTES) {
      return { cards: current.cards, rejected: [`${name} is larger than 8 MiB.`] };
    } else {
      admitted.push({ name, size: file.size, sizeLabel: whatsappAttachmentSizeLabel(file.size), state: "unsent" });
    }
  }
  return { cards: [...current.cards, ...admitted], rejected: [] };
}

export const addPickedWhatsAppAttachmentFiles = addWhatsAppAttachmentFiles;
export const addDroppedWhatsAppAttachmentFiles = addWhatsAppAttachmentFiles;

export type WhatsAppAttachmentSendReceipt = Readonly<{
  cards: readonly WhatsAppAttachmentTrayCard[];
  realMessageSent: false;
}>;

export function whatsappAttachmentSendControlAvailable(
  tray: WhatsAppAttachmentTray = emptyTray(),
): boolean {
  return tray.cards.length > 0;
}

/**
 * Models a direct activation of the attachment send control. Task 1084 only
 * stages unsent metadata, so this receipt never claims that a provider send
 * occurred; its fail-closed precondition prevents empty/invalid trays from
 * reaching a later transport implementation.
 */
export function invokeWhatsAppAttachmentSendControl(
  tray: WhatsAppAttachmentTray = emptyTray(),
): WhatsAppAttachmentSendReceipt {
  if (!whatsappAttachmentSendControlAvailable(tray)) {
    throw new Error("WhatsApp attachment send control is unavailable without a valid card");
  }
  return { cards: tray.cards, realMessageSent: false };
}
