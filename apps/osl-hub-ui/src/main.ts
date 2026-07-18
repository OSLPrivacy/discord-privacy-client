import "@fontsource-variable/inter/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  parseSetupState,
  type SetupState,
} from "./state";
import { isTauriRuntime, loadOnboardingPreferences, saveOnboardingPreferences } from "./preferences";
import {
  escapeHtml,
  closeEmbeddedServiceHost,
  configuredTopStripApps,
  detachNativeAppWindow,
  embeddedAccountsForHomeApp,
  focusNativeAppWindow,
  loadBrowserImports,
  homeAppsFromServices,
  hostNativeAppWindow,
  installNativeApp,
  loadLinkedServices,
  loadNativeApps,
  openBrowserImport,
  openEmbeddedHomeApp,
  resizeNativeAppWindow,
  setupEmbeddedHomeApp,
  type EmailProvider,
  type EmbeddedServiceHost,
  type HomeAppCatalogEntry,
  type HomeAppId,
  type LinkedService,
  type NativeApp,
  type NativeAppId,
  type BrowserImportStatus,
  type ServiceId,
} from "./services";
import {
  coreReadinessLabel,
  clearHubActivationCode,
  createHubOslIdentity,
  importHubOslIdentityPhrase,
  isActivationCode,
  isCoreProtectionReady,
  isRecoveryPhrase,
  isValidMainPassword,
  isValidNewMainPassword,
  loadCoreIntegration,
  loadHubLicenseState,
  loadHubPasswordRoleStatus,
  removeHubAlternatePassword,
  setHubAlternatePassword,
  setupHubMainPassword,
  unavailableCoreIntegration,
  unconfiguredLicenseState,
  unlockHubMainPassword,
  validateHubActivationCode,
  type CoreIntegration,
  type HubLicenseState,
  type HubPasswordRoleStatus,
} from "./core";
import { checkHubForUpdates, installHubUpdate, openHubReleasesPage, type UpdateStatus } from "./updates";
import { serviceLogo, providerLogo } from "./logos";
import { activateLocalLoopbackContext, addOslFriend, burnActiveHubContext, burnHubServiceAccount, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubServiceBurnReadiness, listHubIdentities, listHubPeople, loadAppNotifications, loadFriendProfile, prepareLocalProtectedText, recoverHubIdentitySlot, saveActiveContextSecurity, scanLocalPrivacy, setActiveHubFriendPermission, setHubFriendNickname, setLocalProtectedSheetOpen, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, type AppNotification, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type LocalPrivacyScanResult } from "./adapters";
import { blankLocalProtectedModel, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { withNativeDeadline } from "./native-deadline";
import { FrameRenderScheduler } from "./render-scheduler";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";

type Route = "onboarding" | "home" | "service" | "settings";
type OnboardingRoute = "welcome" | "create" | "import" | "unlock" | "recovery" | "tutorial" | "apps" | "sending" | "scrub";
type SettingsSection = "account" | "apps" | "scrub" | "cleanup" | "notifications" | "appearance" | "about";
type ThemeChoice = "system" | "dark" | "light";
type SavedAccountMode = "ask" | "use" | "clean";
type BurnScope = "chat" | "app" | "account";
type BurnResult = {
  tone: "success" | "warning" | "error";
  message: string;
  showUninstall: boolean;
};
type OwnedConfirmation =
  | { kind: "verifyFriend"; personId: string; verificationCode: string }
  | { kind: "clearActivation" };

function requireRoot(): HTMLDivElement {
  const element = document.querySelector<HTMLDivElement>("#app");
  if (!element) throw new Error("OSL Privacy root is missing");
  return element;
}
const root = requireRoot();

function manualSendingAnimationMarkup(): string {
  return `<div class="manual-send-demo" role="img" aria-label="OSL encrypts a message on this device. You review it, copy it, and paste it into the app yourself."><span>Write</span><i aria-hidden="true"></i><span>Encrypt</span><i aria-hidden="true"></i><span>Copy</span><i aria-hidden="true"></i><span>Send</span></div>`;
}

function passwordEyeIcon(visible = false): string {
  return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2.5 12s3.4-6 9.5-6 9.5 6 9.5 6-3.4 6-9.5 6-9.5-6-9.5-6Z"/><circle cx="12" cy="12" r="2.7"/>${visible ? '<path d="m4 4 16 16"/>' : ""}</svg>`;
}

let services: LinkedService[] = [];
let core: CoreIntegration = structuredClone(unavailableCoreIntegration);
let licenseState: HubLicenseState = structuredClone(unconfiguredLicenseState);
let massCleanupCapabilities: MassCleanupCapabilityManifest | null = null;
let massCleanupLoading = false;
let passwordRoleStatus: HubPasswordRoleStatus | null = null;
let setup: SetupState = parseSetupState(null);
let route: Route = "onboarding";
let onboardingRoute: OnboardingRoute = "welcome";
let settingsSection: SettingsSection = "account";
let activeService: LinkedService | null = null;
let activeHomeAppId: HomeAppId | null = null;
let appLaunchPendingId: HomeAppId | null = null;
let nativeApps: NativeApp[] = [];
let browserImports: BrowserImportStatus[] = [];
let savedAccountMode: SavedAccountMode = "ask";
let savedNativeApps = new Set<NativeAppId>();
const backgroundInstallIds = new Set<NativeAppId>();
const selectedFirstInstallApps = new Set<NativeAppId>();
let backgroundInstallQueue: Promise<void> = Promise.resolve();
let nativeActionBusy = false;
let onboardingServiceSetup = false;
let activeEmbeddedHost: EmbeddedServiceHost | null = null;
let activeNativeHostId: NativeAppId | null = null;
let serviceAccountPickerOpen = false;
let timer = "72h";
let toastTimer: number | undefined;
let updateStatus: UpdateStatus = { state: "unavailable" };
let recoveryBundle: { userId: string; identityPhrase: string | null; passwordPhrase: string } | null = null;
let decryptDisplay = true;
let themeChoice: ThemeChoice = parseTheme(localStorage.getItem("osl-hub-theme"));
let sidebarOrder: string[] = [];
let hiddenServices = new Set<string>();
let homeEditMode = false;
let homeTileOrder: string[] = [];
let hiddenHomeTiles = new Set<string>();
let draggingHomeTileId: string | null = null;
let friendCode: string | null = null;
let friendDisplayId: string | null = null;
let appNotifications: AppNotification[] | null = null;
let notificationsEnabled = false;
let notificationAppPreferences: Partial<Record<ServiceId, boolean>> = {};
let notificationPreviewContent = false;
let notificationScopeSuggestions = true;
let activeContextToken: string | null = null;
let localProtectedSheet: LocalProtectedSheetModel = blankLocalProtectedModel();
let onboardingComplete = false;
let screenshotProtectionEnabled = false;
let hubIdentities: HubIdentitySlot[] = [];
let newIdentityRecoveryPhrase: string | null = null;
let hubPeople: HubPerson[] = [];
let privacyScanResult: LocalPrivacyScanResult | null = null;
let privacyScanFileName: string | null = null;
let privacyScanBusy = false;
let enabledScrubSignals = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
let selectedScrubFindings = new Set<number>();
let scrubResultsPage = 0;
let scrubReviewOpen = false;
let scrubReviewPage = 0;
let scrubReviewConfirmed = false;
let lastFocusKey = "";
let lastWorkspaceMarkup: string | null = null;
let lastWorkspaceViewKey = "";
let deferredBackgroundRender = false;
let serviceGuideStep: ServiceGuideStep | null = null;
let friendsDialogOpen = false;
let friendsDialogPage = 0;
let burnDialogOpen = false;
let burnScope: BurnScope = "chat";
let burnBusy = false;
let burnResult: BurnResult | null = null;
let serviceBurnReadiness: HubServiceBurnReadiness | null = null;
let serviceBurnReadinessBusy = false;
let ownedConfirmation: OwnedConfirmation | null = null;
let ownedConfirmationBusy = false;
let ownedConfirmationError = "";
let navigationIntentEpoch = 0;
let bootstrapEpoch = 0;

const sidebarStorageKey = "osl-hub-sidebar";
const hiddenStorageKey = "osl-hub-sidebar-hidden";
const notificationsStorageKey = "osl-hub-notifications";
const notificationAppsStorageKey = "osl-hub-notification-apps";
const notificationPreviewStorageKey = "osl-hub-notification-previews";
const notificationScopeStorageKey = "osl-hub-notification-scope-suggestions";
const screenshotProtectionStorageKey = "osl-hub-screenshot-protection";
const scrubSignalsStorageKey = "osl-hub-scrub-signals-v1";
const serviceGuideStorageKey = "osl-hub-service-guide-v1";
const homeTileOrderStorageKey = "osl-home-tile-order-v1";
const hiddenHomeTilesStorageKey = "osl-home-tile-hidden-v1";
const savedAccountModeStorageKey = "osl-saved-account-mode-v1";
const savedNativeAppsStorageKey = "osl-saved-native-apps-v1";
const supportedNativeAppIds = new Set<NativeAppId>(["discord", "telegram", "signal", "whatsapp"]);
const friendsDialogPageSize = 24;
const friendScopeRenderLimit = 16;
const scrubResultsPageSize = 50;
const scrubReviewPageSize = 20;
const bootCoreDeadlineMs = 4_000;
const bootPreferenceDeadlineMs = 1_500;
const bootSupportDeadlineMs = 2_000;

function parseTheme(raw: string | null): ThemeChoice {
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

function parseSavedAccountMode(raw: string | null): SavedAccountMode {
  return raw === "use" || raw === "clean" ? raw : "ask";
}

function applyTheme(choice: ThemeChoice): void {
  const resolved = choice === "system"
    ? (window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark")
    : choice;
  document.documentElement.dataset.theme = resolved;
  document.documentElement.dataset.themeChoice = choice;
}

function orderedServices(): LinkedService[] {
  const byId = new Map(services.map((service) => [service.id, service]));
  const ordered = sidebarOrder.flatMap((id) => {
    const service = byId.get(id as ServiceId);
    if (!service) return [];
    byId.delete(id as ServiceId);
    return [service];
  });
  return [...ordered, ...[...byId.values()].sort((a, b) => a.sidebarOrder - b.sidebarOrder)];
}

function loadUiPreferences(): void {
  try {
    const order = JSON.parse(localStorage.getItem(sidebarStorageKey) ?? "[]") as unknown;
    if (Array.isArray(order)) sidebarOrder = order.filter((id): id is string => typeof id === "string").slice(0, 20);
    const hidden = JSON.parse(localStorage.getItem(hiddenStorageKey) ?? "[]") as unknown;
    if (Array.isArray(hidden)) hiddenServices = new Set(hidden.filter((id): id is string => typeof id === "string").slice(0, 20));
    const tileOrder = JSON.parse(localStorage.getItem(homeTileOrderStorageKey) ?? "[]") as unknown;
    if (Array.isArray(tileOrder)) homeTileOrder = tileOrder.filter((id): id is string => typeof id === "string").slice(0, 32);
    const hiddenTiles = JSON.parse(localStorage.getItem(hiddenHomeTilesStorageKey) ?? "[]") as unknown;
    if (Array.isArray(hiddenTiles)) hiddenHomeTiles = new Set(hiddenTiles.filter((id): id is string => typeof id === "string").slice(0, 32));
    const notificationApps = JSON.parse(localStorage.getItem(notificationAppsStorageKey) ?? "{}") as unknown;
    if (typeof notificationApps === "object" && notificationApps !== null && !Array.isArray(notificationApps)) {
      notificationAppPreferences = Object.fromEntries(Object.entries(notificationApps).filter(([, enabled]) => typeof enabled === "boolean").slice(0, 20)) as Partial<Record<ServiceId, boolean>>;
    }
    const savedApps = JSON.parse(localStorage.getItem(savedNativeAppsStorageKey) ?? "[]") as unknown;
    if (Array.isArray(savedApps)) savedNativeApps = new Set(savedApps.filter((id): id is NativeAppId => typeof id === "string" && supportedNativeAppIds.has(id as NativeAppId)));
  } catch {
    sidebarOrder = [];
    hiddenServices.clear();
    homeTileOrder = [];
    hiddenHomeTiles.clear();
    notificationAppPreferences = {};
    savedNativeApps.clear();
  }
  savedAccountMode = parseSavedAccountMode(localStorage.getItem(savedAccountModeStorageKey));
  notificationsEnabled = localStorage.getItem(notificationsStorageKey) === "true";
  notificationPreviewContent = localStorage.getItem(notificationPreviewStorageKey) === "true";
  notificationScopeSuggestions = localStorage.getItem(notificationScopeStorageKey) !== "false";
  screenshotProtectionEnabled = localStorage.getItem(screenshotProtectionStorageKey) === "true";
  enabledScrubSignals = parseScrubSignalGroups(localStorage.getItem(scrubSignalsStorageKey));
}

function saveHomeTilePreferences(): void {
  localStorage.setItem(homeTileOrderStorageKey, JSON.stringify(homeTileOrder));
  localStorage.setItem(hiddenHomeTilesStorageKey, JSON.stringify([...hiddenHomeTiles]));
}

function compactFriendId(value: string): string {
  const normalized = value.replace(/[^A-Za-z0-9]/g, "").toUpperCase();
  if (!normalized) return "Unavailable";
  if (normalized.length <= 16) return normalized.match(/.{1,4}/g)?.join(" ") ?? normalized;
  return `${normalized.slice(0, 8).match(/.{1,4}/g)?.join(" ")} … ${normalized.slice(-4)}`;
}

function commitRender(): void {
  try {
    if (route === "onboarding") renderOnboarding();
    else renderWorkspace();
    bindDesktopTitlebar();
    const focusKey = route === "onboarding"
      ? `${route}:${onboardingRoute}`
      : route === "settings"
        ? `${route}:${settingsSection}`
        : route === "service"
          ? `${route}:${activeService?.id ?? "none"}:${serviceGuideStep ?? "app"}`
          : route;
    if (focusKey !== lastFocusKey) {
      lastFocusKey = focusKey;
      document.querySelector<HTMLElement>(".content-viewport, .onboarding-panel")?.classList.add("view-enter");
      requestAnimationFrame(() => {
        if (focusKey === lastFocusKey) document.querySelector<HTMLElement>("#route-heading")?.focus();
      });
    }
  } catch {
    showRenderRecovery();
  }
}

const renderScheduler = new FrameRenderScheduler(
  (callback) => requestAnimationFrame(callback),
  (handle) => cancelAnimationFrame(handle),
  commitRender,
);

function render(): void {
  renderScheduler.request();
}

function renderNow(): void {
  renderScheduler.flush();
}

function scheduleBackgroundRender(): void {
  render();
}

function renderWhenIdle(): void {
  const active = document.activeElement;
  if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement) {
    if (deferredBackgroundRender) return;
    deferredBackgroundRender = true;
    active.addEventListener("blur", () => {
      if (!deferredBackgroundRender) return;
      deferredBackgroundRender = false;
      scheduleBackgroundRender();
    }, { once: true });
    return;
  }
  deferredBackgroundRender = false;
  scheduleBackgroundRender();
}

function showRenderRecovery(): void {
  renderScheduler.cancel();
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  root.innerHTML = `<main class="ui-recovery" role="alert" aria-labelledby="ui-recovery-title"><img src="${oslLogoUrl}" alt=""/><h1 id="ui-recovery-title">OSL paused this view</h1><p>No error details were displayed or sent.</p><button class="button primary" id="ui-recovery-reload">Reload interface</button></main>`;
  document.querySelector("#ui-recovery-reload")?.addEventListener("click", () => window.location.reload());
}

type WorkspaceFieldSnapshot = {
  id: string;
  value?: string;
  checked?: boolean;
  selectionStart?: number | null;
  selectionEnd?: number | null;
};

type WorkspaceFocusSnapshot = {
  focusedId: string | null;
  fields: WorkspaceFieldSnapshot[];
};

function workspaceViewKey(): string {
  if (route === "settings") return `${route}:${settingsSection}`;
  if (route === "service") return `${route}:${activeService?.id ?? "none"}:${activeHomeAppId ?? "none"}:${serviceGuideStep ?? "app"}`;
  return route;
}

/** Preserve only ordinary form state during a same-view patch. Passwords and files are never copied. */
function captureWorkspaceFocus(surface: HTMLElement): WorkspaceFocusSnapshot {
  const active = document.activeElement;
  const focusedId = active instanceof HTMLElement && surface.contains(active) && active.id ? active.id : null;
  const fields = [...surface.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input[id], textarea[id], select[id]")]
    .filter((field) => !(field instanceof HTMLInputElement) || (field.type !== "password" && field.type !== "file" && field.type !== "hidden"))
    .map((field): WorkspaceFieldSnapshot => {
      if (field instanceof HTMLInputElement && (field.type === "checkbox" || field.type === "radio")) {
        return { id: field.id, checked: field.checked };
      }
      const selection = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement;
      return {
        id: field.id,
        value: field.value,
        selectionStart: selection ? field.selectionStart : null,
        selectionEnd: selection ? field.selectionEnd : null,
      };
    });
  return { focusedId, fields };
}

function restoreWorkspaceFocus(snapshot: WorkspaceFocusSnapshot): void {
  for (const field of snapshot.fields) {
    const element = document.getElementById(field.id);
    if (element instanceof HTMLInputElement && typeof field.checked === "boolean") element.checked = field.checked;
    else if ((element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) && field.value !== undefined) {
      element.value = field.value;
      if ((element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) && field.selectionStart !== undefined) {
        try { element.setSelectionRange(field.selectionStart ?? 0, field.selectionEnd ?? field.selectionStart ?? 0); } catch { /* Some input types do not expose a selection. */ }
      }
    }
  }
  if (snapshot.focusedId) document.getElementById(snapshot.focusedId)?.focus({ preventScroll: true });
}

function containBackgroundFailure(): void {
  if (!root.querySelector(".app-frame")) {
    showRenderRecovery();
    return;
  }
  showToast("That action failed. Nothing changed.");
}

function desktopTitlebar(): string {
  return `<header class="desktop-titlebar"><div class="desktop-drag-region" data-tauri-drag-region aria-hidden="true"></div><div class="window-controls"><button id="window-minimize" aria-label="Minimize"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize or restore"><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-close" class="window-close" aria-label="Close"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button></div></header>`;
}

function bindDesktopTitlebar(): void {
  const appWindow = getCurrentWindow();
  const bindOnce = (selector: string, action: () => Promise<void>): void => {
    const button = document.querySelector<HTMLButtonElement>(selector);
    if (!button || button.dataset.windowControlBound === "true") return;
    button.dataset.windowControlBound = "true";
    button.addEventListener("click", () => void action().catch(() => undefined));
  };
  bindOnce("#window-minimize", () => appWindow.minimize());
  bindOnce("#window-maximize", () => appWindow.toggleMaximize());
  bindOnce("#window-close", () => appWindow.close());
}

function renderOnboarding(): void {
  const canSkipSetup = onboardingRoute === "tutorial" || onboardingRoute === "apps" || onboardingRoute === "sending" || onboardingRoute === "scrub";
  const markup = `<div class="app-frame">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}</main>${canSkipSetup ? '<button class="onboarding-skip-dock" id="skip-onboarding">Skip · manual setup</button>' : ""}</div>${scrubReviewDialogMarkup()}</div>`;
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  root.innerHTML = markup;
  bindOnboarding();
  openScrubReviewDialogAfterRender();
}

function onboardingContent(): string {
  if (onboardingRoute === "welcome") {
    const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired";
    const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired;
    const primaryRoute: OnboardingRoute = partialIdentity ? "create" : returning ? "unlock" : "create";
    const primaryLabel = partialIdentity ? "Finish setup" : returning ? "Unlock this device" : "Create account";
    return `<section class="signin-card" aria-labelledby="route-heading">
      <img class="signin-logo" src="${oslLogoUrl}" alt=""/>
      <h1 id="route-heading" tabindex="-1">${partialIdentity ? "Finish your account" : returning ? "Sign in" : "Create your OSL account"}</h1>
      <button class="button primary signin-primary" data-onboarding="${primaryRoute}">${primaryLabel}</button>
      <button class="signin-link" data-onboarding="import">Use a recovery phrase</button>
      ${returning ? `<div class="signin-divider" aria-hidden="true"><span></span></div><p class="signin-new">New to OSL?</p><button class="button signin-create" data-onboarding="create">Create account</button>` : ""}
    </section>`;
  }

  if (onboardingRoute === "create") return identityPasswordForm("Create a password", "Create account", "setup");
  if (onboardingRoute === "unlock") return identityPasswordForm("Unlock OSL", "Unlock", "unlock");
  if (onboardingRoute === "import") return importIdentityForm();
  if (onboardingRoute === "recovery") return recoveryContent();
  if (onboardingRoute === "tutorial") return tutorialContent();
  if (onboardingRoute === "apps") return onboardingAppsContent();
  if (onboardingRoute === "scrub") return onboardingScrubContent();

  return sendingSetupContent();
}

function tutorialContent(): string {
  const installed = nativeApps.filter((app) => app.availability === "installed");
  const nativeRows = nativeApps.map((app) => {
    if (app.availability === "installed") {
      return `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Installed</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${savedNativeApps.has(app.id) ? "checked" : ""}/></label>`;
    }
    if (app.availability === "installable") {
      const installing = backgroundInstallIds.has(app.id);
      return `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${installing ? "Installing…" : "Install in background"}</small></span></span><input type="checkbox" data-first-install="${app.id}" ${selectedFirstInstallApps.has(app.id) ? "checked" : ""} ${installing ? "disabled" : ""}/></label>`;
    }
    return `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Unavailable on this PC</small></span></span></div>`;
  }).join("");
  const browserButtons = browserImports.filter((browser) => browser.installed).map((browser) => `<button class="button compact" type="button" data-browser-import="${browser.id}">Open ${escapeHtml(browser.displayName)} import</button>`).join("");
  const browserNotice = `<div class="saved-account-browser-note"><strong>Browser passwords stay in your browser</strong><small>Chrome, Edge, Firefox, Brave, Opera, and Vivaldi keep control of import and consent. OSL never reads their password files.</small>${browserButtons ? `<div class="browser-import-actions">${browserButtons}</div>` : ""}</div>`;
  return `<h1 id="route-heading" tabindex="-1">Choose how accounts open</h1><div class="saved-account-animation" data-mode="${savedAccountMode}" aria-label="Use an account already signed in on this PC, or start with a separate signed-out OSL profile."><span class="saved-account-source">PC</span><span class="saved-account-flow"></span><span class="saved-account-destination">OSL</span></div><div class="saved-account-choices"><button class="setting-option ${savedAccountMode === "use" ? "selected" : ""}" data-saved-account-mode="use"><strong>Use existing account</strong><small>${installed.length ? `${installed.length} installed ${installed.length === 1 ? "app" : "apps"} available` : "No installed apps found"}</small></button><button class="setting-option ${savedAccountMode === "clean" ? "selected" : ""}" data-saved-account-mode="clean"><strong>Start fresh</strong><small>Separate OSL profile</small></button></div><p class="saved-account-truth">Nothing opens or installs without your choice.</p><fieldset class="saved-account-advanced first-install-apps"><legend>Apps</legend><div>${nativeRows}</div></fieldset>${browserNotice}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-account-setup" ${savedAccountMode === "ask" ? "disabled" : ""}>Continue</button></div>`;
}

function onboardingAppsContent(): string {
  const apps = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available");
  const choices = apps.length
    ? `<div class="onboarding-app-grid" role="list">${apps.map((app) => `<button type="button" class="onboarding-app" data-onboarding-app="${app.id}" aria-label="Connect ${escapeHtml(app.displayName)} inside OSL"><span class="app-logo-plate">${homeAppLogo(app)}</span><strong>${escapeHtml(app.displayName)}</strong></button>`).join("")}</div>`
    : `<div class="empty-state"><strong>Apps are unavailable</strong><p>Skip for now and add one from Home.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Connect one app</h1>${choices}`;
}

function persistSavedAccountPreferences(): void {
  localStorage.setItem(savedAccountModeStorageKey, savedAccountMode);
  localStorage.setItem(savedNativeAppsStorageKey, JSON.stringify([...savedNativeApps]));
}

function bindSavedAccountControls(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-saved-account-mode]").forEach((button) => button.addEventListener("click", () => {
    savedAccountMode = parseSavedAccountMode(button.dataset.savedAccountMode ?? null);
    if (savedAccountMode === "use" && savedNativeApps.size === 0) {
      savedNativeApps = new Set(nativeApps.filter((app) => app.availability === "installed").map((app) => app.id));
    }
    persistSavedAccountPreferences();
    render();
  }));
  document.querySelectorAll<HTMLInputElement>("[data-saved-native]").forEach((input) => input.addEventListener("change", () => {
    const appId = input.dataset.savedNative as NativeAppId;
    if (!supportedNativeAppIds.has(appId)) return;
    if (input.checked) savedNativeApps.add(appId);
    else savedNativeApps.delete(appId);
    persistSavedAccountPreferences();
  }));
  document.querySelectorAll<HTMLInputElement>("[data-first-install]").forEach((input) => input.addEventListener("change", () => {
    const appId = input.dataset.firstInstall as NativeAppId;
    if (!supportedNativeAppIds.has(appId)) return;
    if (input.checked) selectedFirstInstallApps.add(appId);
    else selectedFirstInstallApps.delete(appId);
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-browser-import]").forEach((button) => button.addEventListener("click", async () => {
    const browserId = button.dataset.browserImport as BrowserImportStatus["id"];
    button.disabled = true;
    try {
      await openBrowserImport(browserId);
      showToast("Browser import settings opened");
    } catch (failure) {
      showToast(localActionError(failure, "Browser import settings could not open"));
    } finally {
      button.disabled = false;
    }
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-background-install]").forEach((button) => button.addEventListener("click", () => {
    void startBackgroundInstall(button.dataset.backgroundInstall as NativeAppId);
  }));
}

