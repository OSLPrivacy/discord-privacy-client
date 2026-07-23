import "@fontsource-variable/inter/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import { invoke } from "@tauri-apps/api/core";
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
  beginProtectedBrowserImport,
  cancelProtectedBrowserImport,
  createProtectedBrowserImportOperationId,
  escapeHtml,
  finishProtectedBrowserImport,
  closeEmbeddedServiceHost,
  configuredTopStripApps,
  detachDefaultBrowserCompanion,
  detachNativeAppWindow,
  embeddedAccountsForHomeApp,
  focusNativeAppWindow,
  focusDefaultBrowserCompanion,
  loadBrowserImports,
  loadDefaultBrowserCompanionStatus,
  homeAppsFromServices,
  hostBrowserCompanion,
  hostNativeAppWindow,
  hostMullvadWindow,
  installMullvad,
  installNativeApp,
  loadFirefoxStatus,
  loadLinkedServices,
  launchFirefoxService,
  launchSystemBrowserService,
  loadMullvadStatus,
  loadNativeApps,
  openMullvad,
  openEmbeddedHomeApp,
  parseDiscordSessionMode,
  parseNativeSessionMode,
  focusMullvadWindow,
  resizeDefaultBrowserCompanion,
  resizeNativeAppWindow,
  resizeMullvadWindow,
  restoreMullvadWindow,
  setupEmbeddedHomeApp,
  type EmailProvider,
  type DiscordSessionMode,
  type NativeSessionMode,
  type EmbeddedServiceHost,
  type FirefoxStatus,
  type HomeAppCatalogEntry,
  type HomeAppId,
  type LinkedService,
  type MullvadStatus,
  type NativeApp,
  type NativeAppId,
  type BrowserCompanionStatus,
  type BrowserImportId,
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
  unlockHubPasswordGate,
  validateHubActivationCode,
  type CoreIntegration,
  type HubLicenseState,
  type HubPasswordRoleStatus,
} from "./core";
import { checkHubForUpdates, installHubUpdate, openHubReleasesPage, openHubSourceRepository, type UpdateStatus } from "./updates";
import { browserLogo, serviceLogo, providerLogo } from "./logos";
import { activateLocalLoopbackContext, activateManualPeerContext, activateNativeManualPeerContext, activateOslChatContext, addOslFriend, addOslFriendByUsername, burnActiveHubContext, burnHubServiceAccount, closeOslChatContext, copyHubFriendInvite, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubServiceBurnReadiness, getOslUsernameStatus, isHubPlaintext, isNormalizedOslUsername, listHubIdentities, listHubPeople, listOslChatHistory, loadActiveContextSecurity, loadAppNotifications, loadFriendProfile, loadOslProfile, openOslChatText, openPeerProseText, prepareLocalProtectedText, prepareOslChatText, preparePeerProseText, recoverHubIdentitySlot, saveActiveContextSecurity, saveOslProfile, setActiveHubFriendPermission, setHubFriendNickname, setLocalProtectedSheetOpen, setNativeDiscordProtectedOverlayOpen, setNativeDiscordProtectedOverlayOpenForQa, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, type AppNotification, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type LocalMessageCandidate, type LocalPrivacyScanResult, type ManualPeerContext, type OslChatOpenedBatch, type OslProfile, type OslProfileEffect, type OslProfileFrame, type PersistedLocalPrivacyScanResult } from "./adapters";
import { blankLocalProtectedModel, isLocalTtlSeconds, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import { blankPeerProtectedModel, boundedPeerProtectedDraft, peerProtectedDraftByteFeedback, peerProtectedSheetMarkup, type PeerProtectedPane, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import mullvadLogoUrl from "./mullvad-logo.svg?url";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { withNativeDeadline } from "./native-deadline";
import { FrameRenderScheduler } from "./render-scheduler";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";
import { initializeThemePreference, themeStorageKey, type ThemeChoice } from "./theme-preference";
import type { NativeOverlayPendingAttachment } from "./overlay-state";
import { listOslChatAttachments, openOslChatAttachment, selectOslChatAttachment } from "./native-overlay-adapter";
import { type ScrubAccountSelection, type ScrubIndexStatus } from "./scrub-index";
import { persistLocalScrubExport } from "./scrub-local";
import { runAutoScrubBatch, summarizeAutoScrubReceipt, unavailableAutoScrubCapabilities, type AutoScrubCapability, type AutoScrubProviderId } from "./autoscrub-flow";
import { configureScrubImapAccount, createDesktopAutoScrubBridge, prepareScrubImapFindings, type ScrubImapLocator } from "./scrub-imap-ipc";
import type { ProviderDeletionReceipt, ScopePolicy } from "./scrub-delete-engine";
import { parseScrubSetupPlan, validateCoverageReceipt, type ScrubCoverageReceipt, type ScrubSetupPlan } from "./scrub-plan";
import { isActiveSetupRoute, parseSetupPrivacyChoices, parseSetupResumeCheckpoint, setupPrivacyChoiceIds, type ScrubSetupStep, type SetupPrivacyChoiceId } from "./setup-persistence";
import { applyAccessibilityPreferences, loadAccessibilityPreferences, saveAccessibilityPreferences, type AccessibilityPreferences, type TextScale } from "./accessibility-preference";
import { applyThemeMod, parseThemeMod, themeModStorageKey, type ThemeMod } from "./theme-mod";
import { oslChatsViewMarkup, type OslChatMessage } from "./osl-chats-view";
import { checkExternalLink, lockHubSession, openExternalLinkInDefaultBrowser, scheduleProtectedClipboardClear } from "./privacy-features";
import { deleteOslNote, listOslNotes, notesWorkspaceMarkup, saveOslNote, type OslNote } from "./osl-notes";

type Route = "onboarding" | "home" | "service" | "settings" | "mullvad" | "osl-chat" | "osl-servers" | "notes";
type OnboardingRoute = "pro" | "welcome" | "create" | "import" | "unlock" | "recovery" | "mullvad" | "sending" | "cover" | "passwords" | "burnpass" | "privacy" | "tutorial" | "detected" | "install" | "apps" | "browser" | "decoy" | "scrub";
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
const discordQaShell = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1";
if (discordQaShell) document.documentElement.classList.add("discord-qa-shell");

function manualSendingAnimationMarkup(mode: SendMode = "clipboard"): string {
  const finalStep = mode === "double" ? "Enter again" : mode === "single" ? "Recheck & send" : "You send";
  const step = (number: number, label: string) => `<span><b>${number}</b><em>${label}</em></span>`;
  return `<div class="manual-send-demo" data-send-demo="${mode}" role="img" aria-label="OSL encrypts on this device, verifies the destination, and fails closed if anything changes.">${step(1, "Write")}<i aria-hidden="true"></i>${step(2, "Encrypt")}<i aria-hidden="true"></i>${step(3, mode === "clipboard" || mode === "manual" ? "Copy" : "Verify")}<i aria-hidden="true"></i>${step(4, finalStep)}</div>`;
}

function passwordEyeIcon(visible = false): string {
  return `<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M1.8 10s2.9-4.7 8.2-4.7 8.2 4.7 8.2 4.7-2.9 4.7-8.2 4.7S1.8 10 1.8 10Z"/><circle cx="10" cy="10" r="2.25"/>${visible ? "" : '<path d="M3 3l14 14"/>'}</svg>`;
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
let mullvadSetupNotice = "";
let mullvadAutoStart = false;
let mullvadAutoStartAttempted = false;
let mullvadWindowHosted = false;
let mullvadReturnRoute: "onboarding" | "home" = "home";
let mullvadPreference: "auto" | "off" | null = null;
let browserImports: BrowserImportStatus[] = [];
let selectedBrowserImports = new Set<BrowserImportId>();
let browserImportSelectionInitialized = false;
let browserReadinessBusy = false;
let browserImportBusy = false;
let browserImportFailureNotice = "";
let browserImportCancelling = false;
let browserImportOperation: { operationId: string; result: ReturnType<typeof beginProtectedBrowserImport> } | null = null;
let browserImportProgress = "";
let detectedBrowserServices = new Set<HomeAppId>();
let firefoxStatus: FirefoxStatus = { availability: "unavailable" };
let defaultBrowserCompanionStatus: BrowserCompanionStatus = { status: "unsupported", browserId: null, displayName: null, reason: "platformUnsupported", captureProtected: false, containment: "bestEffort" };
let useDefaultBrowserCompanion = localStorage.getItem("osl-default-browser-companion-v1") === "true";
let activeDefaultBrowserCompanion = false;
let savedAccountsReady = false;
let preferredBrowserId: BrowserImportId | null = null;
let completedBrowserImportIds = new Set<BrowserImportId>();
let savedAccountMode: SavedAccountMode = "ask";
let savedNativeApps = new Set<NativeAppId>();
let discordSessionMode: DiscordSessionMode = "existingSession";
let telegramSessionMode: NativeSessionMode = "existingSession";
let signalSessionMode: NativeSessionMode = "existingSession";
let whatsappSessionMode: NativeSessionMode = "existingSession";
let outlookSessionMode: NativeSessionMode = "existingSession";
let confirmedNativeSessionModes = new Set<NativeAppId>();
let browserSessionModeConfirmed = false;
const backgroundInstallIds = new Set<NativeAppId>();
const selectedFirstInstallApps = new Set<NativeAppId>();
const selectedOnboardingApps = new Set<HomeAppId>();
let hasExplicitOnboardingAppSelection = false;
let onboardingAppChoicesConfirmed = false;
let onboardingConnectAppId: HomeAppId | null = null;
const handledOnboardingConnectApps = new Set<HomeAppId>();
let backgroundInstallQueue: Promise<void> = Promise.resolve();
const detectedAccountChoices = new Map<string, "existing" | "osl">();
let scrubSetupStep: ScrubSetupStep = "intro";
let selectedOnboardingScrubAccounts = new Set<string>();
let onboardingScrubMode: "scrub" | "autoscrub" = "scrub";
let setupPrivacyChoices = parseSetupPrivacyChoices(null);
let nativeActionBusy = false;
type DiscordQaHostState = "starting" | "hosted" | "failed";
let discordQaHostState: DiscordQaHostState = "starting";
type DiscordQaOverlayState = "starting" | "ready" | "failed";
let discordQaOverlayState: DiscordQaOverlayState = "starting";
let onboardingServiceSetup = false;
let activeEmbeddedHost: EmbeddedServiceHost | null = null;
let activeNativeHostId: NativeAppId | null = null;
let activeNativeHostMode: NativeSessionMode | null = null;
let serviceAccountPickerOpen = false;
let timer = "72h";
let toastTimer: number | undefined;
let updateStatus: UpdateStatus = { state: "unavailable" };
let recoveryBundle: { userId: string; identityPhrase: string | null; passwordPhrase: string } | null = null;
let recoverySavedAcknowledged = false;
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
let nativeDiscordProtectionActive = false;
let nativeProtectPickerOpen = false;
let nativeProtectBusy = false;
let nativeProtectFailureNotice = "";
let onboardingComplete = false;
let screenshotProtectionEnabled = true;
let windowCaptureEnabled = true;
let hubIdentities: HubIdentitySlot[] = [];
let newIdentityRecoveryPhrase: string | null = null;
let hubPeople: HubPerson[] = [];
let privacyScanResult: PersistedLocalPrivacyScanResult | null = null;
let privacyScanStatus: ScrubIndexStatus | null = null;
let privacyCoverageReceipt: ScrubCoverageReceipt | null = null;
let privacyScanFileName: string | null = null;
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
let lastOnboardingMarkup: string | null = null;
let renderedOnboardingRoute: OnboardingRoute | null = null;
let lastWorkspaceMarkup: string | null = null;
let lastWorkspaceViewKey = "";
let deferredBackgroundRender = false;
let serviceGuideStep: ServiceGuideStep | null = null;
let nativeHostFailureNotice = "";
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
const oslChatPendingViewOnce = new Map<string, Set<string>>();
let oslChatPreviewsVisible = true;
let oslChatMutedPeople = new Set<string>();
let oslChatRemoteAccessConfirmed = new Set<string>();
let friendDefaultOslChatEnabled = false;
let oslChatSettingsPersonId: string | null = null;
let oslChatAttachments: NativeOverlayPendingAttachment[] = [];
let oslNotes: OslNote[] = [];
let activeOslNoteId: string | null = null;
let oslNotesLoading = false;
let oslNotesError = "";
let oslNotesSaveTimer: number | undefined;

const sidebarStorageKey = "osl-hub-sidebar";
const hiddenStorageKey = "osl-hub-sidebar-hidden";
const notificationsStorageKey = "osl-hub-notifications";
const notificationAppsStorageKey = "osl-hub-notification-apps";
const notificationPreviewStorageKey = "osl-hub-notification-previews";
const notificationScopeStorageKey = "osl-hub-notification-scope-suggestions";
const notificationChatStorageKey = "osl-hub-notification-chats-v1";
const notificationSecurityStorageKey = "osl-hub-notification-security-v1";
const mullvadAutoStartStorageKey = "osl-mullvad-autostart-v1";
const screenshotProtectionStorageKey = "osl-hub-screenshot-protection";
const scrubSignalsStorageKey = "osl-hub-scrub-signals-v1";
const serviceGuideStorageKey = "osl-hub-service-guide-v1";
const homeTileOrderStorageKey = "osl-home-tile-order-v1";
const hiddenHomeTilesStorageKey = "osl-home-tile-hidden-v1";
const savedAccountModeStorageKey = "osl-saved-account-mode-v1";
const savedNativeAppsStorageKey = "osl-saved-native-apps-v1";
const discordSessionModeStorageKey = "osl-discord-session-mode-v1";
const telegramSessionModeStorageKey = "osl-telegram-session-mode-v1";
const signalSessionModeStorageKey = "osl-signal-session-mode-v1";
const whatsappSessionModeStorageKey = "osl-whatsapp-session-mode-v1";
const outlookSessionModeStorageKey = "osl-outlook-session-mode-v1";
const confirmedNativeSessionModesStorageKey = "osl-native-session-choices-v2";
const browserSessionModeConfirmedStorageKey = "osl-browser-session-choice-v2";
const selectedOnboardingAppsStorageKey = "osl-selected-apps-v1";
const savedAccountsReadyStorageKey = "osl-browser-accounts-ready-v1";
const preferredBrowserStorageKey = "osl-preferred-browser-v1";
const completedBrowserImportsStorageKey = "osl-browser-import-sources-v1";
const detectedBrowserServicesStorageKey = "osl-browser-detected-services-v1";
const mullvadStartupStorageKey = "osl-mullvad-open-on-start-v1";
const setupPrivacyStorageKey = "osl-setup-privacy-v1";
const privacyFeaturesStorageKey = "osl-privacy-features-v1";
const autoLockIdleMilliseconds = 5 * 60 * 1_000;
const protectedClipboardClearSeconds = 30;
const detectedAccountChoicesStorageKey = "osl-detected-account-opening-v1";
const browserImportPendingStorageKey = "osl-browser-import-pending-v1";
const onboardingResumeStorageKey = "osl-onboarding-resume-v1";
const onboardingBranchStorageKey = "osl-onboarding-branch-v1";
const scrubSetupPlanStorageKey = "osl-scrub-setup-plan-v1";
const experimentalSendConsentStorageKey = "osl-experimental-send-consent-v1";
let nativeDiscordCovertextEnabled = true;
const oslChatPreviewStorageKey = "osl-chat-previews-visible-v1";
const oslChatMutedStorageKey = "osl-chat-muted-people-v1";
const oslChatUnreadStorageKey = "osl-chat-unread-v1";
const oslChatNotificationStorageKey = "osl-chat-notifications-v1";
const oslChatRemoteAccessStorageKey = "osl-chat-remote-access-v1";
const friendDefaultOslChatStorageKey = "osl-friend-default-chat-v1";
const supportedNativeAppIds = new Set<NativeAppId>(["discord", "telegram", "signal", "whatsapp", "outlook"]);
const importedFirefoxHomeAppIds = new Set<HomeAppId>([
  "instagram", "snapchat", "x", "messenger", "gmail", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
]);
const friendsDialogPageSize = 24;
const friendScopeRenderLimit = 16;
const scrubResultsPageSize = 50;
const scrubReviewPageSize = 20;
const bootCoreDeadlineMs = 4_000;
const bootPreferenceDeadlineMs = 1_500;
const bootSupportDeadlineMs = 2_000;
const nativeCatalogDecisionDeadlineMs = 8_000;
const protectedBrowserImportSourceDeadlineMs = 90_000;
let lastTrustedActivityAt = Date.now();
let autoLockCheckTimer: number | undefined;
let autoLockInProgress = false;

function resetOnboardingBranch(): void {
  localStorage.removeItem(onboardingBranchStorageKey);
}

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

function identityScopedStorageKey(base: string): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${base}:${encodeURIComponent(owner)}` : null;
}

function pendingOnboardingRoute(): OnboardingRoute | null {
  const key = identityScopedStorageKey(onboardingResumeStorageKey);
  const checkpoint = key ? parseSetupResumeCheckpoint(localStorage.getItem(key)) : null;
  if (!checkpoint) return null;
  scrubSetupStep = checkpoint.scrubStep;
  return checkpoint.route;
}

function persistOnboardingResume(routeToPersist = onboardingRoute, step = scrubSetupStep): void {
  const key = identityScopedStorageKey(onboardingResumeStorageKey);
  if (!key || !isActiveSetupRoute(routeToPersist)) return;
  localStorage.setItem(key, JSON.stringify({ route: routeToPersist, scrubStep: routeToPersist === "scrub" ? step : "intro" }));
}

function beginServiceOnboarding(): void {
  onboardingServiceSetup = true;
  const key = identityScopedStorageKey(onboardingResumeStorageKey);
  if (key) localStorage.removeItem(key);
  localStorage.removeItem(onboardingResumeStorageKey);
}

function markServiceOnboardingOpened(): void {
  if (!onboardingServiceSetup) return;
  persistOnboardingResume("apps", "intro");
}

function clearServiceOnboardingResume(): void {
  onboardingServiceSetup = false;
  const key = identityScopedStorageKey(onboardingResumeStorageKey);
  if (key) localStorage.removeItem(key);
  // Remove the pre-identity legacy checkpoint; it must never cross identities.
  localStorage.removeItem(onboardingResumeStorageKey);
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
    const selectedAppsRaw = localStorage.getItem(selectedOnboardingAppsStorageKey);
    hasExplicitOnboardingAppSelection = selectedAppsRaw !== null;
    const selectedApps = JSON.parse(selectedAppsRaw ?? "[]") as unknown;
    if (Array.isArray(selectedApps)) selectedApps.filter((id): id is HomeAppId => typeof id === "string").slice(0, 32).forEach((id) => selectedOnboardingApps.add(id));
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
    selectedOnboardingApps.clear();
    hasExplicitOnboardingAppSelection = localStorage.getItem(selectedOnboardingAppsStorageKey) !== null;
    oslChatMutedPeople.clear();
    oslChatRemoteAccessConfirmed.clear();
    oslChatUnread.clear();
  }
  savedAccountMode = parseSavedAccountMode(localStorage.getItem(savedAccountModeStorageKey));
  discordSessionMode = parseDiscordSessionMode(localStorage.getItem(discordSessionModeStorageKey));
  const storedTelegramMode = localStorage.getItem(telegramSessionModeStorageKey);
  const storedSignalMode = localStorage.getItem(signalSessionModeStorageKey);
  const storedWhatsappMode = localStorage.getItem(whatsappSessionModeStorageKey);
  const storedOutlookMode = localStorage.getItem(outlookSessionModeStorageKey);
  telegramSessionMode = storedTelegramMode === null ? "existingSession" : parseNativeSessionMode(storedTelegramMode);
  signalSessionMode = storedSignalMode === null ? "existingSession" : parseNativeSessionMode(storedSignalMode);
  whatsappSessionMode = storedWhatsappMode === null ? "existingSession" : parseNativeSessionMode(storedWhatsappMode);
  outlookSessionMode = storedOutlookMode === null ? "existingSession" : parseNativeSessionMode(storedOutlookMode);
  try {
    const confirmed = JSON.parse(localStorage.getItem(confirmedNativeSessionModesStorageKey) ?? "[]") as unknown;
    confirmedNativeSessionModes = new Set(Array.isArray(confirmed)
      ? confirmed.filter((appId): appId is NativeAppId => typeof appId === "string" && supportedNativeAppIds.has(appId as NativeAppId))
      : []);
  } catch {
    confirmedNativeSessionModes.clear();
  }
  browserSessionModeConfirmed = localStorage.getItem(browserSessionModeConfirmedStorageKey) === "true";
  savedAccountsReady = false;
  notificationsEnabled = localStorage.getItem(notificationsStorageKey) === "true";
  notificationPreviewContent = localStorage.getItem(notificationPreviewStorageKey) === "true";
  notificationScopeSuggestions = localStorage.getItem(notificationScopeStorageKey) !== "false";
  notificationChatActivity = localStorage.getItem(notificationChatStorageKey) !== "false";
  notificationSecurityActivity = localStorage.getItem(notificationSecurityStorageKey) !== "false";
  oslChatPreviewsVisible = localStorage.getItem(oslChatPreviewStorageKey) !== "false";
  try {
    const notices = JSON.parse(localStorage.getItem(oslChatNotificationStorageKey) ?? "[]") as unknown;
    if (Array.isArray(notices)) {
      const parsed = notices.slice(0, 20).filter((item): item is AppNotification => typeof item === "object" && item !== null
        && typeof (item as AppNotification).id === "string" && (item as AppNotification).id.length <= 96
        && typeof (item as AppNotification).title === "string" && (item as AppNotification).title.length <= 120
        && (item as AppNotification).detail === "New encrypted message"
        && typeof (item as AppNotification).createdAt === "string" && (item as AppNotification).createdAt.length <= 32);
      if (parsed.length) appNotifications = parsed;
    }
  } catch { /* malformed local notification metadata is ignored */ }
  mullvadAutoStart = localStorage.getItem(mullvadAutoStartStorageKey) === "true";
  friendDefaultOslChatEnabled = localStorage.getItem(friendDefaultOslChatStorageKey) === "true";
  screenshotProtectionEnabled = localStorage.getItem(screenshotProtectionStorageKey) === "true";
  enabledScrubSignals = parseScrubSignalGroups(localStorage.getItem(scrubSignalsStorageKey));
  mullvadPreference = localStorage.getItem(mullvadStartupStorageKey) === "true" ? "auto" : localStorage.getItem(mullvadStartupStorageKey) === "false" ? "off" : null;
  setupPrivacyChoices = parseSetupPrivacyChoices(localStorage.getItem(setupPrivacyStorageKey));
  try {
    const savedChoices = JSON.parse(localStorage.getItem(detectedAccountChoicesStorageKey) ?? "[]") as unknown;
    if (Array.isArray(savedChoices)) for (const entry of savedChoices) {
      if (Array.isArray(entry) && typeof entry[0] === "string" && (entry[1] === "existing" || entry[1] === "osl")) detectedAccountChoices.set(entry[0], entry[1]);
    }
  } catch {
    detectedAccountChoices.clear();
  }
}

function activeBrowserAccountsReadyStorageKey(): string | null {
  return identityScopedStorageKey(savedAccountsReadyStorageKey);
}

function activeOwnerStorageKey(base: string): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${base}:${encodeURIComponent(owner)}` : null;
}

function loadActivePrivacyFeatureChoices(): void {
  const identityKey = activeOwnerStorageKey(privacyFeaturesStorageKey);
  const stored = identityKey ? localStorage.getItem(identityKey) : null;
  setupPrivacyChoices = parseSetupPrivacyChoices(stored ?? localStorage.getItem(setupPrivacyStorageKey));
}

function persistActivePrivacyFeatureChoices(): void {
  const serialized = JSON.stringify([...setupPrivacyChoices]);
  localStorage.setItem(setupPrivacyStorageKey, serialized);
  const identityKey = activeOwnerStorageKey(privacyFeaturesStorageKey);
  if (identityKey) localStorage.setItem(identityKey, serialized);
  if (!setupPrivacyChoices.has("auto-lock")) lastTrustedActivityAt = Date.now();
}

function supportedBrowserId(raw: unknown): raw is BrowserImportId {
  return typeof raw === "string" && ["chrome", "edge", "firefox", "brave", "opera", "duckduckgo"].includes(raw);
}

function persistBrowserAccountPreferences(): void {
  const preferredKey = activeOwnerStorageKey(preferredBrowserStorageKey);
  const importsKey = activeOwnerStorageKey(completedBrowserImportsStorageKey);
  if (preferredKey) {
    if (preferredBrowserId) localStorage.setItem(preferredKey, preferredBrowserId);
    else localStorage.removeItem(preferredKey);
  }
  if (importsKey) localStorage.setItem(importsKey, JSON.stringify([...completedBrowserImportIds]));
}

function activeBrowserImportPendingStorageKey(): string | null {
  return identityScopedStorageKey(browserImportPendingStorageKey);
}

function activeDetectedBrowserServicesStorageKey(): string | null {
  return identityScopedStorageKey(detectedBrowserServicesStorageKey);
}

function refreshActiveBrowserAccountsReady(): void {
  const key = activeBrowserAccountsReadyStorageKey();
  savedAccountsReady = key !== null && localStorage.getItem(key) === "true";
  const preferred = activeOwnerStorageKey(preferredBrowserStorageKey);
  const imported = activeOwnerStorageKey(completedBrowserImportsStorageKey);
  const storedPreferred = preferred ? localStorage.getItem(preferred) : null;
  preferredBrowserId = supportedBrowserId(storedPreferred) ? storedPreferred : null;
  completedBrowserImportIds.clear();
  try {
    const stored = imported ? JSON.parse(localStorage.getItem(imported) ?? "[]") as unknown : [];
    if (Array.isArray(stored)) stored.filter(supportedBrowserId).forEach((id) => completedBrowserImportIds.add(id));
  } catch { completedBrowserImportIds.clear(); }
  detectedBrowserServices.clear();
  const detectedKey = activeDetectedBrowserServicesStorageKey();
  if (detectedKey !== null) {
    try {
      const parsed = JSON.parse(localStorage.getItem(detectedKey) ?? "[]") as unknown;
      if (Array.isArray(parsed)) for (const id of parsed) {
        if (typeof id === "string" && importedFirefoxHomeAppIds.has(id as HomeAppId)) detectedBrowserServices.add(id as HomeAppId);
      }
    } catch {
      localStorage.removeItem(detectedKey);
    }
  }
  const pendingKey = activeBrowserImportPendingStorageKey();
  if (!savedAccountsReady && pendingKey !== null) localStorage.removeItem(pendingKey);
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
        if (focusKey !== lastFocusKey) return;
        const active = document.activeElement;
        const userHasFocusedControl = active instanceof HTMLElement
          && active !== document.body
          && active !== document.documentElement
          && active !== root;
        if (!userHasFocusedControl) document.querySelector<HTMLElement>("#route-heading")?.focus();
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
  lastOnboardingMarkup = null;
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
  const nativeControlsBlocked = activeNativeHostId || activeDefaultBrowserCompanion ? ' disabled title="Unavailable while a companion window is open"' : "";
  return `<header class="desktop-titlebar"><div class="desktop-drag-region" data-tauri-drag-region aria-hidden="true"></div><div class="window-controls"><button id="window-minimize" aria-label="Minimize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize or restore"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-fullscreen" aria-label="Toggle fullscreen"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.75 6V2.75H6M10 2.75h3.25V6M13.25 10v3.25H10M6 13.25H2.75V10"/></svg></button><button id="window-close" class="window-close" aria-label="Close"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button></div></header>`;
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
  bindOnce("#window-fullscreen", () => toggleDesktopFullscreen());
  bindOnce("#window-close", () => appWindow.close());
}

function waitForDesktopGeometrySettlement(
  appWindow: ReturnType<typeof getCurrentWindow>,
  timeoutMs = 400,
): Promise<void> {
  return new Promise((resolve) => {
    let finished = false;
    let unlisten: (() => void) | null = null;
    let timeout = 0;
    const finish = (): void => {
      if (finished) return;
      finished = true;
      if (timeout) window.clearTimeout(timeout);
      unlisten?.();
      requestAnimationFrame(() => resolve());
    };
    timeout = window.setTimeout(finish, timeoutMs);
    void appWindow.onResized(finish).then((stop) => {
      if (finished) stop();
      else unlisten = stop;
    }).catch(finish);
  });
}

async function toggleDesktopFullscreen(): Promise<void> {
  const appWindow = getCurrentWindow();
  const fullscreen = await appWindow.isFullscreen();
  // Arm the listener before asking Windows to transition so a fast resize
  // event cannot race ahead of registration. The timeout remains a bounded
  // fallback for window managers that suppress an otherwise valid event.
  const geometrySettled = waitForDesktopGeometrySettlement(appWindow);
  await appWindow.setFullscreen(!fullscreen);
  await geometrySettled;
  if (activeNativeHostId) await resizeNativeAppWindow().catch(() => undefined);
  if (activeDefaultBrowserCompanion) await resizeDefaultBrowserCompanion().catch(() => undefined);
  await focusActiveNativeCompanion();
  if (mullvadWindowHosted) await resizeMullvadWindow().catch(() => undefined);
}

async function focusActiveNativeCompanion(): Promise<boolean> {
  if (!activeNativeHostId) return false;
  const name = activeHomeAppName();
  const focused = await withNativeDeadline(focusNativeAppWindow(), `Focus ${name}`, 3_000).catch(() => null);
  if (focused?.status !== "focused") return false;
  const resized = await withNativeDeadline(resizeNativeAppWindow(), `Align ${name}`, 3_000).catch(() => null);
  return resized?.status === "resized";
}

async function reopenActiveNativeCompanion(): Promise<void> {
  if (nativeActionBusy || !activeNativeHostId) return;
  if (await focusActiveNativeCompanion()) return;
  const staleAppId = activeNativeHostId;
  const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId)
    ?? homeAppsFromServices(services).find((candidate) => candidate.id === staleAppId);
  const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
  await detachNativeAppWindow().catch(() => undefined);
  activeNativeHostId = null;
  activeNativeHostMode = null;
  if (!app || !service) {
    showToast(`${activeHomeAppName()} could not be reopened`);
    render();
    return;
  }
  await openNativeHostedApp(app, service, staleAppId);
}

function renderOnboarding(): void {
  if (isActiveSetupRoute(onboardingRoute)) persistOnboardingResume();
  const setupScreen = ["pro", "privacy", "sending", "cover", "passwords", "burnpass", "browser", "tutorial", "detected", "install", "apps", "mullvad", "scrub"].includes(onboardingRoute);
  const setupNavigation = setupScreen
    ? `<button class="onboarding-back-dock" id="onboarding-back" type="button">Back</button>`
    : "";
  const markup = `<div class="app-frame">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}</main>${setupNavigation}</div>${scrubReviewDialogMarkup()}</div>`;
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  if (lastOnboardingMarkup === markup && root.querySelector(".onboarding-shell")) {
    openScrubReviewDialogAfterRender();
    return;
  }
  const active = document.activeElement;
  const sensitiveEditInProgress = renderedOnboardingRoute === onboardingRoute
    && [...root.querySelectorAll<HTMLInputElement>('input[type="password"]')]
      .some((input) => input === active || input.value.length > 0);
  if (sensitiveEditInProgress) return;
  lastOnboardingMarkup = markup;
  renderedOnboardingRoute = onboardingRoute;
  root.innerHTML = markup;
  bindOnboarding();
  openScrubReviewDialogAfterRender();
}

function onboardingContent(): string {
  if (onboardingRoute === "pro") return proSetupContent();
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
      ${returning ? `<div class="signin-divider" aria-hidden="true"><span></span></div><p class="signin-new">Unlock first to add another identity in Settings.</p>` : ""}
    </section>`;
  }

  if (onboardingRoute === "create") return identityPasswordForm("Create a password", "Create account", "setup");
  if (onboardingRoute === "unlock") return identityPasswordForm("Unlock OSL", "Unlock", "unlock");
  if (onboardingRoute === "import") return importIdentityForm();
  if (onboardingRoute === "recovery") return recoveryContent();
  if (onboardingRoute === "detected") return detectedAppsContent();
  if (onboardingRoute === "browser") return browserImportContent();
  if (onboardingRoute === "mullvad") return mullvadSetupContent();
  if (onboardingRoute === "cover") return coverDraftSetupContent();
  if (onboardingRoute === "tutorial") return tutorialContent();
  if (onboardingRoute === "install") return installMissingAppsContent();
  if (onboardingRoute === "apps") return onboardingAppChoicesConfirmed ? onboardingAppsContent() : tutorialContent();
  if (onboardingRoute === "sending") return sendingSetupContent();
  if (onboardingRoute === "passwords") return onboardingPasswordRoleContent("stealth");
  if (onboardingRoute === "burnpass") return onboardingPasswordRoleContent("burn");
  if (onboardingRoute === "privacy") return onboardingPrivacyContent();
  if (onboardingRoute === "scrub") return scrubSetupContent();
  if (onboardingRoute === "decoy") return `<section class="decoy-workspace" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Workspace</h1><p>No recent items.</p><button class="button ghost" id="close-decoy" type="button">Close</button></section>`;

  return sendingSetupContent();
}

function proSetupContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  if (pro) return `<section class="pro-setup" aria-labelledby="route-heading"><span class="status-tag active">Pro active</span><h1 id="route-heading" tabindex="-1">OSL Pro is ready</h1><button class="button primary" data-onboarding="sending" type="button">Continue</button></section>`;
  return `<section class="pro-setup" aria-labelledby="route-heading"><p class="eyebrow">Optional</p><h1 id="route-heading" tabindex="-1">Enter Pro code</h1><form id="activation-form" class="pro-setup-form" novalidate><label class="sr-only" for="activation-code">Pro activation code</label><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Continue</button></form><button class="text-button" data-onboarding="sending" type="button">Skip</button></section>`;
}

function tutorialContent(): string {
  const apps = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available");
  const detectedIds = new Set(apps.filter((app) => {
    const native = nativeApps.find((candidate) => candidate.id === app.id);
    return app.linked
      || native?.availability === "installed"
      || (savedAccountsReady && importedFirefoxHomeAppIds.has(app.id));
  }).map((app) => app.id));
  const detected = apps.filter((app) => detectedIds.has(app.id));
  const other = apps.filter((app) => !detectedIds.has(app.id));
  const choices = (items: HomeAppCatalogEntry[], label: string) => items.length
    ? `<div class="onboarding-app-grid onboarding-app-choices" role="group" aria-label="${label}">${items.map((app) => `<button type="button" class="onboarding-app ${selectedOnboardingApps.has(app.id) ? "selected" : ""}" data-onboarding-app-choice="${app.id}" aria-pressed="${selectedOnboardingApps.has(app.id)}"><span class="app-logo-plate">${homeAppLogo(app)}</span><strong>${escapeHtml(app.displayName)}</strong></button>`).join("")}</div>`
    : `<p class="saved-account-truth">None</p>`;
  return `<h1 id="route-heading" tabindex="-1">Choose apps</h1><p class="compact-lead onboarding-centered-copy">Choose what appears on Home. Nothing opens during setup.</p><section class="onboarding-app-section"><h2>Detected</h2>${choices(detected, "Detected apps")}</section><section class="onboarding-app-section"><h2>Other apps</h2>${choices(other, "Other apps")}</section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-app-choice" type="button" ${nativeCatalogBusy ? "disabled" : ""}>${nativeCatalogBusy ? "Checking Windows…" : "Continue"}</button></div>`;
}

function persistCombinedHomeChoices(): void {
  hasExplicitOnboardingAppSelection = true;
  localStorage.setItem(selectedOnboardingAppsStorageKey, JSON.stringify([...selectedOnboardingApps]));
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

function onboardingConnectionApps(): HomeAppCatalogEntry[] {
  return homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available")
    .filter((app) => !hasExplicitOnboardingAppSelection || selectedOnboardingApps.has(app.id));
}

function selectNextConnectApp(): boolean {
  const next = onboardingConnectionApps().find((app) => !handledOnboardingConnectApps.has(app.id));
  onboardingConnectAppId = next?.id ?? null;
  return next !== undefined;
}

function resetOnboardingConnections(): void {
  handledOnboardingConnectApps.clear();
  onboardingConnectAppId = null;
}

function advanceOnboardingConnection(appId: HomeAppId | null): void {
  if (appId) handledOnboardingConnectApps.add(appId);
  clearServiceOnboardingResume();
  const hasNext = selectNextConnectApp();
  activeService = null;
  activeHomeAppId = null;
  route = "onboarding";
  if (!hasNext) {
    onboardingRoute = "pro";
    render();
    return;
  }
  onboardingRoute = "apps";
  render();
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

function selectedNativeAppIntent(appId: HomeAppId): NativeAppId | undefined {
  const nativeId = appId as NativeAppId;
  if (!supportedNativeAppIds.has(nativeId)) return undefined;
  if (!nativeSessionModeConfirmed(nativeId)) return undefined;
  const catalogApp = nativeApps.find((app) => app.id === nativeId);
  if (existingNativeSessionRequested(appId)) return nativeId;
  if (savedAccountMode === "use" && savedNativeApps.has(nativeId) && catalogApp?.availability === "installed" && catalogApp.isolatedProfileAvailable) return nativeId;
  const onboardingDedicatedIntent = onboardingServiceSetup
    && selectedOnboardingApps.has(appId)
    && savedAccountMode !== "clean"
    && nativeSessionModeForApp(nativeId) === "dedicated"
    && catalogApp?.availability === "installed"
    && catalogApp.isolatedProfileAvailable;
  return onboardingDedicatedIntent ? nativeId : undefined;
}

function providerWideInstalledNativeApp(appId: HomeAppId): NativeApp | undefined {
  const nativeId = appId as NativeAppId;
  if (savedAccountMode !== "use" || !supportedNativeAppIds.has(nativeId) || !savedNativeApps.has(nativeId)) return undefined;
  const service = services.find((candidate) => candidate.id === (nativeId as unknown as ServiceId));
  const anyAccountOverridden = service?.accounts.some((account) => detectedAccountChoices.get(detectedAccountChoiceKey(service.id, account.id)) === "osl") ?? false;
  if (anyAccountOverridden) return undefined;
  return nativeApps.find((candidate) => candidate.id === nativeId && candidate.availability === "installed" && candidate.isolatedProfileAvailable);
}

function nativeSessionModeForApp(appId: NativeAppId): NativeSessionMode {
  if (appId === "discord") return discordSessionMode;
  if (appId === "telegram") return telegramSessionMode;
  if (appId === "signal") return signalSessionMode;
  if (appId === "whatsapp") return whatsappSessionMode;
  if (appId === "outlook") return outlookSessionMode;
  return "existingSession";
}

function setNativeSessionMode(appId: NativeAppId, mode: NativeSessionMode): void {
  if (appId === "discord") discordSessionMode = mode;
  else if (appId === "telegram") telegramSessionMode = mode;
  else if (appId === "signal") signalSessionMode = mode;
  else if (appId === "whatsapp") whatsappSessionMode = mode;
  else outlookSessionMode = mode;
  confirmedNativeSessionModes.add(appId);
}

function nativeSessionModeConfirmed(appId: NativeAppId): boolean {
  return confirmedNativeSessionModes.has(appId);
}

function existingNativeSessionRequested(appId: HomeAppId): boolean {
  return (appId === "discord" || appId === "telegram" || appId === "signal" || appId === "whatsapp" || appId === "outlook") && nativeSessionModeForApp(appId) === "existingSession";
}

function separateNativeAccountAvailable(appId: NativeAppId): boolean {
  if (appId === "discord") return true;
  return nativeApps.some((app) => app.id === appId && app.availability === "installed" && app.isolatedProfileAvailable);
}

function nativeSessionModeSettingChoices(appId: NativeAppId, name: string): string {
  const mode = nativeSessionModeForApp(appId);
  const separate = separateNativeAccountAvailable(appId);
  return `<div class="native-mode-setting" role="radiogroup" aria-label="${escapeHtml(name)} account opening"><strong>${escapeHtml(name)}</strong><div><button type="button" role="radio" aria-checked="${mode === "existingSession"}" class="native-mode-option ${mode === "existingSession" ? "selected" : ""}" data-native-mode-app="${appId}" data-native-mode="existingSession">Existing</button><button type="button" role="radio" aria-checked="${mode === "dedicated"}" class="native-mode-option ${mode === "dedicated" ? "selected" : ""}" data-native-mode-app="${appId}" data-native-mode="dedicated" ${separate ? "" : "disabled"}>Separate</button></div></div>`;
}

function discordSessionModeChoices(): string {
  const qaStarting = discordQaShell && discordQaHostState === "starting";
  const existingDisabled = qaStarting ? "disabled aria-disabled=\"true\"" : "";
  const dedicatedDisabled = discordQaShell ? "disabled aria-disabled=\"true\"" : "";
  const existingLabel = qaStarting ? "Opening existing account…" : "Use existing account";
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open Discord"><button id="discord-existing-session" type="button" class="account-launch-choice" data-discord-session-mode="existingSession" ${existingDisabled}>${existingLabel}</button><button id="discord-dedicated-session" type="button" class="account-launch-choice" data-discord-session-mode="dedicated" ${dedicatedDisabled}>Use separate account</button></div>`;
}

function discordQaHostStatusMarkup(): string {
  if (!discordQaShell || activeHomeAppId !== "discord") return "";
  const label = discordQaHostState === "starting"
    ? "Discord native route is starting"
    : discordQaHostState === "hosted"
      ? "Discord native route is hosted"
      : "Discord native route failed; retry is available";
  const overlayLabel = discordQaOverlayState === "failed" && nativeProtectFailureNotice
    ? nativeProtectFailureNotice
    : discordQaOverlayState === "starting"
    ? "OSL Protect is starting"
    : discordQaOverlayState === "ready"
      ? "OSL Protect is ready"
      : "OSL Protect failed";
  return `<p id="discord-qa-host-state" class="form-status" role="status" aria-live="polite" data-host-state="${discordQaHostState}">${label}</p><p id="discord-qa-overlay-state" class="form-status" role="status" aria-live="polite" data-overlay-state="${discordQaOverlayState}">${overlayLabel}</p>`;
}

function telegramSessionModeChoices(): string {
  const separate = separateNativeAccountAvailable("telegram");
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open Telegram"><button type="button" class="account-launch-choice" data-telegram-session-mode="existingSession">Use existing account</button><button type="button" class="account-launch-choice" data-telegram-session-mode="dedicated" ${separate ? "" : "disabled"}>Use separate account</button></div>`;
}

function signalSessionModeChoices(): string {
  const separate = separateNativeAccountAvailable("signal");
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open Signal"><button type="button" class="account-launch-choice" data-signal-session-mode="existingSession">Use existing account</button><button type="button" class="account-launch-choice" data-signal-session-mode="dedicated" ${separate ? "" : "disabled"}>Use separate account</button></div>`;
}

function whatsappSessionModeChoices(): string {
  const separate = separateNativeAccountAvailable("whatsapp");
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open WhatsApp"><button type="button" class="account-launch-choice" data-whatsapp-session-mode="existingSession">Use existing account</button><button type="button" class="account-launch-choice" data-whatsapp-session-mode="dedicated" ${separate ? "" : "disabled"}>Use separate account</button></div>`;
}

function outlookSessionModeChoices(): string {
  const separate = separateNativeAccountAvailable("outlook");
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open Outlook"><button type="button" class="account-launch-choice" data-outlook-session-mode="existingSession">Use existing account</button><button type="button" class="account-launch-choice" data-outlook-session-mode="dedicated" ${separate ? "" : "disabled"}>Use separate account</button></div>`;
}

function defaultBrowserCompanionEligible(appId: HomeAppId | null): appId is HomeAppId {
  return appId !== null && ["instagram", "snapchat", "x", "messenger", "gmail", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud"].includes(appId);
}

function browserSessionModeChoices(): string {
  if (!defaultBrowserCompanionEligible(activeHomeAppId) || !selectedBrowserHasImportReceipt()) return "";
  const isolatedAvailable = selectedBrowserForLaunch() !== "duckduckgo";
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open browser account"><button type="button" class="account-launch-choice" data-browser-session-mode="existingBrowser">Use existing account</button><button type="button" class="account-launch-choice" data-browser-session-mode="isolatedOsl" ${isolatedAvailable ? "" : "disabled"}>Use separate account</button></div>`;
}

function selectedBrowserForLaunch(): BrowserImportId | null {
  return preferredBrowserId ?? defaultBrowserCompanionStatus.browserId;
}

function selectedBrowserHasImportReceipt(): boolean {
  const browserId = selectedBrowserForLaunch();
  return browserId !== null && completedBrowserImportIds.has(browserId);
}

function detectedAccountChoiceKey(serviceId: string, accountId: string): string {
  return `${serviceId}:${accountId}`;
}

function persistDetectedAccountChoices(): void {
  const valid = new Set(services.flatMap((service) => service.accounts.map((account) => detectedAccountChoiceKey(service.id, account.id))));
  for (const appId of detectedBrowserServices) valid.add(detectedAccountChoiceKey("browser", appId));
  for (const key of detectedAccountChoices.keys()) if (!valid.has(key)) detectedAccountChoices.delete(key);
  localStorage.setItem(detectedAccountChoicesStorageKey, JSON.stringify([...detectedAccountChoices]));
}

function detectedAppsContent(): string {
  const installedIds = new Set(nativeApps.filter((app) => app.availability === "installed").map((app) => app.id));
  const accounts = services.flatMap((service) => service.accounts.map((account) => ({ service, account })));
  const configuredAppIds = new Set(accounts.map(({ service, account }) => service.id === "email" && account.provider ? account.provider : service.id));
  const browserApps = homeAppsFromServices(services).filter((app) => detectedBrowserServices.has(app.id) && !configuredAppIds.has(app.id));
  const accountRows = accounts.map(({ service, account }) => {
      const id = detectedAccountChoiceKey(service.id, account.id);
      const choice = detectedAccountChoices.get(id) ?? "existing";
      const source = installedIds.has(service.id as NativeAppId) ? "Installed app" : savedAccountsReady ? "Imported browser data" : "OSL profile";
      return `<article class="detected-account-row detected-account-${choice}" data-detected-account-row="${escapeHtml(id)}"><span class="detected-account-logo service-brand-badge" data-service-brand="${service.id}">${serviceLogo(service.id)}</span><span class="detected-account-name"><strong>${escapeHtml(service.displayName)}</strong><small>${escapeHtml(account.displayHandle || account.label)}</small><em>${source}</em></span><label><span class="sr-only">How to open ${escapeHtml(account.label)}</span><select data-detected-account="${escapeHtml(id)}" aria-label="How to open ${escapeHtml(account.label)}"><option value="existing" ${choice === "existing" ? "selected" : ""}>Use current desktop session · provider-wide</option><option value="osl" ${choice === "osl" ? "selected" : ""}>Use isolated OSL profile · this account</option></select></label></article>`;
    }).join("");
  const browserRows = browserApps.map((app) => {
    const id = detectedAccountChoiceKey("browser", app.id);
    const choice = detectedAccountChoices.get(id) ?? "existing";
    return `<article class="detected-account-row detected-account-${choice}" data-detected-account-row="${escapeHtml(id)}"><span class="detected-account-logo">${homeAppLogo(app)}</span><span class="detected-account-name"><strong>${escapeHtml(app.displayName)}</strong><small>Current browser session</small><em>Found in selected browser history</em></span><label><span class="sr-only">How to open ${escapeHtml(app.displayName)}</span><select data-detected-account="${escapeHtml(id)}" aria-label="How to open ${escapeHtml(app.displayName)}"><option value="existing" ${choice === "existing" ? "selected" : ""}>Use current browser session</option><option value="osl" ${choice === "osl" ? "selected" : ""}>Create isolated OSL profile</option></select></label></article>`;
  }).join("");
  const rows = accountRows || browserRows
    ? `${accountRows}${browserRows}`
    : savedAccountsReady
      ? `<div class="empty-state"><strong>Current browser sessions ready</strong><p>Exact account names stay in your browser. Open an app from Home to use its current signed-in session.</p></div>`
      : `<div class="empty-state"><strong>No accounts detected</strong><p>You can add services from Home later.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Detected services</h1><div class="detected-launch-mode"><label for="detected-launch-select">Provider default</label><select id="detected-launch-select"><option value="use" ${savedAccountMode !== "clean" ? "selected" : ""}>Current desktop session · provider-wide</option><option value="clean" ${savedAccountMode === "clean" ? "selected" : ""}>Isolated OSL profiles only</option></select></div><div class="detected-account-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-detected-apps" type="button">Continue</button></div>`;
}

function installMissingAppsContent(): string {
  const missing = selectedNativeApps().filter((app) => app.availability !== "installed");
  const rows = missing.length
    ? missing.map((app) => app.availability === "installable"
      ? `<label class="saved-account-app"><span>${nativeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Optional Windows install</small></span></span><input type="checkbox" data-first-install="${app.id}" ${selectedFirstInstallApps.has(app.id) ? "checked" : ""}/></label>`
      : `<div class="saved-account-app unavailable"><span>${nativeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Install unavailable on this PC</small></span></span></div>`).join("")
    : `<div class="empty-state"><strong>No missing desktop apps</strong><p>Your selected desktop apps are already installed, or use the web.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Install missing apps</h1><p class="compact-lead onboarding-centered-copy">Optional installs start through Windows after Continue.</p><div class="setup-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-install-apps" type="button">Continue</button></div>`;
}

function onboardingAppsContent(): string {
  const apps = onboardingConnectionApps().filter((app) => !handledOnboardingConnectApps.has(app.id));
  const choices = apps.length
    ? `<div class="onboarding-app-grid" role="radiogroup" aria-label="Apps left to connect">${apps.map((app) => `<button type="button" role="radio" class="onboarding-app ${onboardingConnectAppId === app.id ? "selected" : ""}" data-connect-app-choice="${app.id}" aria-checked="${onboardingConnectAppId === app.id}"><span class="app-logo-plate">${homeAppLogo(app)}</span><strong>${escapeHtml(app.displayName)}</strong></button>`).join("")}</div>`
    : `<div class="empty-state"><strong>Selected apps reviewed</strong><p>Finish setup.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Connect your apps</h1><p class="compact-lead onboarding-centered-copy">Open each selected app, or skip it for now.</p>${choices}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-connect-app" type="button" ${onboardingConnectAppId ? "" : "disabled"}>Open selected app</button><button class="browser-import-skip" id="skip-connect-app" type="button">${onboardingConnectAppId ? "Not now" : "Continue"}</button></div>`;
}

function browserImportContent(): string {
  const installed = browserImports.filter((browser) => browser.installed);
  const selectedCount = installed.filter((browser) => selectedBrowserImports.has(browser.id)).length;
  const allSelected = installed.length > 0 && selectedCount === installed.length;
  const sources = installed.length
    ? `<section class="browser-detected-sources" aria-label="Detected browsers"><div class="browser-detected-heading"><span>${selectedCount} selected</span><button type="button" id="toggle-all-browser-imports" ${browserImportBusy ? "disabled" : ""}>${allSelected ? "Clear all" : "Select all"}</button></div><div class="browser-detected-list">${installed.map((browser) => `<label class="browser-detected-item">${browserLogo(browser.id)}<strong>${escapeHtml(browser.displayName)}</strong><input type="checkbox" data-browser-source="${browser.id}" ${selectedBrowserImports.has(browser.id) ? "checked" : ""} ${browserImportBusy ? "disabled" : ""} aria-label="Import from ${escapeHtml(browser.displayName)}"></label>`).join("")}</div></section>`
    : `<p class="saved-account-truth">No supported browser detected.</p>`;
  const ready = savedAccountsReady
    ? `<div class="saved-account-browser-note"><strong>Browser import completed</strong><small>Account contents remain browser-owned.</small></div>`
    : "";
  const failure = browserImportFailureNotice ? `<p class="saved-account-browser-error" role="alert">${escapeHtml(browserImportFailureNotice)}</p>` : "";
  const importEnabled = selectedCount > 0 && firefoxStatus.availability === "installed" && !browserReadinessBusy && !browserImportBusy;
  const secondaryLabel = browserImportCancelling ? "Closing Firefox…" : browserImportBusy ? "Cancel import" : "Not now";
  const primaryLabel = browserImportBusy ? escapeHtml(browserImportProgress || "Working…") : `Import selected${selectedCount ? ` (${selectedCount})` : ""}`;
  return `<h1 id="route-heading" tabindex="-1">Import browser data</h1><p class="compact-lead onboarding-centered-copy">Choose only the browsers you want to import.</p>${sources}${ready}${failure}<div class="setup-footer onboarding-actions browser-import-actions-primary"><button class="button primary" id="import-saved-accounts" type="button" ${importEnabled ? "" : "disabled"}>${primaryLabel}</button><button class="browser-import-skip" id="continue-browser-import" type="button" ${browserImportCancelling ? "disabled" : ""}>${secondaryLabel}</button></div><p class="saved-account-truth">Stays inside OSL. Windows may ask before protected passwords are used.</p>`;
}

async function importOneBrowser(source: BrowserImportId, position: number, total: number): Promise<Awaited<ReturnType<typeof beginProtectedBrowserImport>>> {
  browserImportProgress = `Browser ${position} of ${total}`;
  render();
  const operationId = createProtectedBrowserImportOperationId();
  const operation = { operationId, result: beginProtectedBrowserImport([source], operationId) };
  browserImportOperation = operation;
  let deadlineTimer: ReturnType<typeof globalThis.setTimeout> | undefined;
  const deadline = new Promise<never>((_resolve, reject) => {
    deadlineTimer = globalThis.setTimeout(() => {
      void cancelProtectedBrowserImport(operationId).finally(() => {
        reject(new Error("This browser took too long and was closed safely. Try again."));
      });
    }, protectedBrowserImportSourceDeadlineMs);
  });
  try {
    const result = await Promise.race([operation.result, deadline]);
    await finishProtectedBrowserImport(operationId);
    return result;
  } finally {
    if (deadlineTimer !== undefined) globalThis.clearTimeout(deadlineTimer);
    if (browserImportOperation === operation) browserImportOperation = null;
    void operation.result.catch(() => undefined);
  }
}

function persistSavedAccountPreferences(): void {
  localStorage.setItem(savedAccountModeStorageKey, savedAccountMode);
  localStorage.setItem(savedNativeAppsStorageKey, JSON.stringify([...savedNativeApps]));
  localStorage.setItem(confirmedNativeSessionModesStorageKey, JSON.stringify([...confirmedNativeSessionModes]));
  if (confirmedNativeSessionModes.has("discord")) localStorage.setItem(discordSessionModeStorageKey, discordSessionMode);
  if (confirmedNativeSessionModes.has("telegram")) localStorage.setItem(telegramSessionModeStorageKey, telegramSessionMode);
  if (confirmedNativeSessionModes.has("signal")) localStorage.setItem(signalSessionModeStorageKey, signalSessionMode);
  if (confirmedNativeSessionModes.has("whatsapp")) localStorage.setItem(whatsappSessionModeStorageKey, whatsappSessionMode);
  if (confirmedNativeSessionModes.has("outlook")) localStorage.setItem(outlookSessionModeStorageKey, outlookSessionMode);
}

function bindSavedAccountControls(): void {
  const finishNativeAccountChoice = (appId: NativeAppId): void => {
    nativeHostFailureNotice = "";
    savedAccountMode = "use";
    savedNativeApps.add(appId);
    persistSavedAccountPreferences();
    if (route === "service" && activeHomeAppId === appId && activeService) {
      void setupEmbeddedApp();
      return;
    }
    render();
  };
  document.querySelectorAll<HTMLButtonElement>("[data-browser-session-mode]").forEach((button) => button.addEventListener("click", () => {
    const requested = button.dataset.browserSessionMode;
    if (requested !== "isolatedOsl" && requested !== "existingBrowser") return;
    useDefaultBrowserCompanion = requested === "existingBrowser";
    localStorage.setItem("osl-default-browser-companion-v1", String(useDefaultBrowserCompanion));
    browserSessionModeConfirmed = true;
    localStorage.setItem(browserSessionModeConfirmedStorageKey, "true");
    nativeHostFailureNotice = "";
    if (route === "service" && activeHomeAppId && activeService) {
      void setupEmbeddedApp();
      return;
    }
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-preferred-browser]").forEach((button) => button.addEventListener("click", () => {
    const requested = button.dataset.preferredBrowser ?? "";
    preferredBrowserId = supportedBrowserId(requested) ? requested : null;
    persistBrowserAccountPreferences();
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-discord-session-mode]").forEach((button) => button.addEventListener("click", () => {
    setNativeSessionMode("discord", parseDiscordSessionMode(button.dataset.discordSessionMode));
    finishNativeAccountChoice("discord");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-telegram-session-mode]").forEach((button) => button.addEventListener("click", () => {
    setNativeSessionMode("telegram", parseNativeSessionMode(button.dataset.telegramSessionMode));
    finishNativeAccountChoice("telegram");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-signal-session-mode]").forEach((button) => button.addEventListener("click", () => {
    setNativeSessionMode("signal", parseNativeSessionMode(button.dataset.signalSessionMode));
    finishNativeAccountChoice("signal");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-whatsapp-session-mode]").forEach((button) => button.addEventListener("click", () => {
    setNativeSessionMode("whatsapp", parseNativeSessionMode(button.dataset.whatsappSessionMode));
    finishNativeAccountChoice("whatsapp");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-outlook-session-mode]").forEach((button) => button.addEventListener("click", () => {
    setNativeSessionMode("outlook", parseNativeSessionMode(button.dataset.outlookSessionMode));
    finishNativeAccountChoice("outlook");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-native-mode-app]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.nativeModeApp as NativeAppId;
    if (!supportedNativeAppIds.has(appId)) return;
    setNativeSessionMode(appId, parseNativeSessionMode(button.dataset.nativeMode));
    savedAccountMode = "use";
    savedNativeApps.add(appId);
    persistSavedAccountPreferences();
    render();
  }));
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
    if (input.checked) {
      savedNativeApps.add(appId);
      savedAccountMode = "use";
    }
    else savedNativeApps.delete(appId);
    persistSavedAccountPreferences();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-background-install]").forEach((button) => button.addEventListener("click", () => {
    void startBackgroundInstall(button.dataset.backgroundInstall as NativeAppId);
  }));
}

