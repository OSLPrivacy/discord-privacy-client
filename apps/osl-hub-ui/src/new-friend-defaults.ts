import "./new-friend-defaults.css";

/**
 * The New friend defaults screen.
 *
 * TASK 0732. The three choices here are the three fields TASK 0249 already
 * persists in `ipc::app_preferences::NewFriendDefaults` and applies at the
 * moment a pending request becomes an accepted friendship:
 *
 *   accounts      -> NewFriendAccountReach          (approved_chats_only | all_shared_chats)
 *   conversations -> AutoWhitelistChoice            (never | ask_me | always | only_if_a_friend)
 *   checkmark     -> NewFriendVerificationWarnings  (always | never, the wire labels
 *                                                    `new_friend_warning_label` emits)
 *
 * The stored values are the backend's own wire strings, not screen-only ids, so
 * connecting this screen to `cmd_osl_save_new_friend_defaults` later is a
 * hand-off of the same object rather than a translation layer. No tauri command
 * for those two calls is registered yet (see apps/osl-hub/src/hub_command_surface.rs),
 * so Save default writes to this device only.
 */

export const NEW_FRIEND_DEFAULTS_TITLE = "New friend defaults";

/**
 * Said once, at the top, in the words a person would use. The whole risk of a
 * defaults screen is that somebody reads "new friend" as "my friends" and
 * believes the radio they just moved reached back through the people they
 * already trust. It does not, and the screen has to say so before the choices,
 * not after them.
 */
export const NEW_FRIEND_DEFAULTS_LEAD =
  "These choices apply to friends you add from now on. Friends you have already added do not change.";

export type NewFriendAccountReach = "approved_chats_only" | "all_shared_chats";
export type NewFriendConversationRule = "never" | "ask_me" | "always" | "only_if_a_friend";
export type NewFriendCheckmarkWarning = "always" | "never";

export interface NewFriendDefaultChoices {
  accountReach: NewFriendAccountReach;
  conversationRule: NewFriendConversationRule;
  checkmarkWarning: NewFriendCheckmarkWarning;
}

/** The same starting point `NewFriendDefaults::default()` uses in Rust. */
export const initialNewFriendDefaults = (): NewFriendDefaultChoices => ({
  accountReach: "approved_chats_only",
  conversationRule: "never",
  checkmarkWarning: "always",
});

interface ChoiceOption {
  value: string;
  label: string;
  detail: string;
}

interface ChoiceGroup {
  /** The accessible name the screenshot check looks for. */
  name: "accounts" | "conversations" | "checkmark";
  field: keyof NewFriendDefaultChoices;
  legend: string;
  options: readonly ChoiceOption[];
}

export const NEW_FRIEND_DEFAULT_GROUPS: readonly ChoiceGroup[] = [
  {
    name: "accounts",
    field: "accountReach",
    legend: "Which of your accounts a new friend can reach",
    options: [
      { value: "approved_chats_only", label: "Approved chats only", detail: "A new friend reaches you only in the chats you approve." },
      { value: "all_shared_chats", label: "All shared chats", detail: "A new friend reaches every account you already share with them." },
    ],
  },
  {
    name: "conversations",
    field: "conversationRule",
    legend: "Which conversations a new friend joins",
    options: [
      { value: "never", label: "Never", detail: "New conversations stay off the whitelist until you add them yourself." },
      { value: "ask_me", label: "Ask me", detail: "OSL asks first, every time a new conversation appears." },
      { value: "always", label: "Always", detail: "Every new conversation with that friend joins the whitelist." },
      { value: "only_if_a_friend", label: "Only if a friend", detail: "A new conversation joins only when everyone in it is already a friend." },
    ],
  },
  {
    name: "checkmark",
    field: "checkmarkWarning",
    legend: "The verified checkmark warning",
    options: [
      { value: "always", label: "Always warn", detail: "Keep warning until you have checked the new friend's checkmark." },
      { value: "never", label: "Never warn", detail: "Do not warn about an unchecked checkmark for friends added from now on." },
    ],
  },
] as const;

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) =>
    character === "&" ? "&amp;"
      : character === "<" ? "&lt;"
      : character === ">" ? "&gt;"
      : character === '"' ? "&quot;"
      : "&#39;");
}

function optionMarkup(group: ChoiceGroup, option: ChoiceOption, selected: boolean): string {
  const id = `nfd-${group.name}-${option.value.replace(/_/gu, "-")}`;
  return `<label class="nfd-option ${selected ? "selected" : ""}" for="${id}"><input id="${id}" type="radio" name="nfd-${group.name}" value="${escapeHtml(option.value)}" data-new-friend-default="${group.name}" ${selected ? "checked" : ""}/><span class="nfd-option-text"><strong>${escapeHtml(option.label)}</strong><small>${escapeHtml(option.detail)}</small></span></label>`;
}

function groupMarkup(group: ChoiceGroup, choices: NewFriendDefaultChoices): string {
  const current = choices[group.field] as string;
  const options = group.options.map((option) => optionMarkup(group, option, option.value === current)).join("");
  return `<fieldset class="nfd-group" aria-label="${group.name}" data-new-friend-group="${group.name}"><legend class="nfd-legend">${escapeHtml(group.legend)}</legend><div class="nfd-options">${options}</div></fieldset>`;
}

/**
 * `saved` is what is on disk; `choices` is what the screen is showing. Save
 * default and Reset are always present and always operable -- a Reset that
 * disappears once the screen matches the saved state is a control the
 * screenshot check cannot find and a person cannot rely on.
 */
export function newFriendDefaultsMarkup(
  choices: NewFriendDefaultChoices,
  saved: NewFriendDefaultChoices = choices,
): string {
  const unsaved = (Object.keys(choices) as Array<keyof NewFriendDefaultChoices>).some((key) => choices[key] !== saved[key]);
  const status = unsaved ? "Not saved yet" : "Saved on this device";
  return `<section class="new-friend-defaults" aria-label="${NEW_FRIEND_DEFAULTS_TITLE}"><h2 class="nfd-title">${NEW_FRIEND_DEFAULTS_TITLE}</h2><p class="nfd-lead">${NEW_FRIEND_DEFAULTS_LEAD}</p><div class="nfd-groups">${NEW_FRIEND_DEFAULT_GROUPS.map((group) => groupMarkup(group, choices)).join("")}</div><div class="nfd-actions"><button class="button primary" id="save-new-friend-default" type="button">Save default</button><button class="button" id="reset-new-friend-default" type="button">Reset</button><span class="nfd-status" data-new-friend-status="${unsaved ? "unsaved" : "saved"}">${status}</span></div></section>`;
}
