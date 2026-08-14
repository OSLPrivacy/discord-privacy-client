import { oslEnclaveStateMarkup, type OslEnclaveState } from "./osl-enclaves-view";
import {
  bindEnclaveSidebarContextMenus,
  renderEnclaveSidebar,
  type EnclaveSidebarContextBinding,
  type EnclaveSidebarContextBindingOptions,
  type EnclaveSidebarController,
  type EnclaveSidebarEntry,
} from "./osl-enclave-sidebar";
import { renderEnclaveLayout, renderMeasuredLimit, type EnclaveLayoutModel, type MeasuredLayoutLimit } from "./osl-enclave-layout";

/**
 * The Enclaves surface is intentionally independent of the application shell.
 * `main.ts` reaches it through the extracted Enclaves view, so this module can
 * grow alongside the Enclave protocol without taking ownership of that file.
 */
export interface OslEnclavesSurfaceModel {
  readonly state?: OslEnclaveState;
  readonly statusTag: (label: string) => string;
  readonly layout?: EnclaveLayoutModel;
  readonly collapsedCategories?: ReadonlySet<string>;
  readonly measuredLimit?: MeasuredLayoutLimit;
}

/** Render the first-party entry point for encrypted OSL Enclaves. */
export function oslEnclavesSurfaceMarkup({ state = {}, statusTag, layout, collapsedCategories, measuredLimit }: OslEnclavesSurfaceModel): string {
  const layoutMarkup = layout ? renderEnclaveLayout(layout, collapsedCategories ?? new Set()) : "";
  const limitMarkup = measuredLimit ? renderMeasuredLimit(measuredLimit) : "";
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">OSL Enclaves</h1></header><p>Enclaves are OSL's encrypted communities for the members you choose.</p><section class="osl-enclave-sidebar-mount" data-osl-enclave-sidebar aria-label="Personal Enclave sidebar"></section><section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line"><span><strong>Enclaves</strong><small>Create and use encrypted Enclaves with their own membership.</small></span>${statusTag("Available")}</div></section>${layoutMarkup}${limitMarkup}${oslEnclaveStateMarkup(state)}<p class="scope-approval-note">OSL Enclaves are separate from third-party platforms. OSL does not claim access to provider communities or read provider pages.</p></main>`;
}

/** Mount the stateful sidebar into the shipping Enclaves surface. */
export function mountOslEnclaveSidebar(
  surface: ParentNode,
  entries: readonly EnclaveSidebarEntry[],
  controller: EnclaveSidebarController,
  options: EnclaveSidebarContextBindingOptions = {},
): EnclaveSidebarContextBinding {
  const mount = surface.querySelector<HTMLElement>("[data-osl-enclave-sidebar]");
  if (!mount) throw new Error("OSL Enclave sidebar mount is missing");
  const sidebar = renderEnclaveSidebar(mount.ownerDocument, entries, controller);
  mount.replaceChildren(sidebar);
  return bindEnclaveSidebarContextMenus(sidebar, controller, options);
}