function importIdentityForm(): string {
  return `<button class="text-back" data-onboarding="welcome">← Back</button><h1 id="route-heading" tabindex="-1">Restore your account</h1><form class="setup-surface password-form" id="identity-import-form" novalidate><label for="identity-recovery-phrase">Recovery phrase</label><textarea id="identity-recovery-phrase" rows="3" autocomplete="off" autocapitalize="none" spellcheck="false" required aria-describedby="import-error"></textarea><small>Stays on this device.</small><label for="import-password">New password</label><div class="password-input-row"><input id="import-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password" aria-controls="import-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small>6 minimum. 12+ suggested.</small><label for="import-password-confirm">Confirm password</label><div class="password-input-row"><input id="import-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password-confirm" aria-controls="import-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="import-error" role="alert"></p><button class="button primary" id="identity-import-submit" type="submit" disabled>Restore</button></form>`;
}

function recoveryContent(): string {
  if (!recoveryBundle) return `<p class="eyebrow">Recovery</p><h1 id="route-heading" tabindex="-1">No recovery secret is available</h1><button class="button primary" data-onboarding="tutorial">Continue</button>`;
  const accountRecovery = recoveryBundle.identityPhrase ? `<code>${escapeHtml(recoveryBundle.identityPhrase)}</code>` : `<p>Keep using the account recovery phrase you imported.</p>`;
  return `<p class="eyebrow">One-time recovery</p><h1 id="route-heading" tabindex="-1">Save your recovery kit</h1><section class="setup-surface recovery-surface"><article class="recovery-kit-item"><span>1</span><div><strong>Account recovery</strong>${accountRecovery}</div></article><article class="recovery-kit-item"><span>2</span><div><strong>Password recovery</strong><code>${escapeHtml(recoveryBundle.passwordPhrase)}</code></div></article><details class="recovery-account-details"><summary>Account details</summary><code>${escapeHtml(recoveryBundle.userId)}</code></details><label class="check"><input id="recovery-saved" type="checkbox"/><span>I saved my recovery kit.</span></label><button class="button primary" id="recovery-continue" disabled>Continue</button></section>`;
}

function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  if (!setup) return `<section class="unlock-card" aria-labelledby="route-heading"><button class="text-back" data-onboarding="welcome">← Back</button><h1 id="route-heading" tabindex="-1">Enter your password</h1><form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>Unlock</button></form></section>`;
  return `<button class="text-back" data-onboarding="welcome">← Back</button><h1 id="route-heading" tabindex="-1">${title}</h1><form class="setup-surface password-form" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>${action}</button></form>`;
}

function sendingSetupContent(): string {
  return `<h1 id="route-heading" tabindex="-1">Send with copy & paste</h1>${manualSendingAnimationMarkup()}<p class="saved-account-truth">OSL prepares encrypted text. You review, copy, paste, and send it yourself.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding">Continue</button></div>`;
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function onboardingScrubContent(): string {
  if (scrubReviewConfirmed) {
    return `<p class="eyebrow">Scrub</p><h1 id="route-heading" tabindex="-1">Your list is confirmed</h1><p class="compact-lead scrub-local-promise"><strong>Your messages never leave this device.</strong> Find each checked message in its original app and decide whether to delete it there.</p><ol class="scrub-manual-directions"><li>Open the app and chat shown for each suggestion.</li><li>Check that you sent the exact message.</li><li>Delete it yourself only if you still want to.</li></ol><aside class="scrub-pro-compact"><span>PRO · COMING SOON</span><strong>AutoScrub assistant</strong><p>Schedules local scans and prepares a list. You still review and confirm every batch.</p></aside><div class="setup-footer"><button class="button primary" id="complete-onboarding">Done</button></div>`;
  }
  const scanAction = `<label class="button ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose message export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>`;
  const results = privacyScanResult ? privacyScanResultsMarkup() : "";
  return `<h1 id="route-heading" tabindex="-1">Try Scrub</h1><p class="compact-lead scrub-local-promise"><strong>Optional. Your messages never leave this device.</strong> Choose a supported export only if you have one.</p><div class="onboarding-scrub-actions">${scanAction}</div>${privacyScanResult ? scrubCategoryChooserMarkup(true) : ""}${results}<div class="setup-footer onboarding-actions"><button class="button primary" id="complete-onboarding">Finish setup</button></div>`;
}

function bindOnboarding(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboarding as OnboardingRoute; render(); }));
  bindSavedAccountControls();
  bindPasswordVisibility();
  bindPasswordForm();
  bindImportForm();
  const recoverySaved = document.querySelector<HTMLInputElement>("#recovery-saved");
  const recoveryContinue = document.querySelector<HTMLButtonElement>("#recovery-continue");
  recoverySaved?.addEventListener("change", () => { if (recoveryContinue) recoveryContinue.disabled = !recoverySaved.checked; });
  recoveryContinue?.addEventListener("click", () => { recoveryBundle = null; onboardingRoute = "tutorial"; render(); });
  document.querySelector<HTMLButtonElement>("#continue-account-setup")?.addEventListener("click", () => {
    if (savedAccountMode === "ask") return;
    persistSavedAccountPreferences();
    const selectedInstalls = [...selectedFirstInstallApps];
    selectedFirstInstallApps.clear();
    if (selectedInstalls.length) enqueueBackgroundInstalls(selectedInstalls);
    onboardingRoute = "apps";
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app]").forEach((button) => button.addEventListener("click", () => {
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === button.dataset.onboardingApp);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    if (!app || !service || app.launchState !== "available") {
      showToast("This app is unavailable right now");
      return;
    }
    onboardingServiceSetup = true;
    activeService = service;
    activeHomeAppId = app.id;
    route = "service";
    serviceGuideStep = 0;
    persistServiceGuideState();
    render();
  }));
  document.querySelector("#skip-onboarding")?.addEventListener("click", () => { onboardingServiceSetup = false; void completeOnboarding(); });
  document.querySelector("#finish-onboarding")?.addEventListener("click", () => {
    if (onboardingRoute !== "sending") return;
    setup.sendMode = "manual";
    setup.placementMode = "atomic";
    setup.acceptedRisk = false;
    setup.acceptedRiskForMode = null;
    onboardingRoute = "scrub";
    render();
  });
  document.querySelector("#skip-scrub-onboarding")?.addEventListener("click", () => void completeOnboarding());
  document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding());
  document.querySelector<HTMLInputElement>("#privacy-export-input")?.addEventListener("change", (event) => void scanPrivacyExport(event.currentTarget as HTMLInputElement));
  bindScrubControls();
}

function bindPasswordVisibility(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-password-toggle]").forEach((button) => button.addEventListener("click", () => {
    const input = document.getElementById(button.dataset.passwordToggle ?? "");
    if (!(input instanceof HTMLInputElement) || (input.type !== "password" && input.type !== "text")) return;
    const show = input.type === "password";
    input.type = show ? "text" : "password";
    button.innerHTML = passwordEyeIcon(show);
    button.setAttribute("aria-label", `${show ? "Hide" : "Show"} password`);
    button.setAttribute("aria-pressed", String(show));
  }));
}

async function completeOnboarding(): Promise<void> {
  try {
    const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, showPlaintextPreview: true });
    setup = saved.setup;
    onboardingComplete = true;
    // A newly-created identity is already unlocked. Load its signed invite and
    // local People state before Home renders so friend setup never incorrectly
    // tells the user to unlock again.
    await refreshIdentityScopedState();
    nativeApps = await loadNativeApps().catch(() => nativeApps);
    route = "home";
    clearPrivacyScanState();
    render();
  } catch {
    showToast("Could not save local setup · nothing changed");
  }
}

function bindPasswordForm(): void {
  const form = document.querySelector<HTMLFormElement>("#identity-password-form");
  const password = document.querySelector<HTMLInputElement>("#identity-password");
  const confirm = document.querySelector<HTMLInputElement>("#identity-password-confirm");
  const submit = document.querySelector<HTMLButtonElement>("#identity-password-submit");
  const error = document.querySelector<HTMLElement>("#password-error");
  if (!form || !password || !submit || !error) return;
  const validate = (): void => {
    const valid = form.dataset.passwordMode === "setup"
      ? isValidNewMainPassword(password.value)
      : isValidMainPassword(password.value);
    submit.disabled = !valid || Boolean(confirm && confirm.value !== password.value);
    error.textContent = "";
  };
  password.addEventListener("input", validate);
  confirm?.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    let secret = password.value;
    password.value = "";
    if (confirm) confirm.value = "";
    submit.disabled = true;
    try {
      if (form.dataset.passwordMode === "setup") {
        const identity = core.readiness.identityLoaded ? null : await createHubOslIdentity();
        const passwordResult = await setupHubMainPassword(secret);
        core = await loadCoreIntegration();
        // The locked bootstrap intentionally cannot read the encrypted
        // service registry. Refresh it immediately after the first password
        // installs the storage key, before the setup app chooser is shown.
        services = await loadLinkedServices().catch(() => services);
        passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
        recoveryBundle = {
          userId: identity?.userId ?? core.readiness.activeOslUserId ?? "Local OSL identity",
          identityPhrase: identity?.identityRecoveryPhrase ?? null,
          passwordPhrase: passwordResult.passwordRecoveryPhrase,
        };
        onboardingRoute = "recovery";
      } else {
        await unlockHubMainPassword(secret);
        core = await loadCoreIntegration();
        services = await loadLinkedServices().catch(() => services);
        passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
        if (onboardingComplete) {
          route = "home";
          void refreshUpdateStatus();
          void refreshIdentitySlots();
          void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") render(); });
          void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") render(); });
        }
        else onboardingRoute = "tutorial";
      }
      secret = "";
      render();
    } catch (failure) {
      secret = "";
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check OSL account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        error.textContent = "OSL could not verify the account state. Try again.";
        submit.disabled = false;
        password.focus();
        return;
      }
      core = refreshedCore;
      const readiness = core.readiness;
      if (readiness.bootstrapStatus === "ready" && readiness.unlocked) {
        services = await loadLinkedServices().catch(() => services);
        passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
        if (form.dataset.passwordMode === "setup" || !onboardingComplete) {
          onboardingRoute = "tutorial";
          route = "onboarding";
          showToast("Password is configured. Continue setup.");
        } else {
          route = "home";
        }
        render();
        return;
      }
      if (form.dataset.passwordMode === "setup" && readiness.bootstrapStatus === "passwordRequired") {
        onboardingRoute = "unlock";
        showToast("Password is configured. Unlock to continue.");
        render();
        return;
      }
      if (form.dataset.passwordMode === "setup" && readiness.bootstrapStatus === "setupRequired" && readiness.identityLoaded) {
        error.textContent = "Account created. Create its password to continue.";
      } else {
        error.textContent = localActionError(failure, "The OSL account action failed. Try again.");
      }
      submit.disabled = false;
      password.focus();
    }
  });
}

function bindImportForm(): void {
  const form = document.querySelector<HTMLFormElement>("#identity-import-form");
  const phrase = document.querySelector<HTMLTextAreaElement>("#identity-recovery-phrase");
  const password = document.querySelector<HTMLInputElement>("#import-password");
  const confirm = document.querySelector<HTMLInputElement>("#import-password-confirm");
  const submit = document.querySelector<HTMLButtonElement>("#identity-import-submit");
  const error = document.querySelector<HTMLElement>("#import-error");
  if (!form || !phrase || !password || !confirm || !submit || !error) return;
  const validate = (): void => {
    submit.disabled = !isRecoveryPhrase(phrase.value) || !isValidNewMainPassword(password.value) || password.value !== confirm.value;
    error.textContent = "";
  };
  phrase.addEventListener("input", validate);
  password.addEventListener("input", validate);
  confirm.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    let phraseSecret = phrase.value;
    let passwordSecret = password.value;
    phrase.value = "";
    password.value = "";
    confirm.value = "";
    submit.disabled = true;
    try {
      const identity = await importHubOslIdentityPhrase(phraseSecret);
      phraseSecret = "";
      const passwordResult = await setupHubMainPassword(passwordSecret);
      passwordSecret = "";
      core = await loadCoreIntegration();
      services = await loadLinkedServices().catch(() => services);
      recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase };
      onboardingRoute = "recovery";
      render();
    } catch (failure) {
      phraseSecret = "";
      passwordSecret = "";
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check recovered account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        error.textContent = "OSL could not verify the recovered account. Try again.";
        submit.disabled = false;
        phrase.focus();
        return;
      }
      core = refreshedCore;
      if (core.readiness.bootstrapStatus === "ready" && core.readiness.unlocked) {
        onboardingRoute = "tutorial";
        showToast("Account recovered. Continue setup.");
        render();
        return;
      }
      if (core.readiness.bootstrapStatus === "passwordRequired") {
        onboardingRoute = "unlock";
        showToast("Account recovered. Unlock to continue.");
        render();
        return;
      }
      error.textContent = core.readiness.bootstrapStatus === "setupRequired" && core.readiness.identityLoaded
        ? "Account recovered. Create its password to continue."
        : localActionError(failure, "Recovery was rejected or secure storage is unavailable.");
      submit.disabled = false;
      phrase.focus();
    }
  });
}

