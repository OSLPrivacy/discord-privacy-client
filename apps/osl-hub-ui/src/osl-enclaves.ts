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
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">OSL Enclaves</h1></header><p>Enclaves are OSL's encrypted communities for the members you choose.</p><section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted Enclaves with their own membership.</small></span>${statusTag("Available")}</div></section>${oslEnclaveStateMarkup(state)}<p class="scope-approval-note">OSL Enclaves are separate from third-party servers. OSL does not claim access to provider communities or read provider pages.</p></main>`;
}
