import type { ScopedProfileRecord } from "./osl-profile-pane";
import "./appearance-live-preview.css";

export const SAFETY_NUMBER_AUTHENTICATION_COPY = "Names identify. Safety numbers authenticate.";

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

function avatar(record: ScopedProfileRecord): string {
  if (record.avatar) return `<img src="${escapeHtml(record.avatar)}" alt="${escapeHtml(record.displayName)} avatar"/>`;
  return `<span aria-hidden="true">${escapeHtml(Array.from(record.displayName.trim())[0]?.toUpperCase() ?? "?")}</span>`;
}

/** A name is presentation only: this card contains no authentication or service proof. */
export function appearanceLivePreviewMarkup(record: ScopedProfileRecord, accent: string): string {
  return `<aside class="appearance-live-preview" data-appearance-honest-preview aria-label="Profile preview" style="--appearance-preview-accent:${escapeHtml(accent)};--appearance-preview-avatar:${escapeHtml(record.colour)}"><small>PREVIEW</small><div class="appearance-live-preview-card"><div class="appearance-live-preview-avatar">${avatar(record)}</div><div><strong>${escapeHtml(record.displayName)}</strong><p>${escapeHtml(record.status)}</p></div></div><p class="appearance-live-preview-authentication">${SAFETY_NUMBER_AUTHENTICATION_COPY}</p></aside>`;
}
