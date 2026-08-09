/**
 * The Profile pane in Appearance settings: one row for the OSL profile
 * (global), one for OSL Chats, and one per enclave. Each non-global row
 * carries a "use a separate profile here" checkbox; unchecked rows inherit
 * every field from the global profile. The avatar field additionally falls
 * back to the global avatar on its own whenever a scope's own avatar is
 * cleared, independent of the other five fields, so REMOVE always has
 * somewhere honest to fall back to.
 */

export type OslProfileScope =
  | { kind: "global" }
  | { kind: "osl_chats" }
  | { kind: "enclave"; enclaveId: string };

export interface ScopedProfileRecord {
  scope: OslProfileScope;
  useSeparateProfileHere: boolean;
  displayName: string;
  aboutLine: string;
  status: string;
  cardBackground: string;
  avatar: string | null;
  colour: string;
}

export interface ScopedProfileFields {
  displayName: string;
  aboutLine: string;
  status: string;
  cardBackground: string;
  avatar: string | null;
  colour: string;
}

export interface ResolvedScopedProfile {
  scope: OslProfileScope;
  useSeparateProfileHere: boolean;
  avatarInherited: boolean;
  profile: ScopedProfileFields;
}

const MAX_DISPLAY_NAME_CHARS = 64;
const MAX_ABOUT_LINE_CHARS = 120;
const MAX_STATUS_CHARS = 160;

export const PROFILE_PANE_FOOTER =
  "A profile is a display name and a colour. Your identity is the key, and that does not change here.";

export function scopeStorageKey(scope: OslProfileScope): string {
  switch (scope.kind) {
    case "global": return "global";
    case "osl_chats": return "osl-chats";
    case "enclave": return `enclave:${scope.enclaveId}`;
  }
}

export function scopeLabel(scope: OslProfileScope): string {
  switch (scope.kind) {
    case "global": return "OSL profile";
    case "osl_chats": return "OSL Chats";
    case "enclave": return `${scope.enclaveId[0]?.toUpperCase() ?? ""}${scope.enclaveId.slice(1)}`;
  }
}

function scopeSortKey(scope: OslProfileScope): [number, string] {
  switch (scope.kind) {
    case "global": return [0, ""];
    case "osl_chats": return [1, ""];
    case "enclave": return [2, scope.enclaveId];
  }
}

function boundedTrimmed(value: string, label: string, maxChars: number, allowBlank: boolean): string {
  const trimmed = value.trim();
  if (!allowBlank && trimmed.length === 0) throw new Error(`${label} cannot be blank`);
  if (trimmed.length > maxChars) throw new Error(`${label} is too long`);
  return trimmed;
}

function validateProfileScope(scope: OslProfileScope): void {
  if (scope.kind !== "enclave") return;
  const trimmed = scope.enclaveId.trim();
  if (trimmed.length === 0 || trimmed !== scope.enclaveId || /\s/u.test(scope.enclaveId)) {
    throw new Error("OSL enclave profile scope id is invalid");
  }
}

function validateScopedFields(record: ScopedProfileRecord, labelPrefix: string): ScopedProfileFields {
  return {
    displayName: boundedTrimmed(record.displayName, `${labelPrefix} display name`, MAX_DISPLAY_NAME_CHARS, false),
    aboutLine: boundedTrimmed(record.aboutLine, `${labelPrefix} about line`, MAX_ABOUT_LINE_CHARS, true),
    status: boundedTrimmed(record.status, `${labelPrefix} status`, MAX_STATUS_CHARS, true),
    cardBackground: record.cardBackground,
    avatar: record.avatar,
    colour: record.colour,
  };
}

/**
 * Resolves every record against the single global record. A non-global
 * record only overrides fields when `useSeparateProfileHere` is set; the
 * avatar overrides again on its own, falling back to the global avatar
 * whenever the record's own avatar is null (REMOVE).
 */
