import "@fontsource-variable/inter/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  bindDesignPreviewNav,
  designPreviewNavMarkup,
  designPreviewProEnabled,
  designPreviewRouteFromHash,
  isDesignPreview,
  type DesignPreviewRoute,
} from "./dev-preview";
import {
  canCompleteSetup,
  formatSendMode,
  needsRiskAcceptance,
  parseSetupState,
  type SendMode,
  type SetupState,
} from "./state";
import { isTauriRuntime, loadOnboardingPreferences, saveOnboardingPreferences } from "./preferences";
import {
  beginBrowserAccountImport,
  escapeHtml,
  closeEmbeddedServiceHost,
  configuredTopStripApps,
  detachNativeAppWindow,
  embeddedAccountsForHomeApp,
  focusNativeAppWindow,
  loadBrowserImports,
  homeAppsFromServices,
  hostNativeAppWindow,
  installFirefox,
  installNativeApp,
  loadFirefoxStatus,
  loadLinkedServices,
  launchFirefoxService,
  loadMullvadStatus,
  loadVpnConnectionDetected,
  loadNativeApps,
  openBrowserImport,
  openEmbeddedHomeApp,
  resizeNativeAppWindow,
  setupEmbeddedHomeApp,
  type EmailProvider,
  type EmbeddedServiceHost,
  type FirefoxStatus,
  type HomeAppCatalogEntry,
  type HomeAppId,
  type LinkedService,
  type NativeApp,
  type NativeAppId,
  type BrowserImportStatus,
  type BrowserAccountImportAction,
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
  unlockHubPasswordGate,
  validateHubActivationCode,
  type CoreIntegration,
  type HubLicenseState,
  type HubPasswordRoleStatus,
} from "./core";
import { checkHubForUpdates, installHubUpdate, openHubReleasesPage, type UpdateStatus } from "./updates";
import { serviceLogo, providerLogo } from "./logos";
import { activateLocalLoopbackContext, activateManualPeerContext, addOslFriend, burnActiveHubContext, burnHubServiceAccount, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubServiceBurnReadiness, isHubPlaintext, listHubIdentities, listHubPeople, loadActiveContextSecurity, loadAppNotifications, loadFriendProfile, openPeerProseText, prepareLocalProtectedText, preparePeerProseText, recoverHubIdentitySlot, saveActiveContextSecurity, setActiveHubFriendPermission, setHubFriendNickname, setLocalProtectedSheetOpen, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, type AppNotification, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type LocalMessageCandidate, type PersistedLocalPrivacyScanResult } from "./adapters";
import { blankLocalProtectedModel, isLocalTtlSeconds, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import { blankPeerProtectedModel, peerProtectedSheetMarkup, type PeerProtectedPane, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import mullvadLogoUrl from "./mullvad-logo.svg?url";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { withNativeDeadline } from "./native-deadline";
import { FrameRenderScheduler } from "./render-scheduler";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubDeletionAllowed, scrubDeletionContract, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { defaultScrubSetupPlan, parseScrubSetupPlan, SCRUB_TARGET_LIMIT, targetId, validateCoverageReceipt, type ScrubCoverageReceipt, type ScrubMode, type ScrubSetupPlan } from "./scrub-plan";
import { persistLocalScrubExport } from "./scrub-local";
import type { ScrubIndexStatus } from "./scrub-index";
import { runAutoScrubBatch, summarizeAutoScrubReceipt, unavailableAutoScrubCapabilities, type AutoScrubCapability, type AutoScrubProviderId } from "./autoscrub-flow";
import { configureScrubImapAccount, createDesktopAutoScrubBridge, prepareScrubImapFindings, type ScrubImapLocator } from "./scrub-imap-ipc";
import type { ProviderDeletionReceipt, ScopePolicy } from "./scrub-delete-engine";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";
import { initializeThemePreference, themeStorageKey, type ThemeChoice } from "./theme-preference";

type Route = "onboarding" | "home" | "service" | "settings";
type OnboardingRoute = "welcome" | "create" | "restore" | "import" | "unlock" | "recovery" | "tutorial" | "detected" | "install" | "mullvad" | "sending" | "passwords" | "burnpass" | "privacy" | "scrub" | "decoy";
type SettingsSection = "account" | "apps" | "scrub" | "cleanup" | "notifications" | "appearance" | "about";
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

function manualSendingAnimationMarkup(mode: SendMode = "clipboard"): string {
  const normalized = mode === "manual" ? "clipboard" : mode;
  const sequence = normalized === "clipboard" ? ["Enter", "Ctrl+V", "Enter"] : normalized === "double" ? ["Enter", "Enter"] : ["Enter"];
  const label = `Press ${sequence.join(", then ")}`;
  const keys = sequence.map((key, index) => `<kbd class="send-demo-key send-demo-key-${index + 1}" aria-hidden="true">${key}</kbd>`).join("");
  return `<span class="send-method-demo send-method-demo-${normalized}" role="img" aria-label="${label}"><span class="send-demo-key-sequence">${keys}</span></span>`;
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
let nativeCatalogBusy = false;
let mullvadBusy = false;
let vpnConnectionDetected = false;
let mullvadPreference: "auto" | "off" | null = null;
let browserImports: BrowserImportStatus[] = [];
let browserAccountImport: BrowserAccountImportAction | null = null;
let browserImportBusy = false;
let browserReadinessBusy = false;
let firefoxStatus: FirefoxStatus = { availability: "unavailable" };
let browserMigrationAwaitingConfirmation = false;
let savedAccountsReady = false;
let savedAccountMode: SavedAccountMode = "ask";
let savedNativeApps = new Set<NativeAppId>();
const backgroundInstallIds = new Set<NativeAppId>();
const selectedFirstInstallApps = new Set<NativeAppId>();
const selectedOnboardingApps = new Set<HomeAppId>();
const detectedAccountChoices = new Map<string, "existing" | "osl">();
let scrubSetupStep: "intro" | "accounts" | "options" = "intro";
const prototypePrivacyChoices = new Set(["hide-notifications", "auto-lock", "disable-previews", "ip-grabber-protection", "external-default-browser"]);
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
let themeChoice: ThemeChoice = initializeThemePreference(localStorage);
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
let peerProtectedSheet: PeerProtectedSheetModel = blankPeerProtectedModel();
let protectedSheetMode: "peer" | "local" = "peer";
let activeProtectedContextKind: "peer" | "local" | null = null;
let onboardingComplete = false;
let screenshotProtectionEnabled = false;
let hubIdentities: HubIdentitySlot[] = [];
let newIdentityRecoveryPhrase: string | null = null;
let hubPeople: HubPerson[] = [];
let privacyScanResult: PersistedLocalPrivacyScanResult | null = null;
let privacyScanFileName: string | null = null;
let privacyScanStatus: ScrubIndexStatus | null = null;
let privacyCoverageReceipt: ScrubCoverageReceipt | null = null;
let privacyScanBusy = false;
let enabledScrubSignals = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
let selectedScrubFindings = new Set<number>();
let scrubResultsPage = 0;
let scrubReviewOpen = false;
let scrubReviewPage = 0;
let autoScrubCapabilities: readonly AutoScrubCapability[] = unavailableAutoScrubCapabilities;
let autoScrubAccountId = "";
let autoScrubPathId: AutoScrubProviderId = "gmail-web";
let autoScrubBusy = false;
let autoScrubDryRunReceipt: ProviderDeletionReceipt | null = null;
let autoScrubExecutionReceipt: ProviderDeletionReceipt | null = null;
let autoScrubError = "";
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
const scrubSetupStorageKey = "osl-hub-scrub-setup-v1";
const serviceGuideStorageKey = "osl-hub-service-guide-v1";
const homeTileOrderStorageKey = "osl-home-tile-order-v1";
const hiddenHomeTilesStorageKey = "osl-home-tile-hidden-v1";
const savedAccountModeStorageKey = "osl-saved-account-mode-v1";
const savedNativeAppsStorageKey = "osl-saved-native-apps-v1";
const savedAccountsReadyStorageKey = "osl-browser-accounts-ready-v1";
const browserImportPendingStorageKey = "osl-browser-import-pending-v1";
const onboardingResumeStorageKey = "osl-onboarding-resume-v1";
const onboardingBranchStorageKey = "osl-onboarding-branch-v1";
const experimentalSendConsentStorageKey = "osl-experimental-send-consent-v1";
const supportedNativeAppIds = new Set<NativeAppId>(["discord", "telegram", "signal", "whatsapp"]);
const importedFirefoxHomeAppIds = new Set<HomeAppId>([
  "instagram", "snapchat", "x", "messenger", "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom",
]);
const friendsDialogPageSize = 24;
const friendScopeRenderLimit = 16;
const scrubResultsPageSize = 50;
const scrubReviewPageSize = 20;
const bootCoreDeadlineMs = 4_000;
const bootPreferenceDeadlineMs = 1_500;
const bootSupportDeadlineMs = 2_000;
const nativeCatalogDecisionDeadlineMs = 8_000;
let scrubSetupPlan: ScrubSetupPlan = parseScrubSetupPlan(
  localStorage.getItem(scrubSetupStorageKey) ?? JSON.stringify(defaultScrubSetupPlan),
  new Set(scrubAccountSelections().map(({ id }) => id)),
  [...enabledScrubSignals],
  licenseState.access === "pro" || licenseState.access === "offlineGrace",
);

type OnboardingBranch = {
  detected: boolean;
  install: boolean;
};

function loadOnboardingBranch(): OnboardingBranch {
  try {
    const parsed = JSON.parse(localStorage.getItem(onboardingBranchStorageKey) ?? "null") as Partial<OnboardingBranch> | null;
    return { detected: parsed?.detected === true, install: parsed?.install === true };
  } catch {
    return { detected: false, install: false };
  }
}

let onboardingBranch = loadOnboardingBranch();

function experimentalSendConsentId(mode: SendMode, serviceId: string, accountId: string): string {
  return `${mode}:${serviceId}:${accountId}`;
}

function loadExperimentalSendConsents(): Set<string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(experimentalSendConsentStorageKey) ?? "[]") as unknown;
    return Array.isArray(parsed) && parsed.every((item) => typeof item === "string")
      ? new Set(parsed.filter((item) => item.length <= 256).slice(0, 100))
      : new Set();
  } catch {
    return new Set();
  }
}

function hasExperimentalSendConsent(mode: SendMode, serviceId: string, accountId: string): boolean {
  return loadExperimentalSendConsents().has(experimentalSendConsentId(mode, serviceId, accountId));
}

function rememberExperimentalSendConsent(mode: SendMode, serviceId: string, accountId: string): void {
  const consents = loadExperimentalSendConsents();
  consents.add(experimentalSendConsentId(mode, serviceId, accountId));
  localStorage.setItem(experimentalSendConsentStorageKey, JSON.stringify([...consents].slice(-100)));
}

function parseTheme(raw: string | null): ThemeChoice {
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "dark";
}

function parseSavedAccountMode(raw: string | null): SavedAccountMode {
  return raw === "use" || raw === "clean" ? raw : "ask";
}

function pendingOnboardingRoute(): OnboardingRoute | null {
  const pending = localStorage.getItem(onboardingResumeStorageKey) as OnboardingRoute | null;
  return pending && ["tutorial", "import", "detected", "install", "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub"].includes(pending)
    ? pending
    : null;
}

function persistCurrentOnboardingRoute(): void {
  if (["tutorial", "import", "detected", "install", "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub"].includes(onboardingRoute)) {
    localStorage.setItem(onboardingResumeStorageKey, onboardingRoute);
  }
}

function markServiceOnboardingOpened(): void {
  if (!onboardingServiceSetup) return;
  localStorage.setItem(onboardingResumeStorageKey, "import");
}

function clearServiceOnboardingResume(): void {
  onboardingServiceSetup = false;
  localStorage.removeItem(onboardingResumeStorageKey);
}

function persistOnboardingBranch(): void {
  localStorage.setItem(onboardingBranchStorageKey, JSON.stringify(onboardingBranch));
}

function resetOnboardingBranch(): void {
  onboardingBranch = { detected: false, install: false };
  localStorage.removeItem(onboardingBranchStorageKey);
}

function markOnboardingBranch(route: OnboardingRoute): void {
  if (route === "detected") onboardingBranch.detected = true;
  if (route === "install") onboardingBranch.install = true;
  persistOnboardingBranch();
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
  savedAccountsReady = false;
  notificationsEnabled = localStorage.getItem(notificationsStorageKey) === "true";
  notificationPreviewContent = localStorage.getItem(notificationPreviewStorageKey) === "true";
  notificationScopeSuggestions = localStorage.getItem(notificationScopeStorageKey) !== "false";
  screenshotProtectionEnabled = localStorage.getItem(screenshotProtectionStorageKey) === "true";
  enabledScrubSignals = parseScrubSignalGroups(localStorage.getItem(scrubSignalsStorageKey));
}

function activeBrowserAccountsReadyStorageKey(): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${savedAccountsReadyStorageKey}:${encodeURIComponent(owner)}` : null;
}

function activeBrowserImportPendingStorageKey(): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${browserImportPendingStorageKey}:${encodeURIComponent(owner)}` : null;
}

function refreshActiveBrowserAccountsReady(): void {
  const key = activeBrowserAccountsReadyStorageKey();
  savedAccountsReady = key !== null && localStorage.getItem(key) === "true";
  const pendingKey = activeBrowserImportPendingStorageKey();
  browserMigrationAwaitingConfirmation = !savedAccountsReady && pendingKey !== null && localStorage.getItem(pendingKey) === "true";
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
    refreshActiveBrowserAccountsReady();
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
      const view = document.querySelector<HTMLElement>(".content-viewport, .onboarding-panel");
      if (route === "settings" || route === "service") view?.classList.add("tool-enter");
      else view?.classList.add("view-enter");
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
  if (isDesignPreview) return;
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
  persistCurrentOnboardingRoute();
  const setupScreen = ["tutorial", "import", "detected", "install", "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub"].includes(onboardingRoute);
  const setupNavigation = setupScreen
    ? '<button class="onboarding-back-dock" id="onboarding-back" type="button">Back</button><button class="onboarding-skip-dock" id="skip-onboarding" type="button">Skip · manual setup</button>'
    : "";
  const markup = `<div class="app-frame">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}</main>${setupNavigation}</div>${scrubReviewDialogMarkup()}${designPreviewNavMarkup(onboardingRoute)}</div>`;
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  root.innerHTML = markup;
  bindOnboarding();
  bindDesignPreviewNav(onboardingRoute, applyDesignPreviewRoute);
  openScrubReviewDialogAfterRender();
}

