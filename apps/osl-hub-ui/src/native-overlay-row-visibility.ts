import type { NativeDiscordCarrierRowBinding } from "./discord-carrier-row-binding";
import type { NativeDiscordOverlayOpened } from "./overlay-state";

export const NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE =
  "OSL could not find the Discord row for this private message. Nothing is shown.";

export interface NativeOverlayVisibleOpenedMessages {
  visibleMessages: NativeDiscordOverlayOpened[];
  missingCoverRows: number;
}

/**
 * Keep decrypted inbound text only when the backend has just proven that the
 * cover row still exists. Older state responses did not carry visible rows, so
 * `undefined` means "no proof available from this backend" rather than deletion.
 */
export function visibleOpenedMessagesForCarrierRows(
  messages: readonly NativeDiscordOverlayOpened[],
  visibleCarrierRows: readonly NativeDiscordCarrierRowBinding[] | undefined,
): NativeOverlayVisibleOpenedMessages {
  if (visibleCarrierRows === undefined) {
    return { visibleMessages: [...messages], missingCoverRows: 0 };
  }
  const visibleMessageIds = new Set(visibleCarrierRows.map((row) => row.messageId));
  const visibleMessages: NativeDiscordOverlayOpened[] = [];
  let missingCoverRows = 0;
  for (const message of messages) {
    if (message.coverPointer === undefined || visibleMessageIds.has(message.messageId)) {
      visibleMessages.push(message);
    } else {
      missingCoverRows += 1;
    }
  }
  return { visibleMessages, missingCoverRows };
}
