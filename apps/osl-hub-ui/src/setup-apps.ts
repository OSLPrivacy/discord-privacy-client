import "./setup-apps.css";
import type { NativeApp, NativeAppId } from "./services";
import { escapeHtml } from "./services";

/**
 * TASK 6802 — "Set up your apps": the one page first run has for apps.
 *
 * Owner rulings D4/D5 replaced four screens (Choose apps, Onboarding Detected,
 * a separate Install route, Onboarding Apps) with this. The rules the rulings
 * fix, and which this module is the only place to change:
 *
 * 1. Four rows, always: Signal, Discord, Telegram, WhatsApp. The list does not
 *    shrink when nothing is installed and does not grow from the service
 *    catalogue — a row that vanished was how "not detected" used to be hidden.
 * 2. Each row is discovered independently and is EXACTLY one of two states:
 *    DETECTED or NOT DETECTED. There is no third state, no "ready", no "needs
 *    claim", no pending.
 * 3. A detected row can be enabled. A not-detected row is greyed, carries the
 *    reason it was not detected, and offers Install.
 * 4. Account claiming and per-account Desktop/Web choices are NOT here. They
 *    live in Settings. Nothing on this page names an account.
 */

export const SETUP_APP_IDS = ["signal", "discord", "telegram", "whatsapp"] as const;

export type SetupAppId = (typeof SETUP_APP_IDS)[number];

/** The only two states a row may hold. */
export type SetupAppDetection = "DETECTED" | "NOT DETECTED";

export const SETUP_APP_DETECTION_STATES: readonly SetupAppDetection[] = ["DETECTED", "NOT DETECTED"];

/**
 * TASK 6810 — the one state a row passes THROUGH.
 *
 * `Installing` is not a third detection: the PC is still measured as NOT
 * DETECTED underneath, and this label lasts exactly as long as the Windows
 * installer is running. It exists so that pressing Install cannot look like
 * nothing happened, and it can never be a resting state — a finished or failed
 * install always lands back on one of the two measured states above.
 */
export const SETUP_APP_INSTALLING = "Installing" as const;

export type SetupAppRowState = SetupAppDetection | typeof SETUP_APP_INSTALLING;

export const SETUP_APP_ROW_STATES: readonly SetupAppRowState[] = [
  "NOT DETECTED",
  SETUP_APP_INSTALLING,
  "DETECTED",
];

export interface SetupAppRow {
  id: SetupAppId;
  displayName: string;
  detection: SetupAppDetection;
  /** What the row shows: its detection, or `Installing` while one is running. */
  state: SetupAppRowState;
  /** Populated only when the row is NOT DETECTED; never empty in that case. */
  reason: string;
  /** Only a not-detected row that is not already installing offers Install. */
  installOffered: boolean;
  /** Only a detected row offers Open, and Open still has to be pressed. */
  openOffered: boolean;
  /** Only a detected row can be enabled. */
  enabled: boolean;
}

const SETUP_APP_DISPLAY_NAMES: Record<SetupAppId, string> = {
  signal: "Signal",
  discord: "Discord",
  telegram: "Telegram",
  whatsapp: "WhatsApp",
};

export function isSetupAppId(candidate: string): candidate is SetupAppId {
  return (SETUP_APP_IDS as readonly string[]).includes(candidate);
}

/**
 * The whole of the binary rule.
 *
 * The backend reports three availabilities. `installed` — and only `installed`
 * — is a detection: `installable` means Windows could fetch it, which is the
 * opposite of it being here, and `unavailable` means it is neither here nor
 * fetchable. Both are NOT DETECTED, and they differ only in the reason shown
 * and whether Install can do anything.
 */
export function detectionForAvailability(availability: NativeApp["availability"]): SetupAppDetection {
  return availability === "installed" ? "DETECTED" : "NOT DETECTED";
}

export const NOT_DETECTED_INSTALLABLE_REASON = "Not found on this PC. OSL can install it through Windows.";
export const NOT_DETECTED_UNAVAILABLE_REASON = "Not found on this PC, and Windows has no installer for it here.";
export const NOT_DETECTED_UNCHECKED_REASON = "OSL has not been able to check this PC yet.";

function reasonForAvailability(availability: NativeApp["availability"] | null): string {
  if (availability === "installable") return NOT_DETECTED_INSTALLABLE_REASON;
  if (availability === "unavailable") return NOT_DETECTED_UNAVAILABLE_REASON;
  return NOT_DETECTED_UNCHECKED_REASON;
}

