/**
 * Owner-facing role editor for a community.  The permissions are deliberately
 * data-driven: adding a permission cannot accidentally omit its enforcement
 * disclosure because the tag is rendered by the shared row renderer.
 */
export const OWNER_ROLE_PERMISSIONS = [
  ["view_channels", "View channels"], ["manage_channels", "Manage channels"],
  ["manage_roles", "Manage roles"], ["manage_emojis", "Manage emojis and stickers"],
  ["view_audit_log", "View audit log"], ["manage_webhooks", "Manage webhooks"],
  ["create_invites", "Create invites"], ["change_nickname", "Change nickname"],
  ["manage_nicknames", "Manage nicknames"], ["kick_members", "Kick members"],
  ["ban_members", "Ban members"], ["timeout_members", "Timeout members"],
  ["send_messages", "Send messages"], ["create_threads", "Create threads"],
  ["send_in_threads", "Send messages in threads"], ["manage_messages", "Manage messages"],
  ["embed_links", "Embed links"], ["attach_files", "Attach files"],
  ["add_reactions", "Add reactions"], ["use_external_emoji", "Use external emoji"],
  ["mention_everyone", "Mention @everyone, @here, and all roles"], ["use_application_commands", "Use application commands"],
  ["connect_voice", "Connect to voice"], ["speak_voice", "Speak in voice"],
  ["video_voice", "Video"], ["mute_members", "Mute members"],
  ["deafen_members", "Deafen members"], ["move_members", "Move members"],
  ["priority_speaker", "Priority speaker"], ["stream", "Stream"],
  ["request_to_speak", "Request to speak"], ["start_activities", "Start activities"],
  ["create_events", "Create events"], ["manage_events", "Manage events"],
  ["moderate_members", "Moderate members"], ["view_insights", "View server insights"],
  ["manage_guild", "Manage community"], ["administrator", "Administrator"],
  ["manage_expressions", "Manage expressions"], ["bypass_slowmode", "Bypass slow mode"],
] as const;

export type OwnerRolePermission = (typeof OWNER_ROLE_PERMISSIONS)[number][0];
export type MentionRule = "none" | "role" | "everyone";
export type RoleTemplate = "custom" | "moderator" | "community-helper" | "event-host";

export interface OwnerRoleLimits {
  slowModeSeconds: number;
  muteCapMinutes: number;
  actionBudget: number;
}

export interface OwnerRole {
  id: string;
  name: string;
  color: string;
  icon: string;
  hoisted: boolean;
  mentionRule: MentionRule;
  selfAssignable: boolean;
  autoGrant: boolean;
  expiryDays: number | null;
  template: RoleTemplate;
  permissions: OwnerRolePermission[];
  limits: OwnerRoleLimits;
  readonly builtIn?: boolean;
}

export interface OwnerRoleEditorState {
  roles: OwnerRole[];
  selectedRoleId: string;
}

const DEFAULT_LIMITS: OwnerRoleLimits = { slowModeSeconds: 0, muteCapMinutes: 0, actionBudget: 0 };
const OWNER_ROLE_ENFORCEMENT_LABEL = "Server-enforced";

function cloneRole(role: OwnerRole): OwnerRole {
  return { ...role, permissions: [...role.permissions], limits: { ...role.limits } };
}

