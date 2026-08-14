/**
 * Authenticated, encrypted SPOILER formatting for first-party OSL Chats.
 *
 * The text marker is placed inside the logical plaintext before the existing
 * message encryption call.  The attachment marker is placed in the original
 * filename before the existing attachment envelope is encrypted.  Neither is
 * carrier metadata.  After normal decryption these helpers project either a
 * reveal control or the content; concealed content is never put in the DOM.
 */

export const OSL_CHAT_SPOILER_TEXT_PREFIX = "OSL-SPOILER/1\n";
export const OSL_CHAT_SPOILER_ATTACHMENT_PREFIX = "OSL-SPOILER-1--";
export const OSL_CHAT_SPOILER_NOTIFICATION = "New encrypted spoiler";

export type OslChatFormat = "plain" | "spoiler";

export interface ParsedOslChatText {
  readonly format: OslChatFormat;
  readonly body: string;
}

export interface SpoilerProjection {
  readonly screenText: string;
  readonly accessibilityText: string;
  readonly copyText: string;
  readonly searchText: string;
  readonly previewText: string;
  readonly notificationText: string;
}

export interface SpoilerAttachmentProjection {
  readonly format: OslChatFormat;
  readonly concealed: boolean;
  readonly filename: string;
  readonly sizeText: string;
}

export interface SpoilerLocalStore {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
}

export function formatOslChatText(body: string, format: OslChatFormat): string {
  return format === "spoiler" ? `${OSL_CHAT_SPOILER_TEXT_PREFIX}${body}` : body;
}

export function parseOslChatText(plaintext: string): ParsedOslChatText {
  return plaintext.startsWith(OSL_CHAT_SPOILER_TEXT_PREFIX)
    ? { format: "spoiler", body: plaintext.slice(OSL_CHAT_SPOILER_TEXT_PREFIX.length) }
    : { format: "plain", body: plaintext };
}

/** Preserve the extension so the existing authenticated MIME gate stays authoritative. */
export function formatOslChatAttachmentFilename(filename: string, format: OslChatFormat): string {
  return format === "spoiler" ? `${OSL_CHAT_SPOILER_ATTACHMENT_PREFIX}${filename}` : filename;
}

export function parseOslChatAttachmentFilename(filename: string): ParsedOslChatText {
  return filename.startsWith(OSL_CHAT_SPOILER_ATTACHMENT_PREFIX)
    ? { format: "spoiler", body: filename.slice(OSL_CHAT_SPOILER_ATTACHMENT_PREFIX.length) }
    : { format: "plain", body: filename };
}

export function projectSpoilerAttachment(
  encodedFilename: string,
  sizeText: string,
  revealed: boolean,
): SpoilerAttachmentProjection {
  const parsed = parseOslChatAttachmentFilename(encodedFilename);
  if (parsed.format === "spoiler" && !revealed) {
    return { format: "spoiler", concealed: true, filename: "", sizeText: "" };
  }
  return { format: parsed.format, concealed: false, filename: parsed.body, sizeText };
}

export function spoilerRevealStorageKey(memberId: string): string {
  if (!memberId || memberId.length > 180) throw new Error("invalid spoiler member");
  return `osl-chat-spoiler-reveals-v1:${encodeURIComponent(memberId)}`;
}

function validRevealId(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 128
    && /^[A-Za-z0-9:_-]+$/u.test(value);
}

export async function loadSpoilerReveals(
  store: SpoilerLocalStore | null,
  memberId: string,
): Promise<Set<string>> {
  if (!store) return new Set();
  try {
    const parsed = JSON.parse(await store.getItem(spoilerRevealStorageKey(memberId)) ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter(validRevealId).slice(0, 512));
  } catch {
    return new Set();
  }
}

/** Local-only write. This helper deliberately has no transport/read-receipt dependency. */
export async function persistSpoilerReveal(
  store: SpoilerLocalStore | null,
  memberId: string,
  revealId: string,
  current: ReadonlySet<string>,
): Promise<Set<string>> {
  if (!validRevealId(revealId)) throw new Error("invalid spoiler reveal");
  const next = new Set(current);
  next.add(revealId);
  if (store) {
    await store.setItem(spoilerRevealStorageKey(memberId), JSON.stringify([...next].slice(-512)));
  }
  return next;
}

/**
 * One projection feeds every surface that could accidentally disclose text.
 * A concealed spoiler contributes exactly zero content to all six surfaces.
 */
export function projectSpoilerContent(body: string, revealed: boolean): SpoilerProjection {
  if (!revealed) {
    return {
      screenText: "",
      accessibilityText: "",
      copyText: "",
      searchText: "",
      previewText: "",
      notificationText: OSL_CHAT_SPOILER_NOTIFICATION,
    };
  }
  return {
    screenText: body,
    accessibilityText: body,
    copyText: body,
    searchText: body,
    previewText: body,
    // Notifications are created on arrival, before a deliberate reveal, and
    // are never rewritten with content afterward.
    notificationText: OSL_CHAT_SPOILER_NOTIFICATION,
  };
}

export function spoilerRevealId(kind: "message" | "attachment", id: string): string {
  return `${kind}:${id}`;
}

/** The sole production markup for a concealed attachment; it contains no metadata. */
export function concealedSpoilerAttachmentControlMarkup(revealId: string): string {
  if (!validRevealId(revealId) || !revealId.startsWith("attachment:")) {
    throw new Error("invalid spoiler attachment reveal");
  }
  return `<button class="setting-line osl-chat-spoiler-control" data-osl-chat-spoiler-reveal="${revealId}" type="button" aria-label="Reveal spoiler attachment"><span aria-hidden="true"><strong>SPOILER attachment</strong><small>Reveal</small></span></button>`;
}