function renderWorkspace(): void {
  const protectedSheet = activeEmbeddedHost ? localProtectedSheetMarkup(localProtectedSheet) : "";
  const markup = `<div class="hub-layout"><section class="hub-workspace">${trustedHeader()}${workspaceContent()}</section></div>${protectedSheet}${peopleDialogMarkup()}${friendsDialogMarkup()}${scrubReviewDialogMarkup()}${burnDialogMarkup()}${ownedConfirmationMarkup()}${updateDialogMarkup()}`;
  let surface = root.querySelector<HTMLElement>("#workspace-render-surface");
  if (!surface) {
    root.innerHTML = `<div class="app-frame">${desktopTitlebar()}<div id="workspace-render-surface"></div></div>`;
    surface = root.querySelector<HTMLElement>("#workspace-render-surface");
    lastWorkspaceMarkup = null;
    lastWorkspaceViewKey = "";
  }
  if (!surface || (lastWorkspaceMarkup === markup && surface.querySelector(".hub-workspace"))) return;
  const nextViewKey = workspaceViewKey();
  const focusSnapshot = nextViewKey === lastWorkspaceViewKey ? captureWorkspaceFocus(surface) : null;
  lastWorkspaceMarkup = markup;
  lastWorkspaceViewKey = nextViewKey;
  surface.innerHTML = markup;
  bindWorkspace();
  if (focusSnapshot) restoreWorkspaceFocus(focusSnapshot);
  if (friendsDialogOpen) requestAnimationFrame(() => {
    const dialog = document.querySelector<HTMLDialogElement>("#friends-dialog");
    if (dialog && !dialog.open) dialog.showModal();
  });
  openScrubReviewDialogAfterRender();
  requestAnimationFrame(() => {
    for (const selector of ["#burn-dialog", "#owned-confirmation-dialog"]) {
      const dialog = document.querySelector<HTMLDialogElement>(selector);
      if (dialog && !dialog.open) dialog.showModal();
    }
  });
}

function appLauncherStrip(): string {
  const configured = configuredTopStripApps(homeAppsFromServices(services), homeTileOrder)
    .filter((app) => !hiddenServices.has(app.serviceId ?? ""));
  return `<nav class="app-launcher-strip" aria-label="Your apps">${configured.map((app) => `<button class="app-launcher ${activeHomeAppId === app.id ? "active" : ""} ${appLaunchPendingId === app.id ? "pending" : ""}" data-home-app="${app.id}" aria-label="Open ${escapeHtml(app.displayName)}" title="${escapeHtml(app.displayName)}" ${appLaunchPendingId ? "disabled" : ""}>${homeAppLogo(app)}</button>`).join("")}</nav>`;
}

function simpleDeviceStatusMarkup(): string {
  const ready = isCoreProtectionReady(core.readiness);
  return `<div class="trust-state ${ready ? "ready" : "pending"}" role="status"><span class="dot"></span><strong>${ready ? "Ready" : "Needs attention"}</strong></div>`;
}

function trustedHeader(): string {
  // Service controls stay compact; deeper setup remains progressively disclosed.
  if (route === "home") return homeHeader();
  if (route === "service" && activeService && serviceGuideStep !== null) {
    return `<div class="trusted-stack home-trusted-stack"><header class="home-header guide-header"><button class="home-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo" src="${oslLogoUrl}" alt=""/><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="guide-header-service">${serviceLogo(activeService.id)}<span><strong>${escapeHtml(activeService.displayName)}</strong><small>${isCoreProtectionReady(core.readiness) ? "Ready" : "Needs attention"}</small></span></div>${settingsButtonMarkup()}</header></div>`;
  }
  const localProtection = route === "service" && activeEmbeddedHost
    ? `<button class="local-protected-toggle" id="local-protected-toggle" type="button" aria-expanded="${localProtectedSheet.open}">Protect locally</button>`
    : "";
  const serviceControls = route === "service" && activeService ? `<div class="service-context"><span class="service-context-logo">${serviceLogo(activeService.id)}</span><span><strong>${escapeHtml(activeHomeAppName())}</strong><small>${activeEmbeddedHost ? "Isolated OSL profile" : "Needs setup"}</small></span>${localProtection}</div>` : "";
  const onboardingContinue = route === "service" && onboardingServiceSetup && activeEmbeddedHost
    ? `<button class="button compact primary" id="onboarding-service-continue">Continue setup</button>`
    : "";
  return `<div class="trusted-stack"><header class="workspace-header"><div class="hub-command"><button class="command-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo" src="${oslLogoUrl}" alt=""/><span><strong>OSL Privacy</strong></span></button>${appLauncherStrip()}${simpleDeviceStatusMarkup()}</div>${serviceControls ? `<div class="context-command">${serviceControls}</div>` : ""}${onboardingContinue}${settingsButtonMarkup("workspace-settings")}</header>${updateBannerMarkup()}</div>`;
}

function homeHeader(): string {
  const ready = isCoreProtectionReady(core.readiness);
  return `<div class="trusted-stack home-trusted-stack"><header class="home-header"><button class="home-brand home-brand-home" data-route="home" aria-label="OSL Privacy home"><span class="home-brand-mark" aria-hidden="true"><img class="osl-logo" src="${oslVectorLogoUrl}" alt=""/></span><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="home-core-state ${ready ? "ready" : "pending"}" role="status"><span class="dot"></span>${ready ? "OSL unlocked" : "Unlock OSL"}</div>${settingsButtonMarkup()}</header>${updateBannerMarkup()}</div>`;
}

function settingsButtonMarkup(extraClass = ""): string {
  return `<button class="button compact home-settings ${extraClass}" data-route="settings" aria-label="Open Settings"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9.6 3.4 10.2 2h3.6l.6 1.4 1.4.8 1.5-.2 1.8 3.1-.9 1.2v1.6l.9 1.2-1.8 3.1-1.5-.2-1.4.8-.6 1.4h-3.6l-.6-1.4-1.4-.8-1.5.2-1.8-3.1.9-1.2V8.3l-.9-1.2L6.7 4l1.5.2 1.4-.8Z"/><circle cx="12" cy="9.1" r="2.6"/></svg><span>Settings</span></button>`;
}

