/**
 * The per-friend "Auto-whitelist new accounts" switch on the friend page.
 *
 * The Hub owns the truth (`get_hub_friend_future_account_auto_whitelist` /
 * `set_hub_friend_future_account_auto_whitelist`, TASK 0263). This module only
 * draws the control from a state the caller already holds, so a fixture can
 * render it without a backend.
 */

export type FutureAccountSwitchState = "on" | "off";

export interface FutureAccountSwitchModel {
  personId: string;
  enabled: boolean;
  busy?: boolean;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function futureAccountSwitchState(model: Pick<FutureAccountSwitchModel, "enabled">): FutureAccountSwitchState {
  return model.enabled ? "on" : "off";
}

export function futureAccountSwitchDetail(model: Pick<FutureAccountSwitchModel, "enabled">): string {
  return model.enabled
    ? "A new account this friend adds is approved for the chats you already share."
    : "A new account this friend adds stays unapproved until you approve it.";
}

export function futureAccountSwitchMarkup(model: FutureAccountSwitchModel): string {
  const person = escapeHtml(model.personId);
  const state = futureAccountSwitchState(model);
  return `<label class="setting-line interactive future-account-switch" data-future-account-switch="${person}" data-future-account-switch-state="${state}"><span><strong>Auto-whitelist new accounts</strong><small>${escapeHtml(futureAccountSwitchDetail(model))}</small></span><input type="checkbox" role="switch" data-future-account-toggle="${person}" aria-label="Auto-whitelist new accounts" ${model.enabled ? "checked " : ""}${model.busy ? "disabled" : ""}/></label>`;
}