/**
 * Build all four rows from whatever the native catalogue answered.
 *
 * A missing catalogue row is NOT DETECTED with the "not checked yet" reason
 * rather than a hidden row: silence about an app is not evidence that it is
 * there, and it is not permission to drop it from the page.
 */
export function setupAppRows(
  nativeApps: readonly NativeApp[],
  enabledApps: ReadonlySet<string>,
  installing: ReadonlySet<string> = new Set<string>(),
  installFailures: ReadonlyMap<string, string> = new Map<string, string>(),
): SetupAppRow[] {
  return SETUP_APP_IDS.map((id) => {
    const native = nativeApps.find((candidate) => candidate.id === id as NativeAppId) ?? null;
    const detection = native ? detectionForAvailability(native.availability) : "NOT DETECTED";
    const detected = detection === "DETECTED";
    const running = !detected && installing.has(id);
    return {
      id,
      displayName: native?.displayName ?? SETUP_APP_DISPLAY_NAMES[id],
      detection,
      state: detected ? "DETECTED" : running ? SETUP_APP_INSTALLING : "NOT DETECTED",
      // A failed install replaces the ordinary "not found" line with the reason
      // it failed. It never leaves the row claiming anything else.
      reason: detected || running
        ? ""
        : installFailures.get(id) ?? reasonForAvailability(native?.availability ?? null),
      installOffered: !detected && !running,
      openOffered: detected,
      // A row that is not detected cannot be enabled, however the saved
      // selection reads. Uninstalling an app must take its switch off, not
      // leave a switch on over an app that is gone.
      enabled: detected && enabledApps.has(id),
    };
  });
}

/** What is persisted across a restart: the enabled detected apps, nothing else. */
export function enabledSetupApps(rows: readonly SetupAppRow[]): SetupAppId[] {
  return rows.filter((row) => row.enabled).map((row) => row.id);
}

function rowMarkup(row: SetupAppRow, logo: (id: SetupAppId) => string): string {
  const detected = row.detection === "DETECTED";
  const running = row.state === SETUP_APP_INSTALLING;
  const rowClass = detected ? "detected" : running ? "installing" : "not-detected";
  const controls = detected
    ? `<label class="setup-app-enable"><input type="checkbox" data-setup-app-enable="${row.id}" ${row.enabled ? "checked" : ""}/><span>Use ${escapeHtml(row.displayName)}</span></label><button class="button compact setup-app-open" type="button" data-setup-app-open="${row.id}" ${row.openOffered ? "" : "disabled"}>Open</button>`
    : running
      ? `<button class="button compact setup-app-install" type="button" disabled>${SETUP_APP_INSTALLING}…</button>`
      : `<button class="button compact setup-app-install" type="button" data-setup-app-install="${row.id}" ${row.installOffered ? "" : "disabled"}>Install</button>`;
  return `<li class="setup-app-row ${rowClass}" data-setup-app="${row.id}" data-setup-app-state="${row.state}">
    <span class="setup-app-logo" aria-hidden="true">${logo(row.id)}</span>
    <span class="setup-app-copy"><strong>${escapeHtml(row.displayName)}</strong><small class="setup-app-state">${row.state}</small>${detected || running ? "" : `<small class="setup-app-reason">${escapeHtml(row.reason)}</small>`}</span>
    ${controls}
  </li>`;
}

export const SETUP_APPS_TITLE = "Set up your apps";

export function setupAppsMarkup(
  rows: readonly SetupAppRow[],
  logo: (id: SetupAppId) => string,
  notice = "",
): string {
  return `<section class="setup-apps" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1">${SETUP_APPS_TITLE}</h1>
    <p class="compact-lead onboarding-centered-copy">OSL checked this PC for each app. Turn on the ones you want. Nothing opens during setup.</p>
    <ul class="setup-app-list" aria-label="Apps OSL checked for">${rows.map((row) => rowMarkup(row, logo)).join("")}</ul>
    ${notice ? `<p class="form-status" id="setup-apps-notice" role="alert">${escapeHtml(notice)}</p><button class="browser-import-skip" id="continue-without-apps" type="button">Continue without Windows apps</button>` : ""}
    <p class="saved-account-truth">Accounts are claimed in Settings, not here.</p>
    <div class="setup-footer onboarding-actions"><button class="button primary" id="continue-setup-apps" type="button">Continue</button></div>
  </section>`;
}