function workspaceContent(): string {
  if (route === "settings") return settingsContent();
  if (route === "service" && activeService) return serviceContent();
  const homeApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const configuredAppCount = homeApps.filter((app) => app.linked).length;
  const modules = [
    { id: "osl-chats", name: "OSL Chats", state: "Coming later", available: false },
    { id: "osl-groups", name: "OSL Groups", state: "Coming later", available: false },
    { id: "notifications", name: "Notifications", state: configuredAppCount >= 2 ? "Local activity" : "Connect 2 apps", available: configuredAppCount >= 2 },
    { id: "osl-notes", name: "OSL Notes", state: "Coming later", available: false },
  ] as const;
  const byId = new Map(homeApps.map((app) => [app.id, app]));
  const moduleById = new Map(modules.map((module) => [module.id, module]));
  const defaultIds = [...homeApps.map((app) => app.id), ...modules.map((module) => module.id)];
  const orderedIds = [...homeTileOrder.filter((id) => defaultIds.includes(id as HomeAppId)), ...defaultIds.filter((id) => !homeTileOrder.includes(id))];
  const renderHomeTile = (id: string, index: number): string => {
    const hidden = hiddenHomeTiles.has(id);
    if (hidden && !homeEditMode) return "";
    const controls = homeEditMode ? `<span class="tile-edit-controls"><button class="tile-remove" type="button" data-tile-toggle="${escapeHtml(id)}" aria-label="${hidden ? "Show" : "Remove"} ${escapeHtml(id)}">${hidden ? "+" : "−"}</button><span class="tile-keyboard-controls"><button type="button" data-tile-move="${escapeHtml(id)}:-1" ${index === 0 ? "disabled" : ""} aria-label="Move before">←</button><button type="button" data-tile-move="${escapeHtml(id)}:1" ${index === orderedIds.length - 1 ? "disabled" : ""} aria-label="Move after">→</button></span></span>` : "";
    const module = moduleById.get(id as typeof modules[number]["id"]);
    if (module) return `<article class="app-tile home-module ${module.available ? "" : "module-unavailable"} ${hidden ? "tile-hidden" : ""}" data-tile-id="${module.id}" draggable="${homeEditMode}" data-module-kind="${module.id}"><button type="button" data-home-module="${module.id}" ${module.available ? "" : "disabled"} aria-label="${escapeHtml(`${module.name}, ${module.state}`)}"><span class="app-logo-plate osl-module-logo" aria-hidden="true">${homeModuleIcon(module.id)}</span><span class="app-tile-copy"><strong>${module.name}</strong><small>${module.state}</small></span></button>${controls}</article>`;
    const app = byId.get(id as HomeAppId);
    if (!app) return "";
    const state = app.linked ? "OSL profile ready" : app.launchState === "available" ? "Set up" : "Coming later";
    const pending = appLaunchPendingId === app.id;
    return `<article class="app-tile ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${homeEditMode}" data-service-kind="${app.serviceId ?? "none"}"><button type="button" data-home-app="${app.id}" aria-label="${escapeHtml(`${app.displayName}, ${pending ? "Opening" : state}`)}" ${appLaunchPendingId ? "disabled" : ""}><span class="app-logo-plate">${homeAppLogo(app)}</span><span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : ""}</span></button>${controls}</article>`;
  };
  const socialIds = new Set(homeApps.filter((app) => app.provider === null).map((app) => app.id));
  const emailIds = new Set(homeApps.filter((app) => app.provider !== null).map((app) => app.id));
  const socialTiles = orderedIds.filter((id) => socialIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const emailTiles = orderedIds.filter((id) => emailIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const oslTiles = orderedIds.filter((id) => moduleById.has(id as typeof modules[number]["id"])).map(renderHomeTile).join("");
  const friendCount = hubPeople.length;
  const friendId = friendDisplayId ? compactFriendId(friendDisplayId) : null;
  const activity = notificationsEnabled
    ? `<button class="friends-activity" data-notification-settings><span class="dot"></span><span><strong>Activity</strong><small>${appNotifications?.length ? `${appNotifications.length} local OSL ${appNotifications.length === 1 ? "event" : "events"}` : "Nothing new"}</small></span></button>`
    : "";
  return `<main class="content-viewport home-dashboard ${homeEditMode ? "editing" : ""}">
    <section class="home-primary">
      <section class="home-apps" aria-labelledby="route-heading"><header><h1 id="route-heading" class="sr-only" tabindex="-1">Apps</h1><button class="button compact" id="edit-home">${homeEditMode ? "Done" : "Edit"}</button></header><div class="home-app-groups"><section class="home-app-section"><h2>Social</h2><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section><section class="home-app-section"><h2>Email</h2><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section><section class="home-app-section"><h2>OSL</h2><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section></div></section>
    </section>
    <aside class="friends-rail" aria-labelledby="friends-heading"><header><h2 id="friends-heading">Friends <span>${friendCount}</span></h2><button class="friends-add" data-open-friends aria-label="Add an OSL friend">+</button></header><div class="friends-rail-list">${peopleListMarkup("home", 8)}</div>${activity}<footer>${friendId ? `<span>Your friend ID</span><code>${escapeHtml(friendId)}</code><button class="text-button" data-open-friends>Share invite</button>` : `<p>Unlock OSL to create your invite.</p>`}</footer></aside>
  </main>`;
}

function homeModuleIcon(id: "osl-chats" | "osl-groups" | "notifications" | "osl-notes"): string {
  if (id === "osl-chats") return `<svg viewBox="0 0 24 24"><path d="M4 5.5h16v10H9l-5 4v-14Z"/><path d="M8 9h8M8 12h5"/></svg>`;
  if (id === "osl-groups") return `<svg viewBox="0 0 24 24"><circle cx="9" cy="9" r="3"/><circle cx="17" cy="10" r="2.3"/><path d="M3.5 19c.5-3 2.3-4.5 5.5-4.5s5 1.5 5.5 4.5M14.5 15c2.9-.4 4.9.8 6 3.5"/></svg>`;
  if (id === "notifications") return `<svg viewBox="0 0 24 24"><path d="M6 16.5h12l-1.5-2V10a4.5 4.5 0 0 0-9 0v4.5l-1.5 2Z"/><path d="M10 19h4"/></svg>`;
  return `<svg viewBox="0 0 24 24"><path d="M6 3.5h9l3 3V20H6V3.5Z"/><path d="M14.5 3.5V7H18M9 11h6M9 14h6M9 17h4"/></svg>`;
}

function activeHomeAppName(): string {
  return homeAppsFromServices(services).find((app) => app.id === activeHomeAppId)?.displayName
    ?? activeService?.displayName
    ?? "App";
}

function homeAppLogo(app: HomeAppCatalogEntry): string {
  return app.provider ? providerLogo(app.provider) : app.serviceId ? serviceLogo(app.serviceId) : "";
}

type PeopleListMode = "home" | "manage" | "service";

function friendScopeLabel(scope: HubPersonWhitelistScope): string {
  const kind = scope.kind === "dm" ? "Direct messages" : scope.kind === "group" ? "Group" : scope.kind === "channel" ? "Channel" : "Space";
  return scope.contextId ? `${kind} · ${compactFriendId(scope.contextId)}` : kind;
}

function peopleListMarkup(mode: PeopleListMode, limit?: number, offset = 0): string {
  if (!hubPeople.length) return `<div class="empty-state"><strong>No OSL friends yet</strong><p>Add an invite, then compare its verification code another way.</p></div>`;
  const end = limit === undefined ? undefined : offset + limit;
  return hubPeople.slice(offset, end).map((person) => {
    const nickname = person.alias ?? "Unnamed friend";
    const identity = compactFriendId(person.oslUserId);
    const action = person.safetyNumberVerified
      ? mode === "service"
        ? activeContextToken
          ? `<button class="button compact" data-allow-person="${escapeHtml(person.personId)}">Approve for this chat</button>`
          : `<span class="status-tag">Open a supported chat first</span>`
        : `<span class="status-tag">Verified</span>`
      : `<button class="button compact" data-verify-person="${escapeHtml(person.personId)}" data-safety-number="${escapeHtml(person.safetyNumber)}">Review request</button>`;
    if (mode === "home") return `<article class="person-row"><div><strong>${escapeHtml(nickname)}</strong><small>${person.pendingKeyChange ? "Security change needs review" : person.safetyNumberVerified ? "Verified" : "Request pending"}</small></div>${action}</article>`;
    const visibleScopes = person.whitelistedScopes.slice(0, friendScopeRenderLimit);
    const scopes = visibleScopes.length
      ? visibleScopes.map((scope) => `<span class="friend-scope">${escapeHtml(friendScopeLabel(scope))}</span>`).join("")
      : `<span class="friend-none">No chats approved</span>`;
    const hiddenScopeCount = Math.max(0, person.whitelistCount - visibleScopes.length);
    const truncated = hiddenScopeCount > 0 || person.whitelistedScopesTruncated
      ? `<small>${hiddenScopeCount > 0 ? `${hiddenScopeCount} more approved ${hiddenScopeCount === 1 ? "chat" : "chats"}` : "More approved chats"} stored locally.</small>`
      : "";
    const nicknameForm = mode === "manage" ? `<form class="friend-nickname-form" data-nickname-person="${escapeHtml(person.personId)}"><label><span>Nickname on this device</span><input name="nickname" maxlength="48" value="${escapeHtml(person.alias ?? "")}" placeholder="Add a nickname" autocomplete="off" spellcheck="false"/></label><button class="button compact" type="submit">Save</button></form>` : "";
    return `<article class="person-row person-profile"><header><div><strong>${escapeHtml(nickname)}</strong>${person.pendingKeyChange ? `<small>Security change needs review</small>` : ""}</div>${action}</header>${nicknameForm}<div class="friend-approvals"><span>Approved chats</span><div>${scopes}</div>${truncated}</div><details class="friend-security"><summary>Security details</summary><div><span>OSL ID</span><code>${escapeHtml(identity)}</code><span>Verification code</span><code>${escapeHtml(person.safetyNumber)}</code></div></details></article>`;
  }).join("");
}

function peopleDialogMarkup(): string {
  if (route !== "service") return "";
  const intro = activeContextToken
    ? "Verify each friend another way before approving this chat."
    : "Open a supported chat first. Encryption and chat approval are still off.";
  return `<dialog class="unlock-dialog" id="people-dialog"><div class="unlock-card"><h2>Friends in this chat</h2><p>${intro}</p><div class="people-list">${peopleListMarkup("service")}</div><button class="button" id="people-dialog-close">Close</button></div></dialog>`;
}

function friendsDialogMarkup(): string {
  if (route !== "home" || !friendsDialogOpen) return "";
  const pageCount = Math.max(1, Math.ceil(hubPeople.length / friendsDialogPageSize));
  friendsDialogPage = Math.min(friendsDialogPage, pageCount - 1);
  const pageStart = friendsDialogPage * friendsDialogPageSize;
  const pagination = pageCount > 1
    ? `<nav class="friends-pagination" aria-label="Friends pages"><button class="button compact" data-friends-page="${friendsDialogPage - 1}" ${friendsDialogPage === 0 ? "disabled" : ""}>Previous</button><span>${friendsDialogPage + 1} / ${pageCount}</span><button class="button compact" data-friends-page="${friendsDialogPage + 1}" ${friendsDialogPage + 1 >= pageCount ? "disabled" : ""}>Next</button></nav>`
    : "";
  const inviteCard = friendCode && friendDisplayId
    ? `<section class="friend-invite" aria-labelledby="friend-id-label"><div><span id="friend-id-label">Your friend ID</span><code>${escapeHtml(compactFriendId(friendDisplayId))}</code></div><button class="button" id="copy-friend-code" type="button">Copy invite</button><p>Send the invite to someone you trust so they can add you.</p></section>`
    : `<div class="empty-inline friend-code-unavailable">Your invite appears after OSL is unlocked.</div>`;
  return `<dialog class="friends-dialog" id="friends-dialog" aria-labelledby="friends-dialog-title"><div class="friends-dialog-card"><header><div><p class="eyebrow">Your circle</p><h2 id="friends-dialog-title">Friends</h2></div><button class="icon-button" id="friends-dialog-close" aria-label="Close friends">×</button></header><form id="add-friend-form" class="friend-add-form"><label class="friend-add-step" for="friend-code-input"><span>1</span><strong>Paste their invite</strong><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label class="friend-add-step" for="friend-nickname-input"><span>2</span><strong>Name them on this device</strong><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add friend</button></form><p class="form-status" id="friend-form-status" role="status"></p><p class="scope-approval-note">Encrypted chats stay off after adding someone. Compare the verification code another way, then approve each chat separately.</p><div class="people-list home-people-list">${peopleListMarkup("manage", friendsDialogPageSize, pageStart)}</div>${pagination}${inviteCard}</div></dialog>`;
}

function activeServiceBurnTarget(): { serviceId: string; accountId: string } | null {
  if (!activeService) return null;
  const provider = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId)?.provider ?? null;
  const matching = activeService.accounts.filter((account) => provider === null || account.provider === provider);
  return matching.length === 1 ? { serviceId: activeService.id, accountId: matching[0].id } : null;
}

function burnScopeReason(scope: BurnScope): string | null {
  if (scope === "chat" && !activeContextToken) return "Open a supported chat first.";
  if (scope === "app") {
    if (!activeService) return "Open an app first.";
    if (!activeServiceBurnTarget()) return "Choose one connected account first.";
    if (serviceBurnReadinessBusy) return "Checking complete local coverage…";
    if (!serviceBurnReadiness?.coverageComplete) return "OSL cannot prove complete coverage for this account yet.";
  }
  if (scope === "account" && !core.readiness.identityLoaded) return "Unlock an OSL account first.";
  return null;
}

function burnConfirmationPhrase(scope: BurnScope): string {
  return scope === "chat" ? "BURN CHAT" : scope === "app" ? "BURN APP" : "BURN ACCOUNT";
}

function closeBurnDialog(): void {
  const accountWasRemoved = burnScope === "account" && burnResult !== null && !core.readiness.identityLoaded;
  burnDialogOpen = false;
  burnScope = "chat";
  burnBusy = false;
  burnResult = null;
  serviceBurnReadiness = null;
  serviceBurnReadinessBusy = false;
  if (accountWasRemoved) {
    route = "onboarding";
    onboardingRoute = "welcome";
  }
  render();
}

function burnDialogMarkup(): string {
  if (!burnDialogOpen) return "";
  if (burnResult) {
    return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card burn-result"><header><div><p class="eyebrow">Burn</p><h2 id="burn-dialog-title">${burnResult.tone === "success" ? "Finished" : burnResult.tone === "warning" ? "Needs attention" : "Nothing was claimed"}</h2></div><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><p class="burn-result-message ${burnResult.tone}" role="status">${escapeHtml(burnResult.message)}</p>${burnResult.showUninstall ? `<div class="burn-uninstall"><strong>Uninstall is separate</strong><p>Your local OSL cleanup finished. Windows controls removal of the app itself.</p><a class="button" href="ms-settings:appsfeatures">Open Windows installed apps</a></div>` : ""}<footer><button class="button primary" data-close-burn>Done</button></footer></section></dialog>`;
  }

  const cards: Array<{ scope: BurnScope; title: string; detail: string }> = [
    { scope: "chat", title: "This chat", detail: "Forget this exact OSL conversation on this device." },
    { scope: "app", title: "This app", detail: "Forget every OSL conversation for the current app." },
    { scope: "account", title: "Entire OSL account", detail: "Remove every OSL identity and local setting on this computer." },
  ];
  const selectedReason = burnScopeReason(burnScope);
  const phrase = burnConfirmationPhrase(burnScope);
  const scopeCards = cards.map((card) => {
    const reason = burnScopeReason(card.scope);
    return `<button class="burn-scope-card ${burnScope === card.scope ? "selected" : ""}" type="button" data-burn-scope="${card.scope}" ${reason ? "disabled" : ""} aria-pressed="${burnScope === card.scope}"><strong>${card.title}</strong><small>${card.detail}</small>${reason ? `<span>${escapeHtml(reason)}</span>` : ""}</button>`;
  }).join("");
  const effects = burnScope === "chat"
    ? "OSL destroys local decrypt material and caches for this exact chat."
    : burnScope === "account"
      ? "OSL removes every local identity, decrypt key, cache, and preference on this computer."
      : serviceBurnReadiness?.coverageComplete
        ? `OSL destroys local decrypt material and caches for all ${serviceBurnReadiness.indexedScopes} indexed OSL ${serviceBurnReadiness.indexedScopes === 1 ? "chat" : "chats"} in this connected account. Its login profile and cookies stay.`
        : "OSL must prove complete local coverage before app-wide burn is available.";
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card"><header><div><p class="eyebrow">Local privacy</p><h2 id="burn-dialog-title">Burn</h2></div><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><div class="burn-scope-grid" aria-label="Burn scope">${scopeCards}</div><section class="burn-truth"><strong>Exactly what happens</strong><ul><li>${effects}</li><li>Messages and history in the service remain.</li><li>Screenshots, exports, backups, and copies held by other people cannot be retracted.</li></ul></section><div class="burn-options"><label class="setting-line unavailable"><span><strong>Also forget messages they sent me</strong><small>Incoming OSL messages are already included in local chat and account burns.</small></span><input type="checkbox" checked disabled/></label><label class="setting-line unavailable"><span><strong>Burn for friends · Pro</strong><small>${pro ? "Requires every recipient’s prior signed consent and an acknowledgment from each device." : "A Pro initiator may request this for Free recipients only after each recipient gives signed consent."} The consent-and-acknowledgment workflow is unavailable in this build.</small></span><input type="checkbox" disabled/></label>${burnScope === "account" ? `<label class="setting-line interactive"><span><strong>Uninstall after burn</strong><small>After a successful local burn, open Windows installed apps.</small></span><input id="burn-uninstall" type="checkbox"/></label>` : ""}</div><form id="burn-confirm-form" class="burn-confirm"><label for="burn-confirm-input">Type <code>${phrase}</code> to continue</label><input id="burn-confirm-input" autocomplete="off" autocapitalize="characters" spellcheck="false" ${selectedReason ? "disabled" : ""}/><p class="form-status" id="burn-form-status" role="status">${selectedReason ? escapeHtml(selectedReason) : "This cannot be undone."}</p><footer><button class="button" type="button" data-close-burn>Cancel</button><button class="button danger" id="burn-confirm-submit" type="submit" disabled>${burnBusy ? "Burning…" : "Burn now"}</button></footer></form></section></dialog>`;
}

function ownedConfirmationMarkup(): string {
  if (!ownedConfirmation) return "";
  const request = ownedConfirmation;
  const verifying = request.kind === "verifyFriend";
  const title = verifying ? "Accept friend request?" : "Clear Pro activation?";
  const detail = request.kind === "verifyFriend"
    ? `<p>Compare this verification code with your friend another way first.</p><code class="verification-code">${escapeHtml(request.verificationCode)}</code><p>Accepting stores the request on this device. It does not turn on decryption in any chat.</p>`
    : `<p>Pro features will be unavailable on this device until you activate again.</p>`;
  return `<dialog class="owned-confirmation-dialog" id="owned-confirmation-dialog" aria-labelledby="owned-confirmation-title"><section class="owned-confirmation-card"><header><h2 id="owned-confirmation-title">${title}</h2><button class="icon-button" data-close-owned-confirmation aria-label="Cancel">×</button></header>${detail}<p class="form-status" role="status">${escapeHtml(ownedConfirmationError)}</p><footer><button class="button" data-close-owned-confirmation>Cancel</button><button class="button ${verifying ? "primary" : "danger"}" id="owned-confirmation-submit" ${ownedConfirmationBusy ? "disabled" : ""}>${ownedConfirmationBusy ? "Working…" : verifying ? "Accept locally" : "Clear activation"}</button></footer></section></dialog>`;
}

function serviceContent(): string {
  const name = escapeHtml(activeHomeAppName());
  if (activeService && serviceGuideStep !== null) return serviceGuideContent(activeService, serviceGuideStep);
  if (activeNativeHostId) return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="${name} is open in an OSL-specific native window"><span class="sr-only">${name} native client is open inside OSL.</span></main>`;
  if (activeEmbeddedHost) return `<main class="content-viewport host-viewport host-open" id="route-heading" tabindex="-1" aria-label="${name} is open inside OSL"><div class="loading-host" aria-hidden="true"><span class="host-skeleton logo"></span><span class="host-skeleton title"></span></div></main>`;
  if (serviceAccountPickerOpen) return serviceAccountPickerContent();
  return `<main class="content-viewport native-app-page" id="route-heading" tabindex="-1"><section class="native-app-card"><span class="service-icon large">${activeService ? serviceLogo(activeService.id) : ""}</span><h1>${name}</h1><p>Open a separate OSL profile. Your normal app stays open.</p><button class="button primary native-app-action" id="embedded-service-setup" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : `Open ${name}`}</button><div class="native-app-secondary"><button class="text-back" id="native-app-back">← Apps</button><button class="text-button" id="burn-button" data-open-burn="app">Burn…</button></div></section></main>`;
}

function activeNativeApp(): NativeApp | null {
  if (!activeHomeAppId) return null;
  return nativeApps.find((app) => app.id === activeHomeAppId) ?? null;
}

function serviceAccountPickerContent(): string {
  const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId);
  const accounts = app ? embeddedAccountsForHomeApp(app, services) : [];
  const name = escapeHtml(activeHomeAppName());
  const choices = accounts.map((account) => `<button class="service-account-choice" data-service-account="${escapeHtml(account.id)}"><span>${serviceLogo(activeService?.id ?? "discord")}</span><strong>${escapeHtml(account.label)}</strong><small>OSL profile</small></button>`).join("");
  return `<main class="content-viewport native-app-page" id="route-heading" tabindex="-1"><section class="native-app-card service-account-picker"><button class="text-back" id="native-app-back">← Apps</button><h1>Choose ${name} profile</h1><div class="service-account-choices">${choices}</div><button class="button" id="add-service-profile">Add another profile</button></section></main>`;
}

function serviceGuideContent(service: LinkedService, step: ServiceGuideStep): string {
  const name = escapeHtml(activeHomeAppName());
  void step;
  const nativeApp = activeNativeApp();
  const installedAction = nativeApp?.availability === "installable"
      ? `<button class="button" data-background-install="${nativeApp.id}" ${backgroundInstallIds.has(nativeApp.id) ? "disabled" : ""}>${backgroundInstallIds.has(nativeApp.id) ? "Installing…" : "Background install"}</button>`
      : "";
  const nativeInstalled = nativeApp?.availability === "installed";
  const details = `<details class="guide-details"><summary>Sign-in privacy</summary><p>${nativeInstalled ? "OSL opens a separate local app profile. Your normal app and account stay untouched." : "OSL keeps this service in its own local browser profile. Sign in once here; later opens reuse that profile."}</p></details>`;
  const selectedApp = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId);
  const openAction = selectedApp?.launchState === "available"
    ? `<button class="button primary" id="embedded-service-setup" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : nativeInstalled ? "Open app in OSL" : "Open in OSL"}</button>`
    : `<button class="button" disabled>Coming later</button>`;
  const nativeNote = nativeInstalled ? `<p class="guide-native-note">A separate ${name} window opens inside OSL. The normal window stays open.</p>` : "";
  return `<main class="content-viewport service-guide" id="route-heading" tabindex="-1"><section class="guide-card guide-card-simple"><header><button class="text-back" id="service-guide-exit">← Apps</button></header><div class="guide-hero"><span class="guide-logo" data-guide-service="${service.id}">${serviceLogo(service.id)}</span><h1>Connect ${name}</h1></div><footer class="guide-actions">${openAction}${installedAction}</footer>${nativeNote}${details}</section>${onboardingServiceSetup ? '<button class="onboarding-skip-dock" id="service-guide-skip">Skip · manual setup</button>' : ""}</main>`;
}

function settingsContent(): string {
  const items: Array<[SettingsSection, string]> = [["account", "Account"], ["apps", "Apps"], ["scrub", "Scrub"], ["cleanup", "Mass cleanup · Pro"], ["notifications", "Notifications"], ["appearance", "Appearance"], ["about", "About"]];
  return `<main class="content-viewport settings-page"><div class="settings-sidebar"><h1 id="route-heading" tabindex="-1">Settings</h1>${items.map(([id, label]) => `<button data-settings="${id}" class="${settingsSection === id ? "active" : ""}">${label}</button>`).join("")}</div><section class="settings-detail">${settingsSectionContent()}</section></main>`;
}

function settingsSectionContent(): string {
  if (settingsSection === "account") return `${identitySettingsContent()}${settingsDivider()}${passwordSecuritySettingsContent()}${settingsDivider()}${accountAdvancedSettingsContent()}`;
  if (settingsSection === "apps") return `${serviceAccountsSettingsContent()}${settingsDivider()}${sendingSettingsContent()}`;
  if (settingsSection === "scrub") return privacySettingsContent();
  if (settingsSection === "cleanup") return massCleanupSettingsContent();
  if (settingsSection === "notifications") return notificationSettingsContent();
  if (settingsSection === "appearance") return appearanceSettingsContent();
  return updateSettingsContent();
}

function massCleanupActionLabel(action: string): string {
  const labels: Record<string, string> = {
    leaveAndRemoveChat: "Leave channels and groups",
    clearHistoryForSelf: "Clear selected histories",
    leaveServer: "Leave selected servers",
    closeConversation: "Close selected conversations",
    archiveConversation: "Archive selected threads",
    deleteConversationForSelf: "Delete selected conversations for you",
  };
  return labels[action] ?? "Cleanup";
}

function massCleanupSettingsContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  if (!pro) {
    return `<h2>Mass cleanup</h2><section class="cleanup-lock"><span>PRO</span><strong>Organize many chats at once</strong><p>Every batch is reviewed and confirmed before anything changes.</p></section>`;
  }
  if (massCleanupLoading) return `<h2>Mass cleanup</h2><div class="settings-unavailable"><strong>Checking this device…</strong></div>`;
  if (!massCleanupCapabilities) {
    return `<h2>Mass cleanup</h2><div class="settings-unavailable"><strong>Unavailable in this version</strong><span>No service was changed.</span></div>`;
  }
  const visible = massCleanupCapabilities.services.filter((service) => service.plannedActions.length > 0);
  const rows = visible.map((capability) => {
    const name = services.find((service) => service.id === capability.serviceId)?.displayName ?? capability.serviceId;
    return `<article class="cleanup-service unavailable"><div><strong>${escapeHtml(name)}</strong><small>${capability.plannedActions.map(massCleanupActionLabel).join(" · ")}</small></div><span>Not ready</span></article>`;
  }).join("");
  return `<h2>Mass cleanup</h2><p>Select, review, then confirm one small batch. OSL never runs deletion unattended.</p><div class="cleanup-service-list">${rows}</div><p class="quiet-note">Adapters remain disabled until each app can be read and verified locally without sending message data to OSL.</p>`;
}

async function refreshMassCleanupCapabilities(): Promise<void> {
  if (massCleanupLoading || massCleanupCapabilities) return;
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  if (!pro) return;
  massCleanupLoading = true;
  render();
  try {
    massCleanupCapabilities = await withNativeDeadline(loadMassCleanupCapabilities(), "Load Mass Cleanup", 2_000);
  } catch {
    massCleanupCapabilities = null;
  } finally {
    massCleanupLoading = false;
    if (route === "settings" && settingsSection === "cleanup") render();
  }
}

function settingsDivider(): string {
  return `<hr class="settings-divider"/>`;
}

function passwordSecuritySettingsContent(): string {
  const passwordAction = core.readiness.bootstrapStatus === "setupRequired"
    ? `<button class="button primary" data-onboarding-action="create">Create password</button>`
    : core.readiness.bootstrapStatus === "passwordRequired"
      ? `<button class="button primary" data-onboarding-action="unlock">Unlock OSL</button>`
      : `<span class="setting-status"><span class="dot"></span>Password configured and unlocked</span>`;
  const roleForm = (role: "stealth" | "burn", configured: boolean, wired: boolean): string => {
    const title = role === "stealth" ? "Stealth password" : "Burn password";
    const consequence = role === "stealth" ? "decoy screen" : "account burn";
    if (!wired) {
      return `<section class="password-role unavailable" aria-disabled="true"><div><strong>${title}</strong><small>${configured ? "Stored but inactive" : "Unavailable"}</small></div><p>The ${consequence} login action is not available in this build. OSL will not let you create or rely on it.</p></section>`;
    }
    return `<details class="password-role"><summary><span><strong>${title}</strong><small>${configured ? "Configured" : "Not set"}</small></span><span>›</span></summary><form data-password-role="${role}" data-password-remove="${configured}"><label>Current password<div class="password-input-row"><input id="${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div></label>${configured ? "" : `<label>New ${role} password<div class="password-input-row"><input id="${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div></label>`}<button class="button ${configured ? "danger" : "primary"}" type="submit">${configured ? "Remove" : "Set password"}</button><p class="password-role-note">Active at login for ${consequence}.</p></form></details>`;
  };
  const roles = passwordRoleStatus
    ? `<div class="security-shortcuts">${roleForm("stealth", passwordRoleStatus.stealthPasswordSet, passwordRoleStatus.stealthActionWired)}${roleForm("burn", passwordRoleStatus.burnPasswordSet, passwordRoleStatus.burnActionWired)}</div>`
    : `<div class="settings-unavailable"><strong>Password roles unavailable</strong><span>Unlock OSL and reopen Settings.</span></div>`;
  return `<h2>Password & security</h2><p>Unlocks encrypted storage on this device.</p><div class="settings-actions">${passwordAction}</div>${roles}`;
}

function accountAdvancedSettingsContent(): string {
  return `<details class="account-advanced"><summary>Advanced</summary><div class="danger-zone"><h3>Burn</h3><p>Review exactly what OSL can remove before anything changes.</p><button class="button danger" id="full-cleanup-button" data-open-burn="account">Open Burn</button></div></details>`;
}

