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

export const GET_NEW_FRIEND_DEFAULTS_COMMAND = "cmd_osl_get_new_friend_defaults";
export const SAVE_NEW_FRIEND_DEFAULTS_COMMAND = "cmd_osl_save_new_friend_defaults";

export type NewFriendDefaultsCommand =
  | typeof GET_NEW_FRIEND_DEFAULTS_COMMAND
  | typeof SAVE_NEW_FRIEND_DEFAULTS_COMMAND;

/** The serde field names on ipc::commands::NewFriendDefaultsDto. */
export interface NewFriendDefaultsDto {
  account_reach: string;
  auto_whitelist: string;
  verification_warnings: string;
}

export interface NewFriendDefaultsCommandPort {
  invoke(
    command: NewFriendDefaultsCommand,
    args?: { defaults: NewFriendDefaultsDto },
  ): Promise<unknown>;
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

function normalizedChoice(value: unknown): string {
  return typeof value === "string"
    ? value.trim().toLowerCase().replace(/[ -]/gu, "_")
    : "";
}

/**
 * The Rust command echoes human-readable auto-whitelist labels (for example,
 * `only if a friend`) while the radio values use stable ids. Normalize both
 * forms here so a save response and the following read draw the same choice.
 */
export function newFriendDefaultModelFromDto(raw: unknown): NewFriendDefaultControlsModel | null {
  if (typeof raw !== "object" || raw === null) return null;
  const dto = raw as Partial<NewFriendDefaultsDto>;
  const accountReach = normalizedChoice(dto.account_reach);
  const autoWhitelist = normalizedChoice(dto.auto_whitelist);
  const verificationWarnings = normalizedChoice(dto.verification_warnings);
  if (accountReach !== "approved_chats_only" && accountReach !== "all_shared_chats") return null;
  if (!["never", "ask_me", "always", "only_if_a_friend"].includes(autoWhitelist)) return null;
  if (verificationWarnings !== "always" && verificationWarnings !== "never") return null;
  return {
    accountReach,
    autoWhitelist: autoWhitelist as NewFriendAutoWhitelist,
    verificationWarnings,
    busy: false,
  };
}

export function newFriendDefaultsDtoFromControls(root: ParentNode): NewFriendDefaultsDto | null {
  const checked = (control: string): string =>
    root.querySelector<HTMLInputElement>(
      `[data-new-friend-control="${control}"] input[type="radio"]:checked`,
    )?.value ?? "";
  const model = newFriendDefaultModelFromDto({
    account_reach: checked("account-reach"),
    auto_whitelist: checked("auto-whitelist"),
    verification_warnings: checked("verification-warnings"),
  });
  return model ? {
    account_reach: model.accountReach,
    auto_whitelist: model.autoWhitelist,
    verification_warnings: model.verificationWarnings,
  } : null;
}

export async function loadNewFriendDefaults(
  port: NewFriendDefaultsCommandPort,
): Promise<NewFriendDefaultControlsModel | null> {
  return newFriendDefaultModelFromDto(await port.invoke(GET_NEW_FRIEND_DEFAULTS_COMMAND));
}

export async function saveNewFriendDefaults(
  root: ParentNode,
  port: NewFriendDefaultsCommandPort,
): Promise<NewFriendDefaultControlsModel | null> {
  const defaults = newFriendDefaultsDtoFromControls(root);
  if (!defaults) return null;
  const saved = await port.invoke(SAVE_NEW_FRIEND_DEFAULTS_COMMAND, { defaults });
  return newFriendDefaultModelFromDto(saved);
}

/**
 * Connect the gate-0252 button to the actual defaults write. Only a response
 * containing all three saved choices is reported as success. While the write
 * is in flight, every control is disabled so a later click cannot be mistaken
 * for the values that were sent.
 */
export function connectNewFriendDefaultControls(
  root: ParentNode,
  port: NewFriendDefaultsCommandPort,
  onSaved: (saved: NewFriendDefaultControlsModel) => void = () => undefined,
): () => void {
  const save = root.querySelector<HTMLButtonElement>('[data-new-friend-action="save-defaults"]');
  if (!save) return () => undefined;
  const fields = [...root.querySelectorAll<HTMLInputElement>("input[data-new-friend-choice]")];
  const handleSave = async (): Promise<void> => {
    save.disabled = true;
    fields.forEach((field) => { field.disabled = true; });
    try {
      const saved = await saveNewFriendDefaults(root, port);
      if (saved) onSaved(saved);
    } finally {
      save.disabled = false;
      fields.forEach((field) => { field.disabled = false; });
    }
  };
  const listener = (): void => { void handleSave(); };
  save.addEventListener("click", listener);
  return () => save.removeEventListener("click", listener);
}
