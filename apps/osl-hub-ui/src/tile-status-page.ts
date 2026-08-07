/**
 * TASK 0856 — the page a connected-app tile opens.
 *
 * TASK 0852 generated the status data; TASK 0853 turned it into route data and
 * put the label, the capability sentence and one working action on the service
 * route. What it deliberately did not build is the PAGE: the route still shows
 * two lines of prose and inherits `← Apps`, and the `back` action TASK 0853
 * carries ("Back to Home") was never drawn. Its own evidence says so —
 * *"TASK 0856 builds the page that shows it."* This is that page.
 *
 * The page states four things and nothing else:
 *
 *   1. **the real capability** — a generated label from what OSL has actually
 *      proven on this surface, on the same five-rung ladder TASK 0804 checks in
 *      `services.rs::generated_tile_label`, plus the generated capability
 *      sentence off the catalog;
 *   2. **plain limits** — one line per capability OSL does NOT have here, each
 *      saying what is missing and why, plus any governance condition standing
 *      on the row;
 *   3. **a useful action** — {@link tileStatusNextAction} from TASK 0853, which
 *      picks something that works in the next second and is reached by a
 *      handler that already binds;
 *   4. **Back to Home** — the `back` action, finally on screen.
 *
 * WHERE THE CAPABILITY LABEL COMES FROM. Not from the badge, and not typed per
 * app. `services.rs` carries a `ServiceCapabilityFacts` table, but that table
 * records `reading: true` for Telegram and Discord, which sends both to "Ready"
 * on the TASK 0804 ladder — a claim `claim_state.rs` refuses and TASK 0808
 * refuses again (Ready needs a matching delivery proof and no surface has one).
 * So the facts here are derived from the EVIDENCE the catalog carries, where a
 * capability counts only once it has been proven live:
 *
 *   placing            ← carrierEvidence === "provenLiveWithReceipt"
 *   reading            ← deliveryEvidence === "provenLiveBothWays"
 *   opening            ← the app can be opened in its own OSL profile here
 *   protectedMessaging ← both of the first two, with no open condition
 *
 * Under that rule exactly one connected app is anything but "Opens the app" or
 * "Not started": Telegram, which holds the only live carry receipt this project
 * has ever earned (`apps/osl-hub/carry-receipts/telegram.json`) and has never
 * proven delivery. Telegram is **Placing only**, and it is placing-only because
 * of the receipt, not because somebody decided it should read that way.
 */

import type { NativeApp } from "./services";
import {
  futurePromisesIn,
  tileStatusRouteFor,
  type TileStatusRoute,
  type TileStatusRouteAction,
} from "./tile-status-route";

/** What OSL has been shown to do on one surface. Same four facts as TASK 0804. */
export interface TileStatusCapabilityFacts {
  /** OSL can put a message into that app's own composer. */
  placing: boolean;
  /** OSL can read a protected message back out of that app. */
  reading: boolean;
  /** OSL can open that app in a separate OSL profile on this device. */
  opening: boolean;
  /** A protected message has gone to another person through that app and back. */
  protectedMessaging: boolean;
}

/** The five rungs of `services.rs::generated_tile_label`, strongest first. */
export const TILE_STATUS_CAPABILITY_LABELS = [
  "Ready",
  "Placing only",
  "Reading only",
  "Opens the app",
  "Not started",
] as const;

export type TileStatusCapabilityLabel = (typeof TILE_STATUS_CAPABILITY_LABELS)[number];

/**
 * The four capability facts, read off the evidence the catalog carries.
 *
 * Nothing here consults `supportStatus`. The badge answers "what may we say";
 * these answer "what has been shown to work", and the whole point of the claim
 * state is that those are different questions.
 */
export function tileStatusCapabilityFacts(app: NativeApp): TileStatusCapabilityFacts {
  const placing = app.carrierEvidence === "provenLiveWithReceipt";
  const reading = app.deliveryEvidence === "provenLiveBothWays";
  return {
    placing,
    reading,
    opening: app.availability !== "unavailable" && app.isolatedProfileAvailable,
    protectedMessaging: placing && reading && app.claimBlockers.length === 0,
  };
}