function serviceAccountsSettingsContent(): string {
  const rows = homeAppsFromServices(services).filter((app) => app.visibility === "launch").map((app) => {
    const state = app.linked ? `${app.accountCount} local ${app.accountCount === 1 ? "profile" : "profiles"}` : app.launchState === "available" ? "Not set up" : "Coming later";
    const action = app.launchState === "available"
      ? `<button class="button compact" data-home-app="${app.id}" ${appLaunchPendingId ? "disabled" : ""}>${appLaunchPendingId === app.id ? "Opening…" : app.linked ? "Open" : "Set up"}</button>`
      : `<button class="button compact" disabled>Coming later</button>`;
    return `<article><div>${homeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${state}</small></span></div>${action}</article>`;
  }).join("");
  const installedChoices = nativeApps.map((app) => {
    if (app.availability === "installed") return `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Ready for OSL</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${savedNativeApps.has(app.id) ? "checked" : ""}/></label>`;
    if (app.availability === "installable") return `<div class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${backgroundInstallIds.has(app.id) ? "Installing…" : "Optional Windows app"}</small></span></span><button class="button compact" type="button" data-background-install="${app.id}" ${backgroundInstallIds.has(app.id) ? "disabled" : ""}>${backgroundInstallIds.has(app.id) ? "Installing…" : "Background install"}</button></div>`;
    return `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Embedded web only</small></span></span></div>`;
  }).join("");
  const browserButtons = browserImports.filter((browser) => browser.installed).map((browser) => `<button class="button compact" type="button" data-browser-import="${browser.id}">Open ${escapeHtml(browser.displayName)} import</button>`).join("");
  const browserNotice = `<div class="saved-account-browser-note"><strong>Browser-owned sign-in</strong><small>Chrome, Edge, Firefox, Brave, Opera, and Vivaldi keep control of saved-password import. OSL never reads their password files.</small>${browserButtons ? `<div class="browser-import-actions">${browserButtons}</div>` : ""}</div>`;
  const savedAccountSettings = `<details class="saved-account-settings"><summary>Account opening</summary><div class="saved-account-choices"><button class="setting-option ${savedAccountMode === "use" ? "selected" : ""}" data-saved-account-mode="use"><strong>Use existing account</strong><small>Reuse OSL profiles</small></button><button class="setting-option ${savedAccountMode === "clean" ? "selected" : ""}" data-saved-account-mode="clean"><strong>Start fresh</strong><small>Create a new OSL profile</small></button></div><div class="saved-account-apps">${installedChoices}</div>${browserNotice}<p>Services open inside OSL. External desktop apps are never launched.</p></details>`;
  return `<h2>Apps</h2><p>Each account has its own local sign-in profile inside OSL.</p>${savedAccountSettings}<div class="account-settings-list">${rows}</div><div class="warning"><strong>Local sessions</strong><p>Service cookies stay in the matching OSL profile so you remain signed in. Your typed service password is not sent to OSL.</p></div>`;
}

async function scanPrivacyExport(input: HTMLInputElement): Promise<void> {
  const file = input.files?.[0];
  input.value = "";
  if (!file || privacyScanBusy) return;
  if (file.size > LOCAL_MESSAGE_IMPORT_MAX_BYTES) {
    showToast("Export is larger than the 8 MiB local scan limit");
    return;
  }
  privacyScanBusy = true;
  render();
  try {
    const candidates = importLocalMessageExport(await file.text(), {
      serviceId: "local_import",
      accountId: "manual-export",
      conversationId: "privacy-scan",
    });
    if (!candidates?.length) throw new Error("No supported messages were found");
    const result = await scanLocalPrivacy(candidates);
    if (!result) throw new Error("The trusted local scanner was unavailable");
    privacyScanResult = result;
    selectedScrubFindings.clear();
    scrubResultsPage = 0;
    scrubReviewOpen = false;
    scrubReviewPage = 0;
    scrubReviewConfirmed = false;
    privacyScanFileName = file.name.slice(0, 96);
  } catch (failure) {
    privacyScanResult = null;
    privacyScanFileName = null;
    showToast(localActionError(failure, "The export could not be scanned locally"));
  } finally {
    privacyScanBusy = false;
    render();
  }
}

function sendingSettingsContent(): string {
  return `<h2>Sending</h2><div class="setting-status"><span class="dot"></span>Copy & paste</div><p>Review encrypted text, then copy, paste, and send it yourself. Automatic sending is not available in this build.</p>`;
}

function privacySettingsContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const autoScrubPlan = proActive ? "PRO ACTIVE · COMING SOON" : "PRO · COMING SOON";
  return `<h2>Scrub</h2><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Scans, previews, selections, and review lists stay local. Pro does not change this.</p><div class="scrub-tier-grid"><section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h3>Review your history</h3><p>Choose a TXT, CSV, or JSON messages file. OSL shows suggestions; you choose what to review.</p></div>${scanActions}</section><section class="autoscrub-card" aria-disabled="true"><header><div><span class="privacy-local-mark">${autoScrubPlan}</span><h3>AutoScrub assistant</h3></div><span class="status-tag">Off by default</span></header><p>Coming soon. It schedules local scans and prepares a complete editable list. Nothing happens until you review and confirm every batch.</p><details><summary>Automation risks</summary><p>Future paced actions must stop on limits, challenges, changed content, or failed checks. Automation may break an app’s rules or restrict an account. Treat removal as unconfirmed until the app shows it is gone.</p></details><button class="button compact" type="button" disabled>Unavailable in this build</button></section></div>${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}<details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, providers, exports, and backups may retain copies.</p><p>This build only gives manual directions. It does not delete app messages. Check the original app and delete each message yourself.</p></div></details><details class="privacy-technical"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Remote app access</span><strong>Blocked</strong></div><label class="setting-line interactive"><span><strong>Windows capture resistance</strong><small>Asks Windows to exclude OSL from ordinary screen capture. Cameras, malware, and modified recipients can still capture content.</small></span><input id="screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label></details>`;
}

function clearPrivacyScanState(): void {
  privacyScanResult = null;
  privacyScanFileName = null;
  selectedScrubFindings.clear();
  scrubResultsPage = 0;
  scrubReviewOpen = false;
  scrubReviewPage = 0;
  scrubReviewConfirmed = false;
}

function privacyScanResultsMarkup(): string {
  if (!privacyScanResult) return "";
  const enabled = new Set(enabledScrubFindings(privacyScanResult.findings, enabledScrubSignals));
  const matching = privacyScanResult.findings.map((finding, index) => ({ finding, index })).filter(({ finding }) => enabled.has(finding));
  const pageCount = Math.max(1, Math.ceil(matching.length / scrubResultsPageSize));
  scrubResultsPage = Math.min(scrubResultsPage, pageCount - 1);
  const pageStart = scrubResultsPage * scrubResultsPageSize;
  const shown = matching.slice(pageStart, pageStart + scrubResultsPageSize);
  const items = shown.map(({ finding, index }) => scrubFindingMarkup(finding, index, "results")).join("");
  const selected = [...selectedScrubFindings].filter((index) => matching.some((item) => item.index === index)).length;
  const selectionControls = matching.length ? `<div class="scrub-selection-controls"><button class="text-button" id="select-all-scrub" type="button">Select all ${matching.length}</button><button class="text-button" id="clear-scrub-selection" type="button" ${selected ? "" : "disabled"}>Clear selection</button></div>` : "";
  const pagination = pageCount > 1 ? `<nav class="scrub-pagination" aria-label="Scrub result pages"><button class="button compact" data-scrub-page="${scrubResultsPage - 1}" ${scrubResultsPage === 0 ? "disabled" : ""}>Previous</button><span>${scrubResultsPage + 1} / ${pageCount}</span><button class="button compact" data-scrub-page="${scrubResultsPage + 1}" ${scrubResultsPage + 1 >= pageCount ? "disabled" : ""}>Next</button></nav>` : "";
  return `<section class="privacy-results" aria-live="polite"><header><div><strong>${matching.length} ${matching.length === 1 ? "suggestion" : "suggestions"}</strong><small>${privacyScanResult.messagesScanned} messages scanned${privacyScanFileName ? ` · ${escapeHtml(privacyScanFileName)}` : ""}</small></div><span class="privacy-local-mark">LOCAL · NOT SAVED</span></header>${selectionControls}${items || `<div class="empty-state"><strong>No suggestions in the categories you chose</strong><p>OSL can miss things. Review important chats yourself too.</p></div>`}${pagination}${items ? `<footer class="scrub-review-footer"><span>${selected} selected</span><button class="button" id="review-scrub-selection" type="button" ${selected ? "" : "disabled"}>Review selected</button></footer>` : ""}</section>`;
}

function scrubFindingLabel(category: LocalPrivacyScanResult["findings"][number]["category"]): string {
  const group = scrubSignalGroupFor(category);
  return scrubSignalDefinitions.find((definition) => definition.id === group)?.label ?? "Review suggestion";
}

function scrubFindingMarkup(finding: LocalPrivacyScanResult["findings"][number], index: number, surface: "results" | "review"): string {
  const selected = selectedScrubFindings.has(index);
  const inputAttribute = surface === "review" ? "data-scrub-review-finding" : "data-scrub-finding";
  const sentCopy = finding.canRequestDelete
    ? "The file says you sent this. Check the exact message in the app."
    : "OSL cannot tell who sent this from the file. Check the exact message in the app.";
  return `<article class="privacy-finding ${selected ? "selected" : ""}"><label class="scrub-finding-select"><input type="checkbox" ${inputAttribute}="${index}" ${selected ? "checked" : ""}/><strong>${escapeHtml(scrubFindingLabel(finding.category))}</strong></label><div class="scrub-finding-field"><span>Why OSL showed this</span><p>${escapeHtml(finding.reason)}</p></div><blockquote>${escapeHtml(finding.localPreview)}</blockquote><div class="scrub-finding-field"><span>Where to find it</span><p>${escapeHtml(finding.serviceId)} · ${escapeHtml(finding.conversationId)} · ${escapeHtml(finding.messageLocator)}</p></div><div class="scrub-finding-field"><span>Check that you sent this</span><p>${sentCopy}</p></div></article>`;
}

function selectedScrubItems(): Array<{ finding: LocalPrivacyScanResult["findings"][number]; index: number }> {
  if (!privacyScanResult) return [];
  return [...selectedScrubFindings]
    .sort((left, right) => left - right)
    .flatMap((index) => privacyScanResult?.findings[index] ? [{ finding: privacyScanResult.findings[index], index }] : []);
}

function scrubReviewDialogMarkup(): string {
  if (!scrubReviewOpen) return "";
  const selected = selectedScrubItems();
  const pageCount = Math.max(1, Math.ceil(selected.length / scrubReviewPageSize));
  scrubReviewPage = Math.min(scrubReviewPage, pageCount - 1);
  const pageStart = scrubReviewPage * scrubReviewPageSize;
  const items = selected.slice(pageStart, pageStart + scrubReviewPageSize).map(({ finding, index }) => scrubFindingMarkup(finding, index, "review")).join("");
  const pagination = pageCount > 1 ? `<nav class="scrub-pagination" aria-label="Review pages"><button class="button compact" data-scrub-review-page="${scrubReviewPage - 1}" ${scrubReviewPage === 0 ? "disabled" : ""}>Previous</button><span>${scrubReviewPage + 1} / ${pageCount}</span><button class="button compact" data-scrub-review-page="${scrubReviewPage + 1}" ${scrubReviewPage + 1 >= pageCount ? "disabled" : ""}>Next</button></nav>` : "";
  return `<dialog class="scrub-review-dialog" id="scrub-review-dialog" aria-labelledby="scrub-review-heading"><div class="scrub-review-card"><header><div><p class="eyebrow">Manual Scrub</p><h2 id="scrub-review-heading">Confirm your list</h2></div><button class="icon-button" id="close-scrub-review" type="button" aria-label="Close review">×</button></header><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Review every checked item before continuing.</p><div class="scrub-review-summary"><strong>${selected.length} selected</strong><span>Nothing is deleted by this build.</span></div><div class="scrub-review-items">${items || `<div class="empty-state"><strong>Nothing selected</strong><p>Close this window and choose the messages you want to review.</p></div>`}</div>${pagination}<footer><p>Confirming only prepares manual directions. It does not contact or change any app.</p><div><button class="button ghost" id="close-scrub-review-footer" type="button">Back</button><button class="button primary" id="confirm-scrub-list" type="button" ${selected.length ? "" : "disabled"}>Confirm this list</button></div></footer></div></dialog>`;
}

function openScrubReviewDialogAfterRender(): void {
  if (!scrubReviewOpen) return;
  requestAnimationFrame(() => {
    const dialog = document.querySelector<HTMLDialogElement>("#scrub-review-dialog");
    if (dialog && !dialog.open) dialog.showModal();
  });
}

function bindScrubControls(): void {
  document.querySelectorAll<HTMLInputElement>("[data-scrub-category]").forEach((input) => input.addEventListener("change", () => {
    const group = input.dataset.scrubCategory as ScrubSignalGroup;
    if (!defaultScrubSignalGroups.includes(group)) return;
    if (input.checked) enabledScrubSignals.add(group); else enabledScrubSignals.delete(group);
    localStorage.setItem(scrubSignalsStorageKey, JSON.stringify([...enabledScrubSignals]));
    selectedScrubFindings.clear();
    scrubResultsPage = 0;
    scrubReviewOpen = false;
    scrubReviewConfirmed = false;
    render();
  }));
  document.querySelectorAll<HTMLInputElement>("[data-scrub-finding]").forEach((input) => input.addEventListener("change", () => {
    const index = Number(input.dataset.scrubFinding);
    if (!Number.isSafeInteger(index) || index < 0 || !privacyScanResult?.findings[index]) return;
    if (input.checked) selectedScrubFindings.add(index); else selectedScrubFindings.delete(index);
    render();
  }));
  document.querySelector<HTMLButtonElement>("#review-scrub-selection")?.addEventListener("click", () => {
    if (!selectedScrubItems().length) return;
    scrubReviewOpen = true;
    scrubReviewPage = 0;
    scrubReviewConfirmed = false;
    render();
  });
  document.querySelector<HTMLButtonElement>("#select-all-scrub")?.addEventListener("click", () => {
    if (!privacyScanResult) return;
    privacyScanResult.findings.forEach((finding, index) => {
      if (enabledScrubSignals.has(scrubSignalGroupFor(finding.category))) selectedScrubFindings.add(index);
    });
    render();
  });
  document.querySelector<HTMLButtonElement>("#clear-scrub-selection")?.addEventListener("click", () => { selectedScrubFindings.clear(); render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-page]").forEach((button) => button.addEventListener("click", () => {
    const next = Number(button.dataset.scrubPage);
    if (!Number.isSafeInteger(next) || next < 0) return;
    scrubResultsPage = next;
    render();
  }));
  document.querySelectorAll<HTMLInputElement>("[data-scrub-review-finding]").forEach((input) => input.addEventListener("change", () => {
    const index = Number(input.dataset.scrubReviewFinding);
    if (!Number.isSafeInteger(index) || index < 0 || !privacyScanResult?.findings[index]) return;
    if (input.checked) selectedScrubFindings.add(index); else selectedScrubFindings.delete(index);
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-review-page]").forEach((button) => button.addEventListener("click", () => {
    const next = Number(button.dataset.scrubReviewPage);
    if (!Number.isSafeInteger(next) || next < 0) return;
    scrubReviewPage = next;
    render();
  }));
  const closeReview = (): void => { scrubReviewOpen = false; render(); };
  document.querySelector("#close-scrub-review")?.addEventListener("click", closeReview);
  document.querySelector("#close-scrub-review-footer")?.addEventListener("click", closeReview);
  document.querySelector("#confirm-scrub-list")?.addEventListener("click", () => {
    if (!selectedScrubItems().length) return;
    scrubReviewConfirmed = true;
    scrubReviewOpen = false;
    render();
  });
}

function notificationSettingsContent(): string {
  const apps = orderedServices().filter((service) => service.category === "consumer").map((service) => `<label class="notification-app-row">${serviceLogo(service.id)}<span><strong>${escapeHtml(service.displayName)}</strong><small>Unread access is not supported yet</small></span><input type="checkbox" data-notification-app="${service.id}" ${notificationAppPreferences[service.id] !== false ? "checked" : ""}/></label>`).join("");
  return `<h2>Notifications</h2><p>OSL can show activity created on this device. It cannot read app unread counts yet.</p><label class="setting-line interactive"><span><strong>Local OSL activity</strong><small>Security and app-connection events on this device.</small></span><input id="notifications-opt-in" type="checkbox" ${notificationsEnabled ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show preview content</strong><small>Off by default. When off, Activity hides event details.</small></span><input id="notification-previews" type="checkbox" ${notificationPreviewContent ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Suggest chat approval</strong><small>Suggestions ask you to approve a chat. They never enable decryption.</small></span><input id="notification-scope-suggestions" type="checkbox" ${notificationScopeSuggestions ? "checked" : ""}/></label><div class="settings-subhead"><h3>Apps</h3><p>Choose which apps may appear here when unread access is supported.</p></div><div class="notification-app-list">${apps}</div>`;
}

function identitySettingsContent(): string {
  const identities = hubIdentities.length
    ? hubIdentities.map((identity) => `<article class="identity-row"><div><strong>${escapeHtml(identity.label)}</strong><small>${escapeHtml(identity.oslUserId)} · ${escapeHtml(identity.safetyNumber)}</small></div>${identity.active ? `<span class="status-tag">Active</span>` : `<button class="button compact" data-switch-identity="${escapeHtml(identity.slotId)}">Switch</button>`}</article>`).join("")
    : `<div class="empty-state"><strong>Identity list unavailable</strong><p>Unlock OSL to manage encrypted identity slots.</p></div>`;
  const recovery = newIdentityRecoveryPhrase ? `<div class="warning recovery-secret"><strong>Save the new identity recovery phrase now</strong><code>${escapeHtml(newIdentityRecoveryPhrase)}</code><p>Visible only on this page. It clears if you leave or hide OSL.</p></div>` : "";
  return `<h2>Plan & identities</h2><p>Manage this device.</p>${activationSettingsContent()}<div class="settings-subhead"><h3>Private identities</h3><p>Only one is active at a time.</p></div><div class="identity-list">${identities}</div>${recovery}<form class="inline-form identity-create-form" id="identity-slot-form"><input id="identity-slot-label" maxlength="80" placeholder="New identity label" required/><button class="button primary">Create identity</button></form><details class="recovery-import"><summary>Recover another identity</summary><form id="identity-recover-form" class="setup-surface"><input id="identity-recover-label" maxlength="80" placeholder="Identity label" required/><textarea id="identity-recover-phrase" rows="3" placeholder="12-word recovery phrase" required></textarea><button class="button">Recover identity</button></form></details><div class="danger-zone"><h3>Burn</h3><p>Review the scope before anything is removed.</p><button class="button danger" id="burn-identity-button" data-open-burn="account">Open Burn</button></div>`;
}

function activationSettingsContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const accessLabel = licenseState.access === "offlineGrace" ? "Pro, offline grace" : pro ? "Pro active" : "Free";
  const period = licenseState.currentPeriodEnd === null ? "" : `<small>${licenseState.status === "CANCELLED" ? "Access through" : "Current period ends"} ${formatUnixDate(licenseState.currentPeriodEnd)}</small>`;
  const clear = licenseState.status === "UNCONFIGURED" ? "" : `<button class="button compact" id="clear-activation-code" type="button">Clear activation</button>`;
  return `<section class="license-card" aria-labelledby="activation-heading"><div class="license-state"><div><h3 id="activation-heading">Plan</h3><strong>${accessLabel}</strong>${period}</div><span class="status-tag ${pro ? "active" : ""}">${escapeHtml(licenseState.status === "UNCONFIGURED" ? "Not activated" : licenseState.status)}</span></div><p>After checkout, your activation code appears in the browser. Paste it here for instant activation. No email is required.</p><form id="activation-form" class="license-form"><label for="activation-code">Activation code</label><div><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Activate Pro</button>${clear}</div></form></section>`;
}

