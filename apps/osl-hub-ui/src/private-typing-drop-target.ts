/**
 * TASK 0628 - direct file drops on a private typing box.
 *
 * A drop is intake only: it creates tray cards and never reaches the explicit
 * message-send command.  Keeping that boundary in this small module makes the
 * browser event path easy to exercise without a WebView or a real filesystem.
 */
import type { AttachmentTrayActions } from "./attachment-tray-actions";
import type { AttachmentTrayCard } from "./attachment-tray-screen";

export interface DroppedAttachmentFile {
  name: string;
  type: string;
  size: number;
}

type DropEvent = Event & { dataTransfer: { files: ArrayLike<DroppedAttachmentFile> } | null };
type DropTarget = Pick<EventTarget, "addEventListener">;

/** Maps files accepted at the private composer directly to tray records. */
export function directPrivateTypingDropCommand(
  files: Iterable<DroppedAttachmentFile>,
  tray: AttachmentTrayActions,
): readonly AttachmentTrayCard[] {
  const records = [...files]
    .filter((file) => file.name.trim().length > 0 && Number.isFinite(file.size) && file.size >= 0)
    .map((file, index) => ({
      removableId: `drop-${crypto.randomUUID()}-${index}`,
      name: file.name,
      type: file.type || "application/octet-stream",
      size: file.size,
      previewDataUrl: null,
    }));
  tray.addCards(records);
  return records;
}

/**
 * Makes `typingBox` a file drop target. Neither listener submits the compose
 * form, so a dropped file remains a tray record until the operator explicitly
 * sends a message.
 */
export function bindPrivateTypingBoxDropTarget(
  typingBox: DropTarget,
  tray: AttachmentTrayActions,
  onDropped: () => void = () => undefined,
): void {
  typingBox.addEventListener("dragover", (rawEvent) => {
    (rawEvent as Event).preventDefault();
  });
  typingBox.addEventListener("drop", (rawEvent) => {
    const event = rawEvent as DropEvent;
    event.preventDefault();
    const records = directPrivateTypingDropCommand(Array.from(event.dataTransfer?.files ?? []), tray);
    if (records.length) onDropped();
  });
}
