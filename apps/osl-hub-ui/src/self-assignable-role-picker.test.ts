import { describe, expect, it } from "vitest";

import {
  PickYourRolesController,
  pickerRoles,
  pickYourRolesMarkup,
  type PickYourRolesState,
} from "./self-assignable-role-picker";

const fixture: PickYourRolesState = {
  roles: [
    { id: "owner", name: "Owner", colour: "#ffffff", icon: "★", selfAssignable: false },
    { id: "announcements", name: "Announcements", colour: "#f2b84b", icon: "◉", selfAssignable: true },
    { id: "moderator", name: "Moderator", colour: "#ef626b", icon: "◆", selfAssignable: false },
    { id: "events", name: "Event host", colour: "#8b5cf6", icon: "✦", selfAssignable: true },
    { id: "member", name: "Member", colour: "#99aab5", icon: "●", selfAssignable: false },
  ],
  memberRoleIds: [],
};

describe("TASK 5001 pick-your-roles screen", () => {
  it("shows exactly the two self-assignable roles, including their colour and icon", () => {
    const visible = pickerRoles(fixture.roles);
    const markup = pickYourRolesMarkup(fixture);

    expect(visible).toHaveLength(2);
    expect((markup.match(/data-pickable-role-id=/gu) ?? [])).toHaveLength(2);
    expect(markup).toContain("Announcements");
    expect(markup).toContain("#f2b84b");
    expect(markup).toContain("◉");
    expect(markup).toContain("Event host");
    expect(markup).toContain("#8b5cf6");
    expect(markup).toContain("✦");
    expect(markup).not.toContain('data-pickable-role-id="moderator"');
    console.log("TASK5001_UI fixture_roles=5 shown_rows=2 hidden_non_self_assignable=3");
  });

  it("takes exactly one role then drops exactly one role immediately", () => {
    const picker = new PickYourRolesController(fixture);
    const before = picker.snapshot().memberRoleIds.length;
    expect(picker.take("announcements")).toBe(true);
    const afterTake = picker.snapshot().memberRoleIds.length;
    expect(picker.drop("announcements")).toBe(true);
    const afterDrop = picker.snapshot().memberRoleIds.length;

    expect(afterTake - before).toBe(1);
    expect(afterTake - afterDrop).toBe(1);
    console.log(`TASK5001_UI take_delta=${afterTake - before} drop_delta=${afterTake - afterDrop} final_member_roles=${afterDrop}`);
  });

  it("refuses a non-self-assignable role at the interaction boundary", () => {
    const picker = new PickYourRolesController(fixture);
    expect(picker.take("moderator")).toBe(false);
    expect(picker.snapshot().memberRoleIds).toEqual([]);
    console.log("TASK5001_UI non_self_assignable=moderator appears=false take_allowed=false");
  });
});
