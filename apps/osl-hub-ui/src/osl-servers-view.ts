import { oslSpaceStateMarkup, type OslSpaceState } from "./osl-spaces-view";

/** The shipping destination for OSL's Discord-equivalent encrypted servers. */
export function oslServersViewMarkup(statusTag: (label: string) => string): string {
  const state: OslSpaceState = {};
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">OSL Enclaves</h1></header><p>Enclaves are OSL's encrypted shared spaces for the members you choose.</p><section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted spaces with their own membership.</small></span>${statusTag("Available")}</div></section>${oslSpaceStateMarkup(state)}<p class="scope-approval-note">OSL Enclaves are separate from third-party servers. OSL does not claim access to provider communities or read provider pages.</p></main>`;
}
