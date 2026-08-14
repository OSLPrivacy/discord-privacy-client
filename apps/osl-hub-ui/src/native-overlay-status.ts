import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";

export const PROTECTED_MESSAGE_COULD_NOT_BE_OPENED = "This encrypted message could not be opened";

export function nativeOverlayReceiveStatusText(
  opened: number,
  batch: Pick<NativeDiscordOverlayOpenedBatch, "deferredRows" | "contentGoneRows" | "unrecognizedWireRows" | "decryptDisplayEnabled">,
): string | null {
  if (opened > 0) return `${opened} private ${opened === 1 ? "message" : "messages"} received through OSL.`;
  if (batch.deferredRows > 0) return "OSL could not reach the protected message store. Retrying.";
  if ((batch.contentGoneRows ?? 0) > 0) return PROTECTED_MESSAGE_COULD_NOT_BE_OPENED;
  if (batch.unrecognizedWireRows > 0) return "A protected message needs a newer version of OSL to open.";
  if (!batch.decryptDisplayEnabled) return "Decrypted text is off for this conversation.";
  return null;
}
