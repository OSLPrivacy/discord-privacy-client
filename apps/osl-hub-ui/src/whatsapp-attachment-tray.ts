export const WHATSAPP_ATTACHMENT_MAX_FILES = 16;
export const WHATSAPP_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export type WhatsAppAttachmentFile = Readonly<{
  name: string;
  size: number;
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
  const cards = [...current.cards];
  const rejected: string[] = [];
  for (const file of files) {
    const name = fileName(file);
    if (!name || !Number.isSafeInteger(file.size) || file.size <= 0) {
      rejected.push("Choose a valid file.");
    } else if (file.size > WHATSAPP_ATTACHMENT_MAX_BYTES) {
      rejected.push(`${name} is larger than 8 MiB.`);
    } else if (cards.length >= WHATSAPP_ATTACHMENT_MAX_FILES) {
      rejected.push(`The attachment tray holds up to ${WHATSAPP_ATTACHMENT_MAX_FILES} files.`);
    } else {
      cards.push({ name, size: file.size, sizeLabel: whatsappAttachmentSizeLabel(file.size), state: "unsent" });
    }
  }
  return { cards, rejected };
}

export const addPickedWhatsAppAttachmentFiles = addWhatsAppAttachmentFiles;
export const addDroppedWhatsAppAttachmentFiles = addWhatsAppAttachmentFiles;
