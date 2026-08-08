/**
 * Local staging for the reviewed Messenger composer.  This is deliberately a
 * tray, not a send queue: choosing or dropping a file only creates a visible
 * card.  Messenger delivery remains an explicit action outside this module.
 */

export const MESSENGER_ATTACHMENT_TRAY_MAX_FILES = 16;
export const MESSENGER_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export interface MessengerAttachmentSource {
  name: string;
  size: number;
  bytes: Uint8Array;
}

export interface MessengerAttachmentCard {
  id: string;
  name: string;
  size: number;
  fingerprint: string;
}

export interface MessengerAttachmentTray {
  cards: MessengerAttachmentCard[];
  /** Only the explicit Messenger Send action may increment this counter. */
  sendCount: number;
}

export interface MessengerAttachmentReceipt {
  addedCardCount: number;
  trayCardCount: number;
  sendCount: number;
  card: MessengerAttachmentCard;
}

export function createMessengerAttachmentTray(): MessengerAttachmentTray {
  return { cards: [], sendCount: 0 };
}

const isSafeName = (name: string): boolean => name.length > 0 && name.length <= 255 && !/[\0-\x1f\x7f]/u.test(name);

/** A stable short identity of the bytes displayed by both intake paths. */
export function messengerAttachmentFingerprint(name: string, bytes: Uint8Array): string {
  let hash = 0xcbf29ce484222325n;
  const mask = 0xffffffffffffffffn;
  for (const byte of bytes) {
    hash = (hash ^ BigInt(byte)) & mask;
    hash = (hash * 0x00000100000001b3n) & mask;
  }
  const stem = name.split(".")[0]?.replace(/[^A-Za-z0-9]/gu, "").slice(0, 16).toUpperCase() || "FILE";
  return `${stem}-${String(hash % 10000n).padStart(4, "0")}`;
}

function stageOne(
  tray: MessengerAttachmentTray,
  source: MessengerAttachmentSource,
): MessengerAttachmentReceipt {
  if (!isSafeName(source.name)) throw new Error("Messenger attachment filename is invalid");
  if (!Number.isSafeInteger(source.size) || source.size < 0 || source.size !== source.bytes.byteLength) {
    throw new Error("Messenger attachment size does not match its bytes");
  }
  if (source.size > MESSENGER_ATTACHMENT_MAX_BYTES) {
    throw new Error("Messenger attachments must be 8 MiB or smaller");
  }
  if (tray.cards.length >= MESSENGER_ATTACHMENT_TRAY_MAX_FILES) {
    throw new Error("Messenger attachment tray holds at most 16 files");
  }

  const card: MessengerAttachmentCard = {
    id: `messenger-attachment-${String(tray.cards.length).padStart(2, "0")}`,
    name: source.name,
    size: source.size,
    fingerprint: messengerAttachmentFingerprint(source.name, source.bytes),
  };
  tray.cards.push(card);
  return { addedCardCount: 1, trayCardCount: tray.cards.length, sendCount: tray.sendCount, card };
}

/** The native file chooser admits exactly one file for this interaction. */
export function addMessengerPickerAttachment(
  tray: MessengerAttachmentTray,
  selected: readonly MessengerAttachmentSource[],
): MessengerAttachmentReceipt {
  if (selected.length !== 1) throw new Error("Choose exactly one Messenger attachment");
  return stageOne(tray, selected[0]!);
}

/** A drop is intentionally equivalent to one picker selection, never Send. */
export function addMessengerDroppedAttachment(
  tray: MessengerAttachmentTray,
  dropped: readonly MessengerAttachmentSource[],
): MessengerAttachmentReceipt {
  if (dropped.length !== 1) throw new Error("Drop exactly one Messenger attachment");
  return stageOne(tray, dropped[0]!);
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

export function messengerAttachmentTrayMarkup(tray: MessengerAttachmentTray): string {
  const cards = tray.cards.map((card) => `<li data-messenger-attachment-card="${card.id}" data-messenger-attachment-fingerprint="${card.fingerprint}"><strong>${escapeHtml(card.name)}</strong><small>${card.size.toLocaleString("en-US")} bytes · ${escapeHtml(card.fingerprint)}</small></li>`).join("");
  return `<ul data-messenger-attachment-tray data-messenger-attachment-count="${tray.cards.length}" data-messenger-send-count="${tray.sendCount}">${cards}</ul>`;
}

async function sourceFromFile(file: File): Promise<MessengerAttachmentSource> {
  return { name: file.name, size: file.size, bytes: new Uint8Array(await file.arrayBuffer()) };
}

/** Bind the real picker and drag/drop events to the same one-card staging path. */
export function bindMessengerAttachmentTray(
  picker: HTMLInputElement,
  dropTarget: HTMLElement,
  tray: MessengerAttachmentTray,
  onStaged: (receipt: MessengerAttachmentReceipt) => void,
  onRefused?: (message: string) => void,
): void {
  const stage = async (files: FileList | null, fromDrop: boolean): Promise<void> => {
    try {
      const sources = await Promise.all(Array.from(files ?? []).map(sourceFromFile));
      onStaged(fromDrop ? addMessengerDroppedAttachment(tray, sources) : addMessengerPickerAttachment(tray, sources));
    } catch (error) {
      onRefused?.(error instanceof Error ? error.message : String(error));
    }
  };
  picker.addEventListener("change", () => { void stage(picker.files, false); });
  dropTarget.addEventListener("dragover", (event) => event.preventDefault());
  dropTarget.addEventListener("drop", (event) => {
    event.preventDefault();
    void stage(event.dataTransfer?.files ?? null, true);
  });
}