/**
 * The generated capability label. This is `generated_tile_label` in
 * `apps/osl-hub/src/services.rs`, rung for rung, and TASK 0804's own table is
 * replayed against it in `task-0856-tile-status-page.test.ts`.
 */
export function tileStatusCapabilityLabel(facts: TileStatusCapabilityFacts): TileStatusCapabilityLabel {
  if (facts.protectedMessaging || (facts.placing && facts.reading)) return "Ready";
  if (facts.placing) return "Placing only";
  if (facts.reading) return "Reading only";
  if (facts.opening) return "Opens the app";
  return "Not started";
}

/** The order the page reads the four facts in. */
export const TILE_STATUS_CAPABILITY_IDS = ["placing", "reading", "opening", "protectedMessaging"] as const;

export type TileStatusCapabilityId = (typeof TILE_STATUS_CAPABILITY_IDS)[number];

/**
 * Every sentence this page is allowed to write, with `{app}` the only hole.
 *
 * A closed list for the same reason TASK 0853 keeps one: a fifth entry is how a
 * future promise would get onto the page, so the check scans this table itself
 * rather than only the assembled page. `held` is what OSL can do; `missing` is
 * the limit, and a limit always says what is absent AND why.
 */
export const TILE_STATUS_CAPABILITY_TEXT: Readonly<
  Record<TileStatusCapabilityId, { readonly name: string; readonly held: string; readonly missing: string }>
> = Object.freeze({
  placing: {
    name: "Place a message in {app}",
    held: "OSL can place a message into {app}'s own composer. A live carry receipt records the cover text landing byte-exact.",
    missing: "OSL cannot place a message into {app}'s own composer. No live carry receipt exists for {app}.",
  },
  reading: {
    name: "Read messages back from {app}",
    held: "OSL can read a protected message back out of {app}.",
    missing: "OSL cannot read a protected message back out of {app}. Delivery through {app} has never been proven against the live client.",
  },
  opening: {
    name: "Open {app} in a separate OSL profile",
    held: "OSL can open {app} in a separate OSL profile, so your normal {app} session stays untouched.",
    missing: "OSL cannot open {app} in a separate OSL profile on this device.",
  },
  protectedMessaging: {
    name: "Protected messaging through {app}",
    held: "OSL can carry a protected message between two people through {app}.",
    missing: "OSL has never carried a protected message between two people through {app}. Nothing you send through {app} is protected by OSL.",
  },
});

/**
 * The governance conditions the catalog can put on a row, in plain words.
 *
 * An id this build does not know is printed verbatim rather than dropped: a
 * condition nobody can read is still better than a condition nobody is told
 * about, and silence here is exactly the failure the claim state exists to
 * refuse.
 */
export const TILE_STATUS_BLOCKER_TEXT: Readonly<Record<string, string>> = Object.freeze({
  "open-security-finding": "An open security finding stands on {app}, so OSL makes no claim about it.",
  "unknown-recheck-required": "A recheck that this build has not run stands on {app}.",
  "send-input-generalisation": "{app}'s adapter refuses synthesised input, which is the only technique any surface here has been shown to land by.",
});

export interface TileStatusPageFact {
  id: string;
  /** The capability, named. */
  name: string;
  /** Whether OSL has it here. */
  held: boolean;
  text: string;
}

export interface TileStatusPage {
  tileId: string;
  route: string;
  title: string;
  /** The public badge, generated from `supportStatus` (TASK 0804/0853). */
  claimLabel: string;
  /** The real capability, generated from the evidence. */
  capabilityLabel: TileStatusCapabilityLabel;
  facts: TileStatusCapabilityFacts;
  /** The generated capability sentence off the catalog, byte for byte. */
  capability: string;
  /** The generated reason off the catalog, byte for byte. */
  explanation: string;
  /** The capabilities OSL holds here. */
  can: TileStatusPageFact[];
  /** The capabilities it does not, plus any open condition. The plain limits. */
  limits: TileStatusPageFact[];
  nextAction: TileStatusRouteAction;
  evidenceAction: TileStatusRouteAction;
  back: TileStatusRouteAction;
}

function fill(template: string, app: NativeApp): string {
  return template.replaceAll("{app}", app.displayName);
}