function bindBrowserImportControls(): void {
  document.querySelectorAll<HTMLInputElement>("[data-browser-source]").forEach((input) => input.addEventListener("change", () => {
    if (browserImportBusy) return;
    const source = input.dataset.browserSource as BrowserImportId;
    if (!browserImports.some((browser) => browser.installed && browser.id === source)) return;
    if (input.checked) selectedBrowserImports.add(source);
    else selectedBrowserImports.delete(source);
    render();
  }));
  document.querySelector<HTMLButtonElement>("#toggle-all-browser-imports")?.addEventListener("click", () => {
    if (browserImportBusy) return;
    const installed = browserImports.filter((browser) => browser.installed).map((browser) => browser.id);
    const allSelected = installed.length > 0 && installed.every((source) => selectedBrowserImports.has(source));
    selectedBrowserImports = new Set(allSelected ? [] : installed);
    render();
  });
  document.querySelector<HTMLButtonElement>("#import-saved-accounts")?.addEventListener("click", async () => {
    if (browserImportBusy || savedAccountsReady) return;
    const selected = browserImports.filter((browser) => browser.installed && selectedBrowserImports.has(browser.id)).map((browser) => browser.id);
    if (selected.length === 0) return;
    browserImportBusy = true;
    browserImportFailureNotice = "";
    render();
    try {
      const results = [];
      for (const [index, source] of selected.entries()) results.push(await importOneBrowser(source, index + 1, selected.length));
      if (results.some((result, index) => !result.sourceSelected || result.selectedSources.length !== 1 || result.selectedSources[0] !== selected[index])) throw new Error("The detected browser queue could not be completed safely.");
      const readyKey = activeBrowserAccountsReadyStorageKey();
      if (readyKey) localStorage.setItem(readyKey, "true");
      savedAccountsReady = true;
      detectedBrowserServices = new Set(results.flatMap((result) => result.detectedServices));
      const detectedKey = activeDetectedBrowserServicesStorageKey();
      if (detectedKey) localStorage.setItem(detectedKey, JSON.stringify([...detectedBrowserServices]));
      savedAccountMode = "use";
      persistSavedAccountPreferences();
      const needsFollowUp = results.some((result) => result.sessionOnlySources.length || result.passwordFollowUpSources.length);
      showToast(needsFollowUp ? "Imported supported data; existing sessions stay available" : "Browser import finished");
      onboardingRoute = "detected";
      nativeApps = await loadNativeApps().catch(() => nativeApps);
    } catch (failure) {
      browserImportFailureNotice = localActionError(failure, "Browser import did not finish");
      showToast(browserImportFailureNotice);
    } finally {
      browserImportBusy = false;
      browserImportProgress = "";
      render();
    }
  });
  document.querySelector<HTMLButtonElement>("#continue-browser-import")?.addEventListener("click", async () => {
    if (browserImportCancelling) return;
    browserImportCancelling = true;
    render();
    const operation = browserImportOperation;
    try {
      if (operation) await cancelProtectedBrowserImport(operation.operationId);
      await operation?.result.catch(() => undefined);
    } catch (failure) {
      browserImportFailureNotice = localActionError(failure, "This browser import belongs to another OSL identity or operation");
      browserImportCancelling = false;
      render();
      return;
    }
    browserImportOperation = null;
    const pendingKey = activeBrowserImportPendingStorageKey();
    if (pendingKey) localStorage.removeItem(pendingKey);
    browserImportBusy = false;
    browserImportCancelling = false;
    onboardingRoute = "detected";
    nativeApps = await loadNativeApps().catch(() => nativeApps);
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
    if (catalog) {
      browserImports = catalog;
      const installed = new Set(catalog.filter((browser) => browser.installed).map((browser) => browser.id));
      selectedBrowserImports = new Set([...selectedBrowserImports].filter((source) => installed.has(source)));
      if (!browserImportSelectionInitialized && installed.size > 0) {
        selectedBrowserImports = installed;
        browserImportSelectionInitialized = true;
      }
    }
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
  if (!recoveryBundle) return `<p class="eyebrow">Recovery</p><h1 id="route-heading" tabindex="-1">No recovery secret is available</h1><button class="button primary" data-onboarding="pro">Continue</button>`;
  const accountRecovery = recoveryBundle.identityPhrase ? `<code>${escapeHtml(recoveryBundle.identityPhrase)}</code>` : `<p>Keep using the account recovery phrase you imported.</p>`;
  return `<h1 id="route-heading" tabindex="-1" class="recovery-heading">Save your recovery kit</h1><section class="setup-surface recovery-surface"><article class="recovery-kit-item"><span>1</span><div><strong>Account recovery</strong>${accountRecovery}</div></article><article class="recovery-kit-item"><span>2</span><div><strong>Password recovery</strong><code>${escapeHtml(recoveryBundle.passwordPhrase)}</code></div></article><details class="recovery-account-details"><summary>Account details</summary><code>${escapeHtml(recoveryBundle.userId)}</code></details><button class="button" id="copy-recovery-kit" type="button">Copy recovery kit</button><label class="check"><input id="recovery-saved" type="checkbox" ${recoverySavedAcknowledged ? "checked" : ""}/><span>I saved my recovery kit.</span></label><button class="button primary" id="recovery-continue" ${recoverySavedAcknowledged ? "" : "disabled"}>Continue</button></section>`;
}

function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  if (!setup) return `<section class="unlock-card" aria-labelledby="route-heading"><div class="unlock-logo-stage" aria-hidden="true"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><h1 id="route-heading" tabindex="-1">Enter your password</h1><form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>Unlock</button></form><button class="text-back" data-onboarding="welcome">← Back</button></section>`;
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><form class="setup-surface password-form" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error account-create-status"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><p class="account-create-status" id="account-create-status" aria-live="polite"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>${action}</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

function sendingSetupContent(): string {
  const selectedMode: SendMode = setup.sendMode === "manual" ? "clipboard" : setup.sendMode;
  const option = (mode: SendMode, title: string, tone: "safe" | "caution" | "danger", badge = "", warning = "") => `<div class="send-choice send-choice-${tone} ${selectedMode === mode ? "selected" : ""}"><button type="button" data-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${title}</strong>${badge ? `<small class="send-mode-badge">${badge}</small>` : ""}</span>${manualSendingAnimationMarkup(mode)}</button>${warning ? `<small class="send-choice-warning">${warning}</small>` : ""}</div>`;
  const risk = needsRiskAcceptance(selectedMode)
    ? `<label class="send-risk"><input id="accept-send-risk" type="checkbox" ${setup.acceptedRisk && setup.acceptedRiskForMode === selectedMode ? "checked" : ""}/><span><strong>I understand</strong><small>Experimental sending can target the wrong chat if an app changes. OSL stops unless it can verify the exact app, account, chat, and composer. Each account asks again.</small></span></label>`
    : "";
  return `<h1 id="route-heading" tabindex="-1">Privacy and sending</h1><h2 class="setup-section-heading">Choose how to send</h2><div class="send-choice-grid">${option("clipboard", "Copy", "safe", "Safest")}${option("double", "Double Enter", "caution", "", "Can possibly break ToS")}${option("single", "Single Enter", "danger", "", "Breaks some ToS · risky")}</div>${risk}<div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding" ${canCompleteSetup({ ...setup, sendMode: selectedMode }) ? "" : "disabled"}>Continue</button></div>`;
}

function coverDraftSetupContent(): string {
  const typedCover = [..."LOOKS GOOD"].map((character) => `<i>${character === " " ? "&nbsp;" : character}</i>`).join("");
  return `<h1 id="route-heading" tabindex="-1">Choose cover insertion</h1><div class="cover-mode-compare" aria-label="Free and Pro cover insertion"><article class="cover-mode-choice selected"><span>Free</span><strong>Insert on send</strong><small>Press Enter. The whole cover appears together.</small><span class="cover-composer cover-atomic-composer" aria-label="LOOKS GOOD appears at once"><em class="cover-atomic-preview">LOOKS GOOD</em><b aria-hidden="true">↵</b></span></article><article class="cover-mode-choice cover-mode-pro" aria-label="Pro pending: AI cover types with you"><span>Pro · pending</span><strong>Type naturally</strong><small>AI writes the cover one character at a time.</small><span class="cover-composer cover-typing-preview" aria-label="LOOKS GOOD types one character at a time"><em aria-hidden="true">${typedCover}</em><b class="cover-caret" aria-hidden="true"></b></span></article></div><p class="send-mode-truth">OSL stops if it cannot verify the exact destination.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-cover-draft" type="button">Continue</button></div>`;
}

function onboardingPasswordRoleContent(role: "stealth" | "burn"): string {
  const stealth = role === "stealth";
  const configured = stealth ? passwordRoleStatus?.stealthPasswordSet : passwordRoleStatus?.burnPasswordSet;
  const title = stealth ? "Stealth password" : "Burn password";
  const detail = stealth ? "Opens an empty workspace without loading your private data." : "Erases OSL data from this device when entered at sign in.";
  const next = stealth ? "burnpass" : "mullvad";
  if (configured) {
    return `<h1 id="route-heading" tabindex="-1">${title}</h1><div class="password-role-ready"><span class="status-tag">Set</span><p>${detail}</p></div><div class="setup-footer onboarding-actions"><button class="button primary" data-password-role-next="${next}" type="button">Continue</button></div>`;
  }
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><p class="compact-lead onboarding-centered-copy">${detail}</p><form class="setup-surface password-form onboarding-role-form" data-onboarding-password-role="${role}" data-onboarding-password-next="${next}" novalidate><label for="setup-${role}-current">Current password</label><div class="password-input-row"><input id="setup-${role}-current" name="current" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-current" aria-label="Show current password">${passwordEyeIcon()}</button></div><label for="setup-${role}-alternate">New ${stealth ? "stealth" : "burn"} password</label><div class="password-input-row"><input id="setup-${role}-alternate" name="alternate" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-alternate" aria-label="Show new password">${passwordEyeIcon()}</button></div><label for="setup-${role}-confirm">Confirm</label><div class="password-input-row"><input id="setup-${role}-confirm" name="confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="setup-${role}-confirm" aria-label="Show password confirmation">${passwordEyeIcon()}</button></div><p class="unlock-error" data-onboarding-role-error role="alert"></p><button class="button primary" type="submit" disabled>Set password</button></form><button class="text-button onboarding-role-skip" type="button" data-skip-onboarding-password-role="${next}">Not now</button>`;
}

function onboardingPrivacyContent(): string {
  const toggle = (id: SetupPrivacyChoiceId, title: string, detail: string) => `<label class="setup-status-row interactive"><span><strong>${title}</strong><small>${detail}</small></span><input type="checkbox" data-setup-privacy="${id}" ${setupPrivacyChoices.has(id) ? "checked" : ""}/></label>`;
  return `<h1 id="route-heading" tabindex="-1">Privacy</h1><section class="privacy-toggle-group"><h2>On screen</h2><div class="setup-list"><label class="setup-status-row interactive"><span><strong>Windows capture resistance</strong><small>Exclude OSL from ordinary Windows capture.</small></span><input id="onboarding-screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label>${toggle("hide-notifications", "Hide notification content", "Show the app, not the message.")}${toggle("disable-previews", "Disable link previews", "Do not render inline URL cards in messages or drafts.")}</div></section><section class="privacy-toggle-group"><h2>Links</h2><div class="setup-list">${toggle("ip-grabber-protection", "IP-grabber protection", "Block known logger domains locally before a link opens.")}${toggle("external-default-browser", "Open links in your default browser", "Send checked HTTP links to Windows instead of opening them inside OSL.")}</div></section><section class="privacy-toggle-group"><h2>When away</h2><div class="setup-list">${toggle("auto-lock", "Auto-lock on idle", "Return to the password gate and clear the live key after 5 minutes.")}${toggle("clear-clipboard", "Clear copied messages", "Clear unchanged protected clipboard text after 30 seconds in the Windows app.")}</div></section><section class="decrypt-display-note"><strong>Decrypt display</strong><span>Set per protected chat.</span></section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-onboarding-privacy" type="button">Continue</button></div>`;
}

function mullvadSetupContent(): string {
  const availability = mullvadStatus.availability;
  const action = availability === "installed"
    ? `<button class="button" id="open-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Opening…" : "Use my Mullvad session"}</button>`
    : availability === "installable"
      ? `<button class="button" id="install-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Starting…" : "Install Mullvad"}</button>`
      : `<p class="mullvad-unavailable">Mullvad or Windows App Installer was not found.</p>`;
  const notice = mullvadSetupNotice
    ? `<p class="mullvad-setup-notice" role="status">${escapeHtml(mullvadSetupNotice)}</p>`
    : "";
  const choice = (value: "auto" | "off", title: string) => `<button class="mullvad-choice ${mullvadPreference === value ? "selected" : ""}" type="button" data-mullvad-choice="${value}" aria-pressed="${mullvadPreference === value}">${title}</button>`;
  return `<section class="mullvad-setup" aria-labelledby="route-heading"><div class="mullvad-mark" aria-hidden="true"><img src="${mullvadLogoUrl}" alt=""/></div><h1 id="route-heading" tabindex="-1">Mullvad</h1><p>Optional network privacy.</p><div class="mullvad-actions">${action}</div>${notice}<div class="mullvad-choice-list">${choice("auto", "Open Mullvad when OSL starts")}${choice("off", "Don't do that")}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-mullvad" type="button" ${mullvadPreference ? "" : "disabled"}>Continue</button><button class="text-button" id="skip-mullvad" type="button">Not now</button></div></section>`;
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function previousSetupRoute(current: OnboardingRoute): OnboardingRoute {
  const routes: Partial<Record<OnboardingRoute, OnboardingRoute>> = {
    browser: "recovery",
    detected: "browser",
    apps: "detected",
    pro: "apps",
    sending: "pro",
    cover: "sending",
    passwords: "cover",
    burnpass: "passwords",
    mullvad: "burnpass",
    privacy: "mullvad",
    scrub: "privacy",
  };
  return routes[current] ?? "welcome";
}

function scrubSetupContent(): string {
  const accounts = scrubAccountSelections();
  if (scrubSetupStep === "intro") return `<section class="scrub-intro"><div class="scrub-hero" aria-hidden="true"><span class="scrub-hero-card"><i></i><i></i><i></i><b></b></span><span class="scrub-hero-sweep"></span></div><h1 id="route-heading" tabindex="-1">Scrub</h1><p>This device only. Nothing is deleted without explicit confirmation.</p><div class="scrub-intro-actions"><button class="button" id="skip-scrub-setup" type="button">Finish setup</button><button class="button primary" id="start-scrub-setup" type="button">Do Scrub</button></div></section>`;
  const cards = accounts.length
    ? `<div class="scrub-account-grid">${accounts.map(({ selection, service, account }) => { const id = `${selection.serviceId}:${selection.accountId}`; return `<button class="scrub-account-choice ${selectedOnboardingScrubAccounts.has(id) ? "selected" : ""}" type="button" data-scrub-target="${escapeHtml(id)}" aria-pressed="${selectedOnboardingScrubAccounts.has(id)}"><span class="scrub-account-logo service-brand-badge" data-service-brand="${selection.serviceId}">${serviceLogo(selection.serviceId as ServiceId)}</span><strong>${escapeHtml(account)}</strong><small>${escapeHtml(service)}</small></button>`; }).join("")}</div>`
    : `<div class="empty-state"><strong>No connected accounts yet</strong><p>You can run Scrub later from Home.</p></div>`;
  if (scrubSetupStep === "accounts") return `<h1 id="route-heading" tabindex="-1">Choose accounts</h1><div class="scrub-selection-controls"><button class="text-button" id="select-all-scrub" type="button">Select all</button><button class="text-button" id="clear-scrub-selection" type="button">Clear</button></div>${cards}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-scrub-accounts" type="button">Continue</button></div>`;
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  return `<h1 id="route-heading" tabindex="-1">Configure Scrub</h1><h2 class="setup-section-heading">Mode</h2><div class="send-mode-list"><button class="send-mode-option ${onboardingScrubMode === "scrub" ? "selected" : ""}" type="button" data-scrub-mode="scrub"><span><strong>Scrub</strong><small class="send-mode-badge">Recommended</small></span><small>Review every match before removing anything.</small></button><button class="send-mode-option ${onboardingScrubMode === "autoscrub" ? "selected" : ""} ${proActive ? "" : "disabled"}" type="button" data-scrub-mode="autoscrub" ${proActive ? "" : "disabled"}><span><strong>AutoScrub</strong><small class="send-mode-badge">Pro</small></span><small>Use the saved plan automatically.</small></button></div><h2 class="setup-section-heading">Categories</h2>${scrubCategoryChooserMarkup(true)}<p class="scrub-config-safety"><strong>Review before removing.</strong> Nothing is deleted during setup; later removal still requires explicit confirmation on an editable list.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-scrub-setup" type="button">Save &amp; finish</button></div>`;
}

function scrubAccountSelections(): Array<{ selection: ScrubAccountSelection; service: string; account: string }> {
  const servicePattern = /^[a-z0-9_-]{1,32}$/u;
  const accountPattern = /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u;
  return services.flatMap((service) => service.accounts.flatMap((account) => {
    if (!servicePattern.test(service.id) || !accountPattern.test(account.id)) return [];
    return [{ selection: { serviceId: service.id, accountId: account.id }, service: service.displayName, account: account.label }];
  })).slice(0, 32);
}

function activeScrubSetupPlanStorageKey(): string | null {
  return identityScopedStorageKey(scrubSetupPlanStorageKey);
}

function applySavedScrubSetupPlan(): void {
  const key = activeScrubSetupPlanStorageKey();
  const raw = key ? localStorage.getItem(key) : null;
  if (raw === null) return;
  const available = new Set(scrubAccountSelections().map(({ selection }) => `${selection.serviceId}:${selection.accountId}`));
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const plan = parseScrubSetupPlan(raw, available, defaultScrubSignalGroups, proActive);
  onboardingScrubMode = plan.mode === "autoscrub" ? "autoscrub" : "scrub";
  selectedOnboardingScrubAccounts = new Set(plan.targetIds);
  enabledScrubSignals = new Set(plan.signalGroups);
  const target = scrubAccountSelections().find(({ selection }) => plan.targetIds.includes(`${selection.serviceId}:${selection.accountId}`));
  if (!target) return;
  if (target.selection.serviceId === "email") autoScrubPathId = "gmail-web";
  else if (target.selection.serviceId === "discord") autoScrubPathId = "discord";
  else if (target.selection.serviceId === "telegram") autoScrubPathId = "telegram-web";
  else return;
  autoScrubAccountId = target.selection.accountId;
}

function saveScrubSetupPlan(mode: ScrubSetupPlan["mode"]): void {
  const key = activeScrubSetupPlanStorageKey();
  if (!key) return;
  const plan = parseScrubSetupPlan(JSON.stringify({
    mode,
    targetIds: [...selectedOnboardingScrubAccounts],
    signalGroups: [...enabledScrubSignals],
  }), new Set(scrubAccountSelections().map(({ selection }) => `${selection.serviceId}:${selection.accountId}`)), defaultScrubSignalGroups, licenseState.access === "pro" || licenseState.access === "offlineGrace");
  localStorage.setItem(key, JSON.stringify(plan));
}

function bindOnboarding(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboarding as OnboardingRoute; render(); }));
  document.querySelector<HTMLFormElement>("#activation-form")?.addEventListener("submit", (event) => void activatePro(event));
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
      showToast("Recovery kit copied — save it, then confirm below");
    } catch {
      showToast("Couldn’t copy the recovery kit");
    }
  });
  recoverySaved?.addEventListener("change", () => {
    recoverySavedAcknowledged = recoverySaved.checked;
    if (recoveryContinue) recoveryContinue.disabled = !recoverySavedAcknowledged;
  });
  recoveryContinue?.addEventListener("click", () => {
    recoveryBundle = null;
    recoverySavedAcknowledged = false;
    resetOnboardingBranch();
    resetOnboardingConnections();
    onboardingAppChoicesConfirmed = false;
    onboardingRoute = "browser";
    render();
    void refreshBrowserImportReadiness();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app-choice]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.onboardingAppChoice as HomeAppId;
    if (selectedOnboardingApps.has(appId)) selectedOnboardingApps.delete(appId);
    else selectedOnboardingApps.add(appId);
    hasExplicitOnboardingAppSelection = true;
    localStorage.setItem(selectedOnboardingAppsStorageKey, JSON.stringify([...selectedOnboardingApps]));
    onboardingConnectAppId = null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-connect-app-choice]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.connectAppChoice as HomeAppId;
    if (!onboardingConnectionApps().some((app) => app.id === appId) || handledOnboardingConnectApps.has(appId)) return;
    onboardingConnectAppId = appId;
    render();
  }));
  document.querySelector<HTMLButtonElement>("#continue-app-choice")?.addEventListener("click", async () => {
    if (!await ensureNativeCatalogForAppChoice()) return;
    persistCombinedHomeChoices();
    onboardingAppChoicesConfirmed = true;
    resetOnboardingConnections();
    if (selectNextConnectApp()) {
      onboardingRoute = "apps";
      render();
      return;
    }
    onboardingRoute = "pro";
    render();
  });
  document.querySelector<HTMLButtonElement>("#continue-detected-apps")?.addEventListener("click", () => {
    if (savedAccountMode === "ask") savedAccountMode = nativeApps.some((app) => app.availability === "installed") ? "use" : "clean";
    if (savedAccountMode === "use") nativeApps.filter((app) => app.availability === "installed").forEach((app) => savedNativeApps.add(app.id));
    persistSavedAccountPreferences();
    persistDetectedAccountChoices();
    onboardingAppChoicesConfirmed = false;
    onboardingRoute = "apps";
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
    selectNextConnectApp();
    onboardingRoute = "apps";
    render();
  });
  document.querySelector<HTMLSelectElement>("#detected-launch-select")?.addEventListener("change", (event) => {
    savedAccountMode = (event.currentTarget as HTMLSelectElement).value === "clean" ? "clean" : "use";
    persistSavedAccountPreferences();
  });
  document.querySelectorAll<HTMLSelectElement>("[data-detected-account]").forEach((select) => select.addEventListener("change", () => {
    const id = select.dataset.detectedAccount;
    if (!id) return;
    const choice = select.value === "osl" ? "osl" : "existing";
    detectedAccountChoices.set(id, choice);
    persistDetectedAccountChoices();
    const row = document.querySelector<HTMLElement>(`[data-detected-account-row="${CSS.escape(id)}"]`);
    row?.classList.toggle("detected-account-osl", choice === "osl");
    row?.classList.toggle("detected-account-existing", choice === "existing");
  }));
  document.querySelector<HTMLButtonElement>("#skip-connect-app")?.addEventListener("click", () => {
    if (onboardingConnectAppId) handledOnboardingConnectApps.add(onboardingConnectAppId);
    if (selectNextConnectApp()) onboardingRoute = "apps";
    else onboardingRoute = "pro";
    render();
  });
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
    else { onboardingRoute = "scrub"; scrubSetupStep = "intro"; render(); }
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
    if (onboardingRoute !== "sending" && onboardingRoute !== "privacy") return;
    if (setup.sendMode === "manual") setup.sendMode = "clipboard";
    if (!canCompleteSetup(setup)) return;
    setup.placementMode = "atomic";
    onboardingRoute = "cover";
    render();
  });
  document.querySelector("#continue-cover-draft")?.addEventListener("click", () => { onboardingRoute = "passwords"; render(); });
  bindOnboardingPasswordRole();
  document.querySelectorAll<HTMLButtonElement>("button[data-password-role-next]").forEach((button) => button.addEventListener("click", () => {
    onboardingRoute = button.dataset.passwordRoleNext as OnboardingRoute;
    render();
    if (onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (onboardingRoute === "mullvad") void refreshMullvadSetup();
  }));
  document.querySelectorAll<HTMLButtonElement>("button[data-skip-onboarding-password-role]").forEach((button) => button.addEventListener("click", () => {
    const next = button.dataset.skipOnboardingPasswordRole as OnboardingRoute;
    onboardingRoute = next;
    render();
    if (next === "browser") void refreshBrowserImportReadiness();
    if (next === "mullvad") void refreshMullvadSetup();
  }));
  document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => {
    notificationPreviewContent = !setupPrivacyChoices.has("hide-notifications");
    localStorage.setItem(notificationPreviewStorageKey, String(notificationPreviewContent));
    persistActivePrivacyFeatureChoices();
    onboardingRoute = "scrub";
    scrubSetupStep = "intro";
    render();
  });
  document.querySelector<HTMLInputElement>("#onboarding-screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelector("#skip-mullvad")?.addEventListener("click", () => { onboardingRoute = "privacy"; render(); });
  document.querySelector("#install-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("install"));
  document.querySelector("#open-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("open"));
  document.querySelectorAll<HTMLButtonElement>("[data-mullvad-choice]").forEach((button) => button.addEventListener("click", () => {
    mullvadPreference = button.dataset.mullvadChoice === "auto" ? "auto" : "off";
    localStorage.setItem(mullvadStartupStorageKey, String(mullvadPreference === "auto"));
    render();
  }));
  document.querySelector("#continue-mullvad")?.addEventListener("click", () => {
    if (!mullvadPreference) return;
    if (mullvadPreference === "auto" && mullvadStatus.availability === "installed") void openMullvad().catch(() => undefined);
    onboardingRoute = "privacy";
    render();
  });
  document.querySelectorAll<HTMLInputElement>("[data-setup-privacy]").forEach((input) => input.addEventListener("change", () => {
    const id = input.dataset.setupPrivacy;
    if (!id || !setupPrivacyChoiceIds.includes(id as SetupPrivacyChoiceId)) return;
    const choice = id as SetupPrivacyChoiceId;
    if (input.checked) setupPrivacyChoices.add(choice); else setupPrivacyChoices.delete(choice);
    persistActivePrivacyFeatureChoices();
  }));
  document.querySelector("#skip-scrub-setup")?.addEventListener("click", () => { saveScrubSetupPlan("skip"); void completeOnboarding(); });
  document.querySelector("#start-scrub-setup")?.addEventListener("click", () => { scrubSetupStep = "accounts"; selectedOnboardingScrubAccounts = new Set(scrubAccountSelections().map(({ selection }) => `${selection.serviceId}:${selection.accountId}`)); render(); });
  document.querySelector("#continue-scrub-accounts")?.addEventListener("click", () => { scrubSetupStep = "options"; render(); });
  document.querySelector("#select-all-scrub")?.addEventListener("click", () => { selectedOnboardingScrubAccounts = new Set(scrubAccountSelections().map(({ selection }) => `${selection.serviceId}:${selection.accountId}`)); render(); });
  document.querySelector("#clear-scrub-selection")?.addEventListener("click", () => { selectedOnboardingScrubAccounts.clear(); render(); });
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-target]").forEach((button) => button.addEventListener("click", () => { const id = button.dataset.scrubTarget; if (!id) return; if (selectedOnboardingScrubAccounts.has(id)) selectedOnboardingScrubAccounts.delete(id); else selectedOnboardingScrubAccounts.add(id); render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-mode]").forEach((button) => button.addEventListener("click", () => { onboardingScrubMode = button.dataset.scrubMode === "autoscrub" ? "autoscrub" : "scrub"; render(); }));
  document.querySelectorAll<HTMLInputElement>("[data-scrub-category]").forEach((input) => input.addEventListener("change", () => {
    const group = input.dataset.scrubCategory as ScrubSignalGroup;
    if (input.checked) enabledScrubSignals.add(group); else enabledScrubSignals.delete(group);
    localStorage.setItem(scrubSignalsStorageKey, JSON.stringify([...enabledScrubSignals]));
  }));
  document.querySelector("#finish-scrub-setup")?.addEventListener("click", () => {
    saveScrubSetupPlan(onboardingScrubMode);
    void completeOnboarding();
  });
  document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding());
  document.querySelector("#close-decoy")?.addEventListener("click", () => void getCurrentWindow().close().catch(() => undefined));
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
      onboardingRoute = form.dataset.onboardingPasswordNext as OnboardingRoute;
      render();
      if (onboardingRoute === "browser") void refreshBrowserImportReadiness();
      if (onboardingRoute === "mullvad") void refreshMullvadSetup();
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
    const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, showPlaintextPreview: true, windowCaptureEnabled });
    setup = saved.setup;
    windowCaptureEnabled = saved.windowCaptureEnabled;
    onboardingComplete = true;
    clearServiceOnboardingResume();
    resetOnboardingBranch();
    resetOnboardingConnections();
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
    mullvadStatus = await withNativeDeadline(loadMullvadStatus(), "Check Mullvad", nativeCatalogDecisionDeadlineMs);
  } catch {
    showToast("Mullvad status is unavailable");
  } finally {
    mullvadBusy = false;
    render();
  }
}

