export type DeletionOutcome = "confirmed-deleted" | "confirmed-not-deleted" | "UNKNOWN";

export interface DeleteFinding { providerId: string; accountId: string; channelId: string; correspondentId: string; itemId: string; authoredBySelf: boolean; createdAtUnixMs: number; contentFingerprint: string }
export interface DeleteInspection { state: "present" | "absent" | "unknown"; authoredBySelf: boolean; contentFingerprint: string | null; authEpoch: string; schemaVersion: string; retractable: boolean; detail?: string }
export interface DeleteRequestResult { accepted: boolean; authEpoch: string; detail?: string }
export interface DeleteVerification { outcome: DeletionOutcome; authEpoch: string; detail?: string }

/** The entire provider boundary: no send/post/DM, HTTP/RPC, script, or generic automation primitive. */
export interface ScrubDeleteAdapter {
  enumerate(scope: { accountId: string; channelIds: readonly string[]; beforeUnixMs: number }): Promise<readonly DeleteFinding[]>;
  inspect(finding: DeleteFinding): Promise<DeleteInspection>;
  delete(finding: DeleteFinding): Promise<DeleteRequestResult>;
  verify(finding: DeleteFinding): Promise<DeleteVerification>;
}

export interface ScopePolicy { providerId: string; accountId: string; itemIds: readonly string[]; channelIds: readonly string[]; protectedChannelIds: readonly string[]; protectedCorrespondentIds: readonly string[]; maxCount: number; minAgeMs: number }
export interface ExecutionConsent { id: string; planFingerprint: string; findingsFingerprint: string; issuedAt: number; expiresAt: number }
export interface StepUpProof { providerId: string; accountId: string; authEpoch: string; authenticatedAt: number; expiresAt: number }
export interface ItemDeletionReceipt { providerId: string; accountId: string; channelId: string; itemId: string; outcome: DeletionOutcome; deletionCalled: boolean; verifiedByReadback: boolean; detail: string }
export interface ProviderDeletionReceipt { providerId: string; accountId: string; dryRun: boolean; consentId: string; startedAt: number; completedAt: number; stoppedFailClosed: boolean; items: ItemDeletionReceipt[] }
export interface ExecutionRequest { adapter: ScrubDeleteAdapter; findings: readonly DeleteFinding[]; approved: ScopePolicy; requested: ScopePolicy; consent: ExecutionConsent; stepUp: StepUpProof; finalConfirmation: boolean; dryRun: boolean; now: number; previousReceipts?: readonly ProviderDeletionReceipt[] }