function cloneState(state: OwnerRoleEditorState): OwnerRoleEditorState {
  return { selectedRoleId: state.selectedRoleId, roles: state.roles.map(cloneRole) };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

function defaultRole(id: string, name = "New role"): OwnerRole {
  return {
    id, name, color: "#5865f2", icon: "✦", hoisted: false, mentionRule: "none",
    selfAssignable: false, autoGrant: false, expiryDays: null, template: "custom",
    permissions: [], limits: { ...DEFAULT_LIMITS },
  };
}

export function initialOwnerRoleEditorState(): OwnerRoleEditorState {
  return {
    selectedRoleId: "member",
    roles: [{ ...defaultRole("member", "MEMBER"), color: "#99aab5", icon: "●", builtIn: true }],
  };
}

function selectedRole(state: OwnerRoleEditorState): OwnerRole {
  return state.roles.find((role) => role.id === state.selectedRoleId) ?? state.roles[0];
}

function withTemplate(role: OwnerRole, template: RoleTemplate): OwnerRole {
  const permissions: Record<RoleTemplate, OwnerRolePermission[]> = {
    custom: role.permissions,
    moderator: ["view_channels", "send_messages", "manage_messages", "timeout_members", "moderate_members"],
    "community-helper": ["view_channels", "send_messages", "create_threads", "add_reactions", "use_application_commands"],
    "event-host": ["view_channels", "send_messages", "connect_voice", "speak_voice", "stream", "create_events"],
  };
  return { ...role, template, permissions: [...permissions[template]] };
}

/** Mutable controller used by the native shell and the deterministic QA fixture. */
export class OwnerRoleEditor {
  private state: OwnerRoleEditorState;
  private saved: OwnerRoleEditorState;
  private nextId = 1;

  constructor(state = initialOwnerRoleEditorState()) {
    this.state = cloneState(state);
    this.saved = cloneState(state);
  }

  snapshot(): OwnerRoleEditorState { return cloneState(this.state); }
  reopened(): OwnerRoleEditorState { return cloneState(this.saved); }
  active(): OwnerRole { return cloneRole(selectedRole(this.state)); }

  select(roleId: string): void {
    if (this.state.roles.some((role) => role.id === roleId)) this.state.selectedRoleId = roleId;
  }

  create(name = "New role"): OwnerRole {
    const role = defaultRole(`custom-${this.nextId++}`, name);
    this.state.roles.push(role);
    this.state.selectedRoleId = role.id;
    return cloneRole(role);
  }

  duplicate(): OwnerRole | null {
    const source = selectedRole(this.state);
    if (source.builtIn) return null;
    const duplicate = { ...cloneRole(source), id: `custom-${this.nextId++}`, name: `${source.name} copy` };
    const index = this.state.roles.findIndex((role) => role.id === source.id);
    this.state.roles.splice(index + 1, 0, duplicate);
    this.state.selectedRoleId = duplicate.id;
    return cloneRole(duplicate);
  }

  rename(name: string): void { this.change((role) => ({ ...role, name: name.trim().slice(0, 64) || "Untitled role" })); }
  setColor(color: string): void { if (/^#[0-9a-f]{6}$/iu.test(color)) this.change((role) => ({ ...role, color })); }
  setIcon(icon: string): void { this.change((role) => ({ ...role, icon: [...icon].slice(0, 2).join("") || "✦" })); }
  setHoisted(hoisted: boolean): void { this.change((role) => ({ ...role, hoisted })); }
  setMentionRule(mentionRule: MentionRule): void { this.change((role) => ({ ...role, mentionRule })); }
  setSelfAssignable(selfAssignable: boolean): void { this.change((role) => ({ ...role, selfAssignable })); }
  setAutoGrant(autoGrant: boolean): void { this.change((role) => ({ ...role, autoGrant })); }
  setExpiryDays(expiryDays: number | null): void { this.change((role) => ({ ...role, expiryDays: expiryDays !== null && Number.isSafeInteger(expiryDays) && expiryDays > 0 ? expiryDays : null })); }
  setTemplate(template: RoleTemplate): void { this.change((role) => withTemplate(role, template)); }
  setPermission(permission: OwnerRolePermission, checked: boolean): void {
    this.change((role) => ({ ...role, permissions: checked ? [...new Set([...role.permissions, permission])] : role.permissions.filter((item) => item !== permission) }));
  }
  setLimits(limits: Partial<OwnerRoleLimits>): void {
    this.change((role) => ({ ...role, limits: {
      slowModeSeconds: sanitizeLimit(limits.slowModeSeconds, role.limits.slowModeSeconds),
      muteCapMinutes: sanitizeLimit(limits.muteCapMinutes, role.limits.muteCapMinutes),
      actionBudget: sanitizeLimit(limits.actionBudget, role.limits.actionBudget),
    } }));
  }

  moveActiveAboveMember(): void {
    const role = selectedRole(this.state);
    if (role.builtIn) return;
    const memberIndex = this.state.roles.findIndex((item) => item.id === "member");
    const roleIndex = this.state.roles.findIndex((item) => item.id === role.id);
    if (memberIndex < 0 || roleIndex < 0 || roleIndex === memberIndex - 1) return;
    this.state.roles.splice(roleIndex, 1);
    const nextMemberIndex = this.state.roles.findIndex((item) => item.id === "member");
    this.state.roles.splice(nextMemberIndex, 0, role);
  }

  save(): void { this.saved = cloneState(this.state); }

  private change(change: (role: OwnerRole) => OwnerRole): void {
    const index = this.state.roles.findIndex((role) => role.id === this.state.selectedRoleId);
    if (index < 0 || this.state.roles[index].builtIn) return;
    this.state.roles[index] = change(this.state.roles[index]);
  }
}

function sanitizeLimit(value: number | undefined, fallback: number): number {
  return value !== undefined && Number.isSafeInteger(value) && value >= 0 && value <= 86_400 ? value : fallback;
}

function selectOption(value: string, selected: string): string { return value === selected ? " selected" : ""; }

/** Actual editor markup; every row gets its tag from this function, not caller discipline. */
export function ownerRoleEditorMarkup(state: OwnerRoleEditorState, showEnforcementTags = true): string {
  const role = selectedRole(state);
  const roleList = state.roles.map((item) => `<button class="owner-role-list-item ${item.id === role.id ? "selected" : ""}" data-owner-role-select="${escapeHtml(item.id)}" type="button" aria-pressed="${item.id === role.id}"><span class="owner-role-swatch" style="--owner-role-color:${escapeHtml(item.color)}">${escapeHtml(item.icon)}</span><span>${escapeHtml(item.name)}</span>${item.builtIn ? '<small>default</small>' : ""}</button>`).join("");
  const permissions = OWNER_ROLE_PERMISSIONS.map(([id, label]) => `<label class="owner-role-permission-row" data-owner-role-permission-row="${id}"><input data-owner-role-permission="${id}" type="checkbox" ${role.permissions.includes(id) ? "checked" : ""} ${role.builtIn ? "disabled" : ""}/><span>${label}</span>${showEnforcementTags ? `<span class="role-enforcement-tag" data-enforcement-tag="server-checked">${OWNER_ROLE_ENFORCEMENT_LABEL}</span>` : ""}</label>`).join("");
  const disabled = role.builtIn ? "disabled" : "";
  const expiry = role.expiryDays === null ? "" : String(role.expiryDays);
  return `<section class="owner-role-editor" data-owner-role-editor aria-labelledby="owner-role-editor-title"><header class="owner-role-editor-header"><div><p class="eyebrow">Community governance</p><h2 id="owner-role-editor-title">Role editor</h2><p>Changes take effect only after you save. Permission enforcement is identified on every row.</p></div><button class="button primary compact" data-owner-role-create type="button">Create role</button></header><div class="owner-role-editor-grid"><aside class="owner-role-list" aria-label="Roles"><div>${roleList}</div><button class="button compact" data-owner-role-duplicate type="button" ${disabled}>Duplicate selected</button></aside><form class="owner-role-form" data-owner-role-form><header><span class="owner-role-icon-preview" style="--owner-role-color:${escapeHtml(role.color)}">${escapeHtml(role.icon)}</span><div><h3>${escapeHtml(role.name)}</h3><p>${role.builtIn ? "The default role cannot be edited." : "Configure identity, assignment, permissions, and guardrails."}</p></div></header><div class="owner-role-fields"><label>Name<input data-owner-role-name type="text" maxlength="64" value="${escapeHtml(role.name)}" ${disabled}/></label><label>Colour<input data-owner-role-color type="color" value="${escapeHtml(role.color)}" ${disabled}/></label><label>Icon<input data-owner-role-icon type="text" maxlength="2" value="${escapeHtml(role.icon)}" ${disabled}/></label><label>Template<select data-owner-role-template ${disabled}><option value="custom"${selectOption("custom", role.template)}>Custom</option><option value="moderator"${selectOption("moderator", role.template)}>Moderator</option><option value="community-helper"${selectOption("community-helper", role.template)}>Community helper</option><option value="event-host"${selectOption("event-host", role.template)}>Event host</option></select></label><label>Mention rule<select data-owner-role-mention-rule ${disabled}><option value="none"${selectOption("none", role.mentionRule)}>Cannot mention</option><option value="role"${selectOption("role", role.mentionRule)}>May mention this role</option><option value="everyone"${selectOption("everyone", role.mentionRule)}>May mention everyone</option></select></label><label>Expiry (days)<input data-owner-role-expiry type="number" min="1" step="1" value="${expiry}" placeholder="Never" ${disabled}/></label></div><div class="owner-role-toggle-grid"><label><input data-owner-role-hoist type="checkbox" ${role.hoisted ? "checked" : ""} ${disabled}/> Display role members separately</label><label><input data-owner-role-self-assignable type="checkbox" ${role.selfAssignable ? "checked" : ""} ${disabled}/> Members can self-assign</label><label><input data-owner-role-auto-grant type="checkbox" ${role.autoGrant ? "checked" : ""} ${disabled}/> Auto-grant on join</label></div><div class="owner-role-actions"><button class="button compact" data-owner-role-move-above-member type="button" ${disabled}>Move above MEMBER</button><button class="button primary compact" data-owner-role-save type="button">Save role changes</button></div><fieldset class="owner-role-limits" ${disabled}><legend>Limits</legend><label>Slow mode (seconds)<input data-owner-role-slow-mode type="number" min="0" max="86400" value="${role.limits.slowModeSeconds}"/></label><label>Mute cap (minutes)<input data-owner-role-mute-cap type="number" min="0" max="86400" value="${role.limits.muteCapMinutes}"/></label><label>Action budget<input data-owner-role-action-budget type="number" min="0" max="86400" value="${role.limits.actionBudget}"/></label></fieldset><fieldset class="owner-role-permissions" ${disabled}><legend>Permissions <small>${role.permissions.length} selected</small></legend><p>Each permission is independently server-enforced.</p><div>${permissions}</div></fieldset></form></div></section>`;
}

/** Binds the markup to a controller. The shell supplies its own render scheduler. */
export function bindOwnerRoleEditor(editor: OwnerRoleEditor, requestRender: () => void): void {
  const refresh = (): void => requestRender();
  document.querySelectorAll<HTMLButtonElement>("[data-owner-role-select]").forEach((button) => button.addEventListener("click", () => { editor.select(button.dataset.ownerRoleSelect ?? ""); refresh(); }));
  document.querySelector<HTMLButtonElement>("[data-owner-role-create]")?.addEventListener("click", () => { editor.create(); refresh(); });
  document.querySelector<HTMLButtonElement>("[data-owner-role-duplicate]")?.addEventListener("click", () => { editor.duplicate(); refresh(); });
  document.querySelector<HTMLButtonElement>("[data-owner-role-move-above-member]")?.addEventListener("click", () => { editor.moveActiveAboveMember(); refresh(); });
  document.querySelector<HTMLInputElement>("[data-owner-role-name]")?.addEventListener("change", (event) => { editor.rename((event.currentTarget as HTMLInputElement).value); refresh(); });
  document.querySelector<HTMLInputElement>("[data-owner-role-color]")?.addEventListener("input", (event) => { editor.setColor((event.currentTarget as HTMLInputElement).value); refresh(); });
  document.querySelector<HTMLInputElement>("[data-owner-role-icon]")?.addEventListener("change", (event) => { editor.setIcon((event.currentTarget as HTMLInputElement).value); refresh(); });
  document.querySelector<HTMLSelectElement>("[data-owner-role-template]")?.addEventListener("change", (event) => { editor.setTemplate((event.currentTarget as HTMLSelectElement).value as RoleTemplate); refresh(); });
  document.querySelector<HTMLSelectElement>("[data-owner-role-mention-rule]")?.addEventListener("change", (event) => { editor.setMentionRule((event.currentTarget as HTMLSelectElement).value as MentionRule); refresh(); });
  document.querySelector<HTMLInputElement>("[data-owner-role-expiry]")?.addEventListener("change", (event) => { const value = (event.currentTarget as HTMLInputElement).value; editor.setExpiryDays(value ? Number(value) : null); refresh(); });
  for (const [selector, setter] of [["[data-owner-role-hoist]", (value: boolean) => editor.setHoisted(value)], ["[data-owner-role-self-assignable]", (value: boolean) => editor.setSelfAssignable(value)], ["[data-owner-role-auto-grant]", (value: boolean) => editor.setAutoGrant(value)]] as const) {
    document.querySelector<HTMLInputElement>(selector)?.addEventListener("change", (event) => { setter((event.currentTarget as HTMLInputElement).checked); refresh(); });
  }
  document.querySelectorAll<HTMLInputElement>("[data-owner-role-permission]").forEach((input) => input.addEventListener("change", () => { editor.setPermission(input.dataset.ownerRolePermission as OwnerRolePermission, input.checked); refresh(); }));
  const updateLimits = (): void => editor.setLimits({ slowModeSeconds: Number(document.querySelector<HTMLInputElement>("[data-owner-role-slow-mode]")?.value), muteCapMinutes: Number(document.querySelector<HTMLInputElement>("[data-owner-role-mute-cap]")?.value), actionBudget: Number(document.querySelector<HTMLInputElement>("[data-owner-role-action-budget]")?.value) });
  document.querySelectorAll<HTMLInputElement>("[data-owner-role-slow-mode], [data-owner-role-mute-cap], [data-owner-role-action-budget]").forEach((input) => input.addEventListener("change", () => { updateLimits(); refresh(); }));
  document.querySelector<HTMLButtonElement>("[data-owner-role-save]")?.addEventListener("click", () => { editor.save(); refresh(); });
}
