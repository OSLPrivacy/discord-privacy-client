import "@fontsource-variable/inter/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
  installMullvad,
  loadFirefoxStatus,
  loadLinkedServices,
  launchFirefoxService,
  loadMullvadStatus,
  loadNativeApps,
  openMullvad,
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
  type MullvadStatus,
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
import { activateLocalLoopbackContext, activateManualPeerContext, activateOslChatContext, addOslFriend, addOslFriendByUsername, burnActiveHubContext, burnHubServiceAccount, claimOslUsername, closeOslChatContext, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubServiceBurnReadiness, isHubPlaintext, isNormalizedOslUsername, listHubIdentities, listHubPeople, listOslChatHistory, loadActiveContextSecurity, loadAppNotifications, loadFriendProfile, loadOslProfile, openOslChatText, openPeerProseText, prepareLocalProtectedText, prepareOslChatText, preparePeerProseText, recoverHubIdentitySlot, saveActiveContextSecurity, saveOslProfile, scanLocalPrivacy, setActiveHubFriendPermission, setHubFriendNickname, setLocalProtectedSheetOpen, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, type AppNotification, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type LocalPrivacyScanResult, type ManualPeerContext, type OslChatOpenedBatch, type OslProfile, type OslProfileEffect, type OslProfileFrame } from "./adapters";
import { blankLocalProtectedModel, isLocalTtlSeconds, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import { blankPeerProtectedModel, peerProtectedSheetMarkup, type PeerProtectedPane, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { withNativeDeadline } from "./native-deadline";
import { FrameRenderScheduler } from "./render-scheduler";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { getScrubIndexStatus, initializeScrubIndex, type ScrubAccountSelection, type ScrubIndexStatus } from "./scrub-index";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";
import { initializeThemePreference, themeStorageKey, type ThemeChoice } from "./theme-preference";
import { applyAccessibilityPreferences, loadAccessibilityPreferences, saveAccessibilityPreferences, type AccessibilityPreferences, type TextScale } from "./accessibility-preference";
import { applyThemeMod, parseThemeMod, themeModStorageKey, type ThemeMod } from "./theme-mod";
import { oslChatsViewMarkup, type OslChatMessage } from "./osl-chats-view";

type Route = "onboarding" | "home" | "service" | "settings" | "osl-chat";
type OnboardingRoute = "welcome" | "create" | "import" | "unlock" | "recovery" | "tutorial" | "detected" | "install" | "apps" | "browser" | "mullvad" | "sending" | "passwords" | "burnpass" | "privacy" | "scrub" | "decoy";
type SettingsSection = "account" | "apps" | "scrub" | "cleanup" | "notifications" | "appearance" | "accessibility" | "developer" | "about";
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
  const finalStep = mode === "double" ? "Enter again" : mode === "single" ? "Recheck & send" : "You send";
  return `<div class="manual-send-demo" role="img" aria-label="OSL encrypts on this device, verifies the destination, and fails closed if anything changes."><span>Write</span><i aria-hidden="true"></i><span>Encrypt</span><i aria-hidden="true"></i><span>${mode === "clipboard" || mode === "manual" ? "Copy" : "Verify"}</span><i aria-hidden="true"></i><span>${finalStep}</span></div>`;
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
let mullvadStatus: MullvadStatus = { availability: "unavailable" };
let mullvadBusy = false;
let mullvadConnectedConfirmed = false;
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
let onboardingConnectAppId: HomeAppId | null = null;
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
let accessibilityPreferences = loadAccessibilityPreferences(localStorage);
let activeThemeMod: ThemeMod | null = parseThemeMod(localStorage.getItem(themeModStorageKey));
let oslProfile: OslProfile | null = null;
let claimedOslUsername: string | null = null;
let profileDraftAvatar: string | null | undefined;
let profileSaving = false;
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
let notificationChatActivity = true;
let notificationSecurityActivity = true;
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
let privacyScanResult: LocalPrivacyScanResult | null = null;
let privacyScanFileName: string | null = null;
let privacyScanBusy = false;
let enabledScrubSignals = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
let selectedScrubFindings = new Set<number>();
let scrubResultsPage = 0;
let scrubReviewOpen = false;
let scrubReviewPage = 0;
let scrubIndexStatus: ScrubIndexStatus | null = null;
let scrubIndexBusy = false;
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
let activeOslChatPersonId: string | null = null;
let activeOslChatContext: ManualPeerContext | null = null;
let oslChatDraft = "";
let oslChatViewOnce = false;
let oslChatBusy = false;
let oslChatBackgroundBusy = false;
let oslChatOperationEpoch = 0;
const oslChatMessages = new Map<string, OslChatMessage[]>();
const oslChatUnread = new Map<string, number>();
let oslChatMutedPeople = new Set<string>();
let oslChatRemoteAccessConfirmed = new Set<string>();
let friendDefaultOslChatEnabled = false;
let oslChatSettingsPersonId: string | null = null;

const sidebarStorageKey = "osl-hub-sidebar";
const hiddenStorageKey = "osl-hub-sidebar-hidden";
const notificationsStorageKey = "osl-hub-notifications";
const notificationAppsStorageKey = "osl-hub-notification-apps";
const notificationPreviewStorageKey = "osl-hub-notification-previews";
const notificationScopeStorageKey = "osl-hub-notification-scope-suggestions";
const notificationChatStorageKey = "osl-hub-notification-chats-v1";
const notificationSecurityStorageKey = "osl-hub-notification-security-v1";
const screenshotProtectionStorageKey = "osl-hub-screenshot-protection";
const scrubSignalsStorageKey = "osl-hub-scrub-signals-v1";
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
const oslChatMutedStorageKey = "osl-chat-muted-people-v1";
const oslChatUnreadStorageKey = "osl-chat-unread-v1";
const oslChatRemoteAccessStorageKey = "osl-chat-remote-access-v1";
const friendDefaultOslChatStorageKey = "osl-friend-default-chat-v1";
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
  return localStorage.getItem(onboardingResumeStorageKey) === "browser" ? "browser" : null;
}

function beginServiceOnboarding(): void {
  onboardingServiceSetup = true;
  localStorage.removeItem(onboardingResumeStorageKey);
}

function markServiceOnboardingOpened(): void {
  if (!onboardingServiceSetup) return;
  localStorage.setItem(onboardingResumeStorageKey, "browser");
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
    const mutedPeople = JSON.parse(localStorage.getItem(oslChatMutedStorageKey) ?? "[]") as unknown;
    if (Array.isArray(mutedPeople)) oslChatMutedPeople = new Set(mutedPeople.filter((personId): personId is string => typeof personId === "string" && personId.length > 0 && personId.length <= 180).slice(0, 512));
    const remoteAccess = JSON.parse(localStorage.getItem(oslChatRemoteAccessStorageKey) ?? "[]") as unknown;
    if (Array.isArray(remoteAccess)) oslChatRemoteAccessConfirmed = new Set(remoteAccess.filter((personId): personId is string => typeof personId === "string" && personId.length > 0 && personId.length <= 180).slice(0, 512));
    const unread = JSON.parse(localStorage.getItem(oslChatUnreadStorageKey) ?? "{}") as unknown;
    if (typeof unread === "object" && unread !== null && !Array.isArray(unread)) {
      for (const [personId, count] of Object.entries(unread).slice(0, 512)) {
        if (personId.length <= 180 && Number.isSafeInteger(count) && Number(count) > 0 && Number(count) <= 10_000) oslChatUnread.set(personId, Number(count));
      }
    }
  } catch {
    sidebarOrder = [];
    hiddenServices.clear();
    homeTileOrder = [];
    hiddenHomeTiles.clear();
    notificationAppPreferences = {};
    savedNativeApps.clear();
    oslChatMutedPeople.clear();
    oslChatRemoteAccessConfirmed.clear();
    oslChatUnread.clear();
  }
  savedAccountMode = parseSavedAccountMode(localStorage.getItem(savedAccountModeStorageKey));
  savedAccountsReady = false;
  notificationsEnabled = localStorage.getItem(notificationsStorageKey) === "true";
  notificationPreviewContent = localStorage.getItem(notificationPreviewStorageKey) === "true";
  notificationScopeSuggestions = localStorage.getItem(notificationScopeStorageKey) !== "false";
  notificationChatActivity = localStorage.getItem(notificationChatStorageKey) !== "false";
  notificationSecurityActivity = localStorage.getItem(notificationSecurityStorageKey) !== "false";
  friendDefaultOslChatEnabled = localStorage.getItem(friendDefaultOslChatStorageKey) === "true";
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
  const setupScreen = ["tutorial", "detected", "install", "apps", "browser", "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub"].includes(onboardingRoute);
  const setupNavigation = setupScreen
    ? '<button class="onboarding-back-dock" id="onboarding-back" type="button">Back</button><button class="onboarding-skip-dock" id="skip-onboarding" type="button">Skip · manual setup</button>'
    : "";
  const markup = `<div class="app-frame">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}</main>${setupNavigation}</div>${scrubReviewDialogMarkup()}</div>`;
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
      <img class="osl-logo signin-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/>
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
  if (onboardingRoute === "detected") return detectedAppsContent();
  if (onboardingRoute === "install") return installMissingAppsContent();
  if (onboardingRoute === "apps") return onboardingAppsContent();
  if (onboardingRoute === "browser") return browserImportContent();
  if (onboardingRoute === "mullvad") return mullvadSetupContent();
  if (onboardingRoute === "passwords") return onboardingPasswordRoleContent("stealth");
  if (onboardingRoute === "burnpass") return onboardingPasswordRoleContent("burn");
  if (onboardingRoute === "privacy") return onboardingPrivacyContent();
  if (onboardingRoute === "scrub") return onboardingScrubContent();
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
  if (hasSelectedInstalledNativeApps()) return "detected";
  return hasSelectedMissingNativeApps() ? "install" : "apps";
}

