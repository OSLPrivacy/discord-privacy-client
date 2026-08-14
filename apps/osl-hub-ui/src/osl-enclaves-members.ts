import "./osl-enclaves-members.css";

/**
 * The local roster is deliberately projected to names alone before it reaches
 * the view.  A Enclave member list is a membership surface, not an activity
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

export const ENCLAVE_REMOVAL_DELAY_WARNING = "Removal takes time and is not immediate.";

export interface EnclaveRemovalProgress {
  readonly jobId: string;
  readonly completed: number;
  readonly remaining: number;
  readonly status: "waiting-confirmation" | "running" | "complete";
}

export interface EnclaveRemovalView {
  readonly targetMemberId: string;
  readonly enclaveMemberCount: number;
  readonly measuredWarningThreshold: number;
  readonly progress: EnclaveRemovalProgress | null;
}

const MAX_MEMBER_NAME_LENGTH = 256;

function assertKnownMember(member: KnownSpaceMember): void {
  if (typeof member.memberId !== "string" || !member.memberId
    || typeof member.displayName !== "string" || !member.displayName.trim()
    || member.displayName.length > MAX_MEMBER_NAME_LENGTH) {
    throw new Error("Invalid Enclave member");
  }
}

/**
 * Creates the only data shape the member-list view is allowed to consume.
 * Do not extend this type with activity signals; those signals do not belong
 * on a Enclave membership surface.
 */
export function projectKnownSpaceMembers(members: readonly KnownSpaceMember[]): SpaceMemberList {
  const memberIds = new Set<string>();
  const rows = members.map((member) => {
    assertKnownMember(member);
    if (memberIds.has(member.memberId)) throw new Error("Duplicate Enclave member");
    memberIds.add(member.memberId);
    return { memberId: member.memberId, label: member.displayName };
  });
  return { label: "Members", rows };
}

/** Renders local membership only; the caller never supplies activity data. */
export function renderSpaceMemberList(
  document: Document,
  members: readonly KnownSpaceMember[],
  removal: EnclaveRemovalView | null = null,
): HTMLElement {
  const listModel = projectKnownSpaceMembers(members);
  const root = document.createElement("section");
  root.className = "osl-enclaves-members";
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

  if (removal) {
    if (!Number.isSafeInteger(removal.enclaveMemberCount) || removal.enclaveMemberCount < 1
      || !Number.isSafeInteger(removal.measuredWarningThreshold) || removal.measuredWarningThreshold < 1
      || !members.some((member) => member.memberId === removal.targetMemberId)) {
      throw new Error("Invalid Enclave removal state");
    }
    const panel = document.createElement("section");
    panel.className = "osl-enclave-removal";
    panel.setAttribute("aria-label", "Remove member");

    if (removal.enclaveMemberCount >= removal.measuredWarningThreshold) {
      const warning = document.createElement("p");
      warning.className = "osl-enclave-removal-delay";
      warning.textContent = ENCLAVE_REMOVAL_DELAY_WARNING;
      panel.append(warning);
    }

    if (removal.progress) {
      const { completed, remaining } = removal.progress;
      if (!Number.isSafeInteger(completed) || completed < 0
        || !Number.isSafeInteger(remaining) || remaining < 0
        || removal.progress.status === "complete" && remaining !== 0) {
        throw new Error("Invalid Enclave removal progress");
      }
      const progress = document.createElement("progress");
      progress.setAttribute("value", String(completed));
      progress.setAttribute("max", String(completed + remaining));
      progress.setAttribute("aria-label", `${completed} successor authorities complete, ${remaining} remaining`);
      panel.append(progress);

      const progressText = document.createElement("p");
      progressText.textContent = `${completed} complete, ${remaining} remaining`;
      panel.append(progressText);
    }

    const confirm = document.createElement("button");
    confirm.setAttribute("type", "button");
    confirm.setAttribute("data-confirm-enclave-removal", removal.targetMemberId);
    confirm.textContent = "Confirm removal";
    panel.append(confirm);
    root.append(panel);
  }
  return root;
}