function formatUnixDate(seconds: number): string {
  return new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" }).format(new Date(seconds * 1_000));
}

function appearanceSettingsContent(): string {
  return `<h2>Appearance</h2><p>Choose a theme. Arrange apps with Edit on Home.</p><div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong>${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>`;
}

async function prepareServiceBurn(): Promise<void> {
  const target = activeServiceBurnTarget();
  serviceBurnReadiness = null;
  if (!target || !burnDialogOpen || burnScope !== "app") { render(); return; }
  serviceBurnReadinessBusy = true;
  render();
  const readiness = await getHubServiceBurnReadiness(target.serviceId, target.accountId);
  if (!burnDialogOpen || burnScope !== "app") return;
  serviceBurnReadiness = readiness?.coverageComplete === true ? readiness : null;
  serviceBurnReadinessBusy = false;
  render();
}

function bindBurnDialog(): void {
  if (!burnDialogOpen) return;
  document.querySelectorAll<HTMLButtonElement>("[data-close-burn]").forEach((button) => button.addEventListener("click", closeBurnDialog));
  const dialog = document.querySelector<HTMLDialogElement>("#burn-dialog");
  dialog?.addEventListener("cancel", (event) => { event.preventDefault(); closeBurnDialog(); });
  dialog?.addEventListener("close", () => { if (burnDialogOpen) closeBurnDialog(); });
  document.querySelectorAll<HTMLButtonElement>("[data-burn-scope]").forEach((button) => button.addEventListener("click", () => {
    const next = button.dataset.burnScope as BurnScope;
    if (burnScopeReason(next)) return;
    burnScope = next;
    burnResult = null;
    render();
    if (next === "app") void prepareServiceBurn();
  }));
  const input = document.querySelector<HTMLInputElement>("#burn-confirm-input");
  const submit = document.querySelector<HTMLButtonElement>("#burn-confirm-submit");
  const validate = (): void => {
    if (!input || !submit) return;
    submit.disabled = burnBusy || input.value !== burnConfirmationPhrase(burnScope) || burnScopeReason(burnScope) !== null;
  };
  input?.addEventListener("input", validate);
  document.querySelector<HTMLFormElement>("#burn-confirm-form")?.addEventListener("submit", (event) => void executeBurn(event));
}

function closeOwnedConfirmation(): void {
  ownedConfirmation = null;
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
}

function bindOwnedConfirmation(): void {
  if (!ownedConfirmation) return;
  document.querySelectorAll<HTMLButtonElement>("[data-close-owned-confirmation]").forEach((button) => button.addEventListener("click", closeOwnedConfirmation));
  const dialog = document.querySelector<HTMLDialogElement>("#owned-confirmation-dialog");
  dialog?.addEventListener("cancel", (event) => { event.preventDefault(); closeOwnedConfirmation(); });
  dialog?.addEventListener("close", () => { if (ownedConfirmation) closeOwnedConfirmation(); });
  document.querySelector<HTMLButtonElement>("#owned-confirmation-submit")?.addEventListener("click", () => void executeOwnedConfirmation());
}

function resetLocalProtectedSheet(): void {
  activeContextToken = null;
  localProtectedSheet = blankLocalProtectedModel();
  void setLocalProtectedSheetOpen(false);
}

async function closeActiveServiceSurface(): Promise<void> {
  if (activeEmbeddedHost) await closeEmbeddedServiceHost().catch(() => undefined);
  if (activeNativeHostId) await detachNativeAppWindow().catch(() => undefined);
  activeEmbeddedHost = null;
  activeNativeHostId = null;
  resetLocalProtectedSheet();
}

async function toggleLocalProtectedSheet(): Promise<void> {
  if (!activeEmbeddedHost) return;
  if (localProtectedSheet.open) {
    localProtectedSheet = blankLocalProtectedModel();
    activeContextToken = null;
    render();
    await setLocalProtectedSheetOpen(false);
    return;
  }
  if (!(await setLocalProtectedSheetOpen(true))) {
    showToast("Local protection could not open safely");
    return;
  }
  localProtectedSheet = blankLocalProtectedModel(true);
  render();
}

async function startLocalProtectedContext(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (!activeEmbeddedHost || localProtectedSheet.busy) return;
  const input = document.querySelector<HTMLInputElement>("#local-chat-label");
  const label = input?.value.trim() ?? "";
  if (!validLocalChatLabel(label)) {
    localProtectedSheet.status = "Use a short chat name.";
    render();
    return;
  }
  localProtectedSheet.chatLabel = label;
  localProtectedSheet.busy = true;
  localProtectedSheet.status = "";
  render();
  try {
    const conversationId = loadOrCreateLocalConversationId(
      localStorage,
      activeEmbeddedHost.serviceId,
      activeEmbeddedHost.accountId,
    );
    const context = await activateLocalLoopbackContext(
      activeEmbeddedHost.serviceId,
      activeEmbeddedHost.accountId,
      conversationId,
    );
    if (!context) throw new Error("local context unavailable");
    localProtectedSheet.context = context;
    activeContextToken = context.contextToken;
    localProtectedSheet.status = "Ready on this device.";
  } catch {
    localProtectedSheet.status = "Could not start. Nothing was sent.";
  } finally {
    localProtectedSheet.busy = false;
    render();
  }
}

async function prepareLocalProtectedDraft(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const contextToken = localProtectedSheet.context?.contextToken;
  const draft = document.querySelector<HTMLTextAreaElement>("#local-protected-draft");
  const ttl = document.querySelector<HTMLSelectElement>("#local-protected-ttl");
  const viewOnce = document.querySelector<HTMLInputElement>("#local-protected-view-once");
  const plaintext = draft?.value ?? "";
  const ttlSeconds = Number(ttl?.value ?? 0);
  if (!contextToken || !plaintext.trim() || ![0, 3_600, 86_400, 604_800].includes(ttlSeconds)) {
    localProtectedSheet.status = "Write a message first.";
    render();
    return;
  }
  localProtectedSheet.busy = true;
  localProtectedSheet.ttlSeconds = ttlSeconds;
  localProtectedSheet.viewOnce = viewOnce?.checked === true;
  localProtectedSheet.status = "";
  render();
  const policy = await saveActiveContextSecurity(contextToken, ttlSeconds, true);
  const prepared = policy
    ? await prepareLocalProtectedText(contextToken, plaintext, localProtectedSheet.viewOnce)
    : null;
  localProtectedSheet.busy = false;
  if (!prepared) {
    localProtectedSheet.status = "Encryption failed closed. Nothing was sent.";
    render();
    return;
  }
  localProtectedSheet.capsule = prepared.capsule;
  localProtectedSheet.status = "Encrypted on this device.";
  render();
}

async function openLocalProtectedCapsule(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const contextToken = localProtectedSheet.context?.contextToken;
  const input = document.querySelector<HTMLTextAreaElement>("#local-capsule-input");
  const capsule = input?.value.trim() ?? "";
  if (!contextToken || !capsule) {
    localProtectedSheet.status = "Paste encrypted text first.";
    render();
    return;
  }
  localProtectedSheet.busy = true;
  localProtectedSheet.openedPlaintext = "";
  localProtectedSheet.status = "";
  render();
  const opened = await decryptLocalProtectedText(contextToken, capsule);
  localProtectedSheet.busy = false;
  if (!opened) {
    localProtectedSheet.status = "This text does not open in this local chat.";
    render();
    return;
  }
  localProtectedSheet.openedPlaintext = opened.plaintext;
  localProtectedSheet.status = opened.viewOnceConsumed ? "Opened once. Its local key was removed." : "Opened on this device.";
  render();
}

async function copyLocalProtectedCapsule(): Promise<void> {
  if (!localProtectedSheet.capsule) return;
  try {
    await navigator.clipboard.writeText(localProtectedSheet.capsule);
    localProtectedSheet.status = "Copied. Paste and send it yourself.";
  } catch {
    localProtectedSheet.status = "Copy failed. Select the encrypted text manually.";
  }
  render();
}

function bindLocalProtectedSheet(): void {
  document.querySelector<HTMLButtonElement>("#local-protected-toggle")?.addEventListener("click", () => void toggleLocalProtectedSheet());
  document.querySelector<HTMLButtonElement>("#local-protected-close")?.addEventListener("click", () => void toggleLocalProtectedSheet());
  document.querySelector<HTMLFormElement>("#local-context-form")?.addEventListener("submit", (event) => void startLocalProtectedContext(event));
  document.querySelector<HTMLFormElement>("#local-protect-form")?.addEventListener("submit", (event) => void prepareLocalProtectedDraft(event));
  document.querySelector<HTMLFormElement>("#local-open-form")?.addEventListener("submit", (event) => void openLocalProtectedCapsule(event));
  document.querySelector<HTMLButtonElement>("#local-capsule-copy")?.addEventListener("click", () => void copyLocalProtectedCapsule());
  document.querySelectorAll<HTMLButtonElement>("[data-local-pane]").forEach((button) => button.addEventListener("click", () => {
    localProtectedSheet.pane = button.dataset.localPane as LocalProtectedPane;
    localProtectedSheet.openedPlaintext = "";
    localProtectedSheet.status = "";
    render();
  }));
}

