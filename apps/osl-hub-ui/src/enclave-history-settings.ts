import "./enclave-history-settings.css";

/**
 * TASK 6933 — the only allowed history path for a person who joined later.
 *
 * This UI model deliberately has no old-key field.  A member can re-encrypt a
 * message plaintext their own device already has to a joiner's current key;
 * the relay only carries that resulting ciphertext.  Configuration is always
 * read from this signed enclave record, never from a selected chip.
 */
export type EnclaveHistoryForNewMembers = "hidden" | "shared_from_when_turned_on";
export type EnclaveHistoryRoute = "mailbox" | "history" | "reconnect" | "restore" | "attachment";

export const ENCLAVE_HISTORY_ROUTES: readonly EnclaveHistoryRoute[] = Object.freeze([
  "mailbox", "history", "reconnect", "restore", "attachment",
]);

export interface SignedEnclaveHistoryRecord {
  readonly enclaveId: string;
  readonly mode: EnclaveHistoryForNewMembers;
  /** The committed send epoch at which sharing became eligible, if any. */
  readonly sharingStartedAtEpoch: number | null;
  /** The committed send epoch at which sharing stopped, if any. */
  readonly sharingStoppedAtEpoch: number | null;
  readonly version: number;
  readonly signature: string;
}

export interface SignedEnclaveHistoryInstruction {
  readonly enclaveId: string;
  readonly actorMemberId: string;
  readonly expectedVersion: number;
  readonly mode: EnclaveHistoryForNewMembers;
  readonly signedEpoch: number;
  readonly signature: string;
}

export interface SignedHistoryChannelEvent {
  readonly kind: "history-sharing-enabled" | "history-sharing-disabled";
  readonly enclaveId: string;
  readonly epoch: number;
  readonly signature: string;
}

export type HistorySettingResult =
  | { readonly ok: true; readonly record: SignedEnclaveHistoryRecord; readonly channelEvent: SignedHistoryChannelEvent }
  | { readonly ok: false; readonly reason: "invalid-signature" | "wrong-enclave" | "not-permitted" | "stale-version" | "invalid-epoch" | "no-change" };

export type EnclavePermissionResolver = (
  actorMemberId: string,
  enclaveId: string,
  permission: "configure_history_for_new_members",
) => boolean;

export type HistoryInstructionVerifier = (instruction: SignedEnclaveHistoryInstruction) => boolean;

export interface EnclaveCommittedHistoryMessage {
  readonly messageId: string;
  readonly committedSendEpoch: number;
  /** Plaintext is present only on the existing sharing member's device. */
  readonly plaintextOnSharerDevice: string;
}

export interface JoiningMemberCurrentKey {
  readonly memberId: string;
  readonly currentKeyId: string;
  readonly joinedAtEpoch: number;
}

export interface MemberResharedHistoryItem {
  readonly messageId: string;
  readonly committedSendEpoch: number;
  readonly sharerMemberId: string;
  readonly sharerDisplayName: string;
  readonly sharerDeviceId: string;
  readonly addressedToCurrentJoinerKey: string;
  readonly ciphertext: string;
}

export interface ReShareRefusal {
  readonly messageId: string;
  readonly epoch: number;
  readonly reason: "history-hidden" | "before-sharing-epoch" | "after-sharing-stopped" | "not-held-by-sharer";
}

export interface MemberReshareOutcome {
  readonly items: readonly MemberResharedHistoryItem[];
  readonly refusals: readonly ReShareRefusal[];
}

/** A non-empty, generated inventory of all permitted re-share sinks. */
export const ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY = Object.freeze([
  "member-device-outbound-reshare-ciphertext",
  "relay-ciphertext-courier",
  "joiner-current-key-inbox",
] as const);

export interface HistorySinkObservation {
  readonly sink: string;
  readonly bytes: number;
  /** Classified runtime writes must never carry message or chain key material. */
  readonly containsPreJoinKeyMaterial: boolean;
}

function nonBlank(value: string): boolean {
  return value.trim().length > 0;
}

function validEpoch(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0;
}

