import type { AppearanceColours } from "./appearance-colours";
import type { ScopedProfileRecord } from "./osl-profile-pane";
import "./appearance-preview.css";

export interface AppearancePreviewService {
  id: string;
  label: string;
  /** Trusted application icon markup supplied by the service-logo renderer. */
  icon: string;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

function initial(name: string): string {
  return Array.from(name.trim())[0]?.toUpperCase() ?? "?";
}

function avatarMarkup(profile: ScopedProfileRecord): string {
  if (profile.avatar) {
    return `<img src="${escapeHtml(profile.avatar)}" alt="${escapeHtml(profile.displayName)} avatar"/>`;
  }
  return `<span aria-hidden="true">${escapeHtml(initial(profile.displayName))}</span>`;
}

function serviceMarkup(services: readonly AppearancePreviewService[]): string {
  if (services.length === 0) return "";
  return `<div class="appearance-preview-services" aria-label="Connected services">${services.map((service) => `<span class="appearance-preview-service" title="${escapeHtml(service.label)}" data-appearance-preview-service="${escapeHtml(service.id)}">${service.icon}</span>`).join("")}</div>`;
}

/**
 * A compact, unsaved rendering of the profile card.  It receives the same
 * objects as the Appearance controls; nothing here reads persisted state.
 */
export function appearancePreviewMarkup(
  profile: ScopedProfileRecord,
  colours: AppearanceColours,
  services: readonly AppearancePreviewService[],
): string {
  const name = profile.displayName.trim() || "Your name";
  const vibe = profile.status.trim() || "Your vibe goes here.";
  return `<aside class="appearance-preview" data-appearance-preview aria-label="Profile preview" style="--appearance-preview-accent:${escapeHtml(colours.accent)};--appearance-preview-background:${escapeHtml(colours.background)};--appearance-preview-avatar:${escapeHtml(profile.colour)};--appearance-preview-card:${escapeHtml(profile.cardBackground)}">
    <p class="appearance-preview-kicker">PREVIEW</p>
    <article class="appearance-preview-card">
      <div class="appearance-preview-avatar">${avatarMarkup({ ...profile, displayName: name })}</div>
      <div class="appearance-preview-copy"><strong>${escapeHtml(name)}</strong><p>${escapeHtml(vibe)}</p></div>
      <div class="appearance-preview-footer"><span class="appearance-preview-verified"><span aria-hidden="true">✓</span> Verified</span>${serviceMarkup(services)}</div>
    </article>
  </aside>`;
}

/** Repaints only the card, preserving the focused Appearance form controls. */
export function updateAppearancePreview(
  mount: HTMLElement | null,
  profile: ScopedProfileRecord,
  colours: AppearanceColours,
  services: readonly AppearancePreviewService[],
): void {
  if (mount) mount.outerHTML = appearancePreviewMarkup(profile, colours, services);
}
