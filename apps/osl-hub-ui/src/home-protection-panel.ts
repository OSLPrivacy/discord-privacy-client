import type { HomeProtectionSummary, HomeSafeStepAction } from "./home-protection-summary";
import {
  connectedAppsPhrase,
  homeSafeStepAction,
  homeSummaryFactErrors,
  protectionStateLabel,
  trustedPeoplePhrase,
} from "./home-protection-summary";

/**
 * TASK 0824 - the Home protection panel.
 *
 * TASK 0821 put the four direct facts on screen as one summary. This panel is
 * the full-width Home version of the same facts, drawn as five items a person
 * can act on:
 *
 *   1. protection state - the state word plus the message-protection level and
 *      the verification warning behind it;
 *   2. main action - the button routed to the next safe step, with the
 *      "Review settings" and "Learn more" side doors;
 *   3. connected-app count;
 *   4. trusted people;
 *   5. local activity controls - what OSL recorded on this device, with the
 *      pause/turn-on switch and the door into Activity.
 *
 * Items 1-4 come straight from the `HomeProtectionSummaryDto` of TASK 0820 and
 * inherit TASK 0821's honesty rules: counted sentences are rebuilt from the
 * counts, the next step is re-derived from the facts, and a summary that
 * contradicts itself is refused rather than drawn. Item 5 gets the same
 * treatment here: recorded events beside an "off" switch, or an event missing
 * its words or its time, refuse to draw instead of drawing calmly.
 */

/** One thing OSL recorded on this device. */
export interface HomeLocalActivityEvent {
  title: string;
  recordedAt: string;
}

export interface HomeLocalActivity {
  enabled: boolean;
  events: HomeLocalActivityEvent[];
}

export function localActivityCountPhrase(count: number): string {
  return count === 1 ? "1 recorded event" : `${count} recorded events`;
}

/** The one line the panel states about local activity, rebuilt from the data. */
export function localActivityStatusLine(activity: HomeLocalActivity): string {
  if (!activity.enabled) return "Local activity is off";
  if (activity.events.length === 0) return "No recent local activity";
  return `${localActivityCountPhrase(activity.events.length)} on this device`;
}

/** Every way the local activity data contradicts itself. Empty means drawable. */
export function localActivityErrors(activity: HomeLocalActivity): string[] {
  const errors: string[] = [];
  if (typeof activity.enabled !== "boolean") {
    errors.push(`local activity switch is not on or off: ${JSON.stringify(activity.enabled)}`);
  }
  if (!Array.isArray(activity.events)) {
    errors.push(`local activity events are missing: ${JSON.stringify(activity.events)}`);
    return errors;
  }
  if (activity.enabled === false && activity.events.length > 0) {
    errors.push(`local activity is off but ${activity.events.length} events are recorded`);
  }
  activity.events.forEach((event, index) => {
    if (!event.title?.trim()) errors.push(`local event ${index} has no words`);
    if (!event.recordedAt?.trim()) errors.push(`local event ${index} has no time`);
  });
  return errors;
}

export const PANEL_ITEMS = [
  "protection-state",
  "main-action",
  "connected-apps",
  "trusted-people",
  "local-activity",
] as const;

export type PanelItem = (typeof PANEL_ITEMS)[number];

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

/** "Warns before sending" from the DTO's "before sending". */
export function verificationLine(summary: HomeProtectionSummary): string {
  const warning = summary.verification_warning.trim();
  if (!warning) return "No warning is set";
  return `Warns ${warning}`;
}

function actionAttributes(action: HomeSafeStepAction): string {
  const section = action.settingsSection ? ` data-settings="${escapeHtml(action.settingsSection)}"` : "";
  const module = action.homeModule ? ` data-home-module="${escapeHtml(action.homeModule)}"` : "";
  return `data-route="${escapeHtml(action.route)}"${section}${module}`;
}

function protectionStateItem(summary: HomeProtectionSummary): string {
  return `<article class="home-panel-item" data-panel-item="protection-state">
    <span class="home-panel-item-label">Protection state</span>
    <strong class="home-panel-item-value" data-panel-value>${escapeHtml(protectionStateLabel(summary.protection_state))}</strong>
    <dl class="home-panel-subfacts">
      <div class="home-panel-subfact" data-panel-subfact="message-protection">
        <dt>Message protection</dt>
        <dd>${escapeHtml(summary.protection_choices.label)}</dd>
      </div>
      <div class="home-panel-subfact" data-panel-subfact="verification">
        <dt>Verification</dt>
        <dd>${escapeHtml(verificationLine(summary))}</dd>
      </div>
    </dl>
  </article>`;
}

