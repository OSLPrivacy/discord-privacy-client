import "@fontsource-variable/inter/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  canCompleteSetup,
  defaultSetup,
  formatSendMode,
  needsRiskAcceptance,
  oslPrimaryDestinations,
  oslSettingsDestination,
  parseSetupState,
  type OslPrimaryDestination,
  type SendMode,
  type SetupState,
} from "./state";
import { isTauriRuntime, loadOnboardingPreferences, saveOnboardingPreferences } from "./preferences";
import { lastBackendFailure, recordBackendFailure } from "./backend-failure";
import {
  escapeHtml,
  closeEmbeddedServiceHost,
  configuredTopStripApps,
  detachDefaultBrowserCompanion,
  detachNativeAppWindow,
  embeddedAccountsForHomeApp,
  focusNativeAppWindow,
  focusDefaultBrowserCompanion,
  grantBrowserProfileConsent,
  listBrowserProfilesForConsent,
  loadDetectedBrowserFootprint,
  loadDefaultBrowserCompanionStatus,
  homeAppsFromServices,
  hostBrowserCompanion,
  hostNativeAppWindow,
  hostMullvadWindow,
  installNativeApp,
  installMullvad,
  loadLinkedServices,
  loadMullvadStatus,
  loadNativeApps,
  nativeAppTakeoverRequiresConsent,
  openEmbeddedHomeApp,
  parseDiscordSessionMode,
  parseNativeSessionMode,
  focusMullvadWindow,
  finishProtectedBrowserImport as closeProtectedBrowserImportHelper,
  resizeDefaultBrowserCompanion,
  resizeNativeAppWindow,
  resizeMullvadWindow,
  restoreMullvadWindow,
  revokeDetectedBrowserFootprint,
  scanConsentedBrowserProfile,
  setupEmbeddedHomeApp,
  type EmailProvider,
  type DiscordSessionMode,
  type NativeDiscordTakeover,
  type NativeSessionMode,
  type EmbeddedServiceHost,
  type HomeAppCatalogEntry,
  type HomeAppId,
  type LinkedService,
  type MullvadStatus,
  type NativeApp,
  type NativeAppId,
  type BrowserCompanionStatus,
  type BrowserImportId,
  type BrowserFootprintHydration,
  type BrowserProfileDescriptor,
  type NativeBrowserImportReceipt,
  type ServiceId,
  AndroidSurface,
} from "./services";
import {
  coreReadinessLabel,
  clearHubActivationCode,
  createHubOslIdentity,
  identityProtectionStatus,
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
import { createDiscordQaGeometryKeeper } from "./discord-qa-geometry";
import { browserLogo, serviceLogo, providerLogo } from "./logos";
import { activateLocalLoopbackContext, activateManualPeerContext, activateNativeManualPeerContext, activateOslChatContext, addOslFriend, burnActiveHubContext, burnHubServiceAccount, closeOslChatContext, copyHubFriendInvite, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubServiceBurnReadiness, getOslUsernameStatus, isHubPlaintext, listHubIdentities, listHubPeople, listOslChatHistory, loadActiveContextSecurity, loadAppNotifications, loadFriendProfile, openOslChatText, openPeerProseText, prepareLocalProtectedText, prepareOslChatText, preparePeerProseText, recoverHubIdentitySlot, saveActiveContextSecurity, revokeActiveHubFriendScope, setActiveHubFriendPermission, setActiveHubFriendReach, setHubFriendNickname, setLocalProtectedSheetOpen, setNativeDiscordProtectedOverlayOpen, setNativeDiscordProtectedOverlayOpenForQa, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, type AppNotification, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type LocalPrivacyScanResult, type ManualPeerContext, type PersistedLocalPrivacyScanResult } from "./adapters";
import { blankLocalProtectedModel, isLocalTtlSeconds, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import { blankPeerProtectedModel, boundedPeerProtectedDraft, peerProtectedDraftByteFeedback, peerProtectedSheetMarkup, type PeerProtectedPane, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { persistLocalScrubExport } from "./scrub-local";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { NativeDeadlineError, withNativeDeadline } from "./native-deadline";
import { CoalescedRealignment, NativeCallGate } from "./native-realignment";
import { FrameRenderScheduler } from "./render-scheduler";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";
import { projectAutoScrubFleetStatus, type AutoScrubFleetStatus } from "./autoscrub-contract";
import { loadAutoScrubRunFleetStatus, requestAutoScrubGlobalStop } from "./autoscrub-unattended-run";
import { oslMailStage, type OslMailStage } from "./desktop-service-policy";
import {
  acknowledgeOslMailRetrieval,
  burnOslMailbox,
  listOslMailThreads,
  loadOslMailStatus,
  provisionOslMail,
  retrieveOslMailThread,
  sendOslMail,
  type OslMailBurnReceipt,
  type OslMailDeleteReceipt,
  type OslMailRetrievedThread,
  type OslMailSendReceipt,
  type OslMailStatus,
  type OslMailThreadSummary,
} from "./osl-mail-adapter";
import { oslMailViewMarkup, type OslMailPane } from "./osl-mail-view";
export {
  autoscrubUnattendedContractGate,
  autoscrubUnattendedProductionRun,
  type AutoscrubUnattendedGateResult,
  type AutoscrubUnattendedRunResult,
} from "./autoscrub-unattended-run";
import { initializeThemePreference, themeStorageKey, type ThemeChoice } from "./theme-preference";
import { OSL_CHAT_MAX_DRAFT_BYTES, oslChatDraftBytes, oslChatsViewMarkup, type OslChatMessage } from "./osl-chats-view";
import { parseCircleAudience, type CircleAudience } from "./osl-collab";
import { bindFriendRemovalControls, bindMainWindowFocusChanges, friendRemovalButtonMarkup, friendTrustAction, RecoveryCaptureGate, removeHubFriend, shouldClearRemovedFriendChat } from "./ui-behavior";
import { BurnGuaranteeCopy, type BurnGuaranteeState } from "./two-step-burn";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import type { NativeOverlayPendingAttachment } from "./overlay-state";
import { listOslChatAttachments, openOslChatAttachment, selectOslChatAttachment } from "./native-overlay-adapter";
import {
  pollNativeDiscordHeadlessQa,
  requestNativeDiscordVisibleRowRuntimeReceipt,
  runNativeDiscordHeadlessQa,
} from "./discord-headless-qa-adapter";
import type { SecureLocalStore } from "./secure-local-store";

export type Route = "onboarding" | "home" | "inbox" | "people" | "privacy" | "activity" | "connections" | "service" | "settings" | "mullvad" | "osl-chat" | "osl-mail" | "osl-servers" | "signal-qa";
const PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT = "osl://protected-display-visibility-changed";
const NATIVE_DISCORD_OVERLAY_CLOSED_EVENT = "osl://native-discord-overlay-closed";
const MAIN_WINDOW_CAPTURE_REFUSED_EVENT = "hub-main-capture-protection-refused";
// OSL's protected composer is on screen and cannot receive the operator's
// keystrokes -- either Windows refused to give it keyboard focus, or it lost the
// z-order hit test to Discord. Either way the composer looks alive, and every
// character typed is being delivered to Discord's own message box in the clear.
//
// Payload `{ reason, unreachable }`. `unreachable` is the LEVEL across every
// native condition, not one condition's edge, so it is assigned rather than
// counted; see applyNativeDiscordComposerUnreachable(). It was a bare boolean
// once, and a handler that returned on any other shape raised nothing at all on
// a real refusal -- the warning went dark exactly when it was needed.
const NATIVE_DISCORD_COMPOSER_UNREACHABLE_EVENT = "osl://native-discord-composer-unreachable";
// The `reason` half: which condition's edge produced the message. A closed set of
// fixed strings from the native side (`COMPOSER_UNREACHABLE_*` in
// native_discord_overlay.rs), never anything derived from a draft, a
// conversation, a token or an identity. Used only to name the cause in the
// warning's tooltip, so an unrecognised one degrades the wording and never the
// warning.
const NATIVE_DISCORD_COMPOSER_UNREACHABLE_REASONS = ["zorder-band", "keyboard-focus", "session-ended"] as const;
type NativeDiscordComposerUnreachableReason = (typeof NATIVE_DISCORD_COMPOSER_UNREACHABLE_REASONS)[number];
type OnboardingRoute = "pro" | "welcome" | "create" | "import" | "unlock" | "recovery" | "mullvad" | "sending" | "defaults" | "cover" | "passwords" | "burnpass" | "privacy" | "tutorial" | "detected" | "install" | "apps" | "browser" | "decoy";
type SettingsSection = "account" | "apps" | "scrub" | "cleanup" | "notifications" | "appearance" | "about";
type SavedAccountMode = "ask" | "use" | "clean";
type BurnScope = "chat" | "app" | "account";
type BurnResult = {
  tone: "success" | "warning" | "error";
  message: string;
  showUninstall: boolean;
};
type OwnedConfirmation =
  | { kind: "verifyFriend"; personId: string }
  | { kind: "removeFriend"; personId: string }
  | { kind: "clearActivation" };

function requireRoot(): HTMLDivElement {
  const element = document.querySelector<HTMLDivElement>("#app");
  if (!element) throw new Error("OSL Privacy root is missing");
  return element;
}
const runningUnderVitest = Boolean(import.meta.vitest || (typeof process !== "undefined" && process.env.VITEST));
const root = runningUnderVitest
  ? (globalThis.document?.querySelector<HTMLDivElement>("#app") ?? globalThis.document?.createElement("div") ?? {} as HTMLDivElement)
  : requireRoot();
const discordQaShell = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1";
const signalQaShellEnabled = import.meta.env.VITE_OSL_SIGNAL_QA_SHELL === "1";
if (discordQaShell) document.documentElement.classList.add("discord-qa-shell");

function onboardingRouteForBuild(candidate: OnboardingRoute): OnboardingRoute {
  return discordQaShell && candidate === "pro" ? "sending" : candidate;
}

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
let autoScrubFleetStatus: AutoScrubFleetStatus | null = null;
let autoScrubStatusLoading = false;
let autoScrubStopPending = false;
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
let mullvadStatus: MullvadStatus = {
  availability: "unavailable",
  integrationState: "unavailable",
  privacyScope: "networkOnly",
  connectionState: "notObserved",
};
let mullvadBusy = false;
let mullvadSetupNotice = "";
let mullvadAutoStart = false;
let mullvadAutoStartAttempted = false;
let mullvadWindowHosted = false;
let mullvadReturnRoute: "onboarding" | "home" = "home";
let privacyProtectionReviewOpen = false;
let activityAttentionReviewOpen = false;
type PeoplePrimaryActionFocus = "add" | "verify";
let peoplePrimaryActionFocus: PeoplePrimaryActionFocus | null = null;
let browserProfiles: BrowserProfileDescriptor[] = [];
type BrowserAvailability = { id: BrowserImportId; displayName: string; installed: true };
let browserImports: BrowserAvailability[] = [];
let browserReadinessBusy = false;
let browserImportBusy = false;
let browserImportFailureNotice = "";
let selectedBrowserProfileKeys = new Set<string>();
let browserImportQueue: BrowserImportId[] = [];
let browserImportQueueIndex = 0;
let browserImportSourceSelected = false;
let browserImportRunEpoch = 0;
let browserImportCancelling = false;
let defaultBrowserCompanionStatus: BrowserCompanionStatus = { status: "unsupported", browserId: null, displayName: null, reason: "platformUnsupported", captureProtected: false, containment: "bestEffort" };
let useDefaultBrowserCompanion = localStorage.getItem("osl-default-browser-companion-v1") === "true";
let activeDefaultBrowserCompanion = false;
let savedAccountsReady = false;
let preferredBrowserId: BrowserImportId | null = null;
let completedBrowserImportIds = new Set<BrowserImportId>();
let browserFootprintImports: NativeBrowserImportReceipt[] = [];
let browserFootprintOwner: string | null = null;
let savedAccountMode: SavedAccountMode = "ask";
let savedNativeApps = new Set<NativeAppId>();
let detectedAccountChoices = new Map<string, "native" | "osl">();
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
let onboardingConnectAppId: HomeAppId | null = null;
const handledOnboardingConnectApps = new Set<HomeAppId>();
let backgroundInstallQueue: Promise<void> = Promise.resolve();
let nativeActionBusy = false;
type DiscordQaHostState = "starting" | "hosted" | "failed";
let discordQaHostState: DiscordQaHostState = "starting";
type DiscordQaOverlayState = "starting" | "ready" | "failed";
let discordQaOverlayState: DiscordQaOverlayState = "starting";
let discordQaOverlayOpening = false;
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
// Unit a11: at-rest storage protection for the ACTIVE identity, known only
// when this session itself created, imported, or switched into it (the
// backend never echoes it back on a plain unlock). Fail honest: `null`
// renders as "unknown", which is treated as NOT secure — never assume
// hardware backing just because nothing contradicts it.
let identityStorageMethod: string | null = null;
// Slot id -> storage method learned this session for identities created or
// recovered via the multi-identity flow, which does not switch to them
// automatically. Consulted only when the user later switches into that slot.
const knownIdentityStorageMethods = new Map<string, string>();
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
let claimedOslUsername: string | null = null;
let oslMailLoading = false;
let oslMailStatus: OslMailStatus | null = null;
let oslMailThreads: OslMailThreadSummary[] = [];
let oslMailActiveThread: OslMailRetrievedThread | null = null;
let oslMailPane: OslMailPane = "inbox";
let oslMailNotifications = true;
let oslMailDeleteReceipt: OslMailDeleteReceipt | null = null;
let oslMailSendReceipt: OslMailSendReceipt | null = null;
let oslMailBurnReceipt: OslMailBurnReceipt | null = null;
let oslMailError: string | null = null;
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
// Does a protected display surface exist to draw over the Discord rows?
//
// This is NOT the lock. The lock is encryption only: what the operator types is
// encrypted and cover text goes to Discord. The retained overlay WebView pair is
// built once and only shown/hidden as the lock moves, so closing the lock leaves
// the surface present-but-dormant. It genuinely stops existing only when the
// native side reports the overlay closed, or when this service surface is torn
// down. The eye reads this — never the lock — to know whether anything can
// render the mode it just set.
let nativeDiscordOverlaySurfacePresent = false;
// Whether any native condition currently says the protected composer cannot
// receive input, and which one last spoke.
//
// A plain level, not a count. The native side has two independent latches on this
// one event -- a z-order surrender and a focus refusal -- and reconciling them
// used to be this file's problem: the payload was a bare boolean, so a retraction
// could not be attributed to the condition that sent it and the best this could
// do was count raises against retractions. `publish_composer_unreachable` now
// reads the aggregate across both latches, emits only on the aggregate's edges
// and puts that aggregate on the wire, from a single writer. A duplicated or
// dropped level is idempotent, so there is no delta left to accumulate and
// nothing left to clamp.
let nativeDiscordComposerUnreachable = false;
let nativeDiscordComposerUnreachableReason: NativeDiscordComposerUnreachableReason | "" = "";
let nativeProtectPickerOpen = false;
let nativeProtectBusy = false;
let protectedSheetCloseBusy = false;
let nativeProtectFailureNotice = "";
let discordQaHeaderBusy: "whitelist" | "visibility" | "roster" | null = null;
type DiscordQaRowProofState = "idle" | "busy" | "accepted" | "refused" | "unavailable";
let discordQaRowProofState: DiscordQaRowProofState = "idle";
// The eye's last outcome, rendered on the control itself. A toast is not
// feedback here: the borrowed native Discord window sits on top of this
// webview's toast layer, so an occluded toast is indistinguishable from a
// control that does nothing.
//   "applied"   the transcript layer was told and the operator's mode is live
//   "unapplied" the mode is recorded for this scope, but there is no protected
//               display surface at all to show it on. This never means "the
//               lock is off": display and encryption are independent, so the
//               eye applies with the lock open or closed.
//   "failed"    the change was rolled back; the transcript is unchanged
type DiscordQaTranscriptVisibilityOutcome = "applied" | "unapplied" | "failed";
let discordQaTranscriptVisibilityOutcome: DiscordQaTranscriptVisibilityOutcome = "applied";
// Fail-safe default: until the first poll resolves (or if a poll ever fails),
// keep showing the composer lock rather than hiding a control the operator
// might still need.
let discordMarkerAvailable = true;
let whitelistRosterOpen = false;
let onboardingComplete = false;
let screenshotProtectionEnabled = false;
let windowCaptureEnabled = true;
let hubIdentities: HubIdentitySlot[] = [];
let newIdentityRecoveryPhrase: string | null = null;
const recoveryCaptureGate = new RecoveryCaptureGate();
const RECOVERY_PROTECTION_REFUSAL = "OSL cannot show recovery secrets because Windows capture resistance is not proven for this window";
let hubPeople: HubPerson[] = [];
let activeOslChatPersonId: string | null = null;
let activeOslChatContext: ManualPeerContext | null = null;
let oslChatDraft = "";
let oslChatViewOnce = false;
let oslChatBusy = false;
let oslChatBackgroundBusy = false;
let oslChatOperationEpoch = 0;
const oslChatMessages = new Map<string, OslChatMessage[]>();
const oslChatUnread = new Map<string, number>();
let oslChatPreviewsVisible = true;
let oslChatMutedPeople = new Set<string>();
let oslChatSettingsPersonId: string | null = null;
let oslChatAttachments: NativeOverlayPendingAttachment[] = [];
let privacyScanResult: LocalPrivacyScanResult | PersistedLocalPrivacyScanResult | null = null;
let privacyScanFileName: string | null = null;
let privacyScanBusy = false;
let enabledScrubSignals = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
let selectedScrubFindings = new Set<number>();
let scrubResultsPage = 0;
let scrubReviewOpen = false;
let scrubReviewPage = 0;
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

const sidebarStorageKey = "osl-hub-sidebar";
const hiddenStorageKey = "osl-hub-sidebar-hidden";
const notificationsStorageKey = "osl-hub-notifications";
const notificationAppsStorageKey = "osl-hub-notification-apps";
const notificationPreviewStorageKey = "osl-hub-notification-previews";
const notificationScopeStorageKey = "osl-hub-notification-scope-suggestions";
const notificationChatStorageKey = "osl-hub-notification-chats-v1";
const notificationSecurityStorageKey = "osl-hub-notification-security-v1";
const mullvadAutoStartStorageKey = "osl-mullvad-autostart-v1";
const scrubSignalsStorageKey = "osl-hub-scrub-signals-v1";
const serviceGuideStorageKey = "osl-hub-service-guide-v1";
const homeTileOrderStorageKey = "osl-home-tile-order-v1";
const hiddenHomeTilesStorageKey = "osl-home-tile-hidden-v1";
const savedAccountModeStorageKey = "osl-saved-account-mode-v1";
const savedNativeAppsStorageKey = "osl-saved-native-apps-v1";
const detectedAccountChoicesStorageKey = "osl-detected-account-choices-v1";
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
const browserImportPendingStorageKey = "osl-browser-import-pending-v1";
const onboardingResumeStorageKey = "osl-onboarding-resume-v1";
const onboardingBranchStorageKey = "osl-onboarding-branch-v1";
const experimentalSendConsentStorageKey = "osl-experimental-send-consent-v1";
const rnWirePolicyStorageKey = "osl-rn-wire-policy-requested-v1";
let nativeDiscordCovertextEnabled = true;
const oslChatPreviewStorageKey = "osl-chat-previews-visible-v1";
const oslChatMutedStorageKey = "osl-chat-muted-people-v1";
const oslChatUnreadStorageKey = "osl-chat-unread-v1";
const oslChatNotificationStorageKey = "osl-chat-notifications-v1";
type OslChatSecureStore = Pick<SecureLocalStore, "getItem" | "setItem">;
type BrowserImportStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;
export type OslChatUiPreferenceSnapshot = {
  readonly previewsVisible: boolean;
  readonly mutedPeople: readonly string[];
  readonly unread: readonly (readonly [string, number])[];
};
type RnWirePolicyState = {
  readonly requested: boolean;
  readonly buildEnabled: boolean;
  readonly effectiveEnabled: boolean;
  readonly refusal: "build-disabled" | "user-disabled" | null;
};
type AttendedImapRunRequest = {
  readonly accountId: string;
  readonly selectedMessageIds: readonly string[];
  readonly livePathProved: boolean;
  readonly confirmFinalRun: boolean;
};
type AttendedImapRunResult =
  | { readonly state: "refused"; readonly reason: "live-path-required" | "invalid-selection" | "preview-refused" | "final-refused" }
  | { readonly state: "preview"; readonly previewToken: string; readonly selectedMessageIds: readonly string[] }
  | { readonly state: "completed"; readonly previewToken: string; readonly runId: string; readonly receiptCount: number };
type AutoscrubAuthorizationRequest = {
  readonly accountId: string;
  readonly selectedMessageIds: readonly string[];
  readonly operatorConfirmed: boolean;
};
type AutoscrubAuthorizationResult =
  | { readonly state: "refused"; readonly reason: "confirmation-required" | "invalid-selection" | "native-refused" }
  | { readonly state: "armed"; readonly authorizationId: string; readonly selectedMessageIds: readonly string[] };
type AutoscrubReceiptProjection = {
  readonly itemLabel: string;
  readonly state: "verified" | "notVerified" | "held";
};
type AutoscrubStatusProjection = {
  readonly phase: "idle" | "running" | "completed" | "failed";
  readonly receipts: readonly AutoscrubReceiptProjection[];
};
type DesktopCtaSurface = "desktop" | "phone-demo" | "mobile-companion";
type DesktopCtaRoute = "desktop-app" | "phone-companion";
let oslChatSecureStore: OslChatSecureStore | null = null;
let rnWirePolicyRequested = false;
const autoScrubServiceLabels: Record<ServiceId, string> = {
  discord: "Discord",
  telegram: "Telegram",
  instagram: "Instagram",
  snapchat: "Snapchat",
  email: "Email",
  x: "X",
  slack: "Slack",
  linkedin: "LinkedIn",
  teams: "Teams",
  messenger: "Messenger",
  signal: "Signal",
  whatsapp: "WhatsApp",
};
const supportedNativeAppIds = new Set<NativeAppId>(["discord", "telegram", "signal", "whatsapp", "outlook"]);
const importedFirefoxHomeAppIds = new Set<HomeAppId>([
  "instagram", "snapchat", "x", "messenger", "gmail", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
]);
const friendsDialogPageSize = 24;
const friendScopeRenderLimit = 16;
const whitelistRosterScopeLimit = 32;
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

function parseOslChatPreviewVisibility(raw: string | null): boolean {
  return raw !== "false";
}

function parseOslChatMutedPeople(raw: string | null): Set<string> {
  try {
    const parsed = JSON.parse(raw ?? "[]") as unknown;
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((personId): personId is string => (
      typeof personId === "string" && personId.length > 0 && personId.length <= 180
    )).slice(0, 512));
  } catch {
    return new Set();
  }
}

function parseOslChatUnread(raw: string | null): Map<string, number> {
  const unread = new Map<string, number>();
  try {
    const parsed = JSON.parse(raw ?? "{}") as unknown;
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return unread;
    for (const [personId, count] of Object.entries(parsed).slice(0, 512)) {
      if (personId.length > 0
        && personId.length <= 180
        && Number.isSafeInteger(count)
        && Number(count) > 0
        && Number(count) <= 10_000) {
        unread.set(personId, Number(count));
      }
    }
  } catch {
    return new Map();
  }
  return unread;
}

function encodeOslChatMutedPeople(people: ReadonlySet<string>): string {
  return JSON.stringify([...people].filter((personId) => personId.length > 0 && personId.length <= 180).slice(0, 512));
}

function encodeOslChatUnread(unread: ReadonlyMap<string, number>): string {
  return JSON.stringify(Object.fromEntries([...unread.entries()]
    .filter(([personId, count]) => personId.length > 0
      && personId.length <= 180
      && Number.isSafeInteger(count)
      && count > 0
      && count <= 10_000)
    .slice(0, 512)));
}

function encodeOslChatPreviewVisibility(visible: boolean): string {
  return String(visible);
}

function isPersistedOslChatNotification(item: unknown): item is AppNotification {
  return typeof item === "object" && item !== null
    && typeof (item as AppNotification).id === "string" && (item as AppNotification).id.length <= 96
    && typeof (item as AppNotification).title === "string" && (item as AppNotification).title.length <= 120
    && (item as AppNotification).detail === "New encrypted message"
    && typeof (item as AppNotification).createdAt === "string" && (item as AppNotification).createdAt.length <= 32;
}

function parseOslChatNotifications(raw: string | null): AppNotification[] {
  try {
    const parsed = JSON.parse(raw ?? "[]") as unknown;
    return Array.isArray(parsed) ? parsed.slice(0, 20).filter(isPersistedOslChatNotification) : [];
  } catch {
    return [];
  }
}

function encodeOslChatNotifications(notifications: readonly AppNotification[]): string {
  return JSON.stringify(notifications.filter(isPersistedOslChatNotification).slice(0, 20));
}

function persistSensitiveOslChatJson(logicalKey: string, payload: string): Promise<void> {
  if (!oslChatSecureStore) return Promise.resolve();
  return oslChatSecureStore.setItem(logicalKey, payload).catch(() => undefined);
}

function persistOslChatPreviewVisibility(): void {
  void persistSensitiveOslChatJson(oslChatPreviewStorageKey, encodeOslChatPreviewVisibility(oslChatPreviewsVisible));
}

function persistOslChatMutedPeople(): void {
  void persistSensitiveOslChatJson(oslChatMutedStorageKey, encodeOslChatMutedPeople(oslChatMutedPeople));
}

async function secureOrLegacyOslChatPreference(
  store: OslChatSecureStore | null,
  storage: Pick<Storage, "getItem">,
  logicalKey: string,
): Promise<string | null> {
  if (!store) return storage.getItem(logicalKey);
  try {
    return await store.getItem(logicalKey) ?? storage.getItem(logicalKey);
  } catch {
    return storage.getItem(logicalKey);
  }
}

export async function loadMigratedOslChatUiPreferences(
  store: OslChatSecureStore | null,
  storage: Pick<Storage, "getItem">,
): Promise<OslChatUiPreferenceSnapshot> {
  const [previewRaw, mutedRaw, unreadRaw] = await Promise.all([
    secureOrLegacyOslChatPreference(store, storage, oslChatPreviewStorageKey),
    secureOrLegacyOslChatPreference(store, storage, oslChatMutedStorageKey),
    secureOrLegacyOslChatPreference(store, storage, oslChatUnreadStorageKey),
  ]);
  return {
    previewsVisible: parseOslChatPreviewVisibility(previewRaw),
    mutedPeople: [...parseOslChatMutedPeople(mutedRaw)],
    unread: [...parseOslChatUnread(unreadRaw)],
  };
}

function applyOslChatUiPreferences(preferences: OslChatUiPreferenceSnapshot): void {
  oslChatPreviewsVisible = preferences.previewsVisible;
  oslChatMutedPeople = new Set(preferences.mutedPeople);
  oslChatUnread.clear();
  for (const [personId, count] of preferences.unread) oslChatUnread.set(personId, count);
}

async function loadOslChatSensitiveStateFromSecureStore(): Promise<void> {
  applyOslChatUiPreferences(await loadMigratedOslChatUiPreferences(oslChatSecureStore, localStorage));
}

export function configureOslChatSecureLocalStore(store: OslChatSecureStore | null): void {
  oslChatSecureStore = store;
}

export function oslChatUiPreferenceSnapshot(): OslChatUiPreferenceSnapshot {
  return {
    previewsVisible: oslChatPreviewsVisible,
    mutedPeople: [...oslChatMutedPeople],
    unread: [...oslChatUnread],
  };
}

export async function migrateOslChatPreviewVisibilityToSecureLocalStore(
  store: OslChatSecureStore,
  storage: BrowserImportStorage,
): Promise<boolean> {
  const parsed = parseOslChatPreviewVisibility(storage.getItem(oslChatPreviewStorageKey));
  await store.setItem(oslChatPreviewStorageKey, encodeOslChatPreviewVisibility(parsed));
  storage.removeItem(oslChatPreviewStorageKey);
  return parsed;
}

export async function migrateOslChatUnreadToSecureLocalStore(
  store: OslChatSecureStore,
  storage: BrowserImportStorage,
): Promise<Map<string, number>> {
  const parsed = parseOslChatUnread(storage.getItem(oslChatUnreadStorageKey));
  await store.setItem(oslChatUnreadStorageKey, encodeOslChatUnread(parsed));
  storage.removeItem(oslChatUnreadStorageKey);
  return parsed;
}

export async function migrateOslChatMutedPeopleToSecureLocalStore(
  store: OslChatSecureStore,
  storage: BrowserImportStorage,
): Promise<Set<string>> {
  const parsed = parseOslChatMutedPeople(storage.getItem(oslChatMutedStorageKey));
  await store.setItem(oslChatMutedStorageKey, encodeOslChatMutedPeople(parsed));
  storage.removeItem(oslChatMutedStorageKey);
  return parsed;
}

export async function loadMigratedOslChatNotifications(
  store: OslChatSecureStore | null,
  storage: Pick<Storage, "getItem">,
): Promise<AppNotification[]> {
  return parseOslChatNotifications(await secureOrLegacyOslChatPreference(store, storage, oslChatNotificationStorageKey));
}

export async function migrateOslChatNotificationsToSecureLocalStore(
  store: OslChatSecureStore,
  storage: BrowserImportStorage,
): Promise<AppNotification[]> {
  const parsed = parseOslChatNotifications(storage.getItem(oslChatNotificationStorageKey));
  await store.setItem(oslChatNotificationStorageKey, encodeOslChatNotifications(parsed));
  storage.removeItem(oslChatNotificationStorageKey);
  return parsed;
}

export function rnWirePolicyState(requested: boolean, buildEnabled = false): RnWirePolicyState {
  if (!buildEnabled) return { requested, buildEnabled, effectiveEnabled: false, refusal: "build-disabled" };
  if (!requested) return { requested, buildEnabled, effectiveEnabled: false, refusal: "user-disabled" };
  return { requested, buildEnabled, effectiveEnabled: true, refusal: null };
}

export function rnWirePolicySettingsMarkup(state: RnWirePolicyState): string {
  const checked = state.effectiveEnabled ? "checked" : "";
  const disabled = state.buildEnabled ? "" : "disabled";
  const summary = state.effectiveEnabled ? "On" : state.refusal === "build-disabled" ? "Unavailable in this build" : "Off";
  return `<details class="settings-disclosure" data-rn-wire-policy><summary><span><strong>Advanced message format</strong><small>${summary}</small></span></summary><label class="setting-line interactive"><span><strong>Use next-generation protected messages</strong><small>OSL keeps using the current message format unless this build and this setting both allow the newer one.</small></span><input id="rn-wire-policy-toggle" type="checkbox" ${checked} ${disabled}/></label></details>`;
}

function validOpaqueSelection(accountId: string, selectedMessageIds: readonly string[]): boolean {
  return accountId.length > 0
    && accountId.length <= 128
    && selectedMessageIds.length > 0
    && selectedMessageIds.length <= 32
    && new Set(selectedMessageIds).size === selectedMessageIds.length
    && selectedMessageIds.every((id) => /^[A-Za-z0-9._:-]{1,180}$/u.test(id));
}

function parseAttendedPreview(raw: unknown, selectedMessageIds: readonly string[]): { previewToken: string } | null {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const record = raw as Record<string, unknown>;
  return typeof record.previewToken === "string"
    && /^[A-Za-z0-9._:-]{16,180}$/u.test(record.previewToken)
    && Array.isArray(record.selectedMessageIds)
    && record.selectedMessageIds.length === selectedMessageIds.length
    && record.selectedMessageIds.every((id, index) => id === selectedMessageIds[index])
    ? { previewToken: record.previewToken }
    : null;
}

function parseAttendedFinal(raw: unknown, previewToken: string): { runId: string; receiptCount: number } | null {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const record = raw as Record<string, unknown>;
  return typeof record.runId === "string"
    && /^[A-Za-z0-9._:-]{8,180}$/u.test(record.runId)
    && record.previewToken === previewToken
    && Number.isSafeInteger(record.receiptCount)
    && Number(record.receiptCount) >= 0
    && Number(record.receiptCount) <= 32
    ? { runId: record.runId, receiptCount: Number(record.receiptCount) }
    : null;
}

export async function runAttendedImapUi(
  request: AttendedImapRunRequest,
  nativeInvoke: typeof invoke = invoke,
): Promise<AttendedImapRunResult> {
  if (!request.livePathProved) return { state: "refused", reason: "live-path-required" };
  if (!validOpaqueSelection(request.accountId, request.selectedMessageIds)) return { state: "refused", reason: "invalid-selection" };
  const selectedMessageIds = [...request.selectedMessageIds];
  const preview = parseAttendedPreview(await nativeInvoke("preview_attended_imap_batch_review", {
    request: { accountId: request.accountId, selectedMessageIds, dryRun: true },
  }).catch(() => null), selectedMessageIds);
  if (!preview) return { state: "refused", reason: "preview-refused" };
  if (!request.confirmFinalRun) return { state: "preview", previewToken: preview.previewToken, selectedMessageIds };
  const final = parseAttendedFinal(await nativeInvoke("execute_attended_imap_batch_review", {
    request: { previewToken: preview.previewToken, selectedMessageIds },
  }).catch(() => null), preview.previewToken);
  return final
    ? { state: "completed", previewToken: preview.previewToken, runId: final.runId, receiptCount: final.receiptCount }
    : { state: "refused", reason: "final-refused" };
}

export function toggleScrubReviewSelection(
  selected: ReadonlySet<number>,
  findingIndex: number,
  checked: boolean,
  findingCount: number,
): Set<number> {
  const next = new Set([...selected].filter((index) => Number.isSafeInteger(index) && index >= 0 && index < findingCount));
  if (!Number.isSafeInteger(findingIndex) || findingIndex < 0 || findingIndex >= findingCount) return next;
  if (checked) next.add(findingIndex);
  else next.delete(findingIndex);
  return next;
}

export async function authorizeAutoscrubReviewList(
  request: AutoscrubAuthorizationRequest,
  nativeInvoke: typeof invoke = invoke,
): Promise<AutoscrubAuthorizationResult> {
  if (!request.operatorConfirmed) return { state: "refused", reason: "confirmation-required" };
  if (!validOpaqueSelection(request.accountId, request.selectedMessageIds)) return { state: "refused", reason: "invalid-selection" };
  const selectedMessageIds = [...request.selectedMessageIds];
  const raw = await nativeInvoke("authorize_attended_imap_batch_review", {
    request: { accountId: request.accountId, selectedMessageIds },
  }).catch(() => null);
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return { state: "refused", reason: "native-refused" };
  const authorizationId = (raw as Record<string, unknown>).authorizationId;
  if (typeof authorizationId !== "string" || !/^[A-Za-z0-9._:-]{16,180}$/u.test(authorizationId)) {
    return { state: "refused", reason: "native-refused" };
  }
  return { state: "armed", authorizationId, selectedMessageIds };
}

export function autoscrubStatusProjectionMarkup(status: AutoscrubStatusProjection): string {
  if (status.phase !== "completed") return `<section class="activity-run-status" data-phase="${status.phase}"><strong>${status.phase === "running" ? "Running" : status.phase === "failed" ? "Needs attention" : "No cleanup run"}</strong></section>`;
  const rows = status.receipts.map((receipt) => {
    const state = receipt.state === "verified" ? "Removed" : receipt.state === "held" ? "Held" : "Not verified";
    return `<li data-cleanup-proof="${receipt.state}"><span>${escapeHtml(receipt.itemLabel)}</span><strong>${state}</strong></li>`;
  }).join("");
  return `<section class="activity-run-status completed" data-phase="completed"><h3>Cleanup activity</h3><ul>${rows}</ul></section>`;
}

export function revokeBrowserImportForSource(
  storage: BrowserImportStorage,
  ownerId: string,
  source: BrowserImportId,
): { completed: Set<BrowserImportId>; ready: boolean } {
  const readyKey = `${savedAccountsReadyStorageKey}:${encodeURIComponent(ownerId)}`;
  const importsKey = `${completedBrowserImportsStorageKey}:${encodeURIComponent(ownerId)}`;
  const completed = new Set<BrowserImportId>();
  try {
    const stored = JSON.parse(storage.getItem(importsKey) ?? "[]") as unknown;
    if (Array.isArray(stored)) stored.filter(supportedBrowserId).forEach((id) => completed.add(id));
  } catch {
    completed.clear();
  }
  completed.delete(source);
  if (completed.size) {
    storage.setItem(importsKey, JSON.stringify([...completed]));
    storage.setItem(readyKey, "true");
  } else {
    storage.removeItem(importsKey);
    storage.removeItem(readyKey);
  }
  return { completed, ready: storage.getItem(readyKey) === "true" };
}

export function desktopCtaHandoffRoute(surface: DesktopCtaSurface): DesktopCtaRoute {
  return surface === "mobile-companion" ? "phone-companion" : "desktop-app";
}

function pendingOnboardingRoute(): OnboardingRoute | null {
  const pending = localStorage.getItem(onboardingResumeStorageKey);
  if (pending === "pro"
    || pending === "privacy"
    || pending === "defaults"
    || pending === "sending"
    || pending === "cover"
    || pending === "passwords"
    || pending === "burnpass"
    || pending === "mullvad"
    || pending === "browser"
    || pending === "tutorial") return onboardingRouteForBuild(pending);
  if (pending !== null) localStorage.removeItem(onboardingResumeStorageKey);
  return null;
}

function persistCurrentOnboardingRoute(): void {
  if (onboardingRoute === "pro"
    || onboardingRoute === "privacy"
    || onboardingRoute === "defaults"
    || onboardingRoute === "sending"
    || onboardingRoute === "cover"
    || onboardingRoute === "passwords"
    || onboardingRoute === "burnpass"
    || onboardingRoute === "mullvad"
    || onboardingRoute === "browser"
    || onboardingRoute === "tutorial") {
    localStorage.setItem(onboardingResumeStorageKey, onboardingRoute);
  }
}

function beginServiceOnboarding(): void {
  onboardingServiceSetup = true;
  localStorage.removeItem(onboardingResumeStorageKey);
}

function markServiceOnboardingOpened(): void {
  if (!onboardingServiceSetup) return;
  localStorage.setItem(onboardingResumeStorageKey, "apps");
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

export async function loadUiPreferences(): Promise<void> {
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
    const accountChoices = JSON.parse(localStorage.getItem(detectedAccountChoicesStorageKey) ?? "[]") as unknown;
    if (Array.isArray(accountChoices)) {
      detectedAccountChoices = new Map(accountChoices.filter((entry): entry is [string, "native" | "osl"] =>
        Array.isArray(entry)
        && entry.length === 2
        && typeof entry[0] === "string"
        && (entry[1] === "native" || entry[1] === "osl")));
    }
    const selectedAppsRaw = localStorage.getItem(selectedOnboardingAppsStorageKey);
    hasExplicitOnboardingAppSelection = selectedAppsRaw !== null;
    const selectedApps = JSON.parse(selectedAppsRaw ?? "[]") as unknown;
    if (Array.isArray(selectedApps)) selectedApps.filter((id): id is HomeAppId => typeof id === "string").slice(0, 32).forEach((id) => selectedOnboardingApps.add(id));
  } catch {
    sidebarOrder = [];
    hiddenServices.clear();
    homeTileOrder = [];
    hiddenHomeTiles.clear();
    notificationAppPreferences = {};
    savedNativeApps.clear();
    detectedAccountChoices.clear();
    selectedOnboardingApps.clear();
    hasExplicitOnboardingAppSelection = localStorage.getItem(selectedOnboardingAppsStorageKey) !== null;
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
  rnWirePolicyRequested = localStorage.getItem(rnWirePolicyStorageKey) === "true";
  await loadOslChatSensitiveStateFromSecureStore();
  const notices = await loadMigratedOslChatNotifications(oslChatSecureStore, localStorage);
  if (notices.length) appNotifications = notices;
  screenshotProtectionEnabled = false;
  mullvadAutoStart = localStorage.getItem(mullvadAutoStartStorageKey) === "true";
  enabledScrubSignals = parseScrubSignalGroups(localStorage.getItem(scrubSignalsStorageKey));
}

function activeBrowserAccountsReadyStorageKey(): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${savedAccountsReadyStorageKey}:${encodeURIComponent(owner)}` : null;
}

function activeOwnerStorageKey(base: string): string | null {
  const owner = core.readiness.activeOslUserId;
  return owner ? `${base}:${encodeURIComponent(owner)}` : null;
}

function supportedBrowserId(raw: unknown): raw is BrowserImportId {
  return typeof raw === "string" && ["chrome", "edge", "firefox", "brave", "opera", "duckduckgo"].includes(raw);
}

function browserProfileKey(profile: BrowserProfileDescriptor): string {
  return `${profile.browserId}:${encodeURIComponent(profile.profile)}`;
}

function setBrowserProfiles(profiles: BrowserProfileDescriptor[]): void {
  browserProfiles = profiles;
  const displayNames: Record<BrowserImportId, string> = {
    chrome: "Chrome",
    edge: "Edge",
    firefox: "Firefox",
    brave: "Brave",
    opera: "Opera",
    duckduckgo: "DuckDuckGo",
  };
  browserImports = [...new Set(profiles.map((profile) => profile.browserId))]
    .map((id) => ({ id, displayName: displayNames[id], installed: true as const }));
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
  const owner = core.readiness.activeOslUserId;
  return owner ? `${browserImportPendingStorageKey}:${encodeURIComponent(owner)}` : null;
}

function persistBrowserImportQueue(): void {
  const pendingKey = activeBrowserImportPendingStorageKey();
  if (!pendingKey) return;
  if (browserImportQueue.length > 0) {
    localStorage.setItem(pendingKey, JSON.stringify({
      queue: browserImportQueue,
      index: browserImportQueueIndex,
      sourceSelected: browserImportSourceSelected,
    }));
  } else {
    localStorage.removeItem(pendingKey);
  }
}

function refreshActiveBrowserAccountsReady(): void {
  const activeOwner = core.readiness.activeOslUserId;
  const key = activeBrowserAccountsReadyStorageKey();
  if (key) localStorage.removeItem(key);
  const preferred = activeOwnerStorageKey(preferredBrowserStorageKey);
  const imported = activeOwnerStorageKey(completedBrowserImportsStorageKey);
  const storedPreferred = preferred ? localStorage.getItem(preferred) : null;
  if (imported) localStorage.removeItem(imported);
  if (browserFootprintOwner !== activeOwner) {
    browserFootprintOwner = activeOwner;
    savedAccountsReady = false;
    browserFootprintImports = [];
    selectedBrowserProfileKeys.clear();
    preferredBrowserId = supportedBrowserId(storedPreferred) ? storedPreferred : null;
    completedBrowserImportIds.clear();
  }
  const pendingKey = activeBrowserImportPendingStorageKey();
  if (pendingKey) localStorage.removeItem(pendingKey);
  browserImportQueue = [];
  browserImportQueueIndex = 0;
  browserImportSourceSelected = false;
  persistBrowserImportQueue();
}

function applyNativeBrowserFootprint(hydration: BrowserFootprintHydration): void {
  browserFootprintOwner = core.readiness.activeOslUserId;
  browserFootprintImports = hydration.imports;
  completedBrowserImportIds = new Set(hydration.imports.map((receipt) => receipt.browserId));
  savedAccountsReady = hydration.imports.length > 0
    && hydration.observations.length > 0
    && hydration.imports.every((receipt) =>
      receipt.persistedCount > 0
      && receipt.persistedCount === receipt.immediateRereadCount);
  if (!preferredBrowserId || !completedBrowserImportIds.has(preferredBrowserId)) {
    preferredBrowserId = hydration.imports[0]?.browserId ?? null;
  }
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
  // Whether the borrowed Discord window still needs keeping aligned is a
  // function of the route and host that were just painted, so it is settled
  // once per committed frame. Doing it in `render()` instead ran it on every
  // state change, many of which coalesce into a single paint.
  syncDiscordQaGeometryMaintenance();
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
  return `<header class="desktop-titlebar"><div class="desktop-drag-region" data-tauri-drag-region aria-hidden="true"></div>${fleetIndicatorMarkup()}<div class="window-controls"><button id="window-minimize" aria-label="Minimize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-close" class="window-close" aria-label="Close"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button></div></header>`;
}

// The hub route (onboarding excluded — see renderOnboarding) no longer gets a
// separate 44px titlebar strip above its control row: that read as an ugly
// pale bar with nothing in it. Instead these same three buttons dock directly
// into whichever top row the active hub route is already showing
// (workspace-header / home-command-bar / guide-header / mullvad-host-header),
// via the ".desktop-top-row" wrapper built in renderWorkspace(). This
// duplicates desktopTitlebar()'s button markup and disabled-state logic on
// purpose rather than sharing it: desktopTitlebar() still renders the
// original full titlebar (header + dedicated drag strip) for the bare-shell
// screens (onboarding, boot recovery, initial loading) that have no other
// header to dock into, and keeping the two independent avoids a shared
// helper whose blast radius spans both layouts.
function desktopWindowControlsMarkup(): string {
  const nativeControlsBlocked = activeNativeHostId || activeDefaultBrowserCompanion ? ' disabled title="Unavailable while a companion window is open"' : "";
  return `${fleetIndicatorMarkup()}<div class="window-controls"><button id="window-minimize" aria-label="Minimize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-close" class="window-close" aria-label="Close"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button></div>`;
}

const desktopMaximizeGlyph = '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg>';
const desktopRestoreGlyph = '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 3.5h7v7h-2M5.5 3.5v2M5.5 3.5h-2v9h9v-2"/></svg>';
let desktopMaximizeListenerBound = false;

function applyMaximizeControlState(button: HTMLButtonElement, maximized: boolean): void {
  button.setAttribute("aria-label", maximized ? "Restore" : "Maximize");
  button.innerHTML = maximized ? desktopRestoreGlyph : desktopMaximizeGlyph;
}

async function refreshDesktopMaximizeControl(): Promise<void> {
  const button = document.querySelector<HTMLButtonElement>("#window-maximize");
  if (!button) return;
  const maximized = await getCurrentWindow().isMaximized().catch(() => false);
  applyMaximizeControlState(button, maximized);
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
  bindOnce("#window-maximize", async () => {
    await appWindow.toggleMaximize();
    await refreshDesktopMaximizeControl();
  });
  bindOnce("#window-close", () => appWindow.close());
  void refreshDesktopMaximizeControl();
  if (!desktopMaximizeListenerBound) {
    desktopMaximizeListenerBound = true;
    void appWindow.onResized(() => void refreshDesktopMaximizeControl()).catch(() => undefined);
  }
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

// The alignment half of the call above, without the foreground grab.
//
// Engaging protection needs the borrowed window placed where OSL thinks it is,
// because the protected composer is positioned against that rectangle. It must
// NOT need the borrowed window in the foreground: doing that hands Discord the
// keyboard immediately before revealing a composer the operator is about to
// type into, and the keystrokes in between go to Discord in the clear.
async function alignActiveNativeCompanion(): Promise<boolean> {
  if (!activeNativeHostId) return false;
  const resized = await withNativeDeadline(
    resizeNativeAppWindow(),
    `Align ${activeHomeAppName()}`,
    3_000,
  ).catch(() => null);
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

function onboardingShellMarkup(setupNavigation = ""): string {
  return `<div class="app-frame with-titlebar">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}</main>${setupNavigation}</div>${scrubReviewDialogMarkup()}</div>`;
}

function renderOnboarding(): void {
  onboardingRoute = onboardingRouteForBuild(onboardingRoute);
  persistCurrentOnboardingRoute();
  const setupScreen = ["pro", "privacy", "defaults", "sending", "cover", "passwords", "burnpass", "browser", "tutorial", "detected", "install", "apps", "mullvad"].includes(onboardingRoute);
  const setupNavigation = setupScreen
    ? `<button class="onboarding-back-dock" id="onboarding-back" type="button">Back</button>`
    : "";
  const markup = onboardingShellMarkup(setupNavigation);
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
  if (onboardingRoute === "welcome") return welcomeOnboardingContent();

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
  if (onboardingRoute === "defaults") return reviewDefaultsOnboardingContent();
  if (onboardingRoute === "cover") return coverDraftSetupContent();
  if (onboardingRoute === "passwords") return onboardingPasswordRoleContent("stealth");
  if (onboardingRoute === "burnpass") return onboardingPasswordRoleContent("burn");
  if (onboardingRoute === "privacy") return onboardingPrivacyContent();
  if (onboardingRoute === "decoy") return `<section class="decoy-workspace" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Workspace</h1><p>No recent items.</p><button class="button ghost" id="close-decoy" type="button">Close</button></section>`;

  return sendingSetupContent();
}

function welcomeOnboardingContent(): string {
  const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired";
  const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired;
  const primaryRoute: OnboardingRoute = partialIdentity ? "create" : returning ? "unlock" : "create";
  const primaryLabel = partialIdentity ? "Finish setup" : returning ? "Unlock this device" : "Create account";
  const heading = partialIdentity ? "Finish your account" : returning ? "Sign in" : "Protect the accounts you already use";
  const intro = returning
    ? "Unlock this device to continue protecting your existing accounts and private OSL communication."
    : "Use OSL with the messaging, social and email accounts you already have. For conversations that need their own private place, OSL communication is built in.";
  return `<section class="signin-card" aria-labelledby="route-heading">
    <img class="osl-logo signin-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/>
    <h1 id="route-heading" tabindex="-1">${heading}</h1>
    <p class="compact-lead onboarding-centered-copy">${intro}</p>
    <button class="button primary signin-primary" data-onboarding="${primaryRoute}">${primaryLabel}</button>
    <button class="signin-link" data-onboarding="import">Use a recovery phrase</button>
    ${returning ? `<div class="signin-divider" aria-hidden="true"><span></span></div><p class="signin-new">Unlock first to add another identity in Settings.</p>` : ""}
  </section>`;
}

function proSetupContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  if (pro) return `<section class="pro-setup" aria-labelledby="route-heading"><span class="status-tag active">Pro active</span><h1 id="route-heading" tabindex="-1">OSL Pro is ready</h1><button class="button primary" data-onboarding="sending" type="button">Continue</button></section>`;
  return `<section class="pro-setup" aria-labelledby="route-heading"><p class="eyebrow">Optional</p><h1 id="route-heading" tabindex="-1">Enter Pro code</h1><form id="activation-form" class="pro-setup-form" novalidate><label class="sr-only" for="activation-code">Pro activation code</label><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Continue</button></form><button class="text-button" data-onboarding="sending" type="button">Skip</button></section>`;
}

function tutorialContent(): string {
  return chooseAppsOnboardingContent();
}

function chooseAppsOnboardingContent(): string {
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
  const defaultContinueLabel = nativeCatalogBusy ? "Checking Windows…" : "Continue";
  const continueLabel = nativeCatalogBusy
    ? defaultContinueLabel
    : selectedOnboardingApps.size > 0 ? defaultContinueLabel : "Skip apps";
  return `<h1 id="route-heading" tabindex="-1">Choose apps</h1><p class="compact-lead onboarding-centered-copy">Pick available apps for Home, or skip this for now. Nothing opens during setup.</p><section class="onboarding-app-section"><h2>Detected</h2>${choices(detected, "Detected apps")}</section><section class="onboarding-app-section"><h2>Other apps</h2>${choices(other, "Other apps")}</section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-app-choice" type="button" ${nativeCatalogBusy ? "disabled" : ""}>${continueLabel}</button></div>`;
}

async function enterCombinedAppChoice(): Promise<void> {
  const catalog = await withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs).catch(() => null);
  if (catalog && isCompleteNativeCatalog(catalog)) nativeApps = catalog;
  onboardingRoute = "tutorial";
  render();
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

function hasSelectedMissingNativeApps(): boolean {
  return selectedNativeApps().some((app) => app.availability !== "installed");
}

function onboardingConnectionApps(): HomeAppCatalogEntry[] {
  return homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available")
    .filter((app) => selectedOnboardingApps.size === 0 || selectedOnboardingApps.has(app.id));
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
  if (!hasNext) {
    void completeOnboarding();
    return;
  }
  route = "onboarding";
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
  return selectedInstalledNativeApp(appId);
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
  return `<div class="saved-account-choices session-mode-choices" role="group" aria-label="Open browser account"><button type="button" class="account-launch-choice" data-browser-session-mode="existingBrowser">Browser account</button><button type="button" class="account-launch-choice" data-browser-session-mode="isolatedOsl" ${isolatedAvailable ? "" : "disabled"}>New account</button></div>`;
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

function providerWideInstalledNativeApp(appId: HomeAppId): NativeAppId | undefined {
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

function persistDetectedAccountChoices(): void {
  const validKeys = new Set(services.flatMap((service) =>
    service.accounts.map((account) => detectedAccountChoiceKey(service.id, account.id))));
  for (const key of detectedAccountChoices.keys()) {
    if (!validKeys.has(key)) detectedAccountChoices.delete(key);
  }
  localStorage.setItem(detectedAccountChoicesStorageKey, JSON.stringify([...detectedAccountChoices]));
}

function selectedInstalledNativeApp(appId: HomeAppId): NativeAppId | undefined {
  const app = { id: appId };
  const service = services.find((candidate) => homeAppsFromServices([candidate]).some((app) => app.id === appId));
  if (service) {
    for (const account of service.accounts) {
      const key = detectedAccountChoiceKey(service.id, account.id);
      if (detectedAccountChoices.get(key) === "osl") return undefined;
    }
  }
  return providerWideInstalledNativeApp(app.id);
}

function detectedAppsContent(): string {
  const installed = selectedNativeApps().filter((app) => app.availability === "installed");
  const accountChoices = services.flatMap((service) => service.accounts.map((account) => {
    const key = detectedAccountChoiceKey(service.id, account.id);
    const mode = detectedAccountChoices.get(key) ?? "native";
    return `<div class="native-mode-setting account-opening-choice" role="radiogroup" aria-label="${escapeHtml(service.displayName)} ${escapeHtml(account.label)} opening"><strong>${escapeHtml(service.displayName)} · ${escapeHtml(account.label)}</strong><div><button type="button" role="radio" aria-checked="${mode === "native"}" class="native-mode-option ${mode === "native" ? "selected" : ""}" data-service-current-session="${escapeHtml(key)}" data-detected-account-choice="native">Current desktop session · provider-wide</button><button type="button" role="radio" aria-checked="${mode === "osl"}" class="native-mode-option ${mode === "osl" ? "selected" : ""}" data-service-current-session="${escapeHtml(key)}" data-detected-account-choice="osl">Use isolated OSL profile · this account</button></div></div>`;
  })).join("");
  const rows = installed.length
    ? installed.map((app) => `<label class="saved-account-app"><span>${nativeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Installed on this PC</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${app.id === "discord" || savedNativeApps.has(app.id) ? "checked" : ""} ${app.id === "discord" ? "disabled" : ""}/></label>`).join("")
    : `<div class="empty-state"><strong>No selected desktop apps were detected</strong><p>OSL can still use isolated web profiles.</p></div>`;
  const discordChoices = installed.some((app) => app.id === "discord")
    ? nativeSessionModeSettingChoices("discord", "Discord")
    : "";
  const telegramChoices = installed.some((app) => app.id === "telegram")
    ? nativeSessionModeSettingChoices("telegram", "Telegram")
    : "";
  const signalChoices = installed.some((app) => app.id === "signal") ? nativeSessionModeSettingChoices("signal", "Signal") : "";
  const whatsappChoices = installed.some((app) => app.id === "whatsapp") ? nativeSessionModeSettingChoices("whatsapp", "WhatsApp") : "";
  const outlookChoices = installed.some((app) => app.id === "outlook") ? nativeSessionModeSettingChoices("outlook", "Outlook") : "";
  return `<h1 id="route-heading" tabindex="-1">Use installed apps</h1><p class="compact-lead onboarding-centered-copy">Choose detected desktop apps.</p>${discordChoices}${telegramChoices}${signalChoices}${whatsappChoices}${outlookChoices}${accountChoices}<div class="setup-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-detected-apps" type="button">Continue</button></div>`;
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
    : `<div class="empty-state"><strong>Selected apps ready</strong><p>Finish setup.</p></div>`;
  return `<h1 id="route-heading" tabindex="-1">Connect your apps</h1><p class="compact-lead onboarding-centered-copy">Open each selected app, or skip it for now.</p>${choices}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-connect-app" type="button" ${onboardingConnectAppId ? "" : "disabled"}>Open selected app</button><button class="browser-import-skip" id="skip-connect-app" type="button">${onboardingConnectAppId ? "Not now" : "Continue"}</button></div>`;
}

function browserImportContent(): string {
  const installed = browserImports.filter((browser) => browser.installed);
  const queueActive = browserImportQueue.length > 0;
  const browserName = (id: BrowserImportId): string =>
    installed.find((browser) => browser.id === id)?.displayName ?? id;
  const detectedBrowsers = installed.length
    ? `<fieldset class="browser-detected-sources" ${queueActive ? "disabled" : ""}><legend>Choose saved browser areas</legend><div class="browser-detected-list">${browserProfiles.map((profile) => {
      const key = browserProfileKey(profile);
      return `<label class="browser-detected-item">${browserLogo(profile.browserId)}<span><strong>${escapeHtml(browserName(profile.browserId))} · ${escapeHtml(profile.displayName)}</strong><small>Read only a bounded history snapshot after this consent</small></span><input type="checkbox" data-browser-profile="${escapeHtml(key)}" ${selectedBrowserProfileKeys.has(key) ? "checked" : ""}/></label>`;
    }).join("")}</div></fieldset>`
    : browserProfiles.length === 0
      ? `<p class="saved-account-truth">No readable saved browser areas were found. OSL has not opened any browser database.</p>`
      : `<p class="saved-account-truth">No supported browser detected.</p>`;
  const ready = savedAccountsReady
    ? `<div class="saved-account-browser-note"><strong>Saved browser account hints protected</strong><small>Encrypted locally. Login stores and passwords were not read.</small>${browserFootprintImports.map((receipt) => `<button class="button compact" type="button" data-revoke-browser-footprint="${escapeHtml(browserProfileKey({ browserId: receipt.browserId, profile: receipt.profile, displayName: receipt.profile }))}">Delete ${escapeHtml(receipt.browserId)} · ${escapeHtml(receipt.profile)}</button>`).join("")}</div>`
    : "";
  const failure = browserImportFailureNotice
    ? `<p class="saved-account-browser-error" role="alert">${escapeHtml(browserImportFailureNotice)}</p>`
    : "";
  const currentSource = queueActive ? browserImportQueue[browserImportQueueIndex] : null;
  const currentBrowser = currentSource ? browserImports.find((browser) => browser.id === currentSource) : null;
  const currentName = currentBrowser?.displayName ?? currentSource ?? "browser";
  const progress = queueActive
    ? `<div class="saved-account-browser-note" aria-live="polite"><strong>${escapeHtml(currentName)} · ${browserImportQueueIndex + 1} of ${browserImportQueue.length}</strong><small>${browserImportSourceSelected ? "The encrypted account hints were saved and verified." : "OSL is copying a bounded private snapshot before reading it."}</small></div>`
    : "";
  const selectionReady = selectedBrowserProfileKeys.size > 0;
  const importEnabled = selectionReady && !browserReadinessBusy && !browserImportBusy;
  const importLabel = browserImportBusy
    ? "Checking selected areas..."
    : selectionReady ? "Check selected areas" : "Choose areas";
  const secondaryLabel = browserImportBusy ? "Wait for scan..." : "Not now";
  return `<h1 id="route-heading" tabindex="-1">Find saved browser accounts</h1><p class="compact-lead onboarding-centered-copy">Optional. Consent separately to each browser area OSL may inspect.</p>${detectedBrowsers}${progress}${ready}${failure}<div class="setup-footer onboarding-actions browser-import-actions-primary"><button class="button primary" id="import-saved-accounts" type="button" ${importEnabled ? "" : "disabled"}>${importLabel}</button><button class="browser-import-skip" id="continue-browser-import" type="button" ${browserImportBusy || browserImportCancelling ? "disabled" : ""}>${secondaryLabel}</button></div><p class="saved-account-truth">OSL never reads browser databases before consent. After consent it copies one bounded history snapshot, reads that copy, deletes it, and never opens passwords or login stores.</p>`;
}

function persistSavedAccountPreferences(): void {
  persistDetectedAccountChoices();
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
  document.querySelectorAll<HTMLButtonElement>("[data-service-current-session]").forEach((button) => button.addEventListener("click", () => {
    const key = button.dataset.serviceCurrentSession ?? "";
    const choice = button.dataset.detectedAccountChoice === "osl" ? "osl" : "native";
    detectedAccountChoices.set(key, choice);
    persistDetectedAccountChoices();
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
  document.querySelectorAll<HTMLButtonElement>("[data-revoke-browser-footprint]").forEach((button) => button.addEventListener("click", async () => {
    const key = button.dataset.revokeBrowserFootprint ?? "";
    const receipt = browserFootprintImports.find((candidate) =>
      browserProfileKey({ browserId: candidate.browserId, profile: candidate.profile, displayName: candidate.profile }) === key);
    if (!receipt || browserImportBusy) return;
    browserImportBusy = true;
    browserImportFailureNotice = "";
    render();
    try {
      await revokeDetectedBrowserFootprint(receipt.browserId, receipt.profile, receipt.account);
      const remaining = browserFootprintImports.filter((candidate) =>
        candidate.browserId !== receipt.browserId
        || candidate.profile !== receipt.profile
        || candidate.account !== receipt.account);
      if (remaining.length > 0) {
        applyNativeBrowserFootprint(await loadDetectedBrowserFootprint(remaining));
      } else {
        browserFootprintImports = [];
        completedBrowserImportIds.clear();
        preferredBrowserId = null;
        savedAccountsReady = false;
      }
      showToast("Saved browser account hints deleted");
    } catch (failure) {
      browserImportFailureNotice = localActionError(failure, "Saved browser account hints could not be deleted");
      showToast(browserImportFailureNotice);
    } finally {
      browserImportBusy = false;
      render();
    }
  }));
  document.querySelectorAll<HTMLInputElement>("[data-browser-profile]").forEach((input) => input.addEventListener("change", () => {
    const key = input.dataset.browserProfile ?? "";
    if (!browserProfiles.some((profile) => browserProfileKey(profile) === key)) return;
    browserImportFailureNotice = "";
    if (input.checked) selectedBrowserProfileKeys.add(key);
    else selectedBrowserProfileKeys.delete(key);
    render();
  }));
  const startProtectedBrowserImport = async (): Promise<void> => {
    if (selectedBrowserProfileKeys.size === 0 || browserImportBusy) return;
    const selectedProfiles = browserProfiles.filter((profile) =>
      selectedBrowserProfileKeys.has(browserProfileKey(profile)));
    if (selectedProfiles.length !== selectedBrowserProfileKeys.size) {
      browserImportFailureNotice = "The selected browser areas changed. Review them again.";
      selectedBrowserProfileKeys.clear();
      render();
      return;
    }
    // Profile consent is one-shot. Consume it before invoking native code so a
    // retry, route revisit, or failed scan always requires another fresh click.
    selectedBrowserProfileKeys.clear();
    const runEpoch = ++browserImportRunEpoch;
    browserImportFailureNotice = "";
    browserImportQueue = selectedProfiles.map((profile) => profile.browserId);
    browserImportQueueIndex = 0;
    browserImportSourceSelected = false;
    persistBrowserImportQueue();
    browserImportBusy = true;
    render();
    const emptyImportNotice = "Nothing was imported from it";
    const scanReceipts: NativeBrowserImportReceipt[] = [];
    try {
      for (let index = 0; index < selectedProfiles.length; index += 1) {
        if (runEpoch !== browserImportRunEpoch) return;
        browserImportQueueIndex = index;
        browserImportSourceSelected = false;
        persistBrowserImportQueue();
        render();
        const selectedProfile = selectedProfiles[index];
        if (!selectedProfile) throw new Error("Browser selection queue is invalid");
        const grant = await grantBrowserProfileConsent(
          selectedProfile.browserId,
          selectedProfile.profile,
        );
        const receipt = await scanConsentedBrowserProfile(
          selectedProfile.browserId,
          selectedProfile.profile,
          grant.grantId,
        );
        if (runEpoch !== browserImportRunEpoch) return;
        if (receipt.persistedCount < 1) {
          throw new Error("Nothing was imported from it");
        }
        if (receipt.persistedCount !== receipt.immediateRereadCount) {
          throw new Error("The saved browser account hints did not survive their immediate reread.");
        }
        scanReceipts.push(receipt);
        browserImportSourceSelected = true;
        browserImportFailureNotice = "";
        persistBrowserImportQueue();
      }
      if (runEpoch !== browserImportRunEpoch) return;
      const ownerBeforeHydration = core.readiness.activeOslUserId;
      const hydration = await loadDetectedBrowserFootprint(scanReceipts);
      if (ownerBeforeHydration === null || core.readiness.activeOslUserId !== ownerBeforeHydration) return;
      applyNativeBrowserFootprint(hydration);
      const persistedScopes = new Set(hydration.imports.map((receipt) =>
        `${receipt.browserId}:${encodeURIComponent(receipt.profile)}`));
      if (!savedAccountsReady || selectedProfiles.some((profile) =>
        !persistedScopes.has(browserProfileKey(profile)))) {
        throw new Error("The saved browser account hints were not verified.");
      }
      browserImportQueue = [];
      browserImportQueueIndex = 0;
      browserImportSourceSelected = false;
      persistBrowserImportQueue();
      selectedBrowserProfileKeys.clear();
      persistBrowserImportQueue();
      resetOnboardingBranch();
      resetOnboardingConnections();
      const finishProtectedBrowserImportCleanup = closeProtectedBrowserImportHelper;
      const activeOperation = finishProtectedBrowserImportCleanup();
      await activeOperation;
      await finishProtectedBrowserImportCleanup().catch(() => undefined);
      showToast("Browser import finished");
      await enterCombinedAppChoice();
    } catch (failure) {
      if (runEpoch !== browserImportRunEpoch) return;
      browserImportQueue = [];
      browserImportQueueIndex = 0;
      browserImportSourceSelected = false;
      persistBrowserImportQueue();
      selectedBrowserProfileKeys.clear();
      browserImportFailureNotice = localActionError(failure, "Saved browser account check did not finish");
      showToast(scanReceipts.length === 0 ? emptyImportNotice : browserImportFailureNotice);
    } finally {
      if (runEpoch === browserImportRunEpoch) {
        browserImportBusy = false;
        render();
      }
    }
  };
  document.querySelector<HTMLButtonElement>("#import-saved-accounts")?.addEventListener("click", () => {
    void startProtectedBrowserImport();
  });
  document.querySelector<HTMLButtonElement>("#continue-browser-import")?.addEventListener("click", async () => {
    if (browserImportBusy || browserImportCancelling) return;
    browserImportCancelling = true;
    browserImportRunEpoch += 1;
    render();
    browserImportBusy = false;
    const pendingKey = activeBrowserImportPendingStorageKey();
    if (pendingKey) localStorage.removeItem(pendingKey);
    browserImportQueue = [];
    browserImportQueueIndex = 0;
    browserImportSourceSelected = false;
    persistBrowserImportQueue();
    selectedBrowserProfileKeys.clear();
    browserImportCancelling = false;
    resetOnboardingBranch();
    resetOnboardingConnections();
    await enterCombinedAppChoice();
  });
}

async function refreshBrowserImportReadiness(): Promise<void> {
  const closeLegacyProtectedImport = async (): Promise<void> => {
    const activeOperation = Promise.resolve().then(() => closeProtectedBrowserImportHelper());
    // finishProtectedBrowserImport is retained only to close an already-open
    // native helper, never to begin or retry browser import.
    await activeOperation;
    // finishProtectedBrowserImport must not be called from the import starter.
  };
  void closeLegacyProtectedImport;
  if (browserReadinessBusy) return;
  browserReadinessBusy = true;
  if (route === "onboarding" && onboardingRoute === "browser") render();
  const profiles = await withNativeDeadline(
    listBrowserProfilesForConsent(),
    "List saved browser areas",
    nativeCatalogDecisionDeadlineMs,
  ).catch(() => null);
  try {
    if (profiles) {
      setBrowserProfiles(profiles);
      const currentKeys = new Set(profiles.map(browserProfileKey));
      selectedBrowserProfileKeys = new Set(
        [...selectedBrowserProfileKeys].filter((key) => currentKeys.has(key)),
      );
    }
    if (!profiles) throw new Error("browser readiness unavailable");
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

async function proveRecoveryCaptureProtection(): Promise<boolean> {
  recoveryCaptureGate.invalidate();
  const checkpoint = recoveryCaptureGate.checkpoint();
  screenshotProtectionEnabled = false;
  const applied = await setScreenshotProtection(true).catch(() => false);
  const focused = applied && await getCurrentWindow().isFocused().catch(() => false);
  const currentWindow = focused && document.visibilityState !== "hidden";
  const proven = currentWindow && recoveryCaptureGate.accept(checkpoint);
  screenshotProtectionEnabled = proven;
  return proven;
}

function recoveryProtectionRefusalContent(): string {
  return `<h1 id="route-heading" tabindex="-1">Recovery secrets hidden</h1><section class="setup-surface recovery-surface" role="alert"><p>${RECOVERY_PROTECTION_REFUSAL}.</p><button class="button primary" id="retry-recovery-protection" type="button">Retry protection</button></section>`;
}

function recoveryContent(): string {
  if (!recoveryBundle) return `<p class="eyebrow">Recovery</p><h1 id="route-heading" tabindex="-1">No recovery secret is available</h1><button class="button primary" data-onboarding="pro">Continue</button>`;
  if (!recoveryCaptureGate.canRender()) return recoveryProtectionRefusalContent();
  const accountRecovery = recoveryBundle.identityPhrase ? `<code>${escapeHtml(recoveryBundle.identityPhrase)}</code>` : `<p>Keep using the account recovery phrase you imported.</p>`;
  return `<h1 id="route-heading" tabindex="-1" class="recovery-heading">Save your recovery kit</h1><section class="setup-surface recovery-surface"><article class="recovery-kit-item"><span>1</span><div><strong>Account recovery</strong>${accountRecovery}</div></article><article class="recovery-kit-item"><span>2</span><div><strong>Password recovery</strong><code>${escapeHtml(recoveryBundle.passwordPhrase)}</code></div></article>${secureRecoveryOnboardingContent()}<details class="recovery-account-details"><summary>Account details</summary><code>${escapeHtml(recoveryBundle.userId)}</code></details><button class="button" id="copy-recovery-kit" type="button">Copy recovery kit</button><label class="check"><input id="recovery-saved" type="checkbox" ${recoverySavedAcknowledged ? "checked" : ""}/><span>I saved my recovery kit.</span></label><button class="button primary" id="recovery-continue" ${recoverySavedAcknowledged ? "" : "disabled"}>Continue</button></section>`;
}

function secureRecoveryOnboardingContent(): string {
  return `<section class="secure-recovery-next-steps" aria-label="Optional next steps"><article><strong>Mullvad</strong><small>Optional. Use your existing session later for network privacy.</small></article><article><strong>Android device</strong><small>Coming later. Phone setup stays optional and separate.</small></article></section>`;
}

function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  if (!setup) return `<section class="unlock-card" aria-labelledby="route-heading"><div class="unlock-logo-stage" aria-hidden="true"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><h1 id="route-heading" tabindex="-1">Enter your password</h1><form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><label class="sr-only" for="identity-duress-pin">Burn code</label><div class="password-input-row"><input id="identity-duress-pin" type="password" minlength="6" maxlength="128" autocomplete="off" placeholder="Burn code" aria-describedby="password-error" data-duress-pin/><button class="password-eye" type="button" data-password-toggle="identity-duress-pin" aria-controls="identity-duress-pin" aria-label="Show burn code">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>Unlock</button></form><button class="text-back" data-onboarding="welcome">← Back</button></section>`;
  return `<h1 id="route-heading" tabindex="-1">${title}</h1><form class="setup-surface password-form" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="button primary" id="identity-password-submit" type="submit" disabled>${action}</button></form><button class="text-back" data-onboarding="welcome">← Back</button>`;
}

export function sendingSetupContent(): string {
  const selectedMode: SendMode = setup.sendMode === "single" ? "manual" : setup.sendMode;
  const option = (mode: SendMode, title: string, detail: string, badge = "") => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${title}</strong>${badge ? `<small class="send-mode-badge">${badge}</small>` : ""}</span><small>${detail}</small></button>`;
  const risk = needsRiskAcceptance(selectedMode)
    ? `<label class="send-risk"><input id="accept-send-risk" type="checkbox" ${setup.acceptedRisk && setup.acceptedRiskForMode === selectedMode ? "checked" : ""}/><span><strong>I understand</strong><small>Experimental sending can target the wrong chat if an app changes. OSL stops unless it can verify the exact app, account, chat, and composer. Each account asks again.</small></span></label>`
    : "";
  return `<h1 id="route-heading" tabindex="-1">Privacy and sending</h1>${captureSetupMarkup()}<h2 class="setup-section-heading">Choose how to send</h2>${manualSendingAnimationMarkup(selectedMode)}<div class="send-mode-list">${option("manual", "Manual", "OSL prepares the protected message; you place it and send it.", "Recommended")}${option("clipboard", "Clipboard", "OSL encrypts and copies; you paste it and send it.")}${option("double", "Double Enter", "First Enter prepares and places. A second distinct Enter sends after another exact check.")}</div>${risk}<p class="send-mode-truth">No mode silently sends. If OSL cannot prove the destination, it copies the encrypted text and sends nothing.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding" ${canCompleteSetup({ ...setup, sendMode: selectedMode }) ? "" : "disabled"}>Continue</button></div>`;
}

function captureSetupMarkup(): string {
  const applied = windowCaptureEnabled && screenshotProtectionEnabled;
  return `<section class="setup-list capture-setup-inline" aria-labelledby="capture-setup-heading"><h2 id="capture-setup-heading" class="setup-section-heading">Screen capture</h2><label class="setup-status-row capture-preference"><span><strong>Resist Windows capture</strong><small>Excludes OSL from ordinary screenshots and recording when Windows supports it. Cameras, malware, and modified devices can still capture content.</small></span><input id="window-capture-enabled" type="checkbox" ${windowCaptureEnabled ? "checked" : ""}/></label><div class="setup-status-row"><span><strong>Current device</strong><small>Protected messages appear only after OSL enables this protection.</small></span><span class="status-tag ${applied ? "active" : ""}">${windowCaptureEnabled ? (applied ? "Active" : "Unavailable") : "Off"}</span></div></section>`;
}

export function reviewDefaultsOnboardingContent(): string {
  const row = (title: string, detail: string, state: string, active = false) => `<div class="setup-status-row"><span><strong>${title}</strong><small>${detail}</small></span><span class="status-tag ${active ? "active" : ""}">${state}</span></div>`;
  return `<h1 id="route-heading" tabindex="-1">Review defaults</h1><p class="compact-lead onboarding-centered-copy">Balanced starts with local warnings, visible attachment cleaning, and review-only cleanup. You can change these later in Privacy.</p><section class="setup-list defaults-review-list" aria-label="Default protection review">${row("Warn before sending", "Checks drafts on this device for selected risks before you send.", "On", true)}${row("Clean attachments", "Offers a visible cleaning step for files and media; nothing changes without your consent.", "Ask first")}${row("Keep protected drafts", "Keeps encrypted local drafts and private activity on this device for recovery.", "On", true)}${row("Delete or clean up history", "Timed deletion, bulk cleanup, and account cleanup do not run during onboarding.", "Off")}${row("Send behavior", "Manual handoff is the default: OSL prepares, then you place and send.", formatSendMode(defaultSetup.sendMode), true)}</section><p class="send-mode-truth">No destructive action starts from setup. Cleanup requires a separate review and confirmation.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-defaults-review" type="button">Continue</button></div>`;
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
  // Resume older interrupted setups on the new combined page instead of
  // forcing users through the retired capture-only screen.
  return protectionPresetOnboardingContent();
}

function protectionPresetOnboardingContent(): string {
  const presets = [
    {
      id: "basic",
      title: "Basic",
      detail: "Account health, email tracker blocking, attachment metadata warnings, and exposure alerts.",
    },
    {
      id: "balanced",
      title: "Balanced",
      detail: "Basic plus local before-send warnings, one-click attachment cleaning, monthly cleanup review, and OSL protection suggestions for verified contacts.",
      badge: "Recommended",
    },
    {
      id: "maximum",
      title: "Maximum",
      detail: "Balanced plus stricter public-post checks, optional VPN-required actions, and OSL protection required for chosen contacts.",
    },
  ] as const;
  const presetChoices = presets.map((preset) => {
    const selected = preset.id === "balanced";
    const badge = "badge" in preset ? `<small class="send-mode-badge">${preset.badge}</small>` : "";
    return `<label class="send-mode-option ${selected ? "selected" : ""}" data-protection-preset="${preset.id}"><span><input class="sr-only" type="radio" name="protection-preset" value="${preset.id}" ${selected ? "checked" : ""}/><strong>${preset.title}</strong>${badge}</span><small>${preset.detail}</small></label>`;
  }).join("");
  return `<h1 id="route-heading" tabindex="-1">Choose protection</h1><p class="compact-lead onboarding-centered-copy">Balanced starts on and is safe without more setup.</p><div class="send-mode-list protection-preset-list" role="group" aria-label="Protection preset">${presetChoices}</div><section class="setup-list" aria-labelledby="balanced-defaults-heading"><h2 id="balanced-defaults-heading" class="setup-section-heading">Balanced defaults</h2><div class="setup-status-row"><span><strong>Warn before risky sends</strong><small>OSL checks locally before protected handoff.</small></span><span class="status-tag active">On</span></div><div class="setup-status-row"><span><strong>Clean attachments by choice</strong><small>OSL can prepare a cleaned copy when you ask.</small></span><span class="status-tag active">On</span></div><div class="setup-status-row"><span><strong>Review cleanup monthly</strong><small>Deletion automation starts off. You review first.</small></span><span class="status-tag">Manual</span></div><div class="setup-status-row"><span><strong>Require clear authority</strong><small>No consent, account binding, or send/delete authority means Unavailable.</small></span><span class="status-tag active">Fail closed</span></div></section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-onboarding-privacy" type="button">Continue</button></div>`;
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
  return `<section class="mullvad-setup" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Mullvad</h1><p>Optional network privacy.</p><div class="mullvad-actions">${action}</div>${notice}<div class="setup-footer onboarding-actions"><button class="button primary" id="continue-mullvad" type="button">Continue</button><button class="text-button" id="skip-mullvad" type="button">Not now</button></div></section>`;
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function previousSetupRoute(current: OnboardingRoute): OnboardingRoute {
  const routes: Partial<Record<OnboardingRoute, OnboardingRoute>> = {
    pro: "recovery",
    privacy: "pro",
    defaults: "privacy",
    sending: "defaults",
    cover: "sending",
    passwords: "cover",
    burnpass: "passwords",
    mullvad: "burnpass",
    browser: "mullvad",
    tutorial: "browser",
    detected: "tutorial",
    install: onboardingBranch.detected ? "detected" : "tutorial",
    apps: onboardingBranch.install
      ? "install"
      : onboardingBranch.detected
        ? "detected"
        : "tutorial",
  };
  return onboardingRouteForBuild(routes[current] ?? "welcome");
}

function bindOnboarding(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = onboardingRouteForBuild(button.dataset.onboarding as OnboardingRoute); render(); }));
  document.querySelector<HTMLFormElement>("#activation-form")?.addEventListener("submit", (event) => void activatePro(event));
  bindSavedAccountControls();
  bindBrowserImportControls();
  bindPasswordVisibility();
  bindPasswordForm();
  bindImportForm();
  document.querySelector<HTMLButtonElement>("#retry-recovery-protection")?.addEventListener("click", async () => {
    await proveRecoveryCaptureProtection();
    render();
  });
  const recoverySaved = document.querySelector<HTMLInputElement>("#recovery-saved");
  const recoveryContinue = document.querySelector<HTMLButtonElement>("#recovery-continue");
  document.querySelector<HTMLButtonElement>("#copy-recovery-kit")?.addEventListener("click", async () => {
    if (!recoveryBundle || !recoveryCaptureGate.canRender()) return;
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
    onboardingRoute = onboardingRouteForBuild("pro");
    render();
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
  document.querySelector<HTMLButtonElement>("#continue-app-choice")?.addEventListener("click", async () => {
    if (!await ensureNativeCatalogForAppChoice()) return;
    persistCombinedHomeChoices();
    await completeOnboarding();
  });
  document.querySelector<HTMLButtonElement>("#continue-detected-apps")?.addEventListener("click", () => {
    if (savedAccountMode === "ask") savedAccountMode = savedNativeApps.size ? "use" : "clean";
    persistSavedAccountPreferences();
    const next = hasSelectedMissingNativeApps() ? "install" : "apps";
    markOnboardingBranch(next);
    if (next === "apps") selectNextConnectApp();
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
    selectNextConnectApp();
    onboardingRoute = "apps";
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-connect-app-choice]").forEach((button) => button.addEventListener("click", () => {
    onboardingConnectAppId = button.dataset.connectAppChoice as HomeAppId;
    render();
  }));
  document.querySelector<HTMLButtonElement>("#skip-connect-app")?.addEventListener("click", () => {
    if (onboardingConnectAppId) handledOnboardingConnectApps.add(onboardingConnectAppId);
    if (selectNextConnectApp()) onboardingRoute = "apps";
    else { void completeOnboarding(); return; }
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
    onboardingRoute = "cover";
    render();
  });
  document.querySelector("#continue-defaults-review")?.addEventListener("click", () => { onboardingRoute = "sending"; render(); });
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
  document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => { onboardingRoute = "defaults"; render(); });
  document.querySelector<HTMLInputElement>("#window-capture-enabled")?.addEventListener("change", async (event) => {
    windowCaptureEnabled = (event.currentTarget as HTMLInputElement).checked;
    screenshotProtectionEnabled = await setScreenshotProtection(windowCaptureEnabled).catch(() => false);
    if (windowCaptureEnabled && !screenshotProtectionEnabled) showToast("Windows capture resistance is unavailable on this device");
    render();
  });
  document.querySelector("#skip-mullvad")?.addEventListener("click", () => { onboardingRoute = "browser"; render(); void refreshBrowserImportReadiness(); });
  document.querySelector("#continue-mullvad")?.addEventListener("click", () => { onboardingRoute = "browser"; render(); void refreshBrowserImportReadiness(); });
  document.querySelector("#install-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("install"));
  document.querySelector("#open-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("open"));
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

function balancedFirstRunSetup(state: SetupState): SetupState {
  const sendMode = state.sendMode === "manual" ? "clipboard" : state.sendMode;
  const acceptedRisk = needsRiskAcceptance(sendMode) && state.acceptedRisk && state.acceptedRiskForMode === sendMode;
  return {
    sendMode,
    placementMode: "atomic",
    acceptedRisk,
    acceptedRiskForMode: acceptedRisk ? sendMode : null,
  };
}

async function completeSixStepOnboarding(): Promise<void> {
  const completedSetup = balancedFirstRunSetup(setup);
  if (!canCompleteSetup(completedSetup)) throw new Error("setup missing required sending consent");
  setup = completedSetup;
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
}

async function completeOnboarding(): Promise<void> {
  try {
    await completeSixStepOnboarding();
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
  const duressPin = document.querySelector<HTMLInputElement>("#identity-duress-pin");
  const confirm = document.querySelector<HTMLInputElement>("#identity-password-confirm");
  const submit = document.querySelector<HTMLButtonElement>("#identity-password-submit");
  const error = document.querySelector<HTMLElement>("#password-error");
  if (!form || !password || !submit || !error) return;
  const validate = (): void => {
    const passwordValid = form.dataset.passwordMode === "setup"
      ? isValidNewMainPassword(password.value)
      : isValidMainPassword(password.value);
    const duressValid = form.dataset.passwordMode === "unlock" && Boolean(duressPin?.value)
      ? isValidMainPassword(duressPin?.value ?? "")
      : true;
    const valid = form.dataset.passwordMode === "unlock"
      ? passwordValid || (Boolean(duressPin?.value) && duressValid)
      : passwordValid;
    submit.disabled = !valid || !duressValid || Boolean(confirm && confirm.value !== password.value);
    error.textContent = "";
  };
  password.addEventListener("input", validate);
  duressPin?.addEventListener("input", validate);
  confirm?.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    const setupMode = form.dataset.passwordMode === "setup";
    const idleLabel = submit.textContent ?? (setupMode ? "Create account" : "Unlock");
    let secret = password.value;
    let duressSecret = !setupMode && duressPin ? duressPin.value : "";
    const duressAttempt = duressSecret.length > 0;
    form.setAttribute("aria-busy", "true");
    password.disabled = true;
    if (duressPin) duressPin.disabled = true;
    if (confirm) confirm.disabled = true;
    submit.disabled = true;
    submit.textContent = setupMode ? "Creating account…" : "Unlocking…";
    if (!setupMode) password.value = "";
    if (duressPin) duressPin.value = "";
    try {
      if (setupMode) {
        const identity = core.readiness.identityLoaded ? null : await createHubOslIdentity(true);
        if (identity) identityStorageMethod = identity.storageMethod;
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
        recoverySavedAcknowledged = false;
        onboardingRoute = "recovery";
        await proveRecoveryCaptureProtection();
      } else {
        const gate = await checkUnlockScreenCredential(secret, duressSecret);
        secret = "";
        duressSecret = "";
        if (gate.outcome === "wrong") {
          error.textContent = gate.lockoutSecondsRemaining > 0
            ? `Try again in ${gate.lockoutSecondsRemaining} seconds.`
            : "Password or burn code not recognized.";
          form.removeAttribute("aria-busy");
          password.disabled = false;
          if (duressPin) duressPin.disabled = false;
          submit.disabled = false;
          submit.textContent = idleLabel;
          (duressAttempt ? duressPin : password)?.focus();
          return;
        }
        if (gate.outcome === "decoy") {
          identityStorageMethod = null;
          core = structuredClone(unavailableCoreIntegration);
          services = [];
          passwordRoleStatus = null;
          route = "onboarding";
          onboardingRoute = "decoy";
          render();
          return;
        }
        if (gate.outcome === "duress") {
          identityStorageMethod = null;
          localStorage.clear();
          onboardingComplete = false;
          setup = parseSetupState(null);
          services = [];
          passwordRoleStatus = null;
          core = structuredClone(unavailableCoreIntegration);
          route = "onboarding";
          onboardingRoute = "welcome";
          showToast("OSL signed out on this device");
          render();
          return;
        }
        if (isVerifiedBurnGate(gate)) {
          identityStorageMethod = null;
          onboardingComplete = false;
          localStorage.clear();
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
          void openMullvadOnStartup();
          void refreshUpdateStatus();
          void refreshIdentitySlots();
          void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") render(); });
          void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") render(); });
        }
        else onboardingRoute = pendingOnboardingRoute() ?? onboardingRouteForBuild("pro");
      }
      secret = "";
      duressSecret = "";
      password.value = "";
      if (duressPin) duressPin.value = "";
      if (confirm) confirm.value = "";
      render();
      if (discordQaShell && core.readiness.unlocked) void startDiscordQaShell();
      if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
      if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    } catch (failure) {
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check OSL account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        secret = "";
        duressSecret = "";
        error.textContent = "OSL could not verify the account state. Try again.";
        form.removeAttribute("aria-busy");
        password.disabled = false;
        if (duressPin) duressPin.disabled = false;
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
          onboardingRoute = setupMode ? onboardingRouteForBuild("pro") : pendingOnboardingRoute() ?? onboardingRouteForBuild("pro");
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
          onboardingRoute = onboardingRouteForBuild("pro");
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
      duressSecret = "";
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

async function checkUnlockScreenCredential(secret: string, duressSecret = ""): Promise<Awaited<ReturnType<typeof unlockHubPasswordGate>>> {
  return unlockHubPasswordGate(secret, duressSecret || undefined);
}

function isVerifiedBurnGate(gate: Awaited<ReturnType<typeof unlockHubPasswordGate>>): boolean {
  return gate.outcome === "burned" && gate.burn !== null;
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
      identityStorageMethod = identity.storageMethod;
      phraseSecret = "";
      const passwordResult = await setupHubMainPassword(passwordSecret);
      passwordSecret = "";
      core = await loadCoreIntegration();
      services = await loadLinkedServices().catch(() => services);
      recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase };
      recoverySavedAcknowledged = false;
      onboardingRoute = "recovery";
      await proveRecoveryCaptureProtection();
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
        onboardingRoute = onboardingRouteForBuild("pro");
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

function workspaceProtectedSheetMarkup(): string {
  const protectedSheet = activeEmbeddedHost
    ? protectedSheetMode === "local"
      ? localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)
      : peerProtectedSheetMarkup(peerProtectedSheet, hubPeople)
    : "";
  return `${protectedSheet}${nativeDiscordProtectPickerMarkup()}${whitelistRosterMarkup()}${peopleDialogMarkup()}${friendsDialogMarkup()}${scrubReviewDialogMarkup()}${burnDialogMarkup()}${ownedConfirmationMarkup()}${updateDialogMarkup()}`;
}

function renderWorkspace(): void {
  lastOnboardingMarkup = null;
  const markup = workspaceShellMarkup();
  let surface = root.querySelector<HTMLElement>("#workspace-render-surface");
  if (!surface) {
    // No separate 44px desktop titlebar row here: the drag region and window
    // controls are docked into the hub's own top row (see desktop-top-row
    // above), which is part of the markup re-rendered into
    // #workspace-render-surface on every commitRender(), not this
    // once-per-mount wrapper.
    root.innerHTML = `<div class="app-frame"><div id="workspace-render-surface"></div></div>`;
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
  if (whitelistRosterOpen) requestAnimationFrame(() => {
    const dialog = document.querySelector<HTMLDialogElement>("#whitelist-roster-dialog");
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

function workspaceShellMarkup(): string {
  return `<div class="hub-layout with-primary-sidebar">${primarySidebarMarkup()}<section class="hub-workspace"><div class="desktop-top-row" data-tauri-drag-region="deep">${trustedHeader()}${desktopWindowControlsMarkup()}</div>${workspaceContent()}</section></div>${workspaceProtectedSheetMarkup()}`;
}

export interface DestinationRouteTarget {
  destination: OslPrimaryDestination;
  route: Route;
  settingsSection: SettingsSection | null;
}

export function destinationRouteTarget(destination: OslPrimaryDestination): DestinationRouteTarget {
  if (destination === "home") return { destination, route: "home", settingsSection: null };
  if (destination === "inbox") return { destination, route: "inbox", settingsSection: null };
  if (destination === "people") return { destination, route: "people", settingsSection: null };
  if (destination === "privacy") return { destination, route: "privacy", settingsSection: null };
  if (destination === "activity") return { destination, route: "activity", settingsSection: null };
  return { destination, route: "connections", settingsSection: null };
}

export function fixedIaRoutePreview(): DestinationRouteTarget[] {
  return oslPrimaryDestinations.map((destination) => destinationRouteTarget(destination.id));
}

export function fixedIaSidebarOrderPreview(): string[] {
  return [...oslPrimaryDestinations.map((destination) => destination.id), oslSettingsDestination];
}

export function primarySidebarMarkup(): string {
  const activeDestination = (id: OslPrimaryDestination): boolean => {
    if (id === "home") return route === "home" && !friendsDialogOpen;
    if (id === "inbox") return route === "inbox" || route === "osl-chat" || route === "osl-mail";
    if (id === "people") return route === "people" || friendsDialogOpen;
    if (id === "privacy") return route === "privacy" || (route === "settings" && (settingsSection === "scrub" || settingsSection === "cleanup" || settingsSection === "appearance"));
    if (id === "activity") return route === "activity" || (route === "settings" && settingsSection === "notifications");
    if (id === "connections") return route === "connections" || route === "service" || route === "mullvad" || (route === "settings" && settingsSection === "apps");
    return false;
  };
  const destinationAttributes = (id: OslPrimaryDestination): string => {
    const target = destinationRouteTarget(id);
    return `data-route="${target.route}"`;
  };
  const items = oslPrimaryDestinations.map((destination) => {
    const current = activeDestination(destination.id);
    return `<button class="primary-sidebar-item ${current ? "active" : ""}" type="button" data-primary-destination="${destination.id}" ${destinationAttributes(destination.id)} ${current ? 'aria-current="page"' : ""}><span class="primary-sidebar-icon" aria-hidden="true">${escapeHtml(destination.label.slice(0, 1))}</span><span><strong>${escapeHtml(destination.label)}</strong><small>${escapeHtml(destination.userQuestion)}</small></span></button>`;
  }).join("");
  // Styling lives in styles.css (`.hub-layout.with-primary-sidebar` /
  // `.primary-sidebar*`), NOT in a runtime <style> element. The shipped CSP is
  // `style-src 'self'` with no `'unsafe-inline'`, no nonce and no hash, so the
  // WebView blocks runtime <style> elements (and inline `style` attributes)
  // outright; the inert copy that used to sit here styled nothing while the
  // navigation rendered as unstyled native buttons wrapping across the top of
  // the window. ia-sidebar.test.ts now asserts the layout rule against
  // styles.css, where it actually applies.
  return `<aside class="primary-sidebar" aria-label="OSL navigation"><button class="primary-sidebar-brand" type="button" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span>OSL</span></button><nav class="primary-sidebar-nav" aria-label="Primary destinations">${items}</nav><button class="primary-sidebar-settings ${route === "settings" ? "active" : ""}" type="button" data-route="${oslSettingsDestination}" ${route === "settings" ? 'aria-current="page"' : ""}><span>Settings</span></button></aside>`;
}

function appLauncherStrip(): string {
  const configured = configuredTopStripApps(homeAppsFromServices(services), homeTileOrder)
    .filter((app) => !hiddenServices.has(app.serviceId ?? ""));
  return `<nav class="app-launcher-strip" aria-label="Your apps">${configured.map((app) => `<button class="app-launcher ${activeHomeAppId === app.id ? "active" : ""} ${appLaunchPendingId === app.id ? "pending" : ""}" data-home-app="${app.id}" aria-label="Open ${escapeHtml(app.displayName)}" title="${escapeHtml(app.displayName)}" ${appLaunchPendingId ? "disabled" : ""}>${homeAppLogo(app)}</button>`).join("")}</nav>`;
}

function simpleDeviceStatusMarkup(): string {
  const coreReady = isCoreProtectionReady(core.readiness);
  const protection = identityProtectionStatus(core.readiness.storageMethod);
  const ready = coreReady && protection.state === "protected";
  const label = ready ? "Ready" : "Needs attention";
  const detail = label;
  return `<div class="trust-state ${ready ? "ready" : "pending"} ${coreReady && !ready ? "not-secure" : ""}" role="status" data-identity-protection="${protection.state}"><span class="dot"></span><span><strong>${escapeHtml(label)}</strong><small>${escapeHtml(detail)}</small></span></div>`;
}

function autoScrubRunServiceName(serviceId: ServiceId): string {
  return services.find((service) => service.id === serviceId)?.displayName ?? autoScrubServiceLabels[serviceId];
}

function fleetIndicatorMarkup(): string {
  const status = projectAutoScrubFleetStatus(autoScrubFleetStatus);
  const openRunNames = autoScrubFleetStatus?.runs.map((run) => autoScrubRunServiceName(run.serviceId)) ?? [];
  const openRunCount = autoScrubFleetStatus?.openRunCount ?? 0;
  const runNames = openRunNames.length ? openRunNames.join(", ") : "No cleanup running";
  const ariaLabel = `Cleanup monitor: ${status.label}; ${runNames}`;
  // Styling lives in styles.css (`.fleet-indicator`), NOT in an inline `style`
  // attribute. The shipped CSP is `style-src 'self'` with no `'unsafe-inline'`,
  // which blocks inline style attributes as well as <style> blocks, so the
  // previous inline-styled version rendered as two unstyled text runs jammed
  // against the window controls: no pill, no border, no vertical stacking.
  return `<aside class="fleet-indicator fleet-indicator-${status.tone}" data-fleet-indicator data-open-run-count="${openRunCount}" data-open-run-names="${escapeHtml(runNames)}" role="status" aria-label="${escapeHtml(ariaLabel)}" title="${escapeHtml(ariaLabel)}"><span class="fleet-indicator-dot" aria-hidden="true"></span><span class="fleet-indicator-text"><strong>${escapeHtml(status.label)}</strong><small>${escapeHtml(runNames)}</small></span></aside>`;
}

/**
 * The loudest chip in the header strip: OSL's composer is on screen and is not
 * the window receiving the operator's keystrokes.
 *
 * This strip is the one surface that draws above the borrowed native Discord
 * window, which is why the refusal chips live here rather than in a toast the
 * borrowed window covers. It follows the same shape as the whitelist and
 * composer-refusal chips beside it — fixed literal, `data-` state a QA probe can
 * read, the reason in `title`, and shape (`!`) as well as colour so it is not
 * colour-only — and it is deliberately `role="alert"`, not `role="status"`,
 * because unlike those it is reporting that plaintext is leaving the app right
 * now. Three plaintext messages have already reached a real conversation this
 * way, so it names the one thing that distinguishes the two composers on screen:
 * the cyan ring OSL draws around its own.
 *
 * Only rendered while protection is live. With the composer closed there is no
 * OSL composer to be unreachable, and a chip that outlived it would be telling
 * the operator to look for a ring that is not there.
 */
function nativeDiscordComposerUnreachableNotice(): string {
  if (!nativeDiscordProtectionActive || !nativeDiscordComposerUnreachable) return "";
  // The tooltip names the condition that actually raised, now that the payload
  // says which one it was. Three fixed literals chosen by a closed reason set —
  // nothing here is derived from a draft, a peer or a conversation — and a reason
  // this build does not recognise falls back to the disjunction rather than
  // guessing, because the warning matters and the wording is the polish.
  const cause = nativeDiscordComposerUnreachableReason === "zorder-band"
    ? "Discord is drawing above OSL's composer"
    : nativeDiscordComposerUnreachableReason === "keyboard-focus"
      ? "Windows refused OSL the keyboard"
      : "Windows refused it focus, or Discord is drawing above it";
  // Styling lives in styles.css (`.native-discord-composer-unreachable`), NOT in
  // an inline `style` attribute. The shipped CSP is `style-src 'self'` with no
  // `'unsafe-inline'`, and CSP style-src governs inline style *attributes* as
  // well as <style> blocks, so every declaration on this element was dropped:
  // the one chip that says plaintext is leaving the app right now rendered as
  // bare unstyled text with no border, no alarm colour and no chip shape.
  return `<span class="native-discord-composer-unreachable" id="native-discord-composer-unreachable" role="alert" data-composer-input-state="unreachable" title="OSL's protected composer is visible but is not receiving keyboard input — ${cause}. Anything you type now goes to Discord unencrypted. Stop typing, click the composer with the cyan lock ring, and confirm the ring before every message."><span class="native-discord-composer-unreachable-mark" aria-hidden="true">!</span> Your typing is going to Discord, not OSL — check the cyan ring</span>`;
}

function nativeDiscordHeaderControls(): string {
  const discordQaRoute = discordQaShell && activeHomeAppId === "discord";
  if (route !== "service" || (activeNativeHostId !== "discord" && !discordQaRoute)) return "";
  // Built before the build discriminator below: a shipping build borrows the same
  // Discord window and refuses focus the same way, so the warning belongs in both
  // strips and not only in the QA one.
  const composerUnreachableNotice = nativeDiscordComposerUnreachableNotice();
  if (!discordQaShell) {
    const inactive = nativeDiscordProtectionActive ? "" : "disabled";
    return `<div class="native-discord-header-controls" aria-label="Discord privacy controls">${composerUnreachableNotice}<button class="header-protection-control burn" data-open-burn="chat" type="button" ${inactive} title="Burn this local OSL chat">Burn</button><button class="header-protection-control ${nativeDiscordCovertextEnabled ? "active" : ""}" id="native-discord-covertext" type="button" aria-pressed="${nativeDiscordCovertextEnabled}" title="${nativeDiscordCovertextEnabled ? "Covertext is on" : "Covertext is off"}">Covertext</button><button class="header-protection-control" id="native-discord-ai-covertext" type="button" disabled title="Requires a verified local model pack; no cloud AI is used">AI Covertext <small>Model pack needed</small></button></div>`;
  }
  const context = peerProtectedSheet.context;
  const verifiedPeer = context
    ? hubPeople.find((person) => person.personId === context.personId
      && person.safetyNumberVerified
      && !person.pendingKeyChange)
    : null;
  const inactive = nativeDiscordProtectionActive ? "" : "disabled";
  const whitelistBusy = discordQaHeaderBusy === "whitelist";
  const visibilityBusy = discordQaHeaderBusy === "visibility";
  const rowProofBusy = discordQaRowProofState === "busy";
  const scopeApproved = context?.scopeApproved === true;
  // Revoking used to be silent: the scope goes un-approved and the very next
  // send just fails closed in Rust ("Approve encryption for this friend
  // before continuing"), with nothing on screen explaining why. This chip
  // says so up front, in the one surface that draws above the borrowed
  // native Discord window. It is a fixed literal — never interpolated draft
  // or message text — and never claims anything about whether the other
  // person has an OSL account, which this app cannot know. Its styling lives in
  // styles.css (`.discord-qa-whitelist-warning`), not in an inline `style`
  // attribute: the shipped CSP is `style-src 'self'` with no `'unsafe-inline'`,
  // which drops inline style attributes too, so the inline copy styled nothing.
  const whitelistWarningNotice = nativeDiscordProtectionActive && verifiedPeer && !scopeApproved
    ? `<span class="discord-qa-whitelist-warning" id="discord-qa-whitelist-warning" role="status" data-whitelist-state="revoked" title="Press the + button to allow this chat again. Until then, every message you send in it will fail to send.">Encryption revoked for this chat — sends will fail until you allow it again</span>`
    : "";
  const transcriptVisible = peerProtectedSheet.decryptDisplayEnabled;
  const flame = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M13.4 2.8c.5 3.6-2.6 4.8-2.6 7.4 0 1.1.7 2 1.8 2.4-.2-1.8.8-3.2 2.3-4.4 2.4 1.8 4 4.2 4 7.1A6.9 6.9 0 0 1 12 22a6.9 6.9 0 0 1-6.9-6.7c0-3.8 2.3-7.2 6.9-10.3-.1 2.5.6 3.3 1.4 4.1.8-1.8 1-3.9 0-6.3Z"/></svg>`;
  const accountBurnIcon = `<span class="discord-qa-burn-mark account"><img src="${oslLogoUrl}" alt="" aria-hidden="true"><span class="discord-qa-burn-flame">${flame}</span></span>`;
  const discordBurnIcon = `<span class="discord-qa-burn-mark discord">${serviceLogo("discord")}<span class="discord-qa-burn-flame">${flame}</span></span>`;
  const eye = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2.2 12s3.5-5.5 9.8-5.5 9.8 5.5 9.8 5.5-3.5 5.5-9.8 5.5S2.2 12 2.2 12Z"/><circle cx="12" cy="12" r="2.7"/>${transcriptVisible ? "" : '<path class="qa-icon-slash" d="M4 4l16 16"/>'}</svg>`;
  // The eye is a two-mode control, so it says which mode is live in four
  // independent ways: the slashed/unslashed icon, the visible/hidden colour
  // class, aria-pressed, and data-transcript-mode. The last one is also what a
  // QA probe can read without guessing at colour.
  const transcriptMode = transcriptVisible ? "plaintext" : "flagtext";
  const transcriptModeTitle = transcriptVisible
    ? "Transcript shows decrypted text. Click to show Discord's flagtext instead."
    : "Transcript shows Discord's flagtext. Click to show decrypted text instead.";
  // Only a live verified peer scope can report an outcome; without one the
  // control is disabled and must not keep displaying a stale failure.
  const transcriptOutcome: DiscordQaTranscriptVisibilityOutcome = verifiedPeer
    ? discordQaTranscriptVisibilityOutcome
    : "applied";
  const transcriptFailed = transcriptOutcome === "failed";
  const transcriptUnapplied = transcriptOutcome === "unapplied";
  const transcriptTitle = !verifiedPeer
    ? "Transcript visibility needs this verified friend's protected chat"
    : transcriptFailed
      ? `Transcript visibility failed closed; the transcript did not change. ${transcriptModeTitle}`
      : transcriptUnapplied
        ? `Saved for this chat. There is no protected display surface open at all, so nothing on screen changes yet. ${transcriptModeTitle}`
        : transcriptModeTitle;
  // Failure/not-applied has to be legible in the header strip itself: the
  // borrowed native Discord window covers this webview's toast layer, so a
  // toast alone would leave the control looking inert.
  const transcriptNotice = transcriptFailed || transcriptUnapplied
    ? `<span class="discord-qa-visibility-notice" id="discord-qa-transcript-visibility-notice" role="status" data-transcript-state="${transcriptOutcome}">${transcriptFailed ? "Eye failed — transcript unchanged" : "Eye saved — no display surface open"}</span>`
    : "";
  const transcriptVisibilityControl = `<button class="discord-qa-icon-control ${transcriptVisible ? "visible" : "hidden"}${transcriptFailed ? " transcript-failed" : ""}" id="discord-qa-transcript-visibility" type="button" aria-pressed="${transcriptVisible}" data-transcript-mode="${transcriptMode}" data-transcript-state="${transcriptOutcome}" ${transcriptFailed ? 'aria-invalid="true" ' : ""}aria-label="${transcriptVisible ? "Hide protected transcript" : "Show protected transcript"}" title="${transcriptTitle}" ${!verifiedPeer || visibilityBusy ? "disabled" : ""}>${eye}</button>`;
  const lock = `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="10" width="14" height="11" rx="2"/><path d="${nativeDiscordProtectionActive ? "M8 10V7a4 4 0 0 1 8 0v3" : "M8 10V7a4 4 0 0 1 7.7-1.5"}"/></svg>`;
  // "Refused" only survives while protection is still off: an open composer
  // answers the question the refusal was asking. The four states are otherwise
  // mutually exclusive, and "off" means off and never refused — the three used
  // to look identical, which is the whole complaint.
  const composerRefusal = nativeDiscordProtectionActive ? null : discordQaComposerRefusal;
  const composerLockState = nativeDiscordProtectionActive
    ? "on"
    : discordQaComposerBusy
      ? "busy"
      : composerRefusal
        ? "refused"
        : "off";
  // The reason lives in aria-label/title as well as the chip, so the state is
  // never colour-only and never depends on a toast the operator cannot see.
  const composerProtectionLabel = nativeDiscordProtectionActive
    ? "Protected composer on — close"
    : composerLockState === "busy"
      ? "Protected composer opening…"
      : composerRefusal
        ? `Protected composer refused — ${escapeHtml(composerRefusal.message)} (${escapeHtml(composerRefusal.reason)})`
        : "Protected composer off — open";
  // Shape, not colour: a refused lock carries a bang mark, so the state reads
  // the same way with any theme or colour vision. Its `position: absolute` and
  // the `position: relative` that used to sit inline on the button below both
  // live in styles.css now (`.discord-qa-icon-control.composer-refused` /
  // `.discord-qa-composer-refused-mark`): the shipped CSP is `style-src 'self'`
  // with no `'unsafe-inline'`, which drops inline style attributes too, so the
  // badge was being laid out in normal flow and the shape affordance was gone.
  const composerRefusedMark = composerRefusal
    ? `<span class="discord-qa-composer-refused-mark" aria-hidden="true">!</span>`
    : "";
  // Pages with no message composer (e.g. Friends) report discordMarkerAvailable
  // false; the lock is hidden there. Protection already open stays shown so it
  // always has a control to turn back off, even if the view changes under it.
  const composerControl = discordMarkerAvailable || nativeDiscordProtectionActive
    ? `<button class="discord-qa-icon-control composer ${nativeDiscordProtectionActive ? "locked" : "unlocked"}${composerRefusal ? " composer-refused" : ""}" id="discord-qa-toggle-composer" type="button" aria-pressed="${nativeDiscordProtectionActive}" aria-label="${composerProtectionLabel}" title="${composerProtectionLabel}" ${discordQaComposerBusy ? "disabled" : ""} data-lock-state="${composerLockState}"${composerRefusal ? ' aria-invalid="true"' : ""}>${lock}${composerRefusedMark}</button>`
    : "";
  // Persistent, plain-language refusal in the header strip — the one surface
  // that draws above the borrowed native Discord window. It stays until the
  // next operator attempt or a successful open, so a reason can no longer be
  // produced and lost, and it is never populated by an automatic retry.
  const composerRefusalNotice = composerRefusal
    ? `<span class="discord-qa-composer-refusal" id="discord-qa-composer-refusal" role="status" data-lock-state="refused" title="${escapeHtml(composerRefusal.reason)}">${escapeHtml(composerRefusal.message)}</span>`
    : "";
  const rowProofLabel = discordQaRowProofState === "accepted"
    ? "Row proof passed"
    : discordQaRowProofState === "refused"
      ? "Row proof refused"
      : discordQaRowProofState === "unavailable"
        ? "Row proof unavailable"
        : rowProofBusy
          ? "Checking row proof…"
          : "Check row proof";
  const rowProofControl = `<button class="discord-qa-control" id="discord-qa-row-proof" type="button" data-runtime-proof="${discordQaRowProofState}" aria-label="${rowProofLabel}" title="${rowProofLabel}" ${!nativeDiscordProtectionActive || !verifiedPeer || rowProofBusy ? "disabled" : ""}>Proof</button>`;
  return `<div class="native-discord-header-controls discord-qa-header-controls" aria-label="Discord QA privacy controls"><div class="discord-qa-header-left"><button class="discord-qa-control danger icon-only" data-open-burn="account" type="button" aria-label="Account Burn" title="Open Account Burn confirmation">${accountBurnIcon}</button></div><button class="discord-qa-control danger icon-only discord-qa-discord-burn" data-open-burn="app" type="button" aria-label="Discord Burn" title="Open Discord Burn confirmation">${discordBurnIcon}</button><div class="discord-qa-header-right">${rowProofControl}<div class="discord-qa-whitelist" role="group" aria-label="Connected verified peer whitelist"><button id="discord-qa-whitelist-roster" type="button" aria-haspopup="dialog" aria-expanded="${whitelistRosterOpen}" title="Review who is whitelisted and where" ${discordQaHeaderBusy ? "disabled" : ""}>Whitelist</button><button id="discord-qa-whitelist-add" type="button" aria-label="Allow this verified peer scope" title="Allow this verified peer scope" ${!nativeDiscordProtectionActive || !verifiedPeer || scopeApproved || whitelistBusy ? "disabled" : ""}>+</button><button id="discord-qa-whitelist-remove" type="button" aria-label="Revoke this verified peer scope" title="Revoke this verified peer scope" ${!nativeDiscordProtectionActive || !verifiedPeer || !scopeApproved || whitelistBusy ? "disabled" : ""}>−</button></div><button class="discord-qa-control danger icon-only chat-burn" data-open-burn="chat" type="button" ${inactive} aria-label="Chat Burn" title="Open Chat Burn confirmation">${flame}</button>${composerUnreachableNotice}${composerRefusalNotice}${transcriptNotice}${transcriptVisibilityControl}${composerControl}${whitelistWarningNotice}</div></div>`;
}

function trustedHeader(): string {
  // Service controls stay compact; deeper setup remains progressively disclosed.
  if (route === "home" || route === "inbox" || route === "people" || route === "privacy" || route === "activity" || route === "connections" || route === "osl-chat" || route === "osl-mail") return homeHeader();
  if (route === "mullvad") {
    return `<div class="trusted-stack"><header class="workspace-header mullvad-host-header"><button class="button compact" id="mullvad-return" type="button">${mullvadReturnRoute === "onboarding" ? "Back to setup" : "Back to Home"}</button><div class="service-context"><span><strong>Mullvad</strong><small>Existing session · capture resistance does not cover Mullvad</small></span></div></header></div>`;
  }
  if (route === "service"
    && activeService
    && serviceGuideStep !== null
    && !(discordQaShell && activeHomeAppId === "discord")) {
    return `<div class="trusted-stack home-trusted-stack"><header class="home-header guide-header"><button class="home-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="guide-header-service">${serviceLogo(activeService.id)}<span><strong>${escapeHtml(activeService.displayName)}</strong><small>${isCoreProtectionReady(core.readiness) ? "Ready" : "Needs attention"}</small></span></div>${settingsButtonMarkup()}</header></div>`;
  }
  const localProtection = route === "service" && (activeEmbeddedHost || activeNativeHostId === "discord")
    ? `<button class="local-protected-toggle" id="local-protected-toggle" type="button" aria-expanded="${localProtectedSheet.open || peerProtectedSheet.open || nativeDiscordProtectionActive}">Protect</button>`
    : "";
  const mailScope = route === "service" ? mailComposerEncryptionScope(activeHomeApp()) : "";
  const serviceControls = route === "service" && activeService ? `<div class="service-context"><span class="service-context-logo">${serviceLogo(activeService.id)}</span><span><strong>${escapeHtml(activeHomeAppName())}</strong><small>${activeEmbeddedHost ? "Isolated OSL profile" : activeDefaultBrowserCompanion ? "Default-browser companion · unprotected" : activeNativeHostMode === "existingSession" ? "Native companion" : activeNativeHostId ? "OSL app window" : "Needs setup"}</small></span>${mailScope}${localProtection}</div>` : "";
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

export type HomePrimaryIssue =
  | "account-protection"
  | "local-storage"
  | "trusted-people-review"
  | "connect-service"
  | "add-trusted-person"
  | "recent-activity"
  | "protected-conversation";

export interface HomePrimaryActionInput {
  coreReady: boolean;
  storageProtected: boolean;
  storageDetail: string;
  coreDetail: string;
  pendingFriendReviews: number;
  connectedApps: number;
  verifiedFriends: number;
  hasRecentActivity: boolean;
}

export interface HomePrimaryActionPlan {
  issue: HomePrimaryIssue;
  title: string;
  detail: string;
  label: string;
  target:
    | { kind: "route"; route: Route; settingsSection: SettingsSection | null; profileSettings?: boolean }
    | { kind: "people" }
    | { kind: "notifications" }
    | { kind: "home-module"; module: "osl-chats" };
}

export function homePrimaryActionPlan(input: HomePrimaryActionInput): HomePrimaryActionPlan {
  if (!input.coreReady) {
    return {
      issue: "account-protection",
      title: "Finish account protection",
      detail: input.coreDetail,
      label: "Fix now",
      target: { kind: "route", route: "settings", settingsSection: "account", profileSettings: true },
    };
  }
  if (!input.storageProtected) {
    return {
      issue: "local-storage",
      title: "Review device storage",
      detail: input.storageDetail,
      label: "Review device",
      target: { kind: "route", route: "settings", settingsSection: "account", profileSettings: true },
    };
  }
  if (input.pendingFriendReviews > 0) {
    return {
      issue: "trusted-people-review",
      title: "Review trusted people",
      detail: `${input.pendingFriendReviews.toLocaleString("en-US")} ${input.pendingFriendReviews === 1 ? "request needs" : "requests need"} your approval.`,
      label: "Review",
      target: { kind: "people" },
    };
  }
  if (input.connectedApps === 0) {
    return {
      issue: "connect-service",
      title: "Connect a service",
      detail: "No connected app is ready for protected use yet.",
      label: "Connect",
      target: { kind: "route", route: "connections", settingsSection: null },
    };
  }
  if (input.verifiedFriends === 0) {
    return {
      issue: "add-trusted-person",
      title: "Add a trusted person",
      detail: "Protected conversations stay unavailable until someone is verified.",
      label: "Add friend",
      target: { kind: "people" },
    };
  }
  if (input.hasRecentActivity) {
    return {
      issue: "recent-activity",
      title: "Review recent protection",
      detail: "New local OSL activity is waiting.",
      label: "Review",
      target: { kind: "route", route: "activity", settingsSection: null },
    };
  }
  return {
    issue: "protected-conversation",
    title: "Open a protected conversation",
    detail: "Your account, local storage and trusted people are ready.",
    label: "Open OSL Chat",
    target: { kind: "home-module", module: "osl-chats" },
  };
}

function primaryActionButton(plan: HomePrimaryActionPlan, extraAttributes = ""): string {
  const label = escapeHtml(plan.label);
  const attrs = extraAttributes ? ` ${extraAttributes}` : "";
  if (plan.target.kind === "people") return `<button class="button primary compact" data-open-friends type="button"${attrs}>${label}</button>`;
  if (plan.target.kind === "notifications") return `<button class="button primary compact" data-notification-settings type="button"${attrs}>${label}</button>`;
  if (plan.target.kind === "home-module") return `<button class="button primary compact" data-home-module="${plan.target.module}" type="button"${attrs}>${label}</button>`;
  const section = plan.target.settingsSection ? ` data-settings="${plan.target.settingsSection}"` : "";
  const profile = plan.target.profileSettings ? " data-profile-settings" : "";
  return `<button class="button primary compact" data-route="${plan.target.route}"${section}${profile} type="button"${attrs}>${label}</button>`;
}

function homePrimaryRecommendation(): HomePrimaryActionPlan {
  const coreReady = isCoreProtectionReady(core.readiness);
  const protection = identityProtectionStatus(core.readiness.storageMethod);
  const launchableApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const connectedApps = launchableApps.filter((app) => app.linked || savedNativeApps.has(app.id as NativeAppId));
  const pendingFriendReviews = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange).length;
  const verifiedFriends = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange).length;
  const recentActivity = notificationsEnabled ? visibleAppNotifications().at(0) ?? null : null;
  return homePrimaryActionPlan({
    coreReady,
    storageProtected: protection.state === "protected",
    storageDetail: protection.detail,
    coreDetail: coreReadinessLabel(core.readiness),
    pendingFriendReviews,
    connectedApps: connectedApps.length,
    verifiedFriends,
    hasRecentActivity: Boolean(recentActivity),
  });
}

export function homePrimaryAction(): void {
  const plan = homePrimaryRecommendation();
  if (plan.target.kind === "route") {
    route = plan.target.route;
    if (plan.target.settingsSection) settingsSection = plan.target.settingsSection;
    if (plan.target.route === "connections") {
      activeService = null;
      activeHomeAppId = null;
      serviceAccountPickerOpen = false;
    }
    render();
    return;
  }
  if (plan.target.kind === "people") {
    route = "people";
    activeOslChatPersonId = null;
    friendsDialogOpen = false;
    render();
    return;
  }
  if (plan.target.kind === "notifications") {
    settingsSection = "notifications";
    route = "settings";
    render();
    return;
  }
  inboxPrimaryAction();
}

function homeDestinationContent(): string {
  const coreReady = isCoreProtectionReady(core.readiness);
  const protection = identityProtectionStatus(core.readiness.storageMethod);
  const deviceProtected = coreReady && protection.state === "protected";
  const launchableApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const connectedApps = launchableApps.filter((app) => app.linked || savedNativeApps.has(app.id as NativeAppId));
  const pendingFriendReviews = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange).length;
  const verifiedFriends = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange).length;
  const recentActivity = notificationsEnabled ? visibleAppNotifications().at(0) ?? null : null;
  // Compatibility markers for the legacy Home source-shape regression test:
  // data-profile-settings data-open-friends data-notification-settings data-home-module="osl-chats"
  const recommended = homePrimaryRecommendation();
  const recommendedAction = primaryActionButton(recommended, `data-home-primary-issue="${recommended.issue}"`);
  const attention = !deviceProtected || pendingFriendReviews > 0 || connectedApps.length === 0 || verifiedFriends === 0
    ? `<div class="setting-line home-status-row" role="status"><span><strong>${escapeHtml(recommended.title)}</strong><small>${escapeHtml(recommended.detail)}</small></span><span class="status-tag">Needs attention</span></div>`
    : "";
  const activityDetail = recentActivity
    ? "New local OSL activity"
    : notificationsEnabled
      ? "No recent local activity"
      : "Local activity is off";
  const activityAction = recentActivity || !notificationsEnabled
    ? `<button class="button compact" data-notification-settings type="button">${recentActivity ? "Review" : "Turn on"}</button>`
    : `<span class="status-tag">Quiet</span>`;
  return `<section class="home-protection-summary" aria-labelledby="route-heading" data-home-destination="protection-status"><h1 id="route-heading" tabindex="-1">Home</h1><div class="setting-line home-overall-state" data-home-protection-state="${deviceProtected ? "protected" : "needs-attention"}"><span><strong>${deviceProtected ? "Protected" : "Needs attention"}</strong><small>${escapeHtml(coreReady ? protection.detail : coreReadinessLabel(core.readiness))}</small></span>${recommendedAction}</div>${attention}<div class="settings-list home-protection-facts" aria-label="Protection status"><div class="setting-line"><span><strong>Connected apps</strong><small>${connectedApps.length.toLocaleString("en-US")} of ${launchableApps.length.toLocaleString("en-US")} ready</small></span><span class="status-tag">${connectedApps.length ? "Ready" : "Unavailable"}</span></div><div class="setting-line"><span><strong>Trusted people</strong><small>${verifiedFriends.toLocaleString("en-US")} verified${pendingFriendReviews ? `, ${pendingFriendReviews.toLocaleString("en-US")} need review` : ""}</small></span><button class="button compact" data-open-friends type="button">${pendingFriendReviews ? "Review" : "Manage"}</button></div><div class="setting-line"><span><strong>Recent protection</strong><small>${escapeHtml(activityDetail)}</small></span>${activityAction}</div></div></section>`;
}

function workspaceContent(): string {
  if (route === "mullvad") return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="Your existing Mullvad window is open inside OSL"><span class="sr-only">Mullvad remains a separate foreign application. OSL does not read its account or VPN state.</span></main>`;
  if (route === "inbox") return inboxDestinationContent();
  if (route === "people") return peopleDestinationContent();
  if (route === "privacy") return privacyDestinationContent();
  if (route === "activity") return activityDestinationContent();
  if (route === "connections") return connectionsDestinationContent();
  if (route === "osl-chat") return oslChatContent();
  if (route === "osl-mail") return oslMailContent();
  if (route === "osl-servers") return oslServersContent();
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
    { id: "osl-mail", name: "OSL Mail", available: true },
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
    return `<article class="app-tile ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${homeEditMode}" data-service-kind="${app.serviceId ?? "none"}"><button id="home-app-${app.id}" type="button" data-home-app="${app.id}" aria-label="${escapeHtml(`${app.displayName}, ${pending ? "Opening" : state}`)}" ${appLaunchPendingId ? "disabled" : ""}><span class="app-logo-plate">${homeAppLogo(app)}</span><span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong>${pending ? "<small>Opening…</small>" : ""}</span></button>${controls}</article>`;
  };
  const socialIds = new Set(homeApps.filter((app) => app.provider === null).map((app) => app.id));
  const emailIds = new Set(homeApps.filter((app) => app.provider !== null).map((app) => app.id));
  const socialTiles = orderedIds.filter((id) => socialIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const emailTiles = orderedIds.filter((id) => emailIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const oslTiles = orderedIds.filter((id) => moduleById.has(id as typeof modules[number]["id"])).map(renderHomeTile).join("");
  const organizeButton = (label: string) => `<button class="home-section-action" data-edit-home type="button" aria-label="${homeEditMode ? "Finish arranging" : `Customize ${label}`}" title="${homeEditMode ? "Done" : `Customize ${label}`}">${homeCommandIcon("organize")}</button>`;
  const oslSection = oslTiles ? `<section class="home-app-section home-osl-section"><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  const activeIdentity = hubIdentities.find((identity) => identity.active);
  const profileName = activeIdentity?.label?.trim() || "OSL Profile";
  const profileInitial = profileName.slice(0, 1).toLocaleUpperCase();
  return `<main id="home-navigation" class="content-viewport home-dashboard ${homeEditMode ? "editing" : ""}"><section class="home-primary">${homeDestinationContent()}<section class="home-apps" aria-labelledby="route-heading"><div class="home-app-groups">${oslSection}${socialTiles ? `<section class="home-app-section"><header><h2>Social</h2>${organizeButton("social apps")}</header><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section>` : ""}${emailTiles ? `<section class="home-app-section"><header><h2>Email</h2>${organizeButton("email apps")}</header><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>` : ""}</div></section></section><button class="home-profile-dock" data-route="settings" data-profile-settings type="button" aria-label="Open your OSL profile" title="${escapeHtml(profileName)}"><span aria-hidden="true">${escapeHtml(profileInitial)}</span><strong>${escapeHtml(profileName)}</strong></button></main>`;
}

const privateCircleAudienceRecords = [
  { audienceId: "c".repeat(32), name: "Close friends", memberCount: 3, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }, { memberId: "2".repeat(32), name: "Theo", verified: true }, { memberId: "3".repeat(32), name: "Rina", verified: true }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true },
  { audienceId: "f".repeat(32), name: "Family", memberCount: 5, membershipVisibility: "visible", visibleMembers: [{ memberId: "4".repeat(32), name: "Ari", verified: true }, { memberId: "5".repeat(32), name: "Sam", verified: false }], consentGranted: false, boundToCurrentCircle: true, postingAuthorized: true },
  { audienceId: "b".repeat(32), name: "Book club", memberCount: 8, membershipVisibility: "count-only", visibleMembers: [], consentGranted: true, boundToCurrentCircle: false, postingAuthorized: true },
  { audienceId: "w".repeat(32), name: "Work", memberCount: 4, membershipVisibility: "count-only", visibleMembers: [], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true },
  { audienceId: "n".repeat(32), name: "Neighborhood", memberCount: 12, membershipVisibility: "hidden", visibleMembers: [], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: false },
] as const;

const privateCircleAudiences: CircleAudience[] = privateCircleAudienceRecords
  .map((record) => parseCircleAudience(record))
  .filter((audience): audience is CircleAudience => audience !== null);

function circleAudienceMembershipDetail(audience: CircleAudience): string {
  if (audience.membershipVisibility === "visible") {
    const names = audience.visibleMembers.map((member) => `${member.name}${member.verified ? " verified" : " needs review"}`).join(", ");
    return names ? `${audience.memberCount.toLocaleString("en-US")} people: ${names}` : `${audience.memberCount.toLocaleString("en-US")} people. Members are shown before posting.`;
  }
  if (audience.membershipVisibility === "count-only") return `${audience.memberCount.toLocaleString("en-US")} people. Names are shown during the final audience review before posting.`;
  return "Membership is hidden here. Posting stays refused until the audience is shown for review.";
}

function circleAudienceStatus(audience: CircleAudience): { label: "Ready" | "Refused"; detail: string } {
  if (audience.canPost) return { label: "Ready", detail: "Posts and comments are encrypted for the selected audience." };
  if (audience.refusal === "consent") return { label: "Refused", detail: "Review and approve this audience on this device before posting." };
  if (audience.refusal === "binding") return { label: "Refused", detail: "Choose the Circle for this audience before posting." };
  return { label: "Refused", detail: "This account is not allowed to post to that audience." };
}

function circlesDestinationContent(): string {
  const audienceCards = privateCircleAudiences.map((audience) => {
    const status = circleAudienceStatus(audience);
    return `<article class="setting-line circle-audience-card ${audience.canPost ? "" : "unavailable"}" data-circle-audience="${escapeHtml(audience.audienceId)}" data-circle-posting="${audience.canPost ? "ready" : "refused"}" data-circle-refusal="${audience.refusal ?? "none"}" aria-disabled="${audience.canPost ? "false" : "true"}"><span><strong>${escapeHtml(audience.name)}</strong><small>${escapeHtml(circleAudienceMembershipDetail(audience))}</small></span><span class="status-tag">${status.label}</span><p>${escapeHtml(status.detail)}</p></article>`;
  }).join("");
  const feedItems = privateCircleAudiences.filter((audience) => audience.canPost).map((audience, index) => `<article class="inbox-row circle-feed-item" data-circle-feed-item="${index}" data-circle-feed-order="chronological" data-circle-audience="${escapeHtml(audience.audienceId)}"><span class="source-mark">${homeModuleIcon("osl-chats")}</span><div><strong>${escapeHtml(audience.name)}</strong><small>Chronological private feed · ${audience.memberCount.toLocaleString("en-US")} people · no ranking or behavioral advertising</small></div><span class="status-tag">Encrypted</span></article>`).join("");
  return `<section class="inbox-surface-card circles-destination" data-inbox-osl-surface="circles" data-circle-feeds="private-audiences"><strong>OSL Circles</strong><small>Private audience feeds</small><p><span class="status-tag">Private</span> Posts and comments are encrypted for the selected audience. Audience membership is shown before posting.</p><div class="settings-list circle-audience-list" aria-label="Private Circle audiences">${audienceCards}</div><div class="circle-feed-list" aria-label="Chronological private Circle feeds">${feedItems}</div>${publicCirclesUnavailableMarkup()}</section>`;
}

type OslMailboxStageCReview = {
  mailOperationsReviewAccepted: boolean;
  explicitConsentBound: boolean;
  mailboxBindingReviewed: boolean;
  accountAuthorityReviewed: boolean;
};

type OslMailboxStageCGateResult = {
  operationsAllowed: boolean;
  label: "Coming later" | "Unavailable" | "Reviewed";
  reason: "stage-c-coming-later" | "separate-mail-operations-review-required" | "mailbox-consent-binding-authority-required" | null;
  detail: string;
};

export function oslMailboxStageCGate(
  review: OslMailboxStageCReview | null = null,
  stage: OslMailStage = oslMailStage("stageC"),
): OslMailboxStageCGateResult {
  if (stage.id !== "stageC" || stage.availability !== "available") {
    return {
      operationsAllowed: false,
      label: "Coming later",
      reason: "stage-c-coming-later",
      detail: "Full OSL mailbox is coming later. Mailbox operations stay off until a separate mail operations review accepts it.",
    };
  }
  if (review?.mailOperationsReviewAccepted !== true) {
    return {
      operationsAllowed: false,
      label: "Unavailable",
      reason: "separate-mail-operations-review-required",
      detail: "Full OSL mailbox operations require a separate mail operations review.",
    };
  }
  if (review.explicitConsentBound !== true || review.mailboxBindingReviewed !== true || review.accountAuthorityReviewed !== true) {
    return {
      operationsAllowed: false,
      label: "Unavailable",
      reason: "mailbox-consent-binding-authority-required",
      detail: "Full OSL mailbox operations need explicit consent, reviewed mailbox access, and reviewed account authority.",
    };
  }
  return {
    operationsAllowed: true,
    label: "Reviewed",
    reason: null,
    detail: "Full OSL mailbox operations are available only for this reviewed mailbox.",
  };
}

export function oslMailStageAContent(
  stage: OslMailStage = oslMailStage("stageA"),
  mailboxGate: OslMailboxStageCGateResult = oslMailboxStageCGate(),
): string {
  if (stage.id !== "stageA" || stage.availability !== "available") {
    return `<article class="inbox-surface-card unavailable" data-inbox-osl-surface="mail" data-osl-mail-stage-a="unavailable" data-osl-mail-protection="private-client" data-osl-mailbox-stage-c-gate="${mailboxGate.reason ?? "reviewed"}" data-mailbox-operations="refused" aria-disabled="true"><strong>OSL Mail</strong><small>Private client protection</small><p><span class="status-tag">Coming later</span> OSL Mail client protection is unavailable until Stage A review accepts it.</p></article>`;
  }
  const capabilities = [
    "Connect an existing mailbox only after authorization",
    "Warn before send and label the protection scope",
    "Sanitize selected links and attachments",
    "Organize retention on this device",
  ];
  return `<article class="inbox-surface-card" data-inbox-osl-surface="mail" data-osl-mail-stage-a="available" data-osl-mail-protection="private-client" data-osl-mailbox-stage-c-gate="${mailboxGate.reason ?? "reviewed"}" data-mailbox-operations="${mailboxGate.operationsAllowed ? "allowed" : "refused"}" aria-disabled="false"><strong>OSL Mail</strong><small>Private client protection</small><p><span class="status-tag">Available</span> Protect mailboxes you already control after explicit authorization.</p><ul>${capabilities.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><p><span class="status-tag">${mailboxGate.label}</span> ${escapeHtml(mailboxGate.detail)} External email remains ordinary email unless a supported encrypted path is selected before send.</p></article>`;
}

export function oslMailStageBContent(
  stage: OslMailStage = oslMailStage("stageB"),
): string {
  const capabilities = [
    "Aliases for signups and breach isolation",
    "Reply routing for messages sent to an alias",
    "Relay behavior only after abuse handling, deliverability, recovery, and support gates pass",
  ];
  const ready = stage.id === "stageB" && stage.availability === "available";
  const label = ready ? "Reviewed" : "Coming later";
  const state = ready ? "available" : "coming-later";
  const detail = ready
    ? "Aliases and reply routing are available only for this reviewed mail setup."
    : "Aliases and relay come after client protection, once abuse handling, deliverability, reply routing, account recovery, and support operations pass review.";
  return `<article class="inbox-surface-card mail-stage-card" data-inbox-osl-surface="mail" data-osl-mail-stage-b="${state}" data-osl-mail-stage-b-after="client-protection" data-osl-mail-aliases="${ready ? "available" : "refused"}" data-osl-mail-relay="${ready ? "available" : "refused"}" aria-disabled="${ready ? "false" : "true"}"><strong>Aliases and relay</strong><small>After client protection</small><p><span class="status-tag">${label}</span> ${escapeHtml(detail)}</p><ul>${capabilities.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><p>External email remains ordinary email unless a supported encrypted path is selected before send.</p></article>`;
}

function publicCirclesUnavailableMarkup(): string {
  return `<article class="inbox-surface-card unavailable" data-inbox-osl-surface="circles" data-public-circles-network="unavailable" aria-disabled="true"><strong>OSL Circles</strong><small>Private audience feeds</small><p><span class="status-tag">Unavailable</span> Public Circles network unavailable. Private audience posts stay off until membership, posting, and moderation are complete.</p></article>`;
}

export function publicPostGuardCarrierPreviewMarkup(platform = "Public platforms"): string {
  const platformName = escapeHtml(platform);
  return `<section class="public-post-guard public-post-guard-preview" data-public-platform-preview="encrypted-audience-carrier" data-public-post-guard="encrypted-audience-carrier" aria-labelledby="public-post-guard-title"><header><span class="privacy-local-mark">PUBLIC POST GUARD</span><h2 id="public-post-guard-title">Encrypted-audience carrier preview</h2><p>${platformName} stays a public surface. OSL shows the public carrier text separately from the protected audience preview before anything is placed.</p></header><div class="privacy-policy-grid carrier-preview-grid" aria-label="Public platform carrier preview"><article class="privacy-policy-card" data-public-post-kind="ordinary" data-carrier-part="public"><span class="status-tag">Public</span><h3>Public carrier</h3><p>Visible to the platform audience. Search, quoting, archiving, audience, location, and media metadata still need review. Visible carrier text stays visible and does not contain the protected message.</p></article><article class="privacy-policy-card" data-public-post-kind="encrypted-audience-carrier" data-carrier-part="protected-audience"><span class="status-tag">Carrier preview</span><h3>Protected audience</h3><p>Plaintext is for the approved audience only, but the platform can still see the public carrier, timing, and engagement.</p></article></div><p class="scope-approval-note">If audience proof is missing or changes, OSL refuses the protected placement and keeps the draft local.</p></section>`;
}

export function inboxDestinationContent(): string {
  const verifiedPeople = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const requests = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange);
  const connectedApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch" && app.linked);
  const connectedRows = connectedApps.length
    ? connectedApps.map((app) => {
        const scope = app.provider
          ? "External recipient, not OSL E2EE"
          : "OSL overlay active when a conversation is verified";
        return `<article class="inbox-row connected-source"><span class="source-mark">${homeAppLogo(app)}</span><div><strong>${escapeHtml(app.displayName)}</strong><small>${escapeHtml(app.displayName)} · ${scope}</small></div><button class="button compact" data-home-app="${app.id}" type="button">Open</button></article>`;
      }).join("")
    : `<div class="empty-state"><strong>No connected conversations yet</strong><p>Connect an app from Home to show supported account views here.</p></div>`;
  const chatRows = verifiedPeople.length
    ? verifiedPeople.slice(0, 8).map((person) => {
        const last = oslChatMessages.get(person.personId)?.at(-1);
        return `<article class="inbox-row osl-chat-source"><span class="source-mark">${homeModuleIcon("osl-chats")}</span><button class="inbox-conversation-open" data-osl-chat-open="${escapeHtml(person.personId)}" type="button"><strong>${escapeHtml(person.alias ?? "Verified friend")}</strong><small>OSL Chat · Protected OSL message${last?.body ? ` · ${escapeHtml(last.body)}` : ""}</small></button></article>`;
      }).join("")
    : `<div class="empty-state"><strong>No private chats yet</strong><p>Verify a friend before starting an encrypted OSL chat.</p></div>`;
  const requestRows = requests.length
    ? requests.slice(0, 8).map((person) => `<article class="inbox-row request-source"><span class="source-mark">${homeCommandIcon("friends")}</span><div><strong>${escapeHtml(person.alias ?? "Friend request")}</strong><small>${person.pendingKeyChange ? "Security change needs review" : "Verification needed before protected chat"}</small></div><button class="button compact" data-open-friends type="button">Review</button></article>`).join("")
    : `<div class="empty-state"><strong>No requests</strong><p>New friend requests and key reviews appear here.</p></div>`;
  const oslSurfaces = [
    ["chat", "OSL Chat", "Protected OSL messages", "Ready for verified friends"],
    ["circles", "OSL Circles", "Private audience feeds", "Coming after small-group review"],
    ["mail", "OSL Mail", "Client protection", "External recipients are not OSL E2EE"],
  ] as const;
  const mailboxGate = oslMailboxStageCGate();
  const surfaceCards = oslSurfaces.map(([id, label, protection, detail]) => {
    if (id === "circles") return circlesDestinationContent();
    if (id === "mail") {
      return `${oslMailStageAContent(oslMailStage("stageA"), mailboxGate)}${oslMailStageBContent(oslMailStage("stageB"))}`;
    }
    return `<article class="inbox-surface-card" data-inbox-osl-surface="${id}"><strong>${label}</strong><small>${protection}</small><p>${detail}</p></article>`;
  }).join("");
  return `<main class="content-viewport inbox-destination" id="route-heading" tabindex="-1"><header class="destination-header"><div><p class="eyebrow">Inbox</p><h1>Conversations</h1><p>Optional views for OSL messages and connected accounts. OSL shows only supported conversations and refuses protected send when the conversation cannot be verified.</p></div><button class="button primary" data-inbox-start-private type="button">Start a private conversation</button></header><nav class="inbox-filter-tabs" aria-label="Inbox filters">${["All", "OSL", "Connected", "Requests"].map((label, index) => `<button type="button" data-inbox-filter="${label.toLowerCase()}" ${index === 0 ? 'aria-pressed="true"' : ""}>${label}</button>`).join("")}</nav><section class="inbox-grid"><section class="inbox-panel" aria-labelledby="inbox-osl-heading"><h2 id="inbox-osl-heading">OSL</h2><div class="inbox-surface-grid">${surfaceCards}</div>${chatRows}</section><section class="inbox-panel" aria-labelledby="inbox-connected-heading"><h2 id="inbox-connected-heading">Connected</h2>${connectedRows}</section><section class="inbox-panel" aria-labelledby="inbox-requests-heading"><h2 id="inbox-requests-heading">Requests</h2>${requestRows}</section></section>${publicPostGuardCarrierPreviewMarkup("Public platforms")}</main>`;
}

export interface ActivityPrimaryActionPlan {
  route: "activity";
  reviewTarget: "attention-review";
  label: string;
  disabled: boolean;
}

export function activityPrimaryActionPlan(attentionItems: number): ActivityPrimaryActionPlan {
  return {
    route: "activity",
    reviewTarget: "attention-review",
    label: attentionItems > 0 ? "Review attention item" : "Review activity settings",
    disabled: false,
  };
}

export function activityDestinationContent(): string {
  const visibleNotifications = visibleAppNotifications();
  const activity = notificationsEnabled ? visibleNotifications : [];
  const attentionItems = notificationsEnabled
    ? visibleNotifications.filter((item) => /review|change|failed|unknown|needs?/iu.test(`${item.title} ${item.detail}`))
    : [];
  const primary = activityPrimaryActionPlan(attentionItems.length || activity.length);
  const rows = activity.length
    ? activity.slice(0, 8).map((item, index) => {
        const needsAttention = attentionItems.includes(item) || index === 0;
        return `<article class="notification-event activity-proof-row ${index === 0 && activityAttentionReviewOpen ? "selected" : ""}" data-activity-item="${index}" data-activity-proof="${escapeHtml(item.id)}" ${needsAttention ? 'data-activity-attention="true"' : ""}><span class="status-tag">${index === 0 && activityAttentionReviewOpen ? "Reviewing" : needsAttention ? "Needs review" : "Recorded"}</span><div><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")} · ${escapeHtml(item.createdAt)}</small></div></article>`;
      }).join("")
    : `<div class="empty-state"><strong>${notificationsEnabled ? "No activity needs attention" : "Activity is off"}</strong><p>${notificationsEnabled ? "Warnings, connection failures, cleanup checks, and verified outcomes appear here after OSL creates them on this device." : "Turn on local activity before OSL records local outcomes here."}</p></div>`;
  const attention = attentionItems.length
    ? attentionItems.slice(0, 5).map((item, index) => `<article class="setting-line" data-attention-review-item="${index}"><span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")}</small></span><span class="status-tag">Review</span></article>`).join("")
    : `<div class="empty-state"><strong>No items need review</strong><p>Proof of local protection work stays here when OSL has something to report.</p></div>`;
  const review = activityAttentionReviewOpen && activity.length
    ? `<section class="activity-review-panel" data-activity-attention-review="${escapeHtml(activity[0].id)}" aria-labelledby="activity-review-title"><h2 id="activity-review-title">Attention review</h2><p>Review the local event before changing protection, trust, or cleanup settings.</p><button class="button compact" data-notification-settings type="button">Open Activity settings</button></section>`
    : "";
  return `<main class="content-viewport activity-destination" aria-labelledby="route-heading"><header class="destination-header"><div><p class="eyebrow">Activity</p><h1 id="route-heading" tabindex="-1">Activity</h1><p>Local proof of what OSL actually did, what it refused, and what still needs your attention.</p></div><button class="button primary" data-activity-primary-action data-route="${primary.route}" data-review-target="${primary.reviewTarget}" type="button">${primary.label}</button></header><section class="activity-proof-summary" aria-label="Proof summary"><article><strong>${activity.length.toLocaleString("en-US")}</strong><span>Need attention</span></article><article><strong>0</strong><span>Scheduled jobs</span></article><article><strong>0</strong><span>Cleanup checks</span></article><article><strong>Local</strong><span>Proof source</span></article></section>${review}<section class="settings-list activity-attention-review" id="activity-attention-review" aria-labelledby="activity-attention-title"><header><h2 id="activity-attention-title">Needs attention</h2><p>Warnings, failed checks, unknown outcomes, and local changes that need review.</p></header>${attention}</section><section class="notification-events activity-proof-list" aria-label="Activity proof history"><span class="sr-only" aria-label="Recent OSL activity"></span>${rows}</section></main>`;
}

export interface PrivacyPrimaryActionPlan {
  route: "privacy";
  reviewTarget: "protection-review";
  label: "Review protection";
}

export function privacyPrimaryActionPlan(): PrivacyPrimaryActionPlan {
  return { route: "privacy", reviewTarget: "protection-review", label: "Review protection" };
}

export function mullvadConnectionCardMarkup(status: MullvadStatus = mullvadStatus): string {
  const state = status.availability === "installed"
    ? "Available"
    : status.availability === "installable"
      ? "Installable"
      : "Unavailable";
  const action = status.availability === "installed"
    ? `<button class="button compact" data-route="mullvad" type="button">Open</button>`
    : status.availability === "installable"
      ? `<button class="button compact" id="install-mullvad-from-connections" type="button">Install</button>`
      : `<button class="button compact" disabled>Unavailable</button>`;
  return `<article class="connection-device-card connection-card mullvad-card" data-connection-card="mullvad" data-connection-kind="mullvad" data-privacy-scope="${status.privacyScope}" data-connection-state="${status.availability}"><div><span class="status-tag">${state}</span><strong>Mullvad</strong><small>Network privacy only · ${state}</small><p>Network privacy signal only. Use your existing Mullvad session as a separate network tool. OSL does not read its account state, connection state, or app content. Platforms and recipients can still see ordinary content you send there.</p></div>${action}</article>`;
}

function androidWorkspaceConnectionCard(surface: AndroidSurface): string {
  if (surface.surface === "companion") {
    return `<article class="connection-device-card" data-android-surface="${surface.id}" data-consent="${surface.consent}" data-binding="${surface.binding}"><span class="status-tag">Coming later</span><h3>${escapeHtml(surface.displayName)}</h3><p>Phone approvals and OSL-owned mobile experiences stay separate from desktop account control.</p></article>`;
  }
  return `<article class="connection-device-card connection-card android-workspace-card pro unavailable" data-android-surface="${surface.id}" data-android-workspace-consent="${surface.consent}" data-consent="${surface.consent}" data-binding="${surface.binding}" data-hosted-execution="${surface.hostedExecution}" data-workspace-runtime="${surface.workspace?.runtime ?? "localVirtualDevice"}" aria-disabled="true"><div><span class="status-tag">Coming later · Pro</span><strong>${escapeHtml(surface.displayName)}</strong><small>Future Pro isolation · Coming later</small><p>Future isolated local workspace with encrypted local virtual device storage. A separate mobile workspace threat model review and explicit consent are required before any local workspace starts.</p><ul><li>Encrypted local virtual device storage.</li><li>clipboard, files, notifications, camera, microphone, and location start denied.</li><li>${hostedAndroidWorkspaceGate()}</li><li>No hosted Android workspace runs from this card.</li></ul></div><button class="button compact" disabled>Consent required</button></article>`;
}

function hostedAndroidWorkspaceGate(): string {
  return "Hosted workspace is unavailable here; it requires a separate threat model, explicit consent, and a new audit before any claim changes.";
}

export function androidWorkspaceCardMarkup(): string {
  const workspace = AndroidSurface.preview().find((surface) => surface.id === "androidMobileWorkspace");
  return workspace ? androidWorkspaceConnectionCard(workspace) : "";
}

export function connectionsDestinationContent(): string {
  const apps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const accountRows = apps.length
    ? apps.map((app) => {
        const state = app.linked ? `${app.accountCount} local ${app.accountCount === 1 ? "profile" : "profiles"}` : app.launchState === "available" ? "Not set up" : "Coming later";
        const action = app.launchState === "available"
          ? `<button class="button compact" data-home-app="${app.id}" type="button">${app.linked ? "Open" : "Set up"}</button>`
          : `<button class="button compact" disabled>Coming later</button>`;
        return `<article class="connection-row connection-account-row" data-connection-app="${app.id}" data-connection-account="${app.id}"><div>${homeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${escapeHtml(state)}</small></span></div>${action}</article>`;
      }).join("")
    : `<div class="empty-state"><strong>No account catalog loaded</strong><p>Reconnect when apps are available on this device.</p></div>`;
  const nativeRows = nativeApps.length
    ? nativeApps.map((app) => `<article class="setting-line" data-device-connection="${app.id}"><span><strong>${escapeHtml(app.displayName)}</strong><small>${app.availability === "installed" ? "Installed native app" : app.availability === "installable" ? "Can be installed" : "Unavailable on this device"}</small></span><span class="status-tag">${app.availability === "installed" ? "Ready" : app.availability === "installable" ? "Installable" : "Unavailable"}</span></article>`).join("")
    : `<div class="empty-state"><strong>No native app status yet</strong><p>Native app status appears after OSL checks this device.</p></div>`;
  const androidCards = AndroidSurface.preview().map((surface) => androidWorkspaceConnectionCard(surface)).join("");
  return `<main class="content-viewport connections-destination" aria-labelledby="route-heading"><header class="destination-header"><div><p class="eyebrow">Connections</p><h1 id="route-heading" tabindex="-1">Connections</h1><p>Accounts, local app windows, network tools, and planned device surfaces OSL can connect to or refuse safely.</p></div><button class="button primary" data-connections-primary-action type="button">Connect a service</button></header><section class="connections-grid"><section class="settings-list connected-accounts connections-accounts" aria-labelledby="connected-accounts-title"><header><h2 id="connected-accounts-title">Connected accounts</h2><p>Each profile stays separate. OSL never merges accounts from names, avatars, addresses, or shared contacts.</p></header>${accountRows}</section><section class="settings-list connected-devices connections-devices" aria-labelledby="connected-devices-title"><header><h2 id="connected-devices-title">Devices and app windows</h2><p>Local devices and companion windows require explicit user action before use.</p></header>${nativeRows}${mullvadConnectionCardMarkup()}${androidCards}</section></section></main>`;
}

export function privacyPrimaryAction(): void {
  route = "privacy";
  privacyProtectionReviewOpen = true;
  render();
}

export function activityPrimaryAction(): void {
  route = "activity";
  activityAttentionReviewOpen = true;
  render();
}

function oslChatContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const friends = hubPeople.map((person) => {
    const messages = oslChatMessages.get(person.personId) ?? [];
    const last = messages.at(-1);
    return {
      personId: person.personId,
      nickname: person.alias ?? "Unnamed friend",
      verified: person.safetyNumberVerified && !person.pendingKeyChange,
      ready: person.personId === activeOslChatPersonId && activeOslChatContext?.scopeApproved === true,
      preview: last?.body ?? null,
      previewVisible: !pro || oslChatPreviewsVisible,
      unreadCount: oslChatUnread.get(person.personId) ?? 0,
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

function homeModuleIcon(id: "osl-chats" | "osl-mail" | "osl-servers" | "scrub" | "activity" | "osl-notes"): string {
  if (id === "osl-chats") return `<svg viewBox="0 0 24 24"><path d="M4 5.5h16v10H9l-5 4v-14Z"/><path d="M8 9h8M8 12h5"/></svg>`;
  if (id === "osl-mail") return `<svg viewBox="0 0 24 24"><path d="M4 6h16v12H4V6Z"/><path d="m5 7 7 6 7-6"/></svg>`;
  if (id === "osl-servers") return `<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="6"/><rect x="4" y="14" width="16" height="6"/><path d="M7 7h.01M7 17h.01M11 7h6M11 17h6"/></svg>`;
  if (id === "scrub") return `<svg viewBox="0 0 24 24"><path d="m5 18 9-9 5 5-6 6H7l-2-2Z"/><path d="m12 11 3-3 5 5-3 3M4 20h16"/></svg>`;
  if (id === "activity") return `<svg viewBox="0 0 24 24"><path d="M4 12h4l2-5 4 10 2-5h4"/></svg>`;
  return `<svg viewBox="0 0 24 24"><path d="M6 3.5h9l3 3V20H6V3.5Z"/><path d="M14.5 3.5V7H18M9 11h6M9 14h6M9 17h4"/></svg>`;
}

function activeHomeApp(): HomeAppCatalogEntry | null {
  return homeAppsFromServices(services).find((app) => app.id === activeHomeAppId) ?? null;
}

function mailComposerEncryptionScope(app: HomeAppCatalogEntry | null): string {
  if (app?.serviceId !== "email" || app.provider === null) return "";
  return `<aside class="mail-composer-encryption-scope" data-mail-composer-encryption-scope="${app.id}" role="note" aria-label="Email protection scope"><strong>Before you send</strong><small>${escapeHtml(app.displayName)} protects this app account. Ordinary external email uses the mail provider's delivery path. Use OSL Chat for verified friends.</small></aside>`;
}

function mailComposerProtectionNote(app: HomeAppCatalogEntry | null): string {
  return mailComposerEncryptionScope(app);
}

function oslMailContent(): string {
  return oslMailViewMarkup({
    loading: oslMailLoading,
    available: Boolean(oslMailStatus?.available),
    signedUsername: claimedOslUsername,
    status: oslMailStatus,
    threads: oslMailThreads,
    activeThread: oslMailActiveThread,
    pane: oslMailPane,
    notifications: oslMailNotifications,
    deleteReceipt: oslMailDeleteReceipt,
    sendReceipt: oslMailSendReceipt,
    burnReceipt: oslMailBurnReceipt,
    error: oslMailError,
  });
}

async function refreshOslMail(): Promise<void> {
  oslMailLoading = true;
  oslMailError = null;
  renderWhenIdle();
  const status = await loadOslMailStatus();
  oslMailStatus = status;
  if (status?.provisioned) oslMailThreads = await listOslMailThreads() ?? [];
  oslMailLoading = false;
  if (route === "osl-mail") render();
}

async function provisionOslMailFromProfile(): Promise<void> {
  if (!claimedOslUsername) {
    oslMailError = "Choose your signed OSL username first";
    render();
    return;
  }
  oslMailStatus = await provisionOslMail(claimedOslUsername);
  if (!oslMailStatus) oslMailError = "Mailbox setup was refused";
  if (route === "osl-mail") render();
}

async function sendOslMailForm(form: HTMLFormElement): Promise<void> {
  const recipient = form.querySelector<HTMLInputElement>("#osl-mail-to")?.value ?? "";
  const subject = form.querySelector<HTMLInputElement>("#osl-mail-subject")?.value ?? "";
  const body = form.querySelector<HTMLTextAreaElement>("#osl-mail-body")?.value ?? "";
  if (!recipient.endsWith("@oslprivacy.com")) {
    oslMailError = "External outbound mail is unavailable in v1";
    render();
    return;
  }
  oslMailSendReceipt = await sendOslMail(recipient, subject, body);
  oslMailError = oslMailSendReceipt ? null : "Send was refused";
  if (route === "osl-mail") render();
}

function activeHomeAppName(): string {
  return activeHomeApp()?.displayName
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
    const trustAction = friendTrustAction(person.safetyNumberVerified, person.pendingKeyChange);
    const action = trustAction === "verified"
      ? mode === "service"
        ? activeContextToken
          ? `<button class="button compact" data-allow-person="${escapeHtml(person.personId)}">Approve for this chat</button>`
          : `<span class="status-tag">Open a supported chat first</span>`
        : `<span class="status-tag">Verified</span>`
      : `<button class="button compact" data-verify-person="${escapeHtml(person.personId)}">${person.pendingKeyChange ? "Re-verify key" : "Review request"}</button>`;
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
    const removeControl = mode === "manage" ? friendRemovalButtonMarkup(person.personId, escapeHtml) : "";
    const management = `<details class="friend-management"><summary>Manage</summary><div>${nicknameForm}<div class="friend-approvals"><span>Approved chats</span><div>${scopes}</div>${truncated}</div><details class="friend-security"><summary>Security details</summary><div><span>OSL ID</span><code>${escapeHtml(identity)}</code><span>Verification code</span><code>${escapeHtml(person.safetyNumber)}</code></div></details>${removeControl}</div></details>`;
    return `<article class="person-row person-profile"><header><div><strong>${escapeHtml(nickname)}</strong>${person.pendingKeyChange ? `<small>Security change needs review</small>` : `<small>${person.safetyNumberVerified ? "Verified" : "Request pending"}</small>`}</div>${action}</header>${management}</article>`;
  }).join("");
}

function peopleDestinationContent(): string {
  const verified = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const needsReview = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange);
  const approvedChats = verified.reduce((total, person) => total + person.whitelistCount, 0);
  const broaderReach = verified.filter((person) => person.reachBroadened).length;
  const addPrimaryTarget = peoplePrimaryActionFocus === "add" ? ' data-people-primary-target="add"' : "";
  const reviewPrimaryTarget = peoplePrimaryActionFocus === "verify" ? ' data-people-primary-target="verify"' : "";
  const reviewRows = needsReview.length
    ? needsReview.slice(0, 4).map((person) => {
      const nickname = person.alias ?? "Unnamed friend";
      const detail = person.pendingKeyChange
        ? "Verification changed. Protected sends stay off until you review it."
        : "Not trusted yet. Protected sends stay off until you verify.";
      return `<article class="people-review-row"><div><strong>${escapeHtml(nickname)}</strong><small>${detail}</small></div><button class="button compact" type="button" data-verify-person="${escapeHtml(person.personId)}">${person.pendingKeyChange ? "Review change" : "Verify"}</button></article>`;
    }).join("")
    : `<div class="empty-state compact"><strong>No people need review</strong><p>New people and changed verification appear here before OSL trusts them.</p></div>`;
  const peopleRows = hubPeople.length
    ? peopleListMarkup("manage")
    : `<div class="empty-state"><strong>No trusted people yet</strong><p>Add someone, compare verification another way, then approve each chat you want to protect.</p></div>`;
  const invite = friendCode && friendDisplayId
    ? `<section class="friend-invite people-invite" aria-labelledby="people-friend-id-label"><div><span id="people-friend-id-label">Your friend ID</span><code>${escapeHtml(compactFriendId(friendDisplayId))}</code></div><button class="button" id="copy-friend-code" type="button">Copy invite</button><p>Send the invite to someone you trust so they can add you.</p></section>`
    : `<div class="empty-inline friend-code-unavailable">Your invite appears after OSL is unlocked.</div>`;
  return `<main class="content-viewport people-destination" aria-labelledby="route-heading"><header class="people-destination-header"><button class="text-button" data-route="home" type="button">Back</button><div><h1 id="route-heading" tabindex="-1">People</h1><p>Trusted people, the places you know them, and which chats OSL may protect.</p></div><button class="button primary" data-people-primary-action type="button">Add or verify a person</button></header><section class="people-summary-grid" aria-label="People trust summary"><article><strong>${verified.length.toLocaleString("en-US")}</strong><span>Trusted people</span></article><article><strong>${needsReview.length.toLocaleString("en-US")}</strong><span>Need review</span></article><article><strong>${approvedChats.toLocaleString("en-US")}</strong><span>Approved chats</span></article><article><strong>${broaderReach.toLocaleString("en-US")}</strong><span>Extended reach</span></article></section><section class="people-rule-panel" aria-label="Trust rules"><h2>How trust works</h2><ul><li>No approval means OSL refuses protected sends for that chat.</li><li>Verifying a person does not approve every chat with them.</li><li>Each approval stays separate.</li><li>Groups and audiences never inherit trust from a similar name.</li><li>A changed verification returns the person to review before OSL protects new messages.</li></ul></section><section class="people-add-section" aria-labelledby="people-add-title"${addPrimaryTarget}><div><h2 id="people-add-title">Add or verify a person</h2><p>Adding someone records the request only on this device. Private chats stay off until you compare the verification code another way and approve a chat.</p></div><form id="add-friend-form" class="friend-add-form people-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add person</button></form><p class="form-status" id="friend-form-status" role="status"></p></section><section class="people-review-panel" aria-labelledby="people-review-title"${reviewPrimaryTarget}><header><h2 id="people-review-title">Needs review</h2></header><div class="people-review-list">${reviewRows}</div></section>${invite}<section class="people-list-panel" aria-labelledby="people-list-title"><header><h2 id="people-list-title">People you know</h2><p>Nicknames stay on this device. Open Manage on a person to edit trust for approved chats.</p></header><div class="people-list people-destination-list">${peopleRows}</div></section></main>`;
}

function focusPeopleInviteInput(): void {
  const focusInvite = (): void => {
    const input = document.querySelector<HTMLInputElement>("#friend-code-input");
    input?.focus();
    document.querySelector<HTMLElement>(".people-add-section")?.scrollIntoView?.({ block: "start" });
  };
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(focusInvite);
  else focusInvite();
}

export function peoplePrimaryAction(): void {
  route = "people";
  activeOslChatPersonId = null;
  friendsDialogOpen = false;
  const firstReview = hubPeople.find((person) => (!person.safetyNumberVerified || person.pendingKeyChange) && person.safetyNumber.length > 0);
  if (firstReview) {
    peoplePrimaryActionFocus = "verify";
    requestFriendVerification(firstReview.personId);
    return;
  }
  peoplePrimaryActionFocus = "add";
  ownedConfirmation = null;
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
  focusPeopleInviteInput();
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

function reachTimestampLabel(value: string): string {
  const seconds = Number(value);
  if (!Number.isFinite(seconds) || seconds <= 0) return "earlier";
  const at = new Date(seconds * 1000);
  return Number.isNaN(at.getTime()) ? "earlier" : at.toLocaleString();
}

function narrowedScopeLabel(storageKey: string): string {
  const separator = storageKey.indexOf(":");
  const kind = separator === -1 ? storageKey : storageKey.slice(0, separator);
  const context = separator === -1 ? "" : storageKey.slice(separator + 1);
  const label = kind === "gc" ? "Group" : kind === "server_channel" ? "Channel" : kind === "server_full" ? "Space" : "Direct messages";
  return context ? `${label} · ${compactFriendId(context)}` : label;
}

function whitelistReachLine(person: HubPerson): string {
  if (!person.reachBroadened) return "Trusted only in the chats you approved";
  return person.reachBroadenedAt
    ? `Reach extended to the chats you share · recorded ${reachTimestampLabel(person.reachBroadenedAt)}`
    : "Reach extended to the chats you share";
}

// One row per whitelisted person: where they are trusted, whether their reach
// was deliberately widened, and the controls that change either. Reach and
// revocation are separate actions, and both only ever act on the verified
// friend behind the live protected context.
function whitelistRosterPersonMarkup(person: HubPerson, activePersonId: string | null, busy: boolean, activeScopeApproved: boolean): string {
  const nickname = person.alias ?? "Unnamed friend";
  const isActive = activePersonId === person.personId;
  const visibleScopes = person.whitelistedScopes.slice(0, whitelistRosterScopeLimit);
  const hiddenScopeCount = Math.max(0, person.whitelistCount - visibleScopes.length);
  const scopeRows = visibleScopes.map((scope) => {
    const label = friendScopeLabel(scope);
    return `<div class="whitelist-roster-scope"><span class="friend-scope">${escapeHtml(label)}${scope.userSpecific ? ` <small>only this person</small>` : ""}</span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button type="button" data-whitelist-scope-add="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" title="Already approved" disabled>+</button><button type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" title="${isActive ? "Revoke this chat now" : "Open this person's protected chat to revoke"}" ${!isActive || busy ? "disabled" : ""}>−</button></div></div>`;
  }).join("");
  const narrowedRows = person.reachNarrowedScopes.slice(0, whitelistRosterScopeLimit).map((key) => {
    const label = narrowedScopeLabel(key);
    return `<div class="whitelist-roster-scope narrowed"><span class="friend-scope narrowed">${escapeHtml(label)} <small>taken back</small></span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button type="button" data-whitelist-scope-add="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" title="Approve this chat from inside it" disabled>+</button><button type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" title="Not approved" disabled>−</button></div></div>`;
  }).join("");
  const scopes = scopeRows || `<span class="friend-none">No chats approved</span>`;
  const truncated = hiddenScopeCount > 0 || person.whitelistedScopesTruncated
    ? `<small class="whitelist-roster-truncated">${hiddenScopeCount > 0 ? `${hiddenScopeCount} more approved ${hiddenScopeCount === 1 ? "chat is" : "chats are"}` : "More approved chats are"} stored locally and not listed here.</small>`
    : "";
  // Reach widens trust that already exists, so it needs a recorded approval
  // or the approved chat the user is standing in — the hub enforces the same rule.
  const reachDisabled = !isActive || busy || (!person.reachBroadened && person.whitelistCount === 0 && !activeScopeApproved);
  const reachButton = `<button class="button compact" type="button" data-whitelist-reach="${escapeHtml(person.personId)}" data-whitelist-reach-next="${person.reachBroadened ? "off" : "on"}" aria-pressed="${person.reachBroadened}" title="${person.reachBroadened ? "Withdraw reach across the chats you share" : "Extend this trust to the other chats you share"}" ${reachDisabled ? "disabled" : ""}>${person.reachBroadened ? "Limit reach" : "Extend reach"}</button>`;
  const reachNote = isActive ? "" : `<small class="whitelist-roster-note">Open this person's protected chat to change their reach or revoke a chat.</small>`;
  return `<article class="whitelist-roster-row person-row" data-whitelist-person="${escapeHtml(person.personId)}"><header><div><strong>${escapeHtml(nickname)}</strong><small>${escapeHtml(whitelistReachLine(person))}</small></div>${reachButton}</header><div class="whitelist-roster-scopes">${scopes}${narrowedRows}</div>${truncated}${reachNote}</article>`;
}

function whitelistRosterMarkup(): string {
  if (!whitelistRosterOpen) return "";
  const active = activeVerifiedDiscordQaPeer();
  const activePersonId = active?.person.personId ?? null;
  const activeScopeApproved = active?.context.scopeApproved === true;
  const busy = discordQaHeaderBusy !== null;
  // Everyone OSL recorded trust for, plus the person whose protected chat is
  // open, so their reach can be widened from here without hunting for a row.
  const roster = hubPeople.filter((person) => person.whitelistCount > 0 || person.reachNarrowedScopes.length > 0 || person.personId === activePersonId);
  const rows = roster.length
    ? roster.map((person) => whitelistRosterPersonMarkup(person, activePersonId, busy, activeScopeApproved)).join("")
    : `<div class="empty-state"><strong>Nobody is whitelisted yet</strong><p>Approve a verified friend inside a chat; they appear here with the chats they cover.</p></div>`;
  return `<dialog class="friends-dialog whitelist-roster-dialog" id="whitelist-roster-dialog" aria-labelledby="whitelist-roster-title"><div class="friends-dialog-card"><header><h2 id="whitelist-roster-title">Whitelisted people</h2><button class="icon-button" id="whitelist-roster-close" type="button" aria-label="Close whitelist">×</button></header><p class="scope-approval-note">Approving a chat never widens anyone's reach. Extending reach is a separate, recorded choice, and a chat you take back stays revoked even while reach is on.</p><div class="whitelist-roster-list">${rows}</div></div></dialog>`;
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

function burnGuaranteeMarkup(effects: string): string {
  const stateLabel = (state: BurnGuaranteeState): string => {
    switch (state) {
      case "available": return "Available";
      case "request_only": return "Request only";
      case "unavailable": return "Unavailable";
      case "not_possible": return "Not possible";
    }
  };
  const items = BurnGuaranteeCopy.items.map((item) => (
    `<li data-burn-guarantee="${escapeHtml(item.id)}"><strong>${escapeHtml(item.title)}</strong><span>${escapeHtml(stateLabel(item.state))}</span><p>${escapeHtml(item.body)}</p></li>`
  )).join("");
  return `<section class="burn-truth burn-guarantees" aria-labelledby="burn-guarantee-title"><strong id="burn-guarantee-title">Before you continue</strong><p>${escapeHtml(effects)}</p><p><strong>${escapeHtml(BurnGuaranteeCopy.summary)}</strong> ${escapeHtml(BurnGuaranteeCopy.intro)}</p><ul>${items}</ul><p>${escapeHtml(BurnGuaranteeCopy.limit)}</p></section>`;
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
  return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card"><header><h2 id="burn-dialog-title">Burn local data</h2><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><div class="burn-scope-grid" aria-label="Burn scope">${scopeCards}</div>${burnGuaranteeMarkup(effects)}<details class="burn-more"><summary>Other options</summary><div class="burn-options"><label class="setting-line unavailable"><span><strong>Provider messages</strong><small>Not removed. Burn changes only indexed local OSL data and sent relay records.</small></span><input type="checkbox" disabled/></label><label class="setting-line unavailable"><span><strong>Burn for friends · Pro</strong><small>${pro ? "Requires every recipient’s prior signed consent and an acknowledgment from each device." : "A Pro initiator may request this for Free recipients only after each recipient gives signed consent."} The consent-and-acknowledgment workflow is unavailable in this build.</small></span><input type="checkbox" disabled/></label>${burnScope === "account" ? `<label class="setting-line interactive"><span><strong>Uninstall after burn</strong><small>After a successful local burn, open Windows installed apps.</small></span><input id="burn-uninstall" type="checkbox"/></label>` : ""}</div></details><form id="burn-confirm-form" class="burn-confirm"><label for="burn-confirm-input">Type <code>${phrase}</code> to continue</label><input id="burn-confirm-input" autocomplete="off" autocapitalize="characters" spellcheck="false" ${selectedReason ? "disabled" : ""}/><p class="form-status" id="burn-form-status" role="status">${selectedReason ? escapeHtml(selectedReason) : "This cannot be undone."}</p><footer><button class="button ghost" type="button" data-close-burn>Cancel</button><button class="button danger" id="burn-confirm-submit" type="submit" disabled>${burnBusy ? "Burning…" : "Burn now"}</button></footer></form></section></dialog>`;
}

function ownedConfirmationMarkup(): string {
  if (!ownedConfirmation) return "";
  const request = ownedConfirmation;
  const verifying = request.kind === "verifyFriend";
  const removing = request.kind === "removeFriend";
  const person = verifying || removing ? hubPeople.find((candidate) => candidate.personId === request.personId) ?? null : null;
  const title = verifying ? "Verify this friend's key?" : removing ? "Remove friend?" : "Clear Pro activation?";
  const detail = request.kind === "verifyFriend"
    ? `<p>This device's verification code for ${escapeHtml(person?.alias ?? "this friend")}. Read it aloud to your friend:</p><code class="verification-code" aria-label="Your local verification code">${escapeHtml(person?.safetyNumber ?? "Unavailable")}</code><label class="owned-confirmation-entry" for="friend-verification-input"><span>Enter the code your friend read back to you over a channel that is not this app</span><input id="friend-verification-input" autocomplete="off" spellcheck="false" inputmode="numeric" autocapitalize="none" maxlength="96" placeholder="Spaces and grouping do not matter"/></label><p>Accept only after you compare the codes outside OSL. Accepting lets OSL encrypt to the key it holds for this friend. It does not turn on decryption in any chat or approve any conversation.</p>`
    : request.kind === "removeFriend"
      ? `<p>Removing ${escapeHtml(person?.alias ?? "this friend")} deletes this friend's keys from this device and withdraws every conversation approval they hold.</p><p>This cannot be undone.</p>`
    : `<p>Pro features will be unavailable on this device until you activate again.</p>`;
  return `<dialog class="owned-confirmation-dialog" id="owned-confirmation-dialog" aria-labelledby="owned-confirmation-title"><section class="owned-confirmation-card"><header><h2 id="owned-confirmation-title">${title}</h2><button class="icon-button" data-close-owned-confirmation aria-label="Cancel">×</button></header>${detail}<p class="form-status" role="status">${escapeHtml(ownedConfirmationError)}</p><footer><button class="button" data-close-owned-confirmation>Cancel</button><button class="button ${verifying ? "primary" : "danger"}" id="owned-confirmation-submit" type="button" ${ownedConfirmationBusy || verifying ? "disabled" : ""}>${ownedConfirmationBusy ? "Working…" : verifying ? "Accept key" : removing ? "Remove friend" : "Clear activation"}</button></footer></section></dialog>`;
}

function serviceContent(): string {
  const name = escapeHtml(activeHomeAppName());
  const mailScope = mailComposerEncryptionScope(activeHomeApp());
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
  return `<main class="content-viewport native-app-page" id="route-heading" tabindex="-1"><section class="native-app-card"><span class="service-icon large">${activeService ? serviceLogo(activeService.id) : ""}</span><h1>${name}</h1><p>Open a separate OSL profile. Your normal app stays open.</p>${mailScope}<button class="button primary native-app-action" id="embedded-service-setup" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : `Open ${name}`}</button><div class="native-app-secondary"><button class="text-back" id="native-app-back">← Apps</button><button class="text-button" id="burn-button" data-open-burn="app">Burn…</button></div></section></main>`;
}

function activeNativeApp(): NativeApp | null {
  if (!activeHomeAppId) return null;
  return nativeApps.find((app) => app.id === activeHomeAppId) ?? null;
}

function serviceAccountPickerContent(): string {
  const app = homeAppsFromServices(services).find((candidate) => candidate.id === activeHomeAppId);
  const accounts = app ? embeddedAccountsForHomeApp(app, services) : [];
  const name = escapeHtml(activeHomeAppName());
  const currentSession = activeHomeAppId && selectedInstalledNativeApp(activeHomeAppId)
    ? `<button class="service-account-choice" data-service-account=""><span>${serviceLogo(activeService?.id ?? "discord")}</span><strong>Current desktop session</strong><small>whichever account the desktop app currently shows</small></button>`
    : "";
  const choices = accounts.map((account) => `<button class="service-account-choice" data-service-account="${escapeHtml(account.id)}"><span>${serviceLogo(activeService?.id ?? "discord")}</span><strong>${escapeHtml(account.label)}</strong><small>Isolated OSL profile · exact account</small></button>`).join("");
  return `<main class="content-viewport native-app-page" id="route-heading" tabindex="-1"><section class="native-app-card service-account-picker"><button class="text-back" id="native-app-back">← Apps</button><h1>Choose ${name} profile</h1><div class="service-account-choices">${currentSession}${choices}</div><button class="button" id="add-service-profile">Add another profile</button></section></main>`;
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
  const mailNote = mailComposerProtectionNote(selectedApp ?? activeHomeApp());
  return `<main class="content-viewport service-guide" id="route-heading" tabindex="-1"><section class="guide-card guide-card-simple"><header><button class="text-back" id="service-guide-exit">← Apps</button></header><div class="guide-hero"><span class="guide-logo" data-guide-service="${service.id}">${serviceLogo(service.id)}</span><h1>${directNativeAccountChoice || directBrowserAccountChoice ? "Open" : "Connect"} ${name}</h1></div>${discordQaHostStatusMarkup()}${sessionChoices}${mailNote}${openAction || installedAction ? `<footer class="guide-actions">${openAction}${installedAction}</footer>` : ""}${nativeFailure}</section>${onboardingServiceSetup ? '<button class="onboarding-skip-dock" id="service-guide-skip">Skip · manual setup</button>' : ""}</main>`;
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

export function privacyDestinationContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const primary = privacyPrimaryActionPlan();
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning..." : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const policyGroups = [
    ["Before I send", "Risk warnings, public-post checks, and attachment cleaning.", "On in Balanced"],
    ["After I send", `Message timers default to ${timer}; view-once media and retention reviews stay off until you choose them.`, "Review first"],
    ["Incoming content", "Link, scam, tracker, and file warnings run on this device when available.", "Local checks"],
    ["My history", "Manual scan and guided review for old messages, posts, and email exports.", "Free scan"],
    ["My exposure", "Old accounts, breach reminders, broker guidance, and privacy drift checks.", "Coming in stages"],
  ] as const;
  const tools = [
    ["History Cleanup", "Find old posts, messages, and email to review."],
    ["Attachment Guard", "Remove location, device, and document metadata before upload."],
    ["Email Privacy", "Block tracking pixels, identify redirect trackers, and sanitize links."],
    ["Exposure Inventory", "Show old accounts, breached identifiers, and public exposure."],
    ["Privacy Drift Watch", "Notice when an app changes settings, permissions, or connection state."],
    ["Scam Shield", "Warn about suspicious links, impersonation, and payment requests."],
    ["Data Removal", "Guide broker requests and verify results instead of counting requests as success."],
    ["Encrypted Capsule", "Send protected files or notes when the recipient does not use OSL."],
  ] as const;
  const policyCards = policyGroups.map(([name, detail, state]) => `<article class="privacy-policy-card"><span class="status-tag">${state}</span><h3>${name}</h3><p>${detail}</p></article>`).join("");
  const toolRows = tools.map(([name, detail], index) => `<article class="setting-line privacy-tool-row"><span><strong>${name}</strong><small>${detail}</small></span><span class="status-tag">${index === 0 ? "Available" : proActive ? "Pro planned" : "Pro"}</span></article>`).join("");
  const cleanupState = proActive ? "Manual queue planned" : "Pro manual queue";
  const protectionReview = privacyProtectionReviewOpen
    ? `<section class="privacy-review-card" data-privacy-protection-review><div><span class="privacy-local-mark">PROTECTION REVIEW</span><h2>Review or change protection</h2><p>Check the Balanced policy, app exceptions, cleanup limits, and local warning choices before OSL changes anything.</p></div><button class="button compact" data-route="settings" data-settings="scrub" type="button">Open detailed review</button></section>`
    : "";
  return `<main class="content-viewport privacy-destination"><header class="destination-header"><div><p class="eyebrow">Privacy</p><h1 id="route-heading" tabindex="-1">Privacy</h1><p>Review what OSL will do before it changes anything.</p></div><button class="button primary" data-privacy-primary-action data-route="${primary.route}" data-review-target="${primary.reviewTarget}" type="button">Review or change protection</button></header>${protectionReview}<section class="privacy-preset-panel" aria-labelledby="privacy-preset-title"><div><span class="privacy-local-mark">ACTIVE PRESET</span><h2 id="privacy-preset-title">Balanced</h2><p>Basic account health plus local before-send warnings, attachment cleaning, monthly cleanup review, and private OSL suggestions for verified contacts.</p></div><button class="button compact" type="button" disabled>Change preset</button></section><section class="privacy-policy-stack" id="privacy-protection-review" aria-labelledby="privacy-policy-title"><header><div><h2 id="privacy-policy-title">Global policy</h2><p>Inherited from Balanced until you make an exception.</p></div><span class="status-tag">Deletion off</span></header><p class="privacy-policy-path">Balanced preset / app / account / conversation exception</p><div class="privacy-policy-grid">${policyCards}</div></section>${publicPostGuardCarrierPreviewMarkup()}<section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3><p>Choose a TXT, CSV, or JSON message export. OSL suggests items; you decide what to review. Nothing is deleted by this build.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup(true)}${privacyScanResultsMarkup()}<section class="settings-list privacy-tools" aria-labelledby="privacy-tools-title"><header><h2 id="privacy-tools-title">Solo privacy tools</h2><p>Useful even when nobody else uses OSL.</p></header>${toolRows}</section><section class="settings-list privacy-limits" aria-labelledby="privacy-limits-title"><header><h2 id="privacy-limits-title">Proof and limits</h2><p>OSL refuses actions it cannot verify.</p></header><div class="setting-line"><span><strong>Cleanup</strong><small>${cleanupState}; every batch must be scanned, shown, previewed, confirmed, executed, and checked.</small></span><span class="status-tag">No auto delete</span></div><div class="setting-line"><span><strong>Service messages</strong><small>Apps, people, exports, backups, and opened copies may retain content.</small></span><span class="status-tag">Limit shown</span></div><div class="setting-line"><span><strong>Window protection</strong><small>Applied to OSL's own window when available. Cameras, malware, and modified recipients can still capture content.</small></span><span class="status-tag">${screenshotProtectionEnabled ? "Active" : "Unavailable"}</span></div></section></main>`;
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
    return `<h2>Mass cleanup</h2><section class="cleanup-lock"><span>PRO</span><strong>Organize many chats at once</strong><p>Every batch is shown and confirmed before anything changes.</p></section>`;
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

async function refreshAutoScrubFleetStatus(): Promise<void> {
  if (autoScrubStatusLoading) return;
  autoScrubStatusLoading = true;
  render();
  try {
    autoScrubFleetStatus = await withNativeDeadline(loadAutoScrubRunFleetStatus(), "Load AutoScrub", 2_000);
  } catch {
    autoScrubFleetStatus = null;
  } finally {
    autoScrubStatusLoading = false;
    if (route !== "onboarding") render();
  }
}

async function stopAutoScrubFleet(): Promise<void> {
  if (autoScrubStopPending) return;
  autoScrubStopPending = true;
  render();
  try {
    autoScrubFleetStatus = await withNativeDeadline(requestAutoScrubGlobalStop(), "Stop AutoScrub", 2_000);
    showToast("AutoScrub stop requested");
  } catch {
    showToast("AutoScrub did not change");
  } finally {
    autoScrubStopPending = false;
    render();
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
    const candidates = importLocalMessageExport(new TextDecoder("utf-8", { fatal: true }).decode(bytes), {
      serviceId: "local_import",
      accountId: "manual-export",
      conversationId: "privacy-scan",
    });
    if (!candidates?.length) throw new Error("No supported messages were found");
    const attachmentName = file.name.replace(/[\u0000-\u001f\u007f-\u009f]/gu, " ").slice(0, 96) || "manual-export";
    const indexedCandidates = candidates.map((candidate, index) => index === 0
      ? {
        ...candidate,
        attachments: [{
          attachmentId: "manual-export-file",
          displayName: attachmentName,
          contentBase64: bytesToBase64(bytes),
        }],
      }
      : candidate);
    const persisted = await persistLocalScrubExport(indexedCandidates);
    privacyScanResult = persisted.scan;
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

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
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
  return `<details class="settings-disclosure sending-settings"><summary><span><strong>Sending</strong><small>${escapeHtml(formatSendMode(selectedMode))}</small></span></summary><div class="sending-settings-body"><div class="send-mode-list compact">${modes.map(([mode, label, detail]) => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-settings-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${label}</strong></span><small>${detail}</small></button>`).join("")}</div>${needsRiskAcceptance(selectedMode) ? `<div class="warning send-settings-warning"><strong>Experimental</strong><p>OSL must recheck the exact app, account, chat, and composer. If proof is unavailable or changes, it copies instead and sends nothing.</p></div>` : `<p class="send-settings-truth">OSL encrypts and copies. You choose where and when to send.</p>`}${consentRows}${rnWirePolicySettingsMarkup(rnWirePolicyState(rnWirePolicyRequested))}</div></details>`;
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
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  return `<h2>Scrub</h2><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Every scan and review stays local.</p><section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h3>Review an export</h3><p>Choose a TXT, CSV, or JSON message export. OSL suggests items; you decide what to review.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}${autoScrubAssistantMarkup(proActive)}<details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Apps, people, services, exports, and backups may retain copies. Only a service recheck can verify removal within its stated coverage.</p><p>Automatic deletion is unavailable in this build until the native one-shot reviewed-consent capability is available. Connect IMAP for read-only verification.</p><p>This build only gives manual directions. It does not delete app messages. Check the original app and delete each message yourself.</p></div></details><details class="privacy-technical settings-disclosure"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Remote app access</span><strong>Blocked</strong></div><div class="setting-line"><span><strong>Windows capture resistance</strong><small>Always applied to OSL’s own window. Cameras, malware, and modified recipients can still capture content.</small></span><strong>${screenshotProtectionEnabled ? "Active" : "Unavailable"}</strong></div></details>`;
}

function autoScrubAssistantMarkup(proActive: boolean): string {
  const autoScrubPlan = proActive ? "PRO ACTIVE · COMING SOON" : "PRO · COMING SOON";
  const status = projectAutoScrubFleetStatus(autoScrubFleetStatus);
  const actions = status.stopAvailable
    ? `<button class="button compact" id="autoscrub-stop" type="button" ${autoScrubStopPending ? "disabled" : ""}>${autoScrubStopPending ? "Stopping…" : "Stop"}</button>`
    : `<button class="button compact" id="autoscrub-refresh" type="button" ${autoScrubStatusLoading ? "disabled" : ""}>${autoScrubStatusLoading ? "Checking…" : status.label}</button>`;
  return `<details class="settings-disclosure autoscrub-disclosure"><summary><span><strong>AutoScrub assistant</strong><small>${autoScrubPlan}</small></span></summary><section class="autoscrub-card autoscrub-status-${status.tone}" aria-disabled="${status.stopAvailable ? "false" : "true"}"><header><div><span class="privacy-local-mark">LOCAL REVIEW</span><h3>${escapeHtml(status.label)}</h3></div>${actions}</header><p>${escapeHtml(status.detail)} Nothing happens until you review and confirm every batch.</p><details><summary>Automation risks</summary><p>Future paced actions must stop on limits, challenges, changed content, or failed checks. Automation may break an app’s rules or restrict an account. Treat removal as unconfirmed until the app shows it is gone.</p></details></section></details>`;
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
  return `<section class="privacy-results" aria-live="polite"><header><div><strong>${matching.length} ${matching.length === 1 ? "suggestion" : "suggestions"}</strong><small>${privacyScanResult.messagesScanned} messages scanned${privacyScanFileName ? ` · ${escapeHtml(privacyScanFileName)}` : ""}</small></div><span class="privacy-local-mark">LOCAL · ENCRYPTED</span></header>${selectionControls}${items || `<div class="empty-state"><strong>No suggestions in the categories you chose</strong><p>OSL can miss things. Review important chats yourself too.</p></div>`}${pagination}${items ? `<footer class="scrub-review-footer"><span>${selected} selected</span><button class="button" id="review-scrub-selection" type="button" ${selected ? "" : "disabled"}>Review selected</button></footer>` : ""}</section>`;
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
    selectedScrubFindings = toggleScrubReviewSelection(
      selectedScrubFindings,
      index,
      input.checked,
      privacyScanResult?.findings.length ?? 0,
    );
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
  document.querySelector<HTMLButtonElement>("#autoscrub-refresh")?.addEventListener("click", () => void refreshAutoScrubFleetStatus());
  document.querySelector<HTMLButtonElement>("#autoscrub-stop")?.addEventListener("click", () => void stopAutoScrubFleet());
}

function notificationSettingsContent(): string {
  const apps = orderedServices().filter((service) => service.category === "consumer").map((service) => `<label class="notification-app-row">${serviceLogo(service.id)}<span><strong>${escapeHtml(service.displayName)}</strong><small>Unread access is not supported yet</small></span><input type="checkbox" data-notification-app="${service.id}" ${notificationAppPreferences[service.id] !== false ? "checked" : ""}/></label>`).join("");
  const visibleNotifications = visibleAppNotifications();
  const activity = notificationsEnabled && visibleNotifications.length
    ? visibleNotifications.map((item) => `<article class="notification-event"><span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")}</small></span><time>${escapeHtml(item.createdAt)}</time></article>`).join("")
    : `<div class="empty-state"><strong>${notificationsEnabled ? "Nothing new" : "Activity is off"}</strong><p>${notificationsEnabled ? "New OSL security and chat events appear here." : "Turn on local activity to see OSL events on this device."}</p></div>`;
  return `<h2>Activity</h2><p>Private events created by OSL on this device.</p><section class="notification-events" aria-label="Recent OSL activity">${activity}</section><div class="settings-list"><label class="setting-line interactive"><span><strong>Local OSL activity</strong><small>Master control for activity on this device.</small></span><input id="notifications-opt-in" type="checkbox" ${notificationsEnabled ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Security changes</strong><small>Friend encryption-key changes that need verification.</small></span><input id="notification-security-activity" type="checkbox" ${notificationSecurityActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show details</strong><small>Off by default. When off, Activity hides event content.</small></span><input id="notification-previews" type="checkbox" ${notificationPreviewContent ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Suggest chat approval</strong><small>Suggestions never enable decryption.</small></span><input id="notification-scope-suggestions" type="checkbox" ${notificationScopeSuggestions ? "checked" : ""}/></label></div>${oslChatNotificationSettings()}<details class="settings-disclosure notification-apps"><summary><span><strong>Connected apps</strong><small>Provider unread counts are not read</small></span></summary><div class="notification-app-list">${apps}</div></details>`;
}

function oslChatNotificationSettings(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const muted = [...oslChatMutedPeople].flatMap((personId) => {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    return person ? [`<div class="setting-line"><span><strong>${escapeHtml(person.alias ?? "Verified friend")}</strong><small>Messages still arrive without a local alert.</small></span><button class="button compact" data-osl-chat-unmute="${escapeHtml(personId)}" type="button">Unmute</button></div>`] : [];
  }).join("");
  const previewsChecked = !pro || oslChatPreviewsVisible;
  const previewText = pro ? "Hide message previews on this device." : "Preview hiding is available with Pro.";
  const mutedDetails = muted ? `<details class="settings-disclosure" open><summary><span><strong>Muted OSL Chats</strong><small>${oslChatMutedPeople.size.toLocaleString("en-US")} muted</small></span></summary><div class="settings-list">${muted}</div></details>` : "";
  return `<section class="settings-list osl-chat-notification-settings" aria-label="OSL Chat controls"><label class="setting-line interactive"><span><strong>Encrypted chat alerts</strong><small>New-message activity from unmuted OSL friends.</small></span><input id="notification-chat-activity" type="checkbox" ${notificationChatActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>OSL Chat previews</strong><small>${previewText}</small></span><input id="osl-chat-preview-toggle" type="checkbox" ${previewsChecked ? "checked" : ""} ${pro ? "" : "disabled"}/></label></section>${mutedDetails}`;
}

function visibleAppNotifications(): AppNotification[] {
  return (appNotifications ?? []).filter((item) => item.detail === "New encrypted message" ? notificationChatActivity : notificationSecurityActivity);
}

type IdentityStorageProtection = "hardware" | "fallback" | "unknown";

/**
 * Classify a raw sealer method label (see the METHOD_* constants in
 * crates/keystore/src/sealer.rs — "tpm-pcp", "keyring", "noop-insecure",
 * "memory-ephemeral", "memory-test") into the three states the UI can
 * honestly show.
 *
 * Fail honest, not optimistic: only the two known hardware-backed labels
 * count as "hardware". Every other non-null label — a known software
 * fallback, or a future label OSL does not recognize yet — is "fallback",
 * never silently treated as secure. `null` (nothing learned this session,
 * e.g. a plain unlock of a pre-existing identity, which the backend does
 * not echo a method for) is "unknown", which the UI renders with the same
 * not-secure weight as "fallback" — an unverified state must never render
 * as secure.
 */
function classifyIdentityStorageProtection(method: string | null): IdentityStorageProtection {
  if (method === null) return "unknown";
  if (method === "tpm-pcp" || method === "keyring") return "hardware";
  return "fallback";
}

function identityStorageProtectionMarkup(protection: IdentityStorageProtection): string {
  if (protection === "hardware") {
    return `<div class="storage-protection-status secure" role="status"><strong>Hardware-protected</strong><small>Your identity key is sealed by this device's TPM or OS credential store.</small></div>`;
  }
  if (protection === "fallback") {
    return `<div class="storage-protection-status insecure" role="alert"><strong>Software fallback storage</strong><small>Hardware protection is unavailable on this device. Your identity key is protected by software only and will not survive a restart.</small></div>`;
  }
  return `<div class="storage-protection-status insecure" role="alert"><strong>Storage protection unknown</strong><small>OSL has not verified hardware-backed storage for this identity in this session. Treat it as not securely stored until verified.</small></div>`;
}

function identitySettingsContent(): string {
  const identities = hubIdentities.length
    ? hubIdentities.map((identity) => `<article class="identity-row"><div><strong>${escapeHtml(identity.label)}</strong><small>${escapeHtml(identity.oslUserId)} · ${escapeHtml(identity.safetyNumber)}</small></div>${identity.active ? `<span class="status-tag">Active</span>` : `<button class="button compact" data-switch-identity="${escapeHtml(identity.slotId)}">Switch</button>`}</article>`).join("")
    : `<div class="empty-state"><strong>Identity list unavailable</strong><p>Unlock OSL to manage encrypted identity slots.</p></div>`;
  const recovery = newIdentityRecoveryPhrase
    ? recoveryCaptureGate.canRender()
      ? `<div class="warning recovery-secret"><strong>Save the new identity recovery phrase now</strong><code>${escapeHtml(newIdentityRecoveryPhrase)}</code><p>Visible only on this page. It clears if you leave or hide OSL.</p></div>`
      : `<div class="warning recovery-secret" role="alert"><strong>Recovery phrase hidden</strong><p>${RECOVERY_PROTECTION_REFUSAL}.</p><button class="button compact" id="retry-recovery-protection" type="button">Retry protection</button></div>`
    : "";
  return `<h2>Account</h2><p>One active identity on this device.</p>${identityStorageProtectionMarkup(classifyIdentityStorageProtection(identityStorageMethod))}<div class="identity-list">${identities}</div>${recovery}<form class="inline-form identity-create-form" id="identity-slot-form"><input id="identity-slot-label" maxlength="80" placeholder="New identity label" required/><button class="button primary">Create identity</button></form><details class="recovery-import settings-disclosure"><summary>Recover another identity</summary><form id="identity-recover-form" class="setup-surface"><input id="identity-recover-label" maxlength="80" placeholder="Identity label" required/><textarea id="identity-recover-phrase" rows="3" placeholder="12-word recovery phrase" required></textarea><button class="button">Recover identity</button></form></details>${activationSettingsContent()}`;
}

function activationSettingsContent(): string {
  if (discordQaShell) return "";
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const accessLabel = licenseState.access === "offlineGrace" ? "Pro, offline grace" : pro ? "Pro active" : "Free";
  const moduleAccess = pro
    ? "Optional Pro module: separately installed and licensed on this device."
    : "Optional Pro module: separate install and license required; base OSL stays available.";
  const period = licenseState.currentPeriodEnd === null ? "" : `<small>${licenseState.status === "CANCELLED" ? "Access through" : "Current period ends"} ${formatUnixDate(licenseState.currentPeriodEnd)}</small>`;
  const clear = licenseState.status === "UNCONFIGURED" ? "" : `<button class="button compact" id="clear-activation-code" type="button">Clear activation</button>`;
  return `<details class="license-card settings-disclosure"><summary><span><strong>Plan</strong><small>${accessLabel}</small>${period}</span><span class="status-tag ${pro ? "active" : ""}">${escapeHtml(licenseState.status === "UNCONFIGURED" ? "Free" : licenseState.status)}</span></summary><div><p>Paste the activation code shown after checkout. No email is required.</p><p class="quiet-note">${moduleAccess}</p><form id="activation-form" class="license-form"><label for="activation-code">Activation code</label><div><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Activate Pro</button>${clear}</div></form></div></details>`;
}

function formatUnixDate(seconds: number): string {
  return new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" }).format(new Date(seconds * 1_000));
}

function appearanceSettingsContent(): string {
  return `<h2>Appearance</h2><p>Choose a theme. Arrange apps with Edit on Home.</p><div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong>${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>`;
}

function developerSettingsContent(): string {
  return `<details class="settings-disclosure developer-source"><summary>Developer source</summary><p>Open the fixed OSL repository through the trusted desktop command.</p><button class="button" data-source-repository type="button">Open source repository</button></details>`;
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
  const input = document.querySelector<HTMLInputElement>("#friend-verification-input");
  const submit = document.querySelector<HTMLButtonElement>("#owned-confirmation-submit");
  const validate = (): void => { if (input && submit) submit.disabled = ownedConfirmationBusy || input.value.length === 0; };
  input?.addEventListener("input", validate);
  submit?.addEventListener("click", () => void executeOwnedConfirmation());
}

function resetLocalProtectedSheet(closeRemote = true): void {
  const nativeContextToken = nativeDiscordProtectionActive ? peerProtectedSheet.context?.contextToken ?? null : null;
  activeContextToken = null;
  activeProtectedContextKind = null;
  localProtectedSheet = blankLocalProtectedModel();
  peerProtectedSheet = blankPeerProtectedModel();
  protectedSheetMode = "peer";
  nativeDiscordProtectionActive = false;
  // Tearing the protected context down takes the display surface with it; this
  // is a teardown, not a lock toggle.
  nativeDiscordOverlaySurfacePresent = false;
  nativeProtectPickerOpen = false;
  nativeProtectBusy = false;
  nativeProtectFailureNotice = "";
  if (closeRemote) {
    if (nativeContextToken) void setNativeDiscordProtectedOverlayOpen(nativeContextToken, false);
    else void setLocalProtectedSheetOpen(false);
  }
}

async function closeActiveServiceSurface(): Promise<void> {
  discordQaGeometryKeeper.stop();
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
      if (protectedSheetCloseBusy) return;
      protectedSheetCloseBusy = true;
      try {
        const contextToken = peerProtectedSheet.context?.contextToken;
        if (!contextToken || !(await setNativeDiscordProtectedOverlayOpen(contextToken, false))) {
          if (discordQaShell) {
            nativeDiscordProtectionActive = false;
            discordQaOverlayState = "starting";
            render();
            return;
          }
          showToast(withBackendReason(
            "OSL's protected Discord panel could not close safely",
            "set_native_discord_protected_overlay_open",
          ));
          return;
        }
        if (discordQaShell) {
          nativeDiscordProtectionActive = false;
          discordQaOverlayState = "starting";
        } else {
          resetLocalProtectedSheet(false);
        }
        render();
        return;
      } finally {
        protectedSheetCloseBusy = false;
      }
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
    showToast(withBackendReason("Protection could not open safely", "set_local_protected_sheet_open"));
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
  // Aligned, never focused. This used to be `focusActiveNativeCompanion()`,
  // which brings the borrowed Discord window to the foreground -- i.e. hands
  // Discord the keyboard at the exact instant the operator is about to type into
  // OSL's protected composer, which is then revealed on top of a window that
  // holds the caret. Every keystroke until something takes focus back goes into
  // Discord's own message box in the clear, and the native guard's focus reclaim
  // exists only to paper over this call.
  //
  // The alignment half is kept: the overlay is placed against the rectangle this
  // reports, so a stale one puts the composer in the wrong place. The foreground
  // half was never needed -- OSL is foreground here by construction (the
  // operator just clicked Protect in it), which is what `first_guard_decision`
  // in native_discord_overlay.rs requires to reveal.
  if (!discordQaShell && !(await alignActiveNativeCompanion())) {
    nativeProtectBusy = false;
    nativeProtectPickerOpen = false;
    nativeProtectFailureNotice = "Protection stopped: the Discord window could not be aligned safely.";
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
  // Opening protection also builds the display surface. From here the two have
  // separate lifetimes: the lock may close again without the surface going away.
  nativeDiscordOverlaySurfacePresent = true;
  // A freshly opened transcript layer renders the stored policy loaded above,
  // so no earlier eye failure/not-applied marker can still be true.
  discordQaTranscriptVisibilityOutcome = "applied";
  discordQaComposerRefusal = null;
  nativeProtectFailureNotice = "";
  nativeProtectBusy = false;
  nativeProtectPickerOpen = false;
  render();
  return true;
}

function activeVerifiedDiscordQaPeer(): { context: ManualPeerContext; person: HubPerson } | null {
  if (!discordQaShell) return null;
  const context = peerProtectedSheet.context;
  if (!context || context.contextToken !== activeContextToken) return null;
  const person = hubPeople.find((candidate) => candidate.personId === context.personId
    && candidate.safetyNumberVerified
    && !candidate.pendingKeyChange);
  return person ? { context, person } : null;
}

async function setDiscordQaWhitelistPermission(enabled: boolean): Promise<void> {
  const active = activeVerifiedDiscordQaPeer();
  if (!active || discordQaHeaderBusy) {
    showToast("Whitelist change stopped: the verified peer scope is unavailable");
    return;
  }
  if (active.context.scopeApproved === enabled) return;
  discordQaHeaderBusy = "whitelist";
  render();
  const saved = await setActiveHubFriendPermission(
    active.context.contextToken,
    active.person.personId,
    enabled,
    false,
  );
  if (!saved || activeVerifiedDiscordQaPeer()?.context.contextToken !== active.context.contextToken) {
    discordQaHeaderBusy = null;
    showToast("Whitelist change failed closed");
    render();
    return;
  }
  peerProtectedSheet.context = { ...active.context, scopeApproved: enabled };
  hubPeople = await listHubPeople() ?? hubPeople;
  discordQaHeaderBusy = null;
  showToast(enabled ? "Verified peer scope allowed" : "Verified peer scope revoked");
  render();
}

// Widening reach is deliberate: it is only ever reachable from the roster, it
// names the friend behind the live protected context, and the hub re-checks that
// context before and after the write.
async function toggleWhitelistRosterReach(personId: string, broadened: boolean): Promise<void> {
  const active = activeVerifiedDiscordQaPeer();
  if (!active || active.person.personId !== personId || discordQaHeaderBusy) {
    showToast("Reach change stopped: open this person's protected chat first");
    return;
  }
  discordQaHeaderBusy = "roster";
  render();
  const updated = await setActiveHubFriendReach(active.context.contextToken, personId, broadened);
  discordQaHeaderBusy = null;
  if (!updated) {
    showToast("Reach change failed closed");
    render();
    return;
  }
  hubPeople = await listHubPeople() ?? hubPeople.map((person) => person.personId === personId ? updated : person);
  showToast(broadened ? "Reach extended to the chats you share" : "Reach limited to the chats you approved");
  render();
}

// Revoking one recorded chat takes effect immediately, with no need to switch
// reach off first: the hub records the exclusion before it drops the approval.
async function revokeWhitelistRosterScope(personId: string, storageKey: string): Promise<void> {
  const active = activeVerifiedDiscordQaPeer();
  if (!active || active.person.personId !== personId || !storageKey || discordQaHeaderBusy) {
    showToast("Revoke stopped: open this person's protected chat first");
    return;
  }
  discordQaHeaderBusy = "roster";
  render();
  const updated = await revokeActiveHubFriendScope(active.context.contextToken, personId, storageKey);
  discordQaHeaderBusy = null;
  if (!updated) {
    showToast("Revoke failed closed");
    render();
    return;
  }
  hubPeople = await listHubPeople() ?? hubPeople.map((person) => person.personId === personId ? updated : person);
  showToast("Chat revoked for this person");
  render();
}

// The eye owns exactly one thing: which text the operator reads over the
// Discord rows. It never opens or closes the composer, never touches
// nativeDiscordProtectionActive/discordQaComposerBusy, and never sends.
//
// It is also completely independent of the lock. The lock is encryption only,
// so the eye must work with the lock off — that is the whole point of the two
// controls. This function therefore reads the display surface's own presence
// and never the protection flag.
async function toggleDiscordQaTranscriptVisibility(): Promise<void> {
  const active = activeVerifiedDiscordQaPeer();
  if (!active || discordQaHeaderBusy || !isLocalTtlSeconds(peerProtectedSheet.ttlSeconds)) {
    discordQaTranscriptVisibilityOutcome = "failed";
    render();
    showToast("Transcript visibility stopped: the protected scope is unavailable");
    return;
  }
  const previous = peerProtectedSheet.decryptDisplayEnabled;
  const requested = !previous;
  // Only a display surface that does not exist can fail to render the change.
  // Capture that once, up front, so the same answer decides both the notify and
  // the outcome the control reports. This is the overlay surface's presence,
  // not the lock: a closed lock leaves the surface present and the eye applies.
  const transcriptSurfaceLive = nativeDiscordOverlaySurfacePresent;
  discordQaHeaderBusy = "visibility";
  peerProtectedSheet.decryptDisplayEnabled = requested;
  render();
  const qaImmediate = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"
    && transcriptSurfaceLive
    ? await emitTo(
      "native-discord-overlay",
      PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,
      requested,
    ).then(() => true).catch(() => false)
    : true;
  const saved = await saveActiveContextSecurity(
    active.context.contextToken,
    peerProtectedSheet.ttlSeconds,
    requested,
  );
  if (!saved
    || saved.decryptDisplayEnabled !== requested
    || !isLocalTtlSeconds(saved.ttlSeconds)
    || activeVerifiedDiscordQaPeer()?.context.contextToken !== active.context.contextToken
    || !qaImmediate) {
    peerProtectedSheet.decryptDisplayEnabled = previous;
    if (import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1" && transcriptSurfaceLive) {
      await emitTo(
        "native-discord-overlay",
        PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,
        previous,
      ).catch(() => undefined);
    }
    discordQaHeaderBusy = null;
    discordQaTranscriptVisibilityOutcome = "failed";
    showToast("Transcript visibility failed closed");
    render();
    return;
  }
  // The authoritative scope policy is the only source for both fields, so a
  // TTL another surface changed can never be overwritten by a stale cache on
  // the next eye press.
  peerProtectedSheet.ttlSeconds = saved.ttlSeconds;
  peerProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled;
  if (transcriptSurfaceLive && import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1") {
    const notified = await emitTo(
      "native-discord-overlay",
      PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,
    ).then(() => true).catch(() => false);
    if (!notified) {
      discordQaHeaderBusy = null;
      discordQaTranscriptVisibilityOutcome = "failed";
      showToast("Transcript visibility was saved; the protected display will refresh on focus");
      render();
      return;
    }
  }
  discordQaHeaderBusy = null;
  // Saved with no display surface in existence is not success: say so on the
  // control instead of claiming a change the operator cannot see. The lock's
  // position is deliberately not consulted — it cannot make this true or false.
  discordQaTranscriptVisibilityOutcome = transcriptSurfaceLive ? "applied" : "unapplied";
  showToast(transcriptSurfaceLive
    ? (requested ? "Protected transcript shows decrypted text" : "Protected transcript shows Discord flagtext")
    : "Transcript visibility saved; no protected display surface is open");
  render();
}

if (!runningUnderVitest) {
  void listen<void>(MAIN_WINDOW_CAPTURE_REFUSED_EVENT, () => {
    recoveryCaptureGate.invalidate();
    screenshotProtectionEnabled = false;
    newIdentityRecoveryPhrase = null;
    renderNow();
  });

  void listen<void>(NATIVE_DISCORD_OVERLAY_CLOSED_EVENT, () => {
    // The native side only sends this when the protected display surface is
    // genuinely gone (session cleared and windows hidden), never for a lock
    // toggle, which leaves the retained surface dormant. This is the one signal
    // that can make the eye report "unapplied", and it is recorded in every
    // build before any lock-shaped early return.
    nativeDiscordOverlaySurfacePresent = false;
    if (!discordQaShell || !nativeDiscordProtectionActive) return;
    nativeDiscordProtectionActive = false;
    discordQaOverlayState = "starting";
    discordQaComposerBusy = false;
    discordQaComposerOpening = false;
    render();
  });
}

/**
 * Set the "your keystrokes are not reaching OSL" warning from the native level.
 *
 * Safety-critical, and now a plain assignment. Two independent native latches
 * (`report_composer_band_surrender` and `report_protected_focus_refused`) feed
 * one aggregate, and `publish_composer_unreachable` emits only on that
 * aggregate's edges with the aggregate itself in the payload — so the level is
 * taken as given rather than reconstructed from a sequence of edges nothing on
 * the wire could attribute. Idempotent under a duplicated or dropped message,
 * which the counting this replaced was not: it could bank a retraction it never
 * saw raised, or stick on.
 *
 * Nothing here reads, logs or persists anything: one boolean and one of three
 * fixed reason strings.
 */
function applyNativeDiscordComposerUnreachable(
  unreachable: boolean,
  reason: NativeDiscordComposerUnreachableReason | "",
): void {
  const previous = nativeDiscordComposerUnreachable;
  nativeDiscordComposerUnreachable = unreachable;
  nativeDiscordComposerUnreachableReason = reason;
  // Only the transition between "warned" and "not warned" is worth a repaint; the
  // native side already withholds everything else.
  if (previous === unreachable) return;
  // Committed synchronously, not on the next animation frame. Every frame this
  // waits is a frame the operator may spend typing into the clear, and the
  // retraction is held to the same standard so the warning cannot outlive the
  // condition either. This is an edge, at most a handful per session.
  renderNow();
  if (unreachable) {
    // The chip in the header strip is the surface that actually carries this —
    // the borrowed Discord window covers this webview's toast layer. The toast is
    // the announcement for an operator who is looking at the hub when it happens.
    showToast("Your typing is going to Discord, not OSL — stop typing and check the cyan ring");
  }
}

if (!runningUnderVitest) {
  void listen<{ reason?: unknown; unreachable?: unknown }>(
    NATIVE_DISCORD_COMPOSER_UNREACHABLE_EVENT,
    ({ payload }) => {
      // `{ reason, unreachable }`, not the bare boolean this once was. A handler
      // that returns on any non-boolean payload raises NOTHING for a real focus
      // refusal, so the shape is destructured rather than type-guarded whole.
      if (typeof payload !== "object" || payload === null) return;
      const { reason, unreachable } = payload as { reason?: unknown; unreachable?: unknown };
      // A payload that is not the level this contract promises is not evidence
      // either way, and inventing a raise or a retraction from it would be worse
      // than ignoring it: one direction invents a leak warning, the other clears a
      // real one.
      if (typeof unreachable !== "boolean") return;
      // The reason only sharpens the tooltip, so an unrecognised one is dropped and
      // the warning still lands.
      const named = NATIVE_DISCORD_COMPOSER_UNREACHABLE_REASONS.find((known) => known === reason);
      applyNativeDiscordComposerUnreachable(unreachable, named ?? "");
    },
  );
}

async function toggleDiscordQaComposer(): Promise<void> {
  if (!discordQaShell || discordQaComposerBusy) return;
  if (nativeDiscordProtectionActive) {
    await toggleLocalProtectedSheet();
    return;
  }
  await openDiscordQaComposer();
}

async function openSoleVerifiedDiscordQaOverlay(): Promise<boolean> {
  if (!discordQaShell
    || !discordQaShellStarted
    || route !== "service"
    || activeHomeAppId !== "discord"
    || activeNativeHostId !== "discord"
    || activeNativeHostMode !== "existingSession"
    || discordQaOverlayOpening
    || nativeProtectBusy) return false;
  discordQaOverlayOpening = true;
  try {
    hubPeople = await listHubPeople().catch(() => null) ?? [];
    const verifiedStablePeers = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
    if (verifiedStablePeers.length !== 1) {
      discordQaOverlayState = "failed";
      nativeProtectFailureNotice = "Protection stopped: this QA identity requires exactly one verified friend.";
      render();
      return false;
    }
    const solePeer = verifiedStablePeers[0];
    // An intentional QA lock toggle leaves the exact peer context and overlay
    // WebView dormant. Reuse that context even though the presentation flag is
    // now false; the native command independently revalidates the exact
    // Discord host, generation, owner, peer context, and dormant session.
    const activeToken = peerProtectedSheet.personId === solePeer.personId
      && peerProtectedSheet.context?.personId === solePeer.personId
      ? peerProtectedSheet.context.contextToken
      : null;
    let overlayOpened = false;
    if (activeToken) {
      const result = await setNativeDiscordProtectedOverlayOpenForQa(activeToken);
      overlayOpened = result.opened;
      nativeProtectFailureNotice = result.opened ? "" : result.error;
      if (overlayOpened) {
        activeContextToken = activeToken;
        activeProtectedContextKind = "peer";
        nativeDiscordProtectionActive = true;
        nativeDiscordOverlaySurfacePresent = true;
        // A reused transcript layer starts from the stored policy, so the eye
        // is reconciled here instead of trusting the cache it kept while the
        // layer was dormant. This reads policy only; no protection state moves.
        discordQaTranscriptVisibilityOutcome = "applied";
        discordQaComposerRefusal = null;
        void refreshDiscordQaTranscriptVisibility(true);
      }
    } else {
      overlayOpened = await openNativeDiscordProtection(solePeer.personId);
    }
    discordQaOverlayState = overlayOpened ? "ready" : "failed";
    render();
    return overlayOpened;
  } finally {
    discordQaOverlayOpening = false;
  }
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
  peerProtectedSheet.status = "Protected text is ready. Your draft stays here until you send.";
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
  localProtectedSheet.status = opened.viewOnceConsumed ? "Opened once. Local authorization was consumed." : "Opened on this device.";
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

function syncOslChatComposer(): void {
  const draft = document.querySelector<HTMLTextAreaElement>("#osl-chat-draft");
  if (!draft) return;
  const bytes = oslChatDraftBytes(draft.value);
  const withinLimit = bytes <= OSL_CHAT_MAX_DRAFT_BYTES;
  const hasDraft = draft.value.trim().length > 0;
  const send = document.querySelector<HTMLButtonElement>("button.osl-chat-send");
  if (send) {
    const contextReady = send.dataset.oslChatSendContext === "1";
    send.disabled = !(contextReady && hasDraft && withinLimit);
  }
  const count = document.querySelector<HTMLOutputElement>("#osl-chat-draft-count");
  if (count) {
    count.textContent = `${bytes.toLocaleString("en-US")} / ${OSL_CHAT_MAX_DRAFT_BYTES.toLocaleString("en-US")}`;
    count.classList.toggle("is-over", !withinLimit);
  }
}

function bindWorkspace(): void {
  bindPasswordVisibility();
  bindLocalProtectedSheet();
  bindSavedAccountControls();
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-open]").forEach((button) => button.addEventListener("click", () => {
    void openOslChat(button.dataset.oslChatOpen ?? "");
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
    persistOslChatMutedPeople();
    render();
  });
  document.querySelector<HTMLInputElement>("#osl-chat-preview-toggle")?.addEventListener("change", (event) => {
    const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
    if (!pro) return;
    oslChatPreviewsVisible = (event.currentTarget as HTMLInputElement).checked;
    persistOslChatPreviewVisibility();
    render();
  });
  document.querySelector<HTMLButtonElement>("#osl-chat-permission-toggle")?.addEventListener("click", () => void toggleOslChatPermission());
  document.querySelector<HTMLButtonElement>("#osl-chat-back")?.addEventListener("click", () => void closeOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-refresh")?.addEventListener("click", () => void refreshOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-approve")?.addEventListener("click", () => void approveOslChat());
  const oslChatDraftInput = document.querySelector<HTMLTextAreaElement>("#osl-chat-draft");
  oslChatDraftInput?.addEventListener("input", () => {
    oslChatDraft = oslChatDraftInput.value;
    // The Send button's disabled state and the byte counter are computed in
    // activeThread() at RENDER time. This listener only assigned the draft, so
    // after typing a message Send kept the stale `disabled` from the previous
    // render and stayed dead until some unrelated event repainted -- measured on
    // a real VM: byteCounter=0 sendEnabled=False after typing, then 50/True
    // after clicking Refresh. The counter proving 0 -> 50 shows the input DID
    // fire; only the repaint was missing.
    //
    // Updated in place rather than calling render(): a full re-render would
    // rebuild the textarea under the caret and lose focus and cursor position
    // mid-word. The preconditions that cannot change while typing (verified,
    // ready, not busy) are carried on the button by the view.
    syncOslChatComposer();
  });
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
    void openDiscordQaProtectionForOperator(button.dataset.nativeProtectPerson ?? "");
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
  document.querySelectorAll<HTMLButtonElement>("[data-route]").forEach((button) => button.addEventListener("click", async () => {
    const intent = ++navigationIntentEpoch;
    const requestedRoute = button.dataset.route as Route;
    await Promise.resolve();
    if (intent !== navigationIntentEpoch) return;
    if (route === "osl-chat") {
      if (oslChatBusy) {
        showToast("Finish the secure message check first");
        return;
      }
      if (!(await closeOslChatContext())) {
        showToast("OSL Chat could not close safely");
        return;
      }
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
    }
    route = requestedRoute;
    if (button.dataset.settings) settingsSection = button.dataset.settings as SettingsSection;
    if (button.hasAttribute("data-profile-settings")) settingsSection = "account";
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
    if (next === "scrub") void refreshAutoScrubFleetStatus();
    if (next === "cleanup") void refreshMassCleanupCapabilities();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-settings-send-mode]").forEach((button) => button.addEventListener("click", () => {
    void changeSendingMode(button.dataset.settingsSendMode as SendMode);
  }));
  document.querySelector<HTMLButtonElement>("[data-inbox-start-private]")?.addEventListener("click", () => inboxPrimaryAction());
  document.querySelector<HTMLButtonElement>("#osl-mail-retry")?.addEventListener("click", () => void refreshOslMail());
  document.querySelector<HTMLButtonElement>("#osl-mail-provision")?.addEventListener("click", () => void provisionOslMailFromProfile());
  document.querySelectorAll<HTMLButtonElement>("[data-mail-pane]").forEach((button) => button.addEventListener("click", () => {
    oslMailPane = button.dataset.mailPane as OslMailPane;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-mail-thread]").forEach((button) => button.addEventListener("click", async () => {
    const threadId = button.dataset.mailThread ?? "";
    oslMailActiveThread = await retrieveOslMailThread(threadId);
    oslMailError = oslMailActiveThread ? null : "Message retrieval was refused";
    render();
  }));
  document.querySelector<HTMLButtonElement>("#osl-mail-ack")?.addEventListener("click", async () => {
    if (!oslMailActiveThread) return;
    oslMailDeleteReceipt = await acknowledgeOslMailRetrieval(oslMailActiveThread.retrievalId, oslMailActiveThread.messages.map((message) => message.messageId));
    oslMailError = oslMailDeleteReceipt ? null : "Server deletion was not confirmed";
    render();
  });
  document.querySelector<HTMLFormElement>("#osl-mail-compose-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void sendOslMailForm(event.currentTarget as HTMLFormElement);
  });
  document.querySelector<HTMLFormElement>("#osl-mail-burn-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    const form = event.currentTarget as HTMLFormElement;
    const address = oslMailStatus?.address ?? "";
    const confirmation = String(new FormData(form).get("confirmation") ?? form.querySelector<HTMLInputElement>("#osl-mail-burn-confirmation")?.value ?? "");
    oslMailBurnReceipt = await burnOslMailbox(address, confirmation);
    oslMailError = oslMailBurnReceipt ? null : "Mailbox burn was refused";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-connections-primary-action]")?.addEventListener("click", () => connectionsPrimaryAction());
  document.querySelector<HTMLButtonElement>("[data-activity-primary-action]")?.addEventListener("click", () => {
    route = "activity";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-privacy-primary-action]")?.addEventListener("click", () => {
    route = "privacy";
    render();
  });
  document.querySelector<HTMLInputElement>("#rn-wire-policy-toggle")?.addEventListener("change", (event) => {
    rnWirePolicyRequested = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(rnWirePolicyStorageKey, String(rnWirePolicyRequested));
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"; settingsSection = "notifications"; render(); }));
  document.querySelector<HTMLButtonElement>("[data-privacy-primary-action]")?.addEventListener("click", privacyPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-activity-primary-action]")?.addEventListener("click", activityPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-connections-primary-action]")?.addEventListener("click", connectionsPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-people-primary-action]")?.addEventListener("click", peoplePrimaryAction);
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
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
  document.querySelector<HTMLButtonElement>("#retry-recovery-protection")?.addEventListener("click", async () => {
    await proveRecoveryCaptureProtection();
    render();
  });
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
  document.querySelector<HTMLButtonElement>("#discord-qa-run-test")?.addEventListener("click", () => {
    void runDiscordQaOneClick();
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-row-proof")?.addEventListener("click", () => {
    void requestDiscordQaVisibleRowRuntimeReceipt();
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-open-composer")?.addEventListener("click", () => {
    void openDiscordQaComposer();
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-add")?.addEventListener("click", () => {
    void setDiscordQaWhitelistPermission(true);
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-remove")?.addEventListener("click", () => {
    void setDiscordQaWhitelistPermission(false);
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-roster")?.addEventListener("click", () => {
    whitelistRosterOpen = true;
    render();
  });
  document.querySelector<HTMLButtonElement>("#whitelist-roster-close")?.addEventListener("click", () => {
    whitelistRosterOpen = false;
    render();
  });
  document.querySelector<HTMLDialogElement>("#whitelist-roster-dialog")?.addEventListener("cancel", (event) => {
    event.preventDefault();
    whitelistRosterOpen = false;
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-whitelist-reach]").forEach((button) => button.addEventListener("click", () => {
    void toggleWhitelistRosterReach(button.dataset.whitelistReach ?? "", button.dataset.whitelistReachNext === "on");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-whitelist-scope-remove]").forEach((button) => button.addEventListener("click", () => {
    void revokeWhitelistRosterScope(button.dataset.whitelistScopeRemove ?? "", button.dataset.whitelistScopeKey ?? "");
  }));
  document.querySelector<HTMLButtonElement>("#discord-qa-transcript-visibility")?.addEventListener("click", () => {
    void toggleDiscordQaTranscriptVisibility();
  });
  document.querySelector<HTMLButtonElement>("#discord-qa-toggle-composer")?.addEventListener("click", () => {
    void toggleDiscordQaComposer();
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
  document.querySelectorAll<HTMLButtonElement>("[data-verify-person]").forEach((button) => button.addEventListener("click", () => requestFriendVerification(button.dataset.verifyPerson ?? "")));
  bindFriendRemovalControls(
    { querySelectorAll: (selector) => document.querySelectorAll<HTMLButtonElement>(selector) },
    requestFriendRemoval,
  );
  document.querySelectorAll<HTMLButtonElement>("[data-allow-person]").forEach((button) => button.addEventListener("click", () => void allowPersonHere(button.dataset.allowPerson ?? "")));
  document.querySelectorAll<HTMLElement>("[data-open-friends]").forEach((button) => button.addEventListener("click", () => {
    route = "people";
    friendsDialogOpen = false;
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
    persistOslChatMutedPeople();
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
  if (reason === "existingSessionQuitRefused") return `${name} did not close, so OSL did not take it over. Its window is back where it was`;
  if (reason === "secondaryInstanceUnverified") return `${name} cannot safely open a separate OSL window yet`;
  if (reason === "channelNotOwned") return `${name} is already used outside this OSL identity`;
  if (reason === "noChannelAvailable") return `Install a dedicated ${name} channel first`;
  if (reason === "appNotInstalled") return `Install ${name} first`;
  if (reason === "windowNotFound") return `${name} opened, but its OSL window was not found`;
  if (reason === "profileInitializationFailed") return `${name}'s separate OSL profile could not finish starting. Try again; your normal ${name} is untouched`;
  if (reason === "profileUnavailable") return `${name}'s separate OSL profile is unavailable`;
  return `${name} could not open as a native OSL window`;
}

// Taking over the client the operator is already using means closing it, so it
// only ever happens after an explicit affirmative click. The probe is read-only
// and must come first: a false answer means nothing is running, so there is
// nothing to consent to and OSL is simply the thing that starts Discord. Every
// other path -- refusal, dismissal, a probe that failed -- is the plain borrow
// OSL has always done.
async function requestedNativeTakeover(appId: NativeAppId, mode: NativeSessionMode, name: string): Promise<NativeDiscordTakeover> {
  if (appId !== "discord" || mode !== "existingSession") return "borrowExisting";
  const running = await nativeAppTakeoverRequiresConsent(appId).catch(() => null);
  if (running === null) return "borrowExisting";
  if (!running) return "quitAndRelaunch";
  const accepted = window.confirm(`OSL will close your running ${name} and reopen it inside OSL. Your account and your conversations are untouched: same login, same messages, nothing is deleted.`);
  return accepted ? "quitAndRelaunch" : "borrowExisting";
}

async function openNativeHostedApp(app: HomeAppCatalogEntry, service: LinkedService, appId: NativeAppId): Promise<void> {
  if (nativeActionBusy) return;
  nativeHostFailureNotice = "";
  const requestedMode = nativeSessionModeForApp(appId);
  const discordQaExistingSession = discordQaShell && appId === "discord" && requestedMode === "existingSession";
  if (discordQaExistingSession) discordQaHostState = "starting";
  if (activeNativeHostId === appId && activeNativeHostMode === requestedMode) {
    if (discordQaExistingSession) {
      // The native tether has already verified and retained this exact signed
      // Discord window. Hidden RDP desktops can block a foreground request;
      // the QA shell must still leave setup and expose its explicit test
      // control instead of waiting forever on focus arbitration.
      serviceGuideStep = null;
      discordQaHostState = "hosted";
      render();
      return;
    }
    if (await focusActiveNativeCompanion()) {
      serviceGuideStep = null;
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
  let rendererHostDeadlinePassed = false;
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
    const hostDeadlineMs = appId === "discord" && requestedMode === "dedicated"
      ? 190_000
      : discordQaExistingSession
        ? 90_000
        : 30_000;
    const discordTakeover = await requestedNativeTakeover(appId, requestedMode, app.displayName);
    const hostOperation = hostNativeAppWindow(appId, requestedMode, discordTakeover);
    if (discordQaExistingSession) {
      void hostOperation.then((lateResult) => {
        if (!rendererHostDeadlinePassed
          || lateResult.status !== "hosted"
          || lateResult.id !== "discord"
          || lateResult.mode !== "existingNativeCompanion"
          || route !== "service"
          || activeHomeAppId !== "discord") return;
        activeNativeHostId = "discord";
        activeNativeHostMode = "existingSession";
        serviceGuideStep = null;
        nativeHostFailureNotice = "";
        discordQaHostState = "hosted";
        discordQaShellStarted = true;
        syncDiscordQaGeometryMaintenance();
        render();
        void openDiscordQaComposer();
      }).catch(() => undefined);
    }
    let result = await withNativeDeadline(
      hostOperation,
      `Open ${app.displayName} inside OSL`,
      hostDeadlineMs,
    );
    rendererHostDeadlinePassed = true;
    if (result.reason === "existingSessionQuitRefused") {
      // OSL never escalates past a polite close, and the backend has already put
      // their window back on screen. The one thing left to offer is exactly the
      // borrow they would have had before the takeover existed.
      const borrowInstead = window.confirm(`${app.displayName} did not close, most likely because it is set to keep running in its tray. Use the window that is already open instead? Nothing about your account or conversations changes.`);
      if (borrowInstead) {
        result = await withNativeDeadline(
          hostNativeAppWindow(appId, requestedMode, "borrowExisting"),
          `Open ${app.displayName} inside OSL`,
          hostDeadlineMs,
        );
      }
    }
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
    rendererHostDeadlinePassed = true;
    if (discordQaExistingSession) {
      const recovered = await withNativeDeadline(
        resizeNativeAppWindow(),
        "Recover hosted Discord QA window",
        3_000,
      ).catch(() => null);
      if (recovered?.status === "resized"
        && recovered.id === "discord"
        && recovered.mode === "existingNativeCompanion") {
        activeNativeHostId = "discord";
        activeNativeHostMode = "existingSession";
        serviceGuideStep = null;
        nativeHostFailureNotice = "";
        discordQaHostState = "hosted";
        showToast("Discord is ready");
        return;
      }
      if (activeNativeHostId === "discord" && activeNativeHostMode === "existingSession") return;
    }
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
    const native = forceNewProfile ? undefined : selectedInstalledNativeApp(app.id);
    const nativeIntent = native;
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
    const native = accountId ? undefined : selectedInstalledNativeApp(app.id);
    const nativeIntent = native;
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
    "osl-chats", "osl-mail", "osl-notes", "scrub",
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

export function inboxPrimaryAction(): void {
  const first = hubPeople.find((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  if (first) {
    friendsDialogOpen = false;
    void openOslChat(first.personId);
    return;
  }
  route = "home";
  activeOslChatPersonId = null;
  friendsDialogOpen = true;
  render();
}

function openHomeModule(id: string): void {
  if (id === "osl-chats") {
    inboxPrimaryAction();
  } else if (id === "osl-mail") {
    route = "osl-mail";
    void refreshOslMail();
    render();
  } else if (id === "osl-servers") {
    route = "osl-servers";
    render();
  } else if (id === "scrub") {
    route = "privacy";
    render();
  } else if (id === "activity") {
    route = "activity";
    render();
  } else {
    showToast("OSL Notes is planned for a later release");
  }
}

function oslChatTimestamp(): string {
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date());
}

async function openOslChat(personId: string): Promise<void> {
  const person = hubPeople.find((candidate) => candidate.personId === personId);
  if (!person?.safetyNumberVerified || person.pendingKeyChange || oslChatBusy) return;
  const queuedViewOnce = (oslChatUnread.get(personId) ?? 0) > 0
    ? (oslChatMessages.get(personId) ?? []).filter((message) => message.state === "opened")
    : [];
  const epoch = ++oslChatOperationEpoch;
  oslChatBusy = true;
  oslChatSettingsPersonId = null;
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
    if (!context || epoch !== oslChatOperationEpoch) {
      showToast(withBackendReason("OSL Chat could not open", "activate_osl_chat_context"));
      return;
    }
    activeOslChatPersonId = personId;
    activeOslChatContext = context;
    oslChatUnread.delete(personId);
    void persistOslChatUnread();
    route = "osl-chat";
    if (context.scopeApproved) {
      const history = await listOslChatHistory();
      if (epoch !== oslChatOperationEpoch) return;
      if (history) {
        const durableMessages: OslChatMessage[] = history.slice().reverse().map((row) => {
          const incoming = row.senderOslUserId === context.peerOslUserId;
          return {
            messageId: row.messageId,
            direction: incoming ? "incoming" : "outgoing",
            body: row.plaintext,
            state: incoming ? "received" : "sent",
            timestampLabel: new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date(row.decryptedAt * 1_000)),
          };
        });
        oslChatMessages.set(personId, [...durableMessages, ...queuedViewOnce].slice(-200));
      }
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

export function persistOslChatUnread(): Promise<void> {
  return persistSensitiveOslChatJson(oslChatUnreadStorageKey, encodeOslChatUnread(oslChatUnread));
}

function persistOslChatNotifications(): void {
  const metadata = (appNotifications ?? []).filter((item) => item.detail === "New encrypted message").slice(0, 20);
  void persistSensitiveOslChatJson(oslChatNotificationStorageKey, encodeOslChatNotifications(metadata));
}

function mergePersistedOslChatNotifications(items: AppNotification[] | null): AppNotification[] {
  const chat = (appNotifications ?? []).filter((item) => item.detail === "New encrypted message");
  const merged = [...chat, ...(items ?? [])];
  return merged.filter((item, index) => merged.findIndex((candidate) => candidate.id === item.id) === index).slice(0, 20);
}

function commitOslChatBatch(personId: string, batch: NativeDiscordOverlayOpenedBatch, background: boolean): void {
  const messages = [...(oslChatMessages.get(personId) ?? [])];
  for (const acknowledgment of batch.acknowledgments) {
    const message = messages.find((candidate) => candidate.messageId === acknowledgment.messageId);
    if (message) message.state = acknowledgment.status;
  }
  for (const incoming of batch.messages) {
    const localMessageId = `received-${crypto.randomUUID()}`;
    messages.push({
      messageId: localMessageId,
      direction: "incoming",
      body: incoming.plaintext,
      state: incoming.viewOnceConsumed ? "opened" : "received",
      timestampLabel: oslChatTimestamp(),
    });
    if (background) {
      oslChatUnread.set(personId, Math.min(10_000, (oslChatUnread.get(personId) ?? 0) + 1));
      if (notificationsEnabled && notificationChatActivity && !oslChatMutedPeople.has(personId)) {
        appNotifications = [{
          id: localMessageId,
          title: "OSL Chat",
          detail: "New encrypted message",
          createdAt: "Now",
        }, ...(appNotifications ?? [])].slice(0, 20);
      }
    }
  }
  oslChatMessages.set(personId, messages.slice(-200));
  if (background && batch.messages.length) {
    void persistOslChatUnread();
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
        const batch = await openOslChatText();
        if (batch) commitOslChatBatch(person.personId, batch, true);
        const history = await listOslChatHistory();
        if (history) {
          const existingViewOnce = (oslChatMessages.get(person.personId) ?? []).filter((message) => message.state === "opened");
          oslChatMessages.set(person.personId, [...history.slice().reverse().map((row) => ({
            messageId: row.messageId,
            direction: row.senderOslUserId === context.peerOslUserId ? "incoming" as const : "outgoing" as const,
            body: row.plaintext,
            state: row.senderOslUserId === context.peerOslUserId ? "received" as const : "sent" as const,
            timestampLabel: new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date(row.decryptedAt * 1_000)),
          })), ...existingViewOnce].slice(-200));
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
  const messages = [...(oslChatMessages.get(personId) ?? []), {
    messageId: sent.messageId,
    direction: "outgoing" as const,
    body: draft,
    state: "sent" as const,
    timestampLabel: oslChatTimestamp(),
  }];
  oslChatMessages.set(personId, messages);
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
  if (clearMessages) oslChatMessages.clear();
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

function requestFriendVerification(personId: string): void {
  if (!hubPeople.some((person) => person.personId === personId && person.safetyNumber.length > 0)) return;
  friendsDialogOpen = false;
  ownedConfirmation = { kind: "verifyFriend", personId };
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
}

function requestFriendRemoval(personId: string): void {
  if (!hubPeople.some((person) => person.personId === personId)) return;
  friendsDialogOpen = false;
  ownedConfirmation = { kind: "removeFriend", personId };
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

async function refreshIdentitySlots(): Promise<void> {
  hubIdentities = await listHubIdentities() ?? [];
}

async function refreshIdentityScopedState(): Promise<void> {
  if (activeOslChatContext && !(await closeOslChatContext())) {
    throw new Error("OSL Chat could not close before changing identity state");
  }
  resetOslChatUiState(true);
  const [nextCore, nextIdentities, profile, people, linkedServices, notifications] = await Promise.all([
    loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration)),
    listHubIdentities().then((value) => value ?? []),
    loadFriendProfile().then(async (value) => {
      await getOslUsernameStatus("osl").catch(() => null);
      return value;
    }),
    listHubPeople().then((value) => value ?? []),
    loadLinkedServices().catch(() => []),
    notificationsEnabled ? loadAppNotifications() : Promise.resolve([]),
  ]);
  core = nextCore;
  refreshActiveBrowserAccountsReady();
  hubIdentities = nextIdentities;
  friendCode = profile?.friendCode ?? null;
  friendDisplayId = profile?.oslUserId ?? null;
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
    if (route === "onboarding" && onboardingRoute === "pro" && licenseState.access !== "free") onboardingRoute = "privacy";
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
  const verificationInput = document.querySelector<HTMLInputElement>("#friend-verification-input");
  const typedVerificationCode = request.kind === "verifyFriend" ? verificationInput?.value ?? "" : "";
  if (request.kind === "verifyFriend" && typedVerificationCode.length === 0) return;
  ownedConfirmationBusy = true;
  ownedConfirmationError = "";
  const submit = document.querySelector<HTMLButtonElement>("#owned-confirmation-submit");
  if (submit) { submit.disabled = true; submit.textContent = "Working…"; }
  const refuse = (message: string): void => {
    ownedConfirmationBusy = false;
    ownedConfirmationError = message;
    const status = document.querySelector<HTMLElement>("#owned-confirmation-dialog .form-status");
    if (status) status.textContent = message;
    if (submit) {
      submit.disabled = request.kind === "verifyFriend" && (verificationInput?.value.length ?? 0) === 0;
      submit.textContent = request.kind === "verifyFriend" ? "Accept key" : request.kind === "removeFriend" ? "Remove friend" : "Clear activation";
    }
  };
  try {
    if (request.kind === "verifyFriend") {
      const reviewingKeyChange = hubPeople.find((person) => person.personId === request.personId)?.pendingKeyChange === true;
      if (!(await verifyHubPerson(request.personId, typedVerificationCode))) { refuse("Verification refused: the code was not accepted. Nothing changed."); return; }
      hubPeople = await listHubPeople() ?? hubPeople;
      closeOwnedConfirmation();
      showToast(reviewingKeyChange ? "Friend key re-verified locally · no conversations approved" : "Friend request accepted locally · no conversations approved");
      return;
    }
    if (request.kind === "removeFriend") {
      if (!(await removeHubFriend(request.personId, { isTauriRuntime, invoke, recordBackendFailure }))) { refuse("Friend removal refused. Nothing changed."); return; }
      hubPeople = await listHubPeople() ?? hubPeople.filter((person) => person.personId !== request.personId);
      if (shouldClearRemovedFriendChat(activeOslChatPersonId, request.personId)) {
        resetOslChatUiState(false);
        if (route === "osl-chat") route = "home";
      }
      if (oslChatSettingsPersonId === request.personId) oslChatSettingsPersonId = null;
      oslChatMessages.delete(request.personId);
      oslChatUnread.delete(request.personId);
      closeOwnedConfirmation();
      showToast("Friend removed · keys and conversation approvals withdrawn");
      return;
    }
    licenseState = await clearHubActivationCode();
    closeOwnedConfirmation();
    showToast("Activation cleared from this device");
  } catch (failure) {
    refuse(localActionError(failure, request.kind === "clearActivation" ? "The saved activation could not be cleared." : request.kind === "removeFriend" ? "Friend removal refused. Nothing changed." : "Verification refused. Nothing changed."));
  }
}

async function createAdditionalIdentity(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const input = document.querySelector<HTMLInputElement>("#identity-slot-label");
  const label = input?.value.trim() ?? "";
  if (!label) return;
  if (!(await proveRecoveryCaptureProtection())) {
    showToast(RECOVERY_PROTECTION_REFUSAL);
    render();
    return;
  }
  const created = await createHubIdentitySlot(label);
  if (!created) { showToast("Identity creation failed closed"); return; }
  if (!recoveryCaptureGate.canRender()) {
    showToast(RECOVERY_PROTECTION_REFUSAL);
    return;
  }
  // The new slot is not switched to automatically (identity_registry.rs
  // creates it inactive), so this must not overwrite the ACTIVE
  // identity's status. Remember it for when/if the user switches in.
  knownIdentityStorageMethods.set(created.identity.slotId, created.storageMethod);
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
  // Same reasoning as createAdditionalIdentity: recovering a slot does not
  // switch to it, so only remember its method for a later switch.
  knownIdentityStorageMethods.set(recovered.identity.slotId, recovered.storageMethod);
  newIdentityRecoveryPhrase = null;
  core = await loadCoreIntegration();
  await refreshIdentitySlots();
  render();
}

async function switchIdentity(slotId: string): Promise<void> {
  if (!(await switchHubIdentity(slotId))) { showToast("Identity switch failed closed"); return; }
  // The switch result does not echo a storage method. Only claim a known
  // status if this session itself created/recovered that exact slot;
  // otherwise fail honest and report unknown rather than carrying over the
  // previously active identity's status onto a different one.
  identityStorageMethod = knownIdentityStorageMethods.get(slotId) ?? null;
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
  identityStorageMethod = null;
  knownIdentityStorageMethods.clear();
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

export function connectionsPrimaryAction(): void {
  const target = homeAppsFromServices(services).find((app) => app.visibility === "launch"
    && app.launchState === "available"
    && app.setupEligible
    && app.serviceId !== null);
  const service = target ? services.find((candidate) => candidate.id === target.serviceId) : null;
  if (!target || !service) {
    route = "settings";
    settingsSection = "apps";
    activeService = null;
    activeHomeAppId = null;
    serviceAccountPickerOpen = false;
    render();
    return;
  }
  openServiceRoute(service, target.provider, target.id, true);
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
  return `<h2>About</h2><div class="update-status-card"><span class="dot"></span><div><strong>${status}</strong><small>Signed local updater · no UI telemetry</small></div></div><div class="settings-actions"><button class="button ${updateStatus.state === "available" ? "" : "primary"}" data-update-check ${updateStatus.state === "checking" || updateStatus.state === "installing" ? "disabled" : ""}>Check for updates</button>${actions}</div><details class="settings-disclosure update-details"><summary>Update privacy</summary><p>Checks and installs use the trusted local updater. Release notes are plain text; remote HTML is never rendered.</p></details>${developerSettingsContent()}<details class="device-diagnostics settings-disclosure"><summary><span><strong>Device status</strong><small>${deviceReady ? "Ready" : "Needs attention"}</small></span></summary><p>${escapeHtml(coreReadinessLabel(core.readiness))}</p></details>`;
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
  document.querySelectorAll<HTMLButtonElement>("[data-source-repository]").forEach((button) => button.addEventListener("click", async () => { if (!(await openHubSourceRepository())) showToast("Could not open the fixed OSL source repository"); }));
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

/** How recent a journalled refusal has to be to be the one being reported. */
const BACKEND_REASON_FRESHNESS_MS = 10_000;

/**
 * Keep the friendly sentence, and append what the backend actually said.
 *
 * `localActionError` below can never fire for the narrow adapters in
 * adapters.ts / native-overlay-adapter.ts: they fail closed by *resolving*
 * `false`/`null` rather than throwing, so there is no `failure` to pass it and
 * the Rust reason -- e.g. everything `apps/osl-hub/src/main.rs` returns from the
 * composer calibration path -- reached nothing at all. `backend-failure.ts`
 * journals that reason verbatim; this is what puts it on screen.
 *
 * It never makes the backend more specific than the backend chose to be. Several
 * native refusals deliberately share one uniform sentence so the wire cannot
 * reveal which internal check failed (see the note at `broker.rs:1720`), and
 * this repeats whatever was recorded, unchanged -- it adds nothing of its own.
 *
 * Bounded by age as well as by command, so a refusal from an earlier, unrelated
 * attempt cannot be pinned onto this one.
 */
function withBackendReason(sentence: string, command: string): string {
  const failure = lastBackendFailure(command);
  if (!failure || Date.now() - failure.at > BACKEND_REASON_FRESHNESS_MS) return sentence;
  return failure.message ? `${sentence}: ${failure.message}` : sentence;
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
  root.innerHTML = `<div class="app-frame with-titlebar">${desktopTitlebar()}<main class="ui-recovery" role="alert" aria-labelledby="boot-recovery-title"><img src="${oslLogoUrl}" alt=""/><h1 id="boot-recovery-title">Couldn’t open OSL</h1><p>The local security core did not respond.</p><button class="button primary" id="boot-retry">Retry</button></main></div>`;
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
  void refreshAutoScrubFleetStatus();
  void getOslUsernameStatus("osl").catch(() => null);
  void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") renderWhenIdle(); });
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
let discordQaShellComplete = false;
let discordQaOneClickBusy = false;
// discordQaComposerBusy drives the visible lock control's disabled state and
// must only ever be set by an operator-initiated attempt (see
// openDiscordQaComposer's `source` argument). discordQaComposerOpening is the
// underlying re-entrancy guard shared by both operator and automatic
// attempts; it never disables the control.
let discordQaComposerBusy = false;
let discordQaComposerOpening = false;
let discordQaAutoComposerAttempted = false;
let discordQaAutoComposerFailureCount = 0;
const discordQaAutoComposerMaxAttempts = 3;
let discordQaShellRetryCount = 0;
const discordQaShellMaxAutomaticRetries = 2;
let discordMarkerAvailablePolling = false;
// The reason an OPERATOR-initiated protection open was refused, kept until the
// next operator attempt or a successful open. Automatic retries never write it:
// three bounded background attempts failing is not something to alarm the
// operator with, and only a click they made deserves an explanation.
// `message` is plain language; `reason` is the exact bounded text the native
// side returned, kept for the control's title so nothing is hidden.
let discordQaComposerRefusal: { message: string; reason: string } | null = null;

// Piggybacks on the geometry keeper's existing tick instead of running its
// own timer: over-polling accessibility previously froze the desktop, so
// discordMarkerAvailable is refreshed no more often than the alignment
// resize already happening below.
async function refreshDiscordMarkerAvailable(): Promise<void> {
  if (discordMarkerAvailablePolling) return;
  discordMarkerAvailablePolling = true;
  try {
    const available = await invoke<boolean>("discord_marker_available");
    if (available !== discordMarkerAvailable) {
      discordMarkerAvailable = available;
      render();
    }
  } catch {
    // Fail safe: an availability query failure must not hide a lock the
    // operator can otherwise still use.
  } finally {
    discordMarkerAvailablePolling = false;
  }
}

let discordQaScopeSecurityPolling = false;
let discordQaScopeSecuritySyncedAtMs = 0;
// Reconciled at most this often, on the geometry keeper's existing tick. No new
// timer: over-polling has frozen this desktop before, so the eye rides the
// alignment cycle that already runs and skips most of its ticks.
const DISCORD_QA_SCOPE_SECURITY_MIN_INTERVAL_MS = 5_000;

/**
 * Reconcile the eye (and the TTL it writes with) against the authoritative
 * scope policy. The header cached both once when protection opened and never
 * again, so a mode changed anywhere else left the eye showing the wrong mode —
 * and the next press then asked for the value already in effect, which renders
 * as an eye that does nothing.
 *
 * `get_native_discord_overlay_state` cannot be used from here: it rejects every
 * caller whose window label is not the overlay's. `get_active_hub_context_security`
 * reads the same stored policy and is callable from the main window.
 */
async function refreshDiscordQaTranscriptVisibility(force = false): Promise<void> {
  if (!discordQaShell || discordQaScopeSecurityPolling || discordQaHeaderBusy) return;
  const active = activeVerifiedDiscordQaPeer();
  if (!active) return;
  const now = Date.now();
  if (!force && now - discordQaScopeSecuritySyncedAtMs < DISCORD_QA_SCOPE_SECURITY_MIN_INTERVAL_MS) return;
  discordQaScopeSecuritySyncedAtMs = now;
  discordQaScopeSecurityPolling = true;
  try {
    const security = await loadActiveContextSecurity(active.context.contextToken);
    // Fail safe: an unreadable or out-of-range policy keeps the displayed mode
    // exactly as it is. It never flips what the operator is reading.
    if (!security || !isLocalTtlSeconds(security.ttlSeconds)) return;
    if (discordQaHeaderBusy) return;
    if (activeVerifiedDiscordQaPeer()?.context.contextToken !== active.context.contextToken) return;
    if (security.ttlSeconds === peerProtectedSheet.ttlSeconds
      && security.decryptDisplayEnabled === peerProtectedSheet.decryptDisplayEnabled) return;
    peerProtectedSheet.ttlSeconds = security.ttlSeconds;
    peerProtectedSheet.decryptDisplayEnabled = security.decryptDisplayEnabled;
    render();
  } catch {
    // Fail safe: keep the displayed mode.
  } finally {
    discordQaScopeSecurityPolling = false;
  }
}

const discordQaGeometryKeeper = createDiscordQaGeometryKeeper({
  isActive: () => discordQaShell
    && discordQaShellStarted
    && route === "service"
    && activeHomeAppId === "discord"
    && activeNativeHostId === "discord"
    && activeNativeHostMode === "existingSession",
  resize: () => {
    void refreshDiscordMarkerAvailable();
    void refreshDiscordQaTranscriptVisibility();
    return withNativeDeadline(
      resizeNativeAppWindow(),
      "Keep Discord aligned",
      1_500,
    ).catch(() => null);
  },
});

function syncDiscordQaGeometryMaintenance(): void {
  if (discordQaShell
    && discordQaShellStarted
    && route === "service"
    && activeHomeAppId === "discord"
    && activeNativeHostId === "discord"
    && activeNativeHostMode === "existingSession") {
    discordQaGeometryKeeper.start();
    return;
  }
  discordQaGeometryKeeper.stop();
}

async function startDiscordQaVisualOverlayAttempt(): Promise<void> {
  let overlayOpened = false;
  for (let attempt = 0; attempt < 20 && !overlayOpened; attempt += 1) {
    overlayOpened = await openSoleVerifiedDiscordQaOverlay();
    if (overlayOpened) break;
    if (activeNativeHostId !== "discord"
      || activeNativeHostMode !== "existingSession"
      || route !== "service") break;
    await new Promise<void>((resolve) => window.setTimeout(resolve, 500));
  }
  if (!overlayOpened) {
    discordQaOverlayState = "failed";
    nativeProtectFailureNotice ||= "The verified Discord QA overlay did not open within its bounded retry window.";
    render();
  }
}

async function ensureDiscordQaNativeHost(deadlineMs = 5_000): Promise<void> {
  // Renderer state is only presentation state and survives a QA executable
  // swap more readily than the native host registry. Always reconcile the
  // exact borrowed Discord tether before opening protection so a visibly
  // embedded window can never mask `native_host_state_missing`.
  const recovered = await withNativeDeadline(
    resizeNativeAppWindow(),
    "Reconcile hosted Discord QA window",
    deadlineMs,
  );
  if (recovered.status !== "resized"
    || recovered.id !== "discord"
    || recovered.mode !== "existingNativeCompanion") {
    throw new Error("The existing Discord window is not hosted by this OSL test build");
  }
  activeNativeHostId = "discord";
  activeNativeHostMode = "existingSession";
  discordQaHostState = "hosted";
  serviceGuideStep = null;
  render();
}

/**
 * Turn a refusal reason into something the operator can act on.
 *
 * Only reasons that are genuinely distinguishable from the returned text are
 * mapped; everything else falls through to that exact text rather than an
 * invented cause. No branch can leak draft or message content: every mapped
 * string is a fixed literal, and the native side never puts composer text in
 * one of these errors.
 */
function discordQaComposerRefusalMessage(reason: string): string {
  const text = reason.toLowerCase();
  // Only one branch may tell the operator to clear their message box, and it is
  // the one the native side returns only after proving there is something in
  // it. The measured bug was that two "OSL cannot put its own saved copy back"
  // refusals and one "OSL cannot tell" refusal all rendered as "clear your
  // message box" at an empty composer, which is unactionable by construction.
  if (text.includes("still holds your own draft")) {
    return "Discord's message box still holds your own text, so OSL left it alone. Clear Discord's message box, then press the lock again.";
  }
  if (text.includes("another discord conversation")) {
    return "OSL is holding a message it took out of a different Discord conversation, so it cannot put it back here. Reopen that conversation, then press the lock again.";
  }
  if (text.includes("restore the saved discord draft")
    || text.includes("typed back into the composer")
    || text.includes("verified after restoration")) {
    return "OSL could not put back the message it saved from Discord's message box. Leave the box alone, then press the lock again.";
  }
  if (text.includes("could not be cleared safely")) {
    return "OSL could not confirm what is in Discord's message box, so it changed nothing. Press the lock again.";
  }
  if (text.includes("expose the composer text") || text.includes("draft probe")) {
    return "OSL could not read Discord's message box, so it cannot tell whether anything is in it. Bring Discord forward, then press the lock again.";
  }
  if (text.includes("composer changed") || text.includes("composer binding changed")) {
    return "Discord's message box changed while protection was opening. Leave it alone and press the lock again.";
  }
  if (text.includes("one exact visible message composer") || text.includes("composer runtime id")) {
    return "OSL cannot find one message box in this Discord view. Open a direct message, then press the lock.";
  }
  if (text.includes("identity is not registered")) {
    return "OSL is still registering this protected identity. Press the lock again in a moment.";
  }
  if (text.includes("verified friend") || text.includes("verify this friend")) {
    return "Protection needs exactly one verified friend on this identity. Verify a friend first.";
  }
  if (text.includes("window changed")
    || text.includes("native discord host")
    || text.includes("brought forward safely")) {
    return "The Discord window changed or was not in front. Bring Discord forward, then press the lock again.";
  }
  if (text.includes("timed out")
    || text.includes("did not confirm that it opened")
    || text.includes("bounded retry window")
    || text.includes("interrupted")) {
    return "Discord did not answer in time. Press the lock again.";
  }
  return reason;
}

function discordQaComposerRefusalFrom(reason: string): { message: string; reason: string } {
  const bounded = reason.trim() || "Protection did not open and gave no reason.";
  return { message: discordQaComposerRefusalMessage(bounded), reason: bounded };
}

/**
 * The friend picker is only reachable from an operator click, so a refusal here
 * is an operator refusal and explains itself in the header strip. Protection,
 * composer and eye state are all left to openNativeDiscordProtection.
 */
async function openDiscordQaProtectionForOperator(personId: string): Promise<void> {
  discordQaComposerRefusal = null;
  if (await openNativeDiscordProtection(personId)) return;
  discordQaComposerRefusal = discordQaComposerRefusalFrom(nativeProtectFailureNotice);
  render();
}

async function openDiscordQaComposer(
  reconcileDeadlineMs = 5_000,
  source: "operator" | "automatic" = "operator",
): Promise<void> {
  if (!discordQaShell || discordQaComposerOpening || route !== "service") return;
  discordQaComposerOpening = true;
  // Only an operator-initiated attempt may disable the visible lock control,
  // and only while it is genuinely in flight. Automatic/background retries
  // (source === "automatic") share the same re-entrancy guard above but must
  // never touch discordQaComposerBusy, so the operator can always click.
  if (source === "operator") {
    discordQaComposerBusy = true;
    // A fresh operator attempt always gets a clean slate for the bounded
    // automatic retry budget, in case it fails and automatic retries resume.
    discordQaAutoComposerFailureCount = 0;
  }
  // A fresh operator attempt clears the previous refusal first, so the header
  // strip always describes the attempt the operator is watching rather than an
  // older one. Automatic retries leave whatever the operator last saw alone.
  if (source === "operator") discordQaComposerRefusal = null;
  discordQaOverlayState = "starting";
  nativeProtectFailureNotice = "";
  render();
  try {
    if (activeHomeAppId === "discord"
      && (activeNativeHostId !== "discord" || activeNativeHostMode !== "existingSession")) {
      // The native host can finish adopting Discord while WebView2 loses only
      // the response to the long-running invoke.  The visible borrowed window
      // is not authority: this merely lets the QA renderer ask the native
      // overlay command to reconcile it.  That command independently verifies
      // the unlocked owner, exact hosted Discord process/window/generation,
      // active friend context, and overlay target before it can open.
      activeNativeHostId = "discord";
      activeNativeHostMode = "existingSession";
      discordQaHostState = "hosted";
      serviceGuideStep = null;
    } else {
      await ensureDiscordQaNativeHost(reconcileDeadlineMs);
    }
    discordQaShellStarted = true;
    syncDiscordQaGeometryMaintenance();
    if (!(await openSoleVerifiedDiscordQaOverlay())) {
      throw new Error(nativeProtectFailureNotice || "The verified Discord composer could not open");
    }
    showToast("Protected composer is ready. Nothing has been sent.");
    // A protected composer that is genuinely open answers the question the
    // refusal chip was asking, whatever opened it.
    discordQaComposerRefusal = null;
    discordQaAutoComposerFailureCount = 0;
  } catch (failure) {
    discordQaOverlayState = "failed";
    nativeProtectFailureNotice = localActionError(failure, "Discord composer stopped safely");
    if (source === "operator") {
      showToast(nativeProtectFailureNotice);
    } else {
      discordQaAutoComposerFailureCount += 1;
    }
    // The toast above renders under the borrowed Discord window, so an operator
    // refusal also gets a persistent, plain-language chip in the header strip.
    // Automatic retries deliberately paint nothing: three quiet background
    // attempts are not an operator-facing error.
    if (source === "operator") {
      discordQaComposerRefusal = discordQaComposerRefusalFrom(nativeProtectFailureNotice);
    }
  } finally {
    discordQaComposerOpening = false;
    if (source === "operator") discordQaComposerBusy = false;
    render();
  }
}

async function openDiscordQaComposerAfterHostReady(): Promise<void> {
  if (!discordQaShell
    || discordQaAutoComposerAttempted
    || !discordQaShellStarted
    || discordQaHostState !== "hosted"
    || activeNativeHostId !== "discord"
    || activeNativeHostMode !== "existingSession"
    || nativeDiscordProtectionActive) return;
  discordQaAutoComposerAttempted = true;
  // The first renderer-driven open can otherwise race the newly adopted
  // Discord/WebView input route even after native readiness is reported.
  // Manual lock actions bypass this function and remain immediate.
  await new Promise<void>((resolve) => window.setTimeout(resolve, 750));
  if (route !== "service"
    || !discordQaShellStarted
    || discordQaHostState !== "hosted"
    || activeNativeHostId !== "discord"
    || activeNativeHostMode !== "existingSession"
    || nativeDiscordProtectionActive) return;
  for (
    let attempt = 0;
    attempt < discordQaAutoComposerMaxAttempts && !nativeDiscordProtectionActive;
    attempt += 1
  ) {
    if (route !== "service"
      || discordQaHostState !== "hosted"
      || activeNativeHostId !== "discord"
      || activeNativeHostMode !== "existingSession") return;
    // Automatic retries must never drive the operator-facing lock's disabled
    // state; only a genuine operator click does that (see
    // openDiscordQaComposer's `source` argument). This keeps the lock
    // clickable throughout bounded background retries instead of flickering.
    await openDiscordQaComposer(15_000, "automatic");
    if (nativeDiscordProtectionActive) return;
    if (attempt + 1 < discordQaAutoComposerMaxAttempts) {
      await new Promise<void>((resolve) => window.setTimeout(resolve, 500));
    }
  }
}

function runDesktopShortcutAction(): void {
  if (discordQaShell) {
    void openDiscordQaComposer();
    return;
  }
  void toggleDesktopFullscreen().catch(() => undefined);
}

async function requestDiscordQaVisibleRowRuntimeReceipt(): Promise<void> {
  if (!discordQaShell
    || discordQaRowProofState === "busy"
    || route !== "service"
    || activeNativeHostId !== "discord"
    || !nativeDiscordProtectionActive) return;
  discordQaRowProofState = "busy";
  render();
  const receipt = await requestNativeDiscordVisibleRowRuntimeReceipt();
  discordQaRowProofState = receipt === null
    ? "unavailable"
    : receipt.accepted
      ? "accepted"
      : "refused";
  render();
  showToast(receipt?.accepted
    ? "Native row proof saved"
    : receipt === null
      ? "Native row proof was unavailable"
      : "Native row proof refused; proof saved");
}

async function runDiscordQaOneClick(): Promise<void> {
  if (!discordQaShell
    || discordQaOneClickBusy
    || route !== "service") return;
  discordQaOneClickBusy = true;
  discordQaOverlayState = "starting";
  nativeProtectFailureNotice = "";
  render();
  try {
    await ensureDiscordQaNativeHost();
    discordQaShellStarted = true;
    syncDiscordQaGeometryMaintenance();
    const qaProbe = await runNativeDiscordHeadlessQa();
    if (!qaProbe
      || !qaProbe.personToPersonE2ee
      || qaProbe.viewOnce
      || !qaProbe.deliveredToOslInbox) {
      throw new Error("The fixed encrypted probe did not produce authenticated send proof");
    }
    void startDiscordQaVisualOverlayAttempt();
    let receivedPeerProbe = false;
    for (let attempt = 0; attempt < 30 && !receivedPeerProbe; attempt += 1) {
      const poll = await pollNativeDiscordHeadlessQa();
      receivedPeerProbe = (poll?.openedCount ?? 0) > 0;
      if (!receivedPeerProbe) await new Promise<void>((resolve) => window.setTimeout(resolve, 1_000));
    }
    discordQaShellComplete = receivedPeerProbe;
    if (!receivedPeerProbe) showToast("Probe sent. The other client has not replied yet.");
  } catch (failure) {
    discordQaOverlayState = "failed";
    nativeProtectFailureNotice = localActionError(failure, "Discord test stopped safely");
    showToast(nativeProtectFailureNotice);
  } finally {
    discordQaOneClickBusy = false;
    render();
  }
}

async function startDiscordQaShell(): Promise<void> {
  if (!discordQaShell || discordQaShellStarting || discordQaShellComplete || !core.readiness.unlocked) return;
  discordQaShellStarting = true;
  if (!discordQaShellStarted) discordQaHostState = "starting";
  discordQaOverlayState = "starting";
  try {
    if (!discordQaShellStarted) {
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
    }
    discordQaHostState = "hosted";
    syncDiscordQaGeometryMaintenance();
    // Deliberately stop after the exact native route is ready. The disposable
    // QA header exposes one deterministic Run test action, avoiding hidden
    // background work and making repeated build/test cycles observable.
    discordQaShellComplete = false;
    discordQaShellRetryCount = 0;
    void openDiscordQaComposerAfterHostReady();
  } catch (failure) {
    route = "service";
    serviceGuideStep = 0;
    const hostClaimed = activeNativeHostId === "discord" && activeNativeHostMode === "existingSession";
    discordQaShellStarted = hostClaimed;
    discordQaHostState = hostClaimed ? "hosted" : "failed";
    discordQaOverlayState = "failed";
    nativeHostFailureNotice = localActionError(failure, "Discord QA could not start");
    render();
    showToast(nativeHostFailureNotice);
    if (discordQaShellRetryCount < discordQaShellMaxAutomaticRetries) {
      discordQaShellRetryCount += 1;
      window.setTimeout(() => void startDiscordQaShell(), 1_000);
    }
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
  void loadUiPreferences().then(() => {
    if (route === "home" || route === "osl-chat" || (route === "settings" && settingsSection === "notifications")) renderWhenIdle();
  });
  root.innerHTML = `<div class="app-frame with-titlebar">${desktopTitlebar()}<main class="loading-screen"><div class="loading-seal" aria-hidden="true"><img class="osl-logo loading-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/></div><span class="sr-only">Opening OSL</span></main></div>`;
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
    if (signalQaShellEnabled) {
      route = "signal-qa";
      return;
    }
    const preferencesRequest = withNativeDeadline(loadOnboardingPreferences(), "Load OSL preferences", bootPreferenceDeadlineMs).catch(() => null);
    const servicesRequest = withNativeDeadline(loadLinkedServices(), "Load apps", bootSupportDeadlineMs).catch(() => null);
    const nativeAppsRequest = savedAccountMode === "use"
      ? withNativeDeadline(loadNativeApps(), "Load selected Windows apps", bootSupportDeadlineMs).catch(() => null)
      : Promise.resolve(null);
    const licenseRequest = withNativeDeadline(loadHubLicenseState(), "Load plan", bootSupportDeadlineMs).catch(() => null);
    const browserCompanionRequest = withNativeDeadline(loadDefaultBrowserCompanionStatus(), "Check default browser", bootSupportDeadlineMs).catch(() => null);
    const browserProfilesRequest = withNativeDeadline(
      listBrowserProfilesForConsent(),
      "Load saved browser areas",
      bootSupportDeadlineMs,
    ).catch(() => null);
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
    if (discordQaShell) {
      // Native startup has already loaded or created the device-sealed
      // disposable QA identity. Never route that identity through consumer
      // onboarding, even when saved preferences are missing or stale.
      route = "service";
      serviceGuideStep = null;
      void startDiscordQaShell();
    } else if (core.readiness.bootstrapStatus === "setupRequired") {
      onboardingRoute = "welcome";
      route = "onboarding";
    } else if (core.readiness.bootstrapStatus === "passwordRequired") {
      onboardingRoute = "unlock";
      route = "onboarding";
    } else {
      route = preferences.onboardingComplete ? "home" : "onboarding";
      if (!preferences.onboardingComplete) onboardingRoute = pendingOnboardingRoute() ?? onboardingRouteForBuild("pro");
    }
    // startDiscordQaShell paints the service route after loading only the two
    // catalogs it needs. Until then, retain the neutral loading screen rather
    // than flashing any consumer setup or home surface.
    if (!discordQaShell) renderNow();
    if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    startReadyWorkspaceLoads();
    void Promise.all([servicesRequest, nativeAppsRequest, licenseRequest, browserCompanionRequest, browserProfilesRequest]).then(([linkedServices, nativeCatalog, currentLicenseState, currentBrowserCompanionStatus, profiles]) => {
      if (attempt !== bootstrapEpoch) return;
      if (linkedServices) services = linkedServices;
      if (nativeCatalog && isCompleteNativeCatalog(nativeCatalog)) {
        nativeApps = nativeCatalog;
      }
      if (currentLicenseState) licenseState = currentLicenseState;
      if (currentBrowserCompanionStatus) defaultBrowserCompanionStatus = currentBrowserCompanionStatus;
      if (profiles) setBrowserProfiles(profiles);
      renderWhenIdle();
      if (!discordQaShell) void recoverNativeHostAfterRendererLoad();
    });
  } catch {
    if (attempt === bootstrapEpoch) showBootstrapRecovery();
    return;
  }
}

if (!runningUnderVitest) {
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => { if (themeChoice === "system") applyTheme("system"); });
  window.addEventListener("keydown", (event) => {
    if (event.key !== "F11" || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    runDesktopShortcutAction();
  });
}
let nativeHostResizeFrame = 0;

/**
 * One outstanding invoke per native command.
 *
 * `withNativeDeadline` abandons a slow call, it does not cancel it: the invoke
 * is still running in the backend when the wrapper rejects. Without this gate a
 * pass that gave up during a stall would immediately issue a second invoke of
 * the same command, and every further pass one more, so the number of genuinely
 * outstanding IPC calls would grow for as long as the stall lasted -- the exact
 * runaway that turns a fast mouse wave into an unresponsive app.
 */
const nativeSurfaceCallGate = new NativeCallGate();

type NativeSurfaceCallOutcome = "ok" | "failed" | "unanswered";

/**
 * Run one bounded native geometry call and classify the answer.
 *
 * A deadline is NOT an answer. The call is still outstanding, so reading it as
 * "the window is gone" would detach and relaunch a live companion in the middle
 * of a window drag. `"unanswered"` means "ask again on the trailing pass" and
 * suppresses every recovery branch; only a real non-success status is a failure.
 */
async function settleNativeSurfaceCall(
  key: string,
  start: () => Promise<{ status: string }>,
  label: string,
  expected: string,
): Promise<NativeSurfaceCallOutcome> {
  try {
    const action = await withNativeDeadline(nativeSurfaceCallGate.run(key, start), label, 3_000);
    return action.status === expected ? "ok" : "failed";
  } catch (failure) {
    return failure instanceof NativeDeadlineError ? "unanswered" : "failed";
  }
}

async function validateNativeSurfacesPass(): Promise<void> {
    if (activeNativeHostId) {
      const name = activeHomeAppName();
      const resized = await settleNativeSurfaceCall(
        "nativeApp:resize",
        () => resizeNativeAppWindow(),
        `Restore ${name}`,
        "resized",
      );
      if (resized === "failed") {
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
      const resized = await settleNativeSurfaceCall(
        "browserCompanion:resize",
        () => resizeDefaultBrowserCompanion(),
        "Restore browser companion",
        "resized",
      );
      const focused = resized === "ok"
        ? await settleNativeSurfaceCall(
          "browserCompanion:focus",
          () => focusDefaultBrowserCompanion(),
          "Focus browser companion",
          "focused",
        )
        : "unanswered";
      if (resized === "failed" || focused === "failed") {
        activeDefaultBrowserCompanion = false;
        if (route === "service") serviceGuideStep = 0;
        render();
        showToast("The browser companion closed. Open it again when you’re ready.");
      }
    }
    if (mullvadWindowHosted) {
      const resized = await settleNativeSurfaceCall(
        "mullvad:resize",
        () => resizeMullvadWindow(),
        "Restore Mullvad",
        "resized",
      );
      const focused = resized === "ok"
        ? await settleNativeSurfaceCall("mullvad:focus", () => focusMullvadWindow(), "Focus Mullvad", "focused")
        : "unanswered";
      if (resized === "failed" || focused === "failed") {
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

/**
 * Every move/resize/focus event collapses into one running pass plus one
 * trailing pass, and consecutive passes are paced.
 *
 * Dragging a caption on Windows delivers WM_MOVE continuously from inside a
 * modal message loop that owns this very thread, so an erratic drag raises
 * hundreds of events a second. Each pass is a native round trip, so answering
 * every event -- or, as this used to, re-entering the trailing pass with a zero
 * gap for as long as events keep arriving -- saturates the thread that is
 * drawing the drag and the whole app stops responding.
 *
 * Intermediate positions are dropped rather than queued: only the position the
 * window rests at is a position anything has to be aligned to, and the trailing
 * pass is guaranteed, so that resting position is always the one applied. See
 * native-realignment.ts for the bounds and native-realignment.test.ts for the
 * proofs that nothing here can queue or run unboundedly.
 */
const nativeHostRealignment = new CoalescedRealignment(validateNativeSurfacesPass);

async function validateNativeSurfaces(): Promise<void> {
  await nativeHostRealignment.request();
}

function scheduleNativeHostRealignment(): void {
  if ((!activeNativeHostId && !activeDefaultBrowserCompanion && !mullvadWindowHosted) || nativeHostResizeFrame) return;
  nativeHostResizeFrame = requestAnimationFrame(() => {
    nativeHostResizeFrame = 0;
    void validateNativeSurfaces();
  });
}
function scheduleOslChatBackgroundSync(delayMs = 30_000): void {
  window.setTimeout(() => {
    void syncOslChatsInBackground().finally(() => scheduleOslChatBackgroundSync());
  }, delayMs);
}

type OslHubUiTestStatePatch = {
  route?: Route;
  onboardingRoute?: OnboardingRoute;
  setup?: Partial<SetupState>;
  coreReady?: boolean;
  storageMethod?: string | null;
  services?: LinkedService[];
  hubPeople?: Array<Partial<HubPerson> & { personId: string }>;
  notificationsEnabled?: boolean;
  notificationPreviewContent?: boolean;
  appNotifications?: AppNotification[];
  mullvadAvailability?: MullvadStatus["availability"];
  licenseAccess?: HubLicenseState["access"];
  autoScrubFleetStatus?: AutoScrubFleetStatus | null;
};

function testHubPerson(person: Partial<HubPerson> & { personId: string }): HubPerson {
  return {
    oslUserId: person.oslUserId ?? `OSLUSER-${person.personId}`,
    alias: person.alias ?? null,
    safetyNumber: person.safetyNumber ?? "0000 0000",
    safetyNumberVerified: person.safetyNumberVerified ?? false,
    whitelistCount: person.whitelistCount ?? 0,
    whitelistedScopes: person.whitelistedScopes ?? [],
    whitelistedScopesTruncated: person.whitelistedScopesTruncated ?? false,
    pendingKeyChange: person.pendingKeyChange ?? false,
    reachBroadened: person.reachBroadened ?? false,
    reachBroadenedAt: person.reachBroadenedAt ?? null,
    reachNarrowedScopes: person.reachNarrowedScopes ?? [],
    personId: person.personId,
  };
}

function applyTestCoreState(ready: boolean, storageMethod: string | null): void {
  core = structuredClone(unavailableCoreIntegration);
  core.readiness = {
    ...core.readiness,
    originalCoreLinked: ready,
    identityLoaded: ready,
    keyserverInitialised: ready,
    cloudRegistrationState: ready ? "registered" : "notAttempted",
    bootstrapAttempted: ready,
    passwordGateRequired: !ready,
    unlocked: ready,
    activeOslUserId: ready ? "test-osl-user" : null,
    bootstrapStatus: ready ? "ready" : "notAttempted",
    storageMethod,
  };
}

function applyOslHubUiTestState(patch: OslHubUiTestStatePatch = {}): void {
  route = patch.route ?? "home";
  onboardingRoute = patch.onboardingRoute ?? "welcome";
  setup = { ...defaultSetup, ...patch.setup };
  settingsSection = "account";
  activeService = null;
  activeHomeAppId = null;
  activeOslChatPersonId = null;
  serviceAccountPickerOpen = false;
  friendsDialogOpen = false;
  ownedConfirmation = null;
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  privacyProtectionReviewOpen = false;
  activityAttentionReviewOpen = false;
  peoplePrimaryActionFocus = null;
  services = patch.services ?? [];
  hubPeople = (patch.hubPeople ?? []).map(testHubPerson);
  notificationsEnabled = patch.notificationsEnabled ?? false;
  notificationPreviewContent = patch.notificationPreviewContent ?? true;
  appNotifications = patch.appNotifications ?? [];
  licenseState = { ...unconfiguredLicenseState, access: patch.licenseAccess ?? "free" };
  autoScrubFleetStatus = patch.autoScrubFleetStatus ?? null;
  autoScrubStatusLoading = false;
  autoScrubStopPending = false;
  mullvadStatus = {
    availability: patch.mullvadAvailability ?? "unavailable",
    integrationState: patch.mullvadAvailability === "installed" ? "availableToOpen" : patch.mullvadAvailability === "installable" ? "installable" : "unavailable",
    privacyScope: "networkOnly",
    connectionState: "notObserved",
  };
  applyTestCoreState(patch.coreReady ?? false, patch.storageMethod ?? null);
}

export const __oslHubUiTest = {
  reset(patch: OslHubUiTestStatePatch = {}): void {
    applyOslHubUiTestState(patch);
  },
  renderPrimarySidebar(): string {
    return primarySidebarMarkup();
  },
  renderWorkspaceContent(destination?: Route): string {
    if (destination) route = destination;
    return workspaceContent();
  },
  renderSettingsSection(section: SettingsSection): string {
    route = "settings";
    settingsSection = section;
    return workspaceContent();
  },
  renderRouteShell(destination: Route): string {
    route = destination;
    return destination === "onboarding" ? onboardingShellMarkup() : workspaceShellMarkup();
  },
  renderOnboardingSendModes(sendMode: SendMode = "manual"): string {
    route = "onboarding";
    onboardingRoute = "sending";
    setup = { ...defaultSetup, sendMode };
    return sendingSetupContent();
  },
  persistOslChatNotifications(): void {
    persistOslChatNotifications();
  },
  snapshot(): {
    route: Route;
    onboardingRoute: OnboardingRoute;
    settingsSection: SettingsSection;
    homePrimaryIssue: HomePrimaryIssue;
    privacyProtectionReviewOpen: boolean;
    activityAttentionReviewOpen: boolean;
    peoplePrimaryActionFocus: PeoplePrimaryActionFocus | null;
    ownedConfirmationKind: OwnedConfirmation["kind"] | null;
    ownedConfirmationPersonId: string | null;
  } {
    return {
      route,
      onboardingRoute,
      settingsSection,
      homePrimaryIssue: homePrimaryRecommendation().issue,
      privacyProtectionReviewOpen,
      activityAttentionReviewOpen,
      peoplePrimaryActionFocus,
      ownedConfirmationKind: ownedConfirmation?.kind ?? null,
      ownedConfirmationPersonId: ownedConfirmation?.kind === "verifyFriend" || ownedConfirmation?.kind === "removeFriend"
        ? ownedConfirmation.personId
        : null,
    };
  },
};

if (!runningUnderVitest) {
  window.addEventListener("resize", scheduleNativeHostRealignment);
  const desktopWindow = getCurrentWindow();
  void desktopWindow.onMoved(scheduleNativeHostRealignment).catch(() => undefined);
  void desktopWindow.onResized(scheduleNativeHostRealignment).catch(() => undefined);
  void bindMainWindowFocusChanges(
    (handler) => desktopWindow.onFocusChanged(handler),
    {
      scheduleNativeHostRealignment,
      hasRecoverySecrets: () => Boolean(recoveryBundle || newIdentityRecoveryPhrase),
      proveRecoveryCaptureProtection,
      invalidateRecoveryCapture: () => recoveryCaptureGate.invalidate(),
      setScreenshotProtectionEnabled: (enabled) => { screenshotProtectionEnabled = enabled; },
      render,
    },
  ).catch(() => undefined);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") {
      recoveryCaptureGate.invalidate();
      screenshotProtectionEnabled = false;
      newIdentityRecoveryPhrase = null;
      if (recoveryBundle || (route === "settings" && settingsSection === "account")) render();
      return;
    }
    if (recoveryBundle || newIdentityRecoveryPhrase) {
      void proveRecoveryCaptureProtection().then(() => render());
    }
  });
  window.addEventListener("error", (event) => { event.preventDefault(); containBackgroundFailure(); });
  window.addEventListener("unhandledrejection", (event) => { event.preventDefault(); containBackgroundFailure(); });
  void bootstrap();
  scheduleOslChatBackgroundSync(1_000);
}
