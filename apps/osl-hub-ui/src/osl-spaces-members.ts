/**
 * The local roster is deliberately projected to names alone before it reaches
 * the view.  A Space member list is a membership surface, not an activity
 * surface: it must not acquire presence, typing, receipt, or time data.
 */
export interface KnownSpaceMember {
  readonly memberId: string;
  readonly displayName: string;
}

export interface SpaceMemberListRow {
  readonly memberId: string;
  readonly label: string;
}

export interface SpaceMemberList {
  readonly label: string;
  readonly rows: readonly SpaceMemberListRow[];
}

const MAX_MEMBER_NAME_LENGTH = 256;

function assertKnownMember(member: KnownSpaceMember): void {
  if (typeof member.memberId !== "string" || !member.memberId
    || typeof member.displayName !== "string" || !member.displayName.trim()
    || member.displayName.length > MAX_MEMBER_NAME_LENGTH) {
    throw new Error("Invalid Space member");
  }
}

/**
 * Creates the only data shape the member-list view is allowed to consume.
 * Do not extend this type with activity signals; those signals do not belong
 * on a Space membership surface.
 */
export function projectKnownSpaceMembers(members: readonly KnownSpaceMember[]): SpaceMemberList {
  const memberIds = new Set<string>();
  const rows = members.map((member) => {
    assertKnownMember(member);
    if (memberIds.has(member.memberId)) throw new Error("Duplicate Space member");
    memberIds.add(member.memberId);
    return { memberId: member.memberId, label: member.displayName };
  });
  return { label: "Members", rows };
}

/** Renders local membership only; the caller never supplies activity data. */
export function renderSpaceMemberList(
  document: Document,
  members: readonly KnownSpaceMember[],
): HTMLElement {
  const listModel = projectKnownSpaceMembers(members);
  const root = document.createElement("section");
  root.className = "osl-spaces-members";
  root.setAttribute("aria-label", listModel.label);

  const heading = document.createElement("h2");
  heading.textContent = listModel.label;
  root.append(heading);

  const list = document.createElement("ul");
  for (const row of listModel.rows) {
    const item = document.createElement("li");
    item.dataset.memberId = row.memberId;
    item.textContent = row.label;
    list.append(item);
  }
  root.append(list);
  return root;
}
