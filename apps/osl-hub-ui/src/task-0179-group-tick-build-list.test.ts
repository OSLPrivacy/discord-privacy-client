import { describe, expect, it } from "vitest";
import {
  blankGroupBuildListModel,
  groupBuildListMarkup,
  groupVerificationTickMarkup,
  openGroupVerificationBuildList,
  type GroupVerificationBuildEntry,
} from "./group-build-list";

// Five two-way members (0178 already filters out anything less than
// two-way before this UI ever sees it): two unmodified, three modified.
const GROUP_ID = "task-0179-group";
const fiveMemberFixture: GroupVerificationBuildEntry[] = [
  { memberId: "member-1", twoWayState: "two-way", buildState: "unmodified" },
  { memberId: "member-2", twoWayState: "two-way", buildState: "modified" },
  { memberId: "member-3", twoWayState: "two-way", buildState: "unmodified" },
  { memberId: "member-4", twoWayState: "two-way", buildState: "modified" },
  { memberId: "member-5", twoWayState: "two-way", buildState: "modified" },
];

function labelCounts(markup: string): { unmodified: number; modified: number; total: number } {
  const unmodified = [...markup.matchAll(/data-osl-build-label="unmodified"/gu)].length;
  const modified = [...markup.matchAll(/data-osl-build-label="modified"/gu)].length;
  const total = [...markup.matchAll(/data-osl-build-label="/gu)].length;
  return { unmodified, modified, total };
}

function memberCount(markup: string): number {
  return [...markup.matchAll(/data-osl-group-build-member="/gu)].length;
}

describe("TASK0179 group tick opens the filtered build list", () => {
  it("the tick click opens exactly 5 members: 2 unmodified, 3 modified, one label each", () => {
    // Closed state: no tick markup drawn without entries to open into.
    const closedModel = blankGroupBuildListModel();
    expect(groupBuildListMarkup(closedModel)).toBe("");
    expect(groupVerificationTickMarkup([])).toBe("");

    // The tick is present once there is a filtered list behind it…
    const tick = groupVerificationTickMarkup(fiveMemberFixture);
    expect(tick).toContain('data-osl-group-tick="open"');

    // …and clicking it (openGroupVerificationBuildList) opens the list.
    const openedModel = openGroupVerificationBuildList(GROUP_ID, fiveMemberFixture);
    expect(openedModel.open).toBe(true);

    const markup = groupBuildListMarkup(openedModel);
    const counts = labelCounts(markup);
    const members = memberCount(markup);

    console.log(`TASK0179 members=${members} unmodified=${counts.unmodified} modified=${counts.modified} labels_total=${counts.total}`);

    expect(members).toBe(5);
    expect(counts.unmodified).toBe(2);
    expect(counts.modified).toBe(3);
    // Every member carries exactly one label: total labels == member count.
    expect(counts.total).toBe(members);
  });

  it("a fixture member given no label makes the check fail", () => {
    const brokenFixture: GroupVerificationBuildEntry[] = [
      ...fiveMemberFixture.slice(0, 4),
      { memberId: "member-5", twoWayState: "two-way", buildState: "" },
    ];
    const openedModel = openGroupVerificationBuildList(GROUP_ID, brokenFixture);

    let threw = false;
    try {
      groupBuildListMarkup(openedModel);
    } catch (error) {
      threw = true;
      console.log(`TASK0179 unlabeled_member_rejected=true message=${(error as Error).message}`);
    }

    expect(threw).toBe(true);
  });
});
