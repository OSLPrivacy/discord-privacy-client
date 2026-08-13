/**
 * TASK 6818 — the sidebar place menu that leaves a group or an enclave, and
 * the honest surface a departing member is left with.
 *
 * The menu is the only entry point to departure. `activateLeave` deliberately
 * works off the *rendered markup* rather than the model it was rendered from:
 * if the item is not in the markup, or is rendered disabled, no departure
 * request comes out of this module and the engine never hears about it.
 *
 * The two facts this surface must never fudge:
 *
 *  - leaving does not delete the copy of the messages already on this device,
 *    and deleting that copy is a separate choice offered in the same menu;
 *  - leaving cannot delete anything from the devices of the members who stay.
 */

export type PlaceKind = "group" | "enclave";

export interface PlaceMenuModel {
  readonly handle: string;
  readonly kind: PlaceKind;
  readonly name: string;
  readonly leave_action: string;
  readonly leave_enabled: boolean;
  readonly refusal_code: string | null;
  readonly authority_verdict: string;
  readonly held_authority_role_labels: readonly string[];
  readonly other_authority_holders: readonly string[];
  readonly retained_messages: number;
}

export interface SidebarModel {
  readonly point: string;
  readonly leaver: string;
  readonly places: readonly PlaceMenuModel[];
}

export interface DepartedPlaceModel {
  readonly handle: string;
  readonly name: string;
  readonly kind: PlaceKind;
  readonly in_place_list: boolean;
  readonly retained_messages: number;
  readonly deletion_choice_applied: boolean;
  readonly left_at_epoch: number;
}

export interface LeaverViewModel {
  readonly leaver: string;
  readonly place_list: readonly string[];
  readonly departed: readonly DepartedPlaceModel[];
}

export const PLACE_DEPARTURE_COPY = {
  leaveGroup: "Leave group",
  leaveEnclave: "Leave enclave",
  deleteCopy: "Also delete my copy of the messages",
  lastAuthorityHeading: "Transfer ownership before you leave",
  retainedHeading: "Your copy is still on this device",
  retainedBody:
    "Leaving removed you from this place. It did not delete the messages you already had — they are still on this device. Deleting them is a separate choice.",
  deletedBody:
    "You left and took the separate choice to delete your copy, so none of its messages are still on this device.",
  othersKeepTheirs:
    "Members who stay keep their own copies. Leaving cannot delete a message from someone else's device.",
} as const;

export function leaveLabel(kind: PlaceKind): string {
  return kind === "group" ? PLACE_DEPARTURE_COPY.leaveGroup : PLACE_DEPARTURE_COPY.leaveEnclave;
}

export function leaveActionFor(kind: PlaceKind): string {
  return kind === "group" ? "leave-group" : "leave-enclave";
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The honest reason a leave item is disabled, naming the authority, not a label. */
export function lastAuthorityRefusal(place: PlaceMenuModel): string {
  const kindWord = place.kind === "group" ? "group" : "enclave";
  return `You are the only member holding the role that governs ${place.name}. Transfer that role to another member before you leave this ${kindWord}.`;
}

/** One place row, with its menu open. */
export function placeMenuMarkup(place: PlaceMenuModel): string {
  const action = leaveActionFor(place.kind);
  const label = leaveLabel(place.kind);
  const enabled = place.leave_enabled;
  const refusal = enabled
    ? ""
    : `<p class="place-menu-refusal" data-place-refusal-for="${escapeHtml(place.handle)}"><strong>${
        PLACE_DEPARTURE_COPY.lastAuthorityHeading
      }</strong> ${escapeHtml(lastAuthorityRefusal(place))}</p>`;
  const leaveAttributes = [
    `class="place-menu-item place-menu-item--leave"`,
    `role="menuitem"`,
    `type="button"`,
    `data-place-menu-action="${escapeHtml(action)}"`,
    `data-place-id="${escapeHtml(place.handle)}"`,
    `data-leave-enabled="${enabled ? "true" : "false"}"`,
    enabled ? "" : `data-leave-refusal="${escapeHtml(place.refusal_code ?? "refused")}"`,
    enabled ? "" : `disabled aria-disabled="true"`,
  ]
    .filter(Boolean)
    .join(" ");
  const deleteCopy = `<button class="place-menu-item place-menu-item--delete-copy" role="menuitemcheckbox" type="button" data-place-menu-delete-copy="${escapeHtml(
    place.handle,
  )}" data-place-id="${escapeHtml(place.handle)}" aria-checked="false">${
    PLACE_DEPARTURE_COPY.deleteCopy
  }</button>`;
  return `<li class="place-row" data-place-row="${escapeHtml(place.handle)}" data-place-kind="${
    place.kind
  }"><button class="place-row-open" type="button" data-place-open="${escapeHtml(
    place.handle,
  )}">${escapeHtml(place.name)}</button><div class="place-menu" role="menu" aria-label="${escapeHtml(
    place.name,
  )} menu" data-place-menu="${escapeHtml(
    place.handle,
  )}"><button class="place-menu-item" role="menuitem" type="button" data-place-menu-action="mark-read" data-place-id="${escapeHtml(
    place.handle,
  )}">Mark as read</button><button ${leaveAttributes}>${escapeHtml(
    label,
  )}</button>${deleteCopy}</div>${refusal}</li>`;
}

/** The whole place rail with every place's menu rendered open. */
export function placeSidebarMarkup(model: SidebarModel): string {
  const rows = model.places.map(placeMenuMarkup).join("");
  return `<nav class="place-rail" aria-label="Your places" data-place-rail-point="${escapeHtml(
    model.point,
  )}"><ul class="place-rail-list">${rows}</ul></nav>`;
}

// ---------------------------------------------------------------------------
// Reading the rendered markup back
// ---------------------------------------------------------------------------

export interface ParsedElement {
  readonly tag: string;
  readonly attributes: Readonly<Record<string, string>>;
  readonly text: string;
}

const ELEMENT_PATTERN = /<(button|p)\b([^>]*)>([\s\S]*?)<\/\1>/g;
const ATTRIBUTE_PATTERN = /([a-zA-Z-]+)(?:="([^"]*)")?/g;

