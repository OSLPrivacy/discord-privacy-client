import type { HubPerson } from "./adapters";
import { oslChatNotificationSettingsMarkup, type OslChatNotificationSettings } from "./osl-chat-notification-settings";
import { peerIntegrityMarkup } from "./peer-integrity";
import { escapeHtml } from "./services";

export interface OslChatSettingsState {
  isActive: boolean;
  approved: boolean;
  verified: boolean;
  notificationSettings: OslChatNotificationSettings;
  busy: boolean;
}

export function oslChatFriendSettingsMarkup(person: HubPerson, state: OslChatSettingsState): string {
  const permissionDetail = !state.verified
    ? "Not verified. Verify the new safety number before changing this whitelist."
    : state.approved
      ? "This friend may exchange encrypted OSL messages with you."
      : "Open this friend to configure its exact chat permission.";
  const permissionControl = state.isActive
    ? `<button class="button compact ${state.approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${state.busy || !state.verified ? 'disabled aria-disabled="true"' : ""}>${state.approved ? "Revoke" : "Enable"}</button>`
    : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button" ${state.verified ? "" : 'disabled aria-disabled="true"'}>Open chat</button>`;
  return `<dialog class="friends-dialog osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="friends-dialog-card"><header><div><span>Encrypted chat</span><h2 id="osl-chat-settings-title">${escapeHtml(person.alias ?? "Verified friend")}</h2></div><button class="icon-button" id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header><div class="settings-list">${peerIntegrityMarkup("unknown")}<button class="setting-line interactive" data-open-safety-number="${escapeHtml(person.personId)}" type="button"><span><strong>Safety number</strong><small>Compare this number through a channel you already trust.</small></span></button>${oslChatNotificationSettingsMarkup(state.notificationSettings)}<div class="setting-line osl-chat-permission-row${state.verified ? "" : " is-not-verified"}" data-osl-chat-whitelist-state="${state.verified ? "available" : "not-verified"}" ${state.verified ? "" : 'aria-disabled="true"'}><span><strong>Chat permission</strong><small>${permissionDetail}</small></span>${permissionControl}</div></div></div></dialog>`;
}
