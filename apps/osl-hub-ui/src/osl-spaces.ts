import { oslSpaceStateMarkup, type OslSpaceState } from "./osl-spaces-view";

/**
 * The Spaces surface is intentionally independent of the application shell.
 * `main.ts` reaches it through the extracted Enclaves view, so this module can
 * grow alongside the Space protocol without taking ownership of that file.
 */
export interface OslSpacesSurfaceModel {
  readonly state?: OslSpaceState;
  readonly statusTag: (label: string) => string;
}

/** Render the first-party entry point for encrypted OSL Spaces. */
export function oslSpacesSurfaceMarkup({ state = {}, statusTag }: OslSpacesSurfaceModel): string {
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">OSL Enclaves</h1></header><p>Enclaves are OSL's encrypted shared spaces for the members you choose.</p><section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted spaces with their own membership.</small></span>${statusTag("Available")}</div></section>${oslSpaceStateMarkup(state)}<p class="scope-approval-note">OSL Enclaves are separate from third-party servers. OSL does not claim access to provider communities or read provider pages.</p></main>`;
}