async function hostMullvadWithDeadline(label: string): Promise<Awaited<ReturnType<typeof hostMullvadWindow>>> {
  const hostAttempt = hostMullvadWindow();
  try {
    return await withNativeDeadline(hostAttempt, label, 30_000);
  } catch (failure) {
    // The native operation is not cancellable. If it succeeds after the UI
    // deadline, immediately restore the borrowed window unless a newer
    // attempt has already become the active host.
    void hostAttempt.then((late) => {
      if (late.status === "hosted" && !mullvadWindowHosted) return restoreMullvadWindow().then(() => undefined);
      return undefined;
    }).catch(() => undefined);
    throw failure;
  }
}

async function hostMullvadUntilReady(label: string, waitMs = 60_000): Promise<Awaited<ReturnType<typeof hostMullvadWindow>>> {
  const deadline = Date.now() + waitMs;
  let result = await hostMullvadWithDeadline(label);
  while (result.status !== "hosted"
    && ["appNotInstalled", "existingSessionUnavailable", "windowOperationRejected"].includes(result.reason)
    && Date.now() < deadline) {
    await new Promise((resolve) => window.setTimeout(resolve, 1_000));
    result = await hostMullvadWithDeadline(label);
  }
  return result;
}

async function runMullvadSetupAction(action: "install" | "open"): Promise<void> {
  if (mullvadBusy) return;
  mullvadBusy = true;
  mullvadSetupNotice = action === "install" ? "Installing Mullvad…" : "Opening Mullvad…";
  render();
  try {
    if (action === "install") {
      await withNativeDeadline(installMullvad(), "Start Mullvad install");
      const installDeadline = Date.now() + 180_000;
      do {
        await new Promise((resolve) => window.setTimeout(resolve, 1_000));
        mullvadStatus = await loadMullvadStatus().catch(() => mullvadStatus);
        if (mullvadStatus.availability === "installed") break;
      } while (Date.now() < installDeadline);
      if (mullvadStatus.availability !== "installed") {
        throw new Error("Mullvad installation did not finish within three minutes");
      }
    }
    const hosted = await hostMullvadUntilReady("Open Mullvad inside OSL");
    if (hosted.status !== "hosted") {
      throw new Error(`Mullvad could not be hosted (${hosted.reason})`);
    }
    mullvadSetupNotice = "";
    mullvadWindowHosted = true;
    mullvadReturnRoute = "onboarding";
    route = "mullvad";
  } catch (failure) {
    mullvadSetupNotice = localActionError(failure, `Mullvad could not ${action === "install" ? "install" : "open"}`);
    showToast(mullvadSetupNotice);
  } finally {
    mullvadBusy = false;
    if (!mullvadWindowHosted) mullvadStatus = await loadMullvadStatus().catch(() => mullvadStatus);
    render();
  }
}

