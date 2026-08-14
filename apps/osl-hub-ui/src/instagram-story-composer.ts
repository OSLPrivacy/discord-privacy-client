/**
 * The protection controls that sit beside Instagram's desktop story editor.
 *
 * A story created from an uploaded file has a different toolbar from text and
 * camera stories.  Keep this predicate deliberately exact: a stale discovery
 * must not add controls to another Instagram composer (or a mobile surface).
 */
export interface InstagramStoryComposerSurface {
  service: string;
  placeKind: string;
  composerKind: string;
  uploadKind: string;
  viewport: string;
}

const STORY_CONTROL_NAMES = ["lock", "private box", "count", "timer", "eye", "view once"] as const;

export type InstagramStoryControlName = (typeof STORY_CONTROL_NAMES)[number];

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

/** True only for the desktop story toolbar beside one plain uploaded file. */
export function isDesktopPlainUploadedInstagramStory(
  surface: InstagramStoryComposerSurface,
): boolean {
  return surface.service === "instagram"
    && surface.placeKind === "story"
    && surface.composerKind === "plain"
    && surface.uploadKind === "uploaded_file"
    && surface.viewport === "desktop";
}

/**
 * Render the complete, named control set for the supported story composer.
 * Every row contains an interactive control with an accessible name; there
 * are no decorative or unnamed placeholder rows in this toolbar.
 */
export function instagramPlainStoryComposerMarkup(
  surface: InstagramStoryComposerSurface,
): string {
  if (!isDesktopPlainUploadedInstagramStory(surface)) return "";

  const controls = STORY_CONTROL_NAMES.map(escapeHtml).join("|");
  return `<section class="instagram-story-composer-controls" data-instagram-story-composer="plain-uploaded-file" data-instagram-story-control-names="${controls}" aria-label="Instagram story protection controls">`
    + `<button type="button" class="instagram-story-control" data-instagram-story-control="lock" aria-label="lock">Lock</button>`
    + `<label class="instagram-story-control" data-instagram-story-control="private box"><input type="checkbox" aria-label="private box"/> <span>Private box</span></label>`
    + `<output class="instagram-story-control" data-instagram-story-control="count" aria-label="count">Count: 0</output>`
    + `<label class="instagram-story-control" data-instagram-story-control="timer"><span>Timer</span><select aria-label="timer"><option value="24h">24 hours</option><option value="12h">12 hours</option><option value="1h">1 hour</option></select></label>`
    + `<label class="instagram-story-control" data-instagram-story-control="eye"><input type="checkbox" aria-label="eye" checked/> <span>Eye</span></label>`
    + `<label class="instagram-story-control" data-instagram-story-control="view once"><input type="checkbox" aria-label="view once"/> <span>View once</span></label>`
    + `</section>`;
}

/** The six controls as a stable list for inspection and assistive QA. */
export function instagramPlainStoryControlNames(
  surface: InstagramStoryComposerSurface,
): readonly InstagramStoryControlName[] {
  return isDesktopPlainUploadedInstagramStory(surface) ? STORY_CONTROL_NAMES : [];
}