function selectSoleConnectApp(): void {
  const choices = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available")
    .filter((app) => selectedOnboardingApps.size === 0 || selectedOnboardingApps.has(app.id));
  onboardingConnectAppId = choices.length === 1 ? choices[0].id : null;
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
  const rows = installed.length
    ? installed.map((app) => `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Installed on this PC</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${savedNativeApps.has(app.id) ? "checked" : ""}/></label>`).join("")
    : `<div class="empty-state"><strong>No selected desktop apps were detected</strong><p>OSL can still use isolated web profiles.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Use installed apps</h1><p class="compact-lead onboarding-centered-copy">Choose which detected desktop apps OSL may open. OSL does not discover their accounts or sign you in.</p><div class="saved-account-choices"><button type="button" class="setting-option ${savedAccountMode === "use" ? "selected" : ""}" data-saved-account-mode="use"><strong>Use selected apps</strong><small>Open only the apps checked below</small></button><button type="button" class="setting-option ${savedAccountMode === "clean" ? "selected" : ""}" data-saved-account-mode="clean"><strong>Use web profiles</strong><small>Start with isolated OSL profiles</small></button></div><div class="setup-list">${rows}</div><p class="saved-account-truth">Nothing opens without your choice.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-detected-apps" type="button">Continue</button></div>`;
}

function installMissingAppsContent(): string {
  const missing = selectedNativeApps().filter((app) => app.availability !== "installed");
  const rows = missing.length
    ? missing.map((app) => app.availability === "installable"
      ? `<label class="saved-account-app"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Optional Windows install</small></span></span><input type="checkbox" data-first-install="${app.id}" ${selectedFirstInstallApps.has(app.id) ? "checked" : ""}/></label>`
      : `<div class="saved-account-app unavailable"><span>${serviceLogo(app.id)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Install unavailable on this PC</small></span></span></div>`).join("")
    : `<div class="empty-state"><strong>No missing desktop apps</strong><p>Your selected desktop apps are already installed, or use the web.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Install missing apps</h1><p class="compact-lead onboarding-centered-copy">Selected installs start through Windows after you continue. OSL does not sign in for you.</p><div class="setup-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-install-apps" type="button">Continue</button></div>`;
}

function onboardingAppsContent(): string {
  const available = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available");
  const apps = selectedOnboardingApps.size
    ? available.filter((app) => selectedOnboardingApps.has(app.id))
    : available;
  const choices = apps.length
    ? `<div class="onboarding-app-grid" role="radiogroup" aria-label="App to connect now">${apps.map((app) => `<button type="button" role="radio" class="onboarding-app ${onboardingConnectAppId === app.id ? "selected" : ""}" data-connect-app-choice="${app.id}" aria-checked="${onboardingConnectAppId === app.id}"><span class="app-logo-plate">${homeAppLogo(app)}</span><strong>${escapeHtml(app.displayName)}</strong></button>`).join("")}</div>`
    : `<div class="empty-state"><strong>Apps are unavailable</strong><p>Skip for now and add one from Home.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Connect one app</h1><p class="compact-lead onboarding-centered-copy">Choose one service to open its real sign-in. You can add the rest later.</p>${choices}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-connect-app" type="button" ${onboardingConnectAppId ? "" : "disabled"}>Continue</button></div>`;
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
      onboardingRoute = "mullvad";
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
          if (route !== "onboarding" || onboardingRoute !== "browser") return;
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
      if (route !== "onboarding" || onboardingRoute !== "browser") return;
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
    onboardingRoute = "mullvad";
    render();
    void refreshMullvadSetup();
  });
}

async function refreshBrowserImportReadiness(): Promise<void> {
  if (browserReadinessBusy) return;
  browserReadinessBusy = true;
  if (route === "onboarding" && onboardingRoute === "browser") render();
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
    if (route === "onboarding" && onboardingRoute === "browser") render();
  }
}

function importIdentityForm(): string {
  return `<h1 id="route-heading" tabindex="-1">Restore your account</h1><form class="setup-surface password-form" id="identity-import-form" novalidate><label for="identity-recovery-phrase">Recovery phrase</label><textarea id="identity-recovery-phrase" rows="3" autocomplete="off" autocapitalize="none" spellcheck="false" required aria-describedby="import-error"></textarea><small>Stays on this device.</small><label for="import-password">New password</label><div class="password-input-row"><input id="import-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password" aria-controls="import-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small>6 minimum. 12+ suggested.</small><label for="import-password-confirm">Confirm password</label><div class="password-input-row"><input id="import-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="import-password-confirm" aria-controls="import-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="import-error" role="alert"></p><button class="button primary" id="identity-import-submit" type="submit" disabled>Restore</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

function recoveryContent(): string {
  if (!recoveryBundle) return `<p class="eyebrow">Recovery</p><h1 id="route-heading" tabindex="-1">No recovery secret is available</h1><button class="button primary" data-onboarding="tutorial">Continue</button>`;
  const accountRecovery = recoveryBundle.identityPhrase ? `<code>${escapeHtml(recoveryBundle.identityPhrase)}</code>` : `<p>Keep using the account recovery phrase you imported.</p>`;
  return `<p class="eyebrow">One-time recovery</p><h1 id="route-heading" tabindex="-1">Save your recovery kit</h1><section class="setup-surface recovery-surface"><article class="recovery-kit-item"><span>1</span><div><strong>Account recovery</strong>${accountRecovery}</div></article><article class="recovery-kit-item"><span>2</span><div><strong>Password recovery</strong><code>${escapeHtml(recoveryBundle.passwordPhrase)}</code></div></article><details class="recovery-account-details"><summary>Account details</summary><code>${escapeHtml(recoveryBundle.userId)}</code></details><button class="button" id="copy-recovery-kit" type="button">Copy recovery kit</button><label class="check"><input id="recovery-saved" type="checkbox"/><span>I saved my recovery kit.</span></label><button class="button primary" id="recovery-continue" disabled>Continue</button></section>`;
}

function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  if (!setup) return `<section class="unlock-card" aria-labelledby="route-heading"><div class="unlock-logo-stage" aria-hidden="true"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><h1 id="route-heading" tabindex="-1">Enter your password</h1><form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>Unlock</button></form><button class="text-back" data-onboarding="welcome">← Back</button></section>`;
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><form class="setup-surface password-form" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>${action}</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

function sendingSetupContent(): string {
  const selectedMode: SendMode = setup.sendMode === "manual" ? "clipboard" : setup.sendMode;
  const option = (mode: SendMode, title: string, detail: string, badge = "") => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${title}</strong>${badge ? `<small class="send-mode-badge">${badge}</small>` : ""}</span><small>${detail}</small></button>`;
  const risk = needsRiskAcceptance(selectedMode)
    ? `<label class="send-risk"><input id="accept-send-risk" type="checkbox" ${setup.acceptedRisk && setup.acceptedRiskForMode === selectedMode ? "checked" : ""}/><span><strong>I understand</strong><small>Experimental sending can target the wrong chat if an app changes. OSL stops unless it can verify the exact app, account, chat, and composer. Each account asks again.</small></span></label>`
    : "";
  return `<h1 id="route-heading" tabindex="-1">Choose how to send</h1>${manualSendingAnimationMarkup(selectedMode)}<div class="send-mode-list">${option("clipboard", "Copy", "Encrypts and copies. Never presses Send.", "Recommended")}${option("double", "Double Enter", "First Enter prepares. A second distinct Enter sends only after another exact check.", "Experimental")}<details class="send-mode-advanced" ${selectedMode === "single" ? "open" : ""}><summary>Advanced</summary>${option("single", "Single Enter", "One Enter prepares and sends after an exact recheck.", "Highest risk")}</details></div>${risk}<p class="send-mode-truth">If OSL cannot prove the destination, it copies the encrypted text and sends nothing.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding" ${canCompleteSetup({ ...setup, sendMode: selectedMode }) ? "" : "disabled"}>Continue</button></div>`;
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
  return `<h1 id="route-heading" tabindex="-1">Privacy</h1><p class="compact-lead onboarding-centered-copy">Turn on the protection this build can enforce.</p><div class="setup-list"><label class="setup-status-row interactive"><span><strong>Windows capture resistance</strong><small>Asks Windows to exclude OSL from ordinary screen capture. Cameras and malware can still capture content.</small></span><input id="onboarding-screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label><section class="setup-status-row" aria-disabled="true"><span><strong>Decrypt display</strong><small>Unavailable during setup. Decryption choices require a real protected chat context.</small></span><span class="status-tag">Unavailable</span></section></div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-onboarding-privacy" type="button">Continue</button></div>`;
}

