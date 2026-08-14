import {
  OslProfilePaneState,
  PROFILE_PANE_FOOTER,
  scopeLabel,
  scopeStorageKey,
  type ScopedProfileFieldName,
  type ScopedProfileRecord,
} from "./osl-profile-pane";

/** The non-profile destinations shown in the lower half of the left rail. */
export const CHAT_APPEARANCE_ITEMS = ["Chat background", "Messages"] as const;

export const CHAT_PROFILE_STATUS_CHOICES = [
  { value: "Online", label: "Online", tone: "online" },
  { value: "Away", label: "Away", tone: "away" },
  { value: "Busy", label: "Busy", tone: "busy" },
  { value: "Invisible", label: "Invisible", tone: "invisible" },
] as const;

export const CHAT_PROFILE_CARD_BACKGROUNDS = [
  "#171a21",
  "#20283a",
  "#28333f",
  "#302943",
  "#3a282d",
] as const;

export const CHAT_PROFILE_COLOURS = [
  "#06b6d4",
  "#5c8dff",
  "#8b7cff",
  "#cf70d9",
  "#ef626b",
  "#f2b84b",
  "#49c58a",
] as const;

export const CHAT_PROFILE_HEX_ERROR = "Enter a six-digit hex colour, such as #06b6d4.";

