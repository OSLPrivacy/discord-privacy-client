import { describe, expect, it } from "vitest";
import {
  ENCLAVE_REMOVAL_DELAY_WARNING,
  projectKnownSpaceMembers,
  renderSpaceMemberList,
  type KnownSpaceMember,
} from "./osl-enclaves-members";

class TestElement {
  className = "";
  textContent = "";
  readonly children: TestElement[] = [];
  readonly attributes = new Map<string, string>();
  readonly dataset: Record<string, string> = {};

  constructor(readonly tagName: string) {}

  append(...children: TestElement[]): void {
    this.children.push(...children);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }
}

function testDocument(): Document {
  return {
    createElement: (tagName: string) => new TestElement(tagName),
  } as unknown as Document;
}

const members: readonly KnownSpaceMember[] = [
  { memberId: "member-alice", displayName: "Alice" },
  { memberId: "member-bob", displayName: "Bob" },
];

describe("Enclave member list", () => {
  it("renders only local membership names, even when an untyped caller includes activity signals", () => {
    const activityBearingInput = members.map((member, index) => ({
      ...member,
      online: index === 0,
      lastSeen: "just now",
    }));

    const root = renderSpaceMemberList(testDocument(), activityBearingInput);
    const rendered = root as unknown as TestElement;
    const list = rendered.children[1];

    expect(rendered.tagName).toBe("section");
    expect(rendered.attributes.get("aria-label")).toBe("Members");
    expect(rendered.children[0]).toMatchObject({ tagName: "h2", textContent: "Members" });
    expect(list.tagName).toBe("ul");
    expect(list.children).toEqual([
      expect.objectContaining({ tagName: "li", textContent: "Alice", dataset: { memberId: "member-alice" } }),
      expect.objectContaining({ tagName: "li", textContent: "Bob", dataset: { memberId: "member-bob" } }),
    ]);
    expect(JSON.stringify(rendered)).not.toMatch(/online|lastSeen|just now/u);
  });

  it("rejects duplicate or incomplete local roster records", () => {
    expect(() => projectKnownSpaceMembers([
      { memberId: "member-alice", displayName: "Alice" },
      { memberId: "member-alice", displayName: "Alice again" },
    ])).toThrow(/Duplicate Enclave member/u);
    expect(() => projectKnownSpaceMembers([{ memberId: "member-alice", displayName: "  " }]))
      .toThrow(/Invalid Enclave member/u);
  });

  it("warns before confirmation at measured N and renders observed fan-out progress", () => {
    const rendered = renderSpaceMemberList(testDocument(), members, {
      targetMemberId: "member-bob",
      enclaveMemberCount: 50,
      measuredWarningThreshold: 50,
      progress: { jobId: "job-1", completed: 17, remaining: 32, status: "running" },
    }) as unknown as TestElement;
    const panel = rendered.children[2];

    expect(panel.children.map((child) => child.textContent)).toEqual([
      ENCLAVE_REMOVAL_DELAY_WARNING,
      "",
      "17 complete, 32 remaining",
      "Confirm removal",
    ]);
    expect(panel.children[0].className).toBe("osl-enclave-removal-delay");
    expect(panel.children[1].tagName).toBe("progress");
    expect(panel.children[1].attributes.get("value")).toBe("17");
    expect(panel.children[1].attributes.get("max")).toBe("49");
    expect(panel.children[3].attributes.get("data-confirm-enclave-removal")).toBe("member-bob");
  });

  it("does not show the delay warning below measured N", () => {
    const rendered = renderSpaceMemberList(testDocument(), members, {
      targetMemberId: "member-bob",
      enclaveMemberCount: 49,
      measuredWarningThreshold: 50,
      progress: null,
    }) as unknown as TestElement;
    expect(JSON.stringify(rendered)).not.toContain(ENCLAVE_REMOVAL_DELAY_WARNING);
    expect(rendered.children[2].children[0].textContent).toBe("Confirm removal");
  });
});
