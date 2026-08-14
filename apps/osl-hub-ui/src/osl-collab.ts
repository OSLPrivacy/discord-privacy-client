import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import type { OslDocumentKind, OslNote } from "./osl-notes";

export interface OslSharedDocument { kind: OslDocumentKind; title: string; body: string; folder: string; tags: string[]; favorite: boolean; }
export interface OslLanInvitation { code: string; address: string; roomId: string; encrypted: true; requiresCloud: false; requiresPro: false; }
export interface OslLanSession { sessionId: string; role: "host" | "guest"; revision: number; document: OslSharedDocument; invitation: OslLanInvitation | null; connected: true; encrypted: true; cloud: false; }
export interface OslLanSync { revision: number; document: OslSharedDocument; changed: boolean; conflict: boolean; connected: boolean; }
export type EnclaveAudienceMembershipVisibility = "visible" | "count-only" | "hidden";
export type EnclaveAudienceRefusal = "consent" | "binding" | "authority";
export type EnclavePostRefusal = EnclaveAudienceRefusal | "audience" | "draft" | "membership-review";
export interface EnclaveAudienceMember { memberId: string; name: string; verified: boolean; }
export interface EnclaveAudience { audienceId: string; name: string; memberCount: number; membershipVisibility: EnclaveAudienceMembershipVisibility; visibleMembers: EnclaveAudienceMember[]; canPost: boolean; refusal: EnclaveAudienceRefusal | null; }
export interface EnclavePostMembershipReview { audienceId: string; audienceName: string; memberCount: number; members: EnclaveAudienceMember[]; shownBeforePosting: true; }
export interface EnclavePostReady { status: "ready"; audienceId: string; audienceName: string; body: string; membershipReview: EnclavePostMembershipReview; encryptedForAudience: true; feedOrder: "chronological"; sendAuthority: "user-action-required"; }
export interface EnclavePostRefused { status: "refused"; reason: EnclavePostRefusal; audienceId: string | null; audienceName: string | null; membershipReview: EnclavePostMembershipReview | null; encryptedForAudience: false; sendAuthority: "none"; }
export type EnclavePostComposition = EnclavePostReady | EnclavePostRefused;
export interface EnclaveServerMember { memberId: string; name: string; }
export interface EnclaveServerContentDraft { contentId: string; authorMemberId: string; body: string; }
export interface EnclaveServerContent { contentId: string; authorMemberId: string; body: string; }
export interface EnclaveServerContentState { serverId: string; members: EnclaveServerMember[]; content: EnclaveServerContent[]; }
export type EnclaveServerContentResult = { status: "accepted"; contentId: string; serverContentCount: number } | { status: "refused"; reason: "invalid-server" | "invalid-content" | "not-server-member" | "duplicate-content"; memberId: string | null; serverContentCount: number };