export interface ChatProfileAppearanceModalHandlers {
  onClose?: () => void;
  /** Receives fresh clones after one exact scoped record changes. */
  onRecordsChange?: (records: ScopedProfileRecord[]) => void;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function recordClones(state: OslProfilePaneState): ScopedProfileRecord[] {
  return state.rows().map((record) => ({ ...record, scope: { ...record.scope } }));
}

function checked(value: boolean): string {
  return value ? " checked" : "";
}

function selected(value: boolean): string {
  return value ? " selected" : "";
}

function disabled(value: boolean): string {
  return value ? " disabled" : "";
}

function profileNavRow(record: ScopedProfileRecord, selectedKey: string): string {
  const key = scopeStorageKey(record.scope);
  const label = scopeLabel(record.scope);
  const initial = label.trim().charAt(0).toUpperCase() || "O";
  return [
    `<button class="chat-profile-nav-row${selected(key === selectedKey)}" type="button" data-chat-profile-scope="${escapeHtml(key)}" aria-pressed="${key === selectedKey}">`,
    `<span class="chat-profile-nav-avatar" aria-hidden="true">${escapeHtml(initial)}</span>`,
    `<span>${escapeHtml(label)}</span>`,
    `</button>`,
  ].join("");
}

function statusChoices(key: string, status: string, fieldsDisabled: boolean): string {
  return CHAT_PROFILE_STATUS_CHOICES.map((choice) => [
    `<label class="chat-profile-status-choice" data-status-tone="${choice.tone}">`,
    `<input class="chat-profile-choice-input" type="radio" name="chat-profile-status-${escapeHtml(key)}" value="${choice.value}" data-chat-profile-status${checked(status === choice.value)}${disabled(fieldsDisabled)}/>`,
    `<span class="chat-profile-status-dot" aria-hidden="true"></span>`,
    `<span>${choice.label}</span>`,
    `</label>`,
  ].join("")).join("");
}

function cardBackgroundChoices(cardBackground: string, fieldsDisabled: boolean): string {
  return CHAT_PROFILE_CARD_BACKGROUNDS.map((colour, index) => [
    `<label class="chat-profile-background-tile" data-background-tile="${index + 1}" title="Card background ${index + 1}">`,
    `<input class="chat-profile-choice-input" type="radio" name="chat-profile-card-background" value="${colour}" data-chat-profile-card-background aria-label="Card background ${index + 1}, ${colour}"${checked(cardBackground.toLowerCase() === colour)}${disabled(fieldsDisabled)}/>`,
    `<span aria-hidden="true"></span>`,
    `</label>`,
  ].join("")).join("");
}

function colourChoices(colour: string, fieldsDisabled: boolean): string {
  return CHAT_PROFILE_COLOURS.map((swatch, index) => [
    `<label class="chat-profile-colour-swatch" data-colour-swatch="${index + 1}" title="Profile colour ${index + 1}">`,
    `<input class="chat-profile-choice-input" type="radio" name="chat-profile-colour" value="${swatch}" data-chat-profile-colour aria-label="Profile colour ${index + 1}, ${swatch}"${checked(colour.toLowerCase() === swatch)}${disabled(fieldsDisabled)}/>`,
    `<span aria-hidden="true"></span>`,
    `</label>`,
  ].join("")).join("");
}

function customHexInput(
  field: "cardBackground" | "colour",
  value: string,
  fieldsDisabled: boolean,
): string {
  const id = field === "cardBackground" ? "chat-profile-card-custom" : "chat-profile-colour-custom";
  return [
    `<label class="chat-profile-custom-hex" for="${id}">`,
    `<span>Custom hex</span>`,
    `<input id="${id}" type="text" inputmode="text" maxlength="7" spellcheck="false" autocomplete="off" value="${escapeHtml(value)}" data-chat-profile-custom-hex="${field}" aria-describedby="${id}-error"${disabled(fieldsDisabled)}/>`,
    `</label>`,
    `<small class="chat-profile-hex-error" id="${id}-error" data-chat-profile-hex-error="${field}" aria-live="polite"></small>`,
  ].join("");
}

function avatarMarkup(record: ScopedProfileRecord, resolvedAvatar: string | null, fieldsDisabled: boolean): string {
  const label = scopeLabel(record.scope);
  const preview = resolvedAvatar
    ? `<img src="${escapeHtml(resolvedAvatar)}" alt="${escapeHtml(label)} avatar"/>`
    : `<span aria-hidden="true">${escapeHtml(label.charAt(0).toUpperCase() || "O")}</span>`;
  const inherited = record.scope.kind !== "global" && record.avatar === null;
  return [
    `<div class="chat-profile-avatar-editor" data-profile-field="avatar">`,
    `<div class="chat-profile-avatar-preview">${preview}</div>`,
    `<div class="chat-profile-avatar-actions">`,
    `<label class="chat-profile-button${fieldsDisabled ? " disabled" : ""}" tabindex="${fieldsDisabled ? "-1" : "0"}">Upload<input type="file" accept="image/*" data-chat-profile-avatar-upload${disabled(fieldsDisabled)}/></label>`,
    `<button class="chat-profile-button quiet" type="button" data-chat-profile-avatar-remove${disabled(fieldsDisabled || record.avatar === null)}>Remove</button>`,
    inherited ? `<small>Using the OSL profile avatar.</small>` : "",
    `</div>`,
    `</div>`,
  ].join("");
}

/**
 * Draws the complete 660px Profile & Appearance window. It deliberately has
 * no dependency on the OSL Chats shell so task 5069 can mount it where needed.
 */
export function chatProfileAppearanceModalMarkup(state: OslProfilePaneState): string {
  const records = state.rows();
  const selectedRecord = state.record(state.selectedKey) ?? records[0];
  if (!selectedRecord) throw new Error("Profile & Appearance needs at least one profile record.");
  const key = scopeStorageKey(selectedRecord.scope);
  const resolved = state.resolvedFor(key);
  if (!resolved) throw new Error(`Profile & Appearance cannot resolve scope ${key}.`);
  const fields = resolved.profile;
  const inherited = selectedRecord.scope.kind !== "global" && !selectedRecord.useSeparateProfileHere;
  const separateControl = selectedRecord.scope.kind === "global" ? "" : [
    `<label class="chat-profile-separate">`,
    `<input type="checkbox" data-chat-profile-separate${checked(selectedRecord.useSeparateProfileHere)}/>`,
    `<span>use a separate profile here</span>`,
    `</label>`,
  ].join("");

  return [
    `<div class="chat-profile-appearance-backdrop" data-chat-profile-appearance-backdrop>`,
    `<section class="chat-profile-appearance-modal" role="dialog" aria-modal="true" aria-labelledby="chat-profile-appearance-title">`,
    `<header class="chat-profile-appearance-header">`,
    `<div><span>OSL Chats</span><h2 id="chat-profile-appearance-title">Profile &amp; Appearance</h2></div>`,
    `<button type="button" class="chat-profile-appearance-close" data-chat-profile-appearance-close aria-label="Close Profile &amp; Appearance">×</button>`,
    `</header>`,
    `<div class="chat-profile-appearance-layout">`,
    `<nav class="chat-profile-appearance-nav" aria-label="Profile and appearance sections">`,
    `<section><h3>YOUR PROFILES</h3>${records.map((record) => profileNavRow(record, key)).join("")}</section>`,
    `<section><h3>APPEARANCE</h3>${CHAT_APPEARANCE_ITEMS.map((item) => `<button class="chat-profile-appearance-nav-row" type="button" data-chat-appearance-item="${escapeHtml(item)}"><span aria-hidden="true">${item === "Chat background" ? "▧" : "☰"}</span><span>${escapeHtml(item)}</span></button>`).join("")}</section>`,
    `</nav>`,
    `<div class="chat-profile-editor">`,
    `<div class="chat-profile-editor-heading"><div><span>PROFILE</span><h3>${escapeHtml(scopeLabel(selectedRecord.scope))}</h3></div>${separateControl}</div>`,
    inherited ? `<p class="chat-profile-inherited-note">This scope is using your OSL profile. Turn on a separate profile to edit it here.</p>` : "",
    `<div class="chat-profile-text-fields">`,
    `<label data-profile-field="display-name"><span>Display name</span><input type="text" maxlength="64" value="${escapeHtml(fields.displayName)}" data-chat-profile-field="displayName"${disabled(inherited)}/></label>`,
    `<label data-profile-field="about-line"><span>About line</span><input type="text" maxlength="120" value="${escapeHtml(fields.aboutLine)}" data-chat-profile-field="aboutLine"${disabled(inherited)}/></label>`,
    `</div>`,
    `<fieldset class="chat-profile-fieldset" data-profile-field="status"${disabled(inherited)}><legend>Status</legend><input class="chat-profile-status-text" type="text" maxlength="160" value="${escapeHtml(fields.status)}" data-chat-profile-field="status" aria-label="Status text"${disabled(inherited)}/><div class="chat-profile-statuses" aria-label="Status presets">${statusChoices(key, fields.status, inherited)}</div></fieldset>`,
    `<fieldset class="chat-profile-fieldset" data-profile-field="card-background"${disabled(inherited)}><legend>Card background</legend><div class="chat-profile-backgrounds">${cardBackgroundChoices(fields.cardBackground, inherited)}</div>${customHexInput("cardBackground", fields.cardBackground, inherited)}</fieldset>`,
    avatarMarkup(selectedRecord, fields.avatar, inherited),
    `<fieldset class="chat-profile-fieldset" data-profile-field="colour"${disabled(inherited)}><legend>Colour</legend><div class="chat-profile-colours">${colourChoices(fields.colour, inherited)}</div>${customHexInput("colour", fields.colour, inherited)}</fieldset>`,
    `</div>`,
    `</div>`,
    `<footer class="chat-profile-appearance-footer">${escapeHtml(PROFILE_PANE_FOOTER)}</footer>`,
    `</section>`,
    `</div>`,
  ].join("");
}

export function isChatProfileHex(value: string): boolean {
  return /^#[0-9a-f]{6}$/iu.test(value.trim());
}

/** Apply one field to one scope only; useful both to the DOM controller and persistence adapters. */
export function setChatProfileField(
  state: OslProfilePaneState,
  scopeKey: string,
  field: ScopedProfileFieldName,
  value: string,
): void {
  if (!state.record(scopeKey)) throw new Error(`Unknown profile scope: ${scopeKey}`);
  if ((field === "cardBackground" || field === "colour") && !isChatProfileHex(value)) {
    throw new Error(CHAT_PROFILE_HEX_ERROR);
  }
  state.setField(scopeKey, field, value.trim());
}

function avatarToken(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => typeof reader.result === "string"
      ? resolve(reader.result)
      : reject(new Error("The avatar could not be read.")));
    reader.addEventListener("error", () => reject(new Error("The avatar could not be read.")));
    reader.readAsDataURL(file);
  });
}

