export interface SharedNotebookInvite {
  version: 1;
  notebookId: string;
  capability: string;
}

export function parseSharedNotebookInvite(value: unknown): SharedNotebookInvite | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const invite = value as Record<string, unknown>;
  if (Object.keys(invite).length !== 3 || invite.version !== 1) return null;
  if (typeof invite.notebookId !== "string" || !/^[a-f0-9]{32}$/u.test(invite.notebookId)) return null;
  if (typeof invite.capability !== "string" || !/^[A-Za-z0-9_-]{43}$/u.test(invite.capability)) return null;
  return invite as unknown as SharedNotebookInvite;
}

export const sharedNotebooksAvailability = {
  available: false,
  reason: "Shared notebooks are fail-closed until an authenticated encrypted sync transport is enabled.",
} as const;
