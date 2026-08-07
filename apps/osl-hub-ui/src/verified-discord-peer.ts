export type ActiveVerifiedDiscordPeerContext = {
  contextToken: string;
  personId: string;
};

export type ActiveVerifiedDiscordPeerPerson = {
  personId: string;
  safetyNumberVerified: boolean;
  pendingKeyChange: boolean;
};

export function activeVerifiedDiscordPeer<
  Context extends ActiveVerifiedDiscordPeerContext,
  Person extends ActiveVerifiedDiscordPeerPerson,
>(
  context: Context | null | undefined,
  activeContextToken: string | null | undefined,
  people: readonly Person[],
): { context: Context; person: Person } | null {
  if (!context || context.contextToken !== activeContextToken) return null;
  const person = people.find((candidate) => candidate.personId === context.personId
    && candidate.safetyNumberVerified
    && !candidate.pendingKeyChange);
  return person ? { context, person } : null;
}
