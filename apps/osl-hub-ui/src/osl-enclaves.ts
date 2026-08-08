import { oslEnclaveStateMarkup, type OslEnclaveState } from "./osl-enclaves-view";

/**
 * The Enclaves surface is intentionally independent of the application shell.
 * `main.ts` reaches it through the extracted Enclaves view, so this module can
 * grow alongside the Enclave protocol without taking ownership of that file.
 */
export interface OslEnclavesSurfaceModel {
  readonly state?: OslEnclaveState;
  readonly statusTag: (label: string) => string;
}

/** Render the first-party entry point for encrypted OSL Enclaves. */
export function oslEnclavesSurfaceMarkup({ state = {}, statusTag }: OslEnclavesSurfaceModel): string {
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">OSL Enclaves</h1></header><p>Enclaves are OSL's encrypted communities for the members you choose.</p><section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted Enclaves with their own membership.</small></span>${statusTag("Available")}</div></section>${oslEnclaveStateMarkup(state)}<section class="settings-list" aria-label="OSL Enclaves voice boundary"><div class="setting-line"><span><strong>Join voice</strong><small>The relay or SFU sees room admission metadata.</small></span>${statusTag("RELAY")}</div><div class="setting-line"><span><strong>Speak in voice</strong><small>Media stays encrypted; packet timing is still visible to the media path.</small></span>${statusTag("RELAY")}</div><div class="setting-line"><span><strong>Move people</strong><small>Room changes are moderation metadata routed through the relay.</small></span>${statusTag("RELAY")}</div><div class="setting-line"><span><strong>Disconnect people</strong><small>Disconnect actions are moderation metadata routed through the relay.</small></span>${statusTag("RELAY")}</div></section><p class="scope-approval-note">OSL can see who is in a voice room and when. Voice media stays encrypted, but voice is not as private as messages.</p><p class="scope-approval-note">OSL Enclaves are separate from third-party servers. OSL does not claim access to provider communities or read provider pages.</p></main>`;
}
