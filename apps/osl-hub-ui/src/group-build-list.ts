import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

// TASK 0179. The group/server verification tick (0175's per-person tick,
// applied to a group or server context) opens the filtered build list the
// backend already produces (0178: `list_group_verification_build_entries`
// returns only two-way members). This module owns that connection: the tick's
// click handler fetches the entries and opens the list; the list draws one
// label per member and refuses to draw a member whose build state it does
// not recognize, so a record that lost its label fails closed instead of
// rendering blank.

/** Mirrors `GroupVerificationBuildListEntryDto` (apps/osl-hub/src/security.rs). */
export interface GroupVerificationBuildEntry {
  memberId: string;
  /** Always `two-way` for entries the backend returns (it filters the rest). */
  twoWayState: string;
  /** `unmodified` or `modified`. Anything else is refused, not guessed. */
  buildState: string;
}

export interface GroupBuildListModel {
  open: boolean;
  groupId: string | null;
  entries: GroupVerificationBuildEntry[];
}

export function blankGroupBuildListModel(): GroupBuildListModel {
  return { open: false, groupId: null, entries: [] };
}

/** The click handler behind the group/server verification tick: opens the
 * list already filtered (server-side, 0178) to two-way members. */
export function openGroupVerificationBuildList(
  groupId: string,
  entries: GroupVerificationBuildEntry[],
): GroupBuildListModel {
  return { open: true, groupId, entries };
}

export function closeGroupVerificationBuildList(): GroupBuildListModel {
  return blankGroupBuildListModel();
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

/** The tick only ever draws for two-way members (0175's rule): a member with
 * anything else does not reach this module at all (0178 filters it out
 * server-side), so there is nothing left here to gate on `twoWayState`. */
export function groupVerificationTickMarkup(entries: GroupVerificationBuildEntry[]): string {
  if (entries.length === 0) return "";
  return `<button class="verification-tick group-verification-tick" data-verification-tick="two-way" data-osl-group-tick="open" type="button" aria-label="Open verified build list">✓</button>`;
}

function buildStateLabel(buildState: string): string {
  if (buildState === "unmodified") return "Unmodified";
  if (buildState === "modified") return "Modified";
  // Fail closed: an entry with no recognized build state carries no label,
  // rather than rendering silently blank or guessing a default.
  throw new Error(`OSL group build list entry has no recognized build state: "${buildState}"`);
}

function groupBuildListEntryMarkup(entry: GroupVerificationBuildEntry): string {
  return `<li class="group-build-entry" data-osl-group-build-member="${escapeHtml(entry.memberId)}"><span class="group-build-member-id">${escapeHtml(entry.memberId)}</span><span class="group-build-label" data-osl-build-label="${entry.buildState}">${buildStateLabel(entry.buildState)}</span></li>`;
}

export function groupBuildListMarkup(model: GroupBuildListModel): string {
  if (!model.open) return "";
  const rows = model.entries.map(groupBuildListEntryMarkup).join("");
  return `<aside class="group-build-list" aria-labelledby="group-build-list-title">
    <header><h2 id="group-build-list-title">Verified build list</h2></header>
    <ul class="group-build-entries" aria-label="Verified members">${rows}</ul>
  </aside>`;
}

/** Real wiring for the tick's click handler: fetch the backend's already-
 * filtered entries for this group, then open the list with them. */
export async function oslOnGroupVerificationTickClick(
  app: string,
  localAccount: string,
  groupId: string,
): Promise<GroupBuildListModel | null> {
  if (!isTauriRuntime()) return null;
  const raw = await invoke("list_group_verification_build_entries", {
    app,
    localAccount,
    groupId,
  });
  if (!Array.isArray(raw)) return null;
  const entries: GroupVerificationBuildEntry[] = [];
  for (const item of raw) {
    if (
      !item
      || typeof item !== "object"
      || typeof (item as { memberId?: unknown }).memberId !== "string"
      || typeof (item as { twoWayState?: unknown }).twoWayState !== "string"
      || typeof (item as { buildState?: unknown }).buildState !== "string"
    ) {
      return null;
    }
    entries.push(item as GroupVerificationBuildEntry);
  }
  return openGroupVerificationBuildList(groupId, entries);
}
