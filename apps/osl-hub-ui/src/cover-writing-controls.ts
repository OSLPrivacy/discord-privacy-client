import { escapeHtml } from "./services";
import { inDomTooltipMarkup } from "./in-dom-tooltip";

/** Every service composer and public-story/post composer uses this one control. */
export const COVER_WRITING_VIEWS = [
  "discord", "telegram", "signal", "whatsapp", "x", "instagram", "messenger", "email", "story",
] as const;

export type CoverWritingView = (typeof COVER_WRITING_VIEWS)[number];

export const COVER_WRITING_LABELS = ["Covertext", "AI Covertext"] as const;
export const COVER_WRITING_SHAPES_BEFORE = 2;
export const COVER_WRITING_SHAPES_NOW = 1;

export interface CoverWritingControlsOptions {
  readonly covertextEnabled?: boolean;
  readonly aiAvailable?: boolean;
  readonly aiSelected?: boolean;
  /** Optional host-specific hooks; the control shape remains shared. */
  readonly covertextId?: string;
  readonly aiCovertextId?: string;
}

/**
 * The sole cover-writing control shape.  Keeping the two actions adjacent but
 * separate makes their availability visible without hiding either in a menu.
 */
export function coverWritingControlsMarkup(
  view: CoverWritingView,
  options: CoverWritingControlsOptions = {},
): string {
  const covertextEnabled = options.covertextEnabled ?? true;
  const aiAvailable = options.aiAvailable ?? false;
  const aiSelected = options.aiSelected ?? false;
  const [covertextLabel, aiCovertextLabel] = COVER_WRITING_LABELS;
  const covertextTitle = covertextEnabled ? `${covertextLabel} is on` : `${covertextLabel} is off`;
  const aiTitle = aiAvailable
    ? "AI Covertext uses the verified model on this device"
    : "Requires a verified local model pack; no cloud AI is used";
  const covertextId = options.covertextId ? ` id="${escapeHtml(options.covertextId)}"` : "";
  const aiCovertextId = options.aiCovertextId ? ` id="${escapeHtml(options.aiCovertextId)}"` : "";
  return `<div class="cover-writing-controls" data-cover-writing-controls="shared" data-cover-writing-view="${escapeHtml(view)}" role="group" aria-label="Cover writing"><button${covertextId} class="header-protection-control in-dom-tooltip-anchor ${covertextEnabled && !aiSelected ? "active" : ""}" data-cover-writing-choice="covertext" type="button" aria-pressed="${covertextEnabled && !aiSelected}">${covertextLabel}${inDomTooltipMarkup(covertextTitle)}</button><button${aiCovertextId} class="header-protection-control in-dom-tooltip-anchor ${aiSelected ? "active" : ""}" data-cover-writing-choice="ai-covertext" type="button" aria-pressed="${aiSelected}" ${aiAvailable ? "" : "disabled"}>${aiCovertextLabel}${aiAvailable ? "" : " <small>Model pack needed</small>"}${inDomTooltipMarkup(aiTitle)}</button></div>`;
}