function mainActionItem(action: HomeSafeStepAction): string {
  return `<article class="home-panel-item" data-panel-item="main-action">
    <span class="home-panel-item-label">Next safe step</span>
    <strong class="home-panel-item-value" data-panel-value>${escapeHtml(action.step)}</strong>
    <p class="home-panel-item-detail">${escapeHtml(action.detail)}</p>
    <div class="home-panel-item-controls">
      <button
        class="button primary home-panel-main-action"
        type="button"
        data-home-panel-action
        data-safe-step="${escapeHtml(action.step)}"
        ${actionAttributes(action)}
      >${escapeHtml(action.label)}</button>
      <button class="button compact" type="button" data-panel-control="review-settings" data-route="settings">Review settings</button>
      <button class="button compact" type="button" data-panel-control="learn-more" data-route="privacy">Learn more</button>
    </div>
  </article>`;
}

function connectedAppsItem(summary: HomeProtectionSummary): string {
  return `<article class="home-panel-item" data-panel-item="connected-apps" data-count="${summary.connected_app_count}">
    <span class="home-panel-item-label">Connected apps</span>
    <strong class="home-panel-item-value" data-panel-value>${escapeHtml(connectedAppsPhrase(summary.connected_app_count))}</strong>
    <div class="home-panel-item-controls">
      <button class="button compact" type="button" data-panel-control="review-apps" data-route="connections">Review apps</button>
    </div>
  </article>`;
}

function trustedPeopleItem(summary: HomeProtectionSummary): string {
  return `<article class="home-panel-item" data-panel-item="trusted-people" data-count="${summary.trusted_people_count}">
    <span class="home-panel-item-label">Trusted people</span>
    <strong class="home-panel-item-value" data-panel-value>${escapeHtml(trustedPeoplePhrase(summary.trusted_people_count))}</strong>
    <div class="home-panel-item-controls">
      <button class="button compact" type="button" data-panel-control="review-people" data-route="people">Review people</button>
    </div>
  </article>`;
}

function localActivityItem(activity: HomeLocalActivity): string {
  const rows = activity.events
    .slice(0, 3)
    .map(
      (event, index) => `<li class="home-panel-activity-event" data-activity-event="${index}">
        <span class="home-panel-activity-title">${escapeHtml(event.title)}</span>
        <time class="home-panel-activity-time">${escapeHtml(event.recordedAt)}</time>
      </li>`,
    )
    .join("");
  const list = rows ? `<ul class="home-panel-activity-events">${rows}</ul>` : "";
  return `<article class="home-panel-item" data-panel-item="local-activity" data-activity-enabled="${activity.enabled}" data-activity-count="${activity.events.length}">
    <span class="home-panel-item-label">Local activity</span>
    <strong class="home-panel-item-value" data-panel-value>${escapeHtml(localActivityStatusLine(activity))}</strong>
    ${list}
    <div class="home-panel-item-controls">
      <button class="button compact" type="button" data-panel-control="activity-switch" data-activity-switch="${activity.enabled ? "pause" : "turn-on"}">${activity.enabled ? "Pause" : "Turn on"}</button>
      <button class="button compact" type="button" data-panel-control="open-activity" data-route="activity">Open Activity</button>
    </div>
  </article>`;
}

/**
 * The whole panel. Throws when the summary contradicts itself (TASK 0821's
 * rules) or the local activity does (this task's): a wrong count or a
 * recorded-while-off list drawn calmly is worse than a screen that refuses.
 */
export function homeProtectionPanelMarkup(
  summary: HomeProtectionSummary,
  activity: HomeLocalActivity,
): string {
  const errors = [...homeSummaryFactErrors(summary), ...localActivityErrors(activity)];
  if (errors.length > 0) {
    throw new Error(`OSL: Home protection panel is not usable: ${errors.join("; ")}`);
  }
  const action = homeSafeStepAction(summary);
  const statusLine = `${protectionStateLabel(summary.protection_state)} · ${connectedAppsPhrase(summary.connected_app_count)} · ${trustedPeoplePhrase(summary.trusted_people_count)}`;
  return `<section
      class="home-protection-panel"
      data-home-protection-panel="task-0824"
      data-protection-state="${escapeHtml(summary.protection_state)}"
      data-next-safe-step="${escapeHtml(summary.next_safe_step)}"
      aria-labelledby="protection-panel-title"
    >
    <header class="home-panel-head">
      <h2 class="home-panel-title" id="protection-panel-title" tabindex="-1">Protection</h2>
      <p class="home-panel-status" data-home-panel-status>${escapeHtml(statusLine)}</p>
    </header>
    <div class="home-panel-items" data-home-panel-items>
      ${protectionStateItem(summary)}
      ${mainActionItem(action)}
      ${connectedAppsItem(summary)}
      ${trustedPeopleItem(summary)}
      ${localActivityItem(activity)}
    </div>
  </section>`;
}