function onboardingContent(): string {
  if (onboardingRoute === "welcome") {
    const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired";
    const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired;
    const primaryRoute: OnboardingRoute = partialIdentity ? "create" : returning ? "unlock" : "create";
    const primaryLabel = partialIdentity ? "Finish setup" : returning ? "Unlock this device" : "Create account";
    return `<section class="signin-card" aria-labelledby="route-heading">
      <img class="osl-logo signin-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/>
      <h1 id="route-heading" tabindex="-1">${partialIdentity ? "Finish your account" : returning ? "Sign in" : "Create your OSL account"}</h1>
      <button class="button primary signin-primary" data-onboarding="${primaryRoute}">${primaryLabel}</button>
      <button class="signin-link" data-onboarding="restore">Use a recovery phrase</button>
      ${returning ? `<div class="signin-divider" aria-hidden="true"><span></span></div><p class="signin-new">New to OSL?</p><button class="button signin-create" data-onboarding="create">Create account</button>` : ""}
    </section>`;
  }

  if (onboardingRoute === "create") return identityPasswordForm("Create a password", "Create account", "setup");
  if (onboardingRoute === "unlock") return identityPasswordForm("Unlock OSL", "Unlock", "unlock");
  if (onboardingRoute === "restore") return importIdentityForm();
  if (onboardingRoute === "recovery") return recoveryContent();
  if (onboardingRoute === "tutorial") return tutorialContent();
  if (onboardingRoute === "detected") return detectedAppsContent();
  if (onboardingRoute === "install") return installMissingAppsContent();
  if (onboardingRoute === "import") return browserImportContent();
  if (onboardingRoute === "mullvad") return mullvadSetupContent();
  if (onboardingRoute === "passwords") return onboardingPasswordRoleContent("stealth");
  if (onboardingRoute === "burnpass") return onboardingPasswordRoleContent("burn");
  if (onboardingRoute === "privacy") return onboardingPrivacyContent();
  if (onboardingRoute === "scrub") return scrubSetupContent();
  if (onboardingRoute === "decoy") return `<section class="decoy-workspace" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Workspace</h1><p>No recent items.</p><button class="button ghost" id="close-decoy" type="button">Close</button></section>`;

  return sendingSetupContent();
}

function tutorialContent(): string {
  const apps = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available");
  const choices = apps.length
    ? `<div class="onboarding-app-grid onboarding-app-choices" role="group" aria-label="Services to set up">${apps.map((app) => `<button type="button" class="onboarding-app ${selectedOnboardingApps.has(app.id) ? "selected" : ""}" data-onboarding-app-choice="${app.id}" aria-pressed="${selectedOnboardingApps.has(app.id)}"><span class="app-logo-plate">${homeAppLogo(app)}</span><strong>${escapeHtml(app.displayName)}</strong></button>`).join("")}</div>`
    : `<div class="empty-state"><strong>No apps are available</strong><p>You can continue and add apps later from Home.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Choose your apps</h1><p class="compact-lead onboarding-centered-copy">Pick the services you want available in OSL. This does not sign in or discover accounts.</p>${choices}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-app-choice" type="button" ${selectedOnboardingApps.size && !nativeCatalogBusy ? "" : "disabled"}>${nativeCatalogBusy ? "Checking Windows…" : "Continue"}</button></div>`;
}

function selectedNativeApps(): NativeApp[] {
  return nativeApps.filter((app) => selectedOnboardingApps.has(app.id));
}

function hasSelectedNativeAppChoice(): boolean {
  return [...selectedOnboardingApps].some((appId) => supportedNativeAppIds.has(appId as NativeAppId));
}

function isCompleteNativeCatalog(catalog: NativeApp[]): boolean {
  const ids = new Set(catalog.map((app) => app.id));
  return catalog.length === supportedNativeAppIds.size
    && ids.size === supportedNativeAppIds.size
    && [...supportedNativeAppIds].every((appId) => ids.has(appId));
}

function hasSelectedInstalledNativeApps(): boolean {
  return selectedNativeApps().some((app) => app.availability === "installed" && app.isolatedProfileAvailable);
}

function hasSelectedMissingNativeApps(): boolean {
  return selectedNativeApps().some((app) => app.availability !== "installed");
}

function routeAfterAppChoice(): OnboardingRoute {
  return "import";
}

async function ensureNativeCatalogForAppChoice(): Promise<boolean> {
  if (!hasSelectedNativeAppChoice()) return true;
  if (nativeCatalogBusy) return false;
  nativeCatalogBusy = true;
  renderNow();
  try {
    const catalog = await withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs);
    if (!isCompleteNativeCatalog(catalog)) {
      showToast("Couldn’t check Windows apps. Try again.");
      return false;
    }
    nativeApps = catalog;
    return true;
  } catch {
    showToast("Couldn’t check Windows apps. Try again.");
    return false;
  } finally {
    nativeCatalogBusy = false;
    render();
  }
}

function selectedInstalledNativeApp(appId: HomeAppId): NativeApp | undefined {
  const nativeId = appId as NativeAppId;
  if (savedAccountMode !== "use" || !supportedNativeAppIds.has(nativeId) || !savedNativeApps.has(nativeId)) return undefined;
  return nativeApps.find((candidate) => candidate.id === nativeId && candidate.availability === "installed" && candidate.isolatedProfileAvailable);
}

function detectedAppsContent(): string {
  const installed = selectedNativeApps().filter((app) => app.availability === "installed" && app.isolatedProfileAvailable);
  const installedIds = new Set(installed.map((app) => app.id));
  const accounts = services.flatMap((service) => service.accounts.map((account) => ({ service, account })))
    .filter(({ service }) => selectedOnboardingApps.size === 0 || selectedOnboardingApps.has(service.id as HomeAppId) || service.id === "email");
  const rows = accounts.length
    ? accounts.map(({ service, account }) => {
      const id = targetId(service.id, account.id);
      const choice = detectedAccountChoices.get(id) ?? "existing";
      const source = account.provider || !installedIds.has(service.id as NativeAppId) ? "Browser import" : "Installed app";
      return `<article class="detected-account-row detected-account-${choice}" data-detected-account-row="${escapeHtml(id)}"><span class="detected-account-logo service-brand-badge" data-service-brand="${service.id}">${serviceLogo(service.id)}</span><span class="detected-account-name"><strong>${escapeHtml(service.displayName)}</strong><small>${escapeHtml(account.label)} · ${escapeHtml(account.displayHandle ?? source)}</small><em>${source}</em></span><label><span class="sr-only">How to use ${escapeHtml(account.label)} on ${escapeHtml(service.displayName)}</span><select data-detected-account="${escapeHtml(id)}" aria-label="How to use ${escapeHtml(account.label)} on ${escapeHtml(service.displayName)}"><option value="existing" ${choice === "existing" ? "selected" : ""}>Use existing detected account</option><option value="osl" ${choice === "osl" ? "selected" : ""}>Create OSL-specific account</option></select></label></article>`;
    }).join("")
    : `<div class="empty-state"><strong>No accounts detected</strong><p>You can create OSL-specific accounts later.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Detected services</h1><div class="detected-launch-mode"><label for="detected-launch-select">Open services with</label><select id="detected-launch-select"><option value="use" ${savedAccountMode !== "clean" ? "selected" : ""}>Installed apps when available</option><option value="clean" ${savedAccountMode === "clean" ? "selected" : ""}>Isolated web profiles</option></select></div><div class="detected-account-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-detected-apps" type="button">Continue</button></div>`;
}

function installMissingAppsContent(): string {
  const missing = selectedNativeApps().filter((app) => app.availability !== "installed");
  const rows = missing.length
    ? missing.map((app) => app.availability === "installable"
      ? `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Optional Windows install</small></span></span><input type="checkbox" data-first-install="${app.id}" ${selectedFirstInstallApps.has(app.id) ? "checked" : ""}/></label>`
      : `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Install unavailable on this PC</small></span></span></div>`).join("")
    : `<div class="empty-state"><strong>No missing desktop apps</strong><p>Your selected desktop apps are already installed, or use the web.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Install missing apps</h1><div class="setup-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-install-apps" type="button">Continue</button></div>`;
}

function browserImportContent(): string {
  const installed = browserImports.filter((browser) => browser.installed);
  const names = installed.map((browser) => escapeHtml(browser.displayName)).join(", ");
  const advancedButtons = installed.map((browser) => `<button class="button compact" type="button" data-browser-import="${browser.id}">Prepare export in ${escapeHtml(browser.displayName)}</button>`).join("");
  const chromeNote = installed.some((browser) => browser.id === "chrome")
    ? `<p class="saved-account-truth"><strong>Chrome on Windows:</strong> app-bound encryption prevents a silent copy. Firefox will guide a browser-owned CSV export, Windows confirmation, and explicit file selection. Delete that plaintext CSV when Firefox finishes.</p>`
    : "";
  const ready = savedAccountsReady
    ? `<div class="saved-account-browser-note"><strong>Firefox import marked complete</strong><small>You confirmed the browser-owned import finished. Firefox may offer imported saved logins on supported sites. MFA and CAPTCHA still apply; OSL never receives your passwords.</small></div>`
    : "";
  const selectedRoute = browserAccountImport
    ? `<p class="saved-account-truth">Recommended source: ${escapeHtml(browserAccountImport.preferredSource)}. Firefox keeps the final source and confirmation visible among ${browserAccountImport.detectedSources.length} detected browser${browserAccountImport.detectedSources.length === 1 ? "" : "s"}.</p>`
    : "";
  const buttonLabel = browserReadinessBusy
    ? "Checking browsers…"
    : browserImportBusy
    ? firefoxStatus.availability === "installed" ? "Opening Firefox…" : "Preparing Firefox…"
    : browserMigrationAwaitingConfirmation
      ? "I finished the Firefox import"
      : savedAccountsReady
        ? "Import marked complete"
        : "Import saved accounts";
  const firefoxNote = !browserReadinessBusy && firefoxStatus.availability === "unavailable"
    ? `<p class="saved-account-truth">Firefox is required for this visible import and could not be installed automatically on this PC.</p>`
    : "";
  const disabled = browserReadinessBusy || browserImportBusy || savedAccountsReady || installed.length === 0 || firefoxStatus.availability === "unavailable";
  return `<h1 id="route-heading" tabindex="-1">Import browser accounts</h1><p class="compact-lead onboarding-centered-copy">${installed.length ? `Found ${names}.` : "No supported browser was found."} Import stays in Firefox's visible, browser-owned flow.</p><section class="saved-account-browser-note"><strong>Local-only consent</strong><small>Firefox performs the migration into this OSL identity's isolated Firefox profile. Supported web apps then open there in a separate Firefox window. Firefox windows are outside OSL capture resistance. OSL does not scrape or decrypt browser databases, discover accounts, receive passwords, or upload account data.</small></section>${ready}${selectedRoute}${firefoxNote}<button class="button" id="import-saved-accounts" type="button" ${disabled ? "disabled" : ""}>${buttonLabel}</button>${browserMigrationAwaitingConfirmation && !savedAccountsReady ? `<p class="saved-account-truth">Finish the temporary Firefox wizard, return to OSL, then confirm with the same button.</p>` : ""}${chromeNote}<details class="saved-account-advanced"><summary>Advanced browser export</summary><p>Use these only if Firefox asks for a manual export.</p><div class="browser-import-actions">${advancedButtons || "No browser export shortcut is available."}</div></details><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-browser-import" type="button" ${browserImportBusy ? "disabled" : ""}>Continue</button></div>`;
}

function persistSavedAccountPreferences(): void {
  localStorage.setItem(savedAccountModeStorageKey, savedAccountMode);
  localStorage.setItem(savedNativeAppsStorageKey, JSON.stringify([...savedNativeApps]));
}

function bindSavedAccountControls(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-saved-account-mode]").forEach((button) => button.addEventListener("click", () => {
    savedAccountMode = parseSavedAccountMode(button.dataset.savedAccountMode ?? null);
    if (savedAccountMode === "use" && savedNativeApps.size === 0) {
      savedNativeApps = new Set(nativeApps.filter((app) => app.availability === "installed" && app.isolatedProfileAvailable).map((app) => app.id));
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
  document.querySelectorAll<HTMLButtonElement>("[data-background-install]").forEach((button) => button.addEventListener("click", () => {
    void startBackgroundInstall(button.dataset.backgroundInstall as NativeAppId);
  }));
}

function bindBrowserImportControls(): void {
  document.querySelector<HTMLButtonElement>("#import-saved-accounts")?.addEventListener("click", async () => {
    if (browserImportBusy || savedAccountsReady) return;
    if (browserMigrationAwaitingConfirmation) {
      savedAccountsReady = true;
      browserMigrationAwaitingConfirmation = false;
      const readyKey = activeBrowserAccountsReadyStorageKey();
      const pendingKey = activeBrowserImportPendingStorageKey();
      if (!readyKey) throw new Error("Unlock an OSL identity before finishing browser import");
      localStorage.setItem(readyKey, "true");
      if (pendingKey) localStorage.removeItem(pendingKey);
      showToast("Saved accounts ready in OSL Firefox");
      onboardingRoute = "detected";
      render();
      void refreshMullvadSetup();
      return;
    }
    browserImportBusy = true;
    render();
    try {
      if (firefoxStatus.availability === "installable") {
        await withNativeDeadline(installFirefox(), "Start Firefox install");
        for (let attempt = 0; attempt < 40; attempt += 1) {
          await new Promise<void>((resolve) => window.setTimeout(resolve, 3_000));
          if (route !== "onboarding" || onboardingRoute !== "import") return;
          firefoxStatus = await loadFirefoxStatus().catch(() => firefoxStatus);
          if (firefoxStatus.availability === "installed") break;
        }
        if (firefoxStatus.availability !== "installed") {
          throw new Error("Firefox is still installing in Windows");
        }
      }
      if (firefoxStatus.availability !== "installed") {
        throw new Error("Firefox is unavailable on this PC");
      }
      if (route !== "onboarding" || onboardingRoute !== "import") return;
      browserAccountImport = await withNativeDeadline(beginBrowserAccountImport(), "Open Firefox account migration", 5_000);
      const pendingKey = activeBrowserImportPendingStorageKey();
      if (!pendingKey) throw new Error("Unlock an OSL identity before starting browser import");
      localStorage.setItem(pendingKey, "true");
      browserMigrationAwaitingConfirmation = true;
      showToast(browserAccountImport.manualExportRequired
        ? "Firefox opened — follow its browser-owned export and import steps"
        : "Temporary Firefox migration window opened");
    } catch (failure) {
      showToast(localActionError(failure, "Saved-account migration could not open"));
    } finally {
      browserImportBusy = false;
      render();
    }
  });
  document.querySelectorAll<HTMLButtonElement>("[data-browser-import]").forEach((button) => button.addEventListener("click", async () => {
    const browserId = button.dataset.browserImport as BrowserImportStatus["id"];
    button.disabled = true;
    try {
      await openBrowserImport(browserId);
      showToast("Browser password manager opened — approve export there");
    } catch (failure) {
      showToast(localActionError(failure, "Browser password manager could not open"));
    } finally {
      button.disabled = false;
    }
  }));
  document.querySelector<HTMLButtonElement>("#continue-browser-import")?.addEventListener("click", () => {
    const pendingKey = activeBrowserImportPendingStorageKey();
    if (pendingKey) localStorage.removeItem(pendingKey);
    browserMigrationAwaitingConfirmation = false;
    browserAccountImport = null;
    onboardingRoute = "detected";
    render();
    void refreshMullvadSetup();
  });
}

async function refreshBrowserImportReadiness(): Promise<void> {
  if (browserReadinessBusy) return;
  browserReadinessBusy = true;
  if (route === "onboarding" && onboardingRoute === "import") render();
  const [catalog, currentFirefoxStatus] = await Promise.all([
    withNativeDeadline(loadBrowserImports(), "Refresh browsers", nativeCatalogDecisionDeadlineMs).catch(() => null),
    withNativeDeadline(loadFirefoxStatus(), "Refresh Firefox", nativeCatalogDecisionDeadlineMs).catch(() => null),
  ]);
  try {
    if (catalog) browserImports = catalog;
    if (currentFirefoxStatus) firefoxStatus = currentFirefoxStatus;
    if (!catalog && !currentFirefoxStatus) throw new Error("browser readiness unavailable");
  } catch {
    showToast("Couldn’t check browser import. Try again.");
  } finally {
    browserReadinessBusy = false;
    if (route === "onboarding" && onboardingRoute === "import") render();
  }
}