const idPattern = /^[A-Za-z0-9][A-Za-z0-9._:@/-]{0,255}$/;
const canonical = (values: readonly string[]) => [...new Set(values)].sort().join("\n");
function token(parts: readonly string[]): string {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(parts.join("\u001f"))) { hash ^= byte; hash = Math.imul(hash, 0x01000193) >>> 0; }
  return hash.toString(16).padStart(8, "0");
}
export const planFingerprint = (p: ScopePolicy): string => token([p.providerId, p.accountId, canonical(p.itemIds), canonical(p.channelIds), canonical(p.protectedChannelIds), canonical(p.protectedCorrespondentIds), String(p.maxCount), String(p.minAgeMs)]);
export const findingsFingerprint = (items: readonly DeleteFinding[]): string => token(items.map((x) => [x.providerId, x.accountId, x.channelId, x.correspondentId, x.itemId, x.createdAtUnixMs, x.contentFingerprint].join("\u001e")).sort());
const subset = (candidate: readonly string[], allowed: readonly string[]) => candidate.every((x) => new Set(allowed).has(x));
export function scopeOnlyAllows(a: ScopePolicy, r: ScopePolicy): boolean {
  return a.providerId === r.providerId && a.accountId === r.accountId && r.maxCount <= a.maxCount && r.minAgeMs >= a.minAgeMs
    && subset(r.itemIds, a.itemIds) && subset(r.channelIds, a.channelIds)
    && subset(a.protectedChannelIds, r.protectedChannelIds) && subset(a.protectedCorrespondentIds, r.protectedCorrespondentIds);
}
function validPolicy(p: ScopePolicy): boolean {
  return idPattern.test(p.providerId) && idPattern.test(p.accountId) && Number.isSafeInteger(p.maxCount) && p.maxCount >= 0 && p.maxCount <= 10_000
    && Number.isSafeInteger(p.minAgeMs) && p.minAgeMs >= 0
    && [p.itemIds, p.channelIds, p.protectedChannelIds, p.protectedCorrespondentIds].every((xs) => xs.length <= 10_000 && xs.every((x) => idPattern.test(x)));
}
function preflight(r: ExecutionRequest): boolean {
  return r.finalConfirmation && validPolicy(r.approved) && validPolicy(r.requested) && scopeOnlyAllows(r.approved, r.requested)
    && r.consent.issuedAt <= r.now && r.consent.expiresAt >= r.now
    && r.consent.planFingerprint === planFingerprint(r.approved) && r.consent.findingsFingerprint === findingsFingerprint(r.findings)
    && r.stepUp.providerId === r.requested.providerId && r.stepUp.accountId === r.requested.accountId
    && r.stepUp.authenticatedAt <= r.now && r.stepUp.expiresAt >= r.now && r.now - r.stepUp.authenticatedAt <= 300_000;
}
function receipt(f: DeleteFinding, outcome: DeletionOutcome, deletionCalled: boolean, verifiedByReadback: boolean, detail: string): ItemDeletionReceipt {
  return { providerId: f.providerId, accountId: f.accountId, channelId: f.channelId, itemId: f.itemId, outcome, deletionCalled, verifiedByReadback, detail };
}
export async function executeDeletion(r: ExecutionRequest): Promise<ProviderDeletionReceipt> {
  const base = { providerId: r.requested.providerId, accountId: r.requested.accountId, dryRun: r.dryRun, consentId: r.consent.id, startedAt: r.now };
  const items: ItemDeletionReceipt[] = [];
  if (!preflight(r)) return { ...base, completedAt: r.now, stoppedFailClosed: true, items };
  const itemIds = new Set(r.requested.itemIds), channels = new Set(r.requested.channelIds), protectedChannels = new Set(r.requested.protectedChannelIds), protectedPeople = new Set(r.requested.protectedCorrespondentIds);
  const eligible = r.findings.filter((f) => f.providerId === r.requested.providerId && f.accountId === r.requested.accountId && f.authoredBySelf && itemIds.has(f.itemId) && channels.has(f.channelId) && !protectedChannels.has(f.channelId) && !protectedPeople.has(f.correspondentId) && r.now - f.createdAtUnixMs >= r.requested.minAgeMs).slice(0, r.requested.maxCount);
  if (r.dryRun) return { ...base, completedAt: r.now, stoppedFailClosed: false, items: eligible.map((f) => receipt(f, "UNKNOWN", false, false, "dry run: deletion was not called")) };
  let stoppedFailClosed = false;
  for (const f of eligible) {
    const wasUnknown = r.previousReceipts?.some((prior) => prior.providerId === f.providerId && prior.accountId === f.accountId && prior.items.some((x) => x.itemId === f.itemId && x.outcome === "UNKNOWN"));
    if (wasUnknown) { items.push(receipt(f, "UNKNOWN", false, false, "prior UNKNOWN requires manual resolution; not retried")); stoppedFailClosed = true; break; }
    let inspected: DeleteInspection;
    try { inspected = await r.adapter.inspect(f); } catch { items.push(receipt(f, "UNKNOWN", false, false, "inspection failed ambiguously")); stoppedFailClosed = true; break; }
    if (inspected.authEpoch !== r.stepUp.authEpoch || inspected.schemaVersion.length === 0) { items.push(receipt(f, "UNKNOWN", false, false, "authentication or provider schema changed")); stoppedFailClosed = true; break; }
    if (inspected.state === "absent") { items.push(receipt(f, "confirmed-deleted", false, true, "readback confirmed already absent")); continue; }
    if (inspected.state !== "present" || !inspected.authoredBySelf || inspected.contentFingerprint !== f.contentFingerprint) { items.push(receipt(f, inspected.state === "present" ? "confirmed-not-deleted" : "UNKNOWN", false, inspected.state === "present", "ownership, content, or presence check failed")); stoppedFailClosed = true; break; }
    if (!inspected.retractable) { items.push(receipt(f, "confirmed-not-deleted", false, true, inspected.detail ?? "cannot be recalled; item remains located at its source")); continue; }
    let deleted: DeleteRequestResult;
    try { deleted = await r.adapter.delete(f); } catch { items.push(receipt(f, "UNKNOWN", true, false, "delete failed ambiguously; not safe to retry")); stoppedFailClosed = true; break; }
    if (deleted.authEpoch !== r.stepUp.authEpoch) { items.push(receipt(f, "UNKNOWN", true, false, "authentication changed during deletion")); stoppedFailClosed = true; break; }
    try {
      const verified = await r.adapter.verify(f);
      const outcome = verified.authEpoch === r.stepUp.authEpoch ? verified.outcome : "UNKNOWN";
      const detail = deleted.accepted ? verified.detail ?? "provider readback completed" : `${deleted.detail ?? "provider rejected deletion"}; ${verified.detail ?? "provider readback completed"}`;
      items.push(receipt(f, outcome, true, outcome !== "UNKNOWN", detail));
      if (outcome === "UNKNOWN") { stoppedFailClosed = true; break; }
    } catch { items.push(receipt(f, "UNKNOWN", true, false, "verification failed ambiguously; not safe to retry")); stoppedFailClosed = true; break; }
  }
  return { ...base, completedAt: r.now, stoppedFailClosed, items };
}
