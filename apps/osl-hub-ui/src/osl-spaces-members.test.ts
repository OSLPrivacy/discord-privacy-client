import { describe, expect, it } from "vitest";
import {
  projectKnownSpaceMembers,
  renderSpaceMemberList,
  type KnownSpaceMember,
} from "./osl-spaces-members";

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

describe("Space member list", () => {
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
    ])).toThrow(/Duplicate Space member/u);
    expect(() => projectKnownSpaceMembers([{ memberId: "member-alice", displayName: "  " }]))
      .toThrow(/Invalid Space member/u);
  });
});
