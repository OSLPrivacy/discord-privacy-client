export interface WhitelistDropdownScope {
  kind: "dm" | "group" | "channel" | "space";
  storageKey: string;
}

export interface WhitelistDropdownPerson {
  personId: string;
  alias: string | null;
  whitelistedScopes: readonly WhitelistDropdownScope[];
}

export interface WhitelistDropdownModel {
  open: boolean;
  people: readonly WhitelistDropdownPerson[];
  activePersonId: string | null;
  activeScopeApproved: boolean;
  busy: boolean;
  groupStorageKey?: string | null;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function isPersonTicked(
  person: WhitelistDropdownPerson,
  activePersonId: string | null,
  activeScopeApproved: boolean,
  groupStorageKey: string | null,
): boolean {
  if (activePersonId === person.personId && activeScopeApproved) return true;
  return person.whitelistedScopes.some((scope) =>
    groupStorageKey ? scope.storageKey === groupStorageKey : scope.kind === "group"
  );
}

export function whitelistDropdownMarkup(model: WhitelistDropdownModel): string {
  if (!model.open) return "";
  const rows = model.people.length
    ? model.people.map((person) => {
      const name = person.alias ?? "Unnamed friend";
      const checked = isPersonTicked(
        person,
        model.activePersonId,
        model.activeScopeApproved,
        model.groupStorageKey ?? null,
      );
      return `<label class="whitelist-dropdown-row" data-whitelist-dropdown-row="${escapeHtml(person.personId)}"><input type="checkbox" data-whitelist-person-checkbox="${escapeHtml(person.personId)}" ${checked ? "checked " : ""}${model.busy ? "disabled" : ""}/><span><strong>${escapeHtml(name)}</strong><small>${checked ? "Whitelisted" : "Not whitelisted"}</small></span></label>`;
    }).join("")
    : `<div class="empty-state compact"><strong>No group people</strong><p>Verified people appear here when OSL can identify this group.</p></div>`;
  return `<div class="whitelist-roster-dropdown" id="whitelist-roster-dropdown" role="menu" aria-label="Group whitelist"><div class="whitelist-dropdown-list">${rows}</div></div>`;
}