function bindPasswordForm(): void {
  const form = document.querySelector<HTMLFormElement>("#identity-password-form");
  const password = document.querySelector<HTMLInputElement>("#identity-password");
  const confirm = document.querySelector<HTMLInputElement>("#identity-password-confirm");
  const submit = document.querySelector<HTMLButtonElement>("#identity-password-submit");
  const error = document.querySelector<HTMLElement>("#password-error");
  const createStatus = document.querySelector<HTMLElement>("#account-create-status");
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
    const setupMode = form.dataset.passwordMode === "setup";
    const idleLabel = submit.textContent ?? (setupMode ? "Create account" : "Unlock");
    let secret = password.value;
    form.setAttribute("aria-busy", "true");
    password.disabled = true;
    if (confirm) confirm.disabled = true;
    submit.disabled = true;
    submit.textContent = setupMode ? "Creating account…" : "Unlocking…";
    if (!setupMode) password.value = "";
    try {
      if (setupMode) {
        if (createStatus) createStatus.textContent = "Creating encryption keys…";
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const identity = core.readiness.identityLoaded ? null : await createHubOslIdentity();
        if (createStatus) createStatus.textContent = "Securing this device…";
        const passwordResult = await setupHubMainPassword(secret);
        if (createStatus) createStatus.textContent = "Loading your account…";
        const [loadedCore, linkedServices, roleStatus] = await Promise.all([
          loadCoreIntegration(),
          loadLinkedServices().catch(() => services),
          loadHubPasswordRoleStatus().catch(() => null),
        ]);
        core = loadedCore;
        loadActivePrivacyFeatureChoices();
        // The locked bootstrap intentionally cannot read the encrypted
        // service registry. Refresh it immediately after the first password
        // installs the storage key, before the setup app chooser is shown.
        services = linkedServices;
        passwordRoleStatus = roleStatus;
        recoveryBundle = {
          userId: identity?.userId ?? core.readiness.activeOslUserId ?? "Local OSL identity",
          identityPhrase: identity?.identityRecoveryPhrase ?? null,
          passwordPhrase: passwordResult.passwordRecoveryPhrase,
        };
        recoverySavedAcknowledged = false;
        onboardingRoute = "recovery";
      } else {
        const gate = await unlockHubPasswordGate(secret);
        secret = "";
        if (gate.outcome === "wrong") {
          error.textContent = gate.lockoutSecondsRemaining > 0
            ? `Try again in ${gate.lockoutSecondsRemaining} seconds.`
            : "Password not recognized.";
          form.removeAttribute("aria-busy");
          password.disabled = false;
          submit.disabled = false;
          submit.textContent = idleLabel;
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
        loadActivePrivacyFeatureChoices();
        services = await loadLinkedServices().catch(() => services);
        passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
        if (onboardingComplete) {
          route = "home";
          void openMullvadOnStartup();
          void refreshUpdateStatus();
          void refreshIdentitySlots();
          void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") render(); });
          void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") render(); });
        }
        else onboardingRoute = pendingOnboardingRoute() ?? "pro";
      }
      secret = "";
      password.value = "";
      if (confirm) confirm.value = "";
      form.removeAttribute("aria-busy");
      render();
      if (discordQaShell && core.readiness.unlocked) void startDiscordQaShell();
      if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
      if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    } catch (failure) {
      secret = "";
      form.removeAttribute("aria-busy");
      if (createStatus) createStatus.textContent = "";
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check OSL account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        secret = "";
        error.textContent = "OSL could not verify the account state. Try again.";
        form.removeAttribute("aria-busy");
        password.disabled = false;
        if (confirm) confirm.disabled = false;
        submit.disabled = false;
        submit.textContent = idleLabel;
        password.focus();
        return;
      }
      core = refreshedCore;
      const readiness = core.readiness;
      if (readiness.bootstrapStatus === "ready" && readiness.unlocked) {
        services = await loadLinkedServices().catch(() => services);
        passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
        secret = "";
        if (setupMode || !onboardingComplete) {
          onboardingRoute = setupMode ? "pro" : pendingOnboardingRoute() ?? "pro";
          route = "onboarding";
          showToast("Password is configured. Continue setup.");
        } else {
          route = "home";
        }
        render();
        if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
        if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
        return;
      }
      if (setupMode && readiness.bootstrapStatus === "passwordRequired") {
        const gate = await unlockHubPasswordGate(secret).catch(() => null);
        secret = "";
        if (gate?.readiness?.unlocked) {
          core = await loadCoreIntegration();
          services = await loadLinkedServices().catch(() => services);
          passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
          onboardingRoute = "pro";
          showToast("Account created. Continue setup.");
          render();
          return;
        }
        onboardingRoute = "unlock";
        showToast("Password is configured. Unlock to continue.");
        render();
        return;
      }
      secret = "";
      if (setupMode && readiness.bootstrapStatus === "setupRequired" && readiness.identityLoaded) {
        error.textContent = "Account created. Create its password to continue.";
      } else {
        error.textContent = localActionError(failure, "The OSL account action failed. Try again.");
      }
      form.removeAttribute("aria-busy");
      password.disabled = false;
      if (confirm) confirm.disabled = false;
      submit.disabled = false;
      submit.textContent = idleLabel;
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
      loadActivePrivacyFeatureChoices();
      services = await loadLinkedServices().catch(() => services);
      recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase };
      recoverySavedAcknowledged = false;
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
        onboardingRoute = "pro";
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
  lastOnboardingMarkup = null;
  const protectedSheet = activeEmbeddedHost
    ? protectedSheetMode === "local"
      ? localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)
      : peerProtectedSheetMarkup(peerProtectedSheet, hubPeople)
    : "";
  const markup = `<div class="hub-layout"><section class="hub-workspace">${trustedHeader()}${workspaceContent()}</section></div>${protectedSheet}${nativeDiscordProtectPickerMarkup()}${peopleDialogMarkup()}${friendsDialogMarkup()}${scrubReviewDialogMarkup()}${burnDialogMarkup()}${ownedConfirmationMarkup()}${updateDialogMarkup()}`;
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