function importIdentityForm(): string {
  return `<h1 id="route-heading" tabindex="-1">Restore your account</h1><form class="setup-surface password-form" id="identity-import-form" novalidate><label for="identity-recovery-phrase">Recovery phrase</label><textarea id="identity-recovery-phrase" rows="3" autocomplete="off" autocapitalize="none" spellcheck="false" required aria-describedby="import-error"></textarea><label for="import-password">New password</label><div class="password-input-row"><input id="import-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password" aria-controls="import-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small>6 minimum. 12+ suggested.</small><label for="import-password-confirm">Confirm password</label><div class="password-input-row"><input id="import-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password-confirm" aria-controls="import-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="import-error" role="alert"></p><button class="button primary" id="identity-import-submit" type="submit" disabled>Restore</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

function recoveryContent(): string {
  if (!recoveryBundle) return `<h1 id="route-heading" tabindex="-1">Recovery phrases</h1><button class="button primary" data-onboarding="tutorial">Continue</button>`;
  const accountRecovery = recoveryBundle.identityPhrase ? `<code>${escapeHtml(recoveryBundle.identityPhrase)}</code>` : `<p>Keep using the account recovery phrase you imported.</p>`;
  return `<h1 id="route-heading" tabindex="-1">Save your recovery phrases</h1><section class="recovery-phrases"><article><strong>Account recovery phrase</strong>${accountRecovery}</article><article><strong>Password recovery phrase</strong><code>${escapeHtml(recoveryBundle.passwordPhrase)}</code></article></section><div class="setup-footer onboarding-actions"><button class="button primary" id="recovery-continue">Continue</button></div>`;
}

function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  if (!setup) return `<section class="unlock-card" aria-labelledby="route-heading"><div class="unlock-logo-stage" aria-hidden="true"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><h1 id="route-heading" tabindex="-1">Enter your password</h1><form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>Unlock</button></form><button class="text-back" data-onboarding="welcome">← Back</button></section>`;
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><form class="setup-surface password-form" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>${action}</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

function sendingSetupContent(): string {
  const selectedMode: SendMode = setup.sendMode === "manual" ? "clipboard" : setup.sendMode;
  const option = (mode: SendMode, title: string, tone: "safe" | "caution" | "danger", badge = "", disclaimer = "") => `<div class="send-choice send-choice-${tone} ${selectedMode === mode ? "selected" : ""}"><button type="button" data-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${title}</strong>${badge ? `<small class="send-mode-badge">${badge}</small>` : ""}</span>${manualSendingAnimationMarkup(mode)}</button>${disclaimer ? `<small class="send-choice-warning">${disclaimer}</small>` : ""}</div>`;
  const risk = needsRiskAcceptance(selectedMode)
    ? `<label class="send-risk"><input id="accept-send-risk" type="checkbox" ${setup.acceptedRisk && setup.acceptedRiskForMode === selectedMode ? "checked" : ""}/><span><strong>I understand the risk</strong></span></label>`
    : "";
  return `<h1 id="route-heading" tabindex="-1">Choose how to send</h1><div class="send-choice-grid">${option("clipboard", "Copy", "safe", "Safest")}${option("double", "Double Enter", "caution", "", "Can possibly break ToS")}${option("single", "Single Enter", "danger", "", "Breaks some ToS - risky")}</div>${risk}<div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding" ${canCompleteSetup({ ...setup, sendMode: selectedMode }) ? "" : "disabled"}>Continue</button></div>`;
}

function onboardingPasswordRoleContent(role: "stealth" | "burn"): string {
  const stealth = role === "stealth";
  const configured = stealth ? passwordRoleStatus?.stealthPasswordSet : passwordRoleStatus?.burnPasswordSet;
  const title = stealth ? "Stealth password" : "Burn password";
  const detail = stealth ? "Opens an empty workspace without loading your private data." : "Erases OSL data from this device when entered at sign in.";
  const next = stealth ? "burnpass" : "privacy";
  if (configured) {
    return `<h1 id="route-heading" tabindex="-1">${title}</h1><div class="password-role-ready"><span class="status-tag">Set</span><p>${detail}</p></div><div class="setup-footer onboarding-actions"><button class="button primary" data-password-role-next="${next}" type="button">Continue</button></div>`;
  }
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><p class="compact-lead onboarding-centered-copy">${detail}</p><form class="setup-surface password-form onboarding-role-form" data-onboarding-password-role="${role}" data-password-role-next="${next}" novalidate><label for="setup-${role}-current">Current password</label><div class="password-input-row"><input id="setup-${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div><label for="setup-${role}-alternate">New ${stealth ? "stealth" : "burn"} password</label><div class="password-input-row"><input id="setup-${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div><label for="setup-${role}-confirm">Confirm</label><div class="password-input-row"><input id="setup-${role}-confirm" name="confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-confirm" aria-label="Show password confirmation">${passwordEyeIcon()}</button></div><p class="unlock-error" data-onboarding-role-error role="alert"></p><button class="button primary" type="submit" disabled>Set password</button></form>`;
}

function onboardingPrivacyContent(): string {
  const prototypeToggle = (id: string, title: string, detail: string) => `<label class="setup-status-row interactive"><span><strong>${title}</strong><small>${detail}</small></span><input type="checkbox" data-prototype-privacy="${id}" ${prototypePrivacyChoices.has(id) ? "checked" : ""}/></label>`;
  // Real wiring requirement: route every external link click to the OS default browser, never an OSL headless or embedded webview.
  const linkProtection = `${prototypeToggle("ip-grabber-protection", "IP-grabber protection", "Strip and block IP-logging links.")}${prototypeToggle("external-default-browser", "Open links in your default browser", "Never open links inside OSL.")}`;
  return `<h1 id="route-heading" tabindex="-1">Privacy</h1><section class="privacy-toggle-group"><h2>On screen</h2><div class="setup-list"><label class="setup-status-row interactive"><span><strong>Windows capture resistance</strong><small>Ask Windows to exclude OSL from ordinary screen capture.</small></span><input id="onboarding-screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label>${prototypeToggle("screenshot-protection", "Screenshot protection", "Hide sensitive OSL surfaces during capture.")}${prototypeToggle("hide-notifications", "Hide notification content", "Show the app name, not the message.")}${prototypeToggle("disable-previews", "Disable link and URL previews", "Do not fetch preview cards.")}</div></section><section class="privacy-toggle-group"><h2>Links</h2><div class="setup-list">${linkProtection}</div></section><section class="privacy-toggle-group"><h2>When away</h2><div class="setup-list">${prototypeToggle("auto-lock", "Auto-lock on idle", "Lock OSL after five minutes.")}${prototypeToggle("clear-clipboard", "Clear copied messages", "Clear protected clipboard content after one minute.")}</div></section><section class="decrypt-display-note"><strong>Decrypt display</strong><span>Set per protected chat after setup.</span></section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-onboarding-privacy" type="button">Continue</button></div>`;
}

function mullvadSetupContent(): string {
  const choice = (value: "auto" | "off", title: string) => `<button class="mullvad-choice ${mullvadPreference === value ? "selected" : ""}" type="button" data-mullvad-choice="${value}" aria-pressed="${mullvadPreference === value}">${title}</button>`;
  const mullvadMark = `<div class="mullvad-mark" aria-hidden="true"><img src="${mullvadLogoUrl}" alt="" /></div>`;
  return `<section class="mullvad-setup" aria-labelledby="route-heading">${mullvadMark}<h1 id="route-heading" tabindex="-1">Mullvad Recommended</h1><p>configure on startup</p><div class="mullvad-choice-list">${choice("auto", "Auto-connect to Mullvad when OSL opens")}${choice("off", "Don't do that")}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-mullvad" type="button" ${mullvadPreference ? "" : "disabled"}>Continue</button></div></section>`;
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function scrubSetupContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const targets = scrubAccountSelections();
  const option = (mode: ScrubMode, title: string, detail: string, badge = "", unavailable = false) => `<button class="send-mode-option ${scrubSetupPlan.mode === mode ? "selected" : ""} ${unavailable ? "disabled" : ""}" type="button" data-scrub-mode="${mode}" aria-pressed="${scrubSetupPlan.mode === mode}" ${unavailable ? 'aria-disabled="true"' : ""}><span><strong>${title}</strong>${badge ? `<small class="send-mode-badge">${badge}</small>` : ""}</span><small>${detail}</small></button>`;
  const accountCards = targets.length
    ? `<div class="scrub-account-grid">${targets.map(({ id, serviceId, service, account }) => `<button class="scrub-account-choice ${scrubSetupPlan.targetIds.includes(id) ? "selected" : ""}" type="button" data-scrub-target="${escapeHtml(id)}" aria-label="${escapeHtml(service)} account ${escapeHtml(account)}" aria-pressed="${scrubSetupPlan.targetIds.includes(id)}"><span class="scrub-account-logo service-brand-badge" data-service-brand="${serviceId}" aria-hidden="true">${serviceLogo(serviceId)}</span><strong>${escapeHtml(account)}</strong></button>`).join("")}</div>`
    : `<p class="compact-lead onboarding-centered-copy">No detected accounts yet.</p>`;
  if (scrubSetupStep === "intro") {
    return `<section class="scrub-intro"><div class="scrub-hero" aria-hidden="true"><span class="scrub-hero-card"><i></i><i></i><i></i><b></b></span><span class="scrub-hero-sweep"></span></div><h1 id="route-heading" tabindex="-1">Scrub</h1><div class="scrub-intro-actions"><button class="button" id="skip-scrub-setup" type="button">Finish setup</button><button class="button primary" id="start-scrub-setup" type="button">Do Scrub</button></div></section>`;
  }
  if (scrubSetupStep === "accounts") {
    return `<h1 id="route-heading" tabindex="-1">Choose accounts</h1>${targets.length ? `<div class="scrub-selection-controls"><button class="text-button" id="select-all-scrub" type="button">Select all ${targets.length}</button><button class="text-button" id="clear-scrub-selection" type="button">Clear</button></div>` : ""}${accountCards}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-scrub-accounts" type="button">Continue</button></div>`;
  }
  return `<h1 id="route-heading" tabindex="-1">Configure Scrub</h1><h2 class="setup-section-heading">Mode</h2><div class="send-mode-list">${option("scrub", "Scrub", "Review matches before removing anything.", "Recommended")}${option("autoscrub", "AutoScrub", proActive ? "Use your saved plan automatically." : "Requires Pro; Scrub will be used instead.", "Pro", !proActive)}</div><h2 class="setup-section-heading">Categories</h2>${scrubCategoryChooserMarkup(true)}<p class="scrub-config-safety"><strong>Review before removing.</strong> Nothing is deleted without explicit confirmation. This device only.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-scrub-setup">Save &amp; finish</button></div>`;
}

function scrubAccountSelections(): Array<{ id: string; serviceId: ServiceId; service: string; account: string }> {
  const servicePattern = /^[a-z0-9_-]{1,32}$/u;
  const accountPattern = /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u;
  return services.flatMap((service) => service.accounts.flatMap((account) => {
    if (!servicePattern.test(service.id) || !accountPattern.test(account.id)) return [];
    return [{ id: targetId(service.id, account.id), serviceId: service.id, service: service.displayName, account: account.displayHandle || account.label }];
  })).slice(0, SCRUB_TARGET_LIMIT);
}

function loadScrubSetupPlan(): void {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  scrubSetupPlan = parseScrubSetupPlan(localStorage.getItem(scrubSetupStorageKey), new Set(scrubAccountSelections().map(({ id }) => id)), [...enabledScrubSignals], proActive);
}

function persistScrubSetupPlan(): void {
  localStorage.setItem(scrubSetupStorageKey, JSON.stringify(scrubSetupPlan));
}

function prepareDesignPreviewRoute(next: DesignPreviewRoute, proEnabled: boolean): void {
  if (!isDesignPreview) return;
  route = "onboarding";
  onboardingRoute = next;
  licenseState = proEnabled
    ? { access: "pro", status: "ACTIVE", currentPeriodEnd: 2_000_000_000, lastValidatedAt: 1_900_000_000 }
    : structuredClone(unconfiguredLicenseState);
  if (selectedOnboardingApps.size === 0) (["discord", "telegram", "signal"] as HomeAppId[]).forEach((appId) => selectedOnboardingApps.add(appId));
  savedAccountMode = "use";
  savedNativeApps = new Set<NativeAppId>(["discord", "signal"]);
  nativeApps = [
    { id: "discord", displayName: "Discord", availability: "installed", isolatedProfileAvailable: true, supportsOverlay: true },
    { id: "telegram", displayName: "Telegram", availability: "installable", isolatedProfileAvailable: true, supportsOverlay: false },
    { id: "signal", displayName: "Signal", availability: "installed", isolatedProfileAvailable: true, supportsOverlay: true },
    { id: "whatsapp", displayName: "WhatsApp", availability: "installable", isolatedProfileAvailable: false, supportsOverlay: false },
  ];
  recoveryBundle ??= {
    userId: "osl_preview_designer",
    identityPhrase: "amber birch canyon drift ember fern harbor ivory juniper kindle lunar meadow",
    passwordPhrase: "canvas copper ember harbor iris maple orbit pebble quiet river silver willow",
  };
  loadScrubSetupPlan();
}

function applyDesignPreviewRoute(next: DesignPreviewRoute, proEnabled: boolean): void {
  prepareDesignPreviewRoute(next, proEnabled);
  render();
  if (next === "import") void refreshBrowserImportReadiness();
  if (next === "mullvad") void refreshMullvadSetup();
}

function previousSetupRoute(current: OnboardingRoute): OnboardingRoute {
  const routes: Partial<Record<OnboardingRoute, OnboardingRoute>> = {
    tutorial: "recovery",
    import: "tutorial",
    detected: "import",
    install: "detected",
    mullvad: onboardingBranch.install ? "install" : "detected",
    sending: "mullvad",
    passwords: "sending",
    burnpass: "passwords",
    privacy: "burnpass",
    scrub: "privacy",
  };
  return routes[current] ?? "welcome";
}

function bindOnboarding(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboarding as OnboardingRoute; render(); }));
  bindSavedAccountControls();
  bindBrowserImportControls();
  bindPasswordVisibility();
  bindPasswordForm();
  bindImportForm();
  const recoveryContinue = document.querySelector<HTMLButtonElement>("#recovery-continue");
  recoveryContinue?.addEventListener("click", () => { recoveryBundle = null; resetOnboardingBranch(); onboardingRoute = "tutorial"; render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app-choice]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.onboardingAppChoice as HomeAppId;
    if (selectedOnboardingApps.has(appId)) selectedOnboardingApps.delete(appId);
    else selectedOnboardingApps.add(appId);
    render();
  }));
  document.querySelector<HTMLButtonElement>("#continue-app-choice")?.addEventListener("click", async () => {
    if (!await ensureNativeCatalogForAppChoice()) return;
    resetOnboardingBranch();
    const next = routeAfterAppChoice();
    onboardingRoute = next;
    render();
    void refreshBrowserImportReadiness();
  });
  document.querySelector<HTMLButtonElement>("#continue-detected-apps")?.addEventListener("click", () => {
    if (savedAccountMode === "ask") savedAccountMode = savedNativeApps.size ? "use" : "clean";
    persistSavedAccountPreferences();
    const next = hasSelectedMissingNativeApps() ? "install" : "mullvad";
    markOnboardingBranch(next);
    onboardingRoute = next;
    render();
    if (next === "mullvad") void refreshMullvadSetup();
  });
  document.querySelector<HTMLButtonElement>("#continue-install-apps")?.addEventListener("click", () => {
    const selectedInstalls = [...selectedFirstInstallApps];
    selectedFirstInstallApps.clear();
    if (selectedInstalls.length) {
      savedAccountMode = "use";
      selectedInstalls.forEach((appId) => savedNativeApps.add(appId));
      persistSavedAccountPreferences();
      enqueueBackgroundInstalls(selectedInstalls);
    } else if (!hasSelectedInstalledNativeApps() && savedAccountMode === "ask") {
      savedAccountMode = "clean";
      persistSavedAccountPreferences();
    }
    onboardingRoute = "mullvad";
    render();
    void refreshMullvadSetup();
  });
  document.querySelector("#onboarding-back")?.addEventListener("click", () => {
    onboardingRoute = previousSetupRoute(onboardingRoute);
    render();
    if (onboardingRoute === "import") void refreshBrowserImportReadiness();
    if (onboardingRoute === "mullvad") void refreshMullvadSetup();
  });
  document.querySelector("#skip-onboarding")?.addEventListener("click", () => {
    clearServiceOnboardingResume();
    if (onboardingRoute === "scrub") {
      scrubSetupPlan.mode = "skip";
      persistScrubSetupPlan();
      void completeOnboarding();
    } else {
      onboardingRoute = "scrub";
      render();
    }
  });
  document.querySelectorAll<HTMLButtonElement>("[data-send-mode]").forEach((button) => button.addEventListener("click", () => {
    const mode = button.dataset.sendMode as SendMode;
    if (!["clipboard", "double", "single"].includes(mode)) return;
    setup.sendMode = mode;
    setup.placementMode = "atomic";
    setup.acceptedRisk = false;
    setup.acceptedRiskForMode = null;
    render();
  }));
  document.querySelector<HTMLInputElement>("#accept-send-risk")?.addEventListener("change", (event) => {
    const accepted = (event.currentTarget as HTMLInputElement).checked;
    setup.acceptedRisk = accepted;
    setup.acceptedRiskForMode = accepted ? setup.sendMode : null;
    render();
  });
  document.querySelector("#finish-onboarding")?.addEventListener("click", () => {
    if (onboardingRoute !== "sending") return;
    if (setup.sendMode === "manual") setup.sendMode = "clipboard";
    if (!canCompleteSetup(setup)) return;
    setup.placementMode = "atomic";
    onboardingRoute = "passwords";
    render();
  });
  bindOnboardingPasswordRole();
  document.querySelectorAll<HTMLButtonElement>("[data-password-role-next]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.passwordRoleNext as OnboardingRoute; render(); }));
  document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => { onboardingRoute = "scrub"; render(); });
  document.querySelector<HTMLInputElement>("#onboarding-screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelectorAll<HTMLInputElement>("[data-prototype-privacy]").forEach((input) => input.addEventListener("change", () => {
    const id = input.dataset.prototypePrivacy;
    if (!id) return;
    if (input.checked) prototypePrivacyChoices.add(id); else prototypePrivacyChoices.delete(id);
  }));
  document.querySelectorAll<HTMLSelectElement>("[data-detected-account]").forEach((select) => select.addEventListener("change", () => {
    const id = select.dataset.detectedAccount;
    if (!id) return;
    const choice = select.value === "osl" ? "osl" : "existing";
    detectedAccountChoices.set(id, choice);
    const row = document.querySelector<HTMLElement>(`[data-detected-account-row="${CSS.escape(id)}"]`);
    row?.classList.toggle("detected-account-osl", choice === "osl");
    row?.classList.toggle("detected-account-existing", choice === "existing");
  }));
  document.querySelector<HTMLSelectElement>("#detected-launch-select")?.addEventListener("change", (event) => {
    savedAccountMode = (event.currentTarget as HTMLSelectElement).value === "clean" ? "clean" : "use";
    persistSavedAccountPreferences();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mullvad-choice]").forEach((button) => button.addEventListener("click", () => {
    mullvadPreference = button.dataset.mullvadChoice === "auto" ? "auto" : "off";
    render();
  }));
  document.querySelector("#continue-mullvad")?.addEventListener("click", () => { if (!mullvadPreference) return; onboardingRoute = "sending"; render(); });
  document.querySelector("#start-scrub-setup")?.addEventListener("click", () => {
    scrubSetupPlan.mode = "scrub";
    scrubSetupStep = "accounts";
    render();
  });
  document.querySelector("#continue-scrub-accounts")?.addEventListener("click", () => { scrubSetupStep = "options"; render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-mode]").forEach((button) => button.addEventListener("click", () => {
    const mode = button.dataset.scrubMode as ScrubMode;
    if (!(["skip", "scrub", "autoscrub"] as ScrubMode[]).includes(mode)) return;
    const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
    scrubSetupPlan = parseScrubSetupPlan(JSON.stringify({ ...scrubSetupPlan, mode }), new Set(scrubAccountSelections().map(({ id }) => id)), [...enabledScrubSignals], proActive);
    persistScrubSetupPlan();
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-target]").forEach((button) => button.addEventListener("click", () => {
    const id = button.dataset.scrubTarget;
    const available = new Set(scrubAccountSelections().map((target) => target.id));
    if (!id || !available.has(id)) return;
    const selected = new Set(scrubSetupPlan.targetIds);
    if (selected.has(id)) selected.delete(id);
    else if (selected.size < SCRUB_TARGET_LIMIT) selected.add(id);
    scrubSetupPlan.targetIds = [...selected];
    persistScrubSetupPlan();
    render();
  }));
  document.querySelector("#select-all-scrub")?.addEventListener("click", () => {
    scrubSetupPlan.targetIds = scrubAccountSelections().map(({ id }) => id).slice(0, SCRUB_TARGET_LIMIT);
    persistScrubSetupPlan();
    render();
  });
  document.querySelector("#clear-scrub-selection")?.addEventListener("click", () => {
    scrubSetupPlan.targetIds = [];
    persistScrubSetupPlan();
    render();
  });
  document.querySelectorAll<HTMLInputElement>("[data-scrub-category]").forEach((input) => input.addEventListener("change", () => {
    const group = input.dataset.scrubCategory as ScrubSignalGroup;
    if (!defaultScrubSignalGroups.includes(group)) return;
    if (input.checked) enabledScrubSignals.add(group); else enabledScrubSignals.delete(group);
    scrubSetupPlan.signalGroups = [...enabledScrubSignals];
    localStorage.setItem(scrubSignalsStorageKey, JSON.stringify(scrubSetupPlan.signalGroups));
    persistScrubSetupPlan();
    render();
  }));
  document.querySelector("#finish-scrub-setup")?.addEventListener("click", () => {
    persistScrubSetupPlan();
    void completeOnboarding();
  });
  document.querySelector("#skip-scrub-setup")?.addEventListener("click", () => {
    scrubSetupPlan.mode = "skip";
    persistScrubSetupPlan();
    void completeOnboarding();
  });
  document.querySelector("#close-decoy")?.addEventListener("click", () => {
    if (isDesignPreview) {
      onboardingRoute = "welcome";
      render();
      return;
    }
    void getCurrentWindow().close().catch(() => undefined);
  });
}

