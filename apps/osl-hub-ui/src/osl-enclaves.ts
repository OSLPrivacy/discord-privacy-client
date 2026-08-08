import { oslEnclaveStateMarkup, type OslEnclaveState } from "./osl-enclaves-view";
import { oslEnclavesFiveRegionSurface } from "./osl-enclaves-surface";
import { serverMemberPermissionsMarkup } from "./server-channel-sidebar";
import { oslEnclaveChannelListMarkup, type EnclaveChannelList } from "./osl-enclave-channel-list";
import {
  enclaveChannelPermissionEditorMarkup,
  type EnclaveChannelPermissionEditorSnapshot,
} from "./osl-enclave-channel-permissions";

/**
 * The Enclaves surface is intentionally independent of the application shell.
 * `main.ts` reaches it through the extracted Enclaves view, so this module can
 * grow alongside the Enclave protocol without taking ownership of that file.
 */
export interface OslEnclavesSurfaceModel {
  readonly state?: OslEnclaveState;
  readonly channelList?: EnclaveChannelList;
  readonly statusTag: (label: string) => string;
  readonly roleEditorMarkup?: string;
  readonly channelPermissions?: EnclaveChannelPermissionEditorSnapshot;
}

/**
 * Render the first-party entry point for encrypted OSL Enclaves: the
 * five-region community surface (app rail, enclave rail, channel sidebar,
 * channel pane, member list — see osl-enclaves-surface.ts). The honest
 * transport notices from `oslEnclaveStateMarkup` render at the top of the
 * channel pane, and the capability/honesty sheet lives on as the surface's
 * About subpage rather than being the whole screen.
 */
export function oslEnclavesSurfaceMarkup({ state = {}, channelList, statusTag, roleEditorMarkup = "", channelPermissions }: OslEnclavesSurfaceModel): string {
  const channelPermissionEditor = channelPermissions
    ? enclaveChannelPermissionEditorMarkup(channelPermissions)
    : "";
  return oslEnclavesFiveRegionSurface({
    stateNotices: oslEnclaveStateMarkup(state),
    statusTag,
    supplementaryChannelListMarkup: `${channelList ? oslEnclaveChannelListMarkup(channelList) : ""}${channelPermissionEditor}`,
    roleEditorMarkup: `${serverMemberPermissionsMarkup()}${roleEditorMarkup}`,
  });
}
