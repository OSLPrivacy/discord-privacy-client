import "./discovery-visibility.css";
import {
  DISCOVERY_DISCLOSURE_SENTENCE,
  DISCOVERY_PINGS_EXPLANATION,
  DISCOVERY_PINGS_LABEL,
  DISCOVERY_STRIP_ROW_NAME,
  discoveryChoiceRows,
  discoverySelectedChoice,
  type DiscoveryVisibilityState,
} from "./discovery-visibility";

/**
 * The BEING SEEN AS AN OSL USER screen.
 *
 * All four choices are on screen at once, in ruling A10's order, because the
 * question a person is answering is a comparison — how far does this go? — and a
 * picker that showed one at a time would hide the comparison it exists to
 * support. The order is also the safety order: the first choice is the shipped
 * default and the last is the one that cannot be un-done.
 *
 * The disclosure sentence sits under the choices rather than under the "Anyone"
 * radio alone, so it is read before a choice is made rather than after.
 */

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function choicesMarkup(state: DiscoveryVisibilityState): string {
  return discoveryChoiceRows(state)
    .map(
      (row) =>
        `<label class="discovery-choice ${row.checked ? "selected" : ""}" data-discovery-choice-row="${escapeHtml(row.choice.id)}" data-discovery-choice-position="${row.position}"><input class="discovery-radio" type="radio" name="discovery-visibility" value="${escapeHtml(row.choice.id)}" data-discovery-choice="${escapeHtml(row.choice.id)}" ${row.checked ? "checked" : ""}/><span class="discovery-choice-text"><strong>${escapeHtml(row.choice.label)}</strong><small>${escapeHtml(row.choice.explanation)}</small></span></label>`,
    )
    .join("");
}

/**
 * Everything the block shows, without the page element around it, so the same
 * markup can sit inside Settings and stand on its own.
 */
export function discoveryVisibilityBody(state: DiscoveryVisibilityState): string {
  const pingsOn = state.replyToPings;
  return `<header class="discovery-head"><h1 id="discovery-title">Being seen as an OSL user</h1><p class="discovery-intro">This decides who can find out that you run OSL. Nothing here is ever written into Discord, Telegram, email, or any other carrier — the answer is kept as a scrambled record on OSL's own key server, and looking someone up never tells that server who you asked about.</p></header><section class="discovery-choices" aria-labelledby="discovery-choices-title"><h2 id="discovery-choices-title">Who can find you</h2><div class="discovery-choice-list" role="radiogroup" aria-labelledby="discovery-choices-title">${choicesMarkup(state)}</div><p class="discovery-disclosure" data-discovery-disclosure>${escapeHtml(DISCOVERY_DISCLOSURE_SENTENCE)}</p></section><section class="discovery-pings" aria-labelledby="discovery-pings-title"><h2 id="discovery-pings-title" class="sr-only">Discovery pings</h2><label class="discovery-toggle"><input type="checkbox" data-discovery-pings ${pingsOn ? "checked" : ""}/><span class="discovery-toggle-text"><strong>${escapeHtml(DISCOVERY_PINGS_LABEL)}</strong><small>${escapeHtml(DISCOVERY_PINGS_EXPLANATION)}</small></span></label><p class="discovery-pings-note">With this off, OSL publishes nothing and answers nothing, whatever you picked above.</p></section>`;
}

export function discoveryVisibilityScreenMarkup(state: DiscoveryVisibilityState): string {
  return `<main class="content-viewport discovery-screen" id="route-heading" tabindex="-1" aria-labelledby="discovery-title">${discoveryVisibilityBody(state)}</main>`;
}

/**
 * The Strip row that replaced the old two-state stranger-findability cycler. It
 * shows the choice in force and nothing else; the only thing clicking it does is
 * open this screen, where the consent gate lives.
 */
export function discoveryStripRowMarkup(state: DiscoveryVisibilityState): string {
  const selected = discoverySelectedChoice(state);
  return `<section class="discovery-strip-row" data-discovery-strip-row><div class="discovery-strip-text"><strong>${escapeHtml(DISCOVERY_STRIP_ROW_NAME)}</strong><small>Change this in Settings, where OSL can explain what it costs.</small></div><span class="discovery-strip-value" data-discovery-strip-value>${escapeHtml(selected.label)}</span><button class="button compact" data-route="settings" data-settings="discovery" data-discovery-open-settings type="button">Open Settings</button></section>`;
}