function bindOnboardingPasswordRole(): void {
  const form = document.querySelector<HTMLFormElement>("[data-onboarding-password-role]");
  if (!form) return;
  const role = form.dataset.onboardingPasswordRole === "stealth" ? "stealth" : "burn";
  const current = form.elements.namedItem("current") as HTMLInputElement;
  const alternate = form.elements.namedItem("alternate") as HTMLInputElement;
  const confirm = form.elements.namedItem("confirm") as HTMLInputElement;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]');
  const error = form.querySelector<HTMLElement>("[data-onboarding-role-error]");
  const validate = (): void => {
    if (!submit || !error) return;
    submit.disabled = !isValidMainPassword(current.value) || !isValidNewMainPassword(alternate.value) || alternate.value !== confirm.value || alternate.value === current.value;
    error.textContent = "";
  };
  current.addEventListener("input", validate);
  alternate.addEventListener("input", validate);
  confirm.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!submit || submit.disabled || !error) return;
    submit.disabled = true;
    const currentSecret = current.value;
    const alternateSecret = alternate.value;
    current.value = "";
    alternate.value = "";
    confirm.value = "";
    try {
      passwordRoleStatus = await setHubAlternatePassword(role, currentSecret, alternateSecret);
      onboardingRoute = form.dataset.passwordRoleNext as OnboardingRoute;
      render();
    } catch (failure) {
      error.textContent = localActionError(failure, "Password was not changed");
      submit.disabled = false;
      current.focus();
    }
  });
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
    clearServiceOnboardingResume();
    resetOnboardingBranch();
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

async function refreshMullvadSetup(): Promise<void> {
  if (mullvadBusy) return;
  mullvadBusy = true;
  render();
  try {
    const [, detected] = await Promise.all([
      withNativeDeadline(loadMullvadStatus(), "Check Mullvad", bootSupportDeadlineMs),
      withNativeDeadline(loadVpnConnectionDetected(), "Check VPN connection", bootSupportDeadlineMs).catch(() => false),
    ]);
    vpnConnectionDetected = detected;
    if (vpnConnectionDetected && route === "onboarding" && onboardingRoute === "mullvad") {
      onboardingRoute = "sending";
    }
  } catch {
    showToast("Mullvad status is unavailable");
  } finally {
    mullvadBusy = false;
    render();
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
        const gate = await unlockHubPasswordGate(secret);
        secret = "";
        if (gate.outcome === "wrong") {
          error.textContent = gate.lockoutSecondsRemaining > 0
            ? `Try again in ${gate.lockoutSecondsRemaining} seconds.`
            : "Password not recognized.";
          submit.disabled = false;
          password.focus();
          return;
        }
        if (gate.outcome === "decoy") {
          core = structuredClone(unavailableCoreIntegration);
          services = [];
          passwordRoleStatus = null;
          route = "onboarding";
          onboardingRoute = "decoy";
          render();
          return;
        }
        if (gate.outcome === "burned") {
          localStorage.clear();
          onboardingComplete = false;
          setup = parseSetupState(null);
          services = [];
          passwordRoleStatus = null;
          core = structuredClone(unavailableCoreIntegration);
          route = "onboarding";
          onboardingRoute = "welcome";
          showToast(gate.burn?.localCleanupComplete ? "Verified local OSL cleanup completed" : "OSL cleanup needs attention");
          render();
          return;
        }
        if (!gate.readiness?.unlocked) throw new Error("OSL did not unlock");
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
        else onboardingRoute = pendingOnboardingRoute() ?? "tutorial";
      }
      secret = "";
      render();
      if (route === "onboarding" && onboardingRoute === "import") void refreshBrowserImportReadiness();
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
          onboardingRoute = pendingOnboardingRoute() ?? "tutorial";
          route = "onboarding";
          showToast("Password is configured. Continue setup.");
        } else {
          route = "home";
        }
        render();
        if (route === "onboarding" && onboardingRoute === "import") void refreshBrowserImportReadiness();
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
        resetOnboardingBranch();
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
  const protectedSheet = activeEmbeddedHost
    ? protectedSheetMode === "local"
      ? localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)
      : peerProtectedSheetMarkup(peerProtectedSheet, hubPeople)
    : "";
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
    return `<div class="trusted-stack home-trusted-stack"><header class="home-header guide-header"><button class="home-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="guide-header-service">${serviceLogo(activeService.id)}<span><strong>${escapeHtml(activeService.displayName)}</strong><small>${isCoreProtectionReady(core.readiness) ? "Ready" : "Needs attention"}</small></span></div>${settingsButtonMarkup()}</header></div>`;
  }
  const localProtection = route === "service" && activeEmbeddedHost
    ? `<button class="local-protected-toggle" id="local-protected-toggle" type="button" aria-expanded="${localProtectedSheet.open || peerProtectedSheet.open}">Protect</button>`
    : "";
  const serviceControls = route === "service" && activeService ? `<div class="service-context"><span class="service-context-logo">${serviceLogo(activeService.id)}</span><span><strong>${escapeHtml(activeHomeAppName())}</strong><small>${activeEmbeddedHost ? "Isolated OSL profile" : activeNativeHostId ? "OSL app window" : "Needs setup"}</small></span>${localProtection}</div>` : "";
  const onboardingContinue = route === "service" && onboardingServiceSetup && (activeEmbeddedHost || activeNativeHostId)
    ? `<button class="button compact primary" id="onboarding-service-continue">Continue setup</button>`
    : "";
  return `<div class="trusted-stack"><header class="workspace-header"><div class="hub-command"><button class="command-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span><strong>OSL Privacy</strong></span></button>${appLauncherStrip()}${simpleDeviceStatusMarkup()}</div>${serviceControls ? `<div class="context-command">${serviceControls}</div>` : ""}${onboardingContinue}${settingsButtonMarkup("workspace-settings")}</header>${updateBannerMarkup()}</div>`;
}