function nativeDiscordHeaderControls(): string {
  if (route !== "service" || activeNativeHostId !== "discord") return "";
  const inactive = nativeDiscordProtectionActive ? "" : "disabled";
  return `<div class="native-discord-header-controls" aria-label="Discord privacy controls"><button class="header-protection-control burn" data-open-burn="chat" type="button" ${inactive} title="Burn this local OSL chat">Burn</button><button class="header-protection-control ${nativeDiscordCovertextEnabled ? "active" : ""}" id="native-discord-covertext" type="button" aria-pressed="${nativeDiscordCovertextEnabled}" title="${nativeDiscordCovertextEnabled ? "Covertext is on" : "Covertext is off"}">Covertext</button><button class="header-protection-control" id="native-discord-ai-covertext" type="button" disabled title="Requires a verified local model pack; no cloud AI is used">AI Covertext <small>Model pack needed</small></button></div>`;
}

function trustedHeader(): string {
  // Service controls stay compact; deeper setup remains progressively disclosed.
  if (route === "home" || route === "osl-chat" || route === "notes") return homeHeader();
  if (route === "mullvad") {
    return `<div class="trusted-stack"><header class="workspace-header mullvad-host-header"><button class="button compact" id="mullvad-return" type="button">${mullvadReturnRoute === "onboarding" ? "Back to setup" : "Back to Home"}</button><div class="service-context"><span><strong>Mullvad</strong><small>Existing session · capture resistance does not cover Mullvad</small></span></div></header></div>`;
  }
  if (route === "service" && activeService && serviceGuideStep !== null) {
    return `<div class="trusted-stack home-trusted-stack"><header class="home-header guide-header"><button class="home-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="guide-header-service">${serviceLogo(activeService.id)}<span><strong>${escapeHtml(activeService.displayName)}</strong><small>${isCoreProtectionReady(core.readiness) ? "Ready" : "Needs attention"}</small></span></div>${settingsButtonMarkup()}</header></div>`;
  }
  const localProtection = route === "service" && (activeEmbeddedHost || activeNativeHostId === "discord")
    ? `<button class="local-protected-toggle" id="local-protected-toggle" type="button" aria-expanded="${localProtectedSheet.open || peerProtectedSheet.open || nativeDiscordProtectionActive}">Protect</button>`
    : "";
  const serviceControls = route === "service" && activeService ? `<div class="service-context"><span class="service-context-logo">${serviceLogo(activeService.id)}</span><span><strong>${escapeHtml(activeHomeAppName())}</strong><small>${activeEmbeddedHost ? "Isolated OSL profile" : activeDefaultBrowserCompanion ? "Default-browser companion · unprotected" : activeNativeHostMode === "existingSession" ? "Native companion" : activeNativeHostId ? "OSL app window" : "Needs setup"}</small></span>${localProtection}</div>` : "";
  const onboardingContinue = route === "service" && onboardingServiceSetup && (activeEmbeddedHost || activeNativeHostId || activeDefaultBrowserCompanion)
    ? `<button class="button compact primary" id="onboarding-service-continue">Continue setup</button>`
    : "";
  return `<div class="trusted-stack"><header class="workspace-header"><div class="hub-command"><button class="command-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span><strong>OSL Privacy</strong></span></button>${appLauncherStrip()}${simpleDeviceStatusMarkup()}</div>${nativeDiscordHeaderControls()}${serviceControls ? `<div class="context-command">${serviceControls}</div>` : ""}${onboardingContinue}${settingsButtonMarkup("workspace-settings")}</header>${updateBannerMarkup()}</div>`;
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
  if (route === "mullvad") return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="Your existing Mullvad window is open inside OSL"><span class="sr-only">Mullvad remains a separate foreign application. OSL does not read its account or VPN state.</span></main>`;
  if (route === "osl-chat") return oslChatContent();
  if (route === "osl-servers") return oslServersContent();
  if (route === "notes") return notesWorkspaceMarkup(oslNotes, activeOslNoteId, oslNotesLoading, oslNotesError);
  if (route === "settings") return settingsContent();
  if (route === "service" && activeService) return serviceContent();
  const launchableHomeApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const rememberedHomeApps = new Set<HomeAppId>(hasExplicitOnboardingAppSelection
    ? selectedOnboardingApps
    : [
        ...selectedOnboardingApps,
        ...launchableHomeApps.filter((app) => app.linked || savedNativeApps.has(app.id as NativeAppId)).map((app) => app.id),
      ]);
  const homeApps = hasExplicitOnboardingAppSelection || rememberedHomeApps.size
    ? launchableHomeApps.filter((app) => rememberedHomeApps.has(app.id))
    : launchableHomeApps;
  const modules = [
    { id: "osl-chats", name: "OSL Chat", available: true },
    { id: "osl-notes", name: "OSL Notes", available: true },
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
    return `<article class="app-tile ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${homeEditMode}" data-service-kind="${app.serviceId ?? "none"}"><button id="home-app-${app.id}" type="button" data-home-app="${app.id}" aria-label="${escapeHtml(`${app.displayName}, ${pending ? "Opening" : state}`)}" ${appLaunchPendingId ? "disabled" : ""}><span class="app-logo-plate">${homeAppLogo(app)}</span><span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : ""}</span></button>${controls}</article>`;
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
  return `<main id="home-navigation" class="content-viewport home-dashboard ${homeEditMode ? "editing" : ""}"><section class="home-primary"><section class="home-apps" aria-labelledby="route-heading"><div class="home-app-groups">${oslSection}${socialTiles ? `<section class="home-app-section"><header><h2>Social</h2>${organizeButton("social apps")}</header><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section>` : ""}${emailTiles ? `<section class="home-app-section"><header><h2>Email</h2>${organizeButton("email apps")}</header><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>` : ""}</div></section></section><button class="home-profile-dock" data-route="settings" data-profile-settings type="button" aria-label="Open your OSL profile" title="OSL Profile"${profileStyle}>${profileVisual}<strong>${escapeHtml(oslProfile?.displayName ?? "OSL Profile")}</strong></button></main>`;
}

function oslChatContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const friends = hubPeople.map((person) => {
    const messages = oslChatMessages.get(person.personId) ?? [];
    const last = messages.at(-1);
    const hasPendingViewOnce = (oslChatPendingViewOnce.get(person.personId)?.size ?? 0) > 0;
    return {
      personId: person.personId,
      nickname: person.alias ?? "Unnamed friend",
      verified: person.safetyNumberVerified && !person.pendingKeyChange,
      ready: person.personId === activeOslChatPersonId && activeOslChatContext?.scopeApproved === true,
      preview: hasPendingViewOnce ? "View-once message" : last?.body ?? null,
      previewVisible: !pro || oslChatPreviewsVisible,
      unreadCount: oslChatUnread.get(person.personId) ?? 0,
      muted: oslChatMutedPeople.has(person.personId),
    };
  });
  const approval = activeOslChatPersonId && activeOslChatContext && !activeOslChatContext.scopeApproved
    ? `<div class="osl-chat-approval"><span><strong>Turn on this encrypted chat</strong><small>Approves only this OSL friend.</small></span><button class="button primary compact" id="osl-chat-approve" type="button" ${oslChatBusy ? "disabled" : ""}>Enable</button></div>`
    : "";
  const settingsPerson = oslChatSettingsPersonId ? hubPeople.find((person) => person.personId === oslChatSettingsPersonId) ?? null : null;
  const settings = settingsPerson ? oslChatFriendSettingsMarkup(settingsPerson, pro) : "";
  const attachments = activeOslChatContext?.scopeApproved && pro
    ? `<section class="osl-chat-attachments" aria-label="Encrypted attachments"><header><strong>Attachments</strong><button class="button compact" id="osl-chat-attach" type="button" ${oslChatBusy ? "disabled" : ""}>Choose file</button></header>${oslChatAttachments.length ? oslChatAttachments.map((item) => `<button class="setting-line" data-osl-chat-attachment="${escapeHtml(item.attachmentId)}" type="button"><span><strong>${escapeHtml(item.originalFilename)}</strong><small>${item.viewOnce ? "View once · " : ""}${item.plaintextSize.toLocaleString("en-US")} bytes</small></span><span class="status-tag">Open</span></button>`).join("") : `<p>No pending attachments.</p>`}<small>Images open in OSL's capture-resistant viewer. Other supported files open temporarily in their Windows viewer, which may allow capture.</small></section>`
    : "";
  return `<main class="content-viewport osl-chat-page"><header class="osl-chat-page-header"><button class="text-button" id="osl-chat-back" type="button" ${oslChatBusy ? "disabled" : ""}>Back</button><h1 id="route-heading" tabindex="-1">OSL Chats</h1><button class="text-button" id="osl-chat-refresh" type="button" ${activeOslChatContext?.scopeApproved && !oslChatBusy ? "" : "disabled"}>Refresh</button></header>${approval}${oslChatsViewMarkup({
    friends,
    activePersonId: activeOslChatPersonId,
    messages: activeOslChatPersonId ? oslChatMessages.get(activeOslChatPersonId) ?? [] : [],
    draft: oslChatDraft,
    busy: oslChatBusy,
    viewOnce: oslChatViewOnce,
    homeLogoUrl: oslVectorLogoUrl,
    disableLinkPreviews: setupPrivacyChoices.has("disable-previews"),
  })}${attachments}${settings}</main>`;
}

function oslChatFriendSettingsMarkup(person: HubPerson, pro: boolean): string {
  const isActive = activeOslChatPersonId === person.personId;
  const approved = isActive && activeOslChatContext?.scopeApproved === true;
  const muted = oslChatMutedPeople.has(person.personId);
  return `<dialog class="friends-dialog osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="friends-dialog-card"><header><div><span>Encrypted chat</span><h2 id="osl-chat-settings-title">${escapeHtml(person.alias ?? "Verified friend")}</h2></div><button class="icon-button" id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header><div class="settings-list"><label class="setting-line interactive"><span><strong>Mute notifications</strong><small>Messages still arrive without creating a local alert.</small></span><input id="osl-chat-mute-toggle" type="checkbox" ${muted ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Message previews</strong><small>${pro ? "Hide previews on this device." : "Preview hiding is available with Pro."}</small></span><input id="osl-chat-preview-toggle" type="checkbox" ${!pro || oslChatPreviewsVisible ? "checked" : ""} ${pro ? "" : "disabled"}/></label><div class="setting-line"><span><strong>Chat permission</strong><small>${approved ? "This friend may exchange encrypted OSL messages with you." : "Open this friend to configure its exact chat permission."}</small></span>${isActive ? `<button class="button compact ${approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${oslChatBusy ? "disabled" : ""}>${approved ? "Revoke" : "Enable"}</button>` : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button">Open chat</button>`}</div></div></div></dialog>`;
}

function oslServersContent(): string {
  const capabilities = [
    ["Discord servers", "Not available yet"],
    ["Telegram groups and channels", "Not available yet"],
    ["Signal groups", "Not available yet"],
    ["Snapchat groups", "Not available yet"],
  ];
  return `<main class="content-viewport osl-servers-page"><header class="osl-chat-page-header"><button class="text-button" data-route="home" type="button">Back</button><h1 id="route-heading" tabindex="-1">Servers</h1></header><p>Shared encrypted spaces will appear here when their sender, membership, delivery, and history security are complete.</p><section class="settings-list" aria-label="Planned server capabilities">${capabilities.map(([name, state]) => `<div class="setting-line"><span><strong>${name}</strong><small>${state}</small></span><span class="status-tag">Coming later</span></div>`).join("")}</section><p class="scope-approval-note">OSL does not claim provider-server access or read provider pages. Direct OSL Chats are available now.</p></main>`;
}

