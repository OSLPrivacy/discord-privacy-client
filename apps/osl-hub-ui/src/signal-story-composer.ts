/**
 * OSL controls that may be displayed alongside Signal's story composer.
 *
 * Signal stories are a separate surface from conversations and note-to-self.
 * Keep the surface receipt deliberately narrow so a disabled Stories setting
 * (or another Signal composer) cannot acquire story controls by accident.
 */
export interface SignalStoryComposerSurface {
  readonly service: string;
  readonly placeKind: string;
  readonly storiesEnabled: boolean;
  /** Fail-closed result returned by the backend for the exact selected audience. */
  readonly selectedAudienceAllowed: boolean;
}

const STORY_CONTROL_NAMES = [
  "lock",
  "private box",
  "count",
  "timer",
  "eye",
  "view once",
  "burn",
] as const;

export type SignalStoryControlName = (typeof STORY_CONTROL_NAMES)[number];

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

/** True only when Signal Stories are enabled for the discovered story surface. */
export function isEnabledSignalStoryComposer(surface: SignalStoryComposerSurface): boolean {
  return surface.service === "signal"
    && surface.placeKind === "story"
    && surface.storiesEnabled
    && surface.selectedAudienceAllowed;
}

/**
 * Render every requested, named Story control for an enabled Signal Story.
 * Returning no markup for every other surface prevents chat, note-to-self,
 * and disabled-Story composers from inheriting Story-only actions.
 */
export function signalStoryComposerMarkup(surface: SignalStoryComposerSurface): string {
  if (!isEnabledSignalStoryComposer(surface)) return "";

  const controls = STORY_CONTROL_NAMES.map(escapeHtml).join("|");
  return `<section class="signal-story-composer-controls" data-signal-story-composer="enabled" data-signal-story-control-names="${controls}" aria-label="Signal story protection controls">`
    + `<button type="button" class="signal-story-control" data-signal-story-control="lock" aria-label="lock">Lock</button>`
    + `<label class="signal-story-control" data-signal-story-control="private box"><input type="checkbox" aria-label="private box"/> <span>Private box</span></label>`
    + `<output class="signal-story-control" data-signal-story-control="count" aria-label="count">Count: 0</output>`
    + `<label class="signal-story-control" data-signal-story-control="timer"><span>Timer</span><select aria-label="timer"><option value="24h">24 hours</option><option value="12h">12 hours</option><option value="1h">1 hour</option></select></label>`
    + `<label class="signal-story-control" data-signal-story-control="eye"><input type="checkbox" aria-label="eye" checked/> <span>Eye</span></label>`
    + `<label class="signal-story-control" data-signal-story-control="view once"><input type="checkbox" aria-label="view once"/> <span>View once</span></label>`
    + `<button type="button" class="signal-story-control" data-signal-story-control="burn" aria-label="burn">Burn</button>`
    + `</section>`;
}

/** Stable names for inspection and accessibility QA of an enabled story. */
export function signalStoryControlNames(
  surface: SignalStoryComposerSurface,
): readonly SignalStoryControlName[] {
  return isEnabledSignalStoryComposer(surface) ? STORY_CONTROL_NAMES : [];
}