export function resolveScopedProfileRecords(records: ScopedProfileRecord[]): ResolvedScopedProfile[] {
  const seen = new Set<string>();
  let global: ScopedProfileRecord | null = null;
  for (const record of records) {
    validateProfileScope(record.scope);
    const key = scopeStorageKey(record.scope);
    if (seen.has(key)) throw new Error(`OSL profile scope record is duplicated: ${key}`);
    seen.add(key);
    if (record.scope.kind === "global") {
      if (global) throw new Error("OSL profile has more than one global record");
      global = record;
    }
  }
  if (!global) throw new Error("OSL global profile record is missing");
  const globalFields = validateScopedFields(global, "global");

  const sorted = [...records].sort((a, b) => {
    const [aRank, aId] = scopeSortKey(a.scope);
    const [bRank, bId] = scopeSortKey(b.scope);
    return aRank !== bRank ? aRank - bRank : aId.localeCompare(bId);
  });

  return sorted.map((record) => {
    const useOverride = record.scope.kind !== "global" && record.useSeparateProfileHere;
    const own = useOverride ? validateScopedFields(record, scopeStorageKey(record.scope)) : globalFields;
    const avatarInherited = record.scope.kind !== "global" && own.avatar === null;
    return {
      scope: record.scope,
      useSeparateProfileHere: useOverride,
      avatarInherited,
      profile: { ...own, avatar: avatarInherited ? globalFields.avatar : own.avatar },
    };
  });
}

function cloneRecord(record: ScopedProfileRecord): ScopedProfileRecord {
  return { ...record, scope: { ...record.scope } };
}

export type ScopedProfileFieldName = "displayName" | "aboutLine" | "status" | "cardBackground" | "colour";

/** Local editing state for the profile pane dialog: which scope is selected and each scope's editable record. */
export class OslProfilePaneState {
  private readonly records = new Map<string, ScopedProfileRecord>();
  selectedKey: string;

  constructor(records: ScopedProfileRecord[]) {
    for (const record of records) this.records.set(scopeStorageKey(record.scope), cloneRecord(record));
    this.selectedKey = "global";
  }

  rows(): ScopedProfileRecord[] {
    return [...this.records.values()].sort((a, b) => {
      const [aRank, aId] = scopeSortKey(a.scope);
      const [bRank, bId] = scopeSortKey(b.scope);
      return aRank !== bRank ? aRank - bRank : aId.localeCompare(bId);
    });
  }

  record(key: string): ScopedProfileRecord | null {
    return this.records.get(key) ?? null;
  }

  resolved(): ResolvedScopedProfile[] {
    return resolveScopedProfileRecords([...this.records.values()]);
  }

  resolvedFor(key: string): ResolvedScopedProfile | null {
    return this.resolved().find((entry) => scopeStorageKey(entry.scope) === key) ?? null;
  }

  selectScope(key: string): void {
    if (this.records.has(key)) this.selectedKey = key;
  }

  setSeparate(key: string, value: boolean): void {
    const record = this.records.get(key);
    if (!record || record.scope.kind === "global") return;
    record.useSeparateProfileHere = value;
  }

  setField(key: string, field: ScopedProfileFieldName, value: string): void {
    const record = this.records.get(key);
    if (!record) return;
    record[field] = value;
  }

  uploadAvatar(key: string, token: string): void {
    const record = this.records.get(key);
    if (!record) return;
    if (record.scope.kind !== "global") record.useSeparateProfileHere = true;
    record.avatar = token;
  }

