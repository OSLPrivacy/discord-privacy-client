import { escapeHtml } from "./services";

export type AutoWhitelistRuleChoiceId = "never" | "ask_me" | "always" | "only_if_a_friend";

export interface AutoWhitelistRuleChoice {
  id: AutoWhitelistRuleChoiceId;
  label: string;
}

export interface AutoWhitelistAppKind {
  appKind: string;
  label: string;
}

export const autoWhitelistRuleChoices: readonly AutoWhitelistRuleChoice[] = [
  { id: "never", label: "never" },
  { id: "ask_me", label: "ask me" },
  { id: "always", label: "always" },
  { id: "only_if_a_friend", label: "only if a friend" },
] as const;

export const configuredAutoWhitelistAppKinds: readonly AutoWhitelistAppKind[] = [
  { appKind: "discord", label: "Discord" },
  { appKind: "telegram", label: "Telegram" },
  { appKind: "signal", label: "Signal" },
  { appKind: "whatsapp", label: "WhatsApp" },
  { appKind: "outlook", label: "Outlook" },
] as const;

export type AutoWhitelistInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export const SAVE_AUTO_WHITELIST_RULE_COMMAND = "osl_save_auto_whitelist_rule";
export const READ_AUTO_WHITELIST_RULE_COMMAND = "osl_read_auto_whitelist_rule";
export const DEFAULT_AUTO_WHITELIST_CHOICE: AutoWhitelistRuleChoiceId = "never";

export function autoRuleChoiceIdFor(raw: string): AutoWhitelistRuleChoiceId | null {
  const normalized = raw.trim().toLowerCase().replace(/[\s-]+/gu, "_");
  const match = autoWhitelistRuleChoices.find((choice) => choice.id === normalized);
  return match ? match.id : null;
}

function ruleDtoChoiceId(dto: unknown, command: string): AutoWhitelistRuleChoiceId {
  const choice = typeof dto === "object" && dto !== null ? (dto as { choice?: unknown }).choice : undefined;
  const id = typeof choice === "string" ? autoRuleChoiceIdFor(choice) : null;
  if (!id) throw new Error(`OSL: ${command} returned an unknown auto-rule choice`);
  return id;
}

export async function saveAutoWhitelistRuleChoice(
  invoke: AutoWhitelistInvoke,
  appKind: string,
  choice: string,
): Promise<AutoWhitelistRuleChoiceId> {
  const requested = autoRuleChoiceIdFor(choice);
  if (!requested) throw new Error(`OSL: unknown auto-rule choice ${JSON.stringify(choice)}`);
  const dto = await invoke(SAVE_AUTO_WHITELIST_RULE_COMMAND, { appKind, choice: requested });
  return ruleDtoChoiceId(dto, SAVE_AUTO_WHITELIST_RULE_COMMAND);
}

export async function loadSavedAutoWhitelistChoices(
  invoke: AutoWhitelistInvoke,
  appKinds: readonly AutoWhitelistAppKind[] = configuredAutoWhitelistAppKinds,
): Promise<Record<string, AutoWhitelistRuleChoiceId>> {
  const saved: Record<string, AutoWhitelistRuleChoiceId> = {};
  for (const app of appKinds) {
    const dto = await invoke(READ_AUTO_WHITELIST_RULE_COMMAND, { appKind: app.appKind });
    const id = ruleDtoChoiceId(dto, READ_AUTO_WHITELIST_RULE_COMMAND);
    // The read command answers with the default for kinds that were never
    // saved, so a default answer cannot count as a saved choice.
    if (id !== DEFAULT_AUTO_WHITELIST_CHOICE) saved[app.appKind] = id;
  }
  return saved;
}

export function whitelistingSettingsMarkup(
  appKinds: readonly AutoWhitelistAppKind[] = configuredAutoWhitelistAppKinds,
  savedChoices: Readonly<Record<string, AutoWhitelistRuleChoiceId>> = {},
): string {
  const rows = appKinds.map((app) => {
    const selected = savedChoices[app.appKind] ?? "never";
    const choices = autoWhitelistRuleChoices.map((choice) => (
      `<button class="auto-rule-choice ${selected === choice.id ? "selected" : ""}" type="button" role="radio" aria-checked="${selected === choice.id}" data-auto-rule-app-kind="${escapeHtml(app.appKind)}" data-auto-rule-choice="${choice.id}">${escapeHtml(choice.label)}</button>`
    )).join("");
    return `<article class="auto-rule-row" data-auto-rule-kind="${escapeHtml(app.appKind)}"><div><strong>${escapeHtml(app.label)}</strong><small>New places in this app</small></div><div class="auto-rule-choices" role="radiogroup" aria-label="Auto-rule for ${escapeHtml(app.label)}">${choices}</div></article>`;
  }).join("");

  return `<details class="saved-account-settings settings-disclosure whitelisting-settings" open><summary>Whitelisting</summary><div class="whitelisting-settings-list">${rows}</div></details>`;
}
