import { inDomTooltipMarkup } from "./in-dom-tooltip";

export type DiscordQaWhitelistButtonState = "off-list" | "on-list";

export interface DiscordQaWhitelistButtonModel {
  scopeApproved: boolean;
  protectionActive: boolean;
  verifiedPeer: boolean;
  busy: boolean;
  id?: string;
}

function escapeAttribute(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

export function discordQaWhitelistButtonState(scopeApproved: boolean): DiscordQaWhitelistButtonState {
  return scopeApproved ? "on-list" : "off-list";
}

export function discordQaWhitelistButtonMarkup(model: DiscordQaWhitelistButtonModel): string {
  const state = discordQaWhitelistButtonState(model.scopeApproved);
  const next = model.scopeApproved ? "revoke" : "allow";
  const label = model.scopeApproved ? "On list" : "Off list";
  const action = model.scopeApproved
    ? "Revoke this verified peer scope"
    : "Allow this verified peer scope";
  const disabled = !model.protectionActive || !model.verifiedPeer || model.busy;
  const disabledReason = !model.protectionActive
    ? "Open the protected composer before changing the whitelist"
    : !model.verifiedPeer
      ? "Whitelist changes need this verified friend's protected chat"
      : model.busy
        ? "Whitelist change is in progress"
        : action;
  const id = escapeAttribute(model.id ?? "discord-qa-whitelist-toggle");
  const ariaLabel = `${label} - ${action}`;

  return `<button class="discord-qa-whitelist-toggle ${state} in-dom-tooltip-anchor" id="${id}" type="button" aria-label="${escapeAttribute(ariaLabel)}" aria-pressed="${model.scopeApproved}" data-whitelist-button="single-place" data-whitelist-state="${state}" data-whitelist-next="${next}" ${disabled ? "disabled " : ""}><span class="discord-qa-whitelist-dot" aria-hidden="true"></span><span class="discord-qa-whitelist-label">${label}</span>${inDomTooltipMarkup(disabled ? disabledReason : action)}</button>`;
}

export function discordQaWhitelistButtonFixtureMarkup(): string {
  return `<section class="task-0109-whitelist-button-fixture" data-ui-fixture="task-0109-whitelist-button" aria-label="Task 0109 whitelist button states"><div class="native-discord-header-controls discord-qa-header-controls"><div class="discord-qa-header-right"><span class="task-0109-conversation-control" aria-hidden="true">Proof</span>${discordQaWhitelistButtonMarkup({ id: "task-0109-whitelist-off-list", scopeApproved: false, protectionActive: true, verifiedPeer: true, busy: false })}</div><div class="discord-qa-header-right"><span class="task-0109-conversation-control" aria-hidden="true">Lock</span>${discordQaWhitelistButtonMarkup({ id: "task-0109-whitelist-on-list", scopeApproved: true, protectionActive: true, verifiedPeer: true, busy: false })}</div></div></section>`;
}
