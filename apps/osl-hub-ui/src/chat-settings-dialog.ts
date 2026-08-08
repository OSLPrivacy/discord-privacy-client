import "./chat-settings-dialog.css";
import type { HubPerson } from "./adapters";
import { onOffToggle } from "./onboarding-controls";
import { oslChatNotificationSettingsMarkup, type OslChatNotificationSettings } from "./osl-chat-notification-settings";
import { peerIntegrityMarkup } from "./peer-integrity";
import { escapeHtml } from "./services";
import { oslChatTypingSettingsMarkup, type OslChatTypingPreferences } from "./typing-indicator";

export interface OslChatSettingsState {
  isActive: boolean;
  approved: boolean;
  verified: boolean;
  notificationSettings: OslChatNotificationSettings;
  typingPreferences?: OslChatTypingPreferences;
  busy: boolean;
}

function settingToggle(id: string, on: boolean, label: string, disabled = false): string {
  const toggle = onOffToggle(id, on, label);
  return disabled ? toggle.replace("<input ", "<input disabled ") : toggle;
}

function toggleRow(name: string, description: string, id: string, on: boolean): string {
  return `<label class="osl-chat-settings-row"><span><strong>${name}</strong>${description ? `<small>${description}</small>` : ""}</span>${settingToggle(id, on, name)}</label>`;
}

function valueRow(name: string, description: string, value: string, tone = ""): string {
  return `<button class="osl-chat-settings-row" type="button"><span><strong>${name}</strong>${description ? `<small>${description}</small>` : ""}</span>${value ? `<b class="${tone}">${value}</b>` : ""}</button>`;
}

function autoDeleteRow(): string {
  return `<fieldset class="osl-chat-settings-row osl-chat-auto-delete"><legend class="sr-only">Auto-delete</legend><span><strong>Auto-delete</strong><small>OSL deletes its copy and stops decrypting. It cannot stop a screenshot.</small></span><input class="sr-only" type="radio" name="osl-chat-auto-delete" id="osl-chat-auto-delete-off"/><input class="sr-only" type="radio" name="osl-chat-auto-delete" id="osl-chat-auto-delete-1h"/><input class="sr-only" type="radio" name="osl-chat-auto-delete" id="osl-chat-auto-delete-24h" checked/><input class="sr-only" type="radio" name="osl-chat-auto-delete" id="osl-chat-auto-delete-7d"/><label class="osl-chat-timer-value is-off" for="osl-chat-auto-delete-1h" title="Set auto-delete to 1 hour">Off</label><label class="osl-chat-timer-value is-1h" for="osl-chat-auto-delete-24h" title="Set auto-delete to 24 hours">1H</label><label class="osl-chat-timer-value is-24h" for="osl-chat-auto-delete-7d" title="Set auto-delete to 7 days">24H</label><label class="osl-chat-timer-value is-7d" for="osl-chat-auto-delete-off" title="Turn auto-delete off">7D</label></fieldset>`;
}

export function oslChatFriendSettingsMarkup(person: HubPerson, state: OslChatSettingsState): string {
  const name = person.alias ?? "Verified friend";
  const safetyState = person.pendingKeyChange ? "Key changed · verify again" : state.verified ? "Verified · online" : "Not verified";
  const permissionDetail = !state.verified
    ? "Not verified. Verify the new safety number before changing this whitelist."
    : state.approved
      ? "This friend may exchange encrypted OSL messages with you."
      : "Open this friend to configure its exact chat permission.";
  const permissionControl = state.isActive
    ? `<button class="button compact ${state.approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${state.busy || !state.verified ? 'disabled aria-disabled="true"' : ""}>${state.approved ? "Revoke" : "Enable"}</button>`
    : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button" ${state.verified ? "" : 'disabled aria-disabled="true"'}>Open chat</button>`;
  return `<dialog class="osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="osl-chat-settings-card">
    <header class="osl-chat-settings-top"><span class="machine-fact">Chat settings</span><button id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header>
    <section class="osl-chat-settings-profile"><span class="osl-chat-settings-avatar" aria-hidden="true">${escapeHtml(name.slice(0, 1).toLocaleUpperCase())}</span><strong id="osl-chat-settings-title">${escapeHtml(name)}</strong><span class="machine-fact ${state.verified ? "is-safe" : "is-warning"}">${escapeHtml(safetyState)}</span></section>
    <nav class="osl-chat-settings-actions" aria-label="Chat actions"><button type="button" title="Mute notifications"><b aria-hidden="true">●</b><span>Mute</span></button><button type="button" title="Search this chat"><b aria-hidden="true">⌕</b><span>Search</span></button><button type="button" title="Chat media"><b aria-hidden="true">▦</b><span>Media</span></button><button data-open-safety-number="${escapeHtml(person.personId)}" type="button" title="Verify safety number"><b aria-hidden="true">△</b><span>Proof</span></button></nav>
    <div class="osl-chat-settings-groups">
      <section><h3>Message lifetime</h3>${autoDeleteRow()}${toggleRow("View once by default", "New messages open once, then close", "osl-chat-view-once-default-toggle", false)}${toggleRow("Cover text", "Show harmless text until you hold the eye", "osl-chat-cover-text-toggle", true)}<div class="osl-chat-settings-row osl-chat-permission-row${state.verified ? "" : " is-disabled"}" data-osl-chat-whitelist-state="${state.verified ? "available" : "not-verified"}" ${state.verified ? "" : 'aria-disabled="true"'}><span><strong>Whitelist</strong><small>${permissionDetail}</small></span>${permissionControl}</div></section>
      ${oslChatNotificationSettingsMarkup(state.notificationSettings)}${oslChatTypingSettingsMarkup(state.typingPreferences ?? { hideOwnTyping: false, showIncomingTyping: true })}
      <section><h3>Encryption</h3>${peerIntegrityMarkup("unknown")}<button class="osl-chat-settings-row" data-open-safety-number="${escapeHtml(person.personId)}" type="button"><span><strong>Safety number</strong><small>Compare all 60 digits in groups of five.</small></span><b>View</b></button>${valueRow("Linked devices", "Review the devices receiving this conversation", "View")}${valueRow("Encryption", "", "OSL-X25519", "is-cipher")}</section>
      <section class="is-danger"><h3>Danger</h3>${valueRow("Clear history", "Removes OSL's copy on this device only", "")}${valueRow(`Block ${escapeHtml(name)}`, "", "")}${valueRow("Burn this chat", "", "Small", "is-danger-value")}</section>
    </div>
  </div></dialog>`;
}
