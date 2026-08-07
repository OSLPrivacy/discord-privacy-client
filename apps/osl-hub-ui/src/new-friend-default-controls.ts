export type NewFriendAccountReach = "approved_chats_only" | "all_shared_chats";
export type NewFriendAutoWhitelist = "never" | "ask_me" | "always" | "only_if_a_friend";
export type NewFriendVerificationWarnings = "always" | "never";

export interface NewFriendDefaultControlsModel {
  accountReach: NewFriendAccountReach;
  autoWhitelist: NewFriendAutoWhitelist;
  verificationWarnings: NewFriendVerificationWarnings;
  busy: boolean;
  idPrefix?: string;
}

interface ControlChoice {
  value: string;
  label: string;
}

interface ControlSpec {
  control: "account-reach" | "auto-whitelist" | "verification-warnings";
  legend: string;
  hint: string;
  choices: readonly ControlChoice[];
}

// Values must round-trip through cmd_osl_save_new_friend_defaults, whose
// parsers normalize spaces and hyphens to underscores before matching.
const CONTROLS: readonly ControlSpec[] = [
  {
    control: "account-reach",
    legend: "New friend can reach",
    hint: "Where a newly accepted friend can message this account.",
    choices: [
      { value: "approved_chats_only", label: "Approved chats only" },
      { value: "all_shared_chats", label: "All shared chats" },
    ],
  },
  {
    control: "auto-whitelist",
    legend: "Auto-whitelist new friends",
    hint: "Whether a newly accepted friend is whitelisted without asking.",
    choices: [
      { value: "never", label: "Never" },
      { value: "ask_me", label: "Ask me" },
      { value: "always", label: "Always" },
      { value: "only_if_a_friend", label: "Only if a friend" },
    ],
  },
  {
    control: "verification-warnings",
    legend: "Verification warnings",
    hint: "Warn before messaging a new friend whose keys are unverified.",
    choices: [
      { value: "always", label: "Always warn" },
      { value: "never", label: "Never warn" },
    ],
  },
];

function escapeAttribute(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function selectedValue(model: NewFriendDefaultControlsModel, control: ControlSpec["control"]): string {
  switch (control) {
    case "account-reach":
      return model.accountReach;
    case "auto-whitelist":
      return model.autoWhitelist;
    case "verification-warnings":
      return model.verificationWarnings;
  }
}

function controlMarkup(model: NewFriendDefaultControlsModel, spec: ControlSpec, idPrefix: string): string {
  const selected = selectedValue(model, spec.control);
  const rows = spec.choices.map((choice) => {
    const inputId = `${idPrefix}-${spec.control}-${choice.value}`;
    const checked = choice.value === selected;
    return `<label class="new-friend-default-choice" for="${escapeAttribute(inputId)}"><input type="radio" id="${escapeAttribute(inputId)}" name="${escapeAttribute(`${idPrefix}-${spec.control}`)}" value="${escapeAttribute(choice.value)}" data-new-friend-choice="${escapeAttribute(choice.value)}" ${checked ? "checked " : ""}${model.busy ? "disabled " : ""}/><span>${choice.label}</span></label>`;
  }).join("");
  return `<fieldset class="new-friend-default-control" data-new-friend-control="${spec.control}"><legend>${spec.legend}</legend><p class="new-friend-default-hint">${spec.hint}</p>${rows}</fieldset>`;
}

export function newFriendDefaultControlsMarkup(model: NewFriendDefaultControlsModel): string {
  const idPrefix = model.idPrefix ?? "new-friend-default";
  const controls = CONTROLS.map((spec) => controlMarkup(model, spec, idPrefix)).join("");
  const save = `<button class="new-friend-default-save" id="${escapeAttribute(`${idPrefix}-save`)}" type="button" data-new-friend-action="save-defaults" ${model.busy ? "disabled " : ""}>Save default</button>`;
  return `<section class="new-friend-default-controls" data-new-friend-defaults aria-label="New friend defaults">${controls}${save}</section>`;
}

export function newFriendDefaultControlsFixtureMarkup(): string {
  return `<section class="task-0252-new-friend-defaults-fixture" data-ui-fixture="task-0252-new-friend-default-controls" aria-label="Task 0252 new friend default controls">${newFriendDefaultControlsMarkup({
    accountReach: "approved_chats_only",
    autoWhitelist: "never",
    verificationWarnings: "always",
    busy: false,
    idPrefix: "task-0252-new-friend",
  })}</section>`;
}