/** One tile's status page, assembled from its route data and its evidence. */
export function tileStatusPageFor(app: NativeApp, route: TileStatusRoute = tileStatusRouteFor(app)): TileStatusPage {
  const facts = tileStatusCapabilityFacts(app);
  const rows = TILE_STATUS_CAPABILITY_IDS.map((id) => ({
    id,
    name: fill(TILE_STATUS_CAPABILITY_TEXT[id].name, app),
    held: facts[id],
    text: fill(facts[id] ? TILE_STATUS_CAPABILITY_TEXT[id].held : TILE_STATUS_CAPABILITY_TEXT[id].missing, app),
  }));
  const blockers = app.claimBlockers.map((blocker) => ({
    id: `blocker:${blocker}`,
    name: blocker,
    held: false,
    text: fill(TILE_STATUS_BLOCKER_TEXT[blocker] ?? blocker, app),
  }));
  return {
    tileId: route.tileId,
    route: route.route,
    title: route.title,
    claimLabel: route.generatedLabel,
    capabilityLabel: tileStatusCapabilityLabel(facts),
    facts,
    capability: route.capability,
    explanation: route.explanation,
    can: rows.filter((row) => row.held),
    limits: [...rows.filter((row) => !row.held), ...blockers],
    nextAction: route.nextAction,
    evidenceAction: route.evidenceAction,
    back: route.back,
  };
}

/** Every tile's status page, in catalog order. */
export function tileStatusPages(apps: readonly NativeApp[]): TileStatusPage[] {
  return apps.map((app) => tileStatusPageFor(app));
}

/**
 * Every future promise on one assembled page, by field.
 *
 * The generated label is exempt for the reason TASK 0853 gives: "Coming later"
 * is what `comingSoon` generates and TASK 0854 requires this route to agree
 * with it exactly. Everything else on the page — including the capability
 * label, which this module generates — is scanned.
 */
