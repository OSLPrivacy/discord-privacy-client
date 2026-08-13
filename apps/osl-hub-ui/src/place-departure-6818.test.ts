/**
 * TASK 6818 — the sidebar place menu, graded on the markup it actually renders.
 *
 * The engine-side check (`crates/place-departure/tests/task_6818_place_departure.rs`)
 * drives this same module through `scripts/render-place-departure-6818.ts`.
 * These tests hold the surface itself to the two things it must never fudge:
 * the leave item is the only way out of a place, and a disabled item cannot be
 * activated by anything this module hands back.
 */

import { describe, expect, it } from "vitest";

import {
  activateLeave,
  leaveActionFor,
  leaveLabel,
  leaverViewMarkup,
  parseElements,
  placeListFromMarkup,
  placeSidebarMarkup,
  PLACE_DEPARTURE_COPY,
  type PlaceMenuModel,
  type SidebarModel,
} from "./place-departure-6818";

function place(overrides: Partial<PlaceMenuModel> = {}): PlaceMenuModel {
  return {
    handle: "atlas-group",
    kind: "group",
    name: "Atlas Group",
    leave_action: "leave-group",
    leave_enabled: true,
    refusal_code: null,
    authority_verdict: "clear",
    held_authority_role_labels: [],
    other_authority_holders: [],
    retained_messages: 2,
    ...overrides,
  };
}

function sidebar(places: PlaceMenuModel[]): SidebarModel {
  return { point: "initial", leaver: "wren", places };
}

describe("the place menu", () => {
  it("renders exactly one leave item per place, worded for the kind", () => {
    const markup = placeSidebarMarkup(
      sidebar([
        place(),
        place({
          handle: "harbor-enclave",
          kind: "enclave",
          name: "Harbor Enclave",
          leave_action: "leave-enclave",
        }),
      ]),
    );
    const leaveItems = parseElements(markup).filter((element) =>
      (element.attributes["data-place-menu-action"] ?? "").startsWith("leave-"),
    );
    expect(leaveItems).toHaveLength(2);
    expect(leaveItems.map((item) => item.text)).toEqual(["Leave group", "Leave enclave"]);
    expect(leaveItems.map((item) => item.attributes["data-place-menu-action"])).toEqual([
      "leave-group",
      "leave-enclave",
    ]);
    expect(leaveLabel("group")).toBe("Leave group");
    expect(leaveLabel("enclave")).toBe("Leave enclave");
    expect(leaveActionFor("group")).toBe("leave-group");
    expect(leaveActionFor("enclave")).toBe("leave-enclave");
  });

  it("activates a leave out of the markup, carrying the rendered wording", () => {
    const markup = placeSidebarMarkup(sidebar([place()]));
    const result = activateLeave(markup, "atlas-group");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.activation).toMatchObject({
      place: "atlas-group",
      menu_action: "leave-group",
      delete_local_history: false,
      rendered_enabled: true,
      rendered_label: "Leave group",
    });
  });

  it("offers deleting the local copy as a separate choice, never as part of leaving", () => {
    const markup = placeSidebarMarkup(sidebar([place()]));
    expect(markup).toContain(PLACE_DEPARTURE_COPY.deleteCopy);
    expect(activateLeave(markup, "atlas-group").ok).toBe(true);
    const withDelete = activateLeave(markup, "atlas-group", { deleteLocalCopy: true });
    expect(withDelete.ok).toBe(true);
    if (withDelete.ok) expect(withDelete.activation.delete_local_history).toBe(true);

    // A menu that dropped the separate choice cannot delete anything on the
    // member's behalf.
    const stripped = markup.replace(
      /<button class="place-menu-item place-menu-item--delete-copy"[\s\S]*?<\/button>/g,
      "",
    );
    expect(activateLeave(stripped, "atlas-group", { deleteLocalCopy: true })).toEqual({
      ok: false,
      reason: "no-delete-copy-item",
    });
    // Leaving without the deletion choice still works on that same menu.
    expect(activateLeave(stripped, "atlas-group").ok).toBe(true);
  });

  it("disables the leave item for the last holder of the governing role, and names the reason", () => {
    const markup = placeSidebarMarkup(
      sidebar([
        place({
          handle: "foundry-group",
          name: "Foundry Group",
          leave_enabled: false,
          refusal_code: "last-authority-holder",
          authority_verdict: "last_holder",
          held_authority_role_labels: ["Warden"],
        }),
      ]),
    );
    expect(markup).toContain('data-leave-enabled="false"');
    expect(markup).toContain('data-leave-refusal="last-authority-holder"');
    expect(markup).toContain("disabled");
    expect(markup).toContain(PLACE_DEPARTURE_COPY.lastAuthorityHeading);
    // The refusal names the governing role, not a display label the place ships.
    expect(markup).toContain(
      "You are the only member holding the role that governs Foundry Group.",
    );
    expect(activateLeave(markup, "foundry-group")).toEqual({
      ok: false,
      reason: "last-authority-holder",
    });
  });

  it("hands back nothing for a place whose leave item was never rendered", () => {
    const markup = placeSidebarMarkup(sidebar([place()]));
    expect(activateLeave(markup, "harbor-enclave")).toEqual({ ok: false, reason: "no-leave-item" });
  });
});

describe("the leaver's own surface", () => {
  const view = {
    leaver: "wren",
    place_list: ["beacon-group"],
    departed: [
      {
        handle: "atlas-group",
        name: "Atlas Group",
        kind: "group" as const,
        in_place_list: false,
        retained_messages: 2,
        deletion_choice_applied: false,
        left_at_epoch: 5,
      },
      {
        handle: "foundry-group",
        name: "Foundry Group",
        kind: "group" as const,
        in_place_list: false,
        retained_messages: 0,
        deletion_choice_applied: true,
        left_at_epoch: 4,
      },
    ],
  };

  it("drops the place from the list and says honestly what is still on the device", () => {
    const markup = leaverViewMarkup(view);
    expect(placeListFromMarkup(markup)).toEqual(["beacon-group"]);
    expect(markup).toContain('data-departed-place="atlas-group"');
    expect(markup).toContain('data-in-place-list="false"');
    expect(markup).toContain('data-retained-messages="2"');
    expect(markup).toContain('data-deletion-choice="not-applied"');
    expect(markup).toContain(PLACE_DEPARTURE_COPY.retainedHeading);
    expect(markup).toContain(
      "It did not delete the messages you already had — they are still on this device.",
    );
    expect(markup).toContain("2 messages from Atlas Group are on this device.");
  });

  it("only claims a deletion where the separate choice was actually taken", () => {
    const markup = leaverViewMarkup(view);
    expect(markup.match(/data-deletion-choice="applied"/g)).toHaveLength(1);
    expect(markup).toContain("Your copy of Foundry Group was deleted");
    expect(markup).toContain("0 messages from Foundry Group are on this device.");
  });

  it("never claims that leaving removed anything from anyone else's device", () => {
    const markup = leaverViewMarkup(view);
    const occurrences = markup.match(/Members who stay keep their own copies\./g);
    expect(occurrences).toHaveLength(view.departed.length);
    expect(markup).not.toContain("deleted for everyone");
  });
});
