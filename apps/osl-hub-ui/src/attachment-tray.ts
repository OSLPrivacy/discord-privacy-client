import {
  DISCORD_MEDIA_MAX_SIZE,
  type DiscordMediaMetadata,
  parseDiscordMediaSelection,
  stageDiscordMedia,
} from "./discord-media-staging";

export interface AttachmentTrayCard {
  jobId: string;
  metadata: DiscordMediaMetadata;
}

export type AttachmentTrayRefusalReason = "oversize" | "invalid";

export interface AttachmentTrayRefusal {
  filename: string | null;
  reason: AttachmentTrayRefusalReason;
}

export interface AttachmentTrayState {
  cards: readonly AttachmentTrayCard[];
  lastRefusal: AttachmentTrayRefusal | null;
}

export const EMPTY_ATTACHMENT_TRAY_STATE: AttachmentTrayState = { cards: [], lastRefusal: null };

/** Best-effort read of a rejected selection, for the refusal message only; never trusted for staging. */
function attemptedSelectionSummary(value: unknown): { filename: string | null; size: number | null } {
  if (typeof value !== "object" || value === null) return { filename: null, size: null };
  const metadata = (value as Record<string, unknown>).metadata;
  if (typeof metadata !== "object" || metadata === null) return { filename: null, size: null };
  const { filename, size } = metadata as Record<string, unknown>;
  return {
    filename: typeof filename === "string" ? filename : null,
    size: typeof size === "number" && Number.isFinite(size) ? size : null,
  };
}

/**
 * Adds one native file selection to the tray. A refused file never touches
 * `cards`: every card already accepted is copied through unchanged, so
 * refusing file 3 of 3 cannot drop the cards already built for files 1 and 2.
 */
export function addAttachmentToTray(state: AttachmentTrayState, selection: unknown, now: number): AttachmentTrayState {
  const staged = stageDiscordMedia(selection, now);
  if (staged) {
    return { cards: [...state.cards, { jobId: staged.jobId, metadata: staged.metadata }], lastRefusal: null };
  }
  const { filename, size } = attemptedSelectionSummary(selection);
  const reason: AttachmentTrayRefusalReason = size !== null && size > DISCORD_MEDIA_MAX_SIZE ? "oversize" : "invalid";
  return { cards: state.cards, lastRefusal: { filename, reason } };
}

export function removeAttachmentFromTray(state: AttachmentTrayState, jobId: string): AttachmentTrayState {
  return { cards: state.cards.filter((card) => card.jobId !== jobId), lastRefusal: state.lastRefusal };
}

/** Re-validates a selection without mutating tray state; lets the composer preflight a drag-drop batch. */
export function attachmentTraySelectionIsValid(selection: unknown): boolean {
  return parseDiscordMediaSelection(selection) !== null;
}

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

function formatCardSize(bytes: number): string {
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toLocaleString("en-US", { maximumFractionDigits: 1 })} MB`;
  if (bytes >= 1_024) return `${(bytes / 1_024).toLocaleString("en-US", { maximumFractionDigits: 1 })} KB`;
  return `${bytes} B`;
}

function attachmentCardMarkup(card: AttachmentTrayCard): string {
  return `<li class="attachment-tray__card" data-attachment-job="${escapeHtml(card.jobId)}"><strong class="attachment-tray__name">${escapeHtml(card.metadata.filename)}</strong><span class="attachment-tray__size">${formatCardSize(card.metadata.size)}</span><button class="attachment-tray__remove" type="button" data-attachment-remove="${escapeHtml(card.jobId)}" aria-label="Remove ${escapeHtml(card.metadata.filename)}">Remove</button></li>`;
}

/**
 * Renders the safe DTO only, same CSP-driven split as attachment-progress.ts:
 * markup carries no inline style, styles.css owns the look.
 */
export function attachmentTrayMarkup(state: AttachmentTrayState): string {
  const cards = state.cards.map(attachmentCardMarkup).join("");
  const refusal = state.lastRefusal
    ? `<p class="attachment-tray__refusal" role="alert">${escapeHtml(state.lastRefusal.filename ?? "That file")} was not added${state.lastRefusal.reason === "oversize" ? " because it is too large." : "."}</p>`
    : "";
  return `<section class="attachment-tray" aria-label="Attachments"><ul class="attachment-tray__cards">${cards}</ul>${refusal}</section>`;
}
