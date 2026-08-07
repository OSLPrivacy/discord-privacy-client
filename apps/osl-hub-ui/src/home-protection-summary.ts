import type { Route } from "./main";

/**
 * TASK 0821 - the Home protection summary, wired to its main action.
 *
 * TASK 0820 made the backend answer four facts directly from saved state:
 * protection state, trusted people, connected apps, and the next safe step.
 * This module puts those facts on screen AND points the main action at the
 * destination that next step names.
 *
 * The point is the wiring, so nothing here is taken on trust:
 *
 * - the action's destination is looked up from the step string the backend
 *   actually returned. There is no default branch: a step this build does not
 *   know throws instead of quietly routing somewhere plausible;
 * - the two counted sentences are rebuilt from the counts. The summary carries
 *   both a number and its prose, and a screen that prints the prose would show
 *   "2 trusted people" beside a count of 0;
 * - the step itself is re-derived from the counts. A summary whose next step
 *   disagrees with its own facts is refused rather than drawn.
 */

export interface HomeProtectionChoices {
  level: string;
  label: string;
  warnings: string;
  cleanup: string;
  app_exceptions: string;
  contact_rules: string;
}

/** The `HomeProtectionSummaryDto` of TASK 0820, exactly as serde serialises it. */
export interface HomeProtectionSummary {
  protection_state: string;
  privacy_level: string;
  protection_choices: HomeProtectionChoices;
  verification_warning: string;
  trusted_people_count: number;
  trusted_people: string;
  connected_app_count: number;
  allowed_place_count: number;
  apps: string;
  next_safe_step: string;
}

export const HOME_SAFE_STEPS = [
  "Finish account protection",
  "Connect an app",
  "Add a trusted person",
  "Open a protected conversation",
] as const;

export type HomeSafeStep = (typeof HOME_SAFE_STEPS)[number];

export interface HomeSafeStepAction {
  /** The step string as the backend wrote it. */
  step: HomeSafeStep;
  /** Where pressing the main action lands. */
  route: Route;
  settingsSection: "account" | "apps" | null;
  /** Set when the destination is a Home module rather than a bare route. */
  homeModule: "osl-chats" | null;
  label: string;
  detail: string;
}

/**
 * One row per step the backend can return. The destinations match what
 * `homePrimaryActionPlan` in main.ts already does for the same situations, so
 * the summary sends you to the same place the rest of Home would.
 */
const SAFE_STEP_ACTIONS: Readonly<Record<HomeSafeStep, HomeSafeStepAction>> = Object.freeze({
  "Finish account protection": {
    step: "Finish account protection",
    route: "settings",
    settingsSection: "account",
    homeModule: null,
    label: "Fix now",
    detail: "Your account is not protected yet.",
  },
  "Connect an app": {
    step: "Connect an app",
    route: "connections",
    settingsSection: null,
    homeModule: null,
    label: "Connect",
    detail: "No connected app is ready for protected use yet.",
  },
  "Add a trusted person": {
    step: "Add a trusted person",
    route: "people",
    settingsSection: null,
    homeModule: null,
    label: "Add friend",
    detail: "Protected conversations stay unavailable until someone is verified.",
  },
  "Open a protected conversation": {
    step: "Open a protected conversation",
    route: "osl-chat",
    settingsSection: null,
    homeModule: "osl-chats",
    label: "Open OSL Chat",
    detail: "Your account, apps and trusted people are ready.",
  },
});

export function isHomeSafeStep(step: string): step is HomeSafeStep {
  return (HOME_SAFE_STEPS as readonly string[]).includes(step);
}

/** The destination for the step the direct summary named. Never guesses. */
export function homeSafeStepAction(summary: HomeProtectionSummary): HomeSafeStepAction {
  const step = summary.next_safe_step;
  if (!isHomeSafeStep(step)) {
    throw new Error(`OSL: Home summary named an unknown next safe step: ${JSON.stringify(step)}`);
  }
  return SAFE_STEP_ACTIONS[step];
}

export function trustedPeoplePhrase(count: number): string {
  return count === 1 ? "1 trusted person" : `${count} trusted people`;
}

export function connectedAppsPhrase(count: number): string {
  return count === 1 ? "1 connected app" : `${count} connected apps`;
}

export function protectionStateLabel(state: string): string {
  return state === "protected" ? "Protected" : "Needs attention";
}

/** The step these counts call for, worked out here rather than believed. */
export function expectedSafeStep(summary: HomeProtectionSummary): HomeSafeStep {
  if (summary.protection_state !== "protected") return "Finish account protection";
  if (summary.connected_app_count === 0) return "Connect an app";
  if (summary.trusted_people_count === 0) return "Add a trusted person";
  return "Open a protected conversation";
}

/**
 * Every way this summary contradicts itself. Empty means the four facts and the
 * next step agree, so the screen can state them.
 */