function bindWorkspace(): void {
  bindPasswordVisibility();
  bindLocalProtectedSheet();
  bindSavedAccountControls();
  if (!activeContextToken) {
    const encryptedMode = document.querySelector<HTMLButtonElement>('[data-mode="protected"]');
    if (encryptedMode) {
      encryptedMode.disabled = true;
      encryptedMode.title = "Encrypted mode unlocks after OSL verifies the exact chat and recipients";
    }
    for (const selector of ["#decrypt-display", "#timer-button"]) {
      const control = document.querySelector<HTMLInputElement | HTMLButtonElement>(selector);
      if (control) control.disabled = true;
    }
  }
  document.querySelectorAll<HTMLButtonElement>("[data-route]").forEach((button) => button.addEventListener("click", async () => {
    const intent = ++navigationIntentEpoch;
    await Promise.resolve();
    if (intent !== navigationIntentEpoch) return;
    if (activeEmbeddedHost || activeNativeHostId) await closeActiveServiceSurface();
    if (route === "settings" && settingsSection === "scrub") clearPrivacyScanState();
    if (route === "settings" && settingsSection === "account") newIdentityRecoveryPhrase = null;
    route = button.dataset.route as Route;
    activeService = null;
    activeHomeAppId = null;
    appLaunchPendingId = null;
    serviceAccountPickerOpen = false;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-service]").forEach((button) => button.addEventListener("click", () => { const service = services.find((item) => item.id === button.dataset.service); if (service) openServiceRoute(service, null); }));
  document.querySelectorAll<HTMLButtonElement>("[data-home-app]").forEach((button) => button.addEventListener("click", () => {
    if (appLaunchPendingId) return;
    const appId = button.dataset.homeApp as HomeAppId;
    if (!homeAppsFromServices(services).some((candidate) => candidate.id === appId)) return;
    const intent = ++navigationIntentEpoch;
    appLaunchPendingId = appId;
    renderNow();
    void openHomeAppFromLauncher(appId, intent);
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-settings]").forEach((button) => button.addEventListener("click", () => {
    const next = button.dataset.settings as SettingsSection;
    if (settingsSection === "scrub" && next !== "scrub") clearPrivacyScanState();
    if (settingsSection === "account" && next !== "account") newIdentityRecoveryPhrase = null;
    settingsSection = next;
    render();
    if (next === "cleanup") void refreshMassCleanupCapabilities();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"; settingsSection = "notifications"; render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#privacy-export-input")?.addEventListener("change", (event) => void scanPrivacyExport(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLButtonElement>("#clear-privacy-scan")?.addEventListener("click", () => { privacyScanResult = null; privacyScanFileName = null; selectedScrubFindings.clear(); scrubReviewOpen = false; scrubReviewConfirmed = false; render(); });
  bindScrubControls();
  document.querySelector<HTMLFormElement>("#activation-form")?.addEventListener("submit", (event) => void activatePro(event));
  document.querySelectorAll<HTMLFormElement>("[data-password-role]").forEach((form) => form.addEventListener("submit", (event) => void submitPasswordRole(event)));
  document.querySelector<HTMLInputElement>("#activation-code")?.addEventListener("pointerdown", (event) => {
    event.stopPropagation();
    (event.currentTarget as HTMLInputElement).focus({ preventScroll: true });
  });
  document.querySelector<HTMLButtonElement>("#clear-activation-code")?.addEventListener("click", requestClearProActivation);
  document.querySelector<HTMLFormElement>("#identity-slot-form")?.addEventListener("submit", (event) => void createAdditionalIdentity(event));
  document.querySelector<HTMLFormElement>("#identity-recover-form")?.addEventListener("submit", (event) => void recoverAdditionalIdentity(event));
  document.querySelectorAll<HTMLButtonElement>("[data-switch-identity]").forEach((button) => button.addEventListener("click", () => void switchIdentity(button.dataset.switchIdentity ?? "")));
  document.querySelectorAll<HTMLButtonElement>("[data-open-burn]").forEach((button) => button.addEventListener("click", () => {
    burnScope = button.dataset.openBurn === "account" ? "account" : button.dataset.openBurn === "app" ? "app" : "chat";
    burnDialogOpen = true;
    burnResult = null;
    render();
    if (burnScope === "app") void prepareServiceBurn();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-theme-choice]").forEach((button) => button.addEventListener("click", () => {
    const next = parseTheme(button.dataset.themeChoice ?? null);
    themeChoice = next;
    localStorage.setItem("osl-hub-theme", next);
    applyTheme(next);
    render();
  }));
  document.querySelector("#service-guide-next")?.addEventListener("click", () => {
    if (serviceGuideStep !== null) setServiceGuideStep(nextServiceGuideStep(serviceGuideStep));
  });
  document.querySelector<HTMLButtonElement>("#embedded-service-setup")?.addEventListener("click", () => void setupEmbeddedApp(false));
  document.querySelector<HTMLButtonElement>("#add-service-profile")?.addEventListener("click", () => void setupEmbeddedApp(true));
  document.querySelectorAll<HTMLButtonElement>("[data-service-account]").forEach((button) => button.addEventListener("click", () => {
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    if (app && service) void openEmbeddedApp(app, service, button.dataset.serviceAccount);
  }));
  document.querySelector<HTMLButtonElement>("#onboarding-service-continue")?.addEventListener("click", () => void continueOnboardingFromService());
  document.querySelector("#service-guide-back")?.addEventListener("click", () => {
    if (serviceGuideStep !== null) setServiceGuideStep(previousServiceGuideStep(serviceGuideStep));
  });
  document.querySelector("#service-guide-skip")?.addEventListener("click", () => {
    if (onboardingServiceSetup) {
      onboardingServiceSetup = false;
      clearServiceGuide();
      void completeOnboarding();
      return;
    }
    clearServiceGuide();
    render();
  });
  document.querySelector("#service-guide-finish")?.addEventListener("click", () => {
    if (onboardingServiceSetup) {
      onboardingServiceSetup = false;
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "sending";
      activeService = null;
      activeHomeAppId = null;
      render();
      return;
    }
    clearServiceGuide();
    render();
  });
  document.querySelector("#service-guide-exit")?.addEventListener("click", async () => {
    if (onboardingServiceSetup) {
      onboardingServiceSetup = false;
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "tutorial";
      activeService = null;
      activeHomeAppId = null;
      await closeActiveServiceSurface();
      render();
      return;
    }
    await closeActiveServiceSurface();
    route = "home";
    activeService = null;
    activeHomeAppId = null;
    serviceAccountPickerOpen = false;
    render();
  });
  document.querySelector("#native-app-back")?.addEventListener("click", async () => { await closeActiveServiceSurface(); serviceAccountPickerOpen = false; route = "home"; activeService = null; activeHomeAppId = null; render(); });
  document.querySelector("#edit-home")?.addEventListener("click", () => { homeEditMode = !homeEditMode; render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-tile-move]").forEach((button) => button.addEventListener("click", () => moveHomeTile(button.dataset.tileMove ?? "")));
  document.querySelectorAll<HTMLButtonElement>("[data-tile-toggle]").forEach((button) => button.addEventListener("click", () => toggleHomeTile(button.dataset.tileToggle ?? "")));
  document.querySelectorAll<HTMLElement>("[data-tile-id][draggable=true]").forEach((tile) => {
    tile.addEventListener("dragstart", (event) => {
      draggingHomeTileId = tile.dataset.tileId ?? null;
      tile.classList.add("dragging");
      if (event.dataTransfer && draggingHomeTileId) {
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", draggingHomeTileId);
      }
    });
    tile.addEventListener("dragover", (event) => { if (draggingHomeTileId && draggingHomeTileId !== tile.dataset.tileId) event.preventDefault(); });
    tile.addEventListener("drop", (event) => {
      event.preventDefault();
      reorderHomeTile(draggingHomeTileId, tile.dataset.tileId ?? null);
    });
    tile.addEventListener("dragend", () => { draggingHomeTileId = null; tile.classList.remove("dragging"); });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-home-module]").forEach((button) => button.addEventListener("click", () => openHomeModule(button.dataset.homeModule ?? "")));
  document.querySelector("#timer-button")?.addEventListener("click", () => void cycleContextTimer());
  document.querySelector("#people-button")?.addEventListener("click", () => {
    const dialog = document.querySelector<HTMLDialogElement>("#people-dialog");
    if (dialog && !dialog.open) dialog.showModal();
  });
  document.querySelector("#people-dialog-close")?.addEventListener("click", () => document.querySelector<HTMLDialogElement>("#people-dialog")?.close());
  document.querySelectorAll<HTMLButtonElement>("[data-verify-person]").forEach((button) => button.addEventListener("click", () => requestFriendVerification(button.dataset.verifyPerson ?? "", button.dataset.safetyNumber ?? "")));
  document.querySelectorAll<HTMLButtonElement>("[data-allow-person]").forEach((button) => button.addEventListener("click", () => void allowPersonHere(button.dataset.allowPerson ?? "")));
  document.querySelectorAll<HTMLElement>("[data-open-friends]").forEach((button) => button.addEventListener("click", () => {
    friendsDialogOpen = true;
    friendsDialogPage = 0;
    render();
  }));
  document.querySelector("#friends-dialog-close")?.addEventListener("click", () => {
    friendsDialogOpen = false;
    document.querySelector<HTMLDialogElement>("#friends-dialog")?.close();
    render();
  });
  document.querySelector<HTMLDialogElement>("#friends-dialog")?.addEventListener("close", () => { if (friendsDialogOpen) { friendsDialogOpen = false; render(); } });
  document.querySelectorAll<HTMLButtonElement>("[data-friends-page]").forEach((button) => button.addEventListener("click", () => {
    const next = Number(button.dataset.friendsPage);
    if (!Number.isSafeInteger(next) || next < 0) return;
    friendsDialogPage = next;
    render();
  }));
  document.querySelector<HTMLFormElement>("#add-friend-form")?.addEventListener("submit", (event) => void submitFriendCode(event));
  document.querySelectorAll<HTMLFormElement>("[data-nickname-person]").forEach((form) => form.addEventListener("submit", (event) => void saveFriendNickname(event)));
  document.querySelector<HTMLButtonElement>("#copy-friend-code")?.addEventListener("click", () => void copyFriendInvite());
  document.querySelector<HTMLInputElement>("#notifications-opt-in")?.addEventListener("change", (event) => void changeNotifications(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#notification-previews")?.addEventListener("change", (event) => { notificationPreviewContent = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationPreviewStorageKey, String(notificationPreviewContent)); });
  document.querySelector<HTMLInputElement>("#notification-scope-suggestions")?.addEventListener("change", (event) => { notificationScopeSuggestions = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationScopeStorageKey, String(notificationScopeSuggestions)); });
  document.querySelectorAll<HTMLInputElement>("[data-notification-app]").forEach((input) => input.addEventListener("change", () => { const id = input.dataset.notificationApp as ServiceId; notificationAppPreferences[id] = input.checked; localStorage.setItem(notificationAppsStorageKey, JSON.stringify(notificationAppPreferences)); }));
  bindBurnDialog();
  bindOwnedConfirmation();
  bindUpdateControls();
}

async function openHomeAppFromLauncher(appId: HomeAppId, intent: number): Promise<void> {
  try {
    const refreshed = await withNativeDeadline(loadLinkedServices(), "Refresh apps", 450).catch(() => null);
    if (intent !== navigationIntentEpoch) return;
    if (refreshed) services = refreshed;
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === appId);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    if (!app || !service) {
      showToast("This app is unavailable right now");
      return;
    }
    appLaunchPendingId = null;
    const native = nativeApps.find((candidate) => candidate.id === app.id && candidate.availability === "installed");
    if (native) {
      void openNativeHostedApp(app, service, native.id);
    } else if (app.linked && savedAccountMode !== "clean") {
      void openEmbeddedApp(app, service);
    } else if (savedAccountMode !== "ask") {
      activeService = service;
      activeHomeAppId = app.id;
      route = "service";
      serviceGuideStep = null;
      void setupEmbeddedApp(true);
    } else {
      openServiceRoute(service, app.provider, app.id, true);
    }
  } finally {
    if (intent === navigationIntentEpoch && appLaunchPendingId === appId) {
      appLaunchPendingId = null;
      render();
    }
  }
}

async function startBackgroundInstall(appId: NativeAppId): Promise<void> {
  if (backgroundInstallIds.has(appId)) return;
  const app = nativeApps.find((candidate) => candidate.id === appId);
  if (!app || app.availability !== "installable") return;
  backgroundInstallIds.add(appId);
  render();
  try {
    await withNativeDeadline(installNativeApp(appId), "Start background install");
    showToast(`${app.displayName} is installing in the background`);
    for (let attempt = 0; attempt < 40; attempt += 1) {
      await new Promise<void>((resolve) => window.setTimeout(resolve, 3_000));
      nativeApps = await loadNativeApps().catch(() => nativeApps);
      if (nativeApps.some((candidate) => candidate.id === appId && candidate.availability === "installed")) {
        savedNativeApps.add(appId);
        persistSavedAccountPreferences();
        showToast(`${app.displayName} is ready`);
        return;
      }
    }
    showToast(`${app.displayName} is still installing in Windows`);
  } catch (failure) {
    showToast(localActionError(failure, "Background install could not start"));
  } finally {
    backgroundInstallIds.delete(appId);
    render();
  }
}

function enqueueBackgroundInstalls(appIds: NativeAppId[]): void {
  const unique = [...new Set(appIds)].filter((appId) => supportedNativeAppIds.has(appId));
  backgroundInstallQueue = backgroundInstallQueue.then(async () => {
    for (const appId of unique) await startBackgroundInstall(appId);
  }).catch(() => undefined);
}

function nativeHostFailureMessage(reason: string, name: string): string {
  if (reason === "secondaryInstanceUnverified") return `${name} cannot safely open a separate OSL window yet`;
  if (reason === "appNotInstalled") return `Install ${name} first`;
  if (reason === "windowNotFound") return `${name} opened, but its OSL window was not found`;
  if (reason === "profileUnavailable") return `${name}'s separate OSL profile is unavailable`;
  return `${name} could not open as a native OSL window`;
}

async function openNativeHostedApp(app: HomeAppCatalogEntry, service: LinkedService, appId: NativeAppId): Promise<void> {
  if (nativeActionBusy) return;
  if (activeNativeHostId === appId) {
    await withNativeDeadline(focusNativeAppWindow(), `Focus ${app.displayName}`, 3_000).catch(() => undefined);
    return;
  }
  navigationIntentEpoch += 1;
  nativeActionBusy = true;
  activeService = service;
  activeHomeAppId = app.id;
  route = "service";
  serviceGuideStep = null;
  serviceAccountPickerOpen = false;
  resetLocalProtectedSheet();
  render();
  try {
    if (activeEmbeddedHost) {
      await closeEmbeddedServiceHost().catch(() => undefined);
      activeEmbeddedHost = null;
    }
    if (activeNativeHostId && activeNativeHostId !== appId) {
      await detachNativeAppWindow().catch(() => undefined);
      activeNativeHostId = null;
    }
    const result = await withNativeDeadline(hostNativeAppWindow(appId), `Open ${app.displayName} inside OSL`, 12_000);
    if (result.status !== "hosted") {
      activeNativeHostId = null;
      serviceGuideStep = 0;
      showToast(nativeHostFailureMessage(result.reason, app.displayName));
      return;
    }
    activeNativeHostId = appId;
    showToast(`${app.displayName} opened in a separate OSL profile`);
  } catch (failure) {
    activeNativeHostId = null;
    serviceGuideStep = 0;
    showToast(localActionError(failure, `${app.displayName} could not open inside OSL`));
  } finally {
    nativeActionBusy = false;
    render();
  }
}

async function setupEmbeddedApp(forceNewProfile = false): Promise<void> {
  if (nativeActionBusy || !activeHomeAppId) return;
  const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId);
  if (!app?.serviceId || app.launchState !== "available") return;
  nativeActionBusy = true;
  resetLocalProtectedSheet();
  render();
  try {
    const native = nativeApps.find((candidate) => candidate.id === app.id && candidate.availability === "installed");
    if (native) {
      const service = services.find((candidate) => candidate.id === app.serviceId);
      if (!service) throw new Error("This app is unavailable right now");
      nativeActionBusy = false;
      await openNativeHostedApp(app, service, native.id);
      return;
    }
    const existingProfiles = embeddedAccountsForHomeApp(app, services);
    const opened = app.linked && !forceNewProfile
      ? { host: await openEmbeddedHomeApp(app, services) }
      : await setupEmbeddedHomeApp(app, existingProfiles.length === 0 ? "Personal" : `Profile ${existingProfiles.length + 1}`);
    activeEmbeddedHost = opened.host;
    serviceAccountPickerOpen = false;
    services = await loadLinkedServices().catch(() => services);
    serviceGuideStep = null;
    localStorage.removeItem(serviceGuideStorageKey);
    render();
  } catch (failure) {
    showToast(localActionError(failure, "This app could not open inside OSL"));
  } finally {
    nativeActionBusy = false;
    render();
  }
}

async function openEmbeddedApp(app: HomeAppCatalogEntry, service: LinkedService, accountId?: string): Promise<void> {
  if (nativeActionBusy) return;
  navigationIntentEpoch += 1;
  resetLocalProtectedSheet();
  activeService = service;
  activeHomeAppId = app.id;
  route = "service";
  serviceGuideStep = null;
  const accounts = embeddedAccountsForHomeApp(app, services);
  if (!accountId && accounts.length > 1) {
    activeEmbeddedHost = null;
    serviceAccountPickerOpen = true;
    render();
    return;
  }
  nativeActionBusy = true;
  serviceAccountPickerOpen = false;
  render();
  try {
    const native = nativeApps.find((candidate) => candidate.id === app.id && candidate.availability === "installed");
    if (native) {
      nativeActionBusy = false;
      await openNativeHostedApp(app, service, native.id);
      return;
    }
    activeEmbeddedHost = await openEmbeddedHomeApp(app, services, accountId);
  } catch (failure) {
    activeEmbeddedHost = null;
    resetLocalProtectedSheet();
    serviceGuideStep = 0;
    showToast(localActionError(failure, "This app could not open inside OSL"));
  } finally {
    nativeActionBusy = false;
    render();
  }
}

async function continueOnboardingFromService(): Promise<void> {
  await closeActiveServiceSurface();
  onboardingServiceSetup = false;
  route = "onboarding";
  onboardingRoute = "sending";
  activeService = null;
  activeHomeAppId = null;
  render();
}

function currentHomeTileIds(): string[] {
  return [
    ...homeAppsFromServices(services).filter((app) => app.visibility === "launch").map((app) => app.id),
    "osl-chats", "osl-groups", "notifications", "osl-notes",
  ];
}

function moveHomeTile(raw: string): void {
  const separator = raw.lastIndexOf(":");
  const id = raw.slice(0, separator);
  const delta = Number(raw.slice(separator + 1));
  const defaults = currentHomeTileIds();
  const order = [...homeTileOrder.filter((item) => defaults.includes(item)), ...defaults.filter((item) => !homeTileOrder.includes(item))];
  const index = order.indexOf(id);
  const target = index + delta;
  if (index < 0 || !Number.isSafeInteger(delta) || Math.abs(delta) !== 1 || target < 0 || target >= order.length) return;
  [order[index], order[target]] = [order[target], order[index]];
  homeTileOrder = order;
  saveHomeTilePreferences();
  render();
}

function reorderHomeTile(sourceId: string | null, targetId: string | null): void {
  if (!sourceId || !targetId || sourceId === targetId) return;
  const defaults = currentHomeTileIds();
  const order = [...homeTileOrder.filter((item) => defaults.includes(item)), ...defaults.filter((item) => !homeTileOrder.includes(item))];
  const source = order.indexOf(sourceId);
  const target = order.indexOf(targetId);
  if (source < 0 || target < 0) return;
  order.splice(source, 1);
  order.splice(target, 0, sourceId);
  homeTileOrder = order;
  saveHomeTilePreferences();
  render();
}

function toggleHomeTile(id: string): void {
  if (!currentHomeTileIds().includes(id)) return;
  if (hiddenHomeTiles.has(id)) hiddenHomeTiles.delete(id); else hiddenHomeTiles.add(id);
  saveHomeTilePreferences();
  render();
}

function openHomeModule(id: string): void {
  if (id === "osl-chats" || id === "osl-groups") {
    friendsDialogOpen = true;
    friendsDialogPage = 0;
    render();
  } else if (id === "notifications") {
    route = "settings";
    settingsSection = "notifications";
    render();
  } else {
    showToast("OSL Notes is planned for a later release");
  }
}

async function submitFriendCode(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const input = document.querySelector<HTMLInputElement>("#friend-code-input");
  const nicknameInput = document.querySelector<HTMLInputElement>("#friend-nickname-input");
  const button = document.querySelector<HTMLButtonElement>("#add-friend-form button");
  const status = document.querySelector<HTMLElement>("#friend-form-status");
  const code = input?.value.trim() ?? "";
  if (!/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(code)) {
    if (status) status.textContent = "Enter a valid OSL invite.";
    input?.focus();
    return;
  }
  if (button) button.disabled = true;
  if (status) status.textContent = "Saving request locally…";
  const added = await addOslFriend(code, nicknameInput?.value ?? "");
  if (button) button.disabled = false;
  if (!added) {
    if (status) status.textContent = "The invite could not be added. Nothing changed.";
    return;
  }
  if (input) input.value = "";
  if (nicknameInput) nicknameInput.value = "";
  hubPeople = await listHubPeople() ?? hubPeople;
  render();
  showToast("Friend added. Encrypted chats are still off.");
}

async function saveFriendNickname(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const personId = form.dataset.nicknamePerson ?? "";
  const input = form.elements.namedItem("nickname") as HTMLInputElement | null;
  const button = form.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (!input || !personId) return;
  button?.setAttribute("disabled", "");
  const updated = await setHubFriendNickname(personId, input.value);
  if (!updated) {
    button?.removeAttribute("disabled");
    showToast("Nickname was not saved · use 48 visible characters or fewer");
    return;
  }
  hubPeople = hubPeople.map((person) => person.personId === updated.personId ? updated : person);
  render();
  showToast(updated.alias ? "Nickname saved on this device" : "Nickname removed from this device");
}

async function copyFriendInvite(): Promise<void> {
  if (!friendCode) { showToast("Friend invite is unavailable"); return; }
  try {
    await navigator.clipboard.writeText(friendCode);
    showToast("Invite copied");
  } catch {
    showToast("Could not copy the invite");
  }
}

function requestFriendVerification(personId: string, verificationCode: string): void {
  if (!personId || !verificationCode) return;
  friendsDialogOpen = false;
  ownedConfirmation = { kind: "verifyFriend", personId, verificationCode };
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
}

async function allowPersonHere(personId: string): Promise<void> {
  if (!activeContextToken) return;
  if (!(await setActiveHubFriendPermission(activeContextToken, personId, true))) { showToast("Chat approval could not be saved"); return; }
  hubPeople = await listHubPeople() ?? hubPeople;
  renderNow();
  const dialog = document.querySelector<HTMLDialogElement>("#people-dialog");
  if (dialog && !dialog.open) dialog.showModal();
  showToast("Verified friend approved for this chat");
}

async function changeNotifications(input: HTMLInputElement): Promise<void> {
  const requested = input.checked;
  input.disabled = true;
  const saved = await setNotificationsEnabled(requested);
  if (!saved) {
    input.checked = notificationsEnabled;
    input.disabled = false;
    showToast("Notification setting is unavailable · nothing changed");
    return;
  }
  notificationsEnabled = requested;
  localStorage.setItem(notificationsStorageKey, String(requested));
  appNotifications = requested ? await loadAppNotifications() : [];
  render();
}

async function changeScreenshotProtection(input: HTMLInputElement): Promise<void> {
  const requested = input.checked;
  input.disabled = true;
  if (!(await setScreenshotProtection(requested))) {
    input.checked = screenshotProtectionEnabled;
    input.disabled = false;
    showToast("Windows capture resistance could not be changed");
    return;
  }
  screenshotProtectionEnabled = requested;
  localStorage.setItem(screenshotProtectionStorageKey, String(requested));
  input.disabled = false;
  showToast(requested ? "Windows capture resistance enabled" : "Windows capture resistance disabled");
}

async function refreshIdentitySlots(): Promise<void> {
  hubIdentities = await listHubIdentities() ?? [];
}

async function refreshIdentityScopedState(): Promise<void> {
  const [nextCore, nextIdentities, profile, people, linkedServices, notifications] = await Promise.all([
    loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration)),
    listHubIdentities().then((value) => value ?? []),
    loadFriendProfile(),
    listHubPeople().then((value) => value ?? []),
    loadLinkedServices().catch(() => []),
    notificationsEnabled ? loadAppNotifications() : Promise.resolve([]),
  ]);
  core = nextCore;
  hubIdentities = nextIdentities;
  friendCode = profile?.friendCode ?? null;
  friendDisplayId = profile?.oslUserId ?? null;
  hubPeople = people;
  services = linkedServices;
  appNotifications = notifications;
  passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
}

async function submitPasswordRole(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const role = form.dataset.passwordRole === "stealth" ? "stealth" : form.dataset.passwordRole === "burn" ? "burn" : null;
  const current = form.elements.namedItem("current") as HTMLInputElement | null;
  const alternate = form.elements.namedItem("alternate") as HTMLInputElement | null;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (!role || !current || !isValidMainPassword(current.value) || (alternate && !isValidNewMainPassword(alternate.value))) return;
  if (submit) submit.disabled = true;
  try {
    passwordRoleStatus = form.dataset.passwordRemove === "true"
      ? await removeHubAlternatePassword(role, current.value)
      : await setHubAlternatePassword(role, current.value, alternate?.value ?? "");
    current.value = "";
    if (alternate) alternate.value = "";
    render();
    const wired = role === "stealth" ? passwordRoleStatus.stealthActionWired : passwordRoleStatus.burnActionWired;
    showToast(wired ? `${role === "stealth" ? "Stealth" : "Burn"} password updated` : "Password saved. Its login action is not enabled yet.");
  } catch (failure) {
    if (submit) submit.disabled = false;
    showToast(localActionError(failure, "Password was not changed"));
  }
}

async function activatePro(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const input = document.querySelector<HTMLInputElement>("#activation-code");
  const submit = document.querySelector<HTMLButtonElement>('#activation-form button[type="submit"]');
  const activationCode = input?.value.trim() ?? "";
  if (!isActivationCode(activationCode)) {
    showToast("Enter the activation code shown after checkout");
    return;
  }
  if (input) input.value = "";
  if (submit) { submit.disabled = true; submit.textContent = "Activating…"; }
  try {
    licenseState = await validateHubActivationCode(activationCode);
    render();
    showToast(licenseState.access === "free" ? "This code does not include active Pro access" : "Pro activated on this device");
  } catch (failure) {
    if (submit) { submit.disabled = false; submit.textContent = "Activate Pro"; }
    showToast(localActionError(failure, "Activation failed. Check the code and try again."));
  }
}

function requestClearProActivation(): void {
  ownedConfirmation = { kind: "clearActivation" };
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
}

async function executeOwnedConfirmation(): Promise<void> {
  if (!ownedConfirmation || ownedConfirmationBusy) return;
  const request = ownedConfirmation;
  ownedConfirmationBusy = true;
  ownedConfirmationError = "";
  const submit = document.querySelector<HTMLButtonElement>("#owned-confirmation-submit");
  if (submit) { submit.disabled = true; submit.textContent = "Working…"; }
  try {
    if (request.kind === "verifyFriend") {
      if (!(await verifyHubPerson(request.personId, request.verificationCode))) {
        ownedConfirmationBusy = false;
        ownedConfirmationError = "Verification failed closed. Nothing changed.";
        render();
        return;
      }
      hubPeople = await listHubPeople() ?? hubPeople;
      closeOwnedConfirmation();
      showToast("Friend request accepted locally · no conversations approved");
      return;
    }
    licenseState = await clearHubActivationCode();
    closeOwnedConfirmation();
    showToast("Activation cleared from this device");
  } catch (failure) {
    ownedConfirmationBusy = false;
    ownedConfirmationError = localActionError(failure, request.kind === "clearActivation" ? "The saved activation could not be cleared." : "Verification failed closed. Nothing changed.");
    render();
  }
}

async function createAdditionalIdentity(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const input = document.querySelector<HTMLInputElement>("#identity-slot-label");
  const label = input?.value.trim() ?? "";
  if (!label) return;
  const created = await createHubIdentitySlot(label);
  if (!created) { showToast("Identity creation failed closed"); return; }
  newIdentityRecoveryPhrase = created.identityRecoveryPhrase;
  core = await loadCoreIntegration();
  await refreshIdentitySlots();
  render();
}

async function recoverAdditionalIdentity(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const labelInput = document.querySelector<HTMLInputElement>("#identity-recover-label");
  const phraseInput = document.querySelector<HTMLTextAreaElement>("#identity-recover-phrase");
  const label = labelInput?.value.trim() ?? "";
  const phrase = phraseInput?.value.trim() ?? "";
  if (!label || !phrase) return;
  const recovered = await recoverHubIdentitySlot(label, phrase);
  if (phraseInput) phraseInput.value = "";
  if (!recovered) { showToast("Identity recovery failed closed"); return; }
  newIdentityRecoveryPhrase = null;
  core = await loadCoreIntegration();
  await refreshIdentitySlots();
  render();
}

async function switchIdentity(slotId: string): Promise<void> {
  if (!(await switchHubIdentity(slotId))) { showToast("Identity switch failed closed"); return; }
  newIdentityRecoveryPhrase = null;
  await refreshIdentityScopedState();
  render();
}

async function executeBurn(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (!burnDialogOpen || burnBusy || burnScopeReason(burnScope)) return;
  const input = document.querySelector<HTMLInputElement>("#burn-confirm-input");
  if (!input || input.value !== burnConfirmationPhrase(burnScope)) return;
  const requestedUninstall = burnScope === "account" && document.querySelector<HTMLInputElement>("#burn-uninstall")?.checked === true;
  input.value = "";
  input.disabled = true;
  burnBusy = true;
  const submit = document.querySelector<HTMLButtonElement>("#burn-confirm-submit");
  const status = document.querySelector<HTMLElement>("#burn-form-status");
  if (submit) { submit.disabled = true; submit.textContent = "Burning…"; }
  if (status) status.textContent = "Removing local OSL data…";

  if (burnScope === "chat") {
    const contextToken = activeContextToken;
    if (!contextToken || !(await burnActiveHubContext(contextToken))) {
      burnBusy = false;
      burnResult = { tone: "error", message: "The chat burn failed closed. No deletion success is being claimed.", showUninstall: false };
      render();
      return;
    }
    burnBusy = false;
    burnResult = { tone: "success", message: "Local OSL decrypt material and caches for this chat were removed. Native app history was not deleted.", showUninstall: false };
    render();
    return;
  }

  if (burnScope === "app") {
    const target = activeServiceBurnTarget();
    const readiness = serviceBurnReadiness;
    if (!target || !readiness?.coverageComplete) {
      burnBusy = false;
      burnResult = { tone: "error", message: "OSL could not prove complete coverage. Nothing was removed.", showUninstall: false };
      render();
      return;
    }
    const result = await burnHubServiceAccount(target.serviceId, target.accountId, readiness.burnId);
    burnBusy = false;
    if (!result || !result.localCleanupComplete || !result.loginProfileUntouched || !result.nativeHistoryUntouched) {
      burnResult = { tone: "error", message: "The connected-account burn failed closed or its scope changed. No complete deletion is being claimed.", showUninstall: false };
      render();
      return;
    }
    activeContextToken = null;
    burnResult = {
      tone: result.remoteCleanupComplete ? "success" : "warning",
      message: result.remoteCleanupComplete
        ? `Local OSL decrypt material and caches for ${result.scopesBurned} ${result.scopesBurned === 1 ? "chat" : "chats"} were removed. The login profile, cookies, and service history remain.`
        : `Local OSL cleanup completed for ${result.scopesBurned} ${result.scopesBurned === 1 ? "chat" : "chats"}, but ${result.remoteBlobDeletionsFailed} remote blob ${result.remoteBlobDeletionsFailed === 1 ? "deletion was" : "deletions were"} not acknowledged. The login profile, cookies, and service history remain.`,
      showUninstall: false,
    };
    render();
    return;
  }

  const result = await executeHubFullCleanup();
  burnBusy = false;
  if (!result) {
    burnResult = { tone: "error", message: "Cleanup returned no verifiable result. No deletion success is being claimed.", showUninstall: false };
    render();
    return;
  }
  if (!result.localCleanupComplete) {
    burnResult = { tone: "warning", message: `Cleanup was partial. Removed: ${result.removedTargets.join(", ") || "none"}. Still present: ${result.failedTargets.join(", ") || "unknown"}. Restart OSL and retry.`, showUninstall: false };
    render();
    return;
  }
  localStorage.clear();
  newIdentityRecoveryPhrase = null;
  activeService = null;
  activeHomeAppId = null;
  await refreshIdentityScopedState();
  const unconfirmedRemote = result.remoteUnregister.failed + result.remoteUnregister.unavailable;
  burnResult = {
    tone: unconfirmedRemote > 0 ? "warning" : "success",
    message: unconfirmedRemote > 0
      ? `All local OSL data was removed. Remote unregister was not acknowledged for ${unconfirmedRemote} identity ${unconfirmedRemote === 1 ? "record" : "records"}; no remote deletion success is being claimed.`
      : "All local OSL identities, decrypt material, caches, and preferences were removed from this computer.",
    showUninstall: requestedUninstall,
  };
  render();
}

function ttlSeconds(label: string): number {
  return label === "1h" ? 3_600 : label === "24h" ? 86_400 : label === "7d" ? 604_800 : 259_200;
}

function ttlLabel(seconds: number): string {
  return seconds === 3_600 ? "1h" : seconds === 86_400 ? "24h" : seconds === 259_200 ? "72h" : seconds === 604_800 ? "7d" : `${Math.max(1, Math.round(seconds / 3_600))}h`;
}

async function cycleContextTimer(): Promise<void> {
  if (!activeContextToken) return;
  const next = timer === "1h" ? "24h" : timer === "24h" ? "72h" : timer === "72h" ? "7d" : "1h";
  const saved = await saveActiveContextSecurity(activeContextToken, ttlSeconds(next), decryptDisplay);
  if (!saved) { showToast("Expiry setting failed closed"); return; }
  timer = ttlLabel(saved.ttlSeconds);
  render();
}

async function changeDecryptDisplay(input: HTMLInputElement): Promise<void> {
  if (!activeContextToken) { input.checked = decryptDisplay; return; }
  const saved = await saveActiveContextSecurity(activeContextToken, ttlSeconds(timer), input.checked);
  if (!saved) { input.checked = decryptDisplay; showToast("Decrypt-display setting failed closed"); return; }
  decryptDisplay = saved.decryptDisplayEnabled;
  render();
  showToast(decryptDisplay ? "Encrypted messages may be decrypted locally" : "Encrypted messages stay encrypted on screen");
}

function openServiceRoute(service: LinkedService, _provider: EmailProvider | null = null, appId?: HomeAppId, forceGuide = false): void {
  navigationIntentEpoch += 1;
  activeService = service;
  activeHomeAppId = appId ?? service.id as HomeAppId;
  serviceAccountPickerOpen = false;
  route = "service";
  const saved = parseServiceGuideState(localStorage.getItem(serviceGuideStorageKey));
  serviceGuideStep = forceGuide ? 0 : saved?.serviceId === service.id ? saved.step : null;
  if (forceGuide) persistServiceGuideState();
  render();
}

function persistServiceGuideState(): void {
  if (!activeService || serviceGuideStep === null) return;
  localStorage.setItem(serviceGuideStorageKey, JSON.stringify({ serviceId: activeService.id, step: serviceGuideStep }));
}

function setServiceGuideStep(step: ServiceGuideStep): void {
  serviceGuideStep = step;
  persistServiceGuideState();
  render();
}

function clearServiceGuide(): void {
  serviceGuideStep = null;
  localStorage.removeItem(serviceGuideStorageKey);
}

function updateBannerMarkup(): string {
  if (route === "service") return "";
  if (updateStatus.state !== "available" && updateStatus.state !== "installing") return "";
  return `<aside class="update-banner" role="status"><span><strong>OSL ${escapeHtml(updateStatus.next)} is available</strong><small>Signed update · installation requires your click</small></span><div><button class="button compact" data-update-read>Read more on GitHub</button><button class="button compact primary" data-update-modal ${updateStatus.state === "installing" ? "disabled" : ""}>Install</button></div></aside>`;
}

function updateDialogMarkup(): string {
  if (updateStatus.state !== "available" && updateStatus.state !== "installing") return "";
  const notes = updateStatus.notes ? escapeHtml(updateStatus.notes) : "No release notes were provided.";
  return `<dialog class="unlock-dialog update-dialog" id="update-dialog" aria-labelledby="update-dialog-title"><div class="unlock-card"><p class="eyebrow">Signed OSL update</p><h2 id="update-dialog-title">Install ${escapeHtml(updateStatus.next)}?</h2><p class="update-notes">${notes}</p><p class="quiet-note">OSL will download, verify, install, and restart. Unsaved work may be lost. Nothing installs until you click Install & restart.</p><div class="control-row unlock-actions"><button class="button ghost" data-update-close>Not now</button><button class="button" data-update-read>Read more on GitHub</button><button class="button primary" data-update-install ${updateStatus.state === "installing" ? "disabled" : ""}>${updateStatus.state === "installing" ? "Installing…" : "Install & restart"}</button></div></div></dialog>`;
}

function updateSettingsContent(): string {
  const deviceReady = isCoreProtectionReady(core.readiness);
  const status = updateStatus.state === "checking" ? "Checking…"
    : updateStatus.state === "upToDate" ? `Up to date · ${escapeHtml(updateStatus.current)}`
    : updateStatus.state === "available" ? `Update available · ${escapeHtml(updateStatus.next)}`
    : updateStatus.state === "installing" ? "Downloading and verifying…"
    : updateStatus.state === "error" ? "Update check failed"
    : "Updater backend unavailable";
  const actions = updateStatus.state === "available" ? `<button class="button" data-update-read>Read more on GitHub</button><button class="button primary" data-update-modal>Install</button>` : "";
  return `<h2>About & updates</h2><p>Checks and installs run only through the trusted local updater. Release notes are bounded plain text; remote HTML is never rendered.</p><div class="update-status-card"><span class="dot"></span><div><strong>${status}</strong><small>No telemetry is sent by this UI.</small></div></div><div class="settings-actions"><button class="button" data-update-check ${updateStatus.state === "checking" || updateStatus.state === "installing" ? "disabled" : ""}>Check for updates</button>${actions}</div><div class="setting-line"><span>Device status</span><strong>${deviceReady ? "Ready" : "Needs attention"}</strong></div><details class="device-diagnostics"><summary>Diagnostics</summary><p>${escapeHtml(coreReadinessLabel(core.readiness))}</p></details>`;
}

function bindUpdateControls(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-update-check]").forEach((button) => button.addEventListener("click", () => void refreshUpdateStatus()));
  document.querySelectorAll<HTMLButtonElement>("[data-update-modal]").forEach((button) => button.addEventListener("click", () => {
    const dialog = document.querySelector<HTMLDialogElement>("#update-dialog");
    if (dialog && !dialog.open) dialog.showModal();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-update-close]").forEach((button) => button.addEventListener("click", () => document.querySelector<HTMLDialogElement>("#update-dialog")?.close()));
  document.querySelectorAll<HTMLButtonElement>("[data-update-read]").forEach((button) => button.addEventListener("click", async () => { if (!(await openHubReleasesPage())) showToast("Could not open the fixed OSL releases page"); }));
  document.querySelectorAll<HTMLButtonElement>("[data-update-install]").forEach((button) => button.addEventListener("click", () => void installUpdateAfterClick()));
}

async function refreshUpdateStatus(background = false): Promise<void> {
  updateStatus = { state: "checking" };
  if (route !== "onboarding") background ? renderWhenIdle() : render();
  updateStatus = await checkHubForUpdates();
  if (route !== "onboarding") background ? renderWhenIdle() : render();
}

async function installUpdateAfterClick(): Promise<void> {
  if (updateStatus.state !== "available") return;
  const expectedVersion = updateStatus.next;
  updateStatus = { ...updateStatus, state: "installing" };
  renderNow();
  const dialog = document.querySelector<HTMLDialogElement>("#update-dialog");
  if (dialog && !dialog.open) dialog.showModal();
  const result = await installHubUpdate(expectedVersion);
  if (result === "noUpdate") await refreshUpdateStatus();
  else { updateStatus = { state: "error" }; render(); showToast("Update was not installed"); }
}

function showToast(message: string): void {
  document.querySelector(".toast")?.remove();
  clearTimeout(toastTimer);
  const toast = document.createElement("div");
  toast.className = "toast";
  toast.role = "status";
  toast.textContent = message;
  document.body.append(toast);
  toastTimer = window.setTimeout(() => {
    toast.classList.add("toast-leaving");
    toast.addEventListener("animationend", () => toast.remove(), { once: true });
    toastTimer = window.setTimeout(() => toast.remove(), 240);
  }, 2560);
}

function localActionError(failure: unknown, fallback: string): string {
  const value = typeof failure === "string" ? failure : failure instanceof Error ? failure.message : "";
  const cleaned = value.replace(/[\u0000-\u001f\u007f]/gu, " ").replace(/\s+/gu, " ").trim();
  return cleaned && cleaned.length <= 240 ? cleaned : fallback;
}

function showBootstrapRecovery(): void {
  renderScheduler.cancel();
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  root.innerHTML = `<div class="app-frame">${desktopTitlebar()}<main class="ui-recovery" role="alert" aria-labelledby="boot-recovery-title"><img src="${oslLogoUrl}" alt=""/><h1 id="boot-recovery-title">Couldn’t open OSL</h1><p>The local security core did not respond.</p><button class="button primary" id="boot-retry">Retry</button></main></div>`;
  bindDesktopTitlebar();
  document.querySelector<HTMLButtonElement>("#boot-retry")?.addEventListener("click", (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    button.disabled = true;
    button.textContent = "Retrying…";
    void bootstrap();
  });
}

function usableBootCore(value: CoreIntegration): boolean {
  if (!isTauriRuntime()) return true;
  const status = value.readiness.bootstrapStatus;
  return value.readiness.originalCoreLinked && status !== "notAttempted" && status !== "inProgress" && status !== "failed";
}

function startReadyWorkspaceLoads(): void {
  if (screenshotProtectionEnabled) {
    void setScreenshotProtection(true).then((applied) => {
      if (applied) return;
      screenshotProtectionEnabled = false;
      localStorage.setItem(screenshotProtectionStorageKey, "false");
      if (route === "settings" && settingsSection === "scrub") render();
      showToast("Windows capture resistance could not be restored");
    });
  }
  if (route === "onboarding") return;
  void loadHubPasswordRoleStatus().then((status) => { passwordRoleStatus = status; if (route === "settings" && settingsSection === "account") renderWhenIdle(); }).catch(() => undefined);
  void refreshUpdateStatus(true);
  void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") renderWhenIdle(); });
  void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") renderWhenIdle(); });
  if (notificationsEnabled) void setNotificationsEnabled(true).then(async (enabled) => {
    appNotifications = enabled ? await loadAppNotifications() : null;
    if (route === "home") renderWhenIdle();
  });
  void refreshIdentitySlots().then(() => { if (route === "settings" && settingsSection === "account") renderWhenIdle(); });
}

async function bootstrap(): Promise<void> {
  const attempt = ++bootstrapEpoch;
  applyTheme(themeChoice);
  loadUiPreferences();
  root.innerHTML = `<div class="app-frame">${desktopTitlebar()}<main class="loading-screen"><img class="osl-logo loading-logo" src="${oslLogoUrl}" alt="OSL Privacy"/><div class="loading-lines" aria-hidden="true"><span></span><span></span></div><span class="sr-only">Opening OSL</span></main></div>`;
  bindDesktopTitlebar();
  const preferencesRequest = withNativeDeadline(loadOnboardingPreferences(), "Load OSL preferences", bootPreferenceDeadlineMs).catch(() => null);
  const servicesRequest = withNativeDeadline(loadLinkedServices(), "Load apps", bootSupportDeadlineMs).catch(() => null);
  const nativeAppsRequest = withNativeDeadline(loadNativeApps(), "Load installed apps", bootSupportDeadlineMs).catch(() => null);
  const browserImportsRequest = withNativeDeadline(loadBrowserImports(), "Load browsers", bootSupportDeadlineMs).catch(() => null);
  const licenseRequest = withNativeDeadline(loadHubLicenseState(), "Load plan", bootSupportDeadlineMs).catch(() => null);
  try {
    const coreIntegration = await withNativeDeadline(loadCoreIntegration(), "Start OSL", bootCoreDeadlineMs);
    if (attempt !== bootstrapEpoch) return;
    if (!usableBootCore(coreIntegration)) {
      showBootstrapRecovery();
      return;
    }
    core = coreIntegration;
    const preferences = await preferencesRequest ?? {
      onboardingComplete: core.readiness.bootstrapStatus === "ready",
      setup: parseSetupState(null),
      showPlaintextPreview: true,
    };
    if (attempt !== bootstrapEpoch) return;
    setup = preferences.setup;
    onboardingComplete = preferences.onboardingComplete;
    if (core.readiness.bootstrapStatus === "setupRequired") {
      onboardingRoute = "welcome";
      route = "onboarding";
    } else if (core.readiness.bootstrapStatus === "passwordRequired") {
      onboardingRoute = "unlock";
      route = "onboarding";
    } else {
      route = preferences.onboardingComplete ? "home" : "onboarding";
    }
  } catch {
    if (attempt === bootstrapEpoch) showBootstrapRecovery();
    return;
  }
  renderNow();
  startReadyWorkspaceLoads();
  void Promise.all([servicesRequest, nativeAppsRequest, browserImportsRequest, licenseRequest]).then(([linkedServices, nativeCatalog, browserCatalog, currentLicenseState]) => {
    if (attempt !== bootstrapEpoch) return;
    if (linkedServices) services = linkedServices;
    if (nativeCatalog) nativeApps = nativeCatalog;
    if (browserCatalog) browserImports = browserCatalog;
    if (currentLicenseState) licenseState = currentLicenseState;
    route === "onboarding" ? render() : renderWhenIdle();
  });
}

window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => { if (themeChoice === "system") applyTheme("system"); });
let nativeHostResizeFrame = 0;
window.addEventListener("resize", () => {
  if (!activeNativeHostId || nativeHostResizeFrame) return;
  nativeHostResizeFrame = requestAnimationFrame(() => {
    nativeHostResizeFrame = 0;
    if (activeNativeHostId) void resizeNativeAppWindow().catch(() => undefined);
  });
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "hidden" || !newIdentityRecoveryPhrase) return;
  newIdentityRecoveryPhrase = null;
  if (route === "settings" && settingsSection === "account") render();
});
window.addEventListener("error", (event) => { event.preventDefault(); containBackgroundFailure(); });
window.addEventListener("unhandledrejection", (event) => { event.preventDefault(); containBackgroundFailure(); });
void bootstrap();
