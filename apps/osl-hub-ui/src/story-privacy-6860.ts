/**
 * TASK 6860 — the story privacy surface.
 *
 * Four settings, rendered from the same stable ids and the same exact copy the
 * Rust engine (`crates/story-privacy`) enforces; the shared strings live in
 * `story-privacy-6860-manifest.json` so neither side can drift alone.
 *
 * The shield row is the one that has to be careful. Where the OS has no
 * capture-protection primitive the control is genuinely disabled and the only
 * sentence on screen is the one that claims nothing — no disclosure, no
 * "protected" badge, no greyed-out promise that reads as "on soon".
 */

import manifest from "./story-privacy-6860-manifest.json";

export type StoryAudienceId =
  | "story-audience-everyone"
  | "story-audience-friends"
  | "story-audience-verified";

export type StoryLifetimeId = "story-lifetime-1h" | "story-lifetime-12h" | "story-lifetime-24h";

export const STORY_AUDIENCES = manifest.audiences as ReadonlyArray<{
  id: StoryAudienceId;
  label: string;
}>;

export const STORY_LIFETIMES = manifest.lifetimes as ReadonlyArray<{
  id: StoryLifetimeId;
  label: string;
  seconds: number;
}>;

export const STORY_PRIVACY_COPY = manifest.copy;
export const STORY_AUDIENCE_SOURCES = manifest.audience_sources;

export interface StoryPrivacySettingsState {
  /** The audience a story with no SEND TO inherits. */
  defaultAudience: StoryAudienceId;
  /** The auto-burn deadline a story inherits. */
  defaultLifetime: StoryLifetimeId;
  /** Whether this OS exposes a supported capture-protection primitive. */
  shieldSupported: boolean;
  /** The stored shield value; forced off wherever it is unsupported. */
  shieldOn: boolean;
  /** Whether the poster receives the aggregate view signal. */
  viewReceiptsOn: boolean;
}

export interface StoryComposerState {
  settings: StoryPrivacySettingsState;
  /** `null` means the composer is still inheriting the settings default. */
  sendTo: StoryAudienceId | null;
}