export function homeSummaryFactErrors(summary: HomeProtectionSummary): string[] {
  const errors: string[] = [];
  if (summary.protection_state !== "protected" && summary.protection_state !== "needs-attention") {
    errors.push(`unknown protection state: ${JSON.stringify(summary.protection_state)}`);
  }
  if (!Number.isInteger(summary.trusted_people_count) || summary.trusted_people_count < 0) {
    errors.push(`missing trusted-person count: ${JSON.stringify(summary.trusted_people_count)}`);
  } else if (summary.trusted_people !== trustedPeoplePhrase(summary.trusted_people_count)) {
    errors.push(
      `trusted people reads ${JSON.stringify(summary.trusted_people)} beside a count of ${summary.trusted_people_count}`,
    );
  }
  if (!Number.isInteger(summary.connected_app_count) || summary.connected_app_count < 0) {
    errors.push(`missing connected-app count: ${JSON.stringify(summary.connected_app_count)}`);
  } else if (summary.apps !== connectedAppsPhrase(summary.connected_app_count)) {
    errors.push(
      `apps reads ${JSON.stringify(summary.apps)} beside a count of ${summary.connected_app_count}`,
    );
  }
  if (!isHomeSafeStep(summary.next_safe_step)) {
    errors.push(`unknown next safe step: ${JSON.stringify(summary.next_safe_step)}`);
  } else if (errors.length === 0 && summary.next_safe_step !== expectedSafeStep(summary)) {
    errors.push(
      `next safe step ${JSON.stringify(summary.next_safe_step)} does not follow from the saved facts (expected ${JSON.stringify(expectedSafeStep(summary))})`,
    );
  }
  return errors;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

export interface HomeProtectionSummaryFact {
  id: string;
  label: string;
  value: string;
}

/** The sentences the summary states, rebuilt from the counted facts. */
export function homeProtectionSummaryFacts(summary: HomeProtectionSummary): HomeProtectionSummaryFact[] {
  return [
    {
      id: "protection-state",
      label: "Protection",
      value: protectionStateLabel(summary.protection_state),
    },
    {
      id: "trusted-people",
      label: "Trusted people",
      value: trustedPeoplePhrase(summary.trusted_people_count),
    },
    {
      id: "connected-apps",
      label: "Connected apps",
      value: connectedAppsPhrase(summary.connected_app_count),
    },
    {
      id: "next-safe-step",
      label: "Next safe step",
      value: summary.next_safe_step,
    },
  ];
}

/** One line a new person can read without opening anything. */
export function homeProtectionSummarySentence(summary: HomeProtectionSummary): string {
  const state = summary.protection_state === "protected"
    ? "Your account is protected"
    : "Your account still needs attention";
  return `${state} · ${trustedPeoplePhrase(summary.trusted_people_count)} · ${connectedAppsPhrase(summary.connected_app_count)}`;
}

function actionAttributes(action: HomeSafeStepAction): string {
  const section = action.settingsSection ? ` data-settings="${escapeHtml(action.settingsSection)}"` : "";
  const module = action.homeModule ? ` data-home-module="${escapeHtml(action.homeModule)}"` : "";
  return `data-route="${escapeHtml(action.route)}"${section}${module}`;
}

/**
 * The summary and its main action. Throws on a self-contradicting summary: a
 * false count drawn calmly is worse than a screen that refuses to draw.
 */
export function homeProtectionSummaryMarkup(summary: HomeProtectionSummary): string {
  const errors = homeSummaryFactErrors(summary);
  if (errors.length > 0) {
    throw new Error(`OSL: Home protection summary is not usable: ${errors.join("; ")}`);
  }
  const action = homeSafeStepAction(summary);
  const facts = homeProtectionSummaryFacts(summary)
    .map((fact) => `<div class="home-summary-fact" data-summary-fact="${escapeHtml(fact.id)}">
          <span class="home-summary-fact-label">${escapeHtml(fact.label)}</span>
          <strong class="home-summary-fact-value">${escapeHtml(fact.value)}</strong>
        </div>`)
    .join("");
  return `<section
      class="home-summary"
      data-home-protection-summary="task-0821"
      data-protection-state="${escapeHtml(summary.protection_state)}"
      data-next-safe-step="${escapeHtml(summary.next_safe_step)}"
      aria-labelledby="home-summary-title"
    >
    <header class="home-summary-head">
      <h1 class="home-summary-title" id="home-summary-title" tabindex="-1">Home</h1>
      <p
        class="home-summary-sentence"
        id="home-summary-sentence"
        data-home-summary-sentence
      >${escapeHtml(homeProtectionSummarySentence(summary))}</p>
    </header>
    <div class="home-summary-facts" data-home-summary-facts>${facts}</div>
    <footer class="home-summary-action-row">
      <p class="home-summary-action-detail" data-home-summary-action-detail>${escapeHtml(action.detail)}</p>
      <button
        class="button primary home-summary-action"
        id="home-summary-action"
        type="button"
        data-home-summary-action
        data-safe-step="${escapeHtml(action.step)}"
        ${actionAttributes(action)}
      >${escapeHtml(action.label)}</button>
    </footer>
  </section>`;
}
