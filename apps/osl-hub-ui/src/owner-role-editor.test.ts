import { describe, expect, it } from "vitest";

import { OWNER_ROLE_PERMISSIONS, OwnerRoleEditor, ownerRoleEditorMarkup } from "./owner-role-editor";

describe("owner role editor fixture", () => {
  it("creates, duplicates, orders, configures, saves, and reopens a custom role", () => {
    const fixture = new OwnerRoleEditor();
    fixture.create("Safety team");
    fixture.rename("Safety lead");
    fixture.setColor("#e67e22");
    fixture.setIcon("🛡️");
    fixture.setHoisted(true);
    fixture.setMentionRule("role");
    fixture.setSelfAssignable(true);
    fixture.setAutoGrant(true);
    fixture.setExpiryDays(30);
    fixture.setTemplate("custom");
    fixture.duplicate();
    fixture.moveActiveAboveMember();
    OWNER_ROLE_PERMISSIONS.slice(0, 12).forEach(([permission]) => fixture.setPermission(permission, true));
    fixture.setLimits({ slowModeSeconds: 15, muteCapMinutes: 60, actionBudget: 20 });
    fixture.save();

    const reopened = fixture.reopened();
    const selected = reopened.roles.find((role) => role.id === reopened.selectedRoleId);
    const customRoles = reopened.roles.filter((role) => !role.builtIn);
    const memberIndex = reopened.roles.findIndex((role) => role.id === "member");
    const activeIndex = reopened.roles.findIndex((role) => role.id === reopened.selectedRoleId);
    const markup = ownerRoleEditorMarkup(reopened);

    expect(customRoles).toHaveLength(2);
    expect(activeIndex).toBeLessThan(memberIndex);
    expect(selected?.name).toBe("Safety lead copy");
    expect(selected?.color).toBe("#e67e22");
    expect(selected?.icon).toBe("🛡️");
    expect(selected?.permissions).toHaveLength(12);
    expect(selected?.limits).toEqual({ slowModeSeconds: 15, muteCapMinutes: 60, actionBudget: 20 });
    expect((markup.match(/data-enforcement-tag=/gu) ?? [])).toHaveLength(40);
    expect((markup.match(/data-owner-role-permission="[^"]+" type="checkbox" checked/gu) ?? [])).toHaveLength(12);
    expect(markup).toContain("Move above MEMBER");
    console.log(`TASK4879 roles=${customRoles.length} checked_permissions=${selected?.permissions.length ?? 0} enforcement_tags=${(markup.match(/data-enforcement-tag=/gu) ?? []).length} limits=3 order=above_MEMBER`);
  });

  it("goes red when enforcement tags are hidden", () => {
    const fixture = new OwnerRoleEditor();
    fixture.create("Check");
    const hidden = ownerRoleEditorMarkup(fixture.snapshot(), false);
    expect((hidden.match(/data-enforcement-tag=/gu) ?? [])).not.toHaveLength(40);
    expect((hidden.match(/data-enforcement-tag=/gu) ?? [])).toHaveLength(0);
    console.log("TASK4879 hidden_enforcement_tags=0");
  });
});
