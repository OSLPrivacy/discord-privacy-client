/**
 * Member-facing role picker. The caller supplies the persisted role records;
 * this module keeps the display filter and take/drop boundary in one place.
 */
export interface SelfAssignableRole {
  readonly id: string;
  readonly name: string;
  readonly colour: string;
  readonly icon: string;
  readonly selfAssignable: boolean;
}

export interface PickYourRolesState {
  readonly roles: readonly SelfAssignableRole[];
  readonly memberRoleIds: readonly string[];
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

/** Only owner-marked self-assignable roles are ever exposed to a member. */
export function pickerRoles(roles: readonly SelfAssignableRole[]): SelfAssignableRole[] {
  return roles.filter((role) => role.selfAssignable);
}

export function pickYourRolesMarkup(state: PickYourRolesState): string {
  const memberRoles = new Set(state.memberRoleIds);
  const rows = pickerRoles(state.roles).map((role) => {
    const taken = memberRoles.has(role.id);
    return `<li class="pick-your-roles-row" data-pickable-role-id="${escapeHtml(role.id)}" data-taken="${taken}">
      <span class="pick-your-roles-icon" aria-hidden="true" style="color:${escapeHtml(role.colour)}">${escapeHtml(role.icon)}</span>
      <span class="pick-your-roles-name" style="color:${escapeHtml(role.colour)}">${escapeHtml(role.name)}</span>
      <button type="button" data-pick-your-role="${escapeHtml(role.id)}" aria-pressed="${taken}">${taken ? "Drop role" : "Take role"}</button>
    </li>`;
  }).join("");
  return `<section class="pick-your-roles" data-pick-your-roles aria-labelledby="pick-your-roles-title">
    <header><h2 id="pick-your-roles-title">Pick your roles</h2><p>Choose the roles this Enclave lets members assign themselves. Changes apply immediately.</p></header>
    <ul aria-label="Self-assignable roles" data-pick-your-roles-rows>${rows}</ul>
  </section>`;
}

/** In-memory adapter for the native screen and deterministic UI fixture. */
export class PickYourRolesController {
  private readonly roles: readonly SelfAssignableRole[];
  private memberRoleIds: Set<string>;

  constructor(state: PickYourRolesState) {
    this.roles = state.roles;
    this.memberRoleIds = new Set(state.memberRoleIds);
  }

  snapshot(): PickYourRolesState {
    return { roles: this.roles, memberRoleIds: [...this.memberRoleIds].sort() };
  }

  take(roleId: string): boolean {
    if (!pickerRoles(this.roles).some((role) => role.id === roleId) || this.memberRoleIds.has(roleId)) return false;
    this.memberRoleIds.add(roleId);
    return true;
  }

  drop(roleId: string): boolean {
    if (!pickerRoles(this.roles).some((role) => role.id === roleId)) return false;
    return this.memberRoleIds.delete(roleId);
  }
}

export function bindPickYourRoles(
  controller: PickYourRolesController,
  requestRender: () => void,
): void {
  document.querySelectorAll<HTMLButtonElement>("[data-pick-your-role]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.pickYourRole ?? "";
      const taken = controller.snapshot().memberRoleIds.includes(id);
      if (taken ? controller.drop(id) : controller.take(id)) requestRender();
    });
  });
}

/** Mount the complete member screen; every click re-renders from the changed role set. */
export function mountPickYourRoles(
  root: HTMLElement,
  initial: PickYourRolesState,
): PickYourRolesController {
  const controller = new PickYourRolesController(initial);
  const render = (): void => {
    root.innerHTML = pickYourRolesMarkup(controller.snapshot());
    bindPickYourRoles(controller, render);
  };
  render();
  return controller;
}