function homeModuleIcon(id: "osl-chats" | "osl-servers" | "scrub" | "activity" | "osl-notes"): string {
  if (id === "osl-chats") return `<svg viewBox="0 0 24 24"><path d="M4 5.5h16v10H9l-5 4v-14Z"/><path d="M8 9h8M8 12h5"/></svg>`;
  if (id === "osl-servers") return `<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="6"/><rect x="4" y="14" width="16" height="6"/><path d="M7 7h.01M7 17h.01M11 7h6M11 17h6"/></svg>`;
  if (id === "scrub") return `<svg viewBox="0 0 24 24"><path d="m5 18 9-9 5 5-6 6H7l-2-2Z"/><path d="m12 11 3-3 5 5-3 3M4 20h16"/></svg>`;
  if (id === "activity") return `<svg viewBox="0 0 24 24"><path d="M4 12h4l2-5 4 10 2-5h4"/></svg>`;
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

function nativeAppLogo(app: NativeApp): string {
  return app.id === "outlook" ? providerLogo("outlook") : serviceLogo(app.id);
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
    if (mode === "home") {
      const lastMessage = oslChatMessages.get(person.personId)?.at(-1);
      const chatState = person.pendingKeyChange ? "Security change needs review" : person.safetyNumberVerified ? (lastMessage?.body ?? "Open encrypted chat") : "Request pending";
      return `<article class="person-row home-friend-row"><button class="home-friend-open" type="button" data-osl-chat-open="${escapeHtml(person.personId)}" ${person.safetyNumberVerified && !person.pendingKeyChange ? "" : "disabled"}><span><strong>${escapeHtml(nickname)}</strong><small>${escapeHtml(chatState)}</small></span></button><button class="home-friend-settings" type="button" data-friend-settings="${escapeHtml(person.personId)}" aria-label="Settings for ${escapeHtml(nickname)}">•••</button></article>`;
    }
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
  return `<dialog class="friends-dialog" id="friends-dialog" aria-labelledby="friends-dialog-title"><div class="friends-dialog-card"><header><h2 id="friends-dialog-title">Friends</h2><button class="icon-button" id="friends-dialog-close" aria-label="Close friends">×</button></header><form id="add-friend-username-form" class="friend-add-form"><label for="friend-username-input"><span>OSL username</span><input id="friend-username-input" minlength="3" maxlength="30" placeholder="username" autocomplete="off" autocapitalize="none" spellcheck="false" required/></label><label for="friend-username-nickname-input"><span>Name them on this device</span><input id="friend-username-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add friend</button></form><p class="form-status" id="friend-form-status" role="status"></p><p class="scope-approval-note">Encrypted chats stay off until you compare the safety number another way and approve each chat separately.</p><details class="settings-disclosure friend-invite-fallback"><summary>Use a long invite instead</summary><form id="add-friend-form" class="friend-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button">Add from invite</button></form></details><div class="people-list home-people-list">${peopleListMarkup("manage", friendsDialogPageSize, pageStart)}</div>${pagination}${inviteCard}</div></dialog>`;
}

function nativeDiscordProtectPickerMarkup(): string {
  if (!nativeProtectPickerOpen || activeNativeHostId !== "discord") return "";
  const friends = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const choices = friends.length
    ? friends.map((person, index) => `<button ${friends.length === 1 && index === 0 ? 'id="native-protect-verified-peer" ' : ""}class="peer-friend-row" type="button" data-native-protect-person="${escapeHtml(person.personId)}" ${nativeProtectBusy ? "disabled" : ""}><span>${escapeHtml(person.alias ?? "Verified friend")}</span><small>Verified</small></button>`).join("")
    : `<p class="peer-empty">Verify a friend first.</p>`;
  return `<dialog class="unlock-dialog" id="native-protect-friend-dialog"><div class="unlock-card"><h2>Protect with</h2><p>OSL will open its own private panel. Discord is not read or controlled.</p><div class="peer-choice-list">${choices}</div><button class="button" id="native-protect-picker-close" type="button">Cancel</button></div></dialog>`;
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
  if (activeNativeHostId && activeNativeHostMode === "existingSession") {
    const protectionFailure = nativeProtectFailureNotice
      ? `<p class="form-status" role="status">${escapeHtml(nativeProtectFailureNotice)}</p>`
      : "";
    return `<main class="content-viewport native-app-page native-companion-page" id="route-heading" tabindex="-1"><section class="native-app-card native-companion-card"><span class="service-icon large">${activeService ? serviceLogo(activeService.id) : ""}</span><h1>${name} is open</h1><p>Signed-in window reused · session not copied</p>${discordQaHostStatusMarkup()}${protectionFailure}<button class="button primary" id="native-companion-focus" type="button">Bring forward or reopen</button><div class="native-app-secondary"><button class="text-back" id="native-app-back">← Apps</button></div></section></main>`;
  }
  if (activeNativeHostId) return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="${name} is open in an OSL-specific native window"><span class="sr-only">${name} native client is open inside OSL.</span></main>`;
  if (activeDefaultBrowserCompanion) return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="${name} is open in your default-browser companion"><span class="sr-only">${name} is open in an app-style normal-profile browser window. It is not capture-protected or shortcut-locked by OSL.</span></main>`;
  if (activeEmbeddedHost) return `<main class="content-viewport host-viewport host-open" id="route-heading" tabindex="-1" aria-label="${name} is open inside OSL"><div class="loading-host" aria-hidden="true"><span class="host-skeleton logo"></span><span class="host-skeleton title"></span></div></main>`;
  if (serviceAccountPickerOpen) return serviceAccountPickerContent();
  if (activeService && activeHomeAppId && ["telegram", "signal", "whatsapp"].includes(activeHomeAppId) && activeNativeApp()?.availability === "installed") {
    return serviceGuideContent(activeService, 0);
  }
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
  const currentSession = app && providerWideInstalledNativeApp(app.id)
    ? `<button class="service-account-choice" data-service-current-session type="button"><span>${serviceLogo(activeService?.id ?? "discord")}</span><strong>Current desktop session</strong><small>Provider-wide · opens whichever account the desktop app currently shows</small></button>`
    : "";
  const choices = accounts.map((account) => `<button class="service-account-choice" data-service-account="${escapeHtml(account.id)}" type="button"><span>${serviceLogo(activeService?.id ?? "discord")}</span><strong>${escapeHtml(account.label)}</strong><small>Isolated OSL profile · exact account</small></button>`).join("");
  return `<main class="content-viewport native-app-page" id="route-heading" tabindex="-1"><section class="native-app-card service-account-picker"><button class="text-back" id="native-app-back">← Apps</button><h1>Choose how to open ${name}</h1><div class="service-account-choices">${currentSession}${choices}</div><button class="button" id="add-service-profile">Add another profile</button></section></main>`;
}

function serviceGuideContent(service: LinkedService, step: ServiceGuideStep): string {
  const name = escapeHtml(activeHomeAppName());
  void step;
  const nativeApp = activeNativeApp();
  const installedAction = nativeApp?.availability === "installable"
      ? `<button class="button" data-background-install="${nativeApp.id}" ${backgroundInstallIds.has(nativeApp.id) ? "disabled" : ""}>${backgroundInstallIds.has(nativeApp.id) ? "Installing…" : "Background install"}</button>`
      : "";
  const directNativeAccountChoice = activeHomeAppId !== null && ["discord", "telegram", "signal", "whatsapp", "outlook"].includes(activeHomeAppId);
  const directBrowserAccountChoice = defaultBrowserCompanionEligible(activeHomeAppId) && selectedBrowserHasImportReceipt();
  const selectedApp = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId);
  const sessionChoices = activeHomeAppId === "discord"
    ? discordSessionModeChoices()
    : activeHomeAppId === "telegram"
      ? telegramSessionModeChoices()
      : activeHomeAppId === "signal"
        ? signalSessionModeChoices()
      : activeHomeAppId === "whatsapp"
        ? whatsappSessionModeChoices()
      : activeHomeAppId === "outlook"
        ? outlookSessionModeChoices()
      : browserSessionModeChoices();
  const openAction = directNativeAccountChoice || directBrowserAccountChoice
    ? ""
    : selectedApp?.launchState === "available"
    ? `<button class="button primary" id="embedded-service-setup" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : "Open"}</button>`
    : `<button class="button" disabled>Coming later</button>`;
  const nativeFailure = nativeHostFailureNotice
    ? `<p class="form-status" role="status">${escapeHtml(nativeHostFailureNotice)}</p>`
    : "";
  return `<main class="content-viewport service-guide" id="route-heading" tabindex="-1"><section class="guide-card guide-card-simple"><header><button class="text-back" id="service-guide-exit">← Apps</button></header><div class="guide-hero"><span class="guide-logo" data-guide-service="${service.id}">${serviceLogo(service.id)}</span><h1>${directNativeAccountChoice || directBrowserAccountChoice ? "Open" : "Connect"} ${name}</h1></div>${discordQaHostStatusMarkup()}${sessionChoices}${openAction || installedAction ? `<footer class="guide-actions">${openAction}${installedAction}</footer>` : ""}${nativeFailure}</section>${onboardingServiceSetup ? '<button class="onboarding-skip-dock" id="service-guide-skip">Skip · manual setup</button>' : ""}</main>`;
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
  const supportedBrowsers = browserImports.filter((browser) => browser.installed && browser.id !== "duckduckgo");
  const defaultSelected = preferredBrowserId === null;
  const browserChoices = `<div class="preferred-browser-choices" role="radiogroup" aria-label="Browser for web apps"><button type="button" role="radio" aria-checked="${defaultSelected}" class="setting-option account-launch-choice ${defaultSelected ? "selected" : ""}" data-preferred-browser=""><strong>Default</strong></button>${supportedBrowsers.map((browser) => `<button type="button" role="radio" aria-checked="${preferredBrowserId === browser.id}" class="setting-option account-launch-choice ${preferredBrowserId === browser.id ? "selected" : ""}" data-preferred-browser="${browser.id}">${browserLogo(browser.id)}<strong>${escapeHtml(browser.displayName)}</strong></button>`).join("")}</div>`;
  const nativeModeRows = nativeApps
    .filter((app) => app.availability === "installed")
    .map((app) => nativeSessionModeSettingChoices(app.id, app.displayName))
    .join("");
  const nativeModeSettings = nativeModeRows
    ? `<details class="saved-account-settings settings-disclosure account-opening-settings" open><summary>Account opening</summary><div class="account-opening-content">${nativeModeRows}</div></details>`
    : "";
  const browserSettings = `<details class="saved-account-settings settings-disclosure account-opening-settings" open><summary>Browser for web apps</summary><div class="account-opening-content">${browserChoices}</div></details>`;
  return `<h2>Apps</h2><div class="account-settings-list">${rows}</div>${nativeModeSettings}${browserSettings}`;
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
    privacyScanStatus = null;
    privacyCoverageReceipt = null;
    privacyScanFileName = null;
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
  return `<details class="settings-disclosure sending-settings"><summary><span><strong>Sending</strong><small>${escapeHtml(formatSendMode(selectedMode))}</small></span></summary><div class="sending-settings-body"><div class="send-mode-list compact">${modes.map(([mode, label, detail]) => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-settings-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${label}</strong></span><small>${detail}</small></button>`).join("")}</div>${needsRiskAcceptance(selectedMode) ? `<div class="warning send-settings-warning"><strong>Experimental</strong><p>OSL must recheck the exact app, account, chat, and composer. If proof is unavailable or changes, it copies instead and sends nothing.</p></div>` : `<p class="send-settings-truth">OSL encrypts and copies. You choose where and when to send. If OSL cannot prove the destination, it copies the encrypted text and sends nothing.</p>`}${consentRows}</div></details>`;
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
    const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, showPlaintextPreview: true, windowCaptureEnabled });
    setup = saved.setup;
    windowCaptureEnabled = saved.windowCaptureEnabled;
    showToast(`${formatSendMode(mode)} selected`);
  } catch {
    setup = previous;
    showToast("Sending preference could not be saved");
  }
  render();
}

function privacySettingsContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const privacyToggle = (id: SetupPrivacyChoiceId, title: string, detail: string) => `<label class="setting-line interactive"><span><strong>${title}</strong><small>${detail}</small></span><input type="checkbox" data-privacy-feature="${id}" ${setupPrivacyChoices.has(id) ? "checked" : ""}/></label>`;
  const privacyControls = `<section class="settings-section privacy-feature-settings"><header><div><h3>Privacy controls</h3><p>Stored for this OSL identity and enforced locally.</p></div></header><div class="settings-list">${privacyToggle("disable-previews", "Disable link previews", "Suppress local inline URL cards in OSL Chats and the composer.")}${privacyToggle("ip-grabber-protection", "IP-grabber protection", "Deny known logger domains locally at the moment a link is opened.")}${privacyToggle("external-default-browser", "Open links in your default browser", "Checked HTTP links leave OSL only through the operating-system browser command.")}${privacyToggle("auto-lock", "Auto-lock on idle", "Lock and clear the live unlock key after 5 minutes without input.")}${privacyToggle("clear-clipboard", "Clear copied messages", "Clear unchanged protected clipboard text after 30 seconds. Windows app only.")}</div></section>`;
  const assistedDeleteWarning = `<details class="safety-disclosure"><summary>Deletion coverage</summary><div><p>Automatic removal is enabled only for a live-confirmed documented adapter with exact item readback. Hosted-session deletion for Gmail web, Discord, and Telegram web is unavailable in this build.</p><p>Exports can be scanned locally, but they cannot prove that a provider copy was removed.</p></div></details>`;
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose file"}</label><input id="privacy-export-input" class="sr-only" type="file" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>${assistedDeleteWarning}`;
  const autoScrubPlan = proActive ? "PRO · TRANSPORT-GATED" : "PRO REQUIRED";
  return `<h2>Scrub</h2>${privacyControls}<p class="scrub-local-promise"><strong>Your messages and attachments never leave this device.</strong> Every scan and review stays local.</p><section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h3>Review a file</h3><p>Choose a message export or attachment of any type. OSL reports exactly what it could and could not inspect.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}<details class="settings-disclosure autoscrub-disclosure"><summary><span><strong>AutoScrub assistant</strong><small>${autoScrubPlan}</small></span></summary>${autoScrubMarkup(proActive)}</details><details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, providers, exports, and backups may retain copies.</p><p>Only a provider readback can verify removal within its stated coverage. Exports, backups, recipients, and other copies may remain.</p></div></details><details class="privacy-technical settings-disclosure"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Automatic deletion</span><strong>Unavailable in this build</strong></div><div class="setting-line"><span>Read-only adapters</span><strong>Local exports and configured IMAP</strong></div><label class="setting-line interactive"><span><strong>Windows capture resistance</strong><small>Asks Windows to exclude OSL from ordinary screen capture. Cameras, malware, and modified recipients can still capture content.</small></span><input id="screenshot-protection" type="checkbox" ${screenshotProtectionEnabled ? "checked" : ""}/></label></details>`;
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
  return `<section class="autoscrub-card" aria-disabled="${!active}"><header><div><span class="privacy-local-mark">DELETE ENGINE · REVIEW REQUIRED</span><h3>One reviewed batch</h3></div><button class="button compact" id="refresh-autoscrub" type="button" ${autoScrubBusy ? "disabled" : ""}>Check live path</button></header><p>Automatic deletion is unavailable in this build. OSL can still create a local reviewed list and use read-only provider inspection where configured. It will not send a delete command without a native one-shot reviewed-consent capability.</p><label class="autoscrub-account"><span>Deletion path</span><select id="autoscrub-path">${pathOptions}</select></label><ul class="autoscrub-providers">${providers}</ul>${accounts.length ? `<label class="autoscrub-account"><span>Account</span><select id="autoscrub-account">${accountOptions}</select></label>` : `<p>No matching account is available.</p>`}${unavailableReason ? `<p class="autoscrub-unavailable"><strong>Unavailable:</strong> ${escapeHtml(unavailableReason)}</p>` : ""}<label class="autoscrub-confirm"><input id="autoscrub-final-confirmation" type="checkbox" disabled/><span><strong>Final confirmation</strong><small>Deletion remains disabled until native reviewed consent is available.</small></span></label><button class="button primary" id="run-autoscrub" type="button" disabled>Deletion unavailable</button>${autoScrubError ? `<p class="autoscrub-error" role="alert">${escapeHtml(autoScrubError)}</p>` : ""}${autoScrubReceiptMarkup(autoScrubDryRunReceipt)}${autoScrubReceiptMarkup(autoScrubExecutionReceipt)}<details class="autoscrub-connect"><summary>Connect IMAP for read-only verification</summary><form id="autoscrub-imap-form"><label>Account<select id="autoscrub-imap-account" required>${accountOptions}</select></label><label>IMAP host<input id="autoscrub-imap-host" autocomplete="off" required/></label><label>Username<input id="autoscrub-imap-username" autocomplete="username" required/></label><label>Credential type<select id="autoscrub-imap-auth-kind"><option value="appPassword">App password</option><option value="oauthBearer">OAuth bearer token</option></select></label><label>Credential<input id="autoscrub-imap-secret" type="password" autocomplete="current-password" required/></label><label>Mailbox<input id="autoscrub-imap-mailbox" value="Sent" required/></label><button class="button" type="submit" ${accounts.length ? "" : "disabled"}>Connect read-only IMAP</button><p>This path uses OS-backed secure storage and is cleared from live memory when its OSL identity locks or changes.</p></form></details><details><summary>Telegram TDLib</summary><p>Unavailable until its client is packaged and live-confirmed.</p></details><p>Account-deletion help links are temporarily unavailable until external links can be routed safely through the operating-system browser.</p></section>`;
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
  privacyScanStatus = null;
  privacyCoverageReceipt = null;
  privacyScanFileName = null;
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
  return `<section class="privacy-results" aria-live="polite"><header><div><strong>${matching.length} ${matching.length === 1 ? "suggestion" : "suggestions"}</strong><small>${privacyScanResult.messagesScanned} messages · ${privacyScanResult.attachmentsScanned} attachments scanned${privacyScanFileName ? ` · ${escapeHtml(privacyScanFileName)}` : ""}</small></div><span class="privacy-local-mark">${privacyScanStatus?.persistedEncrypted ? "ENCRYPTED · THIS DEVICE" : "LOCAL"}</span></header>${scrubCoverageReceiptMarkup()}${selectionControls}${items || `<div class="empty-state"><strong>No suggestions in the categories you chose</strong><p>OSL can miss things. Review important chats yourself too.</p></div>`}${pagination}${items ? `<footer class="scrub-review-footer"><span>${selected} selected</span><button class="button" id="review-scrub-selection" type="button" ${selected ? "" : "disabled"}>Review selected</button></footer>` : ""}</section>`;
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
  const attachmentLocation = finding.attachmentPath ? ` · ${escapeHtml(finding.attachmentPath)}` : "";
  return `<article class="privacy-finding ${selected ? "selected" : ""}"><label class="scrub-finding-select"><input type="checkbox" ${inputAttribute}="${index}" ${selected ? "checked" : ""}/><strong>${escapeHtml(scrubFindingLabel(finding.category))}</strong></label><div class="scrub-finding-field"><span>Why OSL showed this</span><p>${escapeHtml(finding.reason)}</p></div><blockquote>${escapeHtml(finding.localPreview)}</blockquote><div class="scrub-finding-field"><span>Where to find it</span><p>${escapeHtml(finding.serviceId)} · ${escapeHtml(finding.conversationId)} · ${escapeHtml(finding.messageLocator)}${attachmentLocation}</p></div><div class="scrub-finding-field"><span>Check that you sent this</span><p>${sentCopy}</p></div></article>`;
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
  return `<dialog class="scrub-review-dialog" id="scrub-review-dialog" aria-labelledby="scrub-review-heading"><div class="scrub-review-card"><header><div><p class="eyebrow">Manual Scrub</p><h2 id="scrub-review-heading">Confirm your list</h2></div><button class="icon-button" id="close-scrub-review" type="button" aria-label="Close review">×</button></header><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Review every checked item before continuing.</p>${scrubCoverageReceiptMarkup()}<div class="scrub-review-summary"><strong>${selected.length} selected</strong><span>This review step does not delete anything.</span></div><div class="scrub-review-items">${items || `<div class="empty-state"><strong>Nothing selected</strong><p>Close this window and choose the messages you want to review.</p></div>`}</div>${pagination}<footer><p>Confirming keeps the editable list. Any supported AutoScrub batch still requires a dry-run and final confirmation.</p><div><button class="button ghost" id="close-scrub-review-footer" type="button">Back</button><button class="button primary" id="confirm-scrub-list" type="button" ${selected.length ? "" : "disabled"}>Confirm this list</button></div></footer></div></dialog>`;
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
  const visibleNotifications = visibleAppNotifications();
  const activity = notificationsEnabled && visibleNotifications.length
    ? visibleNotifications.map((item) => `<article class="notification-event"><span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")}</small></span><time>${escapeHtml(item.createdAt)}</time></article>`).join("")
    : `<div class="empty-state"><strong>${notificationsEnabled ? "Nothing new" : "Activity is off"}</strong><p>${notificationsEnabled ? "New OSL security and chat events appear here." : "Turn on local activity to see OSL events on this device."}</p></div>`;
  const muted = [...oslChatMutedPeople].flatMap((personId) => {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    return person ? [`<div class="setting-line"><span><strong>${escapeHtml(person.alias ?? "Verified friend")}</strong><small>Messages still arrive without a local alert.</small></span><button class="button compact" data-osl-chat-unmute="${escapeHtml(personId)}" type="button">Unmute</button></div>`] : [];
  }).join("");
  return `<h2>Activity</h2><p>Private events created by OSL on this device.</p><section class="notification-events" aria-label="Recent OSL activity">${activity}</section><div class="settings-list"><label class="setting-line interactive"><span><strong>Local OSL activity</strong><small>Master control for activity on this device.</small></span><input id="notifications-opt-in" type="checkbox" ${notificationsEnabled ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Encrypted chat alerts</strong><small>New-message activity from unmuted OSL friends.</small></span><input id="notification-chat-activity" type="checkbox" ${notificationChatActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Security changes</strong><small>Friend encryption-key changes that need verification.</small></span><input id="notification-security-activity" type="checkbox" ${notificationSecurityActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show details</strong><small>Off by default. When off, Activity hides event content.</small></span><input id="notification-previews" type="checkbox" ${notificationPreviewContent ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Suggest chat approval</strong><small>Suggestions never enable decryption.</small></span><input id="notification-scope-suggestions" type="checkbox" ${notificationScopeSuggestions ? "checked" : ""}/></label></div>${muted ? `<details class="settings-disclosure" open><summary><span><strong>Muted OSL Chats</strong><small>${oslChatMutedPeople.size.toLocaleString("en-US")} muted</small></span></summary><div class="settings-list">${muted}</div></details>` : ""}<details class="settings-disclosure notification-apps"><summary><span><strong>Connected apps</strong><small>Provider unread counts are not read</small></span></summary><div class="notification-app-list">${apps}</div></details>`;
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
  return `<h2>Developer</h2><p>Build OSL from source or install a data-only theme mod.</p><section class="settings-section developer-settings"><header><div><h3>Source</h3><p>Clone the repository, install the UI dependencies, then run the local Vite preview.</p></div><button class="button compact" data-source-repository type="button">GitHub</button></header><pre><code>git clone https://github.com/OSLPrivacy/discord-privacy-client.git
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
  const saved = await saveOslProfile(next);
  profileSaving = false;
  if (!saved) { showToast("OSL profile could not be saved"); render(); return; }
  oslProfile = saved;
  const status = await getOslUsernameStatus(saved.usernameCandidate);
  claimedOslUsername = status?.ownedByActiveIdentity ? status.username : null;
  profileDraftAvatar = undefined;
  showToast(claimedOslUsername ? "OSL profile saved" : "Profile saved · username will verify when OSL is online");
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

function resetLocalProtectedSheet(closeRemote = true): void {
  const nativeContextToken = nativeDiscordProtectionActive ? peerProtectedSheet.context?.contextToken ?? null : null;
  activeContextToken = null;
  activeProtectedContextKind = null;
  localProtectedSheet = blankLocalProtectedModel();
  peerProtectedSheet = blankPeerProtectedModel();
  protectedSheetMode = "peer";
  nativeDiscordProtectionActive = false;
  nativeProtectPickerOpen = false;
  nativeProtectBusy = false;
  nativeProtectFailureNotice = "";
  if (closeRemote) {
    if (nativeContextToken) void setNativeDiscordProtectedOverlayOpen(nativeContextToken, false);
    else void setLocalProtectedSheetOpen(false);
  }
}

async function closeActiveServiceSurface(): Promise<void> {
  if (activeEmbeddedHost) await closeEmbeddedServiceHost().catch(() => undefined);
  if (activeNativeHostId) await detachNativeAppWindow().catch(() => undefined);
  if (activeDefaultBrowserCompanion) await detachDefaultBrowserCompanion().catch(() => undefined);
  activeEmbeddedHost = null;
  activeNativeHostId = null;
  activeNativeHostMode = null;
  activeDefaultBrowserCompanion = false;
  resetLocalProtectedSheet();
}

async function closeMullvadSurface(): Promise<void> {
  if (mullvadWindowHosted) await restoreMullvadWindow().catch(() => undefined);
  mullvadWindowHosted = false;
  route = mullvadReturnRoute;
  if (route === "onboarding") onboardingRoute = "mullvad";
  render();
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  await getCurrentWindow().setFocus().catch(() => undefined);
  if (route === "onboarding") void refreshMullvadSetup();
}

async function toggleLocalProtectedSheet(): Promise<void> {
  if (activeNativeHostId === "discord") {
    if (nativeDiscordProtectionActive) {
      const contextToken = peerProtectedSheet.context?.contextToken;
      if (!contextToken || !(await setNativeDiscordProtectedOverlayOpen(contextToken, false))) {
        showToast("OSL's protected Discord panel could not close safely");
        return;
      }
      resetLocalProtectedSheet(false);
      render();
      return;
    }
    nativeProtectFailureNotice = "";
    nativeProtectPickerOpen = true;
    render();
    return;
  }
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

async function openNativeDiscordProtection(personId: string): Promise<boolean> {
  if (nativeProtectBusy || activeNativeHostId !== "discord") return false;
  const person = hubPeople.find((candidate) => candidate.personId === personId && candidate.safetyNumberVerified && !candidate.pendingKeyChange);
  if (!person) {
    nativeProtectFailureNotice = "Protection stopped: verify this friend again.";
    nativeProtectPickerOpen = false;
    render();
    return false;
  }
  const expectedMode = activeNativeHostMode;
  nativeProtectFailureNotice = "";
  nativeProtectBusy = true;
  render();
  const context = await activateNativeManualPeerContext(person.personId);
  if (!context || activeNativeHostId !== "discord" || activeNativeHostMode !== expectedMode) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = activeNativeHostId !== "discord" || activeNativeHostMode !== expectedMode
      ? "Protection stopped: the Discord window changed."
      : "Protection stopped: the verified friend context is unavailable.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  if (!context.scopeApproved && !(await setActiveHubFriendPermission(context.contextToken, context.personId, true, false))) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = "Protection stopped: friend approval could not be saved.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  const approvedContext = context.scopeApproved ? context : { ...context, scopeApproved: true };
  const security = await loadActiveContextSecurity(approvedContext.contextToken);
  if (!security || !isLocalTtlSeconds(security.ttlSeconds)) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = "Protection stopped: chat security settings are unavailable.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  if (activeNativeHostId !== "discord" || activeNativeHostMode !== expectedMode) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = "Protection stopped: the Discord window changed.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  if (!discordQaShell && !(await focusActiveNativeCompanion())) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = "Protection stopped: Discord could not be brought forward safely.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  const qaOverlayResult = discordQaShell
    ? await setNativeDiscordProtectedOverlayOpenForQa(approvedContext.contextToken)
    : null;
  const overlayOpened = qaOverlayResult?.opened
    ?? await setNativeDiscordProtectedOverlayOpen(approvedContext.contextToken, true);
  if (!overlayOpened) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = qaOverlayResult?.error
      ?? "Protection stopped: bring OSL or Discord forward, clear the Discord composer, then retry.";
    showToast(nativeProtectFailureNotice);
    render();
    return false;
  }
  peerProtectedSheet = {
    ...blankPeerProtectedModel(),
    context: approvedContext,
    personId: person.personId,
    displayName: person.alias ?? "Verified friend",
    ttlSeconds: security.ttlSeconds,
    decryptDisplayEnabled: security.decryptDisplayEnabled,
  };
  activeContextToken = approvedContext.contextToken;
  activeProtectedContextKind = "peer";
  nativeDiscordProtectionActive = true;
  nativeProtectFailureNotice = "";
  nativeProtectBusy = false;
  nativeProtectPickerOpen = false;
  render();
  return true;
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
    && peerProtectedSheet.context?.contextToken === contextToken
    && (activeEmbeddedHost !== null || activeNativeHostId === "discord");
}

async function choosePeerProtectedFriend(personId: string): Promise<void> {
  const embeddedHost = activeEmbeddedHost;
  const nativeDiscordMode = activeNativeHostId === "discord" ? activeNativeHostMode : null;
  if ((!embeddedHost && nativeDiscordMode === null) || peerProtectedSheet.busy) return;
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
  const context = embeddedHost
    ? await activateManualPeerContext(embeddedHost.serviceId, embeddedHost.accountId, person.personId)
    : await activateNativeManualPeerContext(person.personId);
  if (protectedSheetMode !== "peer"
    || !peerProtectedSheet.open
    || (embeddedHost
      ? activeEmbeddedHost?.serviceId !== embeddedHost.serviceId || activeEmbeddedHost.accountId !== embeddedHost.accountId
      : activeNativeHostId !== "discord" || activeNativeHostMode !== nativeDiscordMode)) return;
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
  const viewOnce = document.querySelector<HTMLInputElement>("#peer-protected-view-once");
  const plaintext = boundedPeerProtectedDraft(draft?.value ?? "");
  peerProtectedSheet.draft = plaintext;
  peerProtectedSheet.viewOnce = viewOnce?.checked ?? false;
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
  const prepared = await preparePeerProseText(context.contextToken, plaintext, peerProtectedSheet.viewOnce);
  if (!isCurrentPeerContext(context.contextToken)) return;
  peerProtectedSheet.busy = false;
  if (!prepared || prepared.viewOnce !== peerProtectedSheet.viewOnce) {
    peerProtectedSheet.status = "Encryption failed closed. Nothing was copied.";
    render();
    return;
  }
  peerProtectedSheet.coverText = prepared.coverText;
  peerProtectedSheet.receipt = { direction: "sent", state: "prepared" };
  try {
    await navigator.clipboard.writeText(prepared.coverText);
    scheduleCopiedMessageClear();
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
  peerProtectedSheet.openDraft = input?.value ?? "";
  const coverText = peerProtectedSheet.openDraft.trim();
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
  if (opened.requireCaptureProtection) {
    const applied = await setScreenshotProtection(true).catch(() => false);
    if (!applied) {
      peerProtectedSheet.status = "The sender required capture resistance. Plaintext was withheld because Windows could not enable it.";
      render();
      return;
    }
    screenshotProtectionEnabled = true;
  }
  peerProtectedSheet.openedPlaintext = opened.plaintext;
  peerProtectedSheet.receipt = {
    direction: "received",
    state: opened.viewOnceConsumed ? "opened-once" : "received",
  };
  if (opened.viewOnceConsumed) peerProtectedSheet.openDraft = "";
  peerProtectedSheet.status = opened.viewOnceConsumed ? "Opened once. It cannot be opened again." : "Opened here.";
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
    scheduleCopiedMessageClear();
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
    scheduleCopiedMessageClear();
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
    scheduleCopiedMessageClear();
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
  const peerDraft = document.querySelector<HTMLTextAreaElement>("#peer-protected-draft");
  const reconcilePeerDraft = (): void => {
    if (!peerDraft) return;
    const bounded = boundedPeerProtectedDraft(peerDraft.value);
    if (bounded !== peerDraft.value) peerDraft.value = bounded;
    peerProtectedSheet.draft = bounded;
    const feedback = document.querySelector<HTMLElement>("#peer-protected-draft-bytes");
    if (feedback) feedback.textContent = peerProtectedDraftByteFeedback(bounded);
  };
  let peerDraftComposing = false;
  peerDraft?.addEventListener("compositionstart", () => { peerDraftComposing = true; });
  peerDraft?.addEventListener("compositionend", () => { peerDraftComposing = false; reconcilePeerDraft(); });
  peerDraft?.addEventListener("input", () => { if (!peerDraftComposing) reconcilePeerDraft(); });
  const peerOpenDraft = document.querySelector<HTMLTextAreaElement>("#peer-cover-input");
  peerOpenDraft?.addEventListener("input", () => { peerProtectedSheet.openDraft = peerOpenDraft.value; });
  document.querySelector<HTMLButtonElement>("#peer-cover-copy")?.addEventListener("click", () => void copyPeerProtectedText());
  document.querySelector<HTMLInputElement>("#peer-decrypt-display")?.addEventListener("change", (event) => void changePeerDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelectorAll<HTMLButtonElement>("[data-peer-pane]").forEach((button) => button.addEventListener("click", () => {
    reconcilePeerDraft();
    if (peerOpenDraft) peerProtectedSheet.openDraft = peerOpenDraft.value;
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

interface WorkspaceNavigationOptions {
  profileSettings?: boolean;
  settingsTarget?: SettingsSection;
  openFriends?: boolean;
}

async function navigateWorkspace(requestedRoute: Route, options: WorkspaceNavigationOptions = {}): Promise<void> {
  const intent = ++navigationIntentEpoch;
  if (options.openFriends) friendsDialogOpen = false;
  if (route === "notes" && requestedRoute !== "notes") {
    window.clearTimeout(oslNotesSaveTimer);
    await persistActiveOslNote();
  }
  await Promise.resolve();
  if (intent !== navigationIntentEpoch) return;
  if (route === "osl-chat") {
    if (oslChatBusy || !(await closeOslChatContext())) { showToast("OSL Chat could not close safely"); return; }
    discardOpenedOslChatMessages();
    resetOslChatUiState(false);
  }
  if (activeEmbeddedHost || activeNativeHostId || activeDefaultBrowserCompanion) await closeActiveServiceSurface();
  if (route === "settings" && settingsSection === "scrub") clearPrivacyScanState();
  if (route === "settings" && settingsSection === "account") newIdentityRecoveryPhrase = null;
  if (onboardingServiceSetup && requestedRoute === "home") {
    clearServiceGuide();
    advanceOnboardingConnection(activeHomeAppId);
    return;
  } else {
    route = requestedRoute;
    if (options.profileSettings) settingsSection = "account";
    if (options.settingsTarget) settingsSection = options.settingsTarget;
    if (settingsSection === "scrub") applySavedScrubSetupPlan();
  }
  activeService = null;
  activeHomeAppId = null;
  appLaunchPendingId = null;
  serviceAccountPickerOpen = false;
  friendsDialogOpen = options.openFriends === true && route === "home";
  if (friendsDialogOpen) friendsDialogPage = 0;
  render();
}

function bindWorkspace(): void {
  bindPasswordVisibility();
  bindLocalProtectedSheet();
  bindSavedAccountControls();
  document.querySelectorAll<HTMLButtonElement>("[data-notes-new]").forEach((button) => button.addEventListener("click", () => void createOslNote()));
  document.querySelectorAll<HTMLButtonElement>("[data-note-id]").forEach((button) => button.addEventListener("click", async () => {
    window.clearTimeout(oslNotesSaveTimer);
    await persistActiveOslNote();
    activeOslNoteId = button.dataset.noteId ?? null;
    render();
  }));
  document.querySelector<HTMLInputElement>("#note-title")?.addEventListener("input", scheduleOslNoteSave);
  document.querySelector<HTMLTextAreaElement>("#note-body")?.addEventListener("input", scheduleOslNoteSave);
  document.querySelectorAll<HTMLButtonElement>("[data-note-delete]").forEach((button) => button.addEventListener("click", () => void removeOslNote(button.dataset.noteDelete ?? "")));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-open]").forEach((button) => button.addEventListener("click", () => {
    void openOslChat(button.dataset.oslChatOpen ?? "");
  }));
  document.querySelectorAll<HTMLElement>("[data-osl-chat-context]").forEach((row) => row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    oslChatSettingsPersonId = row.dataset.oslChatContext ?? null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-settings]").forEach((button) => button.addEventListener("click", () => {
    oslChatSettingsPersonId = button.dataset.oslChatSettings ?? null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-friend-settings]").forEach((button) => button.addEventListener("click", () => {
    route = "home";
    friendsDialogOpen = true;
    friendsDialogPage = Math.max(0, Math.floor(Math.max(0, hubPeople.findIndex((person) => person.personId === (button.dataset.friendSettings ?? ""))) / friendsDialogPageSize));
    render();
  }));
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
  document.querySelector<HTMLInputElement>("#osl-chat-preview-toggle")?.addEventListener("change", (event) => {
    const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
    if (!pro) return;
    oslChatPreviewsVisible = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(oslChatPreviewStorageKey, String(oslChatPreviewsVisible));
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
  document.querySelector<HTMLButtonElement>("#osl-chat-attach")?.addEventListener("click", () => void sendOslChatAttachment());
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-attachment]").forEach((button) => button.addEventListener("click", () => void openPendingOslChatAttachment(button.dataset.oslChatAttachment ?? "")));
  const nativeProtectDialog = document.querySelector<HTMLDialogElement>("#native-protect-friend-dialog");
  if (nativeProtectDialog && !nativeProtectDialog.open) nativeProtectDialog.showModal();
  nativeProtectDialog?.addEventListener("cancel", (event) => {
    event.preventDefault();
    nativeProtectPickerOpen = false;
    render();
  });
  document.querySelector<HTMLButtonElement>("#native-protect-picker-close")?.addEventListener("click", () => {
    nativeProtectPickerOpen = false;
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-native-protect-person]").forEach((button) => button.addEventListener("click", () => {
    void openNativeDiscordProtection(button.dataset.nativeProtectPerson ?? "");
  }));
  document.querySelector<HTMLButtonElement>("#native-companion-focus")?.addEventListener("click", () => void reopenActiveNativeCompanion());
  document.querySelector<HTMLButtonElement>("#mullvad-return")?.addEventListener("click", () => void closeMullvadSurface());
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
  document.querySelectorAll<HTMLButtonElement>("[data-route]").forEach((button) => button.addEventListener("click", () => {
    void navigateWorkspace(button.dataset.route as Route, { profileSettings: button.hasAttribute("data-profile-settings") });
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
    if (next === "scrub") applySavedScrubSetupPlan();
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
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => {
    void navigateWorkspace("settings", { settingsTarget: "notifications" });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#screenshot-protection")?.addEventListener("change", (event) => void changeScreenshotProtection(event.currentTarget as HTMLInputElement));
  document.querySelectorAll<HTMLInputElement>("[data-privacy-feature]").forEach((input) => input.addEventListener("change", () => changePrivacyFeature(input)));
  document.querySelectorAll<HTMLButtonElement>("[data-external-url]").forEach((button) => button.addEventListener("click", () => void openCheckedExternalLink(button.dataset.externalUrl ?? "")));
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
  document.querySelector<HTMLButtonElement>("#native-discord-covertext")?.addEventListener("click", () => {
    const requested = !nativeDiscordCovertextEnabled;
    void invoke<boolean>("set_native_discord_covertext_enabled", { enabled: requested }).then((confirmed) => {
      nativeDiscordCovertextEnabled = confirmed === requested ? confirmed : nativeDiscordCovertextEnabled;
      render();
      showToast(nativeDiscordCovertextEnabled ? "Covertext is on" : "Covertext is off; private messages stay inside OSL");
    }).catch(() => showToast("Covertext did not change"));
  });
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
  document.querySelector<HTMLButtonElement>("[data-service-current-session]")?.addEventListener("click", () => {
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    const native = app ? providerWideInstalledNativeApp(app.id) : undefined;
    if (app && service && native) void openNativeHostedApp(app, service, native.id);
  });
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
      advanceOnboardingConnection(activeHomeAppId);
      return;
    }
    clearServiceGuide();
    render();
  });
  document.querySelector("#service-guide-finish")?.addEventListener("click", () => {
    if (onboardingServiceSetup) {
      clearServiceGuide();
      advanceOnboardingConnection(activeHomeAppId);
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
  document.querySelector("#home-add-apps")?.addEventListener("click", () => {
    route = "settings";
    settingsSection = "apps";
    render();
  });
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
    void navigateWorkspace("home", { openFriends: true });
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
  document.querySelector<HTMLInputElement>("#notification-chat-activity")?.addEventListener("change", (event) => {
    notificationChatActivity = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(notificationChatStorageKey, String(notificationChatActivity));
    render();
  });
  document.querySelector<HTMLInputElement>("#notification-security-activity")?.addEventListener("change", (event) => {
    notificationSecurityActivity = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(notificationSecurityStorageKey, String(notificationSecurityActivity));
    render();
  });
  document.querySelector<HTMLInputElement>("#notification-previews")?.addEventListener("change", (event) => { notificationPreviewContent = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationPreviewStorageKey, String(notificationPreviewContent)); });
  document.querySelector<HTMLInputElement>("#notification-scope-suggestions")?.addEventListener("change", (event) => { notificationScopeSuggestions = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationScopeStorageKey, String(notificationScopeSuggestions)); });
  document.querySelectorAll<HTMLInputElement>("[data-notification-app]").forEach((input) => input.addEventListener("change", () => { const id = input.dataset.notificationApp as ServiceId; notificationAppPreferences[id] = input.checked; localStorage.setItem(notificationAppsStorageKey, JSON.stringify(notificationAppPreferences)); }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-unmute]").forEach((button) => button.addEventListener("click", () => {
    oslChatMutedPeople.delete(button.dataset.oslChatUnmute ?? "");
    localStorage.setItem(oslChatMutedStorageKey, JSON.stringify([...oslChatMutedPeople].slice(0, 512)));
    render();
  }));
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
    const nativeIntent = selectedNativeAppIntent(app.id);
    if (nativeIntent) {
      void openNativeHostedApp(app, service, nativeIntent);
    } else if (savedAccountsReady && firefoxStatus.availability === "installed" && importedFirefoxHomeAppIds.has(app.id)) {
      try {
        const isolatedBrowserProfile = detectedAccountChoices.get(detectedAccountChoiceKey("browser", app.id)) === "osl";
        if (savedAccountMode === "use" && !isolatedBrowserProfile) {
          await launchSystemBrowserService(app.id);
          showToast(`${app.displayName} opened in your current browser session`);
        } else {
          await launchFirefoxService(app.id);
          showToast(`${app.displayName} opened in your isolated OSL Firefox profile`);
        }
      } catch {
        if (app.linked) void openEmbeddedApp(app, service);
        else openServiceRoute(service, app.provider, app.id, true);
      }
    } else if (defaultBrowserCompanionEligible(app.id)) {
      if (selectedBrowserHasImportReceipt() && !browserSessionModeConfirmed) openServiceRoute(service, app.provider, app.id, true);
      else void openBrowserCompanionApp(app, service);
    } else if (supportedNativeAppIds.has(app.id as NativeAppId) && !nativeSessionModeConfirmed(app.id as NativeAppId)) {
      openServiceRoute(service, app.provider, app.id, true);
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
        showToast(installed.isolatedProfileAvailable
          ? `${app.displayName} is ready`
          : appId === "discord"
            ? "Discord installed; connect a dedicated native profile"
            : `${app.displayName} installed; OSL will use an isolated web profile`);
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
  if (reason === "existingSessionUnavailable") return `OSL could not reopen ${name} automatically. Try again`;
  if (reason === "existingSessionAmbiguous") return `OSL could not safely select the main ${name} window`;
  if (reason === "secondaryInstanceUnverified") return `${name} cannot safely open a separate OSL window yet`;
  if (reason === "channelNotOwned") return `${name} is already used outside this OSL identity`;
  if (reason === "noChannelAvailable") return `Install a dedicated ${name} channel first`;
  if (reason === "appNotInstalled") return `Install ${name} first`;
  if (reason === "windowNotFound") return `${name} opened, but its OSL window was not found`;
  if (reason === "profileInitializationFailed") return `${name}'s separate OSL profile could not finish starting. Try again; your normal ${name} is untouched`;
  if (reason === "profileUnavailable") return `${name}'s separate OSL profile is unavailable`;
  return `${name} could not open as a native OSL window`;
}

