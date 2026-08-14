/**
 * TASK 6802 — the one authority for which onboarding pages exist.
 *
 * Owner rulings D4 and D5 consolidated first run. The app-facing result is a
 * single "Set up your apps" page; the structural result is that a fixed set of
 * pages is RETAINED and a fixed set is DELETED, and nothing — fresh run,
 * resume, deep link or migration from an older install — may reach a deleted
 * one.
 *
 * Both the static route inventory (`scripts/check-task-6802-onboarding.mjs`)
 * and the physical Windows crawl (`scripts/task-6802-windows-crawl.mjs`) read
 * THIS file. They are required to agree with it exactly, which is what makes
 * "the inventory and the crawl agree" a measurement rather than two copies of
 * the same list.
 */

/**
 * The setup spine, in order. Every one of these is a page a person walks
 * through on a fresh run, and `onboarding-sequence.ts` must contain exactly
 * this list in exactly this order.
 */
export const RETAINED_SETUP_ROUTES = [
  "welcome",
  "recovery",
  "recovery-check",
  "pro",
  "forward-secrecy",
  "privacy",
  "tor",
  "defaults",
  "sending",
  "cover",
  "visibility",
  "passwords",
  "burnpass",
  "mullvad",
  "browser",
  "setup-apps",
] as const;

/**
 * Entry and repair routes. They are real onboarding pages and are retained,
 * but they branch into or out of the spine rather than being steps in it, so
 * `ONBOARDING_SEQUENCE` deliberately does not list them.
 */
export const RETAINED_ENTRY_ROUTES = [
  "create",
  "import",
  "unlock",
  "keylost",
  "account-recovery",
] as const;

/** Every onboarding page that is allowed to exist. */
export const RETAINED_ONBOARDING_ROUTES = [
  ...RETAINED_SETUP_ROUTES,
  ...RETAINED_ENTRY_ROUTES,
] as const;

export type RetainedOnboardingRoute = (typeof RETAINED_ONBOARDING_ROUTES)[number];

/**
 * Pages the owner deleted, by the identifier each one actually had in the
 * shipping renderer. A route id here may not appear in the onboarding route
 * type, the sequence, the content dispatch, the resume record, a deep link, or
 * anything a migration can produce.
 *
 * `cards-1-5` is the tour's five slides: they were never a separate route id,
 * they were `tutorial` plus the `data-onboarding-tour-step` counter, so the
 * marker below is what the inventory and the crawl look for.
 */
export const DELETED_ONBOARDING_ROUTES = [
  "tutorial",
  "detected",
  "install",
  "apps",
  "silent-visible",
] as const;

export type DeletedOnboardingRoute = (typeof DELETED_ONBOARDING_ROUTES)[number];

/**
 * Deleted pages that never had a route id of their own: they were states of a
 * retained route. Each entry names a marker that must not be produced by any
 * onboarding markup.
 *
 * - `cards-1-5`        — the five tour slides.
 * - `choose-apps`      — the old grouped "Choose apps" picker.
 * - `recovery-empty`   — "No recovery secret is available"; replaced by the
 *                        guarantee that a kit exists before the recovery step.
 * - `mullvad-installed`— the separate installed-Mullvad page, now one card.
 */
export const DELETED_ONBOARDING_PAGE_MARKERS = [
  { id: "cards-1-5", marker: "data-onboarding-tour-step" },
  { id: "choose-apps", marker: "onboarding-app-choices" },
  { id: "recovery-empty", marker: "recovery-empty" },
  { id: "mullvad-installed", marker: "mullvad-installed-page" },
] as const;

/**
 * Per-row states the owner removed from app setup. A row is DETECTED or
 * NOT DETECTED and nothing else; READY and NEEDS CLAIM are the two states that
 * used to leak account-claiming into first run, and their reappearance
 * anywhere in onboarding is a failure by itself.
 */
export const FORBIDDEN_ONBOARDING_STATES = ["READY", "NEEDS CLAIM"] as const;

/**
 * Controls that belong to Settings only. Account claiming and the per-account
 * Desktop/Web choice are the two the rulings moved out of onboarding; each
 * entry is the shipping attribute the control is bound by.
 */
export const SETTINGS_ONLY_ACCOUNT_CONTROLS = [
  "data-detected-account-choice",
  "data-service-current-session",
  "data-native-session-mode",
] as const;

const deleted = new Set<string>(DELETED_ONBOARDING_ROUTES);
const retained = new Set<string>(RETAINED_ONBOARDING_ROUTES);

export function isRetainedOnboardingRoute(route: string): route is RetainedOnboardingRoute {
  return retained.has(route);
}

export function isDeletedOnboardingRoute(route: string): route is DeletedOnboardingRoute {
  return deleted.has(route);
}

/**
 * What a persisted resume value, deep link or migrated preference is allowed
 * to become. A value naming a deleted page resolves to the consolidated app
 * page rather than being honoured or silently dropped, so an install
 * interrupted on "Onboarding Detected" comes back on "Set up your apps".
 */
export const MIGRATED_DELETED_ROUTE_DESTINATION = "setup-apps" as const;

export function migrateOnboardingRoute(candidate: string | null | undefined): RetainedOnboardingRoute | null {
  if (!candidate) return null;
  if (isDeletedOnboardingRoute(candidate)) return MIGRATED_DELETED_ROUTE_DESTINATION;
  return isRetainedOnboardingRoute(candidate) ? candidate : null;
}