function homeHeader(): string {
  const ready = isCoreProtectionReady(core.readiness);
  return `<div class="trusted-stack home-trusted-stack"><header class="home-header"><button class="home-brand home-brand-home" data-route="home" aria-label="OSL Privacy home"><span class="home-brand-mark" aria-hidden="true"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></span><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="home-core-state ${ready ? "ready" : "pending"}" role="status"><span class="dot"></span>${ready ? "OSL unlocked" : "Unlock OSL"}</div>${settingsButtonMarkup()}</header>${updateBannerMarkup()}</div>`;
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
    if (module?.state === "Coming later" && !homeEditMode) return "";
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
  const oslSection = oslTiles ? `<section class="home-app-section"><h2>OSL</h2><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  const friendCount = hubPeople.length;
  const friendId = friendDisplayId ? compactFriendId(friendDisplayId) : null;
  const activity = notificationsEnabled
    ? `<button class="friends-activity" data-notification-settings><span class="dot"></span><span><strong>Activity</strong><small>${appNotifications?.length ? `${appNotifications.length} local OSL ${appNotifications.length === 1 ? "event" : "events"}` : "Nothing new"}</small></span></button>`
    : "";
  return `<main class="content-viewport home-dashboard ${homeEditMode ? "editing" : ""}">
    <section class="home-primary">
      <section class="home-apps" aria-labelledby="route-heading"><header><h1 id="route-heading" class="sr-only" tabindex="-1">Apps</h1><button class="button compact" id="edit-home">${homeEditMode ? "Done" : "Edit"}</button></header><div class="home-app-groups"><section class="home-app-section"><h2>Social</h2><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section><section class="home-app-section"><h2>Email</h2><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>${oslSection}</div></section>
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
  if (!hubPeople.length) return `<div class="empty-state"><strong>No friends yet</strong><p>Add one with an invite.</p></div>`;
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
    const management = `<details class="friend-management"><summary>Manage</summary><div>${nicknameForm}<div class="friend-approvals"><span>Approved chats</span><div>${scopes}</div>${truncated}</div><details class="friend-security"><summary>Security details</summary><div><span>OSL ID</span><code>${escapeHtml(identity)}</code><span>Verification code</span><code>${escapeHtml(person.safetyNumber)}</code></div></details></div></details>`;
    return `<article class="person-row person-profile"><header><div><strong>${escapeHtml(nickname)}</strong>${person.pendingKeyChange ? `<small>Security change needs review</small>` : `<small>${person.safetyNumberVerified ? "Verified" : "Request pending"}</small>`}</div>${action}</header>${management}</article>`;
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
  return `<dialog class="friends-dialog" id="friends-dialog" aria-labelledby="friends-dialog-title"><div class="friends-dialog-card"><header><h2 id="friends-dialog-title">Friends</h2><button class="icon-button" id="friends-dialog-close" aria-label="Close friends">×</button></header><form id="add-friend-form" class="friend-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add friend</button></form><p class="form-status" id="friend-form-status" role="status"></p><p class="scope-approval-note">Encrypted chats stay off after adding someone. Compare the verification code another way, then approve each chat separately.</p><div class="people-list home-people-list">${peopleListMarkup("manage", friendsDialogPageSize, pageStart)}</div>${pagination}${inviteCard}</div></dialog>`;
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
    { scope: "chat", title: "This chat", detail: activeProtectedContextKind === "peer" ? "Revoke this app account + friend scope." : "Forget this exact OSL conversation on this device." },
    { scope: "app", title: "This app", detail: "Remove indexed local OSL data and request relay cleanup." },
    { scope: "account", title: "Entire OSL account", detail: "Remove every OSL identity and local setting on this computer." },
  ];
  const selectedReason = burnScopeReason(burnScope);
  const phrase = burnConfirmationPhrase(burnScope);
  const scopeCards = cards.map((card) => {
    const reason = burnScopeReason(card.scope);
    return `<button class="burn-scope-card ${burnScope === card.scope ? "selected" : ""}" type="button" data-burn-scope="${card.scope}" ${reason ? "disabled" : ""} aria-pressed="${burnScope === card.scope}"><strong>${card.title}</strong><small>${card.detail}</small>${reason ? `<span>${escapeHtml(reason)}</span>` : ""}</button>`;
  }).join("");
  const effects = burnScope === "chat"
    ? activeProtectedContextKind === "peer"
      ? "OSL revokes local approval, display, and expiry settings for this app account + friend, then attempts to delete sent relay blobs. Provider messages and opened copies remain."
      : "OSL destroys local decrypt material and caches for this exact chat."
    : burnScope === "account"
      ? "OSL removes every local identity, decrypt key, cache, and preference on this computer."
      : serviceBurnReadiness?.coverageComplete
        ? `OSL removes local settings and caches for ${serviceBurnReadiness.indexedScopes} indexed ${serviceBurnReadiness.indexedScopes === 1 ? "scope" : "scopes"} in this connected account, then attempts to delete their sent relay blobs. Login profile, cookies, provider history, and other copies remain.`
        : "OSL must prove complete local coverage before app-wide burn is available.";
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card"><header><h2 id="burn-dialog-title">Burn local data</h2><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><div class="burn-scope-grid" aria-label="Burn scope">${scopeCards}</div><section class="burn-truth"><strong>Before you continue</strong><ul><li>${effects}</li><li>Messages and history in the service remain.</li><li>Screenshots, exports, backups, and copies held by other people cannot be retracted.</li></ul></section><details class="burn-more"><summary>Other options</summary><div class="burn-options"><label class="setting-line unavailable"><span><strong>Provider messages</strong><small>Not removed. Burn changes only indexed local OSL data and sent relay records.</small></span><input type="checkbox" disabled/></label><label class="setting-line unavailable"><span><strong>Burn for friends · Pro</strong><small>${pro ? "Requires every recipient’s prior signed consent and an acknowledgment from each device." : "A Pro initiator may request this for Free recipients only after each recipient gives signed consent."} The consent-and-acknowledgment workflow is unavailable in this build.</small></span><input type="checkbox" disabled/></label>${burnScope === "account" ? `<label class="setting-line interactive"><span><strong>Uninstall after burn</strong><small>After a successful local burn, open Windows installed apps.</small></span><input id="burn-uninstall" type="checkbox"/></label>` : ""}</div></details><form id="burn-confirm-form" class="burn-confirm"><label for="burn-confirm-input">Type <code>${phrase}</code> to continue</label><input id="burn-confirm-input" autocomplete="off" autocapitalize="characters" spellcheck="false" ${selectedReason ? "disabled" : ""}/><p class="form-status" id="burn-form-status" role="status">${selectedReason ? escapeHtml(selectedReason) : "This cannot be undone."}</p><footer><button class="button ghost" type="button" data-close-burn>Cancel</button><button class="button danger" id="burn-confirm-submit" type="submit" disabled>${burnBusy ? "Burning…" : "Burn now"}</button></footer></form></section></dialog>`;
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
  const details = `<details class="guide-details"><summary>Sign-in privacy</summary><p>${nativeInstalled ? "OSL uses a separate local app profile only when the installed app supports one safely. Otherwise it opens an isolated OSL web profile. Your normal app and account stay untouched." : "OSL keeps this service in its own local browser profile. Sign in once here; later opens reuse that profile."}</p></details>`;
  const selectedApp = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId);
  const openAction = selectedApp?.launchState === "available"
    ? `<button class="button primary" id="embedded-service-setup" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : nativeInstalled ? "Open app in OSL" : "Open in OSL"}</button>`
    : `<button class="button" disabled>Coming later</button>`;
  const nativeNote = nativeInstalled ? `<p class="guide-native-note">OSL never closes or borrows the normal ${name} window.</p>` : "";
  return `<main class="content-viewport service-guide" id="route-heading" tabindex="-1"><section class="guide-card guide-card-simple"><header><button class="text-back" id="service-guide-exit">← Apps</button></header><div class="guide-hero"><span class="guide-logo" data-guide-service="${service.id}">${serviceLogo(service.id)}</span><h1>Connect ${name}</h1></div><footer class="guide-actions">${openAction}${installedAction}</footer>${nativeNote}${details}</section>${onboardingServiceSetup ? '<button class="onboarding-skip-dock" id="service-guide-skip">Skip · manual setup</button>' : ""}</main>`;
}

function settingsContent(): string {
  const items: Array<[SettingsSection, string]> = [["account", "Account"], ["apps", "Apps"], ["scrub", "Scrub"], ["cleanup", "Cleanup"], ["notifications", "Notifications"], ["appearance", "Appearance"], ["about", "About"]];
  return `<main class="content-viewport settings-page"><nav class="settings-sidebar" aria-label="Settings"><h1 id="route-heading" tabindex="-1">Settings</h1>${items.map(([id, label]) => `<button data-settings="${id}" class="${settingsSection === id ? "active" : ""}" ${settingsSection === id ? 'aria-current="page"' : ""}>${label}</button>`).join("")}</nav><section class="settings-detail">${settingsSectionContent()}</section></main>`;
}

function settingsSectionContent(): string {
  if (settingsSection === "account") return `${identitySettingsContent()}${settingsDivider()}${passwordSecuritySettingsContent()}${accountAdvancedSettingsContent()}`;
  if (settingsSection === "apps") return `${serviceAccountsSettingsContent()}${sendingSettingsContent()}`;
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
  return `<section class="settings-section password-security"><header><div><h3>Password & security</h3><p>Protects encrypted storage on this device.</p></div><div class="settings-actions">${passwordAction}</div></header><details class="settings-disclosure"><summary>Alternate passwords</summary><div>${roles}</div></details></section>`;
}

function accountAdvancedSettingsContent(): string {
  return `<details class="account-advanced settings-disclosure"><summary>Advanced</summary><div class="danger-zone"><h3>Burn local data</h3><p>Review the scope and limits before anything changes.</p><button class="button danger" id="full-cleanup-button" data-open-burn="account">Review Burn</button></div></details>`;
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
    if (app.availability === "installed" && app.isolatedProfileAvailable) return `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Ready for an isolated OSL profile</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${savedNativeApps.has(app.id) ? "checked" : ""}/></label>`;
    if (app.availability === "installed") return `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Installed · isolated web profile only</small></span></span></div>`;
    if (app.availability === "installable") return `<div class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${backgroundInstallIds.has(app.id) ? "Installing…" : "Optional Windows app"}</small></span></span><button class="button compact" type="button" data-background-install="${app.id}" ${backgroundInstallIds.has(app.id) ? "disabled" : ""}>${backgroundInstallIds.has(app.id) ? "Installing…" : "Background install"}</button></div>`;
    return `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Embedded web only</small></span></span></div>`;
  }).join("");
  const savedAccountSettings = `<details class="saved-account-settings settings-disclosure"><summary>Account opening</summary><div class="saved-account-choices"><button class="setting-option ${savedAccountMode === "use" ? "selected" : ""}" data-saved-account-mode="use"><strong>Use selected apps</strong><small>Open only checked desktop apps</small></button><button class="setting-option ${savedAccountMode === "clean" ? "selected" : ""}" data-saved-account-mode="clean"><strong>Use web profiles</strong><small>Create an isolated OSL profile</small></button></div><div class="saved-account-apps">${installedChoices}</div><p>Only checked installed apps may open in a separate OSL-owned window.</p></details>`;
  return `<h2>Apps</h2><p>Open each account in its own OSL profile.</p><div class="account-settings-list">${rows}</div>${savedAccountSettings}<details class="settings-disclosure sign-in-details"><summary>How sign-ins stay private</summary><div class="warning"><strong>Local sessions</strong><p>Service cookies stay in the matching OSL profile so you remain signed in. Your typed service password is not sent to OSL.</p></div></details>`;
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
    const bytes = new Uint8Array(await file.arrayBuffer());
    let candidates: LocalMessageCandidate[] | null = null;
    try {
      const decoded = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      if (decoded.trimStart().startsWith("[")) {
        candidates = importLocalMessageExport(decoded, {
          serviceId: "local_import",
          accountId: "manual-export",
          conversationId: "privacy-scan",
        });
      }
    } catch {
      candidates = null;
    }
    if (!candidates?.length) {
      candidates = [{
        serviceId: "local_import",
        accountId: "manual-export",
        conversationId: "privacy-scan",
        messageLocator: "local-attachment-1",
        authoredBySelf: false,
        createdAtUnixMs: null,
        text: "",
        attachments: [{
          attachmentId: "local-attachment-1",
          displayName: file.name.slice(0, 64) || "unnamed attachment",
          contentBase64: bytesToBase64(bytes),
        }],
      }];
    }
    const persisted = await persistLocalScrubExport(candidates);
    if (!validateCoverageReceipt(persisted.receipt)) throw new Error("The coverage receipt was invalid");
    privacyScanResult = persisted.scan;
    privacyScanStatus = persisted.status;
    privacyCoverageReceipt = persisted.receipt;
    selectedScrubFindings.clear();
    scrubResultsPage = 0;
    scrubReviewOpen = false;
    scrubReviewPage = 0;
    privacyScanFileName = file.name.slice(0, 96);
  } catch (failure) {
    privacyScanResult = null;
    privacyScanFileName = null;
    privacyScanStatus = null;
    privacyCoverageReceipt = null;
    showToast(localActionError(failure, "The export could not be scanned locally"));
  } finally {
    privacyScanBusy = false;
    render();
  }
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 32 * 1024;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return btoa(binary);
}

function sendingSettingsContent(): string {
  const selectedMode: SendMode = setup.sendMode === "manual" ? "clipboard" : setup.sendMode;
  const modes: Array<[SendMode, string, string]> = [
    ["clipboard", "Copy", "Never presses Send"],
    ["double", "Double Enter", "Experimental · two exact checks"],
    ["single", "Single Enter", "Advanced · highest risk"],
  ];
  const accounts = services.flatMap((service) => service.accounts.map((account) => ({
    serviceId: service.id,
    service: service.displayName,
    accountId: account.id,
    account: account.label,
  })));
  const consentRows = needsRiskAcceptance(selectedMode) && accounts.length
    ? `<div class="send-account-consents"><strong>Account approvals</strong>${accounts.map((account) => `<div><span>${escapeHtml(account.service)} · ${escapeHtml(account.account)}</span><small>${hasExperimentalSendConsent(selectedMode, account.serviceId, account.accountId) ? "Approved on this device" : "Will ask before first use"}</small></div>`).join("")}</div>`
    : "";
  return `<details class="settings-disclosure sending-settings"><summary><span><strong>Sending</strong><small>${escapeHtml(formatSendMode(selectedMode))}</small></span></summary><div class="sending-settings-body"><div class="send-mode-list compact">${modes.map(([mode, label, detail]) => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-settings-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${label}</strong></span><small>${detail}</small></button>`).join("")}</div>${needsRiskAcceptance(selectedMode) ? `<div class="warning send-settings-warning"><strong>Experimental</strong><p>OSL must recheck the exact app, account, chat, and composer. If proof is unavailable or changes, it copies instead and sends nothing.</p></div>` : `<p class="send-settings-truth">OSL encrypts and copies. You choose where and when to send.</p>`}${consentRows}</div></details>`;
}

async function changeSendingMode(mode: SendMode): Promise<void> {
  if (!["clipboard", "double", "single"].includes(mode)) return;
  if (needsRiskAcceptance(mode)) {
    const accepted = window.confirm(`${formatSendMode(mode)} is experimental. Apps can change without warning. OSL will stop unless it can verify the exact app, account, chat, and composer, and each account will ask again before first use.`);
    if (!accepted) return;
  }
  const previous = { ...setup };
  setup = {
    sendMode: mode,
    placementMode: "atomic",
    acceptedRisk: needsRiskAcceptance(mode),
    acceptedRiskForMode: needsRiskAcceptance(mode) ? mode : null,
  };
  render();
  try {
    const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, showPlaintextPreview: true });
    setup = saved.setup;
    showToast(`${formatSendMode(mode)} selected`);
  } catch {
    setup = previous;
    showToast("Sending preference could not be saved");
  }
  render();
}

function privacySettingsContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const assistedDeleteWarning = `<details class="safety-disclosure"><summary>Assisted deletion and account-deletion options</summary><div><p><strong>Brutally honest warning:</strong> Gmail web, Discord, and Telegram web may restrict or ban an account for assisted UI deletion. OSL permanently stops on every captcha, challenge, rate signal, unknown result, account change, or changed interface. Use only while present, in small fixed human-speed batches.</p><p>IMAP is the lower-ban-risk optional email path. Sanctioned scorched-earth options: <a href="https://support.discord.com/hc/articles/212500837-How-do-I-permanently-delete-my-account" target="_blank" rel="noreferrer">delete the Discord account</a> or <a href="https://support.discord.com/hc/articles/360004027692-Requesting-a-Copy-of-your-Data" target="_blank" rel="noreferrer">request its data first</a>.</p></div></details>`;
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose file"}</label><input id="privacy-export-input" class="sr-only" type="file" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>${assistedDeleteWarning}`;
  const autoScrubPlan = proActive ? "PRO · TRANSPORT-GATED" : "PRO REQUIRED";
  return `<h2>Scrub</h2><p class="scrub-local-promise"><strong>Your messages and attachments never leave this device.</strong> Every scan and review stays local.</p><section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h3>Review a file</h3><p>Choose a message export or attachment of any type. OSL reports exactly what it could and could not inspect.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}<details class="settings-disclosure autoscrub-disclosure"><summary><span><strong>AutoScrub assistant</strong><small>${autoScrubPlan}</small></span></summary>${autoScrubMarkup(proActive)}</details><details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, providers, exports, and backups may retain copies.</p><p>Only a provider readback can verify removal within its stated coverage. Exports, backups, recipients, and other copies may remain.</p></div></details><details class="privacy-technical settings-disclosure"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Primary delete path</span><strong>Existing signed-in hosted session; no re-authentication</strong></div><div class="setting-line"><span>Optional paths</span><strong>IMAP and Telegram TDLib</strong></div><label class="setting-line interactive"><span><strong>Windows capture resistance</strong><small>Asks Windows to exclude OSL from ordinary screen capture. Cameras, malware, and modified recipients can still capture content.</small></span><input id="screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label></details>`;
}