async function openNativeHostedApp(app: HomeAppCatalogEntry, service: LinkedService, appId: NativeAppId): Promise<void> {
  if (nativeActionBusy) return;
  nativeHostFailureNotice = "";
  const requestedMode = nativeSessionModeForApp(appId);
  const discordQaExistingSession = discordQaShell && appId === "discord" && requestedMode === "existingSession";
  if (discordQaExistingSession) discordQaHostState = "starting";
  if (activeNativeHostId === appId && activeNativeHostMode === requestedMode) {
    if (await focusActiveNativeCompanion()) {
      if (discordQaExistingSession) discordQaHostState = "hosted";
      render();
      return;
    }
    activeNativeHostId = null;
    activeNativeHostMode = null;
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
    if (activeDefaultBrowserCompanion) {
      await detachDefaultBrowserCompanion().catch(() => undefined);
      activeDefaultBrowserCompanion = false;
    }
    if (activeEmbeddedHost) {
      await closeEmbeddedServiceHost().catch(() => undefined);
      activeEmbeddedHost = null;
    }
    if (activeNativeHostId && (activeNativeHostId !== appId || activeNativeHostMode !== requestedMode)) {
      await detachNativeAppWindow().catch(() => undefined);
      activeNativeHostId = null;
      activeNativeHostMode = null;
    }
    const hostDeadlineMs = appId === "discord" && requestedMode === "dedicated" ? 190_000 : 30_000;
    const result = await withNativeDeadline(
      hostNativeAppWindow(appId, requestedMode),
      `Open ${app.displayName} inside OSL`,
      hostDeadlineMs,
    );
    if (result.status !== "hosted") {
      activeNativeHostId = null;
      activeNativeHostMode = null;
      serviceGuideStep = 0;
      nativeHostFailureNotice = nativeHostFailureMessage(result.reason, app.displayName);
      if (discordQaExistingSession) discordQaHostState = "failed";
      showToast(nativeHostFailureNotice);
      return;
    }
    activeNativeHostId = appId;
    activeNativeHostMode = requestedMode;
    savedAccountMode = "use";
    savedNativeApps.add(appId);
    persistSavedAccountPreferences();
    markServiceOnboardingOpened();
    if (!discordQaExistingSession && !await focusActiveNativeCompanion()) {
      await detachNativeAppWindow().catch(() => undefined);
      activeNativeHostId = null;
      activeNativeHostMode = null;
      serviceGuideStep = 0;
      if (discordQaExistingSession) discordQaHostState = "failed";
      showToast(`${app.displayName} opened but could not be shown safely`);
      return;
    }
    if (discordQaExistingSession) discordQaHostState = "hosted";
    showToast(requestedMode === "existingSession"
      ? `Using the existing ${app.displayName} session. Its window is not capture-protected by OSL.`
      : appId === "discord"
        ? "Discord PTB opened in its separate OSL profile. Discord itself is not capture-resistant; use Protect for OSL's private layer."
        : `${app.displayName} opened in a separate OSL profile`);
  } catch (failure) {
    activeNativeHostId = null;
    activeNativeHostMode = null;
    serviceGuideStep = 0;
    nativeHostFailureNotice = localActionError(failure, `${app.displayName} could not open inside OSL`);
    if (discordQaExistingSession) discordQaHostState = "failed";
    showToast(nativeHostFailureNotice);
  } finally {
    nativeActionBusy = false;
    render();
  }
}

function browserAccountModeForLaunch(): "existingBrowser" | "isolatedOsl" {
  if (!selectedBrowserHasImportReceipt()) return "existingBrowser";
  if (!useDefaultBrowserCompanion && selectedBrowserForLaunch() !== "duckduckgo") return "isolatedOsl";
  return "existingBrowser";
}

async function openBrowserCompanionApp(app: HomeAppCatalogEntry, service: LinkedService): Promise<void> {
  if (nativeActionBusy || !defaultBrowserCompanionEligible(app.id)) return;
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
    if (activeEmbeddedHost) await closeEmbeddedServiceHost().catch(() => undefined);
    if (activeNativeHostId) await detachNativeAppWindow().catch(() => undefined);
    activeEmbeddedHost = null;
    activeNativeHostId = null;
    activeNativeHostMode = null;
    const accountMode = browserAccountModeForLaunch();
    const result = await withNativeDeadline(hostBrowserCompanion(app.id, preferredBrowserId, accountMode), `Open ${app.displayName}`, 20_000);
    if (result.status !== "hosted") throw new Error("The browser could not open safely");
    activeDefaultBrowserCompanion = true;
    markServiceOnboardingOpened();
    showToast(`${app.displayName} opened`);
  } catch (failure) {
    activeDefaultBrowserCompanion = false;
    serviceGuideStep = 0;
    showToast(localActionError(failure, `${app.displayName} could not open`));
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
    const service = services.find((candidate) => candidate.id === app.serviceId);
    if (!service) throw new Error("This app is unavailable right now");
    const nativeIntent = forceNewProfile ? undefined : selectedNativeAppIntent(app.id);
    if (nativeIntent) {
      nativeActionBusy = false;
      await openNativeHostedApp(app, service, nativeIntent);
      return;
    }
    if (defaultBrowserCompanionEligible(app.id)) {
      nativeActionBusy = false;
      if (forceNewProfile) useDefaultBrowserCompanion = false;
      await openBrowserCompanionApp(app, service);
      return;
    }
    if (supportedNativeAppIds.has(app.id as NativeAppId)) {
      throw new Error(`A separate ${app.displayName} app account is unavailable`);
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
    const nativeIntent = accountId ? undefined : selectedNativeAppIntent(app.id);
    if (nativeIntent) {
      nativeActionBusy = false;
      await openNativeHostedApp(app, service, nativeIntent);
      return;
    }
    if (defaultBrowserCompanionEligible(app.id)) {
      nativeActionBusy = false;
      await openBrowserCompanionApp(app, service);
      return;
    }
    if (supportedNativeAppIds.has(app.id as NativeAppId)) {
      throw new Error(`A separate ${app.displayName} app account is unavailable`);
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
  const completedAppId = activeHomeAppId;
  await closeActiveServiceSurface();
  advanceOnboardingConnection(completedAppId);
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
    else {
      friendsDialogOpen = true;
      friendsDialogPage = 0;
      render();
    }
  } else if (id === "osl-servers") {
    route = "osl-servers";
    render();
  } else if (id === "scrub") {
    applySavedScrubSetupPlan();
    route = "settings";
    settingsSection = "scrub";
    render();
  } else if (id === "activity") {
    route = "settings";
    settingsSection = "notifications";
    render();
  } else if (id === "osl-notes") {
    route = "notes";
    render();
    void loadOslNotesWorkspace();
  } else {
    showToast("That OSL module is unavailable");
  }
}

async function loadOslNotesWorkspace(): Promise<void> {
  if (oslNotesLoading) return;
  oslNotesLoading = true;
  oslNotesError = "";
  render();
  try {
    const loaded = await listOslNotes();
    if (!loaded) throw new Error("native Notes storage unavailable");
    oslNotes = loaded;
    if (!activeOslNoteId || !loaded.some((note) => note.id === activeOslNoteId)) {
      activeOslNoteId = loaded[0]?.id ?? null;
    }
  } catch {
    oslNotesError = "Encrypted Notes could not be opened. Unlock this identity and try again.";
  } finally {
    oslNotesLoading = false;
    render();
  }
}

async function createOslNote(): Promise<void> {
  const saved = await saveOslNote({ id: null, title: "", body: "" }).catch(() => null);
  if (!saved) {
    showToast("The encrypted note could not be created");
    return;
  }
  oslNotes = [saved, ...oslNotes];
  activeOslNoteId = saved.id;
  render();
  requestAnimationFrame(() => document.querySelector<HTMLInputElement>("#note-title")?.focus());
}

async function persistActiveOslNote(): Promise<void> {
  const active = oslNotes.find((note) => note.id === activeOslNoteId);
  if (!active) return;
  const title = document.querySelector<HTMLInputElement>("#note-title")?.value ?? active.title;
  const body = document.querySelector<HTMLTextAreaElement>("#note-body")?.value ?? active.body;
  const saved = await saveOslNote({ id: active.id, title, body }).catch(() => null);
  const status = document.querySelector<HTMLElement>("#notes-save-state");
  if (!saved) {
    if (status) status.textContent = "Save failed";
    return;
  }
  oslNotes = [saved, ...oslNotes.filter((note) => note.id !== saved.id)];
  if (status) status.textContent = "Saved locally";
}

function scheduleOslNoteSave(): void {
  const status = document.querySelector<HTMLElement>("#notes-save-state");
  if (status) status.textContent = "Saving…";
  window.clearTimeout(oslNotesSaveTimer);
  oslNotesSaveTimer = window.setTimeout(() => void persistActiveOslNote(), 400);
}

async function removeOslNote(noteId: string): Promise<void> {
  if (!confirm("Delete this encrypted note from this identity?")) return;
  if (!await deleteOslNote(noteId).catch(() => false)) {
    showToast("The note could not be deleted");
    return;
  }
  oslNotes = oslNotes.filter((note) => note.id !== noteId);
  activeOslNoteId = oslNotes[0]?.id ?? null;
  render();
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
  const queuedViewOnce = (oslChatUnread.get(personId) ?? 0) > 0
    ? (oslChatMessages.get(personId) ?? []).filter((message) => message.state === "opened")
    : [];
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
    // No chat plaintext, including encrypted-at-rest history, crosses IPC
    // until the trusted OSL window has capture resistance applied.
    const captureReady = await setScreenshotProtection(true);
    if (!captureReady || epoch !== oslChatOperationEpoch) {
      showToast("Capture resistance could not be enabled");
      return;
    }
    screenshotProtectionEnabled = true;
    const context = await activateOslChatContext(personId);
    if (!context || epoch !== oslChatOperationEpoch || activeOslChatPersonId !== personId) {
      showToast("OSL Chat could not open");
      return;
    }
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
      oslChatAttachments = await listOslChatAttachments() ?? [];
      shouldRefresh = true;
    }
  } finally {
    if (epoch === oslChatOperationEpoch) {
      oslChatBusy = false;
      render();
    }
  }
  if (shouldRefresh && epoch === oslChatOperationEpoch) await refreshOslChat();
}

function persistOslChatNotifications(): void {
  const metadata = (appNotifications ?? []).filter((item) => item.detail === "New encrypted message").slice(0, 20);
  localStorage.setItem(oslChatNotificationStorageKey, JSON.stringify(metadata));
}

function mergePersistedOslChatNotifications(items: AppNotification[] | null): AppNotification[] {
  const chat = (appNotifications ?? []).filter((item) => item.detail === "New encrypted message");
  const merged = [...chat, ...(items ?? [])];
  return merged.filter((item, index) => merged.findIndex((candidate) => candidate.id === item.id) === index).slice(0, 20);
}

function commitOslChatBatch(personId: string, batch: OslChatOpenedBatch, background: boolean): void {
  const messages = [...(oslChatMessages.get(personId) ?? [])];
  let unreadAdded = 0;
  for (const acknowledgment of batch.acknowledgments) {
    const message = messages.find((candidate) => candidate.messageId === acknowledgment.messageId);
    if (message) message.state = acknowledgment.status;
  }
  if (batch.acknowledgments.length) {
    oslChatRemoteAccessConfirmed.add(personId);
    localStorage.setItem(oslChatRemoteAccessStorageKey, JSON.stringify([...oslChatRemoteAccessConfirmed].slice(0, 512)));
  }
  for (const incoming of batch.messages) {
    if (background && incoming.viewOnceConsumed) {
      // Fail closed if an older backend reveals a view-once payload during a
      // metadata-only poll. Never let that plaintext cross into Home state.
      unreadAdded += 1;
      continue;
    }
    const localMessageId = `received-${crypto.randomUUID()}`;
    messages.push({ messageId: localMessageId, direction: "incoming", body: incoming.plaintext, state: incoming.viewOnceConsumed ? "opened" : "received", timestampLabel: oslChatTimestamp() });
    if (background) unreadAdded += 1;
  }
  if (background && batch.pendingViewOnce.length) {
    const pending = oslChatPendingViewOnce.get(personId) ?? new Set<string>();
    for (const item of batch.pendingViewOnce) if (!pending.has(item.messageId)) {
      pending.add(item.messageId);
      unreadAdded += 1;
    }
    oslChatPendingViewOnce.set(personId, pending);
  } else if (!background) {
    oslChatPendingViewOnce.delete(personId);
  }
  if (background && unreadAdded) {
    oslChatUnread.set(personId, Math.min(10_000, (oslChatUnread.get(personId) ?? 0) + unreadAdded));
    if (notificationsEnabled && notificationChatActivity && !oslChatMutedPeople.has(personId)) {
      const localNotificationId = `osl-chat-${crypto.randomUUID()}`;
      appNotifications = [{ id: localNotificationId, title: "OSL Chat", detail: "New encrypted message", createdAt: "Now" }, ...(appNotifications ?? [])].slice(0, 20);
    }
  }
  oslChatMessages.set(personId, messages.slice(-200));
  if (background && unreadAdded) {
    persistOslChatUnread();
    if (notificationsEnabled) persistOslChatNotifications();
    renderWhenIdle();
  }
}