export function tileStatusPagePromises(page: TileStatusPage): { field: string; phrase: string; text: string }[] {
  const scanned: { field: string; text: string }[] = [
    { field: "title", text: page.title },
    { field: "capabilityLabel", text: page.capabilityLabel },
    { field: "capability", text: page.capability },
    { field: "explanation", text: page.explanation },
    ...page.can.map((row) => ({ field: `can:${row.id}`, text: `${row.name}. ${row.text}` })),
    ...page.limits.map((row) => ({ field: `limit:${row.id}`, text: row.text })),
    { field: "nextAction.label", text: page.nextAction.label },
    { field: "evidenceAction.label", text: page.evidenceAction.label },
    { field: "back.label", text: page.back.label },
  ];
  return scanned.flatMap((entry) =>
    futurePromisesIn(entry.text).map((phrase) => ({ field: entry.field, phrase, text: entry.text })));
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

/**
 * The shipped control that performs one route action, on THIS route.
 *
 * TASK 0853's route data names the handler in the abstract (`data-home-app`
 * opens the app, `data-route` navigates). On the app's own service route,
 * `data-home-app` for this same tile would only re-enter the route you are
 * already standing on; the control that actually opens the separate profile
 * here is `#embedded-service-setup`, bound in `main.ts` to `setupEmbeddedApp`.
 * So the action keeps its generated label and its declared handler is recorded
 * in `data-tile-status-handler`, while the id is the one that does the work.
 */
function actionAttributes(action: TileStatusRouteAction, tileId: string): string {
  const hook = action.handler === "data-home-app"
    ? (action.handlerValue === tileId
      ? `id="embedded-service-setup"`
      : `data-home-app="${escapeHtml(action.handlerValue)}"`)
    : `data-route="${escapeHtml(action.handlerValue)}"`;
  return `data-tile-status-action="${escapeHtml(action.target)}" data-tile-status-handler="${escapeHtml(action.handler)}" ${hook}`;
}

function factList(rows: readonly TileStatusPageFact[], kind: "can" | "limit"): string {
  return rows
    .map((row) => `<li class="tile-status-fact tile-status-fact-${kind}" data-tile-status-${kind}="${escapeHtml(row.id)}"><span class="tile-status-fact-mark" aria-hidden="true">${kind === "can" ? "✓" : "✕"}</span><span class="tile-status-fact-text">${escapeHtml(row.text)}</span></li>`)
    .join("");
}

export interface TileStatusPageOptions {
  /** The service logo `main.ts` already draws on this route. Trusted markup. */
  logo?: string;
  /** True while an open is in flight, so the action reads as busy. */
  busy?: boolean;
}

/**
 * The page. One `<main>` with one `id="route-heading"`, so it IS the service
 * route rather than a panel bolted onto it.
 *
 * Every string in it is either generated (the two labels, the capability
 * sentence, the reason, the action labels) or comes out of the two closed
 * tables above. Every control is one `main.ts` already binds:
 * `#embedded-service-setup` opens the separate profile, `[data-route]`
 * navigates, `#burn-button` opens Burn, and `#native-app-back` is the control
 * this page relabels — its handler has always ended `route = "home"`, while its
 * label said "← Apps". "Back to Home" is where it actually goes.
 */
export function tileStatusPageMarkup(app: NativeApp, options: TileStatusPageOptions = {}): string {
  const page = tileStatusPageFor(app);
  const limits = page.limits.length;
  const busy = options.busy === true;
  const logo = options.logo ?? "";
  return `<main class="content-viewport native-app-page tile-status-page" id="route-heading" tabindex="-1" data-tile-status-page="${escapeHtml(page.tileId)}" data-tile-status-route="${escapeHtml(page.route)}" data-tile-status-capability-label="${escapeHtml(page.capabilityLabel)}">`
    + `<section class="native-app-card tile-status-card">`
    // No logo, no empty box: `.service-icon` is a bordered square, and drawing
    // it around nothing is a hole on the page rather than an icon.
    + `<header class="tile-status-header">${logo ? `<span class="service-icon large">${logo}</span>` : ""}<h1 id="tile-status-title">${escapeHtml(page.title)}</h1>`
    + `<p class="tile-status-labels"><span class="status-tag tile-status-capability-tag" data-tile-status-capability="${escapeHtml(page.capabilityLabel)}">${escapeHtml(page.capabilityLabel)}</span><span class="status-tag tile-status-claim-tag" data-tile-status-claim="${escapeHtml(page.claimLabel)}">${escapeHtml(page.claimLabel)}</span></p></header>`
    + `<p class="tile-status-capability-sentence" data-tile-status-capability-sentence="${escapeHtml(page.tileId)}">${escapeHtml(page.capability)}</p>`
    + `<p class="tile-status-explanation" data-tile-status-explanation="${escapeHtml(page.tileId)}">${escapeHtml(page.explanation)}</p>`
    + (page.can.length
      ? `<section class="tile-status-group tile-status-group-can" aria-label="What OSL can do here"><h2>What OSL can do here</h2><ul class="tile-status-facts" data-tile-status-can-count="${page.can.length}">${factList(page.can, "can")}</ul></section>`
      : "")
    + `<section class="tile-status-group tile-status-group-limits" aria-label="What OSL cannot do here"><h2>What OSL cannot do here</h2><ul class="tile-status-facts" data-tile-status-limit-count="${limits}">${factList(page.limits, "limit")}</ul></section>`
    + `<div class="tile-status-actions"><button class="button primary native-app-action tile-status-next" type="button" ${actionAttributes(page.nextAction, page.tileId)} ${busy ? "disabled" : ""}>${busy ? "Opening…" : escapeHtml(page.nextAction.label)}</button>`
    + `<button class="text-button tile-status-evidence" type="button" ${actionAttributes(page.evidenceAction, page.tileId)}>${escapeHtml(page.evidenceAction.label)}</button>`
    + `<button class="text-button tile-status-burn" id="burn-button" type="button" data-open-burn="app">Burn…</button>`
    // `#native-app-back` already ends `route = "home"`, so it needs no
    // `[data-route]` hook of its own — two hooks would render the same
    // navigation twice. The declared target is recorded either way.
    + `<button class="text-back tile-status-back" id="native-app-back" type="button" data-tile-status-action="${escapeHtml(page.back.target)}" data-tile-status-handler="${escapeHtml(page.back.handler)}">${escapeHtml(page.back.label)}</button></div>`
    + `</section></main>`;
}