function autoScrubAccountIds(): string[] {
  const serviceId: ServiceId = autoScrubPathId === "gmail-web" || autoScrubPathId === "imap" ? "email" : autoScrubPathId === "discord" ? "discord" : "telegram";
  const linked = services.find((service) => service.id === serviceId)?.accounts.map((account) => account.id) ?? [];
  const imported = privacyScanResult?.findings.filter((finding) => finding.serviceId === serviceId).map((finding) => finding.accountId) ?? [];
  return [...new Set([...linked, ...imported])];
}

function selectedImapLocators(): ScrubImapLocator[] {
  if (!autoScrubAccountId) return [];
  return selectedScrubItems().flatMap(({ finding }) => finding.serviceId === "email" && finding.accountId === autoScrubAccountId && finding.authoredBySelf && finding.canRequestDelete && finding.createdAtUnixMs !== null && !finding.messageLocator.startsWith("local-import-")
    ? [{ accountId: finding.accountId, mailbox: finding.conversationId, messageId: finding.messageLocator, sinceDate: finding.createdAtUnixMs }]
    : []);
}

function autoScrubReceiptMarkup(receipt: ProviderDeletionReceipt | null): string {
  if (!receipt) return "";
  const summary = summarizeAutoScrubReceipt(receipt);
  const items = receipt.items.map((item) => `<li><strong>${escapeHtml(item.outcome)}</strong><span>${escapeHtml(item.itemId)} · ${escapeHtml(item.detail)}</span></li>`).join("");
  return `<section class="autoscrub-receipt" aria-live="polite"><strong>${escapeHtml(summary.heading)}</strong><p>${escapeHtml(summary.detail)}</p><ul>${items}</ul></section>`;
}

function autoScrubMarkup(proActive: boolean): string {
  const accounts = autoScrubAccountIds();
  if (!autoScrubAccountId || !accounts.includes(autoScrubAccountId)) autoScrubAccountId = accounts[0] ?? "";
  const capability = autoScrubCapabilities.find((item) => item.providerId === autoScrubPathId) ?? unavailableAutoScrubCapabilities.find((item) => item.providerId === autoScrubPathId) ?? unavailableAutoScrubCapabilities[0];
  const eligible = autoScrubPathId === "imap" ? selectedImapLocators().length : 0;
  const active = proActive && capability.liveConfirmed && Boolean(autoScrubAccountId);
  const providers = autoScrubCapabilities.map((item) => `<li><span><strong>${escapeHtml(item.label)}</strong><small>${item.primary ? "PRIMARY · " : "OPTIONAL · "}${escapeHtml(item.coverage)}</small></span><b class="status-tag ${item.liveConfirmed ? "active" : ""}">${item.liveConfirmed ? "ACTIVE" : "PARKED"}</b>${item.unavailableReason ? `<p>${escapeHtml(item.unavailableReason)}</p>` : ""}</li>`).join("");
  const pathOptions = autoScrubCapabilities.map((item) => `<option value="${escapeHtml(item.providerId)}" ${item.providerId === autoScrubPathId ? "selected" : ""}>${escapeHtml(item.label)}${item.primary ? " — primary" : " — optional"}</option>`).join("");
  const accountOptions = accounts.map((id) => `<option value="${escapeHtml(id)}" ${id === autoScrubAccountId ? "selected" : ""}>${escapeHtml(id)}</option>`).join("");
  const unavailableReason = !proActive ? "AutoScrub requires Pro." : !capability.liveConfirmed ? capability.unavailableReason ?? "This path has not live-confirmed the selected signed-in account." : eligible === 0 ? autoScrubPathId === "imap" ? "Select sent email findings with real Message-ID and date locators. Plain-text lines and ambiguous exports stay manual." : "Load and review your own items in the signed-in service window." : "";
  return `<section class="autoscrub-card" aria-disabled="${!active}"><header><div><span class="privacy-local-mark">DELETE ENGINE · REVIEW REQUIRED</span><h3>One reviewed batch</h3></div><button class="button compact" id="refresh-autoscrub" type="button" ${autoScrubBusy ? "disabled" : ""}>Check live path</button></header><p>The default path reuses the account already signed in inside OSL, scrolls its live UI at a fixed human pace, shows a no-delete dry run, and executes only the reduced reviewed scope. It never asks for separate credentials.</p><label class="autoscrub-account"><span>Deletion path</span><select id="autoscrub-path">${pathOptions}</select></label><ul class="autoscrub-providers">${providers}</ul>${accounts.length ? `<label class="autoscrub-account"><span>Account</span><select id="autoscrub-account">${accountOptions}</select></label>` : `<p>No matching account is available.</p>`}${unavailableReason ? `<p class="autoscrub-unavailable"><strong>Unavailable:</strong> ${escapeHtml(unavailableReason)}</p>` : ""}<label class="autoscrub-confirm"><input id="autoscrub-final-confirmation" type="checkbox" ${active && eligible ? "" : "disabled"}/><span><strong>Final confirmation</strong><small>Delete only the ${eligible} currently selected, eligible ${eligible === 1 ? "item" : "items"}. This can be irreversible.</small></span></label><button class="button primary" id="run-autoscrub" type="button" ${active && eligible && !autoScrubBusy ? "" : "disabled"}>${autoScrubBusy ? "Working…" : "Dry-run, then delete"}</button>${autoScrubError ? `<p class="autoscrub-error" role="alert">${escapeHtml(autoScrubError)}</p>` : ""}${autoScrubReceiptMarkup(autoScrubDryRunReceipt)}${autoScrubReceiptMarkup(autoScrubExecutionReceipt)}<details class="autoscrub-connect"><summary>Optional: use IMAP instead</summary><form id="autoscrub-imap-form"><label>Account<select id="autoscrub-imap-account" required>${accountOptions}</select></label><label>IMAP host<input id="autoscrub-imap-host" autocomplete="off" required/></label><label>Username<input id="autoscrub-imap-username" autocomplete="username" required/></label><label>Credential type<select id="autoscrub-imap-auth-kind"><option value="appPassword">App password</option><option value="oauthBearer">OAuth bearer token</option></select></label><label>Credential<input id="autoscrub-imap-secret" type="password" autocomplete="current-password" required/></label><label>Mailbox<input id="autoscrub-imap-mailbox" value="Sent" required/></label><button class="button" type="submit" ${accounts.length ? "" : "disabled"}>Connect and verify optional IMAP</button><p>This secondary path uses OS-backed secure storage. The signed-in-session path above does not need this.</p></form></details><details><summary>Optional: Telegram TDLib</summary><p>TDLib remains a secondary adapter and is unavailable until its client is packaged and live-confirmed. Telegram Web is the primary path.</p></details><p><a href="https://support.discord.com/hc/articles/212500837-How-do-I-permanently-delete-my-account" target="_blank" rel="noreferrer">Discord account deletion</a> · <a href="https://my.telegram.org/auth?to=delete" target="_blank" rel="noreferrer">Telegram account deletion</a></p></section>`;
}

async function refreshAutoScrubCapability(): Promise<void> {
  if (!autoScrubAccountId) {
    autoScrubCapabilities = unavailableAutoScrubCapabilities;
    return;
  }
  autoScrubCapabilities = await createDesktopAutoScrubBridge([autoScrubAccountId]).capabilities().catch(() => unavailableAutoScrubCapabilities);
}

async function executeSelectedAutoScrubBatch(finalConfirmation: boolean): Promise<void> {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const capability = autoScrubCapabilities.find((item) => item.providerId === autoScrubPathId);
  const locators = selectedImapLocators();
  if (autoScrubPathId !== "imap") throw new Error("The hosted-session command port has not been live-confirmed by this build");
  if (!proActive || !capability?.liveConfirmed || !finalConfirmation || !autoScrubAccountId || locators.length === 0) throw new Error("AutoScrub confirmation or live transport is missing");
  const bridge = createDesktopAutoScrubBridge([autoScrubAccountId]);
  const result = await runAutoScrubBatch({
    target: { providerId: "imap", accountId: autoScrubAccountId },
    prepare: async (stepUp) => {
      const findings = await prepareScrubImapFindings(locators, stepUp);
      const itemIds = findings.map((finding) => finding.itemId);
      const channelIds = [...new Set(findings.map((finding) => finding.channelId))];
      const policy: ScopePolicy = { providerId: "imap", accountId: autoScrubAccountId, itemIds, channelIds, protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: findings.length, minAgeMs: 0 };
      return { providerId: "imap", accountId: autoScrubAccountId, findings, approved: policy, requested: policy };
    },
    capability,
    bridge,
    finalConfirmation,
    onDryRun: (receipt) => {
      autoScrubDryRunReceipt = receipt;
      autoScrubExecutionReceipt = null;
      render();
    },
  });
  autoScrubExecutionReceipt = result.execution;
}

function clearPrivacyScanState(): void {
  privacyScanResult = null;
  privacyScanFileName = null;
  privacyScanStatus = null;
  privacyCoverageReceipt = null;
  selectedScrubFindings.clear();
  scrubResultsPage = 0;
  scrubReviewOpen = false;
  scrubReviewPage = 0;
  autoScrubDryRunReceipt = null;
  autoScrubExecutionReceipt = null;
  autoScrubError = "";
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
  return `<section class="privacy-results" aria-live="polite"><header><div><strong>${matching.length} ${matching.length === 1 ? "suggestion" : "suggestions"}</strong><small>${privacyScanResult.messagesScanned} messages · ${privacyScanResult.attachmentsScanned} attachments scanned${privacyScanFileName ? ` · ${escapeHtml(privacyScanFileName)}` : ""}</small></div><span class="privacy-local-mark">LOCAL · ENCRYPTED</span></header>${scrubCoverageReceiptMarkup()}${selectionControls}${items || `<div class="empty-state"><strong>No suggestions in the categories you chose</strong><p>OSL can miss things. Review important chats yourself too.</p></div>`}${pagination}${items ? `<footer class="scrub-review-footer"><span>${selected} selected</span><button class="button" id="review-scrub-selection" type="button" ${selected ? "" : "disabled"}>Review selected</button></footer>` : ""}</section>`;
}

function scrubFindingLabel(category: PersistedLocalPrivacyScanResult["findings"][number]["category"]): string {
  const group = scrubSignalGroupFor(category);
  return scrubSignalDefinitions.find((definition) => definition.id === group)?.label ?? "Review suggestion";
}

function scrubFindingMarkup(finding: PersistedLocalPrivacyScanResult["findings"][number], index: number, surface: "results" | "review"): string {
  const selected = selectedScrubFindings.has(index);
  const inputAttribute = surface === "review" ? "data-scrub-review-finding" : "data-scrub-finding";
  const sentCopy = finding.canRequestDelete
    ? "The file says you sent this. Check the exact message in the app."
    : "OSL cannot tell who sent this from the file. Check the exact message in the app.";
  const attachmentLocation = finding.attachmentPath ? ` · ${escapeHtml(finding.attachmentPath)}` : "";
  return `<article class="privacy-finding ${selected ? "selected" : ""}"><label class="scrub-finding-select"><input type="checkbox" ${inputAttribute}="${index}" ${selected ? "checked" : ""}/><strong>${escapeHtml(scrubFindingLabel(finding.category))}</strong></label><div class="scrub-finding-field"><span>Why OSL showed this</span><p>${escapeHtml(finding.reason)}</p></div><blockquote>${escapeHtml(finding.localPreview)}</blockquote><div class="scrub-finding-field"><span>Where to find it</span><p>${escapeHtml(finding.serviceId)} · ${escapeHtml(finding.conversationId)} · ${escapeHtml(finding.messageLocator)}${attachmentLocation}</p></div><div class="scrub-finding-field"><span>Check that you sent this</span><p>${sentCopy}</p></div></article>`;
}

function selectedScrubItems(): Array<{ finding: PersistedLocalPrivacyScanResult["findings"][number]; index: number }> {
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
  return `<dialog class="scrub-review-dialog" id="scrub-review-dialog" aria-labelledby="scrub-review-heading"><div class="scrub-review-card"><header><div><p class="eyebrow">Manual Scrub</p><h2 id="scrub-review-heading">Confirm your list</h2></div><button class="icon-button" id="close-scrub-review" type="button" aria-label="Close review">×</button></header><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Review every checked item before continuing.</p>${scrubCoverageReceiptMarkup()}<div class="scrub-review-summary"><strong>${selected.length} selected</strong><span>Nothing is deleted by this build.</span></div><div class="scrub-review-items">${items || `<div class="empty-state"><strong>Nothing selected</strong><p>Close this window and choose the messages you want to review.</p></div>`}</div>${pagination}<footer><p>Confirming only prepares manual directions. It does not contact or change any app.</p><div><button class="button ghost" id="close-scrub-review-footer" type="button">Back</button><button class="button primary" id="confirm-scrub-list" type="button" ${selected.length ? "" : "disabled"}>Confirm this list</button></div></footer></div></dialog>`;
}

function scrubCoverageReceiptMarkup(): string {
  if (!privacyCoverageReceipt || !validateCoverageReceipt(privacyCoverageReceipt)) return "";
  const receipt = privacyCoverageReceipt;
  const date = (value: number | null): string => value === null ? "Unknown" : new Date(value).toLocaleString();
  const complete = receipt.providerReportedComplete && receipt.gaps.length === 0 && receipt.uninspectedAttachments.length === 0;
  const types = receipt.attachmentTypesScanned.length ? receipt.attachmentTypesScanned.map(escapeHtml).join(", ") : "none";
  const uninspected = receipt.uninspectedAttachments.length
    ? `<div class="scrub-uninspected"><strong>Could not inspect</strong><ul>${receipt.uninspectedAttachments.map((attachment) => `<li><span>${escapeHtml(attachment.path)} · ${escapeHtml(attachment.detectedType)}</span><small>${escapeHtml(attachment.reason.replaceAll("_", " "))}: ${escapeHtml(attachment.detail)}</small></li>`).join("")}</ul></div>`
    : "";
  return `<section class="scrub-review-summary coverage-receipt" aria-label="Scan coverage receipt"><strong>Coverage: ${complete ? "complete" : "incomplete"}</strong><span>${receipt.messagesScanned} text messages · ${receipt.attachmentsScanned} attachments checked</span><span>Images: ${receipt.imagesChecked ? "deep-inspected" : "not deep-inspected"} · Videos: ${receipt.videosChecked ? "deep-inspected" : "not deep-inspected"}</span><span>Attachment types scanned: ${types}</span><span>Oldest reachable: ${escapeHtml(date(receipt.oldestReachableAtUnixMs))}</span><span>Newest reachable: ${escapeHtml(date(receipt.newestReachableAtUnixMs))}</span>${receipt.gaps.map((gap) => `<span>Gap: ${escapeHtml(gap)}</span>`).join("")}${uninspected}</section>`;
}

