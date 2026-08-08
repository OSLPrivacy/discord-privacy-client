/**
 * The direct first-party OSL composers that accept a file drop into their
 * private typing box. Keeping the list with the registration prevents a new
 * composer from quietly missing the local-only intake path.
 */
import type { AttachmentTrayActions } from "./attachment-tray-actions";
import { bindPrivateTypingBoxDropTarget } from "./private-typing-drop-target";

export const OSL_PRIVATE_TYPING_DROP_COMPOSER_VIEWS = [
  { viewId: "osl-chat", composerSelector: "#osl-chat-draft" },
  { viewId: "osl-mail", composerSelector: "#osl-mail-body" },
] as const;

export type OslPrivateTypingDropComposerView = typeof OSL_PRIVATE_TYPING_DROP_COMPOSER_VIEWS[number];

/** A deliberately direct list for capability checks and release evidence. */
export function directPrivateTypingDropComposerViews(): readonly OslPrivateTypingDropComposerView[] {
  return OSL_PRIVATE_TYPING_DROP_COMPOSER_VIEWS;
}

type DropTarget = Pick<EventTarget, "addEventListener">;

/** Register TASK 0628's local-only drop intake on every direct OSL composer. */
export function registerPrivateTypingDropIntake(
  find: (selector: string) => DropTarget | null,
  trayFor: (view: OslPrivateTypingDropComposerView) => AttachmentTrayActions,
  onDropped: () => void,
): void {
  for (const view of directPrivateTypingDropComposerViews()) {
    const target = find(view.composerSelector);
    if (target) bindPrivateTypingBoxDropTarget(target, trayFor(view), onDropped);
  }
}
