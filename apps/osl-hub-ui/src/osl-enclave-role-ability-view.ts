/**
 * TASK 4858 — the "show me what this role can actually do" screen.
 *
 * The screen decides nothing. Every row arrives already resolved by the one
 * enclave permission resolver (TASK 4860) plus the relay limit gate, written
 * out by `task-4858-role-ability-rows`. This file's whole job is to draw those
 * answers honestly: an allowed row says so, and a denied row must carry the
 * plain reason it was denied. A denied row with no reason is a bug, so the
 * renderer marks it rather than quietly drawing a bare "Denied".
 */

export const OSL_ENCLAVE_ROLE_ABILITY_REASONS = [
  "no channel key",
  "channel override says deny",
  "relay timeout is active",
  "role limit is lower",
  "permission is off",
] as const;

export type OslEnclaveRoleAbilityReason =
  (typeof OSL_ENCLAVE_ROLE_ABILITY_REASONS)[number];

export interface OslEnclaveRoleAbilityRow {
  permission: string;
  label: string;
  state: "allowed" | "denied";
  decidedBy: string;
  /** Empty on an allowed row; one of the five reasons on a denied one. */
  reason: string;
  detail: string;
}

export interface OslEnclaveRoleAbilityModel {
  roleName: string;
  channelName: string;
  rows: readonly OslEnclaveRoleAbilityRow[];
}

export const OSL_ENCLAVE_ROLE_ABILITY_ALLOWED_NOTE =
  "This role can do this here.";

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function isKnownReason(reason: string): reason is OslEnclaveRoleAbilityReason {
  return (OSL_ENCLAVE_ROLE_ABILITY_REASONS as readonly string[]).includes(reason);
}

function abilityRow(row: OslEnclaveRoleAbilityRow): string {
  const allowed = row.state === "allowed";
  const reason = row.reason.trim();
  const detail = row.detail.trim();

  // Only a real reason gets drawn. An empty or unknown one leaves the row
  // visibly unexplained, which is exactly what the ability check looks for.
  const explanation = allowed
    ? `<p class="osl-role-ability-note">${escapeHtml(OSL_ENCLAVE_ROLE_ABILITY_ALLOWED_NOTE)}</p>`
    : reason.length > 0 && isKnownReason(reason)
      ? `<p class="osl-role-ability-reason">Denied: ${escapeHtml(reason)}</p>${
          detail.length > 0
            ? `<p class="osl-role-ability-detail">${escapeHtml(detail)}</p>`
            : ""
        }`
      : "";

  return `<li class="osl-role-ability-row" data-permission="${escapeHtml(row.permission)}" data-state="${escapeHtml(row.state)}" data-decided-by="${escapeHtml(row.decidedBy)}"${allowed ? "" : ` data-reason="${escapeHtml(reason)}"`}>
      <span class="osl-role-ability-permission">${escapeHtml(row.label)}</span>
      <span class="osl-role-ability-state" data-state="${escapeHtml(row.state)}">${allowed ? "Allowed" : "Denied"}</span>
      ${explanation}
    </li>`;
}

export function oslEnclaveRoleAbilityCounts(
  model: OslEnclaveRoleAbilityModel,
): { rows: number; allowed: number; denied: number } {
  const allowed = model.rows.filter((row) => row.state === "allowed").length;
  return {
    rows: model.rows.length,
    allowed,
    denied: model.rows.length - allowed,
  };
}

/**
 * Denied rows the screen could not explain. The ability check fails on any
 * entry here, so removing the reason from a row cannot pass unnoticed.
 */
export function oslEnclaveRoleAbilityRowsWithoutReason(
  model: OslEnclaveRoleAbilityModel,
): string[] {
  return model.rows
    .filter((row) => row.state !== "allowed")
    .filter(
      (row) =>
        row.reason.trim().length === 0 ||
        !isKnownReason(row.reason.trim()) ||
        row.detail.trim().length === 0,
    )
    .map((row) => row.permission);
}

export function oslEnclaveRoleAbilityViewMarkup(
  model: OslEnclaveRoleAbilityModel,
): string {
  const counts = oslEnclaveRoleAbilityCounts(model);
  const rows = model.rows.map((row) => abilityRow(row)).join("");
  return `<section class="osl-role-ability" aria-label="What ${escapeHtml(model.roleName)} can actually do">
    <header class="osl-role-ability-header">
      <h2>What ${escapeHtml(model.roleName)} can actually do</h2>
      <p class="osl-role-ability-scope">In #${escapeHtml(model.channelName)}, after channel overrides, keys, relay timeouts and this role's limits.</p>
      <p class="osl-role-ability-counts"><span class="osl-role-ability-count-allowed" data-count="${counts.allowed}">${counts.allowed} allowed</span><span class="osl-role-ability-count-denied" data-count="${counts.denied}">${counts.denied} denied</span></p>
    </header>
    <ul class="osl-role-ability-rows" data-row-count="${counts.rows}">${rows}</ul>
  </section>`;
}