function latestMarker(record: SignedEnclaveHistoryRecord): number {
  return Math.max(record.sharingStartedAtEpoch ?? -1, record.sharingStoppedAtEpoch ?? -1);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

/** New enclaves start hidden; this is configuration, not a client preference. */
export function newEnclaveHistoryRecord(enclaveId: string, signature = "signed-new-enclave-hidden"): SignedEnclaveHistoryRecord {
  if (!nonBlank(enclaveId) || !nonBlank(signature)) throw new Error("A signed enclave history record needs an enclave and signature");
  return Object.freeze({
    enclaveId,
    mode: "hidden",
    sharingStartedAtEpoch: null,
    sharingStoppedAtEpoch: null,
    version: 0,
    signature,
  });
}

/** Restarts restore the signed record, never a chip's selected state. */
export function restoreSignedEnclaveHistoryRecord(serialized: string): SignedEnclaveHistoryRecord {
  const parsed: unknown = JSON.parse(serialized);
  if (typeof parsed !== "object" || parsed === null) throw new Error("Invalid signed enclave history record");
  const candidate = parsed as Partial<SignedEnclaveHistoryRecord>;
  if (!nonBlank(candidate.enclaveId ?? "") || !nonBlank(candidate.signature ?? "")
    || (candidate.mode !== "hidden" && candidate.mode !== "shared_from_when_turned_on")
    || !Number.isSafeInteger(candidate.version) || (candidate.version ?? -1) < 0
    || (candidate.sharingStartedAtEpoch !== null && !validEpoch(candidate.sharingStartedAtEpoch ?? -1))
    || (candidate.sharingStoppedAtEpoch !== null && !validEpoch(candidate.sharingStoppedAtEpoch ?? -1))) {
    throw new Error("Invalid signed enclave history record");
  }
  const enclaveId = candidate.enclaveId as string;
  const signature = candidate.signature as string;
  const version = candidate.version as number;
  return Object.freeze({
    enclaveId,
    mode: candidate.mode,
    sharingStartedAtEpoch: candidate.sharingStartedAtEpoch ?? null,
    sharingStoppedAtEpoch: candidate.sharingStoppedAtEpoch ?? null,
    version,
    signature,
  });
}

/**
 * Applies the one permission resolver's answer to a verified, enclave-bound
 * instruction.  The returned event is appended before the next message at
 * that epoch; callers must not use a rendered control as authority.
 */
export function applySignedHistorySetting(
  current: SignedEnclaveHistoryRecord,
  instruction: SignedEnclaveHistoryInstruction,
  verify: HistoryInstructionVerifier,
  resolvePermission: EnclavePermissionResolver,
): HistorySettingResult {
  if (!verify(instruction)) return { ok: false, reason: "invalid-signature" };
  if (instruction.enclaveId !== current.enclaveId) return { ok: false, reason: "wrong-enclave" };
  if (!resolvePermission(instruction.actorMemberId, current.enclaveId, "configure_history_for_new_members")) {
    return { ok: false, reason: "not-permitted" };
  }
  if (instruction.expectedVersion !== current.version) return { ok: false, reason: "stale-version" };
  if (!validEpoch(instruction.signedEpoch) || instruction.signedEpoch <= latestMarker(current)) {
    return { ok: false, reason: "invalid-epoch" };
  }
  if (instruction.mode === current.mode) return { ok: false, reason: "no-change" };

  const enabled = instruction.mode === "shared_from_when_turned_on";
  const record: SignedEnclaveHistoryRecord = Object.freeze({
    enclaveId: current.enclaveId,
    mode: instruction.mode,
    sharingStartedAtEpoch: enabled ? instruction.signedEpoch : current.sharingStartedAtEpoch,
    sharingStoppedAtEpoch: enabled ? null : instruction.signedEpoch,
    version: current.version + 1,
    signature: instruction.signature,
  });
  return {
    ok: true,
    record,
    channelEvent: Object.freeze({
      kind: enabled ? "history-sharing-enabled" : "history-sharing-disabled",
      enclaveId: current.enclaveId,
      epoch: instruction.signedEpoch,
      signature: instruction.signature,
    }),
  };
}

/** A message is eligible only in the signed forward window. */
export function historyMessageEligibility(
  record: SignedEnclaveHistoryRecord,
  message: Pick<EnclaveCommittedHistoryMessage, "committedSendEpoch">,
): ReShareRefusal["reason"] | null {
  if (record.mode !== "shared_from_when_turned_on" || record.sharingStartedAtEpoch === null) return "history-hidden";
  if (message.committedSendEpoch < record.sharingStartedAtEpoch) return "before-sharing-epoch";
  if (record.sharingStoppedAtEpoch !== null && message.committedSendEpoch >= record.sharingStoppedAtEpoch) return "after-sharing-stopped";
  return null;
}

/**
 * Runs only on a named current member device.  `encryptToCurrentJoinerKey`
 * receives plaintext locally and returns ciphertext already addressed to the
 * joiner's current key.  No prior message, chain, or root key is represented.
 */
export function reshareHistoryFromMemberDevice(
  record: SignedEnclaveHistoryRecord,
  sharer: { readonly memberId: string; readonly displayName: string; readonly deviceId: string },
  joiner: JoiningMemberCurrentKey,
  messages: readonly EnclaveCommittedHistoryMessage[],
  encryptToCurrentJoinerKey: (plaintext: string, currentKeyId: string) => string,
): MemberReshareOutcome {
  const items: MemberResharedHistoryItem[] = [];
  const refusals: ReShareRefusal[] = [];
  for (const message of messages) {
    const reason = historyMessageEligibility(record, message);
    if (reason !== null) {
      refusals.push({ messageId: message.messageId, epoch: message.committedSendEpoch, reason });
      continue;
    }
    if (!nonBlank(message.plaintextOnSharerDevice)) {
      refusals.push({ messageId: message.messageId, epoch: message.committedSendEpoch, reason: "not-held-by-sharer" });
      continue;
    }
    items.push(Object.freeze({
      messageId: message.messageId,
      committedSendEpoch: message.committedSendEpoch,
      sharerMemberId: sharer.memberId,
      sharerDisplayName: sharer.displayName,
      sharerDeviceId: sharer.deviceId,
      addressedToCurrentJoinerKey: joiner.currentKeyId,
      ciphertext: encryptToCurrentJoinerKey(message.plaintextOnSharerDevice, joiner.currentKeyId),
    }));
  }
  return Object.freeze({ items: Object.freeze(items), refusals: Object.freeze(refusals) });
}

/**
 * A hand-built request cannot cause a relay, repair path, or historical fetch
 * to create a re-share.  The only producer is the named member device above.
 */
export function directHistoryRequestBytes(
  route: EnclaveHistoryRoute,
  record: SignedEnclaveHistoryRecord,
  message: Pick<EnclaveCommittedHistoryMessage, "committedSendEpoch">,
): Uint8Array {
  void route;
  void record;
  void message;
  return new Uint8Array(0);
}

/** Fails closed if a sink is absent, new, empty, or contains any key material. */
export function reconcileHistoryReshareSinks(observed: readonly HistorySinkObservation[]): { readonly ok: boolean; readonly reason: string | null } {
  const inventory: readonly string[] = ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY;
  if (inventory.length === 0) return { ok: false, reason: "history sink inventory is empty" };
  const expected = new Set<string>(inventory);
  const seen = new Set<string>();
  for (const row of observed) {
    if (!expected.has(row.sink)) return { ok: false, reason: `unclassified history sink=${row.sink}` };
    if (row.bytes <= 0) return { ok: false, reason: `empty history sink=${row.sink}` };
    if (row.containsPreJoinKeyMaterial) return { ok: false, reason: `pre-join key material at sink=${row.sink}` };
    seen.add(row.sink);
  }
  for (const sink of expected) if (!seen.has(sink)) return { ok: false, reason: `unobserved history sink=${sink}` };
  return { ok: true, reason: null };
}

/** The row is intentionally output-only; the signed record is its authority. */
export function enclaveHistoryForNewMembersRowMarkup(record: SignedEnclaveHistoryRecord): string {
  const shared = record.mode === "shared_from_when_turned_on";
  const detail = shared
    ? `A member may re-share only messages sent from signed epoch ${record.sharingStartedAtEpoch}. Already shared items cannot be recalled.`
    : record.sharingStoppedAtEpoch === null
      ? "New members receive no pre-join history or keys."
      : "New members receive no pre-join history or keys. Already shared items cannot be recalled.";
  return `<div class="setting-line enclave-history-for-new-members" data-enclave-history-mode="${record.mode}" data-enclave-id="${escapeHtml(record.enclaveId)}" data-signed-enclave-history-record="${escapeHtml(record.signature)}"><span><strong>History for new members</strong><small>${detail}</small></span><div class="choice-chip-group" role="group" aria-label="History for new members"><button class="choice-chip" type="button" data-enclave-history-mode="hidden" aria-pressed="${!shared}">HIDDEN</button><button class="choice-chip" type="button" data-enclave-history-mode="shared_from_when_turned_on" aria-pressed="${shared}">SHARED FROM WHEN IT WAS TURNED ON</button></div></div>`;
}

export function enclaveHistoryForNewMembersSettingsMarkup(record: SignedEnclaveHistoryRecord): string {
  return `<section class="settings-list enclave-history-settings" aria-label="Moderation and joining"><h2>Moderation and joining</h2>${enclaveHistoryForNewMembersRowMarkup(record)}</section>`;
}

/** This disclosure belongs beside an invite and join review, never behind it. */
export function enclaveHistoryJoinDisclosureMarkup(record: SignedEnclaveHistoryRecord): string {
  const mode = record.mode === "hidden" ? "HIDDEN" : "SHARED FROM WHEN IT WAS TURNED ON";
  const detail = record.mode === "hidden"
    ? "You receive no pre-join history or keys."
    : `A named member may share only messages sent from signed epoch ${record.sharingStartedAtEpoch}; shared items open under your current key.`;
  return `<p class="enclave-history-join-disclosure" data-enclave-history-join-mode="${record.mode}" data-signed-enclave-history-record="${escapeHtml(record.signature)}"><strong>History for new members: ${mode}</strong> ${detail}</p>`;
}

/** Every re-shared item names the sharer before its open control. */
export function memberResharedHistoryItemMarkup(item: MemberResharedHistoryItem): string {
  return `<article class="member-reshared-history-item" data-message-id="${escapeHtml(item.messageId)}" data-reshared-by-member="${escapeHtml(item.sharerMemberId)}" data-reshared-by-device="${escapeHtml(item.sharerDeviceId)}"><p class="member-reshared-history-item__sharer">Shared by ${escapeHtml(item.sharerDisplayName)}</p><button type="button" data-open-reshared-history="${escapeHtml(item.messageId)}">Open shared item</button></article>`;
}