function mullvadSetupContent(): string {
  const availability = mullvadStatus.availability;
  const action = availability === "installed"
    ? `<button class="button" id="open-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Opening…" : "Open Mullvad"}</button>`
    : availability === "installable"
      ? `<button class="button" id="install-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Starting…" : "Install Mullvad"}</button>`
      : `<p class="mullvad-unavailable">Mullvad or Windows App Installer was not found.</p>`;
  return `<section class="mullvad-setup" aria-labelledby="route-heading"><div class="mullvad-mark" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M12 3 19 6v5c0 4.5-2.8 8-7 10-4.2-2-7-5.5-7-10V6l7-3Z"/><path d="M9 12.2 11 14l4-4"/></svg></div><h1 id="route-heading" tabindex="-1">Mullvad</h1><p>Optional. Connect before opening your apps.</p><div class="mullvad-actions">${action}<button class="button ghost" id="refresh-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>Check again</button></div>${availability === "installed" ? `<label class="check mullvad-confirm"><input id="mullvad-connected" type="checkbox" ${mullvadConnectedConfirmed ? "checked" : ""}/><span>Mullvad shows Connected</span></label><p class="mullvad-truth">OSL opens Mullvad but cannot read your account, traffic, settings, or connection.</p>` : ""}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-mullvad" type="button" ${mullvadConnectedConfirmed ? "" : "disabled"}>Continue</button><button class="text-button" id="skip-mullvad" type="button">Not now</button></div></section>`;
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function onboardingScrubContent(): string {
  const accounts = scrubAccountSelections();
  const rows = accounts.length
    ? accounts.map(({ selection, service, account }) => `<label class="scrub-index-account"><span><strong>${escapeHtml(account)}</strong><small>${escapeHtml(service)}</small></span><input type="checkbox" data-scrub-index-account="${escapeHtml(selection.serviceId)}:${escapeHtml(selection.accountId)}" checked ${scrubIndexStatus ? "disabled" : ""}/></label>`).join("")
    : `<div class="empty-state"><strong>No connected accounts</strong><p>Connect an app first, or initialize Scrub later.</p></div>`;
  const state = scrubIndexStatus
    ? `<span class="status-tag">Initialized</span><strong>Private index created</strong><p>${scrubIndexStatus.messagesIndexed} messages indexed · ${formatBytes(scrubIndexStatus.bytesStored)} encrypted. It waits for an explicit export or supported OSL-visible source.</p>`
    : `<span class="status-tag">Local only</span><strong>Build a private index</strong><p>Stores only exports you choose and messages OSL already shows. Nothing is uploaded or deleted.</p>`;
  const action = scrubIndexStatus
    ? `<button class="button primary" id="complete-onboarding">Finish setup</button>`
    : accounts.length
      ? `<button class="button primary" id="initialize-scrub" type="button" ${scrubIndexBusy ? "disabled" : ""}>${scrubIndexBusy ? "Initializing…" : "Initialize"}</button>`
      : `<button class="button primary" id="complete-onboarding">Finish setup</button>`;
  return `<h1 id="route-heading" tabindex="-1">Initialize Scrub</h1><p class="compact-lead scrub-local-promise"><strong>This stays on your device.</strong></p><section class="scrub-index-status" aria-label="Scrub indexing status">${state}<div class="scrub-index-accounts">${rows}</div></section><p class="scrub-final-warning"><strong>Nothing is deleted now.</strong> Every future deletion starts with an editable list and your confirmation.</p><div class="setup-footer onboarding-actions">${action}</div>`;
}

function scrubAccountSelections(): Array<{ selection: ScrubAccountSelection; service: string; account: string }> {
  const servicePattern = /^[a-z0-9_-]{1,32}$/u;
  const accountPattern = /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u;
  return services.flatMap((service) => service.accounts.flatMap((account) => {
    if (!servicePattern.test(service.id) || !accountPattern.test(account.id)) return [];
    return [{ selection: { serviceId: service.id, accountId: account.id }, service: service.displayName, account: account.label }];
  })).slice(0, 32);
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${Math.ceil(value / 1024)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function previousSetupRoute(current: OnboardingRoute): OnboardingRoute {
  const routes: Partial<Record<OnboardingRoute, OnboardingRoute>> = {
    tutorial: "recovery",
    detected: "tutorial",
    install: onboardingBranch.detected ? "detected" : "tutorial",
    apps: onboardingBranch.install
      ? "install"
      : onboardingBranch.detected
        ? "detected"
        : "tutorial",
    browser: "apps",
    mullvad: "browser",
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
  const recoverySaved = document.querySelector<HTMLInputElement>("#recovery-saved");
  const recoveryContinue = document.querySelector<HTMLButtonElement>("#recovery-continue");
  document.querySelector<HTMLButtonElement>("#copy-recovery-kit")?.addEventListener("click", async () => {
    if (!recoveryBundle) return;
    const kit = [
      recoveryBundle.identityPhrase ? `Account recovery\n${recoveryBundle.identityPhrase}` : "Account recovery\nUse the account recovery phrase you imported.",
      `Password recovery\n${recoveryBundle.passwordPhrase}`,
      `Account details\n${recoveryBundle.userId}`,
    ].join("\n\n");
    try {
      await navigator.clipboard.writeText(kit);
      if (recoverySaved) recoverySaved.checked = true;
      if (recoveryContinue) recoveryContinue.disabled = false;
      showToast("Recovery kit copied — save it somewhere private");
    } catch {
      showToast("Couldn’t copy the recovery kit");
    }
  });
  recoverySaved?.addEventListener("change", () => { if (recoveryContinue) recoveryContinue.disabled = !recoverySaved.checked; });
  recoveryContinue?.addEventListener("click", () => { recoveryBundle = null; resetOnboardingBranch(); onboardingRoute = "tutorial"; render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app-choice]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.onboardingAppChoice as HomeAppId;
    if (selectedOnboardingApps.has(appId)) selectedOnboardingApps.delete(appId);
    else selectedOnboardingApps.add(appId);
    onboardingConnectAppId = null;
    render();
  }));
  document.querySelector<HTMLButtonElement>("#continue-app-choice")?.addEventListener("click", async () => {
    if (!await ensureNativeCatalogForAppChoice()) return;
    resetOnboardingBranch();
    const next = routeAfterAppChoice();
    markOnboardingBranch(next);
    if (next === "apps") selectSoleConnectApp();
    onboardingRoute = next;
    render();
  });
  document.querySelector<HTMLButtonElement>("#continue-detected-apps")?.addEventListener("click", () => {
    if (savedAccountMode === "ask") savedAccountMode = savedNativeApps.size ? "use" : "clean";
    persistSavedAccountPreferences();
    const next = hasSelectedMissingNativeApps() ? "install" : "apps";
    markOnboardingBranch(next);
    if (next === "apps") selectSoleConnectApp();
    onboardingRoute = next;
    render();
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
    selectSoleConnectApp();
    onboardingRoute = "apps";
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-connect-app-choice]").forEach((button) => button.addEventListener("click", () => {
    onboardingConnectAppId = button.dataset.connectAppChoice as HomeAppId;
    render();
  }));
  document.querySelector<HTMLButtonElement>("#continue-connect-app")?.addEventListener("click", () => {
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === onboardingConnectAppId);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    if (!app || !service || app.launchState !== "available") {
      showToast("This app is unavailable right now");
      return;
    }
    beginServiceOnboarding();
    activeService = service;
    activeHomeAppId = app.id;
    route = "service";
    serviceGuideStep = 0;
    persistServiceGuideState();
    render();
  });
  document.querySelector("#onboarding-back")?.addEventListener("click", () => {
    onboardingRoute = previousSetupRoute(onboardingRoute);
    render();
    if (onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (onboardingRoute === "mullvad") void refreshMullvadSetup();
  });
  document.querySelector("#skip-onboarding")?.addEventListener("click", () => {
    clearServiceOnboardingResume();
    if (onboardingRoute === "scrub") void completeOnboarding();
    else { onboardingRoute = "scrub"; render(); void refreshScrubIndexStatus(); }
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
  document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => { onboardingRoute = "scrub"; render(); void refreshScrubIndexStatus(); });
  document.querySelector<HTMLInputElement>("#onboarding-screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelector("#skip-mullvad")?.addEventListener("click", () => { mullvadConnectedConfirmed = false; onboardingRoute = "sending"; render(); });
  document.querySelector("#continue-mullvad")?.addEventListener("click", () => { if (!mullvadConnectedConfirmed) return; onboardingRoute = "sending"; render(); });
  document.querySelector<HTMLInputElement>("#mullvad-connected")?.addEventListener("change", (event) => { mullvadConnectedConfirmed = (event.currentTarget as HTMLInputElement).checked; render(); });
  document.querySelector("#refresh-mullvad")?.addEventListener("click", () => void refreshMullvadSetup());
  document.querySelector("#install-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("install"));
  document.querySelector("#open-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("open"));
  document.querySelector("#skip-scrub-onboarding")?.addEventListener("click", () => void completeOnboarding());
  document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding());
  document.querySelector("#initialize-scrub")?.addEventListener("click", () => void initializeOnboardingScrub());
  document.querySelector("#close-decoy")?.addEventListener("click", () => void getCurrentWindow().close().catch(() => undefined));
}

async function initializeOnboardingScrub(): Promise<void> {
  if (scrubIndexBusy || scrubIndexStatus) return;
  const selected = new Set([...document.querySelectorAll<HTMLInputElement>("[data-scrub-index-account]:checked")].map((input) => input.dataset.scrubIndexAccount));
  const selections = scrubAccountSelections().filter(({ selection }) => selected.has(`${selection.serviceId}:${selection.accountId}`)).map(({ selection }) => selection);
  if (!selections.length) {
    showToast("Choose at least one connected account");
    return;
  }
  scrubIndexBusy = true;
  render();
  try {
    scrubIndexStatus = await initializeScrubIndex({ selections, source: "osl_visible_data" });
    showToast("Scrub initialized on this device");
  } catch (failure) {
    showToast(localActionError(failure, "Scrub could not initialize"));
  } finally {
    scrubIndexBusy = false;
    render();
  }
}

async function refreshScrubIndexStatus(): Promise<void> {
  try {
    scrubIndexStatus = await getScrubIndexStatus();
    if (route === "onboarding" && onboardingRoute === "scrub") render();
  } catch {
    scrubIndexStatus = null;
  }
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
    mullvadStatus = await withNativeDeadline(loadMullvadStatus(), "Check Mullvad", bootSupportDeadlineMs);
  } catch {
    showToast("Mullvad status is unavailable");
  } finally {
    mullvadBusy = false;
    render();
  }
}

async function runMullvadSetupAction(action: "install" | "open"): Promise<void> {
  if (mullvadBusy) return;
  mullvadBusy = true;
  render();
  try {
    if (action === "install") {
      await withNativeDeadline(installMullvad(), "Start Mullvad install");
      showToast("Mullvad is installing in Windows · check again when it finishes");
    } else {
      await withNativeDeadline(openMullvad(), "Open Mullvad");
      showToast("Connect in Mullvad, then return to OSL");
    }
  } catch (failure) {
    showToast(localActionError(failure, `Mullvad could not ${action === "install" ? "install" : "open"}`));
  } finally {
    mullvadBusy = false;
    if (action === "open") mullvadStatus = await loadMullvadStatus().catch(() => mullvadStatus);
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
      if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
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
        if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
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
  if (route === "home" || route === "osl-chat") return homeHeader();
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
  const friendRequests = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange).length;
  const notificationCount = notificationsEnabled ? visibleAppNotifications().length : 0;
  return `<div class="trusted-stack home-trusted-stack"><header class="home-header home-command-bar"><button class="home-logo-button" data-route="home" aria-label="OSL Privacy home" title="OSL Privacy"><img src="${oslVectorLogoUrl}" alt=""/></button><nav class="home-command-actions" aria-label="Home controls"><button class="home-command-icon" data-open-friends type="button" aria-label="Friends${friendRequests ? `, ${friendRequests} pending` : ""}" title="Friends">${homeCommandIcon("friends")}${friendRequests ? `<span class="home-command-badge">${Math.min(friendRequests, 99)}</span>` : ""}</button><button class="home-command-icon" data-notification-settings type="button" aria-label="Notifications${notificationCount ? `, ${notificationCount} new` : ""}" title="Notifications">${homeCommandIcon("notifications")}${notificationCount ? `<span class="home-command-dot" aria-hidden="true"></span>` : ""}</button><button class="home-command-icon" data-route="settings" type="button" aria-label="Settings" title="Settings">${homeCommandIcon("settings")}</button></nav></header>${updateBannerMarkup()}</div>`;
}

function homeCommandIcon(id: "friends" | "notifications" | "settings" | "organize"): string {
  if (id === "friends") return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M16 20v-1.8c0-2-1.8-3.7-4-3.7H7c-2.2 0-4 1.7-4 3.7V20M9.5 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM16 11.2c1.7-.3 2.8-1.7 2.8-3.4 0-1.6-1.1-3-2.6-3.3M17.5 14.8c2 .5 3.5 1.9 3.5 3.7V20"/></svg>`;
  if (id === "notifications") return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 7h18s-3 0-3-7ZM10 20h4"/></svg>`;
  if (id === "organize") return `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h10M18 7h2M4 17h2M10 17h10M14 4v6M6 14v6"/></svg>`;
  return `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-1.6v-.2h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></svg>`;
}

function settingsButtonMarkup(extraClass = ""): string {
  return `<button class="button compact home-settings ${extraClass}" data-route="settings" aria-label="Open Settings"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9.6 3.4 10.2 2h3.6l.6 1.4 1.4.8 1.5-.2 1.8 3.1-.9 1.2v1.6l.9 1.2-1.8 3.1-1.5-.2-1.4.8-.6 1.4h-3.6l-.6-1.4-1.4-.8-1.5.2-1.8-3.1.9-1.2V8.3l-.9-1.2L6.7 4l1.5.2 1.4-.8Z"/><circle cx="12" cy="9.1" r="2.6"/></svg><span>Settings</span></button>`;
}

function workspaceContent(): string {
  if (route === "osl-chat") return oslChatContent();
  if (route === "settings") return settingsContent();
  if (route === "service" && activeService) return serviceContent();
  const homeApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const modules = [
    { id: "osl-chats", name: "OSL Chat", available: true },
    { id: "osl-notes", name: "OSL Notes", available: false },
    { id: "scrub", name: "Scrub", available: true },
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
    if (module) return `<article class="app-tile home-module ${module.available ? "" : "module-unavailable"} ${hidden ? "tile-hidden" : ""}" data-tile-id="${module.id}" draggable="${homeEditMode}" data-module-kind="${module.id}"><button type="button" data-home-module="${module.id}" ${module.available ? "" : "disabled"} aria-label="${escapeHtml(module.available ? module.name : `${module.name}, coming later`)}" title="${escapeHtml(module.available ? module.name : `${module.name} · Coming later`)}"><span class="app-logo-plate osl-module-logo" aria-hidden="true">${homeModuleIcon(module.id)}</span><span class="app-tile-copy"><strong>${module.name}</strong></span></button>${controls}</article>`;
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
  const organizeButton = (label: string) => `<button class="home-section-action" data-edit-home type="button" aria-label="${homeEditMode ? "Finish arranging" : `Customize ${label}`}" title="${homeEditMode ? "Done" : `Customize ${label}`}">${homeCommandIcon("organize")}</button>`;
  const oslSection = oslTiles ? `<section class="home-app-section home-osl-section"><h1 id="route-heading" class="sr-only" tabindex="-1">Home</h1><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  const profileVisual = oslProfile?.avatar
    ? `<img src="${escapeHtml(oslProfile.avatar)}" alt=""/>`
    : `<span aria-hidden="true">${escapeHtml((oslProfile?.displayName || "OSL").slice(0, 1).toUpperCase())}</span>`;
  const profileStyle = oslProfile ? ` style="--profile-accent:${oslProfile.accentColor};--profile-banner:${oslProfile.bannerColor}" data-profile-frame="${oslProfile.frame}" data-profile-effect="${oslProfile.effect}"` : "";
  return `<main class="content-viewport home-dashboard ${homeEditMode ? "editing" : ""}"><section class="home-primary"><section class="home-apps" aria-labelledby="route-heading"><div class="home-app-groups">${oslSection}<section class="home-app-section"><header><h2>Social</h2>${organizeButton("social apps")}</header><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section><section class="home-app-section"><header><h2>Email</h2>${organizeButton("email apps")}</header><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section></div></section></section><button class="home-profile-dock" data-route="settings" data-profile-settings type="button" aria-label="Open your OSL profile" title="OSL Profile"${profileStyle}>${profileVisual}<strong>${escapeHtml(oslProfile?.displayName ?? "OSL Profile")}</strong></button></main>`;
}

function oslChatContent(): string {
  const friends = hubPeople.map((person) => {
    const last = oslChatMessages.get(person.personId)?.at(-1);
    return {
      personId: person.personId,
      nickname: person.alias ?? "Unnamed friend",
      verified: person.safetyNumberVerified && !person.pendingKeyChange,
      ready: person.personId === activeOslChatPersonId && activeOslChatContext?.scopeApproved === true,
      preview: last?.body ?? null,
      previewVisible: true,
      unreadCount: oslChatUnread.get(person.personId) ?? 0,
      muted: oslChatMutedPeople.has(person.personId),
    };
  });
  const approval = activeOslChatPersonId && activeOslChatContext && !activeOslChatContext.scopeApproved
    ? `<div class="osl-chat-approval"><span><strong>Turn on this encrypted chat</strong><small>Approves only this OSL friend.</small></span><button class="button primary compact" id="osl-chat-approve" type="button" ${oslChatBusy ? "disabled" : ""}>Enable</button></div>`
    : "";
  const settingsPerson = oslChatSettingsPersonId ? hubPeople.find((person) => person.personId === oslChatSettingsPersonId) ?? null : null;
  const settings = settingsPerson ? oslChatFriendSettingsMarkup(settingsPerson) : "";
  return `<main class="content-viewport osl-chat-page"><header class="osl-chat-page-header"><button class="text-button" id="osl-chat-back" type="button" ${oslChatBusy ? "disabled" : ""}>Back</button><h1 id="route-heading" tabindex="-1">OSL Chats</h1><button class="text-button" id="osl-chat-refresh" type="button" ${activeOslChatContext?.scopeApproved && !oslChatBusy ? "" : "disabled"}>Refresh</button></header>${approval}${oslChatsViewMarkup({ friends, activePersonId: activeOslChatPersonId, messages: activeOslChatPersonId ? oslChatMessages.get(activeOslChatPersonId) ?? [] : [], draft: oslChatDraft, busy: oslChatBusy, viewOnce: oslChatViewOnce, homeLogoUrl: oslVectorLogoUrl })}${settings}</main>`;
}

function oslChatFriendSettingsMarkup(person: HubPerson): string {
  const isActive = activeOslChatPersonId === person.personId;
  const approved = isActive && activeOslChatContext?.scopeApproved === true;
  const muted = oslChatMutedPeople.has(person.personId);
  const remoteConfirmed = oslChatRemoteAccessConfirmed.has(person.personId);
  return `<dialog class="friends-dialog osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="friends-dialog-card"><header><div><span>Encrypted chat</span><h2 id="osl-chat-settings-title">${escapeHtml(person.alias ?? "Verified friend")}</h2></div><button class="icon-button" id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header><div class="settings-list"><label class="setting-line interactive"><span><strong>Mute notifications</strong><small>Messages still arrive without creating a local alert.</small></span><input id="osl-chat-mute-toggle" type="checkbox" ${muted ? "checked" : ""}/></label><div class="setting-line"><span><strong>You allow</strong><small>${approved ? "Encrypted OSL Chat for this verified friend." : "No OSL Chat access."}</small></span>${isActive ? `<button class="button compact ${approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${oslChatBusy ? "disabled" : ""}>${approved ? "Revoke" : "Enable"}</button>` : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button">Open chat</button>`}</div><div class="setting-line"><span><strong>They allow</strong><small>${remoteConfirmed ? "Encrypted OSL Chat confirmed by a signed receipt from their identity." : "Not confirmed yet. OSL updates this after their identity acknowledges a message."}</small></span><span class="status-tag ${remoteConfirmed ? "active" : ""}">${remoteConfirmed ? "Confirmed" : "Waiting"}</span></div><div class="setting-line"><span><strong>Connected services</strong><small>Provider access is approved separately inside each exact account and conversation.</small></span><span class="status-tag">${person.whitelistCount.toLocaleString("en-US")}</span></div></div></div></dialog>`;
}

function homeModuleIcon(id: "osl-chats" | "osl-notes" | "scrub"): string {
  if (id === "osl-chats") return `<svg viewBox="0 0 24 24"><path d="M4 5.5h16v10H9l-5 4v-14Z"/><path d="M8 9h8M8 12h5"/></svg>`;
  if (id === "scrub") return `<svg viewBox="0 0 24 24"><path d="m5 18 9-9 5 5-6 6H7l-2-2Z"/><path d="m12 11 3-3 5 5-3 3M4 20h16"/></svg>`;
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
  return `<dialog class="friends-dialog" id="friends-dialog" aria-labelledby="friends-dialog-title"><div class="friends-dialog-card"><header><h2 id="friends-dialog-title">Friends</h2><button class="icon-button" id="friends-dialog-close" aria-label="Close friends">×</button></header><form id="add-friend-username-form" class="friend-add-form"><label for="friend-username-input"><span>OSL username</span><input id="friend-username-input" minlength="3" maxlength="30" placeholder="username" autocomplete="off" autocapitalize="none" spellcheck="false" required/></label><label for="friend-username-nickname-input"><span>Name them on this device</span><input id="friend-username-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add friend</button></form><p class="form-status" id="friend-form-status" role="status"></p><p class="scope-approval-note">Adding a username never skips verification. Compare the safety number another way before encrypted chat access turns on.</p><details class="settings-disclosure friend-invite-fallback"><summary>Use a long invite instead</summary><form id="add-friend-form" class="friend-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button">Add from invite</button></form></details><div class="people-list home-people-list">${peopleListMarkup("manage", friendsDialogPageSize, pageStart)}</div>${pagination}${inviteCard}</div></dialog>`;
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
  const items: Array<[SettingsSection, string]> = [["account", "Account"], ["apps", "Apps"], ["scrub", "Scrub"], ["cleanup", "Cleanup"], ["notifications", "Notifications"], ["appearance", "Appearance"], ["accessibility", "Accessibility"], ["developer", "Developer"], ["about", "About"]];
  return `<main class="content-viewport settings-page"><nav class="settings-sidebar" aria-label="Settings"><h1 id="route-heading" tabindex="-1">Settings</h1>${items.map(([id, label]) => `<button data-settings="${id}" class="${settingsSection === id ? "active" : ""}" ${settingsSection === id ? 'aria-current="page"' : ""}>${label}</button>`).join("")}</nav><section class="settings-detail">${settingsSectionContent()}</section></main>`;
}

function settingsSectionContent(): string {
  if (settingsSection === "account") return `${identitySettingsContent()}${settingsDivider()}${passwordSecuritySettingsContent()}${accountAdvancedSettingsContent()}`;
  if (settingsSection === "apps") return `${serviceAccountsSettingsContent()}${sendingSettingsContent()}`;
  if (settingsSection === "scrub") return privacySettingsContent();
  if (settingsSection === "cleanup") return massCleanupSettingsContent();
  if (settingsSection === "notifications") return notificationSettingsContent();
  if (settingsSection === "appearance") return appearanceSettingsContent();
  if (settingsSection === "accessibility") return accessibilitySettingsContent();
  if (settingsSection === "developer") return developerSettingsContent();
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
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const autoScrubPlan = proActive ? "PRO ACTIVE · COMING SOON" : "PRO · COMING SOON";
  return `<h2>Scrub</h2><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Every scan and review stays local.</p><section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h3>Review an export</h3><p>Choose a TXT, CSV, or JSON message export. OSL suggests items; you decide what to review.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}<details class="settings-disclosure autoscrub-disclosure"><summary><span><strong>AutoScrub assistant</strong><small>${autoScrubPlan}</small></span></summary><section class="autoscrub-card" aria-disabled="true"><p>Coming soon. It schedules local scans and prepares an editable list. Nothing happens until you review and confirm every batch.</p><details><summary>Automation risks</summary><p>Future paced actions must stop on limits, challenges, changed content, or failed checks. Automation may break an app’s rules or restrict an account. Treat removal as unconfirmed until the app shows it is gone.</p></details><button class="button compact" type="button" disabled>Unavailable in this build</button></section></details><details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, providers, exports, and backups may retain copies.</p><p>This build only gives manual directions. It does not delete app messages. Check the original app and delete each message yourself.</p></div></details><details class="privacy-technical settings-disclosure"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Remote app access</span><strong>Blocked</strong></div><label class="setting-line interactive"><span><strong>Windows capture resistance</strong><small>Asks Windows to exclude OSL from ordinary screen capture. Cameras, malware, and modified recipients can still capture content.</small></span><input id="screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label></details>`;
}

function clearPrivacyScanState(): void {
  privacyScanResult = null;
  privacyScanFileName = null;
  selectedScrubFindings.clear();
  scrubResultsPage = 0;
  scrubReviewOpen = false;
  scrubReviewPage = 0;
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
    scrubReviewOpen = false;
    render();
  });
}

function notificationSettingsContent(): string {
  const apps = orderedServices().filter((service) => service.category === "consumer").map((service) => `<label class="notification-app-row">${serviceLogo(service.id)}<span><strong>${escapeHtml(service.displayName)}</strong><small>Unread access is not supported yet</small></span><input type="checkbox" data-notification-app="${service.id}" ${notificationAppPreferences[service.id] !== false ? "checked" : ""}/></label>`).join("");
  const visibleNotifications = visibleAppNotifications();
  const activity = notificationsEnabled && visibleNotifications.length
    ? visibleNotifications.map((item) => `<article class="notification-event"><span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")}</small></span><time>${escapeHtml(item.createdAt)}</time></article>`).join("")
    : `<div class="empty-state"><strong>${notificationsEnabled ? "Nothing new" : "Activity is off"}</strong><p>${notificationsEnabled ? "New OSL security and chat events appear here." : "Turn on local activity to see OSL events on this device."}</p></div>`;
  const muted = [...oslChatMutedPeople].flatMap((personId) => {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    return person ? [`<div class="setting-line"><span><strong>${escapeHtml(person.alias ?? "Verified friend")}</strong><small>Messages still arrive without a local alert.</small></span><button class="button compact" data-osl-chat-unmute="${escapeHtml(personId)}" type="button">Unmute</button></div>`] : [];
  }).join("");
  return `<h2>Activity</h2><p>Private events created by OSL on this device.</p><section class="notification-events" aria-label="Recent OSL activity">${activity}</section><div class="settings-list"><label class="setting-line interactive"><span><strong>Local OSL activity</strong><small>Master control for activity on this device.</small></span><input id="notifications-opt-in" type="checkbox" ${notificationsEnabled ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Encrypted chat alerts</strong><small>New-message activity from unmuted OSL friends.</small></span><input id="notification-chat-activity" type="checkbox" ${notificationChatActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Security changes</strong><small>Friend encryption-key changes that need verification.</small></span><input id="notification-security-activity" type="checkbox" ${notificationSecurityActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show details</strong><small>Off by default. When off, Activity hides event content.</small></span><input id="notification-previews" type="checkbox" ${notificationPreviewContent ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Suggest chat approval</strong><small>Suggestions never enable decryption.</small></span><input id="notification-scope-suggestions" type="checkbox" ${notificationScopeSuggestions ? "checked" : ""}/></label></div>${muted ? `<details class="settings-disclosure" open><summary><span><strong>Muted OSL Chats</strong><small>${oslChatMutedPeople.size.toLocaleString("en-US")} muted</small></span></summary><div class="settings-list">${muted}</div></details>` : ""}<details class="settings-disclosure notification-apps"><summary><span><strong>Connected apps</strong><small>For future unread support</small></span></summary><div class="notification-app-list">${apps}</div></details>`;
}

function visibleAppNotifications(): AppNotification[] {
  return (appNotifications ?? []).filter((item) => item.detail === "New encrypted message" ? notificationChatActivity : notificationSecurityActivity);
}

function identitySettingsContent(): string {
  const identities = hubIdentities.length
    ? hubIdentities.map((identity) => `<article class="identity-row"><div><strong>${escapeHtml(identity.label)}</strong><small>${escapeHtml(identity.oslUserId)} · ${escapeHtml(identity.safetyNumber)}</small></div>${identity.active ? `<span class="status-tag">Active</span>` : `<button class="button compact" data-switch-identity="${escapeHtml(identity.slotId)}">Switch</button>`}</article>`).join("")
    : `<div class="empty-state"><strong>Identity list unavailable</strong><p>Unlock OSL to manage encrypted identity slots.</p></div>`;
  const recovery = newIdentityRecoveryPhrase ? `<div class="warning recovery-secret"><strong>Save the new identity recovery phrase now</strong><code>${escapeHtml(newIdentityRecoveryPhrase)}</code><p>Visible only on this page. It clears if you leave or hide OSL.</p></div>` : "";
  return `<h2>Account</h2>${profileSettingsContent()}${settingsDivider()}${friendDefaultsSettingsContent()}${settingsDivider()}<p>One active identity on this device.</p><div class="identity-list">${identities}</div>${recovery}<form class="inline-form identity-create-form" id="identity-slot-form"><input id="identity-slot-label" maxlength="80" placeholder="New identity label" required/><button class="button primary">Create identity</button></form><details class="recovery-import settings-disclosure"><summary>Recover another identity</summary><form id="identity-recover-form" class="setup-surface"><input id="identity-recover-label" maxlength="80" placeholder="Identity label" required/><textarea id="identity-recover-phrase" rows="3" placeholder="12-word recovery phrase" required></textarea><button class="button">Recover identity</button></form></details>${activationSettingsContent()}`;
}

function friendDefaultsSettingsContent(): string {
  return `<section class="settings-section friend-defaults"><header><div><h3>Friend defaults</h3><p>Defaults apply only after you verify a friend’s safety number.</p></div></header><div class="settings-list"><label class="setting-line interactive"><span><strong>Enable OSL Chat when opened</strong><small>Automatically approve the exact first-party OSL Chat scope for verified friends you choose to open. Provider accounts remain separate.</small></span><input id="friend-default-osl-chat" type="checkbox" ${friendDefaultOslChatEnabled ? "checked" : ""}/></label></div></section>`;
}

function profileSettingsContent(): string {
  const profile = oslProfile;
  const avatar = profileDraftAvatar === undefined ? profile?.avatar ?? null : profileDraftAvatar;
  const avatarMarkup = avatar ? `<img src="${escapeHtml(avatar)}" alt=""/>` : `<span aria-hidden="true">${escapeHtml((profile?.displayName || "OSL").slice(0, 1).toUpperCase())}</span>`;
  const frames: OslProfileFrame[] = ["none", "thin", "double", "glow"];
  const effects: OslProfileEffect[] = ["none", "gradient", "pulse", "shimmer"];
  const usernameStatus = profile?.usernameCandidate && claimedOslUsername === profile.usernameCandidate ? `@${escapeHtml(profile.usernameCandidate)} is friendable` : "Saving reserves this username";
  return `<section class="settings-section osl-profile-settings"><header><div><h3>OSL profile</h3><p>Your encrypted profile and public friend identity.</p></div></header><form id="osl-profile-form"><div class="profile-editor-preview" style="--profile-accent:${profile?.accentColor ?? "#06b6d4"};--profile-banner:${profile?.bannerColor ?? "#141414"}" data-profile-frame="${profile?.frame ?? "none"}" data-profile-effect="${profile?.effect ?? "none"}">${avatarMarkup}</div><div class="profile-editor-fields"><label>OSL name<input name="displayName" maxlength="64" value="${escapeHtml(profile?.displayName ?? "")}" autocomplete="nickname" required/></label><label>Username<input name="username" minlength="3" maxlength="30" pattern="[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?" value="${escapeHtml(profile?.usernameCandidate ?? "")}" placeholder="your_name" autocomplete="username" autocapitalize="none" spellcheck="false" required/><small>${usernameStatus}</small></label><label class="profile-status-field">Status<input name="status" maxlength="160" value="${escapeHtml(profile?.status ?? "")}" placeholder="Optional"/></label><label>Accent<input name="accentColor" type="color" value="${profile?.accentColor ?? "#06b6d4"}"/></label><label>Banner<input name="bannerColor" type="color" value="${profile?.bannerColor ?? "#141414"}"/></label><label>Frame<select name="frame">${frames.map((value) => `<option value="${value}" ${profile?.frame === value ? "selected" : ""}>${value}</option>`).join("")}</select></label><label>Effect<select name="effect">${effects.map((value) => `<option value="${value}" ${profile?.effect === value ? "selected" : ""}>${value}</option>`).join("")}</select></label></div><div class="profile-avatar-actions"><label class="button" for="osl-profile-avatar-file">Choose image or GIF</label><input class="sr-only" id="osl-profile-avatar-file" type="file" accept="image/png,image/jpeg,image/webp,image/gif"/><label>or HTTPS image URL<input id="osl-profile-avatar-url" type="url" maxlength="2048" value="${avatar?.startsWith("https://") ? escapeHtml(avatar) : ""}" placeholder="https://…"/></label>${avatar ? `<button class="button ghost" id="osl-profile-avatar-remove" type="button">Remove image</button>` : ""}</div><footer><span id="osl-profile-status" role="status"></span><button class="button primary" type="submit" ${profileSaving ? "disabled" : ""}>${profileSaving ? "Saving…" : "Save profile"}</button></footer></form></section>`;
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
  const custom = activeThemeMod ? `<p class="setting-status"><span class="dot"></span>${escapeHtml(activeThemeMod.name)} theme mod active</p>` : "";
  return `<h2>Appearance</h2><p>Choose a theme. Arrange apps with Edit on Home.</p><div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong>${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>${custom}`;
}

function accessibilitySettingsContent(): string {
  const toggle = (id: keyof Pick<AccessibilityPreferences, "highContrast" | "reduceMotion" | "largeTargets">, title: string, detail: string): string => `<label class="setting-line interactive"><span><strong>${title}</strong><small>${detail}</small></span><input type="checkbox" data-accessibility-toggle="${id}" ${accessibilityPreferences[id] ? "checked" : ""}/></label>`;
  return `<h2>Accessibility</h2><p>These choices apply immediately and stay on this device.</p><section class="settings-list accessibility-settings"><label class="setting-line interactive"><span><strong>Text size</strong><small>Increase text throughout OSL.</small></span><select id="accessibility-text-scale" aria-label="Text size"><option value="100" ${accessibilityPreferences.textScale === 100 ? "selected" : ""}>Default</option><option value="112" ${accessibilityPreferences.textScale === 112 ? "selected" : ""}>Large</option><option value="125" ${accessibilityPreferences.textScale === 125 ? "selected" : ""}>Larger</option><option value="150" ${accessibilityPreferences.textScale === 150 ? "selected" : ""}>Largest</option></select></label>${toggle("highContrast", "High contrast", "Strengthen borders, text, and focus indicators.")}${toggle("reduceMotion", "Reduce motion", "Remove nonessential transitions and animations.")}${toggle("largeTargets", "Larger controls", "Keep buttons and interactive rows at least 44 pixels tall.")}</section>`;
}

function developerSettingsContent(): string {
  const modState = activeThemeMod ? `<span class="status-tag active">${escapeHtml(activeThemeMod.name)}</span>` : `<span class="status-tag">None installed</span>`;
  return `<h2>Developer</h2><p>Build OSL from source or install a data-only theme mod.</p><section class="settings-section developer-settings"><header><div><h3>Source</h3><p>Clone the repository, install the UI dependencies, then run the local Vite preview.</p></div><a class="button compact" href="https://github.com/OSLPrivacy/discord-privacy-client" target="_blank" rel="noreferrer">GitHub</a></header><pre><code>git clone https://github.com/OSLPrivacy/discord-privacy-client.git
cd discord-privacy-client/apps/osl-hub-ui
npm ci
npm run dev</code></pre></section><section class="settings-section developer-settings"><header><div><h3>Theme mods</h3><p>Theme mods are JSON data only. Scripts, remote CSS, and unknown fields are rejected.</p></div>${modState}</header><div class="settings-actions"><label class="button" for="theme-mod-input">Install theme mod</label><input class="sr-only" id="theme-mod-input" type="file" accept="application/json,.json"/>${activeThemeMod ? `<button class="button ghost" id="remove-theme-mod" type="button">Remove</button>` : ""}</div><details class="settings-disclosure"><summary>Theme mod format</summary><pre><code>{
  &quot;version&quot;: 1,
  &quot;name&quot;: &quot;My theme&quot;,
  &quot;colors&quot;: {
    &quot;brand&quot;: &quot;#06b6d4&quot;,
    &quot;background&quot;: &quot;#0a0a0a&quot;,
    &quot;panel&quot;: &quot;#141414&quot;,
    &quot;text&quot;: &quot;#e8e8e8&quot;,
    &quot;muted&quot;: &quot;#9aa0a6&quot;
  },
  &quot;radius&quot;: 6
}</code></pre></details></section>`;
}

async function submitOslProfile(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  if (profileSaving) return;
  const form = event.currentTarget as HTMLFormElement;
  const field = (name: string): string => (form.elements.namedItem(name) as HTMLInputElement | HTMLSelectElement | null)?.value.trim() ?? "";
  const displayName = field("displayName");
  const usernameCandidate = field("username").replace(/^@/u, "").toLowerCase();
  if (!isNormalizedOslUsername(usernameCandidate)) {
    showToast("Use 3–30 lowercase letters, numbers, or interior underscores");
    return;
  }
  const frame = field("frame") as OslProfileFrame;
  const effect = field("effect") as OslProfileEffect;
  const avatarUrl = document.querySelector<HTMLInputElement>("#osl-profile-avatar-url")?.value.trim() ?? "";
  const avatar = profileDraftAvatar !== undefined
    ? profileDraftAvatar
    : avatarUrl || (oslProfile?.avatar?.startsWith("data:image/") ? oslProfile.avatar : null);
  const next: OslProfile = {
    displayName,
    usernameCandidate,
    avatar,
    accentColor: field("accentColor").toLowerCase(),
    bannerColor: field("bannerColor").toLowerCase(),
    frame,
    effect,
    status: field("status"),
  };
  profileSaving = true;
  render();
  const claim = await claimOslUsername(usernameCandidate);
  if (!claim) {
    profileSaving = false;
    showToast("That username could not be reserved");
    render();
    return;
  }
  const saved = await saveOslProfile(next);
  profileSaving = false;
  if (!saved) { showToast("OSL profile could not be saved"); render(); return; }
  oslProfile = saved;
  claimedOslUsername = claim.username;
  profileDraftAvatar = undefined;
  showToast("OSL profile saved");
  render();
}

function bindOslProfileControls(): void {
  document.querySelector<HTMLFormElement>("#osl-profile-form")?.addEventListener("submit", (event) => void submitOslProfile(event));
  document.querySelector<HTMLInputElement>("#osl-profile-avatar-file")?.addEventListener("change", (event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !["image/png", "image/jpeg", "image/webp", "image/gif"].includes(file.type) || file.size > 2 * 1024 * 1024) {
      showToast("Choose a PNG, JPEG, WebP, or GIF up to 2 MiB");
      return;
    }
    const reader = new FileReader();
    reader.addEventListener("load", () => {
      if (typeof reader.result !== "string" || !reader.result.startsWith(`data:${file.type};base64,`)) {
        showToast("Profile image could not be read");
        return;
      }
      profileDraftAvatar = reader.result;
      render();
    });
    reader.addEventListener("error", () => showToast("Profile image could not be read"));
    reader.readAsDataURL(file);
  });
  document.querySelector<HTMLButtonElement>("#osl-profile-avatar-remove")?.addEventListener("click", () => {
    profileDraftAvatar = null;
    render();
  });
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
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-open]").forEach((button) => button.addEventListener("click", () => void openOslChat(button.dataset.oslChatOpen ?? "")));
  document.querySelectorAll<HTMLElement>("[data-osl-chat-context]").forEach((row) => row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    oslChatSettingsPersonId = row.dataset.oslChatContext ?? null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-settings]").forEach((button) => button.addEventListener("click", () => { oslChatSettingsPersonId = button.dataset.oslChatSettings ?? null; render(); }));
  const oslChatSettingsDialog = document.querySelector<HTMLDialogElement>("#osl-chat-settings-dialog");
  if (oslChatSettingsDialog && !oslChatSettingsDialog.open) oslChatSettingsDialog.showModal();
  document.querySelector<HTMLButtonElement>("#osl-chat-settings-close")?.addEventListener("click", () => { oslChatSettingsPersonId = null; render(); });
  document.querySelector<HTMLInputElement>("#osl-chat-mute-toggle")?.addEventListener("change", (event) => {
    const personId = oslChatSettingsPersonId;
    if (!personId) return;
    if ((event.currentTarget as HTMLInputElement).checked) oslChatMutedPeople.add(personId); else oslChatMutedPeople.delete(personId);
    localStorage.setItem(oslChatMutedStorageKey, JSON.stringify([...oslChatMutedPeople].slice(0, 512)));
    render();
  });
  document.querySelector<HTMLButtonElement>("#osl-chat-permission-toggle")?.addEventListener("click", () => void toggleOslChatPermission());
  document.querySelector<HTMLButtonElement>("#osl-chat-back")?.addEventListener("click", () => void closeOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-refresh")?.addEventListener("click", () => void refreshOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-approve")?.addEventListener("click", () => void approveOslChat());
  const oslChatDraftInput = document.querySelector<HTMLTextAreaElement>("#osl-chat-draft");
  oslChatDraftInput?.addEventListener("input", () => { oslChatDraft = oslChatDraftInput.value; });
  document.querySelector<HTMLInputElement>("#osl-chat-view-once")?.addEventListener("change", (event) => { oslChatViewOnce = (event.currentTarget as HTMLInputElement).checked; });
  document.querySelector<HTMLFormElement>("[data-osl-chat-compose]")?.addEventListener("submit", (event) => void sendOslChat(event));
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
    if (route === "osl-chat") {
      if (oslChatBusy || !(await closeOslChatContext())) { showToast("OSL Chat could not close safely"); return; }
      discardOpenedOslChatMessages();
      resetOslChatUiState(false);
    }
    if (activeEmbeddedHost || activeNativeHostId) await closeActiveServiceSurface();
    if (route === "settings" && settingsSection === "scrub") clearPrivacyScanState();
    if (route === "settings" && settingsSection === "account") newIdentityRecoveryPhrase = null;
    if (onboardingServiceSetup && requestedRoute === "home") {
      clearServiceGuide();
      route = "onboarding";
      onboardingRoute = "browser";
    } else {
      route = requestedRoute;
      if (button.hasAttribute("data-profile-settings")) settingsSection = "account";
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
  bindOslProfileControls();
  document.querySelector<HTMLInputElement>("#friend-default-osl-chat")?.addEventListener("change", (event) => {
    friendDefaultOslChatEnabled = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(friendDefaultOslChatStorageKey, String(friendDefaultOslChatEnabled));
    showToast("Friend defaults updated");
  });
  document.querySelectorAll<HTMLButtonElement>("[data-settings-send-mode]").forEach((button) => button.addEventListener("click", () => {
    void changeSendingMode(button.dataset.settingsSendMode as SendMode);
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"; settingsSection = "notifications"; render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#privacy-export-input")?.addEventListener("change", (event) => void scanPrivacyExport(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLButtonElement>("#clear-privacy-scan")?.addEventListener("click", () => { privacyScanResult = null; privacyScanFileName = null; selectedScrubFindings.clear(); scrubReviewOpen = false; render(); });
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
  document.querySelector<HTMLSelectElement>("#accessibility-text-scale")?.addEventListener("change", (event) => {
    const next = Number((event.currentTarget as HTMLSelectElement).value);
    if (![100, 112, 125, 150].includes(next)) return;
    accessibilityPreferences = { ...accessibilityPreferences, textScale: next as TextScale };
    saveAccessibilityPreferences(localStorage, accessibilityPreferences);
    applyAccessibilityPreferences(document.documentElement, accessibilityPreferences);
    render();
  });
  document.querySelectorAll<HTMLInputElement>("[data-accessibility-toggle]").forEach((input) => input.addEventListener("change", () => {
    const key = input.dataset.accessibilityToggle;
    if (key !== "highContrast" && key !== "reduceMotion" && key !== "largeTargets") return;
    accessibilityPreferences = { ...accessibilityPreferences, [key]: input.checked };
    saveAccessibilityPreferences(localStorage, accessibilityPreferences);
    applyAccessibilityPreferences(document.documentElement, accessibilityPreferences);
    render();
  }));
  document.querySelector<HTMLInputElement>("#theme-mod-input")?.addEventListener("change", (event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || file.size > 8 * 1024) { showToast("Theme mod must be a small JSON file"); return; }
    void file.text().then((raw) => {
      const parsed = parseThemeMod(raw);
      if (!parsed) { showToast("Theme mod is invalid or contains unsupported fields"); return; }
      activeThemeMod = parsed;
      localStorage.setItem(themeModStorageKey, JSON.stringify(parsed));
      applyThemeMod(document.documentElement, parsed);
      showToast(`${parsed.name} installed`);
      render();
    }).catch(() => showToast("Theme mod could not be read"));
  });
  document.querySelector<HTMLButtonElement>("#remove-theme-mod")?.addEventListener("click", () => {
    activeThemeMod = null;
    localStorage.removeItem(themeModStorageKey);
    applyThemeMod(document.documentElement, null);
    showToast("Theme mod removed");
    render();
  });
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
      onboardingRoute = "browser";
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
      onboardingRoute = "browser";
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
      onboardingRoute = "apps";
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
  document.querySelectorAll("[data-edit-home]").forEach((button) => button.addEventListener("click", () => { homeEditMode = !homeEditMode; render(); }));
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
  document.querySelector<HTMLFormElement>("#add-friend-username-form")?.addEventListener("submit", (event) => void submitFriendUsername(event));
  document.querySelector<HTMLFormElement>("#add-friend-form")?.addEventListener("submit", (event) => void submitFriendCode(event));
  document.querySelectorAll<HTMLFormElement>("[data-nickname-person]").forEach((form) => form.addEventListener("submit", (event) => void saveFriendNickname(event)));
  document.querySelector<HTMLButtonElement>("#copy-friend-code")?.addEventListener("click", () => void copyFriendInvite());
  document.querySelector<HTMLInputElement>("#notifications-opt-in")?.addEventListener("change", (event) => void changeNotifications(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#notification-chat-activity")?.addEventListener("change", (event) => { notificationChatActivity = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationChatStorageKey, String(notificationChatActivity)); render(); });
  document.querySelector<HTMLInputElement>("#notification-security-activity")?.addEventListener("change", (event) => { notificationSecurityActivity = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationSecurityStorageKey, String(notificationSecurityActivity)); render(); });
  document.querySelector<HTMLInputElement>("#notification-previews")?.addEventListener("change", (event) => { notificationPreviewContent = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationPreviewStorageKey, String(notificationPreviewContent)); });
  document.querySelector<HTMLInputElement>("#notification-scope-suggestions")?.addEventListener("change", (event) => { notificationScopeSuggestions = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationScopeStorageKey, String(notificationScopeSuggestions)); });
  document.querySelectorAll<HTMLInputElement>("[data-notification-app]").forEach((input) => input.addEventListener("change", () => { const id = input.dataset.notificationApp as ServiceId; notificationAppPreferences[id] = input.checked; localStorage.setItem(notificationAppsStorageKey, JSON.stringify(notificationAppPreferences)); }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-unmute]").forEach((button) => button.addEventListener("click", () => { oslChatMutedPeople.delete(button.dataset.oslChatUnmute ?? ""); localStorage.setItem(oslChatMutedStorageKey, JSON.stringify([...oslChatMutedPeople].slice(0, 512))); render(); }));
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
  onboardingRoute = "browser";
  activeService = null;
  activeHomeAppId = null;
  render();
  void refreshBrowserImportReadiness();
}

function currentHomeTileIds(): string[] {
  return [
    ...homeAppsFromServices(services).filter((app) => app.visibility === "launch").map((app) => app.id),
    "osl-chats", "osl-notes", "scrub",
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
  if (id === "osl-chats") {
    const first = hubPeople.find((person) => person.safetyNumberVerified && !person.pendingKeyChange);
    if (first) void openOslChat(first.personId);
    else { friendsDialogOpen = true; friendsDialogPage = 0; render(); }
  } else if (id === "scrub") {
    route = "settings";
    settingsSection = "scrub";
    render();
  } else {
    showToast("OSL Notes is planned for a later release");
  }
}

function oslChatTimestamp(seconds?: number): string {
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(seconds === undefined ? new Date() : new Date(seconds * 1_000));
}

function persistOslChatUnread(): void {
  localStorage.setItem(oslChatUnreadStorageKey, JSON.stringify(Object.fromEntries([...oslChatUnread.entries()].slice(0, 512))));
}

function historyMessages(context: ManualPeerContext, rows: NonNullable<Awaited<ReturnType<typeof listOslChatHistory>>>): OslChatMessage[] {
  return rows.slice().reverse().map((row) => {
    const incoming = row.senderOslUserId === context.peerOslUserId;
    return { messageId: row.messageId, direction: incoming ? "incoming" : "outgoing", body: row.plaintext, state: incoming ? "received" : "sent", timestampLabel: oslChatTimestamp(row.decryptedAt) } as OslChatMessage;
  });
}

async function openOslChat(personId: string): Promise<void> {
  const person = hubPeople.find((candidate) => candidate.personId === personId);
  if (!person?.safetyNumberVerified || person.pendingKeyChange || oslChatBusy) return;
  const queuedViewOnce = (oslChatUnread.get(personId) ?? 0) > 0 ? (oslChatMessages.get(personId) ?? []).filter((message) => message.state === "opened") : [];
  const epoch = ++oslChatOperationEpoch;
  oslChatBusy = true;
  activeOslChatPersonId = personId;
  activeOslChatContext = null;
  oslChatSettingsPersonId = null;
  oslChatUnread.delete(personId);
  persistOslChatUnread();
  route = "osl-chat";
  render();
  let shouldRefresh = false;
  try {
    if (!await setScreenshotProtection(true) || epoch !== oslChatOperationEpoch) { showToast("Capture resistance could not be enabled"); return; }
    screenshotProtectionEnabled = true;
    const context = await activateOslChatContext(personId);
    if (!context || epoch !== oslChatOperationEpoch || activeOslChatPersonId !== personId) { showToast("OSL Chat could not open"); return; }
    let resolvedContext = context;
    if (!resolvedContext.scopeApproved && friendDefaultOslChatEnabled) {
      const enabled = await setActiveHubFriendPermission(resolvedContext.contextToken, personId, true, false);
      if (enabled) {
        resolvedContext = { ...resolvedContext, scopeApproved: true };
        hubPeople = await listHubPeople() ?? hubPeople;
      }
    }
    activeOslChatContext = resolvedContext;
    if (resolvedContext.scopeApproved) {
      const history = await listOslChatHistory();
      if (epoch !== oslChatOperationEpoch) return;
      if (history) oslChatMessages.set(personId, [...historyMessages(resolvedContext, history), ...queuedViewOnce].slice(-200));
      shouldRefresh = true;
    }
  } finally {
    if (epoch === oslChatOperationEpoch) { oslChatBusy = false; render(); }
  }
  if (shouldRefresh && epoch === oslChatOperationEpoch) await refreshOslChat();
}

function commitOslChatBatch(personId: string, batch: OslChatOpenedBatch, background: boolean): void {
  const messages = [...(oslChatMessages.get(personId) ?? [])];
  for (const acknowledgment of batch.acknowledgments) {
    const message = messages.find((candidate) => candidate.messageId === acknowledgment.messageId);
    if (message) message.state = acknowledgment.status;
  }
  if (batch.acknowledgments.length) {
    oslChatRemoteAccessConfirmed.add(personId);
    localStorage.setItem(oslChatRemoteAccessStorageKey, JSON.stringify([...oslChatRemoteAccessConfirmed].slice(0, 512)));
  }
  for (const incoming of batch.messages) {
    const localMessageId = `received-${crypto.randomUUID()}`;
    messages.push({ messageId: localMessageId, direction: "incoming", body: incoming.plaintext, state: incoming.viewOnceConsumed ? "opened" : "received", timestampLabel: oslChatTimestamp() });
    if (background) {
      oslChatUnread.set(personId, Math.min(10_000, (oslChatUnread.get(personId) ?? 0) + 1));
      if (notificationsEnabled && notificationChatActivity && !oslChatMutedPeople.has(personId)) {
        appNotifications = [{ id: localMessageId, title: "OSL Chat", detail: "New encrypted message", createdAt: "Now" }, ...(appNotifications ?? [])].slice(0, 20);
      }
    }
  }
  oslChatMessages.set(personId, messages.slice(-200));
  if (background && batch.messages.length) { persistOslChatUnread(); renderWhenIdle(); }
}

async function syncOslChatsInBackground(): Promise<void> {
  if (oslChatBackgroundBusy || route !== "home" || activeContextToken || activeOslChatPersonId || activeNativeHostId || activeEmbeddedHost || !core.readiness.identityLoaded) return;
  const people = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange).slice(0, 32);
  if (!people.length) return;
  oslChatBackgroundBusy = true;
  try {
    if (!await setScreenshotProtection(true)) return;
    screenshotProtectionEnabled = true;
    for (const person of people) {
      if (route !== "home" || activeOslChatPersonId || activeContextToken || activeNativeHostId || activeEmbeddedHost) break;
      const context = await activateOslChatContext(person.personId);
      if (!context) continue;
      try {
        if (!context.scopeApproved) continue;
        const batch = await openOslChatText();
        if (batch) commitOslChatBatch(person.personId, batch, true);
        const history = await listOslChatHistory();
        if (history) {
          const viewOnce = (oslChatMessages.get(person.personId) ?? []).filter((message) => message.state === "opened");
          oslChatMessages.set(person.personId, [...historyMessages(context, history), ...viewOnce].slice(-200));
        }
      } finally { await closeOslChatContext(); }
    }
  } finally { oslChatBackgroundBusy = false; }
}

async function toggleOslChatPermission(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || oslChatBusy || oslChatSettingsPersonId !== context.personId) return;
  const next = !context.scopeApproved;
  oslChatBusy = true;
  render();
  const saved = await setActiveHubFriendPermission(context.contextToken, context.personId, next, false);
  if (saved) {
    activeOslChatContext = { ...context, scopeApproved: next };
    hubPeople = await listHubPeople() ?? hubPeople;
    showToast(next ? "Encrypted chat enabled" : "Encrypted chat revoked");
  } else showToast("Encrypted chat permission could not be changed");
  oslChatBusy = false;
  render();
}

async function approveOslChat(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || context.scopeApproved || oslChatBusy) return;
  oslChatSettingsPersonId = context.personId;
  await toggleOslChatPermission();
  oslChatSettingsPersonId = null;
  if (activeOslChatContext?.scopeApproved) await refreshOslChat();
}

async function refreshOslChat(): Promise<void> {
  const context = activeOslChatContext;
  const personId = activeOslChatPersonId;
  if (!context?.scopeApproved || !personId || oslChatBusy) return;
  const epoch = oslChatOperationEpoch;
  oslChatBusy = true;
  render();
  if (!await setScreenshotProtection(true) || epoch !== oslChatOperationEpoch || activeOslChatContext?.contextToken !== context.contextToken) {
    showToast("Capture resistance could not be enabled");
    if (epoch === oslChatOperationEpoch) { oslChatBusy = false; render(); }
    return;
  }
  screenshotProtectionEnabled = true;
  const batch = await openOslChatText();
  if (batch) commitOslChatBatch(personId, batch, false);
  if (epoch === oslChatOperationEpoch) { oslChatBusy = false; render(); }
}

async function sendOslChat(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const context = activeOslChatContext;
  const personId = activeOslChatPersonId;
  const draft = oslChatDraft;
  if (!context?.scopeApproved || !personId || oslChatBusy || !isHubPlaintext(draft)) return;
  const epoch = oslChatOperationEpoch;
  oslChatBusy = true;
  render();
  const sent = await prepareOslChatText(draft, oslChatViewOnce);
  if (epoch !== oslChatOperationEpoch || activeOslChatContext?.contextToken !== context.contextToken) return;
  if (!sent) { oslChatBusy = false; showToast("Encrypted message was not sent"); render(); return; }
  const outgoing: OslChatMessage = { messageId: sent.messageId, direction: "outgoing", body: draft, state: "sent", timestampLabel: oslChatTimestamp() };
  oslChatMessages.set(personId, [...(oslChatMessages.get(personId) ?? []), outgoing].slice(-200));
  oslChatDraft = "";
  oslChatViewOnce = false;
  oslChatBusy = false;
  render();
}

function resetOslChatUiState(clearMessages: boolean): void {
  oslChatOperationEpoch += 1;
  activeOslChatPersonId = null;
  activeOslChatContext = null;
  oslChatDraft = "";
  oslChatBusy = false;
  if (clearMessages) oslChatMessages.clear();
}

function discardOpenedOslChatMessages(): void {
  if (!activeOslChatPersonId) return;
  oslChatMessages.set(activeOslChatPersonId, (oslChatMessages.get(activeOslChatPersonId) ?? []).filter((message) => message.state !== "opened"));
}

async function closeOslChat(): Promise<void> {
  if (oslChatBusy || !(await closeOslChatContext())) { showToast("OSL Chat could not close safely"); return; }
  discardOpenedOslChatMessages();
  resetOslChatUiState(false);
  route = "home";
  render();
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

async function submitFriendUsername(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const form = event.currentTarget as HTMLFormElement;
  const input = form.querySelector<HTMLInputElement>("#friend-username-input");
  const nicknameInput = form.querySelector<HTMLInputElement>("#friend-username-nickname-input");
  const button = form.querySelector<HTMLButtonElement>('button[type="submit"], button:not([type])');
  const status = document.querySelector<HTMLElement>("#friend-form-status");
  const username = (input?.value ?? "").trim().replace(/^@/u, "").toLowerCase();
  if (!isNormalizedOslUsername(username)) {
    if (status) status.textContent = "Use 3–30 lowercase letters, numbers, or interior underscores.";
    input?.focus();
    return;
  }
  if (button) button.disabled = true;
  if (status) status.textContent = "Finding exact username…";
  const result = await addOslFriendByUsername(username, nicknameInput?.value ?? "");
  if (button) button.disabled = false;
  if (!result) {
    if (status) status.textContent = "That exact username could not be added. Nothing changed.";
    return;
  }
  hubPeople = await listHubPeople() ?? hubPeople;
  if (result.disposition === "key_change_requires_verification") {
    if (status) status.textContent = "Their encryption identity changed. Review the new safety number.";
  }
  if (!result.safetyNumberVerified) {
    requestFriendVerification(result.personId, result.safetyNumber);
    return;
  }
  render();
  showToast(result.disposition === "already_present" ? "Friend already added" : "Friend added");
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
  const [nextCore, nextIdentities, friendProfile, profile, people, linkedServices, notifications] = await Promise.all([
    loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration)),
    listHubIdentities().then((value) => value ?? []),
    loadFriendProfile(),
    loadOslProfile(),
    listHubPeople().then((value) => value ?? []),
    loadLinkedServices().catch(() => []),
    notificationsEnabled ? loadAppNotifications() : Promise.resolve([]),
  ]);
  core = nextCore;
  hubIdentities = nextIdentities;
  friendCode = friendProfile?.friendCode ?? null;
  friendDisplayId = friendProfile?.oslUserId ?? null;
  oslProfile = profile;
  claimedOslUsername = profile ? (await claimOslUsername(profile.usernameCandidate))?.username ?? null : null;
  profileDraftAvatar = undefined;
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
  void loadOslProfile().then(async (profile) => {
    oslProfile = profile;
    profileDraftAvatar = undefined;
    claimedOslUsername = profile ? (await claimOslUsername(profile.usernameCandidate))?.username ?? null : null;
    if (route === "home" || (route === "settings" && settingsSection === "account")) renderWhenIdle();
  });
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
  applyAccessibilityPreferences(document.documentElement, accessibilityPreferences);
  applyThemeMod(document.documentElement, activeThemeMod);
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
    renderNow();
    if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
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
      route === "onboarding" ? render() : renderWhenIdle();
    });
  } catch {
    if (attempt === bootstrapEpoch) showBootstrapRecovery();
    return;
  }
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
  if (document.visibilityState === "visible") void syncOslChatsInBackground();
  if (document.visibilityState !== "hidden" || !newIdentityRecoveryPhrase) return;
  newIdentityRecoveryPhrase = null;
  if (route === "settings" && settingsSection === "account") render();
});
window.addEventListener("error", (event) => { event.preventDefault(); containBackgroundFailure(); });
window.addEventListener("unhandledrejection", (event) => { event.preventDefault(); containBackgroundFailure(); });
function scheduleOslChatBackgroundSync(delayMs = 30_000): void {
  window.setTimeout(() => void syncOslChatsInBackground().finally(() => scheduleOslChatBackgroundSync()), delayMs);
}
scheduleOslChatBackgroundSync(1_000);
void bootstrap();
