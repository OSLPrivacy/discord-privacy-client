/**
 * Attachment intake for the private Instagram composer.  Files remain local
 * tray cards until a later, explicit send operation consumes them.
 */

export const INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES = 16;
export const INSTAGRAM_ATTACHMENT_MAX_BYTES = 8 * 1024 * 1024;

export interface InstagramAttachmentFile {
  readonly name: string;
  readonly size: number;
  /** Set by intake adapters when a dropped candidate is a directory. */
  readonly isDirectory?: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
}

export type InstagramAttachmentRefusalName = "17-files" | "over-8-MB" | "folder";

export class InstagramAttachmentRefusal extends Error {
  constructor(readonly refusalName: InstagramAttachmentRefusalName) {
    const detail = refusalName === "17-files"
      ? "tray accepts at most 16 files"
      : refusalName === "over-8-MB"
        ? "file must be 1 byte through 8 MiB"
        : "folders cannot be attached";
    super(`Instagram attachment refused: ${refusalName} (${detail})`);
    this.name = "InstagramAttachmentRefusal";
  }
}

export interface InstagramAttachmentCard {
  readonly id: string;
  readonly name: string;
  readonly sizeBytes: number;
  readonly fingerprint: string;
}

export interface InstagramAttachmentTray {
  readonly cards: InstagramAttachmentCard[];
  /** Intake deliberately never increments this; Send owns its own counter. */
  readonly sendCount: number;
}

const validFile = (file: InstagramAttachmentFile): boolean =>
  typeof file.name === "string"
  && file.name.trim().length > 0
  && Number.isSafeInteger(file.size)
  && file.size > 0
  && file.size <= INSTAGRAM_ATTACHMENT_MAX_BYTES;

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function fingerprint(file: InstagramAttachmentFile): Promise<string> {
  const bytes = await file.arrayBuffer();
  if (bytes.byteLength !== file.size) throw new Error("Instagram attachment changed while it was being checked");
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return `sha256:${hex(new Uint8Array(digest))}`;
}

export function createInstagramAttachmentTray(): InstagramAttachmentTray {
  return { cards: [], sendCount: 0 };
}

/**
 * The common picker/drop admission path.  It checks a whole selection before
 * appending anything so an over-limit choice cannot leave a partial tray.
 */
export async function addInstagramAttachments(
  tray: InstagramAttachmentTray,
  files: readonly InstagramAttachmentFile[],
): Promise<readonly InstagramAttachmentCard[]> {
  if (files.length === 0) throw new Error("Choose at least one Instagram attachment");
  if (tray.cards.length + files.length > INSTAGRAM_ATTACHMENT_TRAY_MAX_FILES) {
    throw new InstagramAttachmentRefusal("17-files");
  }
  if (files.some((file) => file.isDirectory === true)) {
    throw new InstagramAttachmentRefusal("folder");
  }
  if (files.some((file) => file.size > INSTAGRAM_ATTACHMENT_MAX_BYTES)) {
    throw new InstagramAttachmentRefusal("over-8-MB");
  }
  if (!files.every(validFile)) throw new Error("Each Instagram attachment must be a non-empty file");

  const added = await Promise.all(files.map(async (file, index) => ({
    id: `instagram-attachment-${String(tray.cards.length + index).padStart(2, "0")}`,
    name: file.name,
    sizeBytes: file.size,
    fingerprint: await fingerprint(file),
  })));
  tray.cards.push(...added);
  return added;
}

export const addInstagramPickerFiles = addInstagramAttachments;
export const addInstagramDroppedFiles = addInstagramAttachments;

export function instagramAttachmentTrayMarkup(tray: InstagramAttachmentTray): string {
  const cards = tray.cards.map((card) => `<li data-instagram-attachment-card="${card.id}"><strong>${card.name}</strong><small>${card.sizeBytes.toLocaleString("en-US")} bytes · ${card.fingerprint}</small></li>`).join("");
  return `<section data-instagram-attachment-tray data-instagram-attachment-count="${tray.cards.length}"><button type="button" data-instagram-attachment-picker>Choose files</button><input data-instagram-attachment-input type="file" multiple hidden><ul>${cards}</ul></section>`;
}

/** Wire the rendered picker and a private composer drop zone; neither handler sends. */
export function bindInstagramAttachmentTray(
  root: HTMLElement,
  dropZone: HTMLElement,
  tray: InstagramAttachmentTray,
  onCardsAdded: () => void,
): void {
  const input = root.querySelector<HTMLInputElement>("[data-instagram-attachment-input]");
  root.querySelector<HTMLButtonElement>("[data-instagram-attachment-picker]")?.addEventListener("click", () => input?.click());
  input?.addEventListener("change", () => {
    const files = Array.from(input.files ?? []);
    if (files.length) void addInstagramPickerFiles(tray, files).then(onCardsAdded);
    input.value = "";
  });
  dropZone.addEventListener("dragover", (event) => event.preventDefault());
  dropZone.addEventListener("drop", (event) => {
    event.preventDefault();
    const files = Array.from(event.dataTransfer?.files ?? []);
    if (files.length) void addInstagramDroppedFiles(tray, files).then(onCardsAdded);
  });
}