  removeAvatar(key: string): void {
    const record = this.records.get(key);
    if (!record) return;
    record.avatar = null;
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function avatarPreviewMarkup(avatar: string | null): string {
  return avatar
    ? `<img class="profile-avatar-preview" src="${escapeHtml(avatar)}" alt=""/>`
    : `<div class="profile-avatar-preview empty" aria-hidden="true"></div>`;
}

function profileRowMarkup(record: ScopedProfileRecord, selected: boolean): string {
  const key = scopeStorageKey(record.scope);
  const label = escapeHtml(scopeLabel(record.scope));
  const checkbox = record.scope.kind === "global"
    ? ""
    : `<label class="profile-scope-checkbox"><input type="checkbox" data-profile-separate-toggle="${key}" ${record.useSeparateProfileHere ? "checked" : ""}/> use a separate profile here</label>`;
  return `<div class="setting-line profile-scope-row${selected ? " selected" : ""}" data-profile-row-scope="${key}"><button type="button" class="profile-scope-select" data-profile-select-scope="${key}"><strong>${label}</strong></button>${checkbox}</div>`;
}

function profileFieldsMarkup(key: string, fields: ScopedProfileFields): string {
  return `<form class="profile-fields" data-profile-fields-scope="${key}">`
    + `<label class="profile-field" data-profile-field="display-name"><span>Display name</span><input type="text" data-profile-field-input="displayName" value="${escapeHtml(fields.displayName)}"/></label>`
    + `<label class="profile-field" data-profile-field="about-line"><span>About line</span><input type="text" data-profile-field-input="aboutLine" value="${escapeHtml(fields.aboutLine)}"/></label>`
    + `<label class="profile-field" data-profile-field="status"><span>Status</span><input type="text" data-profile-field-input="status" value="${escapeHtml(fields.status)}"/></label>`
    + `<label class="profile-field" data-profile-field="card-background"><span>Card background</span><input type="color" data-profile-field-input="cardBackground" value="${escapeHtml(fields.cardBackground)}"/></label>`
    + `<div class="profile-field" data-profile-field="avatar"><span>Avatar</span><div class="profile-avatar-editor">${avatarPreviewMarkup(fields.avatar)}<label class="button compact profile-avatar-upload-label">Upload<input type="file" accept="image/*" data-profile-avatar-upload="${key}"/></label><button type="button" class="button compact" data-profile-avatar-remove="${key}" ${fields.avatar ? "" : "disabled"}>REMOVE</button></div></div>`
    + `<label class="profile-field" data-profile-field="colour"><span>Colour</span><input type="color" data-profile-field-input="colour" value="${escapeHtml(fields.colour)}"/></label>`
    + `</form>`;
}

export function oslProfilePaneMarkup(state: OslProfilePaneState): string {
  const rows = state.rows();
  const selectedRecord = state.record(state.selectedKey) ?? rows[0];
  const selectedKey = scopeStorageKey(selectedRecord.scope);
  const selectedResolved = state.resolvedFor(selectedKey);
  const fields = selectedResolved ? selectedResolved.profile : validateScopedFields(selectedRecord, selectedKey);
  const rowsMarkup = rows.map((record) => profileRowMarkup(record, scopeStorageKey(record.scope) === selectedKey)).join("");
  return `<dialog class="friends-dialog osl-profile-pane-dialog" id="osl-profile-pane-dialog" aria-labelledby="osl-profile-pane-title"><div class="friends-dialog-card"><header><div><span>Profile</span><h2 id="osl-profile-pane-title">${escapeHtml(scopeLabel(selectedRecord.scope))}</h2></div><button class="icon-button" id="osl-profile-pane-close" type="button" aria-label="Close profile settings">×</button></header><div class="settings-list"><div class="profile-scope-rows">${rowsMarkup}</div>${profileFieldsMarkup(selectedKey, fields)}</div><footer class="profile-pane-footer"><p>${escapeHtml(PROFILE_PANE_FOOTER)}</p></footer></div></dialog>`;
}

// Seeded USER profile data: card backgrounds and avatar colours are the
// user's own persisted choices (custom hex is allowed by the design), not UI
// chrome — DELIBERATELY not osl-tokens.ts values.
export function seededProfilePaneRecords(): ScopedProfileRecord[] {
  return [
    {
      scope: { kind: "global" },
      useSeparateProfileHere: false,
      displayName: "Quinn",
      aboutLine: "Building things quietly.",
      status: "Available",
      cardBackground: "#1c2230",
      avatar: "https://example.com/global-avatar.png",
      colour: "#5c8dff",
    },
    {
      scope: { kind: "osl_chats" },
      useSeparateProfileHere: true,
      displayName: "Quinn (Chats)",
      aboutLine: "Reachable for encrypted chats only.",
      status: "In chats",
      cardBackground: "#20283a",
      avatar: "https://example.com/chats-avatar.png",
      colour: "#7fb2ff",
    },
    {
      scope: { kind: "enclave", enclaveId: "cedar" },
      useSeparateProfileHere: false,
      displayName: "Cedar Quinn",
      aboutLine: "Cedar enclave about line.",
      status: "Cedar status",
      cardBackground: "#2a3348",
      avatar: "https://example.com/cedar-avatar.png",
      colour: "#a0c4ff",
    },
    {
      scope: { kind: "enclave", enclaveId: "maple" },
      useSeparateProfileHere: false,
      displayName: "Maple Quinn",
      aboutLine: "Maple enclave about line.",
      status: "Maple status",
      cardBackground: "#333e58",
      avatar: "https://example.com/maple-avatar.png",
      colour: "#c0d8ff",
    },
  ];
}