async function syncOslChatsInBackground(): Promise<void> {
  if (oslChatBackgroundBusy || route !== "home" || activeContextToken || activeOslChatPersonId || activeNativeHostId || activeEmbeddedHost || !core.readiness.identityLoaded) return;
  const people = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange).slice(0, 32);
  if (!people.length) return;
  oslChatBackgroundBusy = true;
  try {
    // A sender can require capture protection. Apply it before asking any
    // approved friend inbox to return plaintext, even for background sync.
    if (!await setScreenshotProtection(true)) return;
    screenshotProtectionEnabled = true;
    for (const person of people) {
      if (route !== "home" || activeOslChatPersonId || activeContextToken || activeNativeHostId || activeEmbeddedHost) break;
      const context = await activateOslChatContext(person.personId);
      if (!context) continue;
      try {
        if (!context.scopeApproved) continue;
        const batch = await openOslChatText(false);
        if (batch) commitOslChatBatch(person.personId, batch, true);
        const history = await listOslChatHistory();
        if (history) {
          const existingViewOnce = (oslChatMessages.get(person.personId) ?? []).filter((message) => message.state === "opened");
          oslChatMessages.set(person.personId, [...historyMessages(context, history), ...existingViewOnce].slice(-200));
        }
      } finally {
        await closeOslChatContext();
      }
    }
  } finally {
    oslChatBackgroundBusy = false;
  }
}

async function toggleOslChatPermission(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || oslChatBusy || oslChatSettingsPersonId !== context.personId) return;
  const next = !context.scopeApproved;
  oslChatBusy = true;
  render();
  const saved = await setActiveHubFriendPermission(context.contextToken, context.personId, next, false);
  if (!saved) {
    oslChatBusy = false;
    showToast(next ? "Encrypted chat approval could not be saved" : "Encrypted chat permission could not be revoked");
    render();
    return;
  }
  activeOslChatContext = { ...context, scopeApproved: next };
  hubPeople = await listHubPeople() ?? hubPeople;
  oslChatBusy = false;
  showToast(next ? "Encrypted chat enabled" : "Encrypted chat revoked");
  render();
}

async function approveOslChat(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || oslChatBusy || context.scopeApproved) return;
  const epoch = oslChatOperationEpoch;
  oslChatBusy = true;
  render();
  let approved = false;
  try {
    const saved = await setActiveHubFriendPermission(context.contextToken, context.personId, true, false);
    if (epoch !== oslChatOperationEpoch || activeOslChatContext?.contextToken !== context.contextToken) return;
    if (!saved) {
      showToast("Encrypted chat approval could not be saved");
      return;
    }
    activeOslChatContext = { ...context, scopeApproved: true };
    hubPeople = await listHubPeople() ?? hubPeople;
    approved = epoch === oslChatOperationEpoch;
  } finally {
    if (epoch === oslChatOperationEpoch) {
      oslChatBusy = false;
      render();
    }
  }
  if (approved) await refreshOslChat();
}

async function refreshOslChat(): Promise<void> {
  const context = activeOslChatContext;
  const personId = activeOslChatPersonId;
  if (!context?.scopeApproved || !personId || oslChatBusy) return;
  const epoch = oslChatOperationEpoch;
  oslChatBusy = true;
  render();
  // First-party OSL Chats always apply capture resistance before plaintext is
  // requested. A sender's capture requirement therefore survives the receiver's
  // local display preference.
  const captureReady = await setScreenshotProtection(true);
  if (!captureReady || epoch !== oslChatOperationEpoch || activeOslChatContext?.contextToken !== context.contextToken) {
    showToast("Capture resistance could not be enabled");
    if (epoch === oslChatOperationEpoch) { oslChatBusy = false; render(); }
    return;
  }
  screenshotProtectionEnabled = true;
  const batch = await openOslChatText();
  oslChatAttachments = await listOslChatAttachments() ?? oslChatAttachments;
  // Draining is destructive at the relay. Commit any returned batch to this
  // conversation even if a later UI transition supersedes the render.
  if (batch) commitOslChatBatch(personId, batch, false);
  if (epoch === oslChatOperationEpoch) {
    oslChatBusy = false;
    render();
  }
}

async function sendOslChatAttachment(): Promise<void> {
  if (!activeOslChatContext?.scopeApproved || oslChatBusy) return;
  oslChatBusy = true;
  render();
  const result = await selectOslChatAttachment(oslChatViewOnce);
  oslChatAttachments = await listOslChatAttachments() ?? oslChatAttachments;
  oslChatBusy = false;
  if (result === null) showToast("Encrypted attachment was not sent");
  else if (result !== "cancelled") showToast("Encrypted attachment delivered");
  render();
}

async function openPendingOslChatAttachment(attachmentId: string): Promise<void> {
  if (!activeOslChatContext?.scopeApproved || oslChatBusy || !attachmentId) return;
  oslChatBusy = true;
  render();
  const opened = await openOslChatAttachment(attachmentId);
  oslChatAttachments = await listOslChatAttachments() ?? [];
  oslChatBusy = false;
  showToast(opened ? (opened.viewOnceConsumed ? "View-once attachment opened" : "Attachment opened") : "Attachment could not be opened");
  render();
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
  if (!sent) {
    oslChatBusy = false;
    showToast("Encrypted message was not sent");
    render();
    return;
  }
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
  oslChatAttachments = [];
  if (clearMessages) {
    oslChatMessages.clear();
    oslChatPendingViewOnce.clear();
  }
}

function discardOpenedOslChatMessages(): void {
  if (!activeOslChatPersonId) return;
  const durableMessages = (oslChatMessages.get(activeOslChatPersonId) ?? [])
    .filter((message) => message.state !== "opened");
  oslChatMessages.set(activeOslChatPersonId, durableMessages);
}

async function closeOslChat(): Promise<void> {
  if (oslChatBusy) return;
  if (!(await closeOslChatContext())) {
    showToast("OSL Chat could not close safely");
    return;
  }
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
  showToast(await copyHubFriendInvite(friendCode) ? "Invite copied" : "Could not copy the invite");
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
  appNotifications = mergePersistedOslChatNotifications(requested ? await loadAppNotifications() : []);
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

function changePrivacyFeature(input: HTMLInputElement): void {
  const id = input.dataset.privacyFeature;
  if (!id || !setupPrivacyChoiceIds.includes(id as SetupPrivacyChoiceId)) return;
  const choice = id as SetupPrivacyChoiceId;
  if (input.checked) setupPrivacyChoices.add(choice); else setupPrivacyChoices.delete(choice);
  lastTrustedActivityAt = Date.now();
  persistActivePrivacyFeatureChoices();
  render();
}

async function openCheckedExternalLink(url: string): Promise<void> {
  if (!setupPrivacyChoices.has("external-default-browser")) {
    showToast("External links are blocked · enable default-browser links in Privacy");
    return;
  }
  const decision = checkExternalLink(url, setupPrivacyChoices.has("ip-grabber-protection"));
  if (!decision.allowed) {
    showToast(decision.reason === "knownLinkGrabber" ? "Blocked a known IP-logger link" : "That link is not safe to open");
    return;
  }
  showToast(await openExternalLinkInDefaultBrowser(decision.normalizedUrl) ? "Opened in your default browser" : "Windows could not open that link");
}

function scheduleCopiedMessageClear(): void {
  if (!setupPrivacyChoices.has("clear-clipboard")) return;
  void scheduleProtectedClipboardClear(protectedClipboardClearSeconds).then((scheduled) => {
    if (!scheduled) showToast("Timed clipboard clearing is available in the Windows app");
  });
}

function clearUnlockedRendererState(): void {
  window.clearTimeout(oslNotesSaveTimer);
  oslNotesSaveTimer = undefined;
  oslNotes = [];
  activeOslNoteId = null;
  oslNotesError = "";
  oslNotesLoading = false;
  resetOslChatUiState(true);
  oslChatDraft = "";
  peerProtectedSheet = blankPeerProtectedModel();
  localProtectedSheet = blankLocalProtectedModel();
  activeContextToken = null;
  activeProtectedContextKind = null;
  activeEmbeddedHost = null;
  activeNativeHostId = null;
  activeNativeHostMode = null;
  activeDefaultBrowserCompanion = false;
  mullvadWindowHosted = false;
  privacyScanResult = null;
  privacyScanFileName = null;
  selectedScrubFindings.clear();
  recoveryBundle = null;
}

async function enforceIdleLock(): Promise<void> {
  if (autoLockInProgress || !setupPrivacyChoices.has("auto-lock") || route === "onboarding" || !core.readiness.unlocked) return;
  autoLockInProgress = true;
  try {
    if (!(await lockHubSession())) {
      // If the native key-clear path is unavailable, terminating the process
      // is the only honest fail-closed result.
      await getCurrentWindow().close().catch(() => undefined);
      return;
    }
    clearUnlockedRendererState();
    core = await loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration));
    route = "onboarding";
    onboardingRoute = "unlock";
    lastWorkspaceMarkup = null;
    lastWorkspaceViewKey = "";
    renderNow();
    showToast("OSL locked after 5 minutes idle");
  } finally {
    autoLockInProgress = false;
  }
}

function checkAutoLock(): void {
  if (!setupPrivacyChoices.has("auto-lock") || route === "onboarding" || !core.readiness.unlocked) return;
  if (Date.now() - lastTrustedActivityAt >= autoLockIdleMilliseconds) void enforceIdleLock();
}

function ensureAutoLockTimer(): void {
  if (autoLockCheckTimer !== undefined) return;
  autoLockCheckTimer = window.setTimeout(() => {
    autoLockCheckTimer = undefined;
    checkAutoLock();
    ensureAutoLockTimer();
  }, 1_000);
}

async function refreshIdentitySlots(): Promise<void> {
  hubIdentities = await listHubIdentities() ?? [];
}

async function refreshIdentityScopedState(): Promise<void> {
  window.clearTimeout(oslNotesSaveTimer);
  oslNotesSaveTimer = undefined;
  oslNotes = [];
  activeOslNoteId = null;
  oslNotesError = "";
  oslNotesLoading = false;
  if (activeOslChatContext && !(await closeOslChatContext())) {
    throw new Error("OSL Chat could not close before changing identity state");
  }
  resetOslChatUiState(true);
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
  loadActivePrivacyFeatureChoices();
  refreshActiveBrowserAccountsReady();
  hubIdentities = nextIdentities;
  friendCode = friendProfile?.friendCode ?? null;
  friendDisplayId = friendProfile?.oslUserId ?? null;
  oslProfile = profile;
  const usernameStatus = profile ? await getOslUsernameStatus(profile.usernameCandidate) : null;
  claimedOslUsername = usernameStatus?.ownedByActiveIdentity ? usernameStatus.username : null;
  profileDraftAvatar = undefined;
  hubPeople = people;
  services = linkedServices;
  appNotifications = mergePersistedOslChatNotifications(notifications);
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
    if (route === "onboarding" && onboardingRoute === "pro" && licenseState.access !== "free") onboardingRoute = "sending";
    render();
    showToast(licenseState.access === "free" ? "This code does not include active Pro access" : "Pro activated on this device");
  } catch (failure) {
    if (submit) { submit.disabled = false; submit.textContent = route === "onboarding" && onboardingRoute === "pro" ? "Continue" : "Activate Pro"; }
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
  loadActivePrivacyFeatureChoices();
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
  loadActivePrivacyFeatureChoices();
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
  recoveryBundle = null;
  recoverySavedAcknowledged = false;
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
  nativeHostFailureNotice = "";
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
  document.querySelectorAll<HTMLButtonElement>("[data-source-repository]").forEach((button) => button.addEventListener("click", async () => { if (!(await openHubSourceRepository())) showToast("Could not open the fixed OSL source repository"); }));
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
  ensureAutoLockTimer();
  void setScreenshotProtection(windowCaptureEnabled).then((applied) => {
    screenshotProtectionEnabled = windowCaptureEnabled ? applied : false;
    if (applied) return;
    if (!windowCaptureEnabled) return;
    if (route === "settings" && settingsSection === "scrub") render();
    showToast("Windows capture resistance is unavailable on this Windows session");
  });
  if (route === "onboarding") return;
  void openMullvadOnStartup();
  void loadHubPasswordRoleStatus().then((status) => { passwordRoleStatus = status; if (route === "settings" && settingsSection === "account") renderWhenIdle(); }).catch(() => undefined);
  void refreshUpdateStatus(true);
  void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") renderWhenIdle(); });
  void loadOslProfile().then(async (profile) => {
    oslProfile = profile;
    profileDraftAvatar = undefined;
    const usernameStatus = profile ? await getOslUsernameStatus(profile.usernameCandidate) : null;
    claimedOslUsername = usernameStatus?.ownedByActiveIdentity ? usernameStatus.username : null;
    if (route === "home" || (route === "settings" && settingsSection === "account")) renderWhenIdle();
  });
  void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") renderWhenIdle(); });
  if (notificationsEnabled) void setNotificationsEnabled(true).then(async (enabled) => {
    appNotifications = enabled ? mergePersistedOslChatNotifications(await loadAppNotifications()) : null;
    if (route === "home") renderWhenIdle();
  });
  void refreshIdentitySlots().then(() => { if (route === "settings" && settingsSection === "account") renderWhenIdle(); });
}

async function recoverNativeHostAfterRendererLoad(): Promise<void> {
  if (route === "onboarding" || activeNativeHostId) return;
  const recovered = await withNativeDeadline(
    resizeNativeAppWindow(),
    "Restore Windows app",
    3_000,
  ).catch(() => null);
  if (recovered?.status !== "resized") return;
  const app = homeAppsFromServices(services).find((candidate) => candidate.id === recovered.id);
  const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
  if (!app || !service) return;
  activeNativeHostId = recovered.id;
  activeNativeHostMode = recovered.mode === "existingNativeCompanion" ? "existingSession" : "dedicated";
  activeHomeAppId = app.id;
  activeService = service;
  route = "service";
  serviceGuideStep = null;
  serviceAccountPickerOpen = false;
  render();
}

let discordQaShellStarting = false;
let discordQaShellStarted = false;

async function startDiscordQaShell(): Promise<void> {
  if (!discordQaShell || discordQaShellStarting || discordQaShellStarted || !core.readiness.unlocked) return;
  discordQaShellStarting = true;
  discordQaHostState = "starting";
  discordQaOverlayState = "starting";
  try {
    services = await loadLinkedServices();
    nativeApps = await loadNativeApps();
    const service = services.find((candidate) => candidate.id === "discord");
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === "discord");
    if (!service || !app) throw new Error("Discord is unavailable in this QA identity");
    setNativeSessionMode("discord", "existingSession");
    savedAccountMode = "use";
    savedNativeApps.add("discord");
    route = "service";
    serviceGuideStep = 0;
    activeService = service;
    activeHomeAppId = app.id;
    renderNow();
    await openNativeHostedApp(app, service, "discord");
    if (activeNativeHostId !== "discord" || activeNativeHostMode !== "existingSession") {
      throw new Error("The existing Discord session could not be claimed");
    }
    discordQaShellStarted = true;
    discordQaHostState = "hosted";
    hubPeople = await listHubPeople().catch(() => null) ?? [];
    const verifiedStablePeers = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
    if (verifiedStablePeers.length !== 1) {
      discordQaOverlayState = "failed";
      nativeProtectFailureNotice = "Protection stopped: this QA identity requires exactly one verified friend.";
      render();
      return;
    }
    const overlayOpened = await openNativeDiscordProtection(verifiedStablePeers[0].personId);
    discordQaOverlayState = overlayOpened ? "ready" : "failed";
    render();
  } catch (failure) {
    route = "service";
    serviceGuideStep = 0;
    discordQaHostState = "failed";
    discordQaOverlayState = "failed";
    nativeHostFailureNotice = localActionError(failure, "Discord QA could not start");
    render();
    showToast(nativeHostFailureNotice);
  } finally {
    discordQaShellStarting = false;
  }
}

async function openMullvadOnStartup(): Promise<void> {
  if (!mullvadAutoStart || mullvadAutoStartAttempted || route === "onboarding") return;
  mullvadAutoStartAttempted = true;
  const hosted = await hostMullvadWindow().catch(() => null);
  if (!hosted || hosted.status !== "hosted" || hosted.mode !== "existingMullvadSession" || hosted.captureProtected) return;
  mullvadWindowHosted = true;
  mullvadReturnRoute = "home";
  route = "mullvad";
  render();
}

async function bootstrap(): Promise<void> {
  const attempt = ++bootstrapEpoch;
  mullvadAutoStartAttempted = false;
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
    loadActivePrivacyFeatureChoices();
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
    const browserCompanionRequest = withNativeDeadline(loadDefaultBrowserCompanionStatus(), "Check default browser", bootSupportDeadlineMs).catch(() => null);
    const browserCatalogRequest = withNativeDeadline(loadBrowserImports(), "Load browsers", bootSupportDeadlineMs).catch(() => null);
    const preferences = await preferencesRequest ?? {
      onboardingComplete: core.readiness.bootstrapStatus === "ready",
      setup: parseSetupState(null),
      showPlaintextPreview: true,
      windowCaptureEnabled: true,
    };
    if (attempt !== bootstrapEpoch) return;
    setup = preferences.setup;
    windowCaptureEnabled = preferences.windowCaptureEnabled;
    onboardingComplete = preferences.onboardingComplete;
    if (onboardingComplete && mullvadPreference === "auto") void openMullvad().catch(() => undefined);
    if (core.readiness.bootstrapStatus === "setupRequired") {
      onboardingRoute = "welcome";
      route = "onboarding";
    } else if (core.readiness.bootstrapStatus === "passwordRequired") {
      onboardingRoute = "unlock";
      route = "onboarding";
    } else {
      route = preferences.onboardingComplete ? "home" : "onboarding";
      if (!preferences.onboardingComplete) onboardingRoute = pendingOnboardingRoute() ?? "pro";
    }
    renderNow();
    if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    startReadyWorkspaceLoads();
    void Promise.all([servicesRequest, nativeAppsRequest, firefoxRequest, licenseRequest, browserCompanionRequest, browserCatalogRequest]).then(([linkedServices, nativeCatalog, currentFirefoxStatus, currentLicenseState, currentBrowserCompanionStatus, browserCatalog]) => {
      if (attempt !== bootstrapEpoch) return;
      if (linkedServices) services = linkedServices;
      if (nativeCatalog && isCompleteNativeCatalog(nativeCatalog)) {
        nativeApps = nativeCatalog;
      }
      if (currentFirefoxStatus) firefoxStatus = currentFirefoxStatus;
      if (currentLicenseState) licenseState = currentLicenseState;
      if (currentBrowserCompanionStatus) defaultBrowserCompanionStatus = currentBrowserCompanionStatus;
      if (browserCatalog) browserImports = browserCatalog;
      applySavedScrubSetupPlan();
      renderWhenIdle();
      if (discordQaShell) void startDiscordQaShell();
      else void recoverNativeHostAfterRendererLoad();
    });
  } catch {
    if (attempt === bootstrapEpoch) showBootstrapRecovery();
    return;
  }
}

window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => { if (themeChoice === "system") applyTheme("system"); });
window.addEventListener("keydown", (event) => {
  lastTrustedActivityAt = Date.now();
  if (event.key !== "F11" || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  event.preventDefault();
  void toggleDesktopFullscreen().catch(() => undefined);
});
window.addEventListener("pointerdown", () => { lastTrustedActivityAt = Date.now(); }, { capture: true, passive: true });
window.addEventListener("pointermove", () => { lastTrustedActivityAt = Date.now(); }, { capture: true, passive: true });
window.addEventListener("wheel", () => { lastTrustedActivityAt = Date.now(); }, { capture: true, passive: true });
window.addEventListener("touchstart", () => { lastTrustedActivityAt = Date.now(); }, { capture: true, passive: true });
document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") checkAutoLock(); });
let nativeHostResizeFrame = 0;
let nativeHostValidationBusy = false;
let nativeHostValidationPending = false;

async function validateNativeSurfacesPass(): Promise<void> {
    if (activeNativeHostId) {
      const name = activeHomeAppName();
      const resized = await withNativeDeadline(resizeNativeAppWindow(), `Restore ${name}`, 3_000).catch(() => null);
      if (resized?.status !== "resized") {
        if (activeNativeHostMode === "existingSession") {
          render();
          showToast(`${name} closed. Use Bring forward or reopen.`);
        } else {
          // One bounded recovery attempt uses the same signed executable,
          // fixed dedicated profile, and exact OSL owner path. Failure clears
          // the active state inside openNativeHostedApp; no retry loop runs.
          await reopenActiveNativeCompanion();
          if (!activeNativeHostId) {
            if (route === "service") serviceGuideStep = 0;
            render();
            showToast(`${name} closed and could not be reopened safely.`);
          }
        }
      }
    }
    if (activeDefaultBrowserCompanion) {
      const resized = await withNativeDeadline(resizeDefaultBrowserCompanion(), "Restore browser companion", 3_000).catch(() => null);
      const focused = resized?.status === "resized"
        ? await withNativeDeadline(focusDefaultBrowserCompanion(), "Focus browser companion", 3_000).catch(() => null)
        : null;
      if (resized?.status !== "resized" || focused?.status !== "focused") {
        activeDefaultBrowserCompanion = false;
        if (route === "service") serviceGuideStep = 0;
        render();
        showToast("The browser companion closed. Open it again when you’re ready.");
      }
    }
    if (mullvadWindowHosted) {
      const resized = await withNativeDeadline(resizeMullvadWindow(), "Restore Mullvad", 3_000).catch(() => null);
      const focused = resized?.status === "resized"
        ? await withNativeDeadline(focusMullvadWindow(), "Focus Mullvad", 3_000).catch(() => null)
        : null;
      if (resized?.status !== "resized" || focused?.status !== "focused") {
        mullvadWindowHosted = false;
        const reopened = await hostMullvadWithDeadline("Reopen Mullvad").catch(() => null);
        if (reopened?.status === "hosted") {
          mullvadWindowHosted = true;
          render();
        } else {
          route = mullvadReturnRoute;
          if (route === "onboarding") onboardingRoute = "mullvad";
          render();
          showToast("Mullvad could not be reopened");
        }
      }
    }
}

async function validateNativeSurfaces(): Promise<void> {
  if (nativeHostValidationBusy) {
    nativeHostValidationPending = true;
    return;
  }
  nativeHostValidationBusy = true;
  try {
    do {
      // Every event observed during this pass requests one new trailing pass.
      // Multiple move/resize/focus events collapse into that same pass.
      nativeHostValidationPending = false;
      await validateNativeSurfacesPass();
    } while (nativeHostValidationPending);
  } finally {
    nativeHostValidationBusy = false;
  }
}

function scheduleNativeHostRealignment(): void {
  if ((!activeNativeHostId && !activeDefaultBrowserCompanion && !mullvadWindowHosted) || nativeHostResizeFrame) return;
  nativeHostResizeFrame = requestAnimationFrame(() => {
    nativeHostResizeFrame = 0;
    void validateNativeSurfaces();
  });
}
window.addEventListener("resize", scheduleNativeHostRealignment);
const desktopWindow = getCurrentWindow();
void desktopWindow.onMoved(scheduleNativeHostRealignment).catch(() => undefined);
void desktopWindow.onResized(scheduleNativeHostRealignment).catch(() => undefined);
void desktopWindow.onFocusChanged(({ payload }) => {
  if (payload) scheduleNativeHostRealignment();
}).catch(() => undefined);
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") void syncOslChatsInBackground();
  if (document.visibilityState !== "hidden" || !newIdentityRecoveryPhrase) return;
  newIdentityRecoveryPhrase = null;
  if (route === "settings" && settingsSection === "account") render();
});
window.addEventListener("error", (event) => { event.preventDefault(); containBackgroundFailure(); });
window.addEventListener("unhandledrejection", (event) => { event.preventDefault(); containBackgroundFailure(); });
function scheduleOslChatBackgroundSync(delayMs = 30_000): void {
  window.setTimeout(() => {
    void syncOslChatsInBackground().finally(() => scheduleOslChatBackgroundSync());
  }, delayMs);
}
scheduleOslChatBackgroundSync(1_000);
void bootstrap();