const kinds = new Set(["note", "document", "spreadsheet", "drawing", "presentation", "photo", "video", "audio", "model3d"]);
const visibility = new Set(["visible", "count-only", "hidden"]);
const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const exact = (value: Record<string, unknown>, keys: string[]) => Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const boundedText = (value: unknown, maxBytes: number): value is string => typeof value === "string" && value.trim().length > 0 && !/[\p{Cc}\p{Cf}]/u.test(value) && new TextEncoder().encode(value).byteLength <= maxBytes;
const boundedPostBody = (value: unknown): value is string => typeof value === "string" && value.trim().length > 0 && !/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F\p{Cf}]/u.test(value) && new TextEncoder().encode(value).byteLength <= 16 * 1024;
const localId = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{32}$/u.test(value);
const contentId = (value: unknown): value is string => typeof value === "string" && /^[a-z0-9][a-z0-9-]{0,79}$/u.test(value);
export function parseSharedDocument(value: unknown): OslSharedDocument | null { if (!record(value) || !exact(value, ["kind", "title", "body", "folder", "tags", "favorite"]) || !kinds.has(String(value.kind)) || typeof value.title !== "string" || new TextEncoder().encode(value.title).byteLength > 240 || typeof value.body !== "string" || new TextEncoder().encode(value.body).byteLength > 256 * 1024 || typeof value.folder !== "string" || new TextEncoder().encode(value.folder).byteLength > 80 || !Array.isArray(value.tags) || value.tags.length > 16 || !value.tags.every((tag) => typeof tag === "string" && tag.length > 0 && new TextEncoder().encode(tag).byteLength <= 32) || typeof value.favorite !== "boolean") return null; return value as unknown as OslSharedDocument; }
export function sharedDocument(note: OslNote): OslSharedDocument { return { kind: note.kind, title: note.title, body: note.body, folder: note.folder, tags: [...note.tags], favorite: note.favorite }; }
function parseInvitation(value: unknown): OslLanInvitation | null { if (!record(value) || !exact(value, ["code", "address", "roomId", "encrypted", "requiresCloud", "requiresPro"]) || typeof value.code !== "string" || value.code.length > 256 || typeof value.address !== "string" || typeof value.roomId !== "string" || !/^[a-f0-9]{32}$/u.test(value.roomId) || value.encrypted !== true || value.requiresCloud !== false || value.requiresPro !== false) return null; return value as unknown as OslLanInvitation; }
export function parseLanSession(value: unknown): OslLanSession | null { if (!record(value) || !exact(value, ["sessionId", "role", "revision", "document", "invitation", "connected", "encrypted", "cloud"]) || typeof value.sessionId !== "string" || !/^[a-f0-9]{32}$/u.test(value.sessionId) || !["host", "guest"].includes(String(value.role)) || !Number.isSafeInteger(value.revision) || Number(value.revision) < 0 || value.connected !== true || value.encrypted !== true || value.cloud !== false) return null; const document = parseSharedDocument(value.document); const invitation = value.invitation === null ? null : parseInvitation(value.invitation); if (!document || value.invitation !== null && !invitation || value.role === "host" && !invitation || value.role === "guest" && invitation) return null; return { sessionId: value.sessionId, role: value.role as "host" | "guest", revision: Number(value.revision), document, invitation, connected: true, encrypted: true, cloud: false }; }
export function parseLanSync(value: unknown): OslLanSync | null { if (!record(value) || !exact(value, ["revision", "document", "changed", "conflict", "connected"]) || !Number.isSafeInteger(value.revision) || Number(value.revision) < 0 || typeof value.changed !== "boolean" || typeof value.conflict !== "boolean" || typeof value.connected !== "boolean") return null; const document = parseSharedDocument(value.document); return document ? { revision: Number(value.revision), document, changed: value.changed, conflict: value.conflict, connected: value.connected } : null; }
function parseEnclaveAudienceMember(value: unknown): EnclaveAudienceMember | null { if (!record(value) || !exact(value, ["memberId", "name", "verified"]) || typeof value.memberId !== "string" || !/^[a-f0-9]{32}$/u.test(value.memberId) || !boundedText(value.name, 80) || typeof value.verified !== "boolean") return null; return { memberId: value.memberId, name: value.name, verified: value.verified }; }
export function parseEnclaveAudience(value: unknown): EnclaveAudience | null {
  if (!record(value) || !exact(value, ["audienceId", "name", "memberCount", "membershipVisibility", "visibleMembers", "consentGranted", "boundToCurrentEnclave", "postingAuthorized"]) || typeof value.audienceId !== "string" || !/^[a-f0-9]{32}$/u.test(value.audienceId) || !boundedText(value.name, 80) || !Number.isSafeInteger(value.memberCount) || Number(value.memberCount) < 0 || Number(value.memberCount) > 10_000 || !visibility.has(String(value.membershipVisibility)) || !Array.isArray(value.visibleMembers) || value.visibleMembers.length > Number(value.memberCount) || typeof value.consentGranted !== "boolean" || typeof value.boundToCurrentEnclave !== "boolean" || typeof value.postingAuthorized !== "boolean") return null;
  const visibleMembers = value.visibleMembers.map(parseEnclaveAudienceMember);
  if (visibleMembers.some((member) => member === null)) return null;
  const membershipVisibility = value.membershipVisibility as EnclaveAudienceMembershipVisibility;
  if (membershipVisibility !== "visible" && visibleMembers.length !== 0) return null;
  const refusal = value.consentGranted !== true ? "consent" : value.boundToCurrentEnclave !== true ? "binding" : value.postingAuthorized !== true ? "authority" : null;
  return { audienceId: value.audienceId, name: value.name, memberCount: Number(value.memberCount), membershipVisibility, visibleMembers: visibleMembers as EnclaveAudienceMember[], canPost: refusal === null, refusal };
}

function parseComposedEnclaveAudience(value: unknown): EnclaveAudience | null {
  if (!record(value) || !exact(value, ["audienceId", "name", "memberCount", "membershipVisibility", "visibleMembers", "canPost", "refusal"]) || typeof value.audienceId !== "string" || !/^[a-f0-9]{32}$/u.test(value.audienceId) || !boundedText(value.name, 80) || !Number.isSafeInteger(value.memberCount) || Number(value.memberCount) < 0 || Number(value.memberCount) > 10_000 || !visibility.has(String(value.membershipVisibility)) || !Array.isArray(value.visibleMembers) || value.visibleMembers.length > Number(value.memberCount) || typeof value.canPost !== "boolean" || !(value.refusal === null || value.refusal === "consent" || value.refusal === "binding" || value.refusal === "authority") || value.canPost !== (value.refusal === null)) return null;
  const visibleMembers = value.visibleMembers.map(parseEnclaveAudienceMember);
  if (visibleMembers.some((member) => member === null)) return null;
  const membershipVisibility = value.membershipVisibility as EnclaveAudienceMembershipVisibility;
  if (membershipVisibility !== "visible" && visibleMembers.length !== 0) return null;
  return { audienceId: value.audienceId, name: value.name, memberCount: Number(value.memberCount), membershipVisibility, visibleMembers: visibleMembers as EnclaveAudienceMember[], canPost: value.canPost, refusal: value.refusal };
}