/** Mounts the standalone piece and wires every editor control to its selected scoped record. */
export function attachChatProfileAppearanceModal(
  mount: HTMLElement,
  state: OslProfilePaneState,
  handlers: ChatProfileAppearanceModalHandlers = {},
): () => void {
  let active = true;
  const draw = (): void => {
    if (active) mount.innerHTML = chatProfileAppearanceModalMarkup(state);
  };
  const changed = (): void => handlers.onRecordsChange?.(recordClones(state));
  const setHexError = (field: string, message: string): void => {
    const error = mount.querySelector<HTMLElement>(`[data-chat-profile-hex-error="${field}"]`);
    if (error) error.textContent = message;
  };

  const click = (event: Event): void => {
    const target = event.target as HTMLElement | null;
    const scopeButton = target?.closest<HTMLElement>("[data-chat-profile-scope]");
    if (scopeButton?.dataset.chatProfileScope) {
      state.selectScope(scopeButton.dataset.chatProfileScope);
      draw();
      return;
    }
    if (target?.closest("[data-chat-profile-avatar-remove]")) {
      state.removeAvatar(state.selectedKey);
      changed();
      draw();
      return;
    }
    if (target?.closest("[data-chat-profile-appearance-close]")) handlers.onClose?.();
  };

  const change = (event: Event): void => {
    const target = event.target as HTMLInputElement | null;
    if (!target) return;
    if (target.matches("[data-chat-profile-separate]")) {
      state.setSeparate(state.selectedKey, target.checked);
      changed();
      draw();
      return;
    }
    const field = target.dataset.chatProfileField as ScopedProfileFieldName | undefined;
    if (field) {
      setChatProfileField(state, state.selectedKey, field, target.value);
      changed();
      draw();
      return;
    }
    if (target.matches("[data-chat-profile-status]")) {
      setChatProfileField(state, state.selectedKey, "status", target.value);
      changed();
      draw();
      return;
    }
    if (target.matches("[data-chat-profile-card-background]")) {
      setChatProfileField(state, state.selectedKey, "cardBackground", target.value);
      changed();
      draw();
      return;
    }
    if (target.matches("[data-chat-profile-colour]")) {
      setChatProfileField(state, state.selectedKey, "colour", target.value);
      changed();
      draw();
      return;
    }
    const customHex = target.dataset.chatProfileCustomHex as "cardBackground" | "colour" | undefined;
    if (customHex) {
      if (!isChatProfileHex(target.value)) {
        setHexError(customHex, CHAT_PROFILE_HEX_ERROR);
        return;
      }
      setChatProfileField(state, state.selectedKey, customHex, target.value);
      changed();
      draw();
      return;
    }
    if (target.matches("[data-chat-profile-avatar-upload]")) {
      const file = target.files?.[0];
      if (!file) return;
      const scopeKey = state.selectedKey;
      void avatarToken(file).then((token) => {
        if (!active) return;
        state.uploadAvatar(scopeKey, token);
        changed();
        draw();
      });
    }
  };

  mount.addEventListener("click", click);
  mount.addEventListener("change", change);
  draw();
  return () => {
    active = false;
    mount.removeEventListener("click", click);
    mount.removeEventListener("change", change);
  };
}
