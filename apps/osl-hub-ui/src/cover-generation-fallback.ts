import { honestStateTone } from "./honest-state";
import type { CoverGenerationPresentationState } from "./cover-generation-progress";

/**
 * Explains when the send path changes from generated cover text to the
 * always-available word-bank carrier. This is deliberately separate from the
 * progress surface: a terminal generation outcome must remain visible after
 * the bar has gone away.
 */
export function renderCoverGenerationFallback(
  state: CoverGenerationPresentationState,
): string | null {
  switch (state.status) {
    case "fell-back":
      return fallbackNotice("Cover text took too long");
    case "failed":
      return fallbackNotice("Cover text generation failed");
    case "idle":
    case "generating":
      return null;
  }
}

function fallbackNotice(reason: string): string {
  const tone = honestStateTone("unknown");
  return `<p class="cover-generation-fallback" data-cover-generation-fallback="word-bank" data-honest-tone="${tone}" role="status">${reason}. Your message was sent using the word-bank carrier instead.</p>`;
}
