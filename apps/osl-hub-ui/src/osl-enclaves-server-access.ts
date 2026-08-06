export interface EnclaveServerMessage {
  readonly messageId: string;
  readonly channelId: string;
  readonly senderId: string;
  readonly plaintext: string;
  readonly marked?: boolean;
}

export interface EnclaveServerMember {
  readonly memberId: string;
  readonly displayName: string;
}

export interface EnclaveServerChannel {
  readonly channelId: string;
  readonly name: string;
}

export interface EnclaveServerSnapshot {
  readonly serverId: string;
  readonly messages: readonly EnclaveServerMessage[];
  readonly members: readonly EnclaveServerMember[];
  readonly channels: readonly EnclaveServerChannel[];
}

export interface EnclaveServerAccessResult<T> {
  readonly requesterName: string;
  readonly refused: boolean;
  readonly refusal: string | null;
  readonly rows: readonly T[];
}

export interface EnclaveServerScreen {
  readonly requesterName: string;
  readonly messageRows: readonly EnclaveServerMessage[];
  readonly memberRows: readonly EnclaveServerMember[];
  readonly channelRows: readonly EnclaveServerChannel[];
  readonly refusals: readonly string[];
}

function requesterIsMember(snapshot: EnclaveServerSnapshot, requesterName: string): boolean {
  return snapshot.members.some((member) => member.displayName === requesterName);
}

function refused<T>(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
  listName: "message list" | "member list" | "channel list",
): EnclaveServerAccessResult<T> {
  return {
    requesterName,
    refused: true,
    refusal: `${requesterName} was never a member of ${snapshot.serverId}; ${listName} refused`,
    rows: [],
  };
}

function allowed<T>(requesterName: string, rows: readonly T[]): EnclaveServerAccessResult<T> {
  return { requesterName, refused: false, refusal: null, rows };
}

export function requestEnclaveServerMessages(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerMessage> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "message list");
  }
  return allowed(requesterName, snapshot.messages);
}

export function requestEnclaveServerMembers(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerMember> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "member list");
  }
  return allowed(requesterName, snapshot.members);
}

export function requestEnclaveServerChannels(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerAccessResult<EnclaveServerChannel> {
  if (!requesterIsMember(snapshot, requesterName)) {
    return refused(snapshot, requesterName, "channel list");
  }
  return allowed(requesterName, snapshot.channels);
}

export function enclaveServerScreen(
  snapshot: EnclaveServerSnapshot,
  requesterName: string,
): EnclaveServerScreen {
  const messages = requestEnclaveServerMessages(snapshot, requesterName);
  const members = requestEnclaveServerMembers(snapshot, requesterName);
  const channels = requestEnclaveServerChannels(snapshot, requesterName);
  return {
    requesterName,
    messageRows: messages.rows,
    memberRows: members.rows,
    channelRows: channels.rows,
    refusals: [messages.refusal, members.refusal, channels.refusal].filter((row): row is string => row !== null),
  };
}