function escapeText(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** The shield row's whole honest state, derived rather than stored. */
export function shieldRowState(state: StoryPrivacySettingsState): {
  available: boolean;
  on: boolean;
  claimsProtection: boolean;
  copy: string;
} {
  if (!state.shieldSupported) {
    return {
      available: false,
      on: false,
      claimsProtection: false,
      copy: STORY_PRIVACY_COPY.shield_unavailable,
    };
  }
  return {
    available: true,
    on: state.shieldOn,
    claimsProtection: state.shieldOn,
    copy: state.shieldOn ? STORY_PRIVACY_COPY.shield_disclosure : "",
  };
}

/** The audience a story will actually use, and where that came from. */
export function resolvedComposerAudience(state: StoryComposerState): {
  audience: StoryAudienceId;
  source: string;
} {
  return state.sendTo === null
    ? { audience: state.settings.defaultAudience, source: STORY_AUDIENCE_SOURCES.inherited }
    : { audience: state.sendTo, source: STORY_AUDIENCE_SOURCES.override };
}

/** The sentence a viewer sees before opening, which must match the mode. */
export function viewerPreOpenCopy(state: StoryPrivacySettingsState): string {
  return state.viewReceiptsOn
    ? STORY_PRIVACY_COPY.receipts_on_viewer
    : STORY_PRIVACY_COPY.receipts_off_viewer;
}

export function storyPrivacySettingsMarkup(state: StoryPrivacySettingsState): string {
  const audienceOptions = STORY_AUDIENCES.map(
    (option) =>
      `<button class="story-audience-option" type="button" role="radio" data-story-audience="${option.id}" aria-checked="${option.id === state.defaultAudience}">${escapeText(option.label)}</button>`,
  ).join("");
  const lifetimeOptions = STORY_LIFETIMES.map(
    (option) =>
      `<button class="story-lifetime-option" type="button" role="radio" data-story-lifetime="${option.id}" data-story-lifetime-seconds="${option.seconds}" aria-checked="${option.id === state.defaultLifetime}">${escapeText(option.label)}</button>`,
  ).join("");
  const shield = shieldRowState(state);
  const shieldControl = shield.available
    ? `<button class="story-shield-toggle" type="button" role="switch" data-story-shield-toggle aria-checked="${shield.on}">${shield.on ? "ON" : "OFF"}</button>`
    : `<button class="story-shield-toggle disabled" type="button" role="switch" data-story-shield-toggle aria-checked="false" aria-disabled="true" disabled>OFF</button>`;
  const shieldCopy = shield.copy
    ? `<p class="story-shield-copy" data-story-shield-copy>${escapeText(shield.copy)}</p>`
    : "";
  return [
    `<section class="story-privacy-settings" data-story-privacy-settings aria-labelledby="story-privacy-title">`,
    `<header><h2 class="machine-fact" id="story-privacy-title">Story privacy</h2><p>Every new story starts from these. A story can still choose its own audience with SEND TO.</p></header>`,
    `<div class="setting-line story-default-audience"><span><strong class="machine-fact">Default audience</strong><small>Who a story goes to when you do not change SEND TO.</small></span><div class="story-option-row" role="radiogroup" aria-label="Default story audience" data-story-default-audience="${state.defaultAudience}">${audienceOptions}</div></div>`,
    `<div class="setting-line story-default-lifetime"><span><strong class="machine-fact">Auto-burn</strong><small>A story burns at this deadline even if OSL was closed when it passed.</small></span><div class="story-option-row" role="radiogroup" aria-label="Story auto-burn" data-story-default-lifetime="${state.defaultLifetime}">${lifetimeOptions}</div></div>`,
    `<div class="setting-line story-screenshot-shield" data-story-shield-available="${shield.available}" data-story-shield-claims-protection="${shield.claimsProtection}"><span><strong class="machine-fact">Screenshot shield</strong><small>${shield.available ? "Uses the Windows capture-protection primitive on OSL's own window." : "No supported capture-protection primitive on this system."}</small>${shieldCopy}</span>${shieldControl}</div>`,
    `<div class="setting-line story-view-receipts"><span><strong class="machine-fact">View receipts</strong><small data-story-receipts-copy>${escapeText(viewerPreOpenCopy(state))}</small></span><button class="story-receipts-toggle" type="button" role="switch" data-story-receipts-toggle aria-checked="${state.viewReceiptsOn}">${state.viewReceiptsOn ? "ON" : "OFF"}</button></div>`,
    `</section>`,
  ].join("");
}

export function storyComposerSendToMarkup(state: StoryComposerState): string {
  const resolved = resolvedComposerAudience(state);
  const options = STORY_AUDIENCES.map(
    (option) =>
      `<button class="story-send-to-option" type="button" role="radio" data-story-send-to="${option.id}" aria-checked="${option.id === resolved.audience}">${escapeText(option.label)}</button>`,
  ).join("");
  const lifetime =
    STORY_LIFETIMES.find((option) => option.id === state.settings.defaultLifetime) ??
    STORY_LIFETIMES[STORY_LIFETIMES.length - 1];
  return [
    `<div class="story-composer-send-to" data-story-composer-send-to data-story-audience-source="${resolved.source}" data-story-resolved-audience="${resolved.audience}">`,
    `<span class="machine-fact">SEND TO</span>`,
    `<div class="story-option-row" role="radiogroup" aria-label="Send this story to">${options}</div>`,
    `<span class="story-composer-lifetime machine-fact" data-story-composer-lifetime="${lifetime.id}">Burns in ${escapeText(lifetime.label)}</span>`,
    `</div>`,
  ].join("");
}

export function storyViewerPreOpenMarkup(state: StoryPrivacySettingsState): string {
  return `<p class="story-viewer-pre-open" data-story-viewer-pre-open data-story-receipts-on="${state.viewReceiptsOn}">${escapeText(viewerPreOpenCopy(state))}</p>`;
}