function scrubDeletionPathEnabled(): boolean {
  if (!privacyScanStatus) return false;
  return scrubDeletionAllowed({
    deletionEnabled: privacyScanStatus.deletionEnabled,
    mechanism: "none",
    stopOn: scrubDeletionContract.stopOn,
    requestedDeletionCountsAsVerified: false,
  });
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
    if (!scrubDeletionPathEnabled()) {
      showToast("Deletion is disabled. Use the original app to review any removal.");
      scrubReviewOpen = false;
      render();
      return;
    }
    scrubReviewOpen = false;
    render();
  });
  document.querySelector<HTMLSelectElement>("#autoscrub-account")?.addEventListener("change", async (event) => {
    autoScrubAccountId = (event.currentTarget as HTMLSelectElement).value;
    autoScrubCapabilities = unavailableAutoScrubCapabilities;
    autoScrubDryRunReceipt = null;
    autoScrubExecutionReceipt = null;
    await refreshAutoScrubCapability();
    render();
  });
  document.querySelector<HTMLSelectElement>("#autoscrub-path")?.addEventListener("change", async (event) => {
    const value = (event.currentTarget as HTMLSelectElement).value as AutoScrubProviderId;
    if (!autoScrubCapabilities.some((capability) => capability.providerId === value)) return;
    autoScrubPathId = value;
    autoScrubDryRunReceipt = null;
    autoScrubExecutionReceipt = null;
    autoScrubError = "";
    await refreshAutoScrubCapability();
    render();
  });
  document.querySelector<HTMLButtonElement>("#refresh-autoscrub")?.addEventListener("click", async () => {
    if (autoScrubBusy) return;
    autoScrubBusy = true;
    autoScrubError = "";
    render();
    await refreshAutoScrubCapability();
    autoScrubBusy = false;
    render();
  });
  document.querySelector<HTMLFormElement>("#autoscrub-imap-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (autoScrubBusy) return;
    const form = event.currentTarget as HTMLFormElement;
    const accountId = form.querySelector<HTMLSelectElement>("#autoscrub-imap-account")?.value ?? "";
    const host = form.querySelector<HTMLInputElement>("#autoscrub-imap-host")?.value.trim() ?? "";
    const username = form.querySelector<HTMLInputElement>("#autoscrub-imap-username")?.value.trim() ?? "";
    const authKind = form.querySelector<HTMLSelectElement>("#autoscrub-imap-auth-kind")?.value === "oauthBearer" ? "oauthBearer" as const : "appPassword" as const;
    const secretInput = form.querySelector<HTMLInputElement>("#autoscrub-imap-secret");
    const mailbox = form.querySelector<HTMLInputElement>("#autoscrub-imap-mailbox")?.value.trim() ?? "Sent";
    let secret = secretInput?.value ?? "";
    if (secretInput) secretInput.value = "";
    if (!accountId || !host || !username || !secret) return;
    autoScrubBusy = true;
    autoScrubError = "";
    render();
    try {
      const result = await configureScrubImapAccount({ accountId, host, username, auth: { kind: authKind, secret }, defaultMailbox: mailbox });
      secret = "";
      if (!result.configured || !result.liveConfirmed) throw new Error(result.detail || "IMAP connection was not live-confirmed");
      autoScrubAccountId = accountId;
      autoScrubPathId = "imap";
      await refreshAutoScrubCapability();
    } catch (error) {
      secret = "";
      autoScrubError = error instanceof Error ? error.message : "IMAP connection could not be verified";
    } finally {
      autoScrubBusy = false;
      render();
    }
  });
  document.querySelector<HTMLButtonElement>("#run-autoscrub")?.addEventListener("click", async () => {
    if (autoScrubBusy) return;
    const confirmed = document.querySelector<HTMLInputElement>("#autoscrub-final-confirmation")?.checked === true;
    autoScrubBusy = true;
    autoScrubError = "";
    autoScrubDryRunReceipt = null;
    autoScrubExecutionReceipt = null;
    render();
    try {
      await executeSelectedAutoScrubBatch(confirmed);
    } catch (error) {
      autoScrubError = error instanceof Error ? error.message : "AutoScrub stopped without a verified result";
    } finally {
      autoScrubBusy = false;
      render();
    }
  });
}

function notificationSettingsContent(): string {
  const apps = orderedServices().filter((service) => service.category === "consumer").map((service) => `<label class="notification-app-row">${serviceLogo(service.id)}<span><strong>${escapeHtml(service.displayName)}</strong><small>Unread access is not supported yet</small></span><input type="checkbox" data-notification-app="${service.id}" ${notificationAppPreferences[service.id] !== false ? "checked" : ""}/></label>`).join("");
  return `<h2>Notifications</h2><p>Local OSL activity only. App unread counts are not supported yet.</p><div class="settings-list"><label class="setting-line interactive"><span><strong>Local OSL activity</strong><small>Security and app-connection events on this device.</small></span><input id="notifications-opt-in" type="checkbox" ${notificationsEnabled ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show details</strong><small>Off by default. When off, Activity hides event content.</small></span><input id="notification-previews" type="checkbox" ${notificationPreviewContent ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Suggest chat approval</strong><small>Suggestions never enable decryption.</small></span><input id="notification-scope-suggestions" type="checkbox" ${notificationScopeSuggestions ? "checked" : ""}/></label></div><details class="settings-disclosure notification-apps"><summary><span><strong>Apps</strong><small>For future unread support</small></span></summary><div class="notification-app-list">${apps}</div></details>`;
}

function identitySettingsContent(): string {
  const identities = hubIdentities.length
    ? hubIdentities.map((identity) => `<article class="identity-row"><div><strong>${escapeHtml(identity.label)}</strong><small>${escapeHtml(identity.oslUserId)} · ${escapeHtml(identity.safetyNumber)}</small></div>${identity.active ? `<span class="status-tag">Active</span>` : `<button class="button compact" data-switch-identity="${escapeHtml(identity.slotId)}">Switch</button>`}</article>`).join("")
    : `<div class="empty-state"><strong>Identity list unavailable</strong><p>Unlock OSL to manage encrypted identity slots.</p></div>`;
  const recovery = newIdentityRecoveryPhrase ? `<div class="warning recovery-secret"><strong>Save the new identity recovery phrase now</strong><code>${escapeHtml(newIdentityRecoveryPhrase)}</code><p>Visible only on this page. It clears if you leave or hide OSL.</p></div>` : "";
  return `<h2>Account</h2><p>One active identity on this device.</p><div class="identity-list">${identities}</div>${recovery}<form class="inline-form identity-create-form" id="identity-slot-form"><input id="identity-slot-label" maxlength="80" placeholder="New identity label" required/><button class="button primary">Create identity</button></form><details class="recovery-import settings-disclosure"><summary>Recover another identity</summary><form id="identity-recover-form" class="setup-surface"><input id="identity-recover-label" maxlength="80" placeholder="Identity label" required/><textarea id="identity-recover-phrase" rows="3" placeholder="12-word recovery phrase" required></textarea><button class="button">Recover identity</button></form></details>${activationSettingsContent()}`;
}

function activationSettingsContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const accessLabel = licenseState.access === "offlineGrace" ? "Pro, offline grace" : pro ? "Pro active" : "Free";
  const period = licenseState.currentPeriodEnd === null ? "" : `<small>${licenseState.status === "CANCELLED" ? "Access through" : "Current period ends"} ${formatUnixDate(licenseState.currentPeriodEnd)}</small>`;
  const clear = licenseState.status === "UNCONFIGURED" ? "" : `<button class="button compact" id="clear-activation-code" type="button">Clear activation</button>`;
  return `<details class="license-card settings-disclosure"><summary><span><strong>Plan</strong><small>${accessLabel}</small>${period}</span><span class="status-tag ${pro ? "active" : ""}">${escapeHtml(licenseState.status === "UNCONFIGURED" ? "Free" : licenseState.status)}</span></summary><div><p>Paste the activation code shown after checkout. No email is required.</p><form id="activation-form" class="license-form"><label for="activation-code">Activation code</label><div><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Activate Pro</button>${clear}</div></form></div></details>`;
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
  activeProtectedContextKind = null;
  localProtectedSheet = blankLocalProtectedModel();
  peerProtectedSheet = blankPeerProtectedModel();
  protectedSheetMode = "peer";
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
  if (localProtectedSheet.open || peerProtectedSheet.open) {
    localProtectedSheet = blankLocalProtectedModel();
    peerProtectedSheet = blankPeerProtectedModel();
    protectedSheetMode = "peer";
    activeContextToken = null;
    activeProtectedContextKind = null;
    render();
    await setLocalProtectedSheetOpen(false);
    return;
  }
  if (!(await setLocalProtectedSheetOpen(true))) {
    showToast("Protection could not open safely");
    return;
  }
  protectedSheetMode = "peer";
  peerProtectedSheet = blankPeerProtectedModel(true);
  localProtectedSheet = blankLocalProtectedModel();
  activeContextToken = null;
  activeProtectedContextKind = null;
  render();
}

function showLocalProtectedChoice(): void {
  protectedSheetMode = "local";
  peerProtectedSheet = blankPeerProtectedModel();
  localProtectedSheet = blankLocalProtectedModel(true);
  activeContextToken = null;
  activeProtectedContextKind = null;
  render();
}

function showPeerProtectedChoice(): void {
  protectedSheetMode = "peer";
  peerProtectedSheet = blankPeerProtectedModel(true);
  localProtectedSheet = blankLocalProtectedModel();
  activeContextToken = null;
  activeProtectedContextKind = null;
  render();
}

function isCurrentPeerContext(contextToken: string): boolean {
  return protectedSheetMode === "peer"
    && peerProtectedSheet.open
    && peerProtectedSheet.context?.contextToken === contextToken;
}

async function choosePeerProtectedFriend(personId: string): Promise<void> {
  if (!activeEmbeddedHost || peerProtectedSheet.busy) return;
  const host = activeEmbeddedHost;
  const person = hubPeople.find((candidate) => candidate.personId === personId
    && candidate.safetyNumberVerified
    && !candidate.pendingKeyChange);
  if (!person) {
    peerProtectedSheet.status = "Verify this friend first.";
    render();
    return;
  }
  peerProtectedSheet.busy = true;
  peerProtectedSheet.status = "";
  render();
  const context = await activateManualPeerContext(
    host.serviceId,
    host.accountId,
    person.personId,
  );
  if (protectedSheetMode !== "peer"
    || !peerProtectedSheet.open
    || activeEmbeddedHost?.serviceId !== host.serviceId
    || activeEmbeddedHost.accountId !== host.accountId) return;
  peerProtectedSheet.busy = false;
  if (!context) {
    peerProtectedSheet.status = "This app + friend could not be activated safely.";
    render();
    return;
  }
  peerProtectedSheet.context = context;
  peerProtectedSheet.personId = person.personId;
  peerProtectedSheet.displayName = person.alias ?? "Verified friend";
  activeContextToken = context.contextToken;
  activeProtectedContextKind = "peer";
  const security = await loadActiveContextSecurity(context.contextToken);
  if (!isCurrentPeerContext(context.contextToken)) return;
  if (security && isLocalTtlSeconds(security.ttlSeconds)) {
    peerProtectedSheet.ttlSeconds = security.ttlSeconds;
    peerProtectedSheet.decryptDisplayEnabled = security.decryptDisplayEnabled;
  }
  peerProtectedSheet.status = context.scopeApproved ? "Ready." : "";
  render();
}

async function approvePeerProtectedDm(): Promise<void> {
  const context = peerProtectedSheet.context;
  if (!context || context.scopeApproved || peerProtectedSheet.busy) return;
  peerProtectedSheet.busy = true;
  peerProtectedSheet.status = "";
  render();
  const saved = await setActiveHubFriendPermission(context.contextToken, context.personId, true, false);
  if (!isCurrentPeerContext(context.contextToken)) return;
  peerProtectedSheet.busy = false;
  if (!saved) {
    peerProtectedSheet.status = "Approval could not be saved.";
    render();
    return;
  }
  peerProtectedSheet.context = { ...context, scopeApproved: true };
  peerProtectedSheet.status = "Approved for this app + friend.";
  hubPeople = await listHubPeople() ?? hubPeople;
  render();
}

async function preparePeerProtectedDraft(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const context = peerProtectedSheet.context;
  const draft = document.querySelector<HTMLTextAreaElement>("#peer-protected-draft");
  const ttl = document.querySelector<HTMLSelectElement>("#peer-protected-ttl");
  const plaintext = draft?.value ?? "";
  const requestedTtl = Number(ttl?.value ?? 3_600);
  if (!context?.scopeApproved || !isHubPlaintext(plaintext) || !isLocalTtlSeconds(requestedTtl)) {
    peerProtectedSheet.status = "Write a message first.";
    render();
    return;
  }
  peerProtectedSheet.busy = true;
  peerProtectedSheet.draft = plaintext;
  peerProtectedSheet.status = "";
  render();
  const policy = await saveActiveContextSecurity(context.contextToken, requestedTtl, peerProtectedSheet.decryptDisplayEnabled);
  if (!isCurrentPeerContext(context.contextToken)) return;
  if (!policy || !isLocalTtlSeconds(policy.ttlSeconds)) {
    peerProtectedSheet.busy = false;
    peerProtectedSheet.status = "Encryption failed closed. Nothing was copied.";
    render();
    return;
  }
  peerProtectedSheet.ttlSeconds = policy.ttlSeconds;
  const prepared = await preparePeerProseText(context.contextToken, plaintext);
  if (!isCurrentPeerContext(context.contextToken)) return;
  peerProtectedSheet.busy = false;
  if (!prepared) {
    peerProtectedSheet.status = "Encryption failed closed. Nothing was copied.";
    render();
    return;
  }
  peerProtectedSheet.coverText = prepared.coverText;
  try {
    await navigator.clipboard.writeText(prepared.coverText);
    peerProtectedSheet.status = "Encrypted and copied. OSL did not press Send.";
  } catch {
    peerProtectedSheet.status = "Encrypted. Select the protected text to copy it.";
  }
  render();
}

async function openPeerProtectedText(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const context = peerProtectedSheet.context;
  const input = document.querySelector<HTMLTextAreaElement>("#peer-cover-input");
  const coverText = input?.value.trim() ?? "";
  if (!context?.scopeApproved || !coverText) {
    peerProtectedSheet.status = "Paste protected text first.";
    render();
    return;
  }
  if (!peerProtectedSheet.decryptDisplayEnabled) {
    peerProtectedSheet.status = "Decrypted display is off for this app + friend.";
    render();
    return;
  }
  peerProtectedSheet.busy = true;
  peerProtectedSheet.openedPlaintext = "";
  peerProtectedSheet.status = "";
  render();
  const opened = await openPeerProseText(context.contextToken, context.personId, coverText);
  if (!isCurrentPeerContext(context.contextToken)) return;
  peerProtectedSheet.busy = false;
  if (!opened) {
    peerProtectedSheet.status = "This protected text could not be opened here.";
    render();
    return;
  }
  peerProtectedSheet.openedPlaintext = opened.plaintext;
  peerProtectedSheet.status = "Opened here.";
  render();
}

async function changePeerDecryptDisplay(input: HTMLInputElement): Promise<void> {
  const context = peerProtectedSheet.context;
  if (!context?.scopeApproved) {
    input.checked = peerProtectedSheet.decryptDisplayEnabled;
    return;
  }
  const saved = await saveActiveContextSecurity(context.contextToken, peerProtectedSheet.ttlSeconds, input.checked);
  if (!isCurrentPeerContext(context.contextToken)) return;
  if (!saved || !isLocalTtlSeconds(saved.ttlSeconds)) {
    input.checked = peerProtectedSheet.decryptDisplayEnabled;
    peerProtectedSheet.status = "This app + friend setting could not be saved.";
    render();
    return;
  }
  peerProtectedSheet.ttlSeconds = saved.ttlSeconds;
  peerProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled;
  if (!saved.decryptDisplayEnabled) peerProtectedSheet.openedPlaintext = "";
  peerProtectedSheet.status = saved.decryptDisplayEnabled ? "Decrypted display is on." : "Decrypted display is off.";
  render();
}

