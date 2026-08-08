/**
 * TASK 0628 - direct file drops on a private typing box.
 *
 * A drop is intake only: it creates tray cards and never reaches the explicit
 * message-send command.  Keeping that boundary in this small module makes the
 * browser event path easy to exercise without a WebView or a real filesystem.
 * Directories are rejected before any card is appended, keeping the tray atomic.
 */
import type { AttachmentTrayActions } from "./attachment-tray-actions";
import type { AttachmentTrayCard } from "./attachment-tray-screen";

export const FOLDER_DROP_REJECTION = "Choose files, not folders";

export interface DroppedAttachmentFile {
  name: string;
  type: string;
  size: number;
  /** Set by the drop adapter when the platform reports a directory entry. */
  isDirectory?: boolean;
}

type DropItem = { webkitGetAsEntry?: () => { isDirectory: boolean } | null };
type DropEvent = Event & {
  dataTransfer: { files: ArrayLike<DroppedAttachmentFile>; items?: ArrayLike<DropItem>; types?: ArrayLike<string> } | null;
};
type DropTarget = Pick<EventTarget, "addEventListener">;

function carriesFiles(rawEvent: Event): boolean {
  const transfer = (rawEvent as DropEvent).dataTransfer;
  if (!transfer) return false;
  return transfer.files.length > 0 || Array.from(transfer.types ?? []).includes("Files");
}

export interface PrivateTypingDropOutlineState {
  readonly outlineVisible: boolean;
  readonly text: "Drop to attach" | null;
}

export function privateTypingDropOutlineState(dragOver: boolean): PrivateTypingDropOutlineState {
  return { outlineVisible: dragOver, text: dragOver ? "Drop to attach" : null };
}

/** Maps files accepted at the private composer directly to tray records. */
export function directPrivateTypingDropCommand(
  files: Iterable<DroppedAttachmentFile>,
  tray: AttachmentTrayActions,
): readonly AttachmentTrayCard[] {
  const dropped = [...files];
  const records = dropped
    .filter((file) => !file.isDirectory)
    .filter((file) => file.name.trim().length > 0 && Number.isFinite(file.size) && file.size >= 0)
    .map((file, index) => ({
      removableId: `drop-${crypto.randomUUID()}-${index}`,
      name: file.name,
      type: file.type || "application/octet-stream",
      size: file.size,
      previewDataUrl: null,
    }));
  tray.addCards(records);
  // Keep valid files from one physical drop in the tray, but never turn a
  // folder into a tray record. A folder-only drop still leaves the tray empty.
  if (dropped.some((file) => file.isDirectory)) throw new Error(FOLDER_DROP_REJECTION);
  return records;
}

/**
 * Makes `typingBox` a file drop target. Neither listener submits the compose
 * form, so a dropped file remains a tray record until the operator explicitly
 * sends a message.
 */
function droppedFiles(event: DropEvent): DroppedAttachmentFile[] {
  const transfer = event.dataTransfer;
  const files = Array.from(transfer?.files ?? []);
  return files.map((file, index) => ({
    ...file,
    isDirectory: file.isDirectory || transfer?.items?.[index]?.webkitGetAsEntry?.()?.isDirectory === true,
  }));
}

/** Makes `typingBox` a file drop target without submitting the compose form. */
export function bindPrivateTypingBoxDropTarget(
  typingBox: DropTarget,
  tray: AttachmentTrayActions,
  onDropped: () => void = () => undefined,
  onOutlineStateChange: (state: PrivateTypingDropOutlineState) => void = () => undefined,
): void {
  let dragDepth = 0;
  const setOutline = (dragOver: boolean): void => onOutlineStateChange(privateTypingDropOutlineState(dragOver));

  typingBox.addEventListener("dragenter", (rawEvent) => {
    if (!carriesFiles(rawEvent)) return;
    rawEvent.preventDefault();
    dragDepth += 1;
    setOutline(true);
  });
  typingBox.addEventListener("dragover", (rawEvent) => {
    if (!carriesFiles(rawEvent)) return;
    rawEvent.preventDefault();
    if (dragDepth === 0) dragDepth = 1;
    setOutline(true);
  });
  typingBox.addEventListener("dragleave", (rawEvent) => {
    if (dragDepth === 0) return;
    rawEvent.preventDefault();
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) setOutline(false);
  });
  typingBox.addEventListener("drop", (rawEvent) => {
    const event = rawEvent as DropEvent;
    if (!carriesFiles(rawEvent)) return;
    event.preventDefault();
    dragDepth = 0;
    setOutline(false);
    const records = directPrivateTypingDropCommand(droppedFiles(event), tray);
    if (records.length) onDropped();
  });
}