function enclavePostRefusal(reason: EnclavePostRefusal, audience: EnclaveAudience | null = null, membershipReview: EnclavePostMembershipReview | null = null): EnclavePostRefused {
  return { status: "refused", reason, audienceId: audience?.audienceId ?? null, audienceName: audience?.name ?? null, membershipReview, encryptedForAudience: false, sendAuthority: "none" };
}

function enclaveMembershipReview(audience: EnclaveAudience): EnclavePostMembershipReview | null {
  if (audience.membershipVisibility !== "visible" || audience.visibleMembers.length !== audience.memberCount) return null;
  return { audienceId: audience.audienceId, audienceName: audience.name, memberCount: audience.memberCount, members: audience.visibleMembers.map((member) => ({ ...member })), shownBeforePosting: true };
}

export function composeEnclavePost(audienceInput: unknown, bodyInput: unknown): EnclavePostComposition {
  const audience = parseEnclaveAudience(audienceInput) ?? parseComposedEnclaveAudience(audienceInput);
  if (!audience) return enclavePostRefusal("audience");
  const membershipReview = enclaveMembershipReview(audience);
  if (!membershipReview) return enclavePostRefusal("membership-review", audience);
  if (!audience.canPost) return enclavePostRefusal(audience.refusal ?? "authority", audience, membershipReview);
  if (!boundedPostBody(bodyInput)) return enclavePostRefusal("draft", audience, membershipReview);
  return { status: "ready", audienceId: audience.audienceId, audienceName: audience.name, body: bodyInput.trim(), membershipReview, encryptedForAudience: true, feedOrder: "chronological", sendAuthority: "user-action-required" };
}

export function createEnclaveServerContent(state: EnclaveServerContentState, draft: EnclaveServerContentDraft): EnclaveServerContentResult {
  if (!localId(state.serverId) || !Array.isArray(state.members) || !Array.isArray(state.content) || state.members.some((member) => !localId(member.memberId) || !boundedText(member.name, 80)) || state.content.some((item) => !contentId(item.contentId) || !localId(item.authorMemberId) || !boundedPostBody(item.body))) {
    return { status: "refused", reason: "invalid-server", memberId: null, serverContentCount: Array.isArray(state.content) ? state.content.length : 0 };
  }
  if (!contentId(draft.contentId) || !localId(draft.authorMemberId) || !boundedPostBody(draft.body)) {
    return { status: "refused", reason: "invalid-content", memberId: localId(draft.authorMemberId) ? draft.authorMemberId : null, serverContentCount: state.content.length };
  }
  if (!state.members.some((member) => member.memberId === draft.authorMemberId)) {
    return { status: "refused", reason: "not-server-member", memberId: draft.authorMemberId, serverContentCount: state.content.length };
  }
  if (state.content.some((item) => item.contentId === draft.contentId)) {
    return { status: "refused", reason: "duplicate-content", memberId: draft.authorMemberId, serverContentCount: state.content.length };
  }
  state.content.push({ contentId: draft.contentId, authorMemberId: draft.authorMemberId, body: draft.body.trim() });
  return { status: "accepted", contentId: draft.contentId, serverContentCount: state.content.length };
}

export async function hostOslLanRoom(document: OslSharedDocument): Promise<OslLanSession | null> { if (!isTauriRuntime() || !parseSharedDocument(document)) return null; return parseLanSession(await invoke("host_osl_lan_room", { document })); }
export async function joinOslLanRoom(code: string): Promise<OslLanSession | null> { if (!isTauriRuntime() || !code || code.length > 256) return null; return parseLanSession(await invoke("join_osl_lan_room", { code })); }
export async function syncOslLanRoom(session: OslLanSession, document?: OslSharedDocument): Promise<OslLanSync | null> { if (!isTauriRuntime() || document && !parseSharedDocument(document)) return null; const command = session.role === "host" ? "sync_hosted_osl_lan_room" : "sync_joined_osl_lan_room"; const identity = session.role === "host" ? { roomId: session.sessionId } : { sessionId: session.sessionId }; return parseLanSync(await invoke(command, { ...identity, baseRevision: session.revision, document: document ?? null })); }
export async function stopOslLanRoom(sessionId: string): Promise<boolean> { return isTauriRuntime() && /^[a-f0-9]{32}$/u.test(sessionId) && await invoke("stop_osl_lan_room", { sessionId }) === true; }