async function copyPeerProtectedText(): Promise<void> {
  if (!peerProtectedSheet.coverText) return;
  try {
    await navigator.clipboard.writeText(peerProtectedSheet.coverText);
    peerProtectedSheet.status = "Copied. Paste and send it yourself.";
  } catch {
    peerProtectedSheet.status = "Copy failed. Select the protected text manually.";
  }
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
    activeProtectedContextKind = "local";
    const security = await loadActiveContextSecurity(context.contextToken);
    if (security) {
      localProtectedSheet.ttlSeconds = security.ttlSeconds;
      localProtectedSheet.decryptDisplayEnabled = security.decryptDisplayEnabled;
    }
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
  const ttlSeconds = Number(ttl?.value ?? 3_600);
  if (!contextToken || !plaintext.trim() || !isLocalTtlSeconds(ttlSeconds)) {
    localProtectedSheet.status = "Write a message first.";
    render();
    return;
  }
  const sendContext = localProtectedSheet.context;
  if ((setup.sendMode === "double" || setup.sendMode === "single")
    && sendContext
    && !hasExperimentalSendConsent(setup.sendMode, sendContext.serviceId, sendContext.accountId)) {
    const accepted = window.confirm(`${formatSendMode(setup.sendMode)} is experimental for this account. OSL will send nothing unless it can verify the exact account, chat, and composer immediately before every action. Continue with safe Copy fallback?`);
    if (!accepted) {
      localProtectedSheet.draft = plaintext;
      localProtectedSheet.status = "Cancelled. Your draft is still here and nothing was sent.";
      render();
      return;
    }
    rememberExperimentalSendConsent(setup.sendMode, sendContext.serviceId, sendContext.accountId);
  }
  localProtectedSheet.busy = true;
  localProtectedSheet.draft = plaintext;
  localProtectedSheet.viewOnce = viewOnce?.checked === true;
  localProtectedSheet.status = "";
  render();
  const policy = await saveActiveContextSecurity(contextToken, ttlSeconds, localProtectedSheet.decryptDisplayEnabled);
  if (!policy || !isLocalTtlSeconds(policy.ttlSeconds)) {
    localProtectedSheet.busy = false;
    localProtectedSheet.status = "Encryption failed closed. Nothing was sent.";
    render();
    return;
  }
  localProtectedSheet.ttlSeconds = policy.ttlSeconds;
  const prepared = await prepareLocalProtectedText(contextToken, plaintext, localProtectedSheet.viewOnce);
  localProtectedSheet.busy = false;
  if (!prepared) {
    localProtectedSheet.status = "Encryption failed closed. Nothing was sent.";
    render();
    return;
  }
  localProtectedSheet.capsule = prepared.capsule;
  try {
    await navigator.clipboard.writeText(prepared.capsule);
    localProtectedSheet.status = setup.sendMode === "double" || setup.sendMode === "single"
      ? "Exact composer verification is unavailable here. Copied safely; nothing was sent."
      : "Encrypted and copied. OSL did not press Send.";
  } catch {
    localProtectedSheet.status = "Encrypted. Automatic copy failed; select the encrypted text below. Nothing was sent.";
  }
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
  if (!localProtectedSheet.decryptDisplayEnabled) {
    localProtectedSheet.status = "Decrypted display is off for this local chat.";
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

async function changeLocalDecryptDisplay(input: HTMLInputElement): Promise<void> {
  const contextToken = localProtectedSheet.context?.contextToken;
  if (!contextToken) {
    input.checked = localProtectedSheet.decryptDisplayEnabled;
    return;
  }
  const saved = await saveActiveContextSecurity(contextToken, localProtectedSheet.ttlSeconds, input.checked);
  if (!saved) {
    input.checked = localProtectedSheet.decryptDisplayEnabled;
    localProtectedSheet.status = "This chat setting could not be saved.";
    render();
    return;
  }
  localProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled;
  if (!saved.decryptDisplayEnabled) localProtectedSheet.openedPlaintext = "";
  localProtectedSheet.status = saved.decryptDisplayEnabled ? "Decrypted display is on for this local chat." : "Decrypted display is off for this local chat.";
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
  document.querySelector<HTMLButtonElement>("#protect-local-only")?.addEventListener("click", showLocalProtectedChoice);
  document.querySelector<HTMLButtonElement>("#peer-protected-back")?.addEventListener("click", showPeerProtectedChoice);
  document.querySelector<HTMLButtonElement>("#peer-approve")?.addEventListener("click", () => void approvePeerProtectedDm());
  document.querySelectorAll<HTMLButtonElement>("[data-peer-person]").forEach((button) => button.addEventListener("click", () => void choosePeerProtectedFriend(button.dataset.peerPerson ?? "")));
  document.querySelector<HTMLFormElement>("#peer-protect-form")?.addEventListener("submit", (event) => void preparePeerProtectedDraft(event));
  document.querySelector<HTMLFormElement>("#peer-open-form")?.addEventListener("submit", (event) => void openPeerProtectedText(event));
  document.querySelector<HTMLButtonElement>("#peer-cover-copy")?.addEventListener("click", () => void copyPeerProtectedText());
  document.querySelector<HTMLInputElement>("#peer-decrypt-display")?.addEventListener("change", (event) => void changePeerDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelectorAll<HTMLButtonElement>("[data-peer-pane]").forEach((button) => button.addEventListener("click", () => {
    peerProtectedSheet.pane = button.dataset.peerPane as PeerProtectedPane;
    peerProtectedSheet.openedPlaintext = "";
    peerProtectedSheet.status = "";
    render();
  }));
  document.querySelector<HTMLFormElement>("#local-context-form")?.addEventListener("submit", (event) => void startLocalProtectedContext(event));
  document.querySelector<HTMLFormElement>("#local-protect-form")?.addEventListener("submit", (event) => void prepareLocalProtectedDraft(event));
  document.querySelector<HTMLFormElement>("#local-open-form")?.addEventListener("submit", (event) => void openLocalProtectedCapsule(event));
  document.querySelector<HTMLButtonElement>("#local-capsule-copy")?.addEventListener("click", () => void copyLocalProtectedCapsule());
  document.querySelector<HTMLInputElement>("#local-decrypt-display")?.addEventListener("change", (event) => void changeLocalDecryptDisplay(event.currentTarget as HTMLInputElement));
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
    const requestedRoute = button.dataset.route as Route;
    await Promise.resolve();
    if (intent !== navigationIntentEpoch) return;
    if (activeEmbeddedHost || activeNativeHostId) await closeActiveServiceSurface();
    if (route === "settings" && settingsSection === "scrub") clearPrivacyScanState();
    if (route === "settings" && settingsSection === "account") newIdentityRecoveryPhrase = null;
    if (onboardingServiceSetup && requestedRoute === "home") {
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "import";
    } else {
      route = requestedRoute;
    }
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
  document.querySelectorAll<HTMLButtonElement>("[data-settings-send-mode]").forEach((button) => button.addEventListener("click", () => {
    void changeSendingMode(button.dataset.settingsSendMode as SendMode);
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"; settingsSection = "notifications"; render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#privacy-export-input")?.addEventListener("change", (event) => void scanPrivacyExport(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLButtonElement>("#clear-privacy-scan")?.addEventListener("click", () => { clearPrivacyScanState(); render(); });
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
    localStorage.setItem(themeStorageKey, next);
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
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "import";
      activeService = null;
      activeHomeAppId = null;
      render();
      return;
    }
    clearServiceGuide();
    render();
  });
  document.querySelector("#service-guide-finish")?.addEventListener("click", () => {
    if (onboardingServiceSetup) {
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "import";
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
      clearServiceOnboardingResume();
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "detected";
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
    const native = selectedInstalledNativeApp(app.id);
    if (native) {
      void openNativeHostedApp(app, service, native.id);
    } else if (savedAccountsReady && firefoxStatus.availability === "installed" && importedFirefoxHomeAppIds.has(app.id)) {
      try {
        await launchFirefoxService(app.id);
        showToast(`${app.displayName} opened in your isolated OSL Firefox profile`);
      } catch {
        if (app.linked) void openEmbeddedApp(app, service);
        else openServiceRoute(service, app.provider, app.id, true);
      }
    } else if (app.linked) {
      void openEmbeddedApp(app, service);
    } else if (savedAccountMode !== "ask") {
      activeService = service;
      activeHomeAppId = app.id;
      route = "service";
      serviceGuideStep = null;
      void setupEmbeddedApp();
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
      const installed = nativeApps.find((candidate) => candidate.id === appId && candidate.availability === "installed");
      if (installed) {
        if (installed.isolatedProfileAvailable) savedNativeApps.add(appId);
        persistSavedAccountPreferences();
        showToast(installed.isolatedProfileAvailable ? `${app.displayName} is ready` : `${app.displayName} installed; OSL will use an isolated web profile`);
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

async function openSafeEmbeddedFallback(app: HomeAppCatalogEntry): Promise<void> {
  const existingProfiles = embeddedAccountsForHomeApp(app, services);
  const opened = existingProfiles.length > 0
    ? { host: await openEmbeddedHomeApp(app, services) }
    : await setupEmbeddedHomeApp(app, existingProfiles.length === 0 ? "Personal" : `Profile ${existingProfiles.length + 1}`);
  activeEmbeddedHost = opened.host;
  markServiceOnboardingOpened();
  activeNativeHostId = null;
  serviceAccountPickerOpen = false;
  services = await loadLinkedServices().catch(() => services);
  serviceGuideStep = null;
  localStorage.removeItem(serviceGuideStorageKey);
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
      if (result.reason === "secondaryInstanceUnverified") {
        await openSafeEmbeddedFallback(app);
        showToast(`${app.displayName} opened in a separate OSL web profile; the normal app stayed open`);
        return;
      }
      serviceGuideStep = 0;
      showToast(nativeHostFailureMessage(result.reason, app.displayName));
      return;
    }
    activeNativeHostId = appId;
    markServiceOnboardingOpened();
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
    const native = selectedInstalledNativeApp(app.id);
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
    markServiceOnboardingOpened();
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
    const native = selectedInstalledNativeApp(app.id);
    if (native) {
      nativeActionBusy = false;
      await openNativeHostedApp(app, service, native.id);
      return;
    }
    activeEmbeddedHost = await openEmbeddedHomeApp(app, services, accountId);
    markServiceOnboardingOpened();
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
  route = "onboarding";
  onboardingRoute = "import";
  activeService = null;
  activeHomeAppId = null;
  render();
  void refreshBrowserImportReadiness();
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
    const contextKind = activeProtectedContextKind;
    if (!contextToken || !(await burnActiveHubContext(contextToken))) {
      burnBusy = false;
      burnResult = { tone: "error", message: "The chat burn failed closed. No deletion success is being claimed.", showUninstall: false };
      render();
      return;
    }
    burnBusy = false;
    resetLocalProtectedSheet();
    burnResult = {
      tone: "success",
      message: contextKind === "peer"
        ? "Local approval, display, and expiry settings for this app account + friend were revoked. OSL attempted relay cleanup. Provider messages and opened copies remain."
        : "Local OSL decrypt material and caches for this chat were removed. Native app history was not deleted.",
      showUninstall: false,
    };
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
        ? `Local OSL settings and caches for ${result.scopesBurned} indexed ${result.scopesBurned === 1 ? "scope was" : "scopes were"} removed. Sent relay cleanup was acknowledged. Login profile, cookies, provider history, and other copies remain.`
        : `Local OSL settings and caches for ${result.scopesBurned} indexed ${result.scopesBurned === 1 ? "scope was" : "scopes were"} removed, but ${result.remoteBlobDeletionsFailed} sent relay blob ${result.remoteBlobDeletionsFailed === 1 ? "deletion was" : "deletions were"} not acknowledged. Login profile, cookies, provider history, and other copies remain.`,
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
  return `<h2>About</h2><div class="update-status-card"><span class="dot"></span><div><strong>${status}</strong><small>Signed local updater · no UI telemetry</small></div></div><div class="settings-actions"><button class="button ${updateStatus.state === "available" ? "" : "primary"}" data-update-check ${updateStatus.state === "checking" || updateStatus.state === "installing" ? "disabled" : ""}>Check for updates</button>${actions}</div><details class="settings-disclosure update-details"><summary>Update privacy</summary><p>Checks and installs use the trusted local updater. Release notes are plain text; remote HTML is never rendered.</p></details><details class="device-diagnostics settings-disclosure"><summary><span><strong>Device status</strong><small>${deviceReady ? "Ready" : "Needs attention"}</small></span></summary><p>${escapeHtml(coreReadinessLabel(core.readiness))}</p></details>`;
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
  root.innerHTML = `<div class="app-frame">${desktopTitlebar()}<main class="loading-screen"><div class="loading-seal" aria-hidden="true"><img class="osl-logo loading-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><span class="sr-only">Opening OSL</span></main></div>`;
  bindDesktopTitlebar();
  try {
    const coreIntegration = await withNativeDeadline(loadCoreIntegration(), "Start OSL", bootCoreDeadlineMs);
    if (attempt !== bootstrapEpoch) return;
    if (!usableBootCore(coreIntegration)) {
      showBootstrapRecovery();
      return;
    }
    core = coreIntegration;
    refreshActiveBrowserAccountsReady();
    const preferencesRequest = withNativeDeadline(loadOnboardingPreferences(), "Load OSL preferences", bootPreferenceDeadlineMs).catch(() => null);
    const servicesRequest = withNativeDeadline(loadLinkedServices(), "Load apps", bootSupportDeadlineMs).catch(() => null);
    const nativeAppsRequest = savedAccountMode === "use"
      ? withNativeDeadline(loadNativeApps(), "Load selected Windows apps", bootSupportDeadlineMs).catch(() => null)
      : Promise.resolve(null);
    const firefoxRequest = savedAccountsReady
      ? withNativeDeadline(loadFirefoxStatus(), "Check selected Firefox profile", bootSupportDeadlineMs).catch(() => null)
      : Promise.resolve(null);
    const licenseRequest = withNativeDeadline(loadHubLicenseState(), "Load plan", bootSupportDeadlineMs).catch(() => null);
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
      if (!preferences.onboardingComplete) onboardingRoute = pendingOnboardingRoute() ?? onboardingRoute;
    }
    const previewRoute = designPreviewRouteFromHash();
    if (previewRoute) prepareDesignPreviewRoute(previewRoute, designPreviewProEnabled());
    renderNow();
    if (route === "onboarding" && onboardingRoute === "import") void refreshBrowserImportReadiness();
    if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    startReadyWorkspaceLoads();
    void Promise.all([servicesRequest, nativeAppsRequest, firefoxRequest, licenseRequest]).then(([linkedServices, nativeCatalog, currentFirefoxStatus, currentLicenseState]) => {
      if (attempt !== bootstrapEpoch) return;
      if (linkedServices) services = linkedServices;
      if (nativeCatalog && isCompleteNativeCatalog(nativeCatalog)) {
        nativeApps = nativeCatalog;
      }
      if (currentFirefoxStatus) firefoxStatus = currentFirefoxStatus;
      if (currentLicenseState) licenseState = currentLicenseState;
      loadScrubSetupPlan();
      route === "onboarding" ? render() : renderWhenIdle();
    });
  } catch {
    if (attempt === bootstrapEpoch) showBootstrapRecovery();
    return;
  }
}

window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => { if (themeChoice === "system") applyTheme("system"); });
window.addEventListener("hashchange", () => {
  const previewRoute = designPreviewRouteFromHash();
  if (previewRoute) applyDesignPreviewRoute(previewRoute, designPreviewProEnabled());
});
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
