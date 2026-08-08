/** Connects the generic view-once controls to the one-claim Discord command. */
import { revealNativeDiscordOverlayViewOnce } from "./native-overlay-adapter";
import {
  createViewOnceOverlayController,
  type ViewOnceOverlayController,
  type ViewOnceOverlayControllerOptions,
} from "./view-once-overlay-controller";

export type NativeViewOnceOverlayControllerOptions = Omit<ViewOnceOverlayControllerOptions, "claim">;

/**
 * Play reaches the sole trusted reveal command. Its response carries both the
 * plaintext and the authenticated display duration; no duration is invented by
 * the renderer.
 */
export function createNativeViewOnceOverlayController(
  options: NativeViewOnceOverlayControllerOptions,
): ViewOnceOverlayController {
  return createViewOnceOverlayController({
    ...options,
    claim: async (messageId) => {
      const opened = await revealNativeDiscordOverlayViewOnce(messageId);
      if (!opened || !opened.viewOnceConsumed || opened.displayDurationSeconds === undefined) return null;
      return {
        messageId: opened.messageId,
        content: { kind: "text", text: opened.plaintext },
        displayDurationSeconds: opened.displayDurationSeconds,
      };
    },
  });
}
