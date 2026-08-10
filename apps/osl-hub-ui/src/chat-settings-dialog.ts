import type { HubPerson } from "./adapters";
import { chatPreviewHidingVisible } from "./entitlement-gates";
import { peerIntegrityMarkup } from "./peer-integrity";
import { escapeHtml } from "./services";
import { oslChatTypingSettingsMarkup, type OslChatTypingPreferences } from "./typing-indicator";

export interface OslChatSettingsState {
  activePersonId: string | null;
  activeScopeApproved: boolean;
  muted: boolean;
  previewsVisible: boolean;
  typingPreferences: OslChatTypingPreferences;
  busy: boolean;
}

export function oslChatFriendSettingsMarkup(person: HubPerson, state: OslChatSettingsState): string {
  const isActive = state.activePersonId === person.personId;
  const approved = isActive && state.activeScopeApproved;
  return `<dialog class="friends-dialog osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="friends-dialog-card"><header><div><span>Encrypted chat</span><h2 id="osl-chat-settings-title">${escapeHtml(person.alias ?? "Verified friend")}</h2></div><button class="icon-button" id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header><div class="settings-list">${peerIntegrityMarkup("unknown")}<label class="setting-line interactive"><span><strong>Mute notifications</strong><small>Messages still arrive without creating a local alert.</small></span><input id="osl-chat-mute-toggle" type="checkbox" ${state.muted ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Message previews</strong><small>Hide previews on this device.</small></span><input id="osl-chat-preview-toggle" type="checkbox" ${chatPreviewHidingVisible(state.previewsVisible) ? "checked" : ""}/></label>${oslChatTypingSettingsMarkup(state.typingPreferences)}<div class="setting-line"><span><strong>Chat permission</strong><small>${approved ? "This friend may exchange encrypted OSL messages with you." : "Open this friend to configure its exact chat permission."}</small></span>${isActive ? `<button class="button compact ${approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${state.busy ? "disabled" : ""}>${approved ? "Revoke" : "Enable"}</button>` : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button">Open chat</button>`}</div></div></div></dialog>`;
}