/** A deliberately small reader over the markup this module produced. */
export function parseElements(markup: string): ParsedElement[] {
  const out: ParsedElement[] = [];
  for (const match of markup.matchAll(ELEMENT_PATTERN)) {
    const attributes: Record<string, string> = {};
    for (const attribute of match[2].matchAll(ATTRIBUTE_PATTERN)) {
      attributes[attribute[1]] = attribute[2] ?? "";
    }
    out.push({
      tag: match[1],
      attributes,
      text: match[3].replace(/<[^>]*>/g, "").trim(),
    });
  }
  return out;
}

export interface LeaveActivation {
  readonly place: string;
  readonly menu_action: string;
  readonly delete_local_history: boolean;
  readonly rendered_enabled: boolean;
  readonly rendered_label: string;
}

export type LeaveActivationResult =
  | { readonly ok: true; readonly activation: LeaveActivation }
  | { readonly ok: false; readonly reason: string };

/**
 * Activates the leave item for one place out of the rendered markup.
 *
 * `deleteLocalCopy` is only honoured when the menu actually renders the
 * separate deletion choice; a menu that dropped it cannot silently delete a
 * member's local copy on their behalf.
 */
export function activateLeave(
  markup: string,
  placeHandle: string,
  options: { readonly deleteLocalCopy?: boolean } = {},
): LeaveActivationResult {
  const elements = parseElements(markup);
  const item = elements.find(
    (element) =>
      element.tag === "button" &&
      element.attributes["data-place-id"] === placeHandle &&
      (element.attributes["data-place-menu-action"] ?? "").startsWith("leave-"),
  );
  if (!item) return { ok: false, reason: "no-leave-item" };
  const enabled = item.attributes["data-leave-enabled"] === "true";
  if (!enabled) {
    return { ok: false, reason: item.attributes["data-leave-refusal"] ?? "leave-item-disabled" };
  }
  const wantsDelete = options.deleteLocalCopy === true;
  if (wantsDelete) {
    const toggle = elements.find(
      (element) =>
        element.tag === "button" && element.attributes["data-place-menu-delete-copy"] === placeHandle,
    );
    if (!toggle) return { ok: false, reason: "no-delete-copy-item" };
  }
  return {
    ok: true,
    activation: {
      place: placeHandle,
      menu_action: item.attributes["data-place-menu-action"] ?? "",
      delete_local_history: wantsDelete,
      rendered_enabled: true,
      rendered_label: item.text,
    },
  };
}

// ---------------------------------------------------------------------------
// The leaver's surface after departure
// ---------------------------------------------------------------------------

function departedRowMarkup(place: DepartedPlaceModel): string {
  const body = place.deletion_choice_applied
    ? PLACE_DEPARTURE_COPY.deletedBody
    : PLACE_DEPARTURE_COPY.retainedBody;
  const heading = place.deletion_choice_applied
    ? `Your copy of ${place.name} was deleted`
    : PLACE_DEPARTURE_COPY.retainedHeading;
  const count = place.retained_messages === 1 ? "1 message" : `${place.retained_messages} messages`;
  return `<li class="departed-place" data-departed-place="${escapeHtml(
    place.handle,
  )}" data-departed-kind="${place.kind}" data-in-place-list="${
    place.in_place_list ? "true" : "false"
  }" data-retained-messages="${place.retained_messages}" data-deletion-choice="${
    place.deletion_choice_applied ? "applied" : "not-applied"
  }"><h3>${escapeHtml(place.name)}</h3><p class="departed-place-heading"><strong>${escapeHtml(
    heading,
  )}</strong></p><p class="departed-place-body">${escapeHtml(body)}</p><p class="departed-place-count">${escapeHtml(
    count,
  )} from ${escapeHtml(place.name)} ${
    place.retained_messages === 1 ? "is" : "are"
  } on this device.</p><p class="departed-place-others">${escapeHtml(
    PLACE_DEPARTURE_COPY.othersKeepTheirs,
  )}</p></li>`;
}

/**
 * The leaver's own view: the places still in their list, and an honest row per
 * place they left saying what is and is not still on this device.
 */
export function leaverViewMarkup(view: LeaverViewModel): string {
  const remaining = view.place_list
    .map(
      (handle) =>
        `<li class="place-row" data-place-row="${escapeHtml(handle)}">${escapeHtml(handle)}</li>`,
    )
    .join("");
  const departed = view.departed.map(departedRowMarkup).join("");
  return `<section class="leaver-view" data-leaver="${escapeHtml(
    view.leaver,
  )}"><h2>Your places</h2><ul class="place-rail-list" data-place-list>${remaining}</ul><h2>Places you left</h2><ul class="departed-places" data-departed-places>${departed}</ul></section>`;
}

/** Is this place still shown in the leaver's list? Read from the markup. */
export function placeListFromMarkup(markup: string): string[] {
  const list = markup.match(/<ul class="place-rail-list" data-place-list>([\s\S]*?)<\/ul>/);
  if (!list) return [];
  return [...list[1].matchAll(/data-place-row="([^"]+)"/g)].map((match) => match[1]);
}
