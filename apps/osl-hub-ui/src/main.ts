import "@fontsource-variable/inter/wght.css";
// The brand kit's two faces: Onest for UI, Source Sans 3 for body text. Bundled
// rather than fetched, so the first screen a person ever sees renders the same
// offline and on first run instead of flashing a fallback.
import "@fontsource-variable/onest/wght.css";
import "@fontsource-variable/source-sans-3/wght.css";
import "./styles.css";
import "./local-protected-sheet.css";
import "./friend-invite.css";
import "./friend-page.css";
import "./recovery-screen.css";
import "./onboarding-mullvad.css";
import "./appearance-settings.css";
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
import { chooseForwardSecrecyMode, initialForwardSecrecyOnboardingState, onboardingForwardSecrecyMarkup, type ForwardSecrecyChoice, type ForwardSecrecyOnboardingState } from "./onboarding-forward-secrecy";
import { onboardingPasswordRoleContent as passwordRoleContent } from "./password-roles";
import { backOnboardingPasswordRole, canSetOnboardingPasswordRole, continueOnboardingPasswordRole, skipOnboardingPasswordRole, togglePasswordVisibility, type OnboardingPasswordRoleValues } from "./onboarding-password-role";
import { chooseTorRoute, initialTorOnboardingState, onboardingTorMarkup, type TorOnboardingState } from "./onboarding-tor";
import { chooseCoverInsertion, initialCoverInsertionChoice, onboardingCoverMarkup, type CoverInsertionChoice } from "./onboarding-cover";
import { chooseSilentVisibleMode, onboardingSilentVisibleMarkup, type SilentVisibleMode } from "./onboarding-silent-visible";
import { onboardingCaptureVisibilityMarkup } from "./onboarding-capture-visibility";
import { continueButton, onOffToggle } from "./onboarding-controls";
import { identityChoiceMarkup, privateContactLinkMarkup, type IdentityDiscoveryChoice } from "./onboarding-identity";
import { CLEAN_FILES_CHOICES, initialBeforeSendChecks, onboardingBeforeSendMarkup, type BeforeSendChecks, type CleanFilesChoice } from "./onboarding-before-send";
import { firstTimedDeleteWarningMarkup, initialDeleteChoices, onboardingDeleteMarkup, timedDeleteContinueAllowed, type DeleteChoices } from "./onboarding-delete";
import { onboardingSendingMarkup } from "./onboarding-sending";
import { continuePasswordSetup } from "./password-setup-continue";
import { renderRecoveryStatesSettings } from "./recovery-states";
import {
  initialCleanDeviceRestoreState,
  refuseRestore,
  renderCleanDeviceRestoreStatus,
  restoreCheckFromFailure,
  restoreProgress,
  type CleanDeviceRestoreState,
  type RestoreCheck,
} from "./clean-device-restore";
import { continueFromProOnboarding, previousOnboardingRoute } from "./onboarding-sequence";
import { componentPickerScreen } from "./component-picker";
import { componentManagerFromOnboarding } from "./component-manager";
import { autoScrubConsentPrompt, decideAutoScrubInstall } from "./component-consent";
import { autoScrubTierStatus } from "./autoscrub-tier";
import { deviceTransferManifestScreen } from "./device-transfer";
import { initialOldDeviceCopyDecision, oldDeviceCopyDecisionView } from "./device-transfer-source";
import { renderDeadmanScreen, selectDeadmanAction } from "./deadman";
import { groupOnboardingApps } from "./onboarding-app-groups";
import {
  dragHomeTileArrangement,
  moveHomeTileArrangement,
  normalizeHomeTileArrangement,
  toggleHomeTileVisibility,
} from "./home-tile-arrangement";
import { peopleDestinationHeaderMarkup } from "./people-destination-header";
import { lastBackendFailure, recordBackendFailure } from "./backend-failure";
import { unlockAttemptWarning } from "./unlock-attempts";
import { chatPreviewHidingVisible } from "./entitlement-gates";
import { entitlementCopy } from "./entitlement-copy";
import { entitlementView } from "./entitlement-view";
import { chooseDetectedAccountOpening, detectedOpeningChoiceKey } from "./detected-account-opening";
import { bindProtectedTextBoxShortcutGuards } from "./protected-box-shortcuts";
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
  generatedCapabilityLabel,
  hostBrowserCompanion,
  hostNativeAppWindow,
  hostMullvadWindow,
  installNativeApp,
  installMullvad,
  loadLinkedServices,
  loadDetectedAccounts,
  loadMullvadStatus,
  loadNativeApps,
  nativeAppTileLabel,
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
  type DetectedAccount,
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
import { tileStatusPageMarkup } from "./tile-status-page";
import {
  coreReadinessLabel,
  checkHubRecoveryWordRetype,
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
  lockHubSession,
  removeHubAlternatePassword,
  setHubAlternatePassword,
  setupHubMainPassword,
  checkHubPasswordResetPhrase,
  resetHubMainPasswordAfterRecovery,
  unavailableCoreIntegration,
  unconfiguredLicenseState,
  unlockHubPasswordGate,
  validateHubActivationCode,
  type BootstrapStatus,
  type CoreIntegration,
  type HubLicenseState,
  type HubPasswordRoleStatus,
} from "./core";
import { checkHubForUpdates, installHubUpdate, openHubReleasesPage, openHubSourceRepository, type UpdateStatus } from "./updates";
import { createDiscordQaGeometryKeeper } from "./discord-qa-geometry";
import { composerLockAvailability } from "./composer-protection-trace";
import { browserLogo, serviceLogo, providerLogo } from "./logos";
import { appearanceColourRowsMarkup, bindAppearanceColourRows, loadAppearanceColours } from "./appearance-colours";
import { appearancePreviewMarkup, updateAppearancePreview, type AppearancePreviewService } from "./appearance-preview";
import { attachSettingsProfileBlock, restoreSettingsProfile, settingsProfileBlockMarkup } from "./settings-profile-block";
import { activateLocalLoopbackContext, activateManualPeerContext, activateNativeManualPeerContext, activateOslChatContext, addOslChatReaction, addOslFriend, addOslFriendByUsername, answerHubChatApprovalSuggestion, backBurnReview, burnActiveHubContext, burnHubServiceAccount, captureProtectionEnforced, closeOslChatContext, copyHubFriendInvite, createHubIdentitySlot, decryptLocalProtectedText, executeHubFullCleanup, getHubRevocationStatus, getHubServiceBurnReadiness, getOslUsernameStatus, isHubPlaintext, isNormalizedOslUsername, listHubIdentities, listHubPeople, listOslChatHistory, loadActiveContextSecurity, loadAppNotifications, loadBuildIntegrityStatus, loadFriendProfile, loadInstalledBuildChatWarningStatus, openOslChatText, openPeerProseText, peerIsVerified, prepareLocalProtectedText, prepareOslChatText, preparePeerProseText, recoverHubIdentitySlot, removeOslChatReaction, saveActiveContextSecurity, saveBurnReviewState, revokeActiveHubFriendScope, setActiveHubFriendPermission, setActiveHubFriendReach, setHubChatApprovalSuggestionChoice, setHubFriendNickname, setLocalProtectedSheetOpen, setNativeDiscordProtectedOverlayOpen, setNativeDiscordProtectedOverlayOpenForQa, setNotificationsEnabled, setScreenshotProtection, switchHubIdentity, verifyHubPerson, viewHubRecoveryPhrase, type AppNotification, type BuildIntegrityStatus, type HubIdentitySlot, type HubPerson, type HubPersonWhitelistScope, type HubServiceBurnReadiness, type InstalledBuildChatWarning, type LocalPrivacyScanResult, type ManualPeerContext, type PersistedLocalPrivacyScanResult } from "./adapters";
import { blankLocalProtectedModel, isLocalTtlSeconds, loadOrCreateLocalConversationId, localProtectedSheetMarkup, validLocalChatLabel, type LocalProtectedPane, type LocalProtectedSheetModel } from "./local-protected-sheet";
import { claimOslUsername, createHubPrivateContactLink, createOslFriendRequestByOslName, type HubPrivateContactLink } from "./adapters";
import { blankPeerProtectedModel, boundedPeerProtectedDraft, peerProtectedDraftByteFeedback, peerProtectedSheetMarkup, type PeerProtectedPane, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import { peerIntegrityMarkup } from "./peer-integrity";
import { futureAccountSwitchMarkup } from "./future-account-switch";
import { loadFutureAccountSwitchStates, saveFutureAccountSwitch } from "./future-account-switch-connect";
import oslLogoUrl from "../../osl-hub/icons/icon-cyan.png";
import oslVectorLogoUrl from "./assets/logo-mark.svg";
import oslGhostMarkUrl from "./assets/Ghost-white.svg";
import { importLocalMessageExport, LOCAL_MESSAGE_IMPORT_MAX_BYTES } from "./local-message-import";
import { clearPersistedLocalScrubExport, persistLocalScrubExport } from "./scrub-local";
import {
  defaultScrubConsentGateState,
  evaluateScrubConsentGate,
  scrubConsentGatedRouteMarkup,
  type ScrubConsentGateRequest,
  type ScrubConsentGateState,
} from "./scrub-consent-gate";
import type { ScrubRouteState, ScrubRouteStep } from "./scrub-route";
import { bindScrubDiscoveryScreen, scrubDiscoveryScreenMarkup } from "./scrub-discovery-screen";
import { buildScrubReviewList, type ScrubReviewRow } from "./scrub-review-list";
import { computeScopeFingerprint, type ScrubScopeFingerprintInput } from "./scrub-scope-fingerprint";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep, type ServiceGuideStep } from "./service-guide";
import { NativeDeadlineError, withNativeDeadline } from "./native-deadline";
import { CoalescedRealignment, NativeCallGate } from "./native-realignment";
import { bindWindowLifecycleRealignment } from "./window-lifecycle-bindings";
import { FrameRenderScheduler } from "./render-scheduler";
import { whitelistDropdownMarkup } from "./whitelist-dropdown";
import { settingsHomeExplanation, settingsHomeMenuItems, settingsHomeMenuMarkup, type SettingsHomeChoiceId } from "./settings-home";
import { whitelistingClearAll, whitelistingPendingChanges, whitelistingReset, whitelistingScreenMarkup, whitelistingSelectAll, whitelistingSetSearch, whitelistingToggleConversation, type WhitelistingConversation, type WhitelistingScreenState } from "./whitelisting-screen";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubSignalDefinitions, scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";
import { loadMassCleanupCapabilities, type MassCleanupCapabilityManifest } from "./mass-cleanup";
import { initialMessageDefaultsScreenState, savedMessageDefaultLabels } from "./message-defaults";
import { projectAutoScrubFleetStatus, type AutoScrubFleetStatus } from "./autoscrub-contract";
import { freshStartCleanupPresentation, freshStartLimitationsMarkup } from "./fresh-start";
import { loadAutoScrubRunFleetStatus, requestAutoScrubGlobalStop } from "./autoscrub-unattended-run";
import { oslMailStage, type OslMailStage } from "./desktop-service-policy";
import { webSurfaceLabel, type WebSurfaceCapability } from "./web-surface-label";
import { homeOverallStatus, homeProtectionState } from "./home-protection-state";
import {
  autoScrubActivityRecordMarkup,
  autoScrubHomeActivityMarkup,
  openAutoScrubHomeActivity,
  type AutoScrubHomeActivityRecord,
} from "./autoscrub-home-activity";
import {
  OSL_MAIL_NAMED_SEND_REQUIRED,
  acknowledgeOslMailRetrieval,
  burnOslMailbox,
  listOslMailThreads,
  loadOslMailStatus,
  provisionOslMail,
  retrieveOslMailThread,
  sendOslMailWithChoice,
  type OslMailSendChoice,
  type OslMailBurnReceipt,
  type OslMailDeleteReceipt,
  type OslMailRetrievedThread,
  type OslMailSendReceipt,
  type OslMailStatus,
  type OslMailThreadSummary,
} from "./osl-mail-adapter";
import { oslMailViewMarkup, type OslMailComposeDraft, type OslMailPane } from "./osl-mail-view";
import { oslServersViewMarkup } from "./osl-servers-view";
import { bindOwnerRoleEditor, OwnerRoleEditor, ownerRoleEditorMarkup } from "./owner-role-editor";
export {
  autoscrubUnattendedContractGate,
  autoscrubUnattendedProductionRun,
  type AutoscrubUnattendedGateResult,
  type AutoscrubUnattendedRunResult,
} from "./autoscrub-unattended-run";
import { initializeThemePreference, themeStorageKey, type ThemeChoice } from "./theme-preference";
import { defaultWindowSoundsSettings, loadWindowSoundsSettings, saveWindowSoundsSettings, windowSoundsSettingsMarkup, type WindowPosition, type WindowSoundsSettings } from "./window-sounds-settings";
import { accentChoices, appearanceSettingsMarkup, avatarChoices, backgroundChoices, loadAppearancePreferences, resetAppearancePreferences, saveAppearancePreferences, windowPositionChoices, type AppearancePreferences } from "./appearance-preferences";
import { applyLookState, defaultLookState, loadLookState, lookScreenMarkup, lookStorageKey, saveLookState, type LookMode, type LookState } from "./look-screen";
import { inDomTooltipMarkup } from "./in-dom-tooltip";
import { coverWritingControlsMarkup } from "./cover-writing-controls";
import { applyOslChatDraftToElement, firstPartyOslSurfaceContract, OSL_CHAT_KEY_CHANGED_REFUSAL_REASON, OSL_CHAT_MAX_DRAFT_BYTES, oslChatDraftBytes, oslChatHandshakeConfirmed, oslChatsViewMarkup, senderReceiptStateFor, submitsOslChatDraft, type OslChatMessage } from "./osl-chats-view";
import { createOslChatDeliveryRuntime, mergeOslChatTimeline, oslChatHistoryMessages, oslChatOpenRefusalMessage, pruneExpiredOslChatMessages, receivedOslChatBatchMessage, type OslChatDeliveryHost } from "./osl-chat-runtime";
import { attachChatProfileAppearanceModal } from "./chat-profile-appearance-modal";
import { OslProfilePaneState, seededProfilePaneRecords, type ScopedProfileRecord } from "./osl-profile-pane";
import { mountChatBackgroundPane } from "./chat-background-pane";
import { renderChatMessagesPane } from "./chat-messages-pane";
import { bindSafetyNumberPanel, safetyNumberPanelMarkup } from "./safety-number-panel";
import { peopleReverificationNoticeMarkup } from "./people-reverification-notice";
import { discordQaWhitelistButtonMarkup } from "./discord-qa-whitelist-button";
import { connectDiscordQaWhitelistButton, discordQaOpenPlace } from "./discord-qa-whitelist-place";
import { parseEnclaveAudience, type EnclaveAudience } from "./osl-collab";
import { startDirectConversation, startEnclave, startGroupConversation, startSomethingSheetMarkup, type EnclaveJoiningRule, type StartSomethingDependencies, type StartSomethingPerson } from "./start-something";
import { addFriendByNameBoxMarkup, addFriendFailureStatus, bindAddFriendByNameForm, bindFriendRemovalControls, bindMainWindowFocusChanges, friendHandshakeDetail, friendHandshakeSummary, friendInviteCardMarkup, friendRemovalButtonMarkup, friendTrustAction, friendVerificationCopy, friendWideWhitelistButtonsMarkup, inviteCopyFailureToast, onboardingPaintDecision, ownedConfirmationSubmitDisabled, RecoveryCaptureGate, removeHubFriend, shouldClearRemovedFriendChat, verificationSubmission, type FriendVerificationCopy, type PendingFriendRequestEntry } from "./ui-behavior";
import { runRecoveryReveal, submitsRecoveryReveal } from "./recovery-reveal";
import { addLegacyPhraseWrap, initialAccountRecoveryFlow, legacyMarkerRecoveryRefused, legacyRecoveryMigrationMarkup, recoveryScreenMarkup, submitRecoveredPassword, submitRecoveryPhrase, type AccountRecoveryDependencies, type AccountRecoveryFlow, type LegacyRecoveryMigration, type RecoveryMigrationDependencies } from "./account-recovery";
import { RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT, recoveryKitReducer, recoveryKitSecretCardsMarkup, recoveryKitView, visibleRecoverySecrets, type RecoveryKitAction, type RecoveryKitState, type RecoveryKitView } from "./recovery-kit";
import { applyRecoveryWordRetypeResult, everyRecoveryWordAnswered, initialRecoveryWordCheckState, recoveryWordCheckContinueDisabled, recoveryWordCheckMarkup, recoveryWordRetypeRequest, setRecoveryWordCheckAnswer, type RecoveryWordCheckState } from "./recovery-word-check";
import { resumeOnboardingRoute } from "./onboarding-resume";
import { createRecoveryKitUnsavedFlag } from "./recovery-kit-flag";
import { loadHubRecoveryKitUnsaved, setHubRecoveryKitUnsaved } from "./adapters";
import { burnFeatureClaimsMarkup } from "./feature-claims";
import { removeEverythingScreenMarkup } from "./remove-everything-screen";
import { burnRevocationReceipt, type BurnRevocationReceipt } from "./burn-revocation-receipt";
import { senderReceiptStatus } from "./receipt-status";
import { attachmentProgressMarkup, parseAttachmentProgressEvent, type AttachmentProgressEvent } from "./attachment-progress";
import { attachOslChatComposerDragAndDrop, attachmentTrayMarkup, createOslChatAttachmentTray, type OslChatAttachmentTrayState } from "./chat-attachment-drop";
import { createAttachmentTrayActions } from "./attachment-tray-actions";
import { attachmentTrayScreenMarkup } from "./attachment-tray-screen";
import { bindPrivateTypingBoxDropTarget } from "./private-typing-drop-target";
import { burnReviewScreenMarkup, initialBurnReviewScreenState, selectBurnReviewSide, toggleBurnReviewHideOtherPeople, type BurnReviewSide, type BurnReviewScreenState } from "./burn-review-screen";
import { destructStatusMarkup, type ServerDestructStatus } from "./destruct-status";
import { discoveryVisibilityBody } from "./discovery-visibility-screen";
import { DISCOVERY_VISIBILITY_STORAGE_KEY, defaultDiscoveryVisibilityState, readSavedDiscoveryVisibility, selectDiscoveryChoice, serializeDiscoveryVisibility, setDiscoveryReplyToPings, type DiscoveryVisibilityState } from "./discovery-visibility";
import { offlineCapabilityStatus, type OfflineUnavailableCapability, type OslConnectionState } from "./offline-capability-status";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";
import type { NativeOverlayPendingAttachment } from "./overlay-state";
import { acceptOslChatClipboardImageAttachment, listOslChatAttachments, openOslChatAttachment, selectOslChatAttachment } from "./native-overlay-adapter";
import { VerificationWarningMemory, verificationWarningDecision, type VerificationWarningSetting, type VerificationWarningSurface } from "./verification-warning";
import {
  pollNativeDiscordHeadlessQa,
  requestNativeDiscordVisibleRowRuntimeReceipt,
  runNativeDiscordHeadlessQa,
} from "./discord-headless-qa-adapter";
import type { SecureLocalStore } from "./secure-local-store";
import { createOslChatSecureLocalStore } from "./osl-chat-secure-store";
import {
  oslChatNotificationPreview,
  oslChatNotificationSettingsMarkup,
  readOslChatNotificationSettings,
  setOslChatNotificationSwitch,
  type OslChatNotificationSwitch,
} from "./osl-chat-notification-settings";
import { PublicNamePageController } from "./public-name-page";

export type Route = "onboarding" | "home" | "arrange-tiles" | "inbox" | "people" | "privacy" | "scrub" | "activity" | "connections" | "service" | "settings" | "mullvad" | "osl-chat" | "osl-mail" | "osl-mail-status" | "osl-notes-status" | "osl-servers" | "signal-qa";

/**
 * The colour a status chip is allowed to claim, resolved from the word printed
 * inside it.
 *
 * Every chip used to be painted with the success colour unconditionally, so
 * "UNAVAILABLE", "NEEDS ATTENTION", "REFUSED" and "DELETION OFF" all rendered
 * green: colour contradicting the only thing on screen that was telling the
 * truth. Colour is never the sole signal here -- the label always spells the
 * state out, and this only decides whether the chip's colour agrees with it.
 *
 * A label this table does not recognise gets no tone at all. An unclassified
 * state is not a verified-good one, and silence is the only honest default.
 */
const STATUS_TAG_TONES: ReadonlyArray<readonly [tone: "ok" | "warn" | "danger", labels: readonly string[]]> = [
  ["ok", ["active", "allowed", "applied", "available", "encrypted", "fail closed", "on", "private", "pro active", "protected", "quiet", "ready", "recorded", "set", "verified"]],
  ["warn", ["ask first", "carrier preview", "coming later", "coming later · pro", "installable", "limit shown", "manual", "needs review", "open a supported chat first", "pending", "pro", "pro planned", "request only", "review", "review first", "reviewing"]],
  ["danger", ["blocked", "deletion off", "failed", "needs attention", "no auto delete", "no cleanup run", "not possible", "not set up", "off", "refused", "revoked", "unavailable"]],
];

function statusTagTone(label: string): string {
  const normalized = label.trim().toLowerCase();
  for (const [tone, labels] of STATUS_TAG_TONES) if (labels.includes(normalized)) return tone;
  return "";
}

/**
 * A status chip whose colour is derived from its own text. `extra` carries any
 * caller-supplied modifier class through unchanged.
 */
function statusTag(label: string, extra = ""): string {
  const classes = ["status-tag", statusTagTone(label), extra].filter(Boolean).join(" ");
  return `<span class="${classes}">${label}</span>`;
}

/** The claim row for one connected app: the label, and the sentence behind it. */
function nativeClaimMarkup(app: NativeApp): string {
  return `<p class="native-claim-note" data-claim-status="${app.supportStatus}" data-carrier-evidence="${app.carrierEvidence}" data-delivery-evidence="${app.deliveryEvidence}" data-status-page-capability="${escapeHtml(app.statusPage.capability)}">${statusTag(app.statusPage.generatedLabel)} ${escapeHtml(app.statusPage.explanation)}</p>`;
}

/**
 * What OSL has proven about carrying a message through somebody else's app,
 * stated as a count rather than left in a plan document.
 *
 * Computed from the catalog on every render, so the day a surface earns a
 * receipt this sentence changes on its own.
 */
/**
 * The census as it is rendered. Built here rather than inline at the call site
 * so the sentence lives next to the contract it reports on.
 */
function carrierReceiptCensusMarkup(apps: readonly NativeApp[]): string {
  const earned = apps.filter((app) => app.carrierEvidence === "provenLiveWithReceipt").length;
  return `<p class="carrier-receipt-census" data-carrier-receipt-census="${earned}">${escapeHtml(carrierReceiptCensusLine(apps))}</p>`;
}

function carrierReceiptCensusLine(apps: readonly NativeApp[]): string {
  const earned = apps.filter((app) => app.carrierEvidence === "provenLiveWithReceipt").length;
  // No denominator is quoted, because this list is the connected apps on THIS
  // device and not the full surface list. Quoting a total the view does not have
  // would be the same shortcut the claim state exists to refuse.
  return earned === 0
    ? "None of the apps above has earned a live carry receipt. OSL has not proven that it can place a protected message in another app's composer."
    : `${earned} of the apps above have earned a live carry receipt. The rest are unproven.`;
}

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
const onboardingRouteValues = ["pro", "welcome", "create", "import", "unlock", "keylost", "account-recovery", "recovery", "recovery-check", "identity-choice", "private-link", "mullvad", "sending", "defaults", "tor", "forward-secrecy", "cover", "silent-visible", "visibility", "passwords", "burnpass", "privacy", "tutorial", "detected", "install", "apps", "browser", "decoy"] as const;
type OnboardingRoute = typeof onboardingRouteValues[number];
// Derived, never re-declared: the Settings sections ARE the Settings home
// choices. A hand-copied union here once drifted from settings-home.ts (it
// lost "privacy"), settingsHomeMenuMarkup's guard threw on every render, and
// the error boundary blanked the whole Settings screen.
type SettingsSection = SettingsHomeChoiceId;
type SavedAccountMode = "ask" | "use" | "clean";
type BurnScope = "chat" | "app" | "account";

/**
 * Temporary display authority for the one monthly allowance.
 *
 * STORAGE-RULING.txt calls these figures provisional.  evidence/0575.md
 * subsequently records a different Pro figure and service ceiling, so this is
 * deliberately named after its source rather than presented as a settled
 * product decision.  Change this single constant when the owner settles it.
 */
const DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING = {
  source: "STORAGE-RULING.txt · 6 August · provisional",
  freeBytes: 1_000_000_000,
  proBytes: 16_000_000_000,
  warningPercent: 90,
  conflict: "evidence/0575.md records Pro as 150 GB and a revenue-linked $250/month floor; the owner must settle the conflict before sale.",
} as const;

type DataAllowanceCategory = "backgroundConnection" | "messages" | "attachments" | "storiesAndPosts" | "deviceSync" | "voice";

/**
 * The UI has no ledger from the relay/storage service yet.  Keep the absence
 * distinct from zero: a displayed zero would tell a person that OSL counted
 * everything when it has not.  The category list is the contract the ledger
 * must fill -- one pool, no exempt traffic.
 */
const dataAllowanceLedger: Readonly<Record<DataAllowanceCategory, number | null>> = {
  backgroundConnection: null,
  messages: null,
  attachments: null,
  storiesAndPosts: null,
  deviceSync: null,
  voice: null,
};
type BurnResult = {
  tone: "success" | "warning" | "error";
  message: string;
  showUninstall: boolean;
  destructServerStatus?: ServerDestructStatus;
  /**
   * Peer-acknowledgement half of a chat burn, shown as its own line. Absent for
   * the burn scopes that queue no peer revocation, so nothing is implied about
   * a conversation that was never asked about.
   */
  revocation?: BurnRevocationReceipt;
};
type OwnedConfirmation =
  | { kind: "verifyFriend"; personId: string }
  | { kind: "removeFriend"; personId: string }
  | { kind: "clearActivation" };

type ProtectionPreset = "basic" | "balanced" | "maximum";
type InboxFilter = "all" | "osl" | "connected" | "requests";
const protectionPresetStorageKey = "osl-protection-preset-v1";
const oslMailNotificationsStorageKey = "osl-mail-notifications-v1";
const protectionPresetValues: readonly ProtectionPreset[] = ["basic", "balanced", "maximum"];
const inboxFilterValues: readonly InboxFilter[] = ["all", "osl", "connected", "requests"];
const quickTourRoute: OnboardingRoute = "tutorial";
const quickTourCards = [
  ["Lock", "Lock turns on protected input and protected send routing. Turn it off to use the host app’s ordinary composer."],
  ["Eye", "Eye controls decrypted display only. With Eye off, OSL leaves the native carrier rows untouched."],
  ["Cyan ring", "The small cyan ring around the native composer means protected input is active. No ring means you are typing into the host’s plaintext composer."],
  ["Send mode", "Choose Manual, Clipboard, Double Enter or Single Enter. No mode silently sends: OSL stops if it cannot prove the exact destination."],
  ["App limits", "Protection is limited to the app, account, chat, and composer OSL can verify. If that proof changes, OSL refuses protected routing rather than guessing."],
] as const;

function parseProtectionPreset(raw: unknown): ProtectionPreset {
  return protectionPresetValues.includes(raw as ProtectionPreset) ? raw as ProtectionPreset : "balanced";
}

function parseInboxFilter(raw: unknown): InboxFilter {
  return inboxFilterValues.includes(raw as InboxFilter) ? raw as InboxFilter : "all";
}

function loadProtectionPreset(storage: Pick<Storage, "getItem"> = localStorage): ProtectionPreset {
  return parseProtectionPreset(storage.getItem(protectionPresetStorageKey));
}

function persistProtectionPreset(storage: Pick<Storage, "setItem"> = localStorage): void {
  storage.setItem(protectionPresetStorageKey, protectionPreset);
}

function requireRoot(): HTMLDivElement {
  const element = document.querySelector<HTMLDivElement>("#app");
  if (!element) throw new Error("OSL Privacy root is missing");
  return element;
}
const runningUnderVitest = Boolean(import.meta.vitest || (typeof process !== "undefined" && process.env.VITEST));
const fixedNoRecoverySecretFixture = !runningUnderVitest
  && import.meta.env.DEV
  && new URLSearchParams(window.location.search).get("osl-fixture") === "no-recovery-secret";
const root = runningUnderVitest
  ? (globalThis.document?.querySelector<HTMLDivElement>("#app") ?? globalThis.document?.createElement("div") ?? {} as HTMLDivElement)
  : requireRoot();
const discordQaShell = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1";
const signalQaShellEnabled = import.meta.env.VITE_OSL_SIGNAL_QA_SHELL === "1";
if (discordQaShell) document.documentElement.classList.add("discord-qa-shell");

function onboardingRouteForBuild(candidate: OnboardingRoute): OnboardingRoute {
  return discordQaShell && (candidate === "pro" || candidate === "passwords") ? "sending" : candidate;
}

function isOnboardingRoute(value: unknown): value is OnboardingRoute {
  return typeof value === "string" && onboardingRouteValues.includes(value as OnboardingRoute);
}

function handleOnboardingRouteAction(rawRoute: unknown): boolean {
  if (!isOnboardingRoute(rawRoute)) {
    showToast("Unknown onboarding route refused");
    return false;
  }
  onboardingRoute = onboardingRouteForBuild(rawRoute);
  if (onboardingRoute === "import") cleanDeviceRestoreState = initialCleanDeviceRestoreState;
  // Arriving at recovery always starts at the phrase step: a half-finished
  // flow, or a token from a previous attempt, must never be inherited.
  if (onboardingRoute === "account-recovery") resetAccountRecovery();
  render();
  return true;
}


function passwordEyeIcon(visible = false): string {
  return `<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M1.8 10s2.9-4.7 8.2-4.7 8.2 4.7 8.2 4.7-2.9 4.7-8.2 4.7S1.8 10 1.8 10Z"/><circle cx="10" cy="10" r="2.25"/>${visible ? "" : '<path d="M3 3l14 14"/>'}</svg>`;
}

let services: LinkedService[] = [];
let core: CoreIntegration = structuredClone(unavailableCoreIntegration);
let licenseState: HubLicenseState = structuredClone(unconfiguredLicenseState);
let proOnboardingReadyResult = false;
let proOnboardingCodeEntryRequested = false;
let massCleanupCapabilities: MassCleanupCapabilityManifest | null = null;
let massCleanupLoading = false;
let autoScrubFleetStatus: AutoScrubFleetStatus | null = null;
let autoScrubStatusLoading = false;
let autoScrubStopPending = false;
let autoScrubOpenedActivityRecord: AutoScrubHomeActivityRecord | null = null;
let passwordRoleStatus: HubPasswordRoleStatus | null = null;
const ownerRoleEditor = new OwnerRoleEditor();
// "Forgot password?" (the `data-onboarding="account-recovery"` link on the
// unlock card) rendered `recoveryScreenMarkup(initialAccountRecoveryFlow)` --
// always the *initial* flow, with no submit handler on either form. Typing a
// recovery phrase and pressing "Verify phrase" did nothing at all, silently.
// The flow now lives here so the already-specified state machine in
// account-recovery.ts actually runs and its refusals reach the screen.
let accountRecoveryFlow: AccountRecoveryFlow = initialAccountRecoveryFlow;
let legacyRecoveryMigration: LegacyRecoveryMigration | null = null;
// The approved phrase remains in memory only for the short interval between
// the native phrase check and the native reset. The opaque token ties that
// phrase approval to the flow state; neither value is persisted or rendered.
let approvedAccountRecovery: { phrase: string; token: string } | null = null;
let accountRecoveryDependencies: AccountRecoveryDependencies = {
  verifyPhrase: async (phrase) => {
    const checked = await checkHubPasswordResetPhrase(phrase);
    if (checked.status !== "approved" || !checked.recoveryToken) {
      approvedAccountRecovery = null;
      return { ok: false, lockoutStatus: checked.lockoutStatus };
    }
    approvedAccountRecovery = { phrase, token: checked.recoveryToken };
    return { ok: true, recoveryToken: checked.recoveryToken, lockoutStatus: checked.lockoutStatus };
  },
  setPassword: async (newPassword, recoveryToken) => {
    const approved = approvedAccountRecovery;
    if (!approved || approved.token !== recoveryToken) throw new Error("Password reset requires a fresh phrase approval");
    await resetHubMainPasswordAfterRecovery(approved.phrase, newPassword);
    approvedAccountRecovery = null;
  },
};
let recoveryMigrationDependencies: RecoveryMigrationDependencies = {
  addPhraseWrap: () => Promise.reject(new Error("Recovery migration is unavailable in this build")),
  freshStart: () => Promise.reject(new Error("Recovery migration is unavailable in this build")),
};
let setup: SetupState = parseSetupState(null);
let route: Route = "onboarding";
let onboardingRoute: OnboardingRoute = "welcome";
// The tour is deliberately separate from setup completion: people can replay
// it from Settings without changing their account, app, or sending choices.
let onboardingTourStep = 0;
let replayingOnboardingTour = false;
let torOnboarding: TorOnboardingState = initialTorOnboardingState();
let silentVisibleMode: SilentVisibleMode | null = null;
// Which of the two insertion styles is highlighted. It starts unset so setup
// cannot silently accept a default the owner never chose.
let coverInsertion: CoverInsertionChoice | null = initialCoverInsertionChoice();
// The three before-send checks. Live on screen; not yet persisted, because
// nothing reads them at send time yet.
let beforeSendChecks: BeforeSendChecks = initialBeforeSendChecks();
// What OSL is allowed to delete on this device. Both start off; nothing is
// deleted unless it is turned on here. If this record is ever missing, the
// review step fails closed instead of advancing on implied defaults.
let deleteChoices: DeleteChoices | null = initialDeleteChoices();
// In-memory and false on every launch: the warning is the boundary before the
// first timed delete, not a preference silently inherited from WebView storage.
let timedDeleteWarningAgreed = false;
let forwardSecrecyOnboarding: ForwardSecrecyOnboardingState = initialForwardSecrecyOnboardingState();
let forwardSecrecyMode: "protectPast" | "keepGroupDelivery" = "keepGroupDelivery";
// A cache only. The authority is encrypted account state in the native hub;
// WebView storage is deliberately not consulted because burn/duress erase it.
// NEW-1: the mirror moves synchronously with the owner's decision, not with the
// native write — see `recovery-kit-flag.ts` for why.
const recoveryKitUnsavedFlag = createRecoveryKitUnsavedFlag({
  storage: localStorage,
  read: () => loadHubRecoveryKitUnsaved(),
  write: (unsaved) => setHubRecoveryKitUnsaved(unsaved),
});
let settingsSection: SettingsSection = "account";
// Account-local review state. The final typed confirmation remains in the
// existing Burn dialog, after this page has shown its scope summaries.
let removeEverythingScreenOpen = false;
let windowSoundsSettings: WindowSoundsSettings = loadWindowSoundsSettings();
let discoveryVisibility: DiscoveryVisibilityState = defaultDiscoveryVisibilityState();
let discoveryVisibilityStatus: string | null = null;
// Appearance keeps a small, local draft: the preview deliberately reads this
// object rather than waiting for a server-side profile save.
const appearanceProfileState = new OslProfilePaneState([{
  scope: { kind: "global" },
  useSeparateProfileHere: false,
  displayName: "Your name",
  aboutLine: "",
  status: "Your vibe goes here.",
  cardBackground: "#0d1114",
  avatar: null,
  colour: "#2ac0f0",
}]);
restoreSettingsProfile(appearanceProfileState);
let activeService: LinkedService | null = null;
let activeHomeAppId: HomeAppId | null = null;
let appLaunchPendingId: HomeAppId | null = null;
let nativeApps: NativeApp[] = [];
let nativeCatalogBusy = false;
/**
 * Why the last "Choose apps" Continue refused, or null if it did not refuse.
 *
 * D-190. A refusal used to exist only as a toast, which is gone in 2.5s and
 * carries no way out; the reporter clicked Continue ~25 times and the panel never
 * changed. This keeps the refusal on the panel until the state that caused it
 * changes, and `chooseAppsOnboardingContent` renders an explicit escape beside it.
 */
let nativeCatalogRefusal: string | null = null;
let mullvadStatus: MullvadStatus = {
  availability: "unavailable",
  integrationState: "unavailable",
  privacyScope: "networkOnly",
  connectionState: "notObserved",
};
let mullvadBusy = false;
let mullvadSetupNotice = "";
let mullvadSetupRoute: MullvadSetupRoute | null = null;
let mullvadAutoStart = false;
let mullvadAutoStartAttempted = false;
let mullvadWindowHosted = false;
let mullvadReturnRoute: "onboarding" | "home" | "connections" = "home";
let protectionPreset: ProtectionPreset = loadProtectionPreset();
let inboxFilter: InboxFilter = "all";
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
let detectedAccounts: DetectedAccount[] = [];
let detectedAccountOpeningChoices = new Map<string, "windowsApp" | "browser">();
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
let cleanDeviceRestoreState: CleanDeviceRestoreState = initialCleanDeviceRestoreState;
let recoverySavedAcknowledged = false;
let recoveryNoSecretAcknowledged = false;
let recoveryWordCheckState: RecoveryWordCheckState = initialRecoveryWordCheckState();
let recoveryWordCheckEpoch = 0;
// T15-A7: the owner typed the acknowledgement and asked to see the kit even
// though capture resistance is not proven. In-memory only, and reset the
// moment the recovery step is left.
let recoveryShownWithoutProtection = false;
let recoveryRevealError: string | null = null;
let recoveryRevealBusy = false;
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
let appearancePreferences: AppearancePreferences = loadAppearancePreferences(localStorage);
let savedAppearancePreferences: AppearancePreferences = { ...appearancePreferences };
let lookState: LookState = localStorage.getItem(lookStorageKey) === null
  ? { ...defaultLookState, mode: themeChoice === "system" ? "computer" : themeChoice }
  : loadLookState(localStorage);
let sidebarOrder: string[] = [];
let hiddenServices = new Set<string>();
let homeEditMode = false;
let homeTileOrder: string[] = [];
let hiddenHomeTiles = new Set<string>();
let homeTilePreferenceOwner: string | null = null;
let draggingHomeTileId: string | null = null;
let homeTileArrangementNotice = "";
let friendCode: string | null = null;
let friendDisplayId: string | null = null;
let claimedOslUsername: string | null = null;
const publicNamePage = new PublicNamePageController();
let oslMailLoading = false;
let oslMailStatus: OslMailStatus | null = null;
let oslMailThreads: OslMailThreadSummary[] = [];
let oslMailActiveThread: OslMailRetrievedThread | null = null;
let oslMailPane: OslMailPane = "inbox";
let oslMailComposeDraft: OslMailComposeDraft = { to: "", subject: "", body: "" };
let oslMailNotifications = localStorage.getItem(oslMailNotificationsStorageKey) !== "false";
let oslMailDeleteReceipt: OslMailDeleteReceipt | null = null;
let oslMailSendReceipt: OslMailSendReceipt | null = null;
let oslMailBurnReceipt: OslMailBurnReceipt | null = null;
let oslMailError: string | null = null;
let oslMailThreadSyncUnavailable = false;
let escapeAuditSendAttempts = 0;
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
// Whitelisting screen. The saved answer always comes from the hub (which chats
// each verified person is approved in); these two hold only what the user has
// typed and ticked since the screen was opened, so a re-render never invents an
// approval and Reset has something real to go back to.
let whitelistingSearch = "";
let whitelistingDraft: readonly string[] | null = null;
let whitelistingBusy = false;
let onboardingComplete = false;
let screenshotProtectionEnabled = false;
let linkedServicesChecked = false;
let windowCaptureEnabled = true;
let hubIdentities: HubIdentitySlot[] = [];
// An empty `hubIdentities` used to be read as "OSL is locked", which is a
// different question answered by a different source of truth. Track the load
// itself so "never loaded", "the backend refused", and "genuinely empty" stay
// three separate states and none of them can masquerade as a lock.
type IdentityListLoad = "pending" | "loaded" | "unavailable";
let hubIdentitiesLoad: IdentityListLoad = "pending";
let identityListRefreshInFlight = false;
let newIdentityRecoveryPhrase: string | null = null;
const recoveryCaptureGate = new RecoveryCaptureGate();
const RECOVERY_PROTECTION_REFUSAL = "OSL cannot show recovery secrets because Windows capture resistance is not proven for this window";
let hubPeople: HubPerson[] = [];
let activeOslChatPersonId: string | null = null;
let activeOslChatContext: ManualPeerContext | null = null;
let oslChatDraft = "";
let oslChatViewOnce = false;
let oslChatBusy = false;
let oslChatOperationEpoch = 0;
const oslChatMessages = new Map<string, OslChatMessage[]>();
const oslChatUnread = new Map<string, number>();
let lastOslChatOpenRefusal: string | null = null;
let oslChatVerificationWarningSetting: VerificationWarningSetting = "every-time";
const oslChatVerificationWarningMemory = new VerificationWarningMemory();
let oslChatVerificationWarningSurface: VerificationWarningSurface = "none";
let oslChatPreviewsVisible = true;
let oslChatMutedPeople = new Set<string>();
let oslChatSettingsPersonId: string | null = null;
let oslChatFilter: "direct" | "groups" | "enclaves" = "direct";
let oslChatSearch = "";
let oslChatSendBlockedReason: string | null = null;
let chatProfileAppearanceOpen = false;
let chatAppearancePane: "profile" | "background" | "messages" = "profile";
const chatProfileAppearanceState = new OslProfilePaneState(seededProfilePaneRecords());
let safetyNumberPanelPersonId: string | null = null;
let oslChatAttachments: NativeOverlayPendingAttachment[] = [];
const oslChatDropTray: OslChatAttachmentTrayState = createOslChatAttachmentTray();
const oslMailDropTray = createAttachmentTrayActions();
let buildIntegrityStatus: BuildIntegrityStatus | null = null;
let startSomethingChoice: "direct" | "group" | "enclave" | null = null;
let startSomethingJoiningRule: EnclaveJoiningRule = "invite_only";
let startSomethingBusy = false;
const attachmentProgressByContext = new Map<string, AttachmentProgressEvent>();
let privacyScanResult: LocalPrivacyScanResult | PersistedLocalPrivacyScanResult | null = null;
let privacyScanFileName: string | null = null;
let persistedLocalScrubImportId: string | null = null;
let privacyScanBusy = false;
let enabledScrubSignals = new Set<ScrubSignalGroup>(defaultScrubSignalGroups);
let selectedScrubFindings = new Set<number>();
let scrubResultsPage = 0;
let scrubReviewOpen = false;
let scrubReviewPage = 0;
// The fingerprint of the exact scope the owner is looking at, kept beside the
// scan it describes. `key` is the input it was computed from, so a re-render
// never recomputes and never shows a digest for a scope that has since changed.
let scrubScopeFingerprint: { readonly key: string; readonly value: string } | null = null;
const localScrubConsentRequest: ScrubConsentGateRequest = {
  serviceId: "local-export",
  serviceName: "message export",
  warning: "Scrub can permanently delete messages from a connected service and may cause account termination. Confirm this risk before scanning or reviewing a deletion preview.",
};
let localScrubConsentState: ScrubConsentGateState = defaultScrubConsentGateState();
let localScrubRouteOpened = false;
let localScrubRouteStep: ScrubRouteStep = "choose";
let localScrubRouteAccountSelected = false;
let localScrubRouteCategories = new Set<ScrubSignalGroup>();
let lastFocusKey = "";
let lastOnboardingMarkup: string | null = null;
let renderedOnboardingRoute: OnboardingRoute | null = null;
/**
 * Set by a flow that is answering the owner's own keystroke, so its paint is
 * not deferred by the "someone is typing a password" guard in
 * `renderOnboarding`. Cleared by the pass that consumes it.
 */
let forceOnboardingPaint = false;
let lastWorkspaceMarkup: string | null = null;
let lastWorkspaceViewKey = "";
let deferredBackgroundRender = false;
let serviceGuideStep: ServiceGuideStep | null = null;
let nativeHostFailureNotice = "";
let friendsDialogOpen = false;
let friendsDialogPage = 0;
// Home launcher chrome state: the bell popover in the shared header, and the
// collapsible Friends panel on the right (design export 2026-08-06, Home).
let homeNotificationsOpen = false;
let homeFriendsPanelCollapsed = false;
const pendingFriendRequestsByName: PendingFriendRequestEntry[] = [];
// The Hub owns this preference. The map is only the most recently loaded or
// acknowledged state used to draw each friend's switch.
const friendFutureAccountAutoWhitelist = new Map<string, boolean>();
const friendFutureAccountAutoWhitelistBusy = new Set<string>();
let burnDialogOpen = false;
let burnScope: BurnScope = "chat";
let burnBusy = false;
let burnResult: BurnResult | null = null;
let serviceBurnReadiness: HubServiceBurnReadiness | null = null;
let serviceBurnReadinessBusy = false;
let burnReviewScreenOpen = false;
let burnReviewScreenState: BurnReviewScreenState = initialBurnReviewScreenState();
let ownedConfirmation: OwnedConfirmation | null = null;
let ownedConfirmationBusy = false;
let ownedConfirmationError = "";
let navigationIntentEpoch = 0;
let bootstrapEpoch = 0;
let installedBuildChatWarning: InstalledBuildChatWarning | null = null;

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
const detectedAccountOpeningChoicesStorageKey = "osl-detected-account-opening-choices-v1";
const HOME_TILE_ARRANGEMENT_REFUSAL = "Home must keep at least one tile visible.";
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
const mullvadSetupRouteStorageKey = "osl-mullvad-setup-route-v1";
const onboardingResumeStorageKey = "osl-onboarding-resume-v1";
const identityDiscoveryChoiceStorageKey = "osl-identity-discovery-choice-v1";
const onboardingBranchStorageKey = "osl-onboarding-branch-v1";
const experimentalSendConsentStorageKey = "osl-experimental-send-consent-v1";
const rnWirePolicyStorageKey = "osl-rn-wire-policy-requested-v1";
// No cover-writing choice is active until the operator presses one of the two
// shared buttons. TASK 3520 wires the plain choice to the built-in wordbank.
let nativeDiscordCovertextEnabled = false;
// The verified pack is bundled and materialized during Rust startup. The
// command still re-checks readiness before it accepts the selection.
let nativeDiscordAiCovertextSelected = false;
const oslChatPreviewStorageKey = "osl-chat-previews-visible-v1";
const oslChatMutedStorageKey = "osl-chat-muted-people-v1";
const oslChatUnreadStorageKey = "osl-chat-unread-v1";
const oslChatNotificationStorageKey = "osl-chat-notifications-v1";
function storedIdentityDiscoveryChoice(): IdentityDiscoveryChoice | null {
  const stored = localStorage.getItem(identityDiscoveryChoiceStorageKey);
  return stored === "public-name" || stored === "private-link" ? stored : null;
}
let identityDiscoveryChoice: IdentityDiscoveryChoice | null = storedIdentityDiscoveryChoice();
let privateContactLink: HubPrivateContactLink | null = null;
let privateContactLinkBusy = false;
let privateContactLinkError = "";
let identityChoiceError = "";
type OslChatSecureStore = Pick<SecureLocalStore, "getItem" | "setItem">;
type BrowserImportStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;
type MullvadSetupRoute = "found-session" | "no-mullvad";
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
  email: "Email",
  signal: "Signal",
  whatsapp: "WhatsApp",
  x: "X",
  instagram: "Instagram",
  messenger: "Messenger",
};
const supportedNativeAppIds = new Set<NativeAppId>(["discord"]);
const importedFirefoxHomeAppIds = new Set<HomeAppId>([
  "messenger", "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
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
/**
 * D-190. The two things Continue is allowed to do on "Choose apps" are proceed,
 * or say why it cannot. These are the "why". They stay on the panel, unlike the
 * toast beside them, and they ship with the escape that clears them.
 */
const nativeCatalogFailedRefusal = "OSL could not check your Windows apps, so it cannot finish setting up the Windows apps you picked. Nothing has been changed.";
const nativeCatalogCheckingRefusal = "OSL is still checking your Windows apps.";

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
    && typeof (item as AppNotification).detail === "string" && (item as AppNotification).detail.length <= 1_000
    && ((item as AppNotification).id.startsWith("received-") || (item as AppNotification).detail === "New encrypted message")
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

// D-108. Everything above this line was written, unit-tested and never called:
// `configureOslChatSecureLocalStore` had no production caller, so
// `persistSensitiveOslChatJson` returned early on every write and the four
// `migrate*ToSecureLocalStore` functions were exported with none. This is the
// missing construction site.
//
// The migrations are guarded per key on the legacy plaintext actually being
// present, mirroring `encrypt_existing_state_files`'s `migrate skip <file>:
// not present` in crates/ipc/src/main_password.rs. Running one unconditionally
// would re-encrypt the parse of `null` — the defaults — over a good sealed
// value on the second launch and silently destroy it.
let oslChatSecureLocalStoreReady: Promise<void> | null = null;

async function migrateOslChatKeyOffPlaintext(
  key: string,
  migrate: () => Promise<unknown>,
): Promise<void> {
  if (localStorage.getItem(key) === null) {
    console.info(`[OSL][chat] migrate skip ${key}: not present`);
    return;
  }
  await migrate();
  console.info(`[OSL][chat] migrated ${key} to the secure local store; plaintext removed`);
}

export async function ensureOslChatSecureLocalStore(): Promise<void> {
  if (oslChatSecureStore) return;
  oslChatSecureLocalStoreReady ??= (async () => {
    const store = await createOslChatSecureLocalStore(localStorage);
    if (!store) return;
    configureOslChatSecureLocalStore(store);
    await migrateOslChatKeyOffPlaintext(
      oslChatPreviewStorageKey,
      () => migrateOslChatPreviewVisibilityToSecureLocalStore(store, localStorage),
    );
    await migrateOslChatKeyOffPlaintext(
      oslChatUnreadStorageKey,
      () => migrateOslChatUnreadToSecureLocalStore(store, localStorage),
    );
    await migrateOslChatKeyOffPlaintext(
      oslChatMutedStorageKey,
      () => migrateOslChatMutedPeopleToSecureLocalStore(store, localStorage),
    );
    await migrateOslChatKeyOffPlaintext(
      oslChatNotificationStorageKey,
      () => migrateOslChatNotificationsToSecureLocalStore(store, localStorage),
    );
  })().catch((error: unknown) => {
    console.info(`[OSL][chat] secure local store bootstrap failed: ${String(error)}`);
  });
  await oslChatSecureLocalStoreReady;
  // A main-password gate that is still locked at bootstrap has no key yet, and
  // that attempt must not be cached as the answer forever: `loadUiPreferences`
  // runs before the unlock screen. Clearing the memo lets the post-unlock call
  // in startReadyWorkspaceLoads() migrate the profile off plaintext.
  if (!oslChatSecureStore) oslChatSecureLocalStoreReady = null;
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

export function readRnWirePolicyRequested(storage: Pick<Storage, "getItem"> = localStorage): boolean {
  return storage.getItem(rnWirePolicyStorageKey) === "true";
}

export function rnWirePolicySettingsMarkup(state: RnWirePolicyState): string {
  const disabled = state.buildEnabled ? "" : "disabled";
  const summary = state.effectiveEnabled ? "On" : state.refusal === "build-disabled" ? "Unavailable in this build" : "Off";
  const toggle = onOffToggle("rn-wire-policy-toggle", state.effectiveEnabled, "Use next-generation protected messages").replace("<input ", `<input ${disabled ? "disabled " : ""}`);
  return `<details class="settings-disclosure" data-rn-wire-policy><summary><span><strong>Advanced message format</strong><small>${summary}</small></span></summary><label class="setting-line interactive"><span><strong>Use next-generation protected messages</strong><small>OSL keeps using the current message format unless this build and this setting both allow the newer one.</small></span><span class="rn-wire-policy-toggle${disabled ? " is-disabled" : ""}">${toggle}</span></label></details>`;
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

/**
 * T15-A8: the allow-list this used to inline did not contain `recovery`, so a
 * relaunch resumed at `pro` and the recovery step was skipped in silence —
 * with the phrases, which only ever lived in a module-local `let`, already
 * gone. The policy now lives in `onboarding-resume.ts` where it is tested, and
 * an unsaved recovery kit outranks every other pending step.
 */
function pendingOnboardingRoute(): OnboardingRoute | null {
  const resumed = recoveryKitUnsavedFlag.unsaved()
    ? "recovery"
    : identityDiscoveryChoice === null
      ? "identity-choice"
      : resumeOnboardingRoute(localStorage, onboardingResumeStorageKey);
  return resumed === null ? null : onboardingRouteForBuild(resumed);
}

/**
 * NEW-1: this used to hold the mirror back until the native write returned, so
 * a caller that routed on the same turn — which is exactly what Continue does —
 * read the pre-decision value and bounced the owner back to the gate. The
 * mirror now moves first and the resolved value still reports whether anything
 * durable was recorded, which is what the creation path warns about.
 */
async function persistRecoveryKitUnsaved(unsaved: boolean): Promise<boolean> {
  return recoveryKitUnsavedFlag.set(unsaved);
}

function persistCurrentOnboardingRoute(): void {
  if (onboardingRoute === "identity-choice"
    || onboardingRoute === "private-link"
    || onboardingRoute === "pro"
    || onboardingRoute === "privacy"
    || onboardingRoute === "defaults"
    || onboardingRoute === "tor"
    || onboardingRoute === "sending"
    || onboardingRoute === "cover"
    || onboardingRoute === "silent-visible"
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
  applyLookState(document.documentElement, lookState);
}

function saveAppearance(next: AppearancePreferences): void {
  appearancePreferences = next;
  render();
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
    loadHomeTilePreferences();
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
    const openingChoices = JSON.parse(localStorage.getItem(detectedAccountOpeningChoicesStorageKey) ?? "[]") as unknown;
    if (Array.isArray(openingChoices)) {
      detectedAccountOpeningChoices = new Map(openingChoices.filter((entry): entry is [string, "windowsApp" | "browser"] =>
        Array.isArray(entry) && entry.length === 2 && typeof entry[0] === "string"
        && (entry[1] === "windowsApp" || entry[1] === "browser")));
    }
    discoveryVisibility = readSavedDiscoveryVisibility(localStorage.getItem(DISCOVERY_VISIBILITY_STORAGE_KEY));
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
    detectedAccountOpeningChoices.clear();
    discoveryVisibility = defaultDiscoveryVisibilityState();
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
  mullvadSetupRoute = parseMullvadSetupRoute(localStorage.getItem(mullvadSetupRouteStorageKey));
  savedAccountsReady = false;
  notificationsEnabled = localStorage.getItem(notificationsStorageKey) === "true";
  notificationPreviewContent = localStorage.getItem(notificationPreviewStorageKey) === "true";
  notificationScopeSuggestions = localStorage.getItem(notificationScopeStorageKey) !== "false";
  notificationChatActivity = localStorage.getItem(notificationChatStorageKey) !== "false";
  notificationSecurityActivity = localStorage.getItem(notificationSecurityStorageKey) !== "false";
  protectionPreset = loadProtectionPreset();
  oslMailNotifications = localStorage.getItem(oslMailNotificationsStorageKey) !== "false";
  rnWirePolicyRequested = readRnWirePolicyRequested();
  await ensureOslChatSecureLocalStore();
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
      receipt.observationCount > 0
      && receipt.snapshotDeleted);
  if (!preferredBrowserId || !completedBrowserImportIds.has(preferredBrowserId)) {
    preferredBrowserId = hydration.imports[0]?.browserId ?? null;
  }
}

function saveHomeTilePreferences(): void {
  const { orderKey, hiddenKey } = homeTilePreferenceStorageKeys();
  localStorage.setItem(orderKey, JSON.stringify(homeTileOrder));
  localStorage.setItem(hiddenKey, JSON.stringify([...hiddenHomeTiles]));
}

function homeTilePreferenceStorageKeys(owner = core.readiness.activeOslUserId): { orderKey: string; hiddenKey: string } {
  if (!owner) return { orderKey: homeTileOrderStorageKey, hiddenKey: hiddenHomeTilesStorageKey };
  const suffix = `:${encodeURIComponent(owner)}`;
  return { orderKey: `${homeTileOrderStorageKey}${suffix}`, hiddenKey: `${hiddenHomeTilesStorageKey}${suffix}` };
}

function loadHomeTilePreferences(): void {
  const owner = core.readiness.activeOslUserId;
  const { orderKey, hiddenKey } = homeTilePreferenceStorageKeys(owner);
  const tileOrder = JSON.parse(localStorage.getItem(orderKey) ?? "[]") as unknown;
  const hiddenTiles = JSON.parse(localStorage.getItem(hiddenKey) ?? "[]") as unknown;
  homeTileOrder = Array.isArray(tileOrder) ? tileOrder.filter((id): id is string => typeof id === "string").slice(0, 32) : [];
  hiddenHomeTiles = new Set(Array.isArray(hiddenTiles) ? hiddenTiles.filter((id): id is string => typeof id === "string").slice(0, 32) : []);
  homeTilePreferenceOwner = owner;
}

function syncHomeTilePreferencesForActiveProfile(): void {
  if (homeTilePreferenceOwner === core.readiness.activeOslUserId) return;
  try {
    loadHomeTilePreferences();
  } catch {
    homeTileOrder = [];
    hiddenHomeTiles.clear();
    homeTilePreferenceOwner = core.readiness.activeOslUserId;
  }
}

function compactFriendId(value: string): string {
  const normalized = value.replace(/[^A-Za-z0-9]/g, "").toUpperCase();
  if (!normalized) return "Unavailable";
  if (normalized.length <= 16) return normalized.match(/.{1,4}/g)?.join(" ") ?? normalized;
  return `${normalized.slice(0, 8).match(/.{1,4}/g)?.join(" ")} … ${normalized.slice(-4)}`;
}

function commitRender(): void {
  try {
    pruneExpiredOslChatLocalCopies();
    refreshActiveBrowserAccountsReady();
    if (route === "onboarding") renderOnboarding();
    else renderWorkspace();
    if (!fixedNoRecoverySecretFixture) bindDesktopTitlebar();
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

/**
 * The crash view. `data-osl-render-recovery="true"` is load-bearing: the
 * capture harness once photographed this screen and its blank-detection and
 * colour-conformance checks both PASSED it, because the crash view uses the
 * correct background and button colours. No healthy screen renders this
 * attribute, so tooling that reads the DOM can hard-fail a crashed view
 * instead of grading its screenshot as a rendered screen.
 */
function renderRecoveryMarkup(): string {
  return `<main class="ui-recovery" data-osl-render-recovery="true" role="alert" aria-labelledby="ui-recovery-title"><img src="${oslLogoUrl}" alt=""/><h1 id="ui-recovery-title">OSL paused this view</h1><p>No error details were displayed or sent.</p><button class="button primary" id="ui-recovery-reload">Reload interface</button></main>`;
}

function showRenderRecovery(): void {
  renderScheduler.cancel();
  lastOnboardingMarkup = null;
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  root.innerHTML = renderRecoveryMarkup();
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

function containBackgroundFailure(detail?: string): void {
  if (!root.querySelector(".app-frame")) {
    showRenderRecovery();
    return;
  }
  showToast(detail
    ? `That action failed. Nothing changed. (${detail})`
    : "That action failed. Nothing changed.");
}

// A rejection from invoking a command the Rust side does not define is the one
// runtime signal that a frontend client is calling into nothing. It was
// suppressed for the life of the project by a preventDefault() on
// unhandledrejection, which is why four such commands shipped unnoticed. Naming
// the command in the toast is deliberate: a generic "that action failed" is
// indistinguishable from a network blip, so it teaches the user -- and anyone
// reading a bug report -- nothing about which action is missing.
function describeRejection(reason: unknown): string | undefined {
  const text = typeof reason === "string"
    ? reason
    : reason instanceof Error
      ? reason.message
      : typeof reason === "object" && reason !== null && "message" in reason
        ? String((reason as { message: unknown }).message)
        : undefined;
  if (!text) return undefined;
  const missing =
    /(?:command|Command)\s+([a-z0-9_]+)\s+not\s+found/.exec(text) ??
    /unknown\s+command:?\s+([a-z0-9_]+)/i.exec(text);
  if (missing) return `unknown command: ${missing[1]}`;
  return text.length > 80 ? `${text.slice(0, 77)}...` : text;
}

const unhandledRejectionEventType = "unhandledrejection";

function handleUnhandledRejection(event: PromiseRejectionEvent): void {
  console.error("Unhandled background rejection", event.reason);
  containBackgroundFailure(describeRejection(event.reason));
}

function desktopTitlebar(): string {
  const controlsBlocked = activeNativeHostId || activeDefaultBrowserCompanion;
  const nativeControlsBlocked = controlsBlocked ? ' disabled aria-describedby="desktop-controls-unavailable"' : "";
  const unavailableTooltip = controlsBlocked ? `<span class="in-dom-tooltip" id="desktop-controls-unavailable" role="tooltip">Unavailable while a companion window is open</span>` : "";
  return `<header class="desktop-titlebar"><div class="desktop-drag-region" data-tauri-drag-region aria-hidden="true"></div>${fleetIndicatorMarkup()}<div class="window-controls in-dom-tooltip-anchor"><button id="window-minimize" aria-label="Minimize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-close" class="window-close" aria-label="Close"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>${unavailableTooltip}</div></header>`;
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
  const controlsBlocked = activeNativeHostId || activeDefaultBrowserCompanion;
  const nativeControlsBlocked = controlsBlocked ? ' disabled aria-describedby="desktop-controls-unavailable"' : "";
  const unavailableTooltip = controlsBlocked ? `<span class="in-dom-tooltip" id="desktop-controls-unavailable" role="tooltip">Unavailable while a companion window is open</span>` : "";
  return `${fleetIndicatorMarkup()}<div class="window-controls in-dom-tooltip-anchor"><button id="window-minimize" aria-label="Minimize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8.5h10"/></svg></button><button id="window-maximize" aria-label="Maximize"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9"/></svg></button><button id="window-close" class="window-close" aria-label="Close"${nativeControlsBlocked}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8"/></svg></button>${unavailableTooltip}</div>`;
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
  nativeActionBusy = true;
  render();
  try {
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
      return;
    }
    nativeActionBusy = false;
    await openNativeHostedApp(app, service, staleAppId);
  } finally {
    if (nativeActionBusy) {
      nativeActionBusy = false;
      render();
    }
  }
}

function onboardingShellMarkup(setupNavigation = ""): string {
  return `<div class="app-frame with-titlebar">${desktopTitlebar()}<div class="onboarding-shell"><main class="onboarding-panel onboarding-${onboardingRoute}">${onboardingContent()}${setupNavigation}</main></div>${scrubReviewDialogMarkup()}</div>`;
}

function setupOnboardingNavigationMarkup(): string {
  return `<div class="setup-footer onboarding-actions onboarding-nav"><button class="button ghost onboarding-back" id="onboarding-back" type="button">Back</button></div>`;
}

function isSetupOnboardingRoute(candidate: OnboardingRoute): boolean {
  return ["pro", "forward-secrecy", "privacy", "defaults", "tor", "sending", "cover", "passwords", "burnpass", "browser", "detected", "install", "apps", "mullvad"].includes(candidate);
}

/**
 * Back used to render as a bare link pinned to the viewport's bottom-left
 * corner while the step's own Continue sat centred in the middle of the panel;
 * at 1440x900 they were ~680px apart and Back read as a stray link rather than
 * as navigation. It now renders as a real action row inside the panel and is
 * folded into the step's own action row where the step has one, so the two
 * controls are always adjacent with Continue as the primary.
 *
 * The move is structural, not stylistic: the shipped CSP is `style-src 'self'`
 * (apps/osl-hub/tauri.conf.json), which drops inline style attributes, so every
 * rule involved lives in styles.css.
 */
function dockOnboardingBackControl(): void {
  const nav = root.querySelector<HTMLElement>(".onboarding-panel > .onboarding-nav");
  const back = nav?.querySelector<HTMLButtonElement>("#onboarding-back");
  if (!nav || !back) return;
  const primaryRow = [...root.querySelectorAll<HTMLElement>(".onboarding-panel .setup-footer.onboarding-actions")]
    .find((row) => row !== nav);
  // Steps with no action row of their own (Pro code, the password-role forms)
  // keep the nav row itself, which is already a footer in the same position.
  if (!primaryRow) return;
  // A step that navigates its own internal sequence renders its own Back and
  // owns that direction entirely (the quick tour walks five sub-steps before
  // it leaves the route). Docking the global Back beside it shipped two
  // identically labelled "Back" buttons side by side on all five tour steps.
  if (primaryRow.querySelector(".onboarding-step-back")) {
    nav.remove();
    return;
  }
  primaryRow.prepend(back);
  nav.remove();
}

function onboardingSetupNavigationMarkup(): string {
  return ["recovery-check", "identity-choice", "private-link", "pro", "forward-secrecy", "privacy", "defaults", "tor", "sending", "cover", "silent-visible", "visibility", "passwords", "burnpass", "browser", "detected", "install", "apps", "mullvad"].includes(onboardingRoute)
    ? `<div class="setup-footer onboarding-actions onboarding-nav"><button class="button ghost onboarding-back" id="onboarding-back" type="button">Back</button></div>`
    : "";
}

function renderOnboarding(): void {
  onboardingRoute = onboardingRouteForBuild(onboardingRoute);
  persistCurrentOnboardingRoute();
  const setupNavigation = onboardingSetupNavigationMarkup();
  const markup = onboardingShellMarkup(setupNavigation);
  lastWorkspaceMarkup = null;
  lastWorkspaceViewKey = "";
  const active = document.activeElement;
  // Consumed by this pass whatever it decides, so one forced paint can never
  // leak into the next background refresh and start clobbering live typing.
  const forced = forceOnboardingPaint;
  forceOnboardingPaint = false;
  const decision = onboardingPaintDecision({
    markupUnchanged: lastOnboardingMarkup === markup,
    shellMounted: root.querySelector(".onboarding-shell") !== null,
    sameRouteAsRendered: renderedOnboardingRoute === onboardingRoute,
    passwordEditInProgress: [...root.querySelectorAll<HTMLInputElement>('input[type="password"]')]
      .some((input) => input === active || input.value.length > 0),
    forced,
  });
  if (decision === "skip-unchanged") {
    openScrubReviewDialogAfterRender();
    return;
  }
  if (decision === "defer-sensitive-edit") return;
  lastOnboardingMarkup = markup;
  renderedOnboardingRoute = onboardingRoute;
  root.innerHTML = markup;
  dockOnboardingBackControl();
  bindOnboarding();
  openScrubReviewDialogAfterRender();
}

function onboardingContent(): string {
  if (onboardingRoute === "pro") return proSetupContent();
  if (onboardingRoute === "welcome") return welcomeOnboardingContent();

  if (onboardingRoute === "create") return identityPasswordForm("Create a password", "Create account", "setup");
  if (onboardingRoute === "unlock") return identityPasswordForm("Unlock OSL", "Unlock", "unlock");
  if (onboardingRoute === "keylost") return identityKeyLostContent();
  if (onboardingRoute === "account-recovery") {
    // A legacy marker refusal is not a generic reset failure: it keeps its own
    // migration screen, which is the only place the two repair paths exist.
    return legacyRecoveryMigration
      ? legacyRecoveryMigrationMarkup(legacyRecoveryMigration)
      : recoveryScreenMarkup(accountRecoveryFlow);
  }
  if (onboardingRoute === "import") return importIdentityForm();
  if (onboardingRoute === "recovery") {
    return cleanDeviceRestoreState.phase === "account-ready"
      ? restoredAccountReadyContent()
      : recoveryContent();
  }
  if (onboardingRoute === "recovery-check") return recoveryWordCheckMarkup(recoveryWordCheckState, escapeHtml);
  if (onboardingRoute === "identity-choice") return identityChoiceMarkup(identityChoiceError);
  if (onboardingRoute === "private-link") return privateContactLinkMarkup({
    busy: privateContactLinkBusy,
    linkValue: privateContactLink?.linkValue ?? null,
    error: privateContactLinkError,
  });
  if (onboardingRoute === "tutorial") return tutorialContent();
  if (onboardingRoute === "detected") return detectedAppsContent();
  if (onboardingRoute === "install") return installMissingAppsContent();
  if (onboardingRoute === "apps") return onboardingAppsContent();
  if (onboardingRoute === "browser") return browserImportContent();
  if (onboardingRoute === "mullvad") return mullvadSetupContent();
  if (onboardingRoute === "defaults") return reviewDefaultsOnboardingContent();
  if (onboardingRoute === "tor") return onboardingTorMarkup(torOnboarding);
  if (onboardingRoute === "forward-secrecy") return onboardingForwardSecrecyMarkup(forwardSecrecyOnboarding);
  if (onboardingRoute === "cover") return coverDraftSetupContent();
  if (onboardingRoute === "silent-visible") return silentVisibleSetupContent();
  if (onboardingRoute === "visibility") return onboardingCaptureVisibilityMarkup();
  if (onboardingRoute === "passwords") return onboardingPasswordRoleContent("stealth");
  if (onboardingRoute === "burnpass") return onboardingPasswordRoleContent("burn");
  if (onboardingRoute === "privacy") return onboardingPrivacyContent();
  if (onboardingRoute === "decoy") return `<section class="decoy-workspace" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Workspace</h1><p>No recent items.</p><button class="button ghost" id="close-decoy" type="button">Close</button></section>`;

  return sendingSetupContent();
}

/**
 * D-207 / D-150 — the device key that opens this account is gone.
 *
 * `identity.json` is on disk and will not unseal, so nothing the user can type
 * will open it. This screen exists so that failure stops looking like the two
 * it is routinely mistaken for: a fresh install (which would offer "Create
 * account" over an account that still exists) and a locked session (which
 * would offer a password box that cannot work). Both of those are how an
 * unrecoverable state came to present as a healthy one.
 *
 * The only real way forward is the recovery phrase, so that is the primary
 * action and it is the only one offered.
 */
function identityKeyLostContent(): string {
  // 2026-08-06 restyle. Two paragraphs became one. The first used to spend a
  // sentence on what OSL will not pretend, which is a promise about OSL rather
  // than an answer to the question the person is actually asking, which is
  // "have I lost everything".
  //
  // The footnote answers that, so it stays: nothing has been deleted.
  return `<section class="keylost-screen" aria-labelledby="route-heading">
    <img class="keylost-logo" src="${oslGhostMarkUrl}" alt="" width="104" height="104"/>
    <h1 id="route-heading" tabindex="-1" class="keylost-title">Device key lost</h1>
    <p class="keylost-copy">The key that unlocks it is gone from this device. Your 12-word recovery phrase restores the same account and contacts, here or on any other device.</p>
    <button class="signin-unlock keylost-action" data-onboarding="import" type="button"><span class="signin-unlock-label">Restore with recovery phrase</span>${signinArrowIcon()}</button>
    <p class="keylost-quiet">Nothing has been deleted. Until you restore, everything stays encrypted and unreadable</p>
  </section>`;
}

/**
 * The lock on the sign-in button. The shackle is its own element because the
 * hover springs it open independently of the body, around an origin at the
 * hinge (16px, 11px) rather than the icon's centre.
 */
function signinLockIcon(): string {
  // viewBox is 24, not 20. The body runs to y=20 and its 2px stroke reaches y=21,
  // so a 20-tall box clipped the bottom edge clean off. 24 also centres the
  // drawing: content spans y=4..20, centre 12, which is the box centre.
  // The body sits at x=5 -- the shackle spans x=8..16 with centre 12, so a
  // 14-wide body has to start at 5 to share it.
  return `<svg class="signin-icon signin-lock" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path class="signin-lock-shackle" d="M8 11 V8 a4 4 0 0 1 8 0 v3"/><rect x="5" y="11" width="14" height="9" rx="2"/></svg>`;
}

/** The arrow on the create-password submit. Slides right on hover. */
function signinArrowIcon(): string {
  return `<svg class="signin-icon signin-arrow" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12 h14"/><path d="M13 6 l6 6 -6 6"/></svg>`;
}

/** The plus on the create-account button. Rotates a quarter turn on hover. */
function signinPlusIcon(): string {
  return `<svg class="signin-icon signin-plus" viewBox="0 0 24 24" width="17" height="17" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5 v14"/><path d="M5 12 h14"/></svg>`;
}

/**
 * The bare entry screen: mark, one action, one way out. Sign in and Create
 * account are the SAME screen -- Liam's ruling, 2026-08-06, which overrides the
 * handoff's suggestion that the two keep different corner radii. Only the
 * label, the icon and where the button goes differ, so they are arguments
 * rather than two copies that drift apart.
 */
function entryScreenContent(label: string, icon: string, route: OnboardingRoute): string {
  return `<section class="signin-card signin-lock-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" class="sr-only" tabindex="-1">${label}</h1>
    <div class="signin-lock-column">
      <img class="signin-ghost-mark" src="${oslGhostMarkUrl}" alt="" width="148" height="148"/>
      <button class="signin-unlock" data-onboarding="${route}" type="button"><span class="signin-unlock-label">${label}</span>${icon}</button>
      <button class="signin-recovery" data-onboarding="import" type="button">Use recovery phrase</button>
    </div>
  </section>`;
}

function welcomeOnboardingContent(): string {
  const returning = core.readiness.bootstrapStatus === "passwordRequired" || core.readiness.passwordGateRequired;

  if (returning) return entryScreenContent("Sign in", signinLockIcon(), "unlock");

  // 2026-08-08: the real design export arrived. First run is Create Account.dc.html
  // -- the SAME bare entry skeleton as sign-in: mark, ONE button, one quiet
  // "Use recovery phrase" way out. The three-button "Welcome" chooser this
  // branch used to render came from the invented interim spec and is gone;
  // Restore lives behind the recovery link, and Unlock is the returning branch.
  return entryScreenContent("Create account", signinPlusIcon(), "create");
}

function proSetupContent(): string {
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  if (pro && !proOnboardingCodeEntryRequested) return `<section class="pro-setup onboarding-centered-step" aria-labelledby="route-heading">${statusTag("Pro active", "active")}<h1 id="route-heading" tabindex="-1">OSL Pro is ready</h1><p class="compact-lead onboarding-centered-copy">Pro features are available on this device. Making view-once items needs Pro; opening one is free.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-pro-ready" type="button">Continue</button></div></section>`;
  // No third variant. Requesting code entry (Back on the ready screen) shows
  // the actual code form below even while a licence is active -- that request
  // is the only reason `proOnboardingCodeEntryRequested` exists. A second
  // "Pro is ready" screen used to sit here and swallowed the request.
  // The submit and the Skip escape hatch sit in the step's own action row, so
  // the docking pass folds Back in beside them instead of leaving a third,
  // separate footer below a loose text link.
  // The eyebrow, the divider and the solid cyan button are gone by the 2026-08-06
  // redesign. Same button and same tokens as the other entry screens -- one
  // component, so a change to it lands everywhere at once.
  return `<section class="pro-setup pro-code-screen" aria-labelledby="route-heading"><h1 id="route-heading" class="pro-code-title" tabindex="-1">Enter Pro code</h1><form id="activation-form" class="pro-setup-form pro-code-form" novalidate><label class="sr-only" for="activation-code">Pro activation code</label><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="signin-unlock pro-code-continue" type="submit"><span class="signin-unlock-label">Continue</span>${signinArrowIcon()}</button><button class="signin-recovery" id="skip-pro-setup" type="button">Skip</button></form></section>`;
}

function tutorialContent(): string {
  const current = quickTourCards[onboardingTourStep];
  if (!current) return replayingOnboardingTour
    ? `<h1 id="route-heading" tabindex="-1">Tour complete</h1><p class="compact-lead onboarding-centered-copy">You can replay this tour any time from Settings → About.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="finish-onboarding-tour" type="button">Return to Home</button></div>`
    : chooseAppsOnboardingContent();
  const [title, detail] = current;
  return `<section class="onboarding-tour" aria-labelledby="route-heading" data-onboarding-tour-step="${onboardingTourStep + 1}"><p class="eyebrow">Quick tour · ${onboardingTourStep + 1} of ${quickTourCards.length}</p><h1 id="route-heading" tabindex="-1">${title}</h1><p class="compact-lead onboarding-centered-copy">${detail}</p><p class="send-mode-truth">You can return to this tour later from Settings → About.</p><div class="setup-footer onboarding-actions"><button class="button ghost onboarding-back onboarding-step-back" id="onboarding-tour-back" type="button">Back</button><button class="button primary" id="onboarding-tour-next" type="button">${onboardingTourStep + 1 === quickTourCards.length ? "Choose apps" : "Next"}</button></div></section>`;
}

function chooseAppsOnboardingContent(): string {
  const apps = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch");
  const { connected, browserHistory, other } = groupOnboardingApps({
    apps,
    nativeApps,
    savedAccountsReady,
    importedBrowserAppIds: importedFirefoxHomeAppIds,
  });
  const choices = (items: HomeAppCatalogEntry[], label: string) => items.length
    ? `<div class="onboarding-app-grid onboarding-app-choices" role="group" aria-label="${label}">${items.map((app) => {
      const available = app.launchState === "available";
      const selected = available && selectedOnboardingApps.has(app.id);
      const action = available ? `data-onboarding-app-choice="${app.id}" aria-pressed="${selected}"` : `data-onboarding-app-not-built="${app.id}" aria-disabled="false"`;
      return `<button type="button" class="onboarding-app ${selected ? "selected" : ""} ${available ? "" : "unavailable"}" ${action}><span class="app-logo-plate">${homeAppLogo(app)}</span><span><strong>${escapeHtml(app.displayName)}</strong>${available ? "" : "<small>Coming soon</small>"}</span></button>`;
    }).join("")}</div>`
    : `<p class="saved-account-truth">None</p>`;
  const defaultContinueLabel = nativeCatalogBusy ? "Checking Windows…" : "Continue";
  const continueLabel = nativeCatalogBusy
    ? defaultContinueLabel
    : selectedOnboardingApps.size > 0 ? defaultContinueLabel : "Skip apps";
  // D-190: the last step of first-run setup is not allowed to have a live
  // Continue that does nothing. If the catalog probe refused, the reason stays
  // here, and the way past it is a labelled button rather than the undiscoverable
  // trick of de-selecting a tile the user did not select.
  const refusal = nativeCatalogRefusal
    ? `<p class="form-status" id="app-choice-refusal" role="alert">${escapeHtml(nativeCatalogRefusal)}</p><button class="browser-import-skip" id="continue-without-apps" type="button">Continue without Windows apps</button>`
    : "";
  return `<h1 id="route-heading" tabindex="-1">Choose apps</h1><p class="compact-lead onboarding-centered-copy">Pick available apps for Home, or skip this for now. Nothing opens during setup.</p><section class="onboarding-app-section"><h2>Connected</h2>${choices(connected, "Connected apps")}</section><section class="onboarding-app-section"><h2>Seen in your browser history</h2>${choices(browserHistory, "Apps seen in your browser history")}</section><section class="onboarding-app-section"><h2>Other apps</h2>${choices(other, "Other apps")}</section><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-app-choice" type="button" ${nativeCatalogBusy ? "disabled" : ""}>${continueLabel}</button>${refusal}</div>`;
}

/**
 * D-190. Leave "Choose apps" without the Windows apps OSL could not verify.
 *
 * Drops only the native selections -- the ones the failed probe makes
 * unresolvable -- keeps every other pick, and says what it dropped. Onboarding
 * always ends; a step that cannot be finished is not a step.
 */
async function continueWithoutNativeApps(): Promise<void> {
  const dropped = [...selectedOnboardingApps].filter((appId) => supportedNativeAppIds.has(appId as NativeAppId));
  for (const appId of dropped) selectedOnboardingApps.delete(appId);
  hasExplicitOnboardingAppSelection = true;
  nativeCatalogRefusal = null;
  persistCombinedHomeChoices();
  if (dropped.length) showToast("Continued without the Windows apps OSL could not check. Add them later in Settings → Apps.");
  await completeOnboarding();
}

async function enterCombinedAppChoice(): Promise<void> {
  const catalog = await withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs).catch(() => null);
  if (catalog && isCompleteNativeCatalog(catalog)) nativeApps = catalog;
  // Was "tutorial". The tour is no longer part of first run, so this goes
  // straight to the step that used to follow it.
  onboardingRoute = "detected";
  render();
}

function persistCombinedHomeChoices(): void {
  hasExplicitOnboardingAppSelection = true;
  const available = new Set(homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available")
    .map((app) => app.id));
  for (const appId of [...selectedOnboardingApps]) {
    if (!available.has(appId)) selectedOnboardingApps.delete(appId);
  }
  localStorage.setItem(selectedOnboardingAppsStorageKey, JSON.stringify([...selectedOnboardingApps]));
  // Setup ends on this path. Keep the app route, detected accounts, and their
  // opening choices in the same durable checkpoint rather than depending on
  // an earlier control click having happened to write each one.
  persistSavedAccountPreferences();
}

function selectedNativeApps(): NativeApp[] {
  return nativeApps.filter((app) => supportedNativeAppIds.has(app.id) && selectedOnboardingApps.has(app.id));
}

function hasSelectedNativeAppChoice(): boolean {
  return [...selectedOnboardingApps].some((appId) => supportedNativeAppIds.has(appId as NativeAppId));
}

/**
 * Did the Windows catalog probe answer for every app this build can act on?
 *
 * D-190. This asks about COVERAGE, not equality. The earlier form also required
 * `catalog.length === supportedNativeAppIds.size`, which was true only while the
 * two sets happened to be the same size. `f02104ac0` narrowed
 * `supportedNativeAppIds` to `{discord}` -- correctly, Discord is the only carrier
 * this build enables -- while `list_native_apps` kept returning all five rows of
 * `NATIVE_APPS` (`apps/osl-hub/src/native_apps.rs:434-547`). From that commit on
 * the predicate was false for EVERY real catalog, so `ensureNativeCatalogForAppChoice`
 * refused forever and Continue became a no-op whenever a native app was selected.
 *
 * The fail-closed intent is preserved: a truncated catalog that is missing Discord
 * is still rejected. Only "and nothing else" is dropped, because the backend
 * legitimately reports apps the frontend does not act on.
 */
function isCompleteNativeCatalog(catalog: NativeApp[]): boolean {
  const ids = new Set(catalog.map((app) => app.id));
  return [...supportedNativeAppIds].every((appId) => ids.has(appId));
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
  if (!hasSelectedNativeAppChoice()) {
    nativeCatalogRefusal = null;
    return true;
  }
  // D-190: a second click while the first probe is still running is the one path
  // out of here that says nothing at all, so it says something now.
  if (nativeCatalogBusy) {
    nativeCatalogRefusal = nativeCatalogCheckingRefusal;
    return false;
  }
  nativeCatalogBusy = true;
  nativeCatalogRefusal = null;
  renderNow();
  try {
    const catalog = await withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs);
    if (!isCompleteNativeCatalog(catalog)) {
      nativeCatalogRefusal = nativeCatalogFailedRefusal;
      showToast("Couldn’t check Windows apps. Try again.");
      return false;
    }
    nativeApps = catalog;
    nativeCatalogRefusal = null;
    return true;
  } catch {
    nativeCatalogRefusal = nativeCatalogFailedRefusal;
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

function defaultBrowserCompanionEligible(appId: HomeAppId | null): appId is HomeAppId {
  return appId !== null && ["messenger", "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud"].includes(appId);
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

function persistDetectedAccountOpeningChoices(): void {
  const validKeys = new Set(detectedAccounts.map(detectedOpeningChoiceKey));
  for (const key of detectedAccountOpeningChoices.keys()) {
    if (!validKeys.has(key)) detectedAccountOpeningChoices.delete(key);
  }
  localStorage.setItem(detectedAccountOpeningChoicesStorageKey, JSON.stringify([...detectedAccountOpeningChoices]));
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
  const accountChoices = detectedAccounts.map((account) => {
    const key = detectedOpeningChoiceKey(account);
    const selected = detectedAccountOpeningChoices.get(key);
    const choices = account.openChoices.map((choice) => {
      const isSelected = selected === choice.kind;
      const label = choice.kind === "windowsApp" ? `Windows app · ${choice.label}` : `Browser · ${choice.label}`;
      return `<button type="button" role="radio" aria-checked="${isSelected}" class="native-mode-option ${isSelected ? "selected" : ""}" data-detected-account-opening="${escapeHtml(key)}" data-detected-account-opening-choice="${choice.kind}">${escapeHtml(label)}</button>`;
    }).join("");
    return `<div class="native-mode-setting account-opening-choice" role="radiogroup" aria-label="${escapeHtml(account.accountLabel)} opening"><strong>${escapeHtml(account.accountLabel)}</strong><div>${choices}</div></div>`;
  }).join("");
  const rows = installed.length
    ? installed.map((app) => `<label class="saved-account-app"><span>${nativeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>Installed on this PC</small></span></span><input type="checkbox" data-saved-native="${app.id}" ${app.id === "discord" || savedNativeApps.has(app.id) ? "checked" : ""} ${app.id === "discord" ? "disabled" : ""}/></label>`).join("")
    : `<div class="empty-state"><strong>No selected desktop apps were detected</strong><p>OSL can still use isolated web profiles.</p></div>`;
  const discordChoices = installed.some((app) => app.id === "discord")
    ? nativeSessionModeSettingChoices("discord", "Discord")
    : "";
  const detectedLead = detectedAccounts.length ? "Choose how each detected account opens." : "Choose detected desktop apps.";
  return `<h1 id="route-heading" tabindex="-1">Use installed apps</h1><p class="compact-lead onboarding-centered-copy">${detectedLead}</p>${discordChoices}${accountChoices}<div class="setup-list">${rows}</div><div class="setup-footer onboarding-actions"><button class="button primary" id="continue-detected-apps" type="button">Continue</button></div>`;
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
    ? `<div class="saved-account-browser-note"><strong>Saved browser account hints protected</strong><small>Encrypted locally. Login stores and passwords were not read.</small>${browserFootprintImports.map((receipt) => `<button class="button compact" type="button" data-revoke-browser-footprint="${escapeHtml(browserProfileKey({ browserId: receipt.browserId, profile: receipt.profile, displayName: receipt.profile }))}" aria-label="Delete ${escapeHtml(receipt.browserId)} · ${escapeHtml(receipt.profile)} browser area">Delete area</button>`).join("")}</div>`
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
    : selectionReady ? "Check selected" : "Choose areas";
  const secondaryLabel = browserImportBusy ? "Wait for scan..." : "Not now";
  return `<h1 id="route-heading" tabindex="-1">Find saved browser accounts</h1><p class="compact-lead onboarding-centered-copy">Optional. Consent separately to each browser area OSL may inspect.</p>${detectedBrowsers}${progress}${ready}${failure}<p class="saved-account-truth">OSL never reads browser databases before consent. After consent it copies one bounded history snapshot, reads that copy, deletes it, and never opens passwords or login stores.</p><div class="setup-footer onboarding-actions"><button class="button primary" id="import-saved-accounts" type="button" ${importEnabled ? "" : "disabled"}>${importLabel}</button><button class="browser-import-skip" id="continue-browser-import" type="button" ${browserImportBusy || browserImportCancelling ? "disabled" : ""}>${secondaryLabel}</button></div>`;
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
  document.querySelectorAll<HTMLButtonElement>("[data-detected-account-opening]").forEach((button) => button.addEventListener("click", () => {
    const key = button.dataset.detectedAccountOpening ?? "";
    const choice = button.dataset.detectedAccountOpeningChoice;
    if (!key || (choice !== "windowsApp" && choice !== "browser")) return;
    const account = detectedAccounts.find((candidate) => detectedOpeningChoiceKey(candidate) === key);
    if (!account) return;
    detectedAccountOpeningChoices = chooseDetectedAccountOpening(detectedAccountOpeningChoices, account, choice);
    persistDetectedAccountOpeningChoices();
    render();
  }));
  // A `[data-saved-account-mode]` click binding used to sit here. No markup in
  // this build -- or anywhere else in the repo -- writes that attribute, so the
  // listener could never run; `savedAccountMode` is now driven entirely by the
  // per-app `[data-saved-native]` choices below and by the launch paths. Kept as
  // a note rather than a binding, because a listener with no control is not a
  // feature, and ledger 1 reported it as a live selector waiting on dead markup.
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
    void deleteBrowserAccountFinderArea(key);
  }));
  document.querySelectorAll<HTMLInputElement>("[data-browser-profile]").forEach((input) => input.addEventListener("change", () => {
    const key = input.dataset.browserProfile ?? "";
    tickBrowserAccountFinderArea(key, input.checked);
  }));
  document.querySelector<HTMLButtonElement>("#import-saved-accounts")?.addEventListener("click", () => {
    void checkSelectedBrowserAccountFinderAreas();
  });
  document.querySelector<HTMLButtonElement>("#continue-browser-import")?.addEventListener("click", async () => {
    await leaveBrowserAccountFinderForApps();
  });
}

function tickBrowserAccountFinderArea(key: string, checked: boolean): boolean {
  if (!browserProfiles.some((profile) => browserProfileKey(profile) === key)) return false;
  browserImportFailureNotice = "";
  if (checked) selectedBrowserProfileKeys.add(key);
  else selectedBrowserProfileKeys.delete(key);
  render();
  return true;
}

async function deleteBrowserAccountFinderArea(key: string): Promise<boolean> {
  const receipt = browserFootprintImports.find((candidate) =>
    browserProfileKey({ browserId: candidate.browserId, profile: candidate.profile, displayName: candidate.profile }) === key);
  if (!receipt || browserImportBusy) return false;
  browserImportBusy = true;
  browserImportFailureNotice = "";
  render();
  try {
    await revokeDetectedBrowserFootprint(receipt.browserId, receipt.profile, receipt.account, receipt.runId);
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
    return true;
  } catch (failure) {
    browserImportFailureNotice = localActionError(failure, "Saved browser account hints could not be deleted");
    showToast(browserImportFailureNotice);
    return false;
  } finally {
    browserImportBusy = false;
    render();
  }
}

async function checkSelectedBrowserAccountFinderAreas(): Promise<boolean> {
  if (selectedBrowserProfileKeys.size === 0 || browserImportBusy) return false;
  const selectedProfiles = browserProfiles.filter((profile) =>
    selectedBrowserProfileKeys.has(browserProfileKey(profile)));
  if (selectedProfiles.length !== selectedBrowserProfileKeys.size) {
    browserImportFailureNotice = "The selected browser areas changed. Review them again.";
    selectedBrowserProfileKeys.clear();
    render();
    return false;
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
      if (runEpoch !== browserImportRunEpoch) return false;
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
      if (runEpoch !== browserImportRunEpoch) return false;
      if (receipt.observationCount < 1) {
        throw new Error("Nothing was imported from it");
      }
      if (!receipt.snapshotDeleted) {
        throw new Error("The temporary browser snapshot was not deleted.");
      }
      scanReceipts.push(receipt);
      browserImportSourceSelected = true;
      browserImportFailureNotice = "";
      persistBrowserImportQueue();
    }
    if (runEpoch !== browserImportRunEpoch) return false;
    const ownerBeforeHydration = core.readiness.activeOslUserId;
    const hydration = await loadDetectedBrowserFootprint(scanReceipts);
    if (ownerBeforeHydration === null || core.readiness.activeOslUserId !== ownerBeforeHydration) return false;
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
    return true;
  } catch (failure) {
    if (runEpoch !== browserImportRunEpoch) return false;
    browserImportQueue = [];
    browserImportQueueIndex = 0;
    browserImportSourceSelected = false;
    persistBrowserImportQueue();
    selectedBrowserProfileKeys.clear();
    browserImportFailureNotice = localActionError(failure, "Saved browser account check did not finish");
    showToast(scanReceipts.length === 0 ? emptyImportNotice : browserImportFailureNotice);
    return false;
  } finally {
    if (runEpoch === browserImportRunEpoch) {
      browserImportBusy = false;
      render();
    }
  }
}

async function leaveBrowserAccountFinderForApps(): Promise<boolean> {
  if (browserImportBusy || browserImportCancelling) return false;
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
  return true;
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
  // 2026-08-06 restyle. Built from the same parts as the stealth and burn
  // screens: same card, same inputs, same eye toggles, same button and link.
  // The two helper sentences became hints beside their labels -- a standalone
  // "6 minimum. 12+ suggested." under a box reads as a rule you already broke.
  const eye = (id: string, label: string) =>
    `<button class="password-eye" type="button" data-password-toggle="${id}" aria-controls="${id}" aria-label="${label}">${passwordEyeIcon()}</button>`;
  return `<section class="stealth-screen restore-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="stealth-title restore-title">Restore your account</h1>
    <form class="password-form stealth-form" id="identity-import-form" novalidate>
      <span class="restore-label-row"><label for="identity-recovery-phrase">Recovery phrase</label><em>stays on this device</em></span>
      <textarea class="restore-phrase" id="identity-recovery-phrase" rows="3" autocomplete="off" autocapitalize="none" spellcheck="false" required aria-describedby="import-error restore-journey-status"></textarea>
      <span class="restore-label-row"><label for="import-password">New password</label><em>6 minimum · 12+ suggested</em></span>
      <div class="password-input-row"><input id="import-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/>${eye("import-password", "Show password")}</div>
      <span class="restore-label-row"><label for="import-password-confirm">Confirm password</label></span>
      <div class="password-input-row"><input id="import-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/>${eye("import-password-confirm", "Show password")}</div>
      <p class="unlock-error" id="import-error" aria-live="polite"></p>
      <div id="restore-journey-status" aria-live="polite" aria-atomic="true">${renderCleanDeviceRestoreStatus(cleanDeviceRestoreState)}</div>
      <button class="stealth-submit restore-submit" id="identity-import-submit" type="submit" disabled><span>Restore</span>${signinArrowIcon()}</button>
    </form>
    <div class="setup-footer onboarding-actions stealth-links restore-links"><button class="text-button" type="button" data-onboarding="welcome">← Back</button></div>
  </section>`;
}

function restoredAccountReadyContent(): string {
  return `<section class="stealth-screen restore-screen restored-account-ready" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="stealth-title restore-title">Account ready</h1>
    <p class="restore-ready-lead">Your account passed every restore check and is protected on this device.</p>
    ${renderCleanDeviceRestoreStatus(cleanDeviceRestoreState)}
    <button class="stealth-submit restore-submit" data-onboarding="${identityChoiceOrProRoute()}" type="button"><span>Continue setup</span>${signinArrowIcon()}</button>
  </section>`;
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

/**
 * T15-A7/A8 — the live inputs to the recovery-kit state machine.
 *
 * `captureProven` is the runtime latch; `captureEnforcement` is the separate
 * question of whether this platform has any capture-protection primitive at
 * all. They were conflated before, which is how a Linux build ended up
 * claiming Windows capture resistance over a completely unprotected window.
 */
function recoveryKitStateNow(): RecoveryKitState {
  return {
    secrets: recoveryBundle,
    captureProven: recoveryCaptureGate.canRender(),
    captureEnforcement: captureProtectionEnforced() ? "enforced" : "unenforced",
    shownWithoutProtection: recoveryShownWithoutProtection,
    savedAcknowledged: recoverySavedAcknowledged,
    noRecoverySecretAcknowledged: recoveryNoSecretAcknowledged,
    kitUnsaved: recoveryKitUnsavedFlag.unsaved(),
  };
}

function applyRecoveryKitAction(action: RecoveryKitAction): "none" | "rejected" | "leave-recovery" {
  const { state, outcome } = recoveryKitReducer(recoveryKitStateNow(), action);
  if (outcome === "rejected") return outcome;
  recoveryBundle = state.secrets;
  recoveryShownWithoutProtection = state.shownWithoutProtection;
  recoverySavedAcknowledged = state.savedAcknowledged;
  recoveryNoSecretAcknowledged = state.noRecoverySecretAcknowledged;
  void persistRecoveryKitUnsaved(state.kitUnsaved);
  return outcome;
}

function recoveryProtectionNoticeMarkup(view: RecoveryKitView): string {
  return `<p class="recovery-protection-notice" data-recovery-claim="${view.claim}">${escapeHtml(view.notice)}</p>`;
}

function recoveryExitsMarkup(view: RecoveryKitView): string {
  return view.exits.map((exit) => {
    // One button shape everywhere (design record §3): transparent fill, outline,
    // cyan on hover. The filled teal `.button.primary` these exits used to wear
    // broke the single-outline-button rule on a screen canon never drew.
    if (exit.id === "retry-protection") {
      return `<button class="signin-unlock osl-continue" id="retry-recovery-protection" type="button"><span class="signin-unlock-label">${escapeHtml(exit.label)}</span></button>`;
    }
    if (exit.id === "show-anyway") {
      return `<div class="recovery-show-anyway"><label for="recovery-show-anyway-ack">${escapeHtml(view.acknowledgementPrompt ?? "")}</label><input id="recovery-show-anyway-ack" type="text" maxlength="32" autocomplete="off" autocapitalize="none" spellcheck="false"/><button class="signin-unlock osl-continue" id="recovery-show-anyway" type="button"><span class="signin-unlock-label">${escapeHtml(exit.label)}</span></button></div>`;
    }
    if (exit.id === "remind-me-later") {
      return `<button class="signin-unlock osl-continue" id="recovery-remind-later" type="button"><span class="signin-unlock-label">${escapeHtml(exit.label)}</span></button>`;
    }
    return `<form class="setup-surface recovery-reveal-form" id="recovery-reveal-form" novalidate><label for="recovery-reveal-password">Your password</label><div class="password-input-row"><input id="recovery-reveal-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" required/><button class="password-eye" type="button" data-password-toggle="recovery-reveal-password" aria-controls="recovery-reveal-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="recovery-reveal-error" role="alert">${recoveryRevealError ? escapeHtml(recoveryRevealError) : ""}</p><button class="signin-unlock osl-continue" type="submit" ${recoveryRevealBusy ? "disabled" : ""}><span class="signin-unlock-label">${escapeHtml(exit.label)}</span></button></form>`;
  }).join("");
}

/**
 * The refusal screen. It used to be a dead end: one heading, one sentence, one
 * "Retry protection" button, and a backend that hid the window on the way in,
 * which read as a crash. Losing a recovery phrase forever is a worse outcome
 * than showing it on a screen that might be captured, so the owner now always
 * has a way through — and a way to defer that is remembered.
 */
function recoveryProtectionRefusalContent(view: RecoveryKitView): string {
  return `<h1 id="route-heading" tabindex="-1">Recovery secrets are being held back</h1><section class="setup-surface recovery-surface" role="alert">${recoveryProtectionNoticeMarkup(view)}<p>Your recovery kit has not been lost. Choose how you want to continue.</p>${recoveryExitsMarkup(view)}</section>`;
}

function recoveryRevealContent(view: RecoveryKitView): string {
  return `<h1 id="route-heading" tabindex="-1">Finish saving your recovery kit</h1><section class="setup-surface recovery-surface">${recoveryProtectionNoticeMarkup(view)}${recoveryExitsMarkup(view)}</section>`;
}

/** Two overlaid glyphs: the copy one, and the tick that replaces it for 2s. */
/**
 * The button says "Copied" and shows a tick for two seconds. The toast alone was
 * easy to miss on a screen where the thing you just copied is still on display,
 * and "did that work?" on a recovery phrase is the wrong doubt to leave.
 */
let recoveryCopiedTimer: ReturnType<typeof setTimeout> | null = null;

function flashRecoveryCopied(): void {
  const button = document.querySelector<HTMLButtonElement>("#copy-recovery-kit");
  const label = button?.querySelector<HTMLElement>(".signin-unlock-label");
  if (!button || !label) return;
  button.classList.add("copied");
  label.textContent = label.dataset.copiedLabel ?? "Copied";
  if (recoveryCopiedTimer) clearTimeout(recoveryCopiedTimer);
  recoveryCopiedTimer = setTimeout(() => {
    // The screen may have been repainted in the meantime, so re-find it rather
    // than holding the element captured above.
    const current = document.querySelector<HTMLButtonElement>("#copy-recovery-kit");
    const currentLabel = current?.querySelector<HTMLElement>(".signin-unlock-label");
    current?.classList.remove("copied");
    if (currentLabel) currentLabel.textContent = currentLabel.dataset.copyLabel ?? "Copy recovery kit";
    recoveryCopiedTimer = null;
  }, 2000);
}

function recoveryCopyIcon(): string {
  return `<svg class="signin-icon recovery-copy-glyph" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15 V5 a2 2 0 0 1 2 -2 h10"/></svg>`;
}

function recoveryCopiedIcon(): string {
  return `<svg class="signin-icon recovery-copied-glyph" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12 l5 5 L20 7"/></svg>`;
}

/** Drawn, not a styled native box: the native control cannot be recoloured
 *  reliably and rendered soft against this background. */
function recoveryCheckbox(): string {
  return `<svg class="recovery-check" viewBox="0 0 18 18" width="18" height="18" fill="none" aria-hidden="true"><rect class="recovery-check-box" x="1.5" y="1.5" width="15" height="15" rx="2" stroke-width="1.5"/><path class="recovery-check-mark" d="M5 9.5 L8 12.5 L13 6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
}

function recoveryContent(): string {
  const state = recoveryKitStateNow();
  const view = recoveryKitView(state);
  if (view.mode === "reveal-required") return recoveryRevealContent(view);
  if (view.mode === "refusal") return recoveryProtectionRefusalContent(view);
  const secrets = visibleRecoverySecrets(state);
  if (!secrets) return `<section class="onboarding-centered-step recovery-empty" aria-labelledby="route-heading"><p class="eyebrow">Recovery</p><h1 id="route-heading" tabindex="-1">No recovery secret is available</h1><button class="button primary" id="recovery-no-secret-continue" type="button">Continue</button></section>`;
  // 2026-08-06 restyle. Gone from this screen: the Mullvad and Android "next
  // steps" cards (neither is a step, and one is not built) and numbered badges.
  //
  // The capture notice is deliberately NOT here. Liam removed it from this
  // screen on 2026-08-06 after it was raised with him. It still shows on the
  // refusal and reveal screens, which are the two states where OSL is holding
  // the secret back and has to say why.
  return `<section class="recovery-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="recovery-screen-title">Save your recovery kit</h1>
    <div class="recovery-phrase-list">${recoveryKitSecretCardsMarkup(secrets, escapeHtml)}</div>
    <details class="recovery-account-details" id="recovery-account-details"><summary>Account details</summary><code>${escapeHtml(secrets.userId)}</code></details>
    <button class="signin-unlock osl-continue recovery-copy" id="copy-recovery-kit" type="button"><span class="signin-unlock-label" data-copy-label="Copy recovery kit" data-copied-label="Copied">Copy recovery kit</span>${recoveryCopyIcon()}${recoveryCopiedIcon()}</button>
    <label class="recovery-saved-row"><input class="sr-only" id="recovery-saved" type="checkbox" ${recoverySavedAcknowledged ? "checked" : ""}/>${recoveryCheckbox()}<span>I saved my recovery kit</span></label>
    ${continueButton(`id="recovery-continue" ${recoverySavedAcknowledged ? "" : "disabled aria-disabled=\"true\""}`, "recovery-continue-button")}
  </section>`;
}

/**
 * T15-A3/A4 + A8 — read the kit back after a restart.
 *
 * This is what makes "Remind me later" safe and what makes the resume policy
 * possible at all: the phrase is not held anywhere in this layer, it is
 * decrypted out of the password marker by the backend against the password the
 * owner types here, and it lands in memory only.
 */
async function revealRecoveryKit(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const password = document.querySelector<HTMLInputElement>("#recovery-reveal-password")?.value ?? "";
  const outcome = await runRecoveryReveal(password, {
    isBusy: () => recoveryRevealBusy,
    setBusy: (busy) => { recoveryRevealBusy = busy; },
    setError: (message) => { recoveryRevealError = message; },
    // Forced and flushed: the password field this form owns still holds the
    // typed password when the answer lands, and an unforced paint would be
    // deferred by the sensitive-edit guard — which is exactly how a failed or
    // even a *successful* reveal used to leave the screen frozen.
    render: () => { forceOnboardingPaint = true; renderNow(); },
    proveCaptureProtection: () => proveRecoveryCaptureProtection(),
    readRecoveryPhrase: (typed) => viewHubRecoveryPhrase(typed),
  });
  if (outcome.kind !== "revealed") return;
  applyRecoveryKitAction({
    kind: "revealed",
    secrets: {
      userId: core.readiness.activeOslUserId ?? "Local OSL identity",
      // The 12-word ACCOUNT phrase is shown once at creation and is not
      // re-derivable from the password marker. Saying so is the honest
      // answer; pretending this screen restores it would not be.
      identityPhrase: null,
      passwordPhrase: outcome.passwordPhrase,
    },
  });
  forceOnboardingPaint = true;
  renderNow();
}


function identityPasswordForm(title: string, action: string, mode: "setup" | "unlock"): string {
  const setup = mode === "setup";
  // D80: ONE input. The value typed here is matched against the main, stealth
  // and burn credentials by the same constant-time backend comparison, and the
  // screen may not name, hint at, or reserve space for any of the alternates.
  // A labelled "Burn code" row used to sit under this one; it told anyone who
  // seized the device that a duress mechanism exists, which is the only thing
  // a duress mechanism cannot survive. Nothing below may reintroduce that --
  // not as visible text, not as a placeholder, not as an `sr-only` label,
  // `aria-label`, `title`, `autocomplete` token or `data-` attribute, because
  // the accessibility tree is a published surface this project already drives
  // the app through. See `unlock-screen-single-credential.test.ts`.
  // 2026-08-08: rebuilt against the real design export (Sign In Final.dc.html).
  // The unlock step keeps the sign-in entry skeleton: ghost mark, the password
  // row, ONE outline button with the lock that opens on hover, then quiet
  // links. Gone by design: the visible "Sign in" heading, the filled teal
  // Unlock button, the boxed Back bar, and the eye floating outside its field.
  if (!setup) return `<section class="signin-card signin-lock-screen signin-password-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" class="sr-only" tabindex="-1">Sign in</h1>
    <div class="signin-lock-column signin-password-column">
      <img class="signin-ghost-mark signin-password-mark" src="${oslGhostMarkUrl}" alt="" width="148" height="148"/>
      <form class="password-form unlock-form" id="identity-password-form" data-password-mode="unlock" novalidate><label class="sr-only" for="identity-password">Password</label><div class="password-input-row signin-password-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="current-password" placeholder="Password" required aria-describedby="password-error" autofocus/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="signin-unlock" id="identity-password-submit" type="submit" disabled><span class="signin-unlock-label">Sign in</span>${signinLockIcon()}</button></form>
      <div class="signin-quiet-links"><button class="signin-recovery" type="button" data-onboarding="account-recovery">Forgot password?</button><button class="signin-recovery" type="button" data-onboarding="welcome">Back</button></div>
    </div>
  </section>`;
  return `<h1 id="route-heading" class="password-screen-title" tabindex="-1">${title}</h1><form class="setup-surface password-form password-screen" id="identity-password-form" data-password-mode="setup" novalidate><label for="identity-password">Password</label><div class="password-input-row"><input id="identity-password" type="password" minlength="6" maxlength="128" autocomplete="new-password" required aria-describedby="password-help password-error"/><button class="password-eye" type="button" data-password-toggle="identity-password" aria-controls="identity-password" aria-label="Show password">${passwordEyeIcon()}</button></div><small id="password-help">6 minimum. 12+ suggested.</small><label for="identity-password-confirm">Confirm</label><div class="password-input-row"><input id="identity-password-confirm" type="password" minlength="6" maxlength="128" autocomplete="new-password" required/><button class="password-eye" type="button" data-password-toggle="identity-password-confirm" aria-controls="identity-password-confirm" aria-label="Show password">${passwordEyeIcon()}</button></div><p class="unlock-error" id="password-error" role="alert"></p><button class="signin-unlock" id="identity-password-submit" type="submit" disabled><span class="signin-unlock-label">${action}</span>${signinArrowIcon()}</button></form><button class="text-back password-screen-back" data-onboarding="welcome">← Back</button>`;
}

export function sendingSetupContent(): string {
  return onboardingSendingMarkup({
    // No downgrade. This line used to rewrite a saved Single Enter back to
    // Manual before rendering, so the owner was shown a mode they had not
    // chosen. P-13 in main-prohibitions.test.ts now bans that rewrite by name,
    // which is why the old expression is not quoted here.
    mode: setup.sendMode,
    riskAccepted: setup.acceptedRisk && setup.acceptedRiskForMode === setup.sendMode,
    captureEnabled: windowCaptureEnabled,
    captureApplied: windowCaptureEnabled && screenshotProtectionEnabled,
  });
}


/**
 * 2026-08-06 re-split. This screen and the preset screen before it used to show
 * the SAME six rows -- pick a preset, then review the preset. Two screens, one
 * list, and the preset itself is read by nothing in the app.
 *
 * They are now one topic each, and each row is a real choice rather than a
 * read-only summary of a setting that does nothing:
 *   the preset screen  -> what happens AT THE MOMENT YOU SEND
 *   this screen        -> what OSL KEEPS ON THIS DEVICE afterwards
 * Same six settings, split by when they actually apply, which is the thing a
 * person can reason about.
 */
export function reviewDefaultsOnboardingContent(): string {
  if (deleteChoices === null) {
    const warning = `<p class="del-quiet" id="defaults-record-missing" role="alert">Review defaults could not be loaded. Choose what OSL may delete before continuing.</p>`;
    return onboardingDeleteMarkup(initialDeleteChoices()).replace("</section>", `${warning}</section>`);
  }
  return deleteChoices.deleteOldMessages
    ? firstTimedDeleteWarningMarkup(timedDeleteWarningAgreed)
    : onboardingDeleteMarkup(deleteChoices);
}

function coverDraftSetupContent(): string {
  return onboardingCoverMarkup(coverInsertion);
}

function silentVisibleSetupContent(): string {
  return onboardingSilentVisibleMarkup(silentVisibleMode);
}

function onboardingPasswordRoleContent(role: "stealth" | "burn"): string {
  return passwordRoleContent({
    role,
    configured: role === "stealth" ? passwordRoleStatus?.stealthPasswordSet : passwordRoleStatus?.burnPasswordSet,
    passwordEyeIcon,
    statusTag,
  });
}

function onboardingPrivacyContent(): string {
  // Resume older interrupted setups on the new combined page instead of
  // forcing users through the retired capture-only screen.
  return protectionPresetOnboardingContent();
}

function protectionPresetOnboardingContent(): string {
  return onboardingBeforeSendMarkup(beforeSendChecks);
}

function mullvadSetupContent(): string {
  // 2026-08-06 restyle. The three states used to look like three different
  // screens -- two of them a button, the third a bordered warning box. They are
  // now one status card whose DOT carries the state, so the page does not
  // rearrange itself depending on what is installed.
  //
  // Adapted from the handoff, which only drew the not-found case: the other two
  // states still need their action, so the card keeps one beside the status
  // line rather than becoming a read-only strip.
  const availability = mullvadStatus.availability;
  const found = availability === "installed";
  const state = found ? "found" : availability === "installable" ? "installable" : "missing";
  const line = found
    ? "Mullvad is installed on this device"
    : availability === "installable"
      ? "Mullvad is not installed. Windows can install it for you"
      : "Mullvad or Windows App Installer was not found";
  const action = found
    ? `<button class="mv-action" id="found-session-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Checking…" : "Found session"}</button><button class="mv-action" id="open-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Opening…" : "found session"}</button>`
    : availability === "installable"
      ? `<button class="mv-action" id="install-mullvad" type="button" ${mullvadBusy ? "disabled" : ""}>${mullvadBusy ? "Starting…" : "install"}</button>`
      : "";
  const notice = mullvadSetupNotice
    ? `<p class="mullvad-setup-notice" role="status">${escapeHtml(mullvadSetupNotice)}</p>`
    : "";
  return `<section class="mv-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="mv-title">Mullvad</h1>
    <p class="mv-quiet">Optional network privacy</p>
    <div class="mv-status" data-mullvad-state="${state}">
      <span class="mv-dot" aria-hidden="true"></span>
      <span class="mv-status-line">${line}</span>
      ${action}
    </div>
    ${notice}
    ${continueButton('id="continue-mullvad"', "mv-continue")}
    <div class="setup-footer onboarding-actions mv-links"><button class="text-button" id="skip-mullvad" type="button">Skip</button></div>
  </section>`;
}

function parseMullvadSetupRoute(raw: string | null): MullvadSetupRoute | null {
  return raw === "found-session" || raw === "no-mullvad" ? raw : null;
}

function persistMullvadSetupRoute(choice: MullvadSetupRoute): void {
  mullvadSetupRoute = choice;
  localStorage.setItem(mullvadSetupRouteStorageKey, choice);
}

function openOnboardingAppSelection(): void {
  resetOnboardingBranch();
  resetOnboardingConnections();
  onboardingRoute = "apps";
  render();
}

function confirmMullvadFoundSession(): boolean {
  if (mullvadStatus.availability !== "installed") {
    showToast("No existing Mullvad session was found");
    return false;
  }
  persistMullvadSetupRoute("found-session");
  mullvadSetupNotice = "Existing Mullvad session selected";
  render();
  return true;
}

async function openMullvadInstallPage(): Promise<void> {
  if (mullvadBusy) return;
  mullvadBusy = true;
  mullvadSetupNotice = "Opening Mullvad install page…";
  render();
  try {
    await withNativeDeadline(installMullvad(), "Open Mullvad install page");
    mullvadSetupNotice = "Mullvad install page opened";
  } catch (failure) {
    mullvadSetupNotice = localActionError(failure, "Mullvad install page could not open");
    showToast(mullvadSetupNotice);
  } finally {
    mullvadBusy = false;
    render();
  }
}

function continueMullvadSetup(): boolean {
  if (mullvadSetupRoute !== "found-session") {
    showToast("Choose Found session or Skip first");
    return false;
  }
  openOnboardingAppSelection();
  return true;
}

function skipMullvadSetup(): void {
  persistMullvadSetupRoute("no-mullvad");
  openOnboardingAppSelection();
}

function scrubCategoryChooserMarkup(compact = false): string {
  return `<details class="scrub-category-details" ${compact ? "" : "open"}><summary>Change what OSL looks for</summary><fieldset class="scrub-category-picker ${compact ? "compact" : ""}"><legend class="sr-only">Message categories</legend><p>All categories start on. These are review reminders, not judgments.</p><div>${scrubSignalDefinitions.map((signal) => `<label><input type="checkbox" data-scrub-category="${signal.id}" ${enabledScrubSignals.has(signal.id) ? "checked" : ""}/><span><strong>${signal.label}</strong><small>${signal.detail}</small></span></label>`).join("")}</div></fieldset></details>`;
}

function previousSetupRoute(current: OnboardingRoute): OnboardingRoute {
  if (current === "private-link") return "identity-choice";
  return onboardingRouteForBuild(previousOnboardingRoute(current, onboardingBranch) ?? "welcome");
}

async function saveSendingSetupDraft(): Promise<void> {
  await saveOnboardingPreferences({
    onboardingComplete: false,
    setup,
    coverInsertion,
    showPlaintextPreview: true,
    windowCaptureEnabled,
    rnWirePolicyRequested,
    forwardSecrecyMode,
  });
}

type QuickTourScreen = "setup" | "tour-card" | "app-selection" | "home";
type QuickTourControl = "Back" | "Next" | "Choose apps" | "Set card";
type QuickTourControlResult = {
  accepted: boolean;
  control: QuickTourControl;
  screen: QuickTourScreen;
  route: Route;
  onboardingRoute: OnboardingRoute;
  cardNumber: number | null;
  completedCardCount: number;
  reason: string | null;
};

function quickTourCardNumber(): number | null {
  return onboardingRoute === "tutorial" && onboardingTourStep >= 0 && onboardingTourStep < quickTourCards.length
    ? onboardingTourStep + 1
    : null;
}

function completedQuickTourCardCount(): number {
  return Math.min(Math.max(onboardingTourStep, 0), quickTourCards.length);
}

function quickTourScreen(): QuickTourScreen {
  if (route === "home") return "home";
  if (route === "onboarding" && onboardingRoute === "tutorial") {
    return quickTourCardNumber() === null ? "app-selection" : "tour-card";
  }
  return "setup";
}

function quickTourControlResult(control: QuickTourControl, accepted: boolean, reason: string | null = null): QuickTourControlResult {
  return {
    accepted,
    control,
    screen: quickTourScreen(),
    route,
    onboardingRoute,
    cardNumber: quickTourCardNumber(),
    completedCardCount: completedQuickTourCardCount(),
    reason,
  };
}

function setQuickTourCardNumber(cardNumber: number): QuickTourControlResult {
  if (!Number.isInteger(cardNumber) || cardNumber < 1 || cardNumber > quickTourCards.length) {
    return quickTourControlResult("Set card", false, "invalid-card-number");
  }
  route = "onboarding";
  onboardingRoute = quickTourRoute;
  onboardingTourStep = cardNumber - 1;
  return quickTourControlResult("Set card", true);
}

function backQuickTour(): QuickTourControlResult {
  if (onboardingTourStep > 0) {
    onboardingTourStep -= 1;
  } else if (replayingOnboardingTour) {
    replayingOnboardingTour = false;
    route = "home";
  } else {
    onboardingRoute = previousSetupRoute(onboardingRoute);
  }
  return quickTourControlResult("Back", true);
}

function nextQuickTour(): QuickTourControlResult {
  if (onboardingTourStep < 0 || onboardingTourStep >= quickTourCards.length) {
    return quickTourControlResult("Next", false, "invalid-card-number");
  }
  onboardingTourStep += 1;
  return quickTourControlResult("Next", true);
}

function chooseAppsFromQuickTour(): QuickTourControlResult {
  if (onboardingTourStep !== quickTourCards.length - 1) {
    return quickTourControlResult("Choose apps", false, "quick-tour-incomplete");
  }
  onboardingTourStep += 1;
  return quickTourControlResult("Choose apps", true);
}

function rememberIdentityDiscoveryChoice(choice: IdentityDiscoveryChoice): void {
  identityDiscoveryChoice = choice;
  localStorage.setItem(identityDiscoveryChoiceStorageKey, choice);
}

function identityChoiceOrProRoute(): OnboardingRoute {
  return onboardingRouteForBuild(identityDiscoveryChoice === null ? "identity-choice" : "pro");
}

async function chooseNoPublicName(): Promise<void> {
  if (privateContactLinkBusy) return;
  onboardingRoute = "private-link";
  privateContactLink = null;
  privateContactLinkError = "";
  privateContactLinkBusy = true;
  render();
  const created = await createHubPrivateContactLink();
  privateContactLinkBusy = false;
  if (!created) {
    privateContactLinkError = "OSL could not create a private link. Nothing was published.";
    render();
    return;
  }
  privateContactLink = created;
  rememberIdentityDiscoveryChoice("private-link");
  render();
}

async function submitPublicName(form: HTMLFormElement): Promise<void> {
  const input = form.querySelector<HTMLInputElement>("#public-name-input");
  const username = input?.value.trim() ?? "";
  if (!isNormalizedOslUsername(username)) {
    identityChoiceError = "Use 3–30 lowercase letters, numbers, or underscores.";
    render();
    return;
  }
  form.setAttribute("aria-busy", "true");
  if (input) input.disabled = true;
  const claimed = await claimOslUsername(username);
  if (!claimed) {
    identityChoiceError = "That public name could not be claimed. Try another, or continue with no public name.";
    render();
    return;
  }
  claimedOslUsername = claimed.username;
  identityChoiceError = "";
  rememberIdentityDiscoveryChoice("public-name");
  onboardingRoute = onboardingRouteForBuild("pro");
  render();
}

function continueFromPrivateContactLink(): boolean {
  if (!privateContactLink || identityDiscoveryChoice !== "private-link") return false;
  onboardingRoute = onboardingRouteForBuild("pro");
  render();
  return true;
}

function syncRecoveryWordCheckControls(message = ""): void {
  const button = document.querySelector<HTMLButtonElement>("#recovery-word-check-continue");
  const status = document.querySelector<HTMLElement>("#recovery-word-check-status");
  const disabled = recoveryWordCheckContinueDisabled(recoveryWordCheckState);
  if (button) {
    button.disabled = disabled;
    if (disabled) button.setAttribute("aria-disabled", "true");
    else button.removeAttribute("aria-disabled");
  }
  if (status) status.textContent = message;
}

async function verifyRecoveryWordCheck(epoch: number): Promise<void> {
  if (!recoveryBundle || !everyRecoveryWordAnswered(recoveryWordCheckState)) return;
  syncRecoveryWordCheckControls("Checking…");
  try {
    const result = await checkHubRecoveryWordRetype(
      recoveryWordRetypeRequest(recoveryWordCheckState, recoveryBundle.passwordPhrase),
    );
    if (epoch !== recoveryWordCheckEpoch || onboardingRoute !== "recovery-check") return;
    recoveryWordCheckState = applyRecoveryWordRetypeResult(recoveryWordCheckState, result);
    const failed = new Set(result.failedPositions);
    document.querySelectorAll<HTMLInputElement>("[data-recovery-word-position]").forEach((input) => {
      const position = Number(input.dataset.recoveryWordPosition);
      if (failed.has(position)) input.setAttribute("aria-invalid", "true");
      else input.removeAttribute("aria-invalid");
    });
    syncRecoveryWordCheckControls(
      recoveryWordCheckContinueDisabled(recoveryWordCheckState)
        ? "One or more words did not match. Check the numbered words and try again."
        : "All requested words match.",
    );
  } catch {
    if (epoch !== recoveryWordCheckEpoch || onboardingRoute !== "recovery-check") return;
    syncRecoveryWordCheckControls("OSL could not check those words. Edit an answer to try again.");
  }
}

function bindRecoveryWordCheck(): void {
  document.querySelectorAll<HTMLInputElement>("[data-recovery-word-position]").forEach((input) => {
    input.addEventListener("input", () => {
      const position = Number(input.dataset.recoveryWordPosition);
      recoveryWordCheckState = setRecoveryWordCheckAnswer(recoveryWordCheckState, position, input.value);
      const epoch = ++recoveryWordCheckEpoch;
      input.removeAttribute("aria-invalid");
      syncRecoveryWordCheckControls();
      if (everyRecoveryWordAnswered(recoveryWordCheckState)) void verifyRecoveryWordCheck(epoch);
    });
  });
  document.querySelector<HTMLButtonElement>("#recovery-word-check-continue")?.addEventListener("click", () => {
    if (recoveryWordCheckContinueDisabled(recoveryWordCheckState)) return;
    if (applyRecoveryKitAction({ kind: "continue" }) !== "leave-recovery") return;
    recoveryWordCheckState = initialRecoveryWordCheckState();
    recoveryWordCheckEpoch += 1;
    resetOnboardingBranch();
    resetOnboardingConnections();
    onboardingRoute = pendingOnboardingRoute() ?? onboardingRouteForBuild("passwords");
    render();
  });
}

function bindOnboarding(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding]").forEach((button) => button.addEventListener("click", () => {
    handleOnboardingRouteAction(button.dataset.onboarding);
  }));
  bindAccountRecovery();
  document.querySelector<HTMLButtonElement>("#skip-pro-setup")?.addEventListener("click", () => {
    proOnboardingReadyResult = false;
    proOnboardingCodeEntryRequested = false;
    onboardingRoute = onboardingRouteForBuild(continueFromProOnboarding("skipped").route);
    render();
  });
  document.querySelector<HTMLButtonElement>("#continue-pro-ready")?.addEventListener("click", () => {
    if (!proOnboardingReadyResult) return;
    onboardingRoute = onboardingRouteForBuild(continueFromProOnboarding("activated").route);
    render();
  });
  document.querySelector<HTMLFormElement>("#activation-form")?.addEventListener("submit", (event) => void activatePro(event));
  bindSavedAccountControls();
  bindBrowserImportControls();
  bindPasswordVisibility();
  bindPasswordForm();
  bindImportForm();
  document.querySelector<HTMLFormElement>("#public-name-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void submitPublicName(event.currentTarget as HTMLFormElement);
  });
  document.querySelector<HTMLButtonElement>("#choose-no-public-name")?.addEventListener("click", () => void chooseNoPublicName());
  document.querySelector<HTMLButtonElement>("#retry-private-contact-link")?.addEventListener("click", () => void chooseNoPublicName());
  document.querySelector<HTMLButtonElement>("#copy-private-contact-link")?.addEventListener("click", async () => {
    if (!privateContactLink) return;
    try {
      await navigator.clipboard.writeText(privateContactLink.linkValue);
      showToast("Private link copied");
    } catch {
      showToast("Couldn’t copy the private link. Select it instead.");
    }
  });
  document.querySelector<HTMLButtonElement>("#continue-private-contact-link")?.addEventListener("click", () => {
    continueFromPrivateContactLink();
  });
  bindRecoveryWordCheck();
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
      flashRecoveryCopied();
    } catch {
      showToast("Couldn’t copy the recovery kit");
    }
  });
  recoverySaved?.addEventListener("change", () => {
    applyRecoveryKitAction({ kind: "set-saved-acknowledged", acknowledged: recoverySaved.checked });
    if (recoveryContinue) recoveryContinue.disabled = !recoverySavedAcknowledged;
  });
  recoveryContinue?.addEventListener("click", () => {
    if (!recoverySavedAcknowledged || !recoveryBundle) {
      showToast("Confirm you saved your recovery kit first");
      return;
    }
    recoveryWordCheckState = initialRecoveryWordCheckState();
    recoveryWordCheckEpoch += 1;
    onboardingRoute = "recovery-check";
    render();
  });
  document.querySelector<HTMLButtonElement>("#recovery-no-secret-continue")?.addEventListener("click", () => {
    if (applyRecoveryKitAction({ kind: "continue" }) !== "leave-recovery") return;
    resetOnboardingBranch();
    resetOnboardingConnections();
    onboardingRoute = identityChoiceOrProRoute();
    render();
  });
  // T15-A7: the two exits that make the refusal escapable.
  document.querySelector<HTMLButtonElement>("#recovery-show-anyway")?.addEventListener("click", () => {
    const typed = document.querySelector<HTMLInputElement>("#recovery-show-anyway-ack")?.value ?? "";
    if (applyRecoveryKitAction({ kind: "show-anyway", acknowledgement: typed }) === "rejected") {
      showToast(`Type “${RECOVERY_SHOW_ANYWAY_ACKNOWLEDGEMENT}” exactly to see your recovery kit without proven capture resistance`);
      return;
    }
    render();
  });
  document.querySelector<HTMLButtonElement>("#recovery-remind-later")?.addEventListener("click", () => {
    applyRecoveryKitAction({ kind: "remind-me-later" });
    showToast("OSL will ask again every time it opens until you save your recovery kit");
    resetOnboardingBranch();
    resetOnboardingConnections();
    onboardingRoute = identityChoiceOrProRoute();
    render();
  });
  const recoveryRevealForm = document.querySelector<HTMLFormElement>("#recovery-reveal-form");
  recoveryRevealForm?.addEventListener("submit", (event) => void revealRecoveryKit(event));
  // Implicit submission is not something this screen may depend on. It is the
  // only screen an owner can be stranded on, and a keyboard-only owner reaching
  // it must be able to finish without a pointer.
  document.querySelector<HTMLInputElement>("#recovery-reveal-password")?.addEventListener("keydown", (event) => {
    if (!submitsRecoveryReveal(event)) return;
    event.preventDefault();
    recoveryRevealForm?.requestSubmit();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app-choice]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.onboardingAppChoice as HomeAppId;
    const available = homeAppsFromServices(services)
      .some((app) => app.id === appId && app.visibility === "launch" && app.launchState === "available");
    if (!available) return;
    if (selectedOnboardingApps.has(appId)) selectedOnboardingApps.delete(appId);
    else selectedOnboardingApps.add(appId);
    hasExplicitOnboardingAppSelection = true;
    localStorage.setItem(selectedOnboardingAppsStorageKey, JSON.stringify([...selectedOnboardingApps]));
    onboardingConnectAppId = null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-app-not-built]").forEach((button) => button.addEventListener("click", () => {
    const appId = button.dataset.onboardingAppNotBuilt as HomeAppId;
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === appId);
    showToast(`${app?.displayName ?? "This app"} isn't built yet`);
  }));
  document.querySelector<HTMLButtonElement>("#continue-app-choice")?.addEventListener("click", async () => {
    // D-190: `ensureNativeCatalogForAppChoice` sets `nativeCatalogRefusal` on every
    // false it returns, and the panel renders it, so this early return is now a
    // visible refusal with an escape rather than a silent no-op.
    if (!await ensureNativeCatalogForAppChoice()) {
      render();
      return;
    }
    persistCombinedHomeChoices();
    await completeOnboarding();
  });
  document.querySelector<HTMLButtonElement>("#continue-without-apps")?.addEventListener("click", () => {
    void continueWithoutNativeApps();
  });
  // The tour's Back is the only Back on this step, so at the first sub-step it
  // has to leave the route rather than sit there disabled: back out of a replay
  // to Home, and out of first-run setup to the previous setup step.
  document.querySelector<HTMLButtonElement>("#onboarding-tour-back")?.addEventListener("click", () => {
    backQuickTour();
    render();
  });
  document.querySelector<HTMLButtonElement>("#onboarding-tour-next")?.addEventListener("click", () => {
    if (onboardingTourStep + 1 === quickTourCards.length) chooseAppsFromQuickTour();
    else nextQuickTour();
    render();
  });
  document.querySelector<HTMLButtonElement>("#finish-onboarding-tour")?.addEventListener("click", () => {
    replayingOnboardingTour = false;
    onboardingTourStep = 0;
    route = "home";
    render();
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
    // A replay opened from Settings is not first-run setup: Back there has to
    // leave the way it came in, not walk backwards into the setup spine.
    if (replayingOnboardingTour) {
      replayingOnboardingTour = false;
      onboardingTourStep = 0;
      route = "home";
      render();
      return;
    }
    onboardingRoute = onboardingRoute === "passwords"
      ? backOnboardingPasswordRole("stealth").route
      : onboardingRoute === "burnpass"
        ? backOnboardingPasswordRole("burn").route
        : onboardingRoute === "mullvad"
          ? "silent-visible"
          : previousSetupRoute(onboardingRoute);
    // Back on the Pro-ready screen re-opens code entry rather than leaving the
    // step: an active licence hides the form, and this is the one way back to
    // it during setup.
    if (onboardingRoute === "pro"
      && (licenseState.access === "pro" || licenseState.access === "offlineGrace")
      && !proOnboardingReadyResult
      && !proOnboardingCodeEntryRequested
    ) {
      proOnboardingCodeEntryRequested = true;
      render();
      return;
    }
    render();
    if (onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (onboardingRoute === "mullvad") void refreshMullvadSetup();
  });
  document.querySelectorAll<HTMLInputElement>("[data-send-mode]").forEach((button) => button.addEventListener("change", () => {
    const mode = button.dataset.sendMode as SendMode;
    // Single Enter restored 2026-08-06 on the owner's instruction. It reaches
    // the same risk acknowledgement Double Enter does.
    if (!["manual", "clipboard", "double", "single"].includes(mode)) return;
    setup.sendMode = mode;
    setup.placementMode = "atomic";
    setup.acceptedRisk = false;
    setup.acceptedRiskForMode = null;
    void saveSendingSetupDraft().catch(() => undefined);
    render();
  }));
  document.querySelector<HTMLInputElement>("#accept-send-risk")?.addEventListener("change", (event) => {
    const accepted = (event.currentTarget as HTMLInputElement).checked;
    setup.acceptedRisk = accepted;
    setup.acceptedRiskForMode = accepted ? setup.sendMode : null;
    void saveSendingSetupDraft().catch(() => undefined);
    render();
  });
  document.querySelector("#finish-onboarding")?.addEventListener("click", () => {
    if (onboardingRoute !== "sending") return;
    if (!canCompleteSetup(setup)) return;
    setup.placementMode = "atomic";
    void saveSendingSetupDraft().then(() => {
      onboardingRoute = "cover";
      render();
    }).catch(() => undefined);
  });
  // ONE listener. A second, guardless copy of this handler used to sit right
  // below (a merge artifact, like the duplicated sending grid): both fired, so
  // Continue advanced to "sending" even when the review record failed to load
  // and the guard above had already refused.
  document.querySelector("#continue-defaults-review")?.addEventListener("click", () => {
    if (deleteChoices === null) {
      showToast("Review defaults could not be loaded. Nothing changed.");
      return;
    }
    if (!timedDeleteContinueAllowed(deleteChoices, timedDeleteWarningAgreed)) return;
    onboardingRoute = "sending";
    render();
  });
  document.querySelectorAll<HTMLInputElement>('input[name="tor-route"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && (input.value === "tor" || input.value === "direct")) {
      torOnboarding = chooseTorRoute(torOnboarding, input.value);
      render();
    }
  }));
  document.querySelector<HTMLButtonElement>("[data-tor-choice-continue]")?.addEventListener("click", () => {
    if (torOnboarding.choice === null) return;
    void invoke("set_tor_preference", { preference: torOnboarding.choice }).then(() => {
      onboardingRoute = "defaults";
      render();
    }).catch(() => {
      // Do not advance: without native persistence the send boundary remains
      // fail-closed, and showing the next step would imply otherwise.
    });
  });
  document.querySelectorAll<HTMLInputElement>('input[name="forward-secrecy-mode"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && (input.value === "protect-past" || input.value === "keep-group-delivery")) {
      forwardSecrecyOnboarding = chooseForwardSecrecyMode(forwardSecrecyOnboarding, input.value);
      render();
    }
  }));
  document.querySelector<HTMLButtonElement>("[data-forward-secrecy-continue]")?.addEventListener("click", () => {
    if (forwardSecrecyOnboarding.choice === null) return;
    const selectedForwardSecrecyMode = forwardSecrecyOnboarding.choice === "protect-past" ? "protectPast" : "keepGroupDelivery";
    void saveOnboardingPreferences({ onboardingComplete: false, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode: selectedForwardSecrecyMode }).then((saved) => {
      forwardSecrecyMode = saved.forwardSecrecyMode;
      rnWirePolicyRequested = saved.rnWirePolicyRequested;
      onboardingRoute = "privacy";
      render();
    }).catch(() => undefined);
  });
  document.querySelectorAll<HTMLInputElement>('input[name="protection-preset"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && protectionPresetValues.includes(input.value as ProtectionPreset)) {
      protectionPreset = input.value as ProtectionPreset;
      persistProtectionPreset();
      render();
    }
  }));
  document.querySelector<HTMLInputElement>("#delete-drafts")?.addEventListener("change", (event) => {
    deleteChoices = { ...(deleteChoices ?? initialDeleteChoices()), deleteDrafts: (event.currentTarget as HTMLInputElement).checked };
    render();
  });
  document.querySelector<HTMLInputElement>("#delete-old-messages")?.addEventListener("change", (event) => {
    deleteChoices = { ...(deleteChoices ?? initialDeleteChoices()), deleteOldMessages: (event.currentTarget as HTMLInputElement).checked };
    timedDeleteWarningAgreed = false;
    render();
  });
  document.querySelector<HTMLInputElement>("#timed-delete-warning-agreement")?.addEventListener("change", (event) => {
    timedDeleteWarningAgreed = (event.currentTarget as HTMLInputElement).checked;
    render();
  });
  document.querySelector<HTMLButtonElement>("#timed-delete-warning-not-now")?.addEventListener("click", () => {
    deleteChoices = { ...(deleteChoices ?? initialDeleteChoices()), deleteOldMessages: false };
    timedDeleteWarningAgreed = false;
    render();
  });
  document.querySelector<HTMLInputElement>("#warn-unprotected")?.addEventListener("change", (event) => {
    beforeSendChecks = { ...beforeSendChecks, warnUnprotected: (event.currentTarget as HTMLInputElement).checked };
    render();
  });
  document.querySelector<HTMLInputElement>("#warn-protected")?.addEventListener("change", (event) => {
    beforeSendChecks = { ...beforeSendChecks, warnProtected: (event.currentTarget as HTMLInputElement).checked };
    render();
  });
  document.querySelectorAll<HTMLInputElement>('input[name="clean-files"]').forEach((input) => input.addEventListener("change", () => {
    if (!input.checked || !CLEAN_FILES_CHOICES.includes(input.value as CleanFilesChoice)) return;
    beforeSendChecks = { ...beforeSendChecks, cleanFiles: input.value as CleanFilesChoice };
    render();
  }));
  document.querySelectorAll<HTMLInputElement>('input[name="cover-mode"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && (input.value === "insert-on-send" || input.value === "type-naturally")) {
      coverInsertion = chooseCoverInsertion(coverInsertion, input.value);
      void saveOnboardingPreferences({ onboardingComplete: false, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode });
      render();
    }
  }));
  document.querySelector("#continue-cover-draft")?.addEventListener("click", () => {
    // The step exists to make this choice; Continue without one is a no-op,
    // not a silent default.
    if (coverInsertion === null) {
      showToast("Choose how the cover text is inserted first");
      return;
    }
    onboardingRoute = "silent-visible";
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-silent-visible-mode]").forEach((button) => button.addEventListener("click", () => {
    const next = chooseSilentVisibleMode(silentVisibleMode, button.dataset.silentVisibleMode);
    if (next === silentVisibleMode) return;
    silentVisibleMode = next;
    render();
  }));
  document.querySelector<HTMLButtonElement>("#continue-silent-visible")?.addEventListener("click", () => {
    if (silentVisibleMode === null) return;
    // The password steps moved to the front of the spine (owner's order,
    // 2026-08-08), so this step now hands over to the Mullvad offer instead.
    onboardingRoute = "mullvad";
    void refreshMullvadSetup();
    render();
  });
  bindOnboardingPasswordRole();
  document.querySelectorAll<HTMLButtonElement>("button[data-password-role-next]").forEach((button) => button.addEventListener("click", () => {
    onboardingRoute = button.dataset.passwordRoleNext as OnboardingRoute;
    render();
    if (onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (onboardingRoute === "mullvad") void refreshMullvadSetup();
  }));
  document.querySelectorAll<HTMLButtonElement>("button[data-skip-onboarding-password-role]").forEach((button) => button.addEventListener("click", () => {
    const role = onboardingRoute === "passwords" ? "stealth" : "burn";
    const outcome = skipOnboardingPasswordRole(role);
    const next = button.dataset.skipOnboardingPasswordRole as OnboardingRoute;
    if (outcome.route !== next) return;
    onboardingRoute = next;
    render();
    if (next === "browser") void refreshBrowserImportReadiness();
    if (next === "mullvad") void refreshMullvadSetup();
  }));
  document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => { onboardingRoute = "tor"; render(); });
  document.querySelector<HTMLInputElement>("#window-capture-enabled")?.addEventListener("change", async (event) => {
    windowCaptureEnabled = (event.currentTarget as HTMLInputElement).checked;
    await setScreenshotProtection(windowCaptureEnabled).catch(() => false);
    screenshotProtectionEnabled = windowCaptureEnabled && captureProtectionEnforced();
    if (windowCaptureEnabled && !screenshotProtectionEnabled) showToast("Windows capture resistance is unavailable on this device");
    await saveSendingSetupDraft().catch(() => undefined);
    render();
  });
  document.querySelector("#skip-mullvad")?.addEventListener("click", () => { skipMullvadSetup(); });
  document.querySelector("#continue-mullvad")?.addEventListener("click", () => { continueMullvadSetup(); });
  document.querySelector("#install-mullvad")?.addEventListener("click", () => void openMullvadInstallPage());
  document.querySelector("#found-session-mullvad")?.addEventListener("click", () => { confirmMullvadFoundSession(); });
  document.querySelector("#open-mullvad")?.addEventListener("click", () => void runMullvadSetupAction("open"));
  document.querySelector("#close-decoy")?.addEventListener("click", () => {
    onboardingRoute = "unlock";
    render();
    void getCurrentWindow()?.close().catch(() => undefined);
  });
}

function resetAccountRecovery(): void {
  accountRecoveryFlow = initialAccountRecoveryFlow;
  legacyRecoveryMigration = null;
  approvedAccountRecovery = null;
}

function formValue(form: HTMLFormElement, name: string): string {
  const field = form.elements.namedItem(name) as { value?: unknown } | null;
  return typeof field?.value === "string" ? field.value : "";
}

async function runAccountRecoveryPhrase(phrase: string): Promise<void> {
  // submitRecoveryPhrase deliberately swallows the verifier's error into one
  // safe message, so the legacy-marker refusal is captured on the way past
  // rather than re-derived from the message it returns.
  let refusal: unknown = null;
  accountRecoveryFlow = await submitRecoveryPhrase(accountRecoveryFlow, phrase, {
    setPassword: (newPassword, recoveryToken) => accountRecoveryDependencies.setPassword(newPassword, recoveryToken),
    verifyPhrase: async (value) => {
      try {
        return await accountRecoveryDependencies.verifyPhrase(value);
      } catch (failure) {
        refusal = failure;
        throw failure;
      }
    },
  });
  if (legacyMarkerRecoveryRefused(refusal)) legacyRecoveryMigration = { kind: "needs-current-password", phraseVerified: true };
  render();
}

async function runAccountRecoveryPassword(newPassword: string, confirmPassword: string): Promise<void> {
  accountRecoveryFlow = await submitRecoveredPassword(accountRecoveryFlow, newPassword, confirmPassword, accountRecoveryDependencies);
  render();
}

async function runLegacyPhraseWrap(currentPassword: string): Promise<void> {
  try {
    const repaired = await addLegacyPhraseWrap(currentPassword, recoveryMigrationDependencies);
    // Repaired means the phrase alone can drive recovery again, so the user is
    // returned to the phrase step rather than left on the migration screen.
    legacyRecoveryMigration = repaired.kind === "recoverable" ? null : repaired;
    if (repaired.kind === "recoverable") accountRecoveryFlow = initialAccountRecoveryFlow;
  } catch (failure) {
    showToast(localActionError(failure, "The recovery wrap was not added. Nothing was changed."));
  }
  render();
}

async function runRecoveryFreshStart(): Promise<void> {
  const accepted = window.confirm("Start over? This permanently removes this device's OSL account, including your burn list and everything OSL has stored on this device. It cannot be undone.");
  if (!accepted) return;
  try {
    await recoveryMigrationDependencies.freshStart();
    legacyRecoveryMigration = { kind: "fresh-start" };
  } catch (failure) {
    showToast(localActionError(failure, "OSL could not start over. Nothing was removed."));
  }
  render();
}

function bindAccountRecovery(): void {
  document.querySelector<HTMLFormElement>("[data-account-recovery-phrase]")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void runAccountRecoveryPhrase(formValue(event.currentTarget as HTMLFormElement, "recoveryPhrase"));
  });
  const passwordForm = document.querySelector<HTMLFormElement>("[data-account-recovery-password]");
  const newPassword = document.querySelector<HTMLInputElement>("#account-recovery-new-password");
  const confirmPassword = document.querySelector<HTMLInputElement>("#account-recovery-confirm-password");
  const continueButton = document.querySelector<HTMLButtonElement>("#account-recovery-continue");
  if (passwordForm) {
    const currentPasswords = (): { replacement: string; confirmation: string } => ({
      replacement: newPassword?.value ?? formValue(passwordForm, "newPassword"),
      confirmation: confirmPassword?.value ?? formValue(passwordForm, "confirmPassword"),
    });
    const canContinue = (replacement: string, confirmation: string): boolean =>
      accountRecoveryFlow.step === "password"
      && accountRecoveryFlow.recoveryToken !== null
      && isValidNewMainPassword(replacement)
      && replacement === confirmation;
    const updateContinue = (): void => {
      const values = currentPasswords();
      if (continueButton) continueButton.disabled = !canContinue(values.replacement, values.confirmation);
    };
    newPassword?.addEventListener("input", updateContinue);
    confirmPassword?.addEventListener("input", updateContinue);
    updateContinue();
    passwordForm.addEventListener("submit", (event) => {
      event.preventDefault();
      const values = currentPasswords();
      // Disabled is an affordance, not authority. Re-check the native phrase
      // approval and both live fields so dispatching submit directly cannot
      // reach the password-reset command.
      if (!canContinue(values.replacement, values.confirmation)) {
        if (continueButton) continueButton.disabled = true;
        // Preserve the state machine's specific local validation message on an
        // approved phrase; it refuses before invoking the native reset.
        if (accountRecoveryFlow.step === "password" && accountRecoveryFlow.recoveryToken) {
          void runAccountRecoveryPassword(values.replacement, values.confirmation);
        }
        return;
      }
      if (newPassword) newPassword.value = "";
      if (confirmPassword) confirmPassword.value = "";
      if (continueButton) continueButton.disabled = true;
      void runAccountRecoveryPassword(values.replacement, values.confirmation);
    });
  }
  document.querySelector<HTMLButtonElement>("[data-account-recovery-back]")?.addEventListener("click", () => {
    if (continueButton) continueButton.disabled = true;
    resetAccountRecovery();
    render();
  });
  document.querySelector<HTMLFormElement>("[data-recovery-add-phrase-wrap]")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void runLegacyPhraseWrap(formValue(event.currentTarget as HTMLFormElement, "currentPassword"));
  });
  document.querySelector<HTMLButtonElement>("[data-recovery-fresh-start]")?.addEventListener("click", () => void runRecoveryFreshStart());
}

function bindOnboardingPasswordRole(): void {
  const form = document.querySelector<HTMLFormElement>("[data-onboarding-password-role]");
  if (!form) return;
  const role = form.dataset.onboardingPasswordRole === "stealth" ? "stealth" : "burn";
  const current = form.elements.namedItem("current") as HTMLInputElement;
  const alternate = form.elements.namedItem("alternate") as HTMLInputElement;
  const confirm = form.elements.namedItem("confirm") as HTMLInputElement;
  const burnConfirmation = form.elements.namedItem("burnConfirmation") as HTMLInputElement | null;
  // The submit sits in the step's shared action row outside the form card and
  // is bound to it by the form-owner attribute, so it is not in the form's own
  // subtree.
  const submit = document.querySelector<HTMLButtonElement>("[data-onboarding-role-submit]")
    ?? form.querySelector<HTMLButtonElement>('button[type="submit"]');
  const error = form.querySelector<HTMLElement>("[data-onboarding-role-error]");
  const values = (): OnboardingPasswordRoleValues => ({
    current: current.value,
    alternate: alternate.value,
    confirm: confirm.value,
    burnConfirmation: burnConfirmation?.value ?? "",
  });
  const validate = (): void => {
    if (!submit || !error) return;
    submit.disabled = !canSetOnboardingPasswordRole(role, values());
    error.textContent = "";
  };
  current.addEventListener("input", validate);
  alternate.addEventListener("input", validate);
  confirm.addEventListener("input", validate);
  burnConfirmation?.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!submit || !error) return;
    const submitted = values();
    if (!canSetOnboardingPasswordRole(role, submitted)) {
      validate();
      return;
    }
    submit.disabled = true;
    current.value = "";
    alternate.value = "";
    confirm.value = "";
    if (burnConfirmation) burnConfirmation.value = "";
    try {
      const outcome = await continueOnboardingPasswordRole(role, submitted, setHubAlternatePassword);
      if (!outcome.accepted || !outcome.status) return;
      passwordRoleStatus = outcome.status;
      const next = form.dataset.onboardingPasswordNext as OnboardingRoute;
      if (outcome.route !== next) return;
      onboardingRoute = next;
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
    const next = togglePasswordVisibility(input.type);
    input.type = next.type;
    button.innerHTML = passwordEyeIcon(next.visible);
    button.setAttribute("aria-label", next.label);
    button.setAttribute("aria-pressed", String(next.visible));
  }));
}

function balancedFirstRunSetup(state: SetupState): SetupState {
  const sendMode = state.sendMode;
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
  if (identityDiscoveryChoice === null) throw new Error("setup missing identity discovery choice");
  setup = completedSetup;
  const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode });
  setup = saved.setup;
  coverInsertion = saved.coverInsertion;
  windowCaptureEnabled = saved.windowCaptureEnabled;
  rnWirePolicyRequested = saved.rnWirePolicyRequested;
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

async function runMullvadSetupAction(action: "install" | "open", returnRoute: "onboarding" | "connections" = "onboarding"): Promise<void> {
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
    mullvadReturnRoute = returnRoute;
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
  if (!form || !password || !submit || !error) return;
  const validate = (): void => {
    const valid = form.dataset.passwordMode === "setup"
      ? isValidNewMainPassword(password.value)
      : isValidMainPassword(password.value);
    submit.disabled = !valid || Boolean(confirm && confirm.value !== password.value);
    // A submit button that silently stays dead reads as a broken app. Two
    // mismatched password fields already disabled it above, but this line used
    // to clear the error region on every keystroke, so nothing ever said why --
    // measured in the running Linux build: both fields filled, "Create account"
    // inert, no message, no red field, no toast, no focus move. The disabled
    // condition above is deliberately unchanged; this only explains it. Stay
    // quiet until the user has actually typed a confirmation, because an empty
    // second field is not yet a mismatch.
    const confirmMismatch = Boolean(confirm && confirm.value.length > 0 && confirm.value !== password.value);
    error.textContent = confirmMismatch ? "Both passwords must match." : "";
    confirm?.classList.toggle("input-mismatch", confirmMismatch);
  };
  password.addEventListener("input", validate);
  confirm?.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    const setupMode = form.dataset.passwordMode === "setup";
    if (setupMode) {
      const decision = continuePasswordSetup("create", password.value, confirm?.value ?? "");
      if (!decision.accepted) {
        error.textContent = decision.message;
        submit.disabled = true;
        confirm?.classList.toggle("input-mismatch", decision.reason === "mismatched-passwords");
        password.disabled = false;
        if (confirm) confirm.disabled = false;
        password.focus();
        return;
      }
    }
    const idleLabel = submit.textContent ?? (setupMode ? "Create account" : "Unlock");
    let secret = password.value;
    // D80: every unlock outcome leaves the busy state at the same wall-clock
    // moment. See `unlockTransitionFloor`.
    const settleUnlockTransition = setupMode ? async (): Promise<void> => {} : unlockTransitionFloor();
    form.setAttribute("aria-busy", "true");
    password.disabled = true;
    if (confirm) confirm.disabled = true;
    submit.disabled = true;
    submit.textContent = setupMode ? "Creating account…" : "Unlocking…";
    if (!setupMode) password.value = "";
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
        recoveryNoSecretAcknowledged = false;
        recoveryShownWithoutProtection = false;
        // T15-A8: from this instant a kit exists that nobody has confirmed
        // saving. Until they do, every launch comes back here.
        //
        // This used to `throw`, which routed a created account into the
        // catch-all "the OSL account action failed" branch. The account was
        // NOT undone by that throw and could not be: `identity.json` and
        // `password_marker.json` are already on disk and the session is
        // already unlocked. The owner was told creation failed, restarted, and
        // was asked to unlock an account they had just been told did not
        // exist. The reminder is a resume hint, not the account and not the
        // secret, so a failure to persist it must not be reported as a failure
        // to create the account. Go to the recovery screen — which is where
        // the one-shot phrases are — and say plainly that this screen will not
        // be offered again.
        const recoveryKitReminderPersisted = await persistRecoveryKitUnsaved(true);
        onboardingRoute = "recovery";
        await proveRecoveryCaptureProtection();
        if (!recoveryKitReminderPersisted) showToast("Save your recovery kit now. OSL could not store the reminder that brings you back to this screen.");
      } else {
        const gate = await checkUnlockScreenCredential(secret);
        secret = "";
        if (gate.outcome === "wrong") {
          // D80: the message may not name the alternates. "Password or burn
          // code not recognized" told a shoulder-surfer that a burn code is a
          // thing you can type here.
          const failureMessage = gate.lockoutSecondsRemaining > 0
            ? `Try again in ${gate.lockoutSecondsRemaining} seconds.`
            : "Password not recognized.";
          const attemptWarning = unlockAttemptWarning(gate.attemptsUsed);
          error.textContent = attemptWarning === null
            ? failureMessage
            : `${failureMessage} ${attemptWarning}`;
          await settleUnlockTransition();
          form.removeAttribute("aria-busy");
          password.disabled = false;
          submit.disabled = false;
          submit.textContent = idleLabel;
          password.focus();
          return;
        }
        if (gate.outcome === "decoy") {
          identityStorageMethod = null;
          core = structuredClone(unavailableCoreIntegration);
          services = [];
          passwordRoleStatus = null;
          route = "onboarding";
          onboardingRoute = "decoy";
          await settleUnlockTransition();
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
          // D80: no toast. "OSL signed out on this device" announced that
          // something happened; landing silently on the welcome screen is
          // indistinguishable from a device that was never set up.
          await settleUnlockTransition();
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
          // D80: no toast. "Verified local OSL cleanup completed" was a
          // confession printed on the screen the adversary is holding.
          await settleUnlockTransition();
          render();
          return;
        }
        if (!gate.readiness?.unlocked) throw new Error("OSL did not unlock");
        // D80: concurrent, not sequential. Three chained IPC round trips made a
        // real unlock measurably slower than every alternate outcome, which is
        // the side channel this ruling is about.
        const [unlockedCore, unlockedServices, unlockedRoles] = await Promise.all([
          loadCoreIntegration(),
          loadLinkedServices().catch(() => services),
          loadHubPasswordRoleStatus().catch(() => null),
        ]);
        core = unlockedCore;
        services = unlockedServices;
        passwordRoleStatus = unlockedRoles;
        // T15-A8: an unsaved recovery kit outranks a "finished" onboarding.
        // Deferring the kit used to be indistinguishable from never having
        // been offered it, because nothing survived the unlock.
        if (onboardingComplete && !recoveryKitUnsavedFlag.unsaved()) {
          route = "home";
          void openMullvadOnStartup();
          void refreshUpdateStatus();
          void refreshIdentitySlots(true);
          void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") render(); });
          void listHubPeople().then((people) => { hubPeople = people ?? []; if (route === "home") render(); });
        }
        else {
          route = "onboarding";
          onboardingRoute = pendingOnboardingRoute() ?? onboardingRouteForBuild("passwords");
        }
      }
      secret = "";
      password.value = "";
      if (confirm) confirm.value = "";
      await settleUnlockTransition();
      render();
      if (discordQaShell && core.readiness.unlocked) void startDiscordQaShell();
      if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
      if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    } catch (failure) {
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check OSL account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        secret = "";
        error.textContent = "OSL could not verify the account state. Try again.";
        await settleUnlockTransition();
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
          onboardingRoute = setupMode ? onboardingRouteForBuild("passwords") : pendingOnboardingRoute() ?? onboardingRouteForBuild("passwords");
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
          onboardingRoute = identityChoiceOrProRoute();
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

// D80: the unlock screen has one input, so it has one call. The value goes to
// the single constant-time role comparison in the backend, which decides
// between main, stealth, burn and duress. The frontend never learns which
// credential it is holding and never branches before the call -- there is no
// "is this the burn code?" test on this side to time, and no second command
// whose mere presence in the IPC surface would advertise the mechanism.
async function checkUnlockScreenCredential(secret: string): Promise<Awaited<ReturnType<typeof unlockHubPasswordGate>>> {
  return unlockHubPasswordGate(secret);
}

/// D80 timing equalizer. Every unlock outcome -- main, stealth, burn, duress
/// and wrong -- must leave the busy state at the same wall-clock moment, or an
/// adversary who has watched one ordinary unlock can classify the next one by
/// stopwatch. This returns a gate, armed at submit, that all five branches
/// await immediately before they touch the DOM.
///
/// It is a floor, not a clamp: it can only hold a fast outcome back to the
/// deadline, it cannot pull a slow one forward. That is why the successful
/// branch above also loads its three post-unlock IPCs concurrently -- the goal
/// is that the slowest path still fits inside the floor. The one residual is
/// the burn outcome, whose backend cleanup is awaited before the command
/// returns and includes network-bound remote unregistration; see the report.
const unlockTransitionFloorMs = 1200;

function unlockTransitionFloor(): () => Promise<void> {
  const armedAt = Date.now();
  return async (): Promise<void> => {
    const remaining = unlockTransitionFloorMs - (Date.now() - armedAt);
    if (remaining <= 0) return;
    await new Promise<void>((resolve) => { setTimeout(resolve, remaining); });
  };
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
  const journey = document.querySelector<HTMLElement>("#restore-journey-status");
  if (!form || !phrase || !password || !confirm || !submit || !error) return;
  const canSubmit = (): boolean =>
    isRecoveryPhrase(phrase.value)
    && isValidNewMainPassword(password.value)
    && password.value === confirm.value;
  const showState = (state: CleanDeviceRestoreState): void => {
    cleanDeviceRestoreState = state;
    if (journey) journey.innerHTML = renderCleanDeviceRestoreStatus(state);
    const busy = !["restore", "refused", "account-ready"].includes(state.phase);
    form.setAttribute("aria-busy", busy ? "true" : "false");
    phrase.disabled = busy;
    password.disabled = busy;
    confirm.disabled = busy;
    submit.disabled = busy || state.phase === "account-ready";
    const label = submit.firstElementChild as HTMLElement | null;
    if (label) label.textContent = state.phase === "account-ready" ? "Account ready" : busy ? "Restoring…" : "Restore";
  };
  const fallbackCheck = (): RestoreCheck => cleanDeviceRestoreState.phase === "protecting-account"
    ? "account-protection"
    : cleanDeviceRestoreState.phase === "confirming-ready"
      ? "account-readiness"
      : "recovery-package-integrity";
  const refuse = (check: RestoreCheck): void => {
    error.textContent = "";
    showState(refuseRestore(check));
    submit.disabled = !canSubmit();
    phrase.focus();
  };
  const validate = (): void => {
    if (cleanDeviceRestoreState.phase === "refused") showState(initialCleanDeviceRestoreState);
    submit.disabled = !canSubmit();
    error.textContent = "";
  };
  phrase.addEventListener("input", validate);
  password.addEventListener("input", validate);
  confirm.addEventListener("input", validate);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    // Disabled is presentation rather than authorization. A synthetic submit
    // or altered DOM still runs this check and receives one named refusal.
    if (!canSubmit()) {
      refuse("recovery-phrase-format");
      return;
    }
    let phraseSecret = phrase.value;
    let passwordSecret = password.value;
    phrase.value = "";
    password.value = "";
    confirm.value = "";
    showState(restoreProgress("checking-input"));
    try {
      showState(restoreProgress("verifying-package"));
      const identity = await importHubOslIdentityPhrase(phraseSecret);
      identityStorageMethod = identity.storageMethod;
      phraseSecret = "";
      showState(restoreProgress("protecting-account"));
      const passwordResult = await setupHubMainPassword(passwordSecret);
      passwordSecret = "";
      showState(restoreProgress("confirming-ready"));
      core = await loadCoreIntegration();
      if (core.readiness.bootstrapStatus !== "ready" || !core.readiness.unlocked) {
        throw new Error("[restore-check:account-readiness]");
      }
      services = await loadLinkedServices().catch(() => services);
      recoveryBundle = { userId: identity.userId, identityPhrase: null, passwordPhrase: passwordResult.passwordRecoveryPhrase };
      recoverySavedAcknowledged = false;
      recoveryNoSecretAcknowledged = false;
      recoveryShownWithoutProtection = false;
      // Same rule as account creation above: the imported identity and its new
      // password are already on disk, so a failed reminder write is a warning,
      // not a failed recovery.
      const recoveryKitReminderPersisted = await persistRecoveryKitUnsaved(true);
      showState(restoreProgress("account-ready"));
      onboardingRoute = "recovery";
      await proveRecoveryCaptureProtection();
      render();
      if (!recoveryKitReminderPersisted) showToast("Save your recovery kit now. OSL could not store the reminder that brings you back to this screen.");
    } catch (failure) {
      phraseSecret = "";
      passwordSecret = "";
      const refreshedCore = await withNativeDeadline(loadCoreIntegration(), "Check recovered account", bootPreferenceDeadlineMs).catch(() => null);
      if (!refreshedCore) {
        refuse(restoreCheckFromFailure(failure, fallbackCheck()));
        return;
      }
      core = refreshedCore;
      if (core.readiness.bootstrapStatus === "ready" && core.readiness.unlocked) {
        showState(restoreProgress("account-ready"));
        onboardingRoute = "recovery";
        render();
        return;
      }
      if (core.readiness.bootstrapStatus === "passwordRequired") {
        onboardingRoute = "unlock";
        showToast("Account recovered. Unlock to continue.");
        render();
        return;
      }
      refuse(restoreCheckFromFailure(failure, fallbackCheck()));
    }
  });
}

function workspaceProtectedSheetMarkup(): string {
  const protectedSheet = protectedSheetMode === "local"
    ? localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)
    : activeEmbeddedHost
      ? peerProtectedSheetMarkup(peerProtectedSheet, hubPeople)
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
  mountChatSurfaceOverlays();
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
  // HOME IS A LAUNCHER, not a dashboard (PRODUCT.txt §2; DECISIONS.txt "UI
  // WORKSTREAMS" item 1, 6 August): no sidebar at all on this route, one 58px
  // shared header with integrated window controls, launcher tiles in the body
  // and the Friends panel down the right. Every other route keeps the hub
  // shell unchanged — the ruling removes the rail from Home, not the screens
  // it pointed at.
  if (route === "home" || route === "osl-chat") {
    const header = route === "osl-chat" ? homeHeader() : homeLauncherHeader();
    return `<div class="hub-layout home-launcher-shell ${route === "osl-chat" ? "osl-chats-shell" : ""}"><section class="hub-workspace"><div class="desktop-top-row home-launcher-top" data-tauri-drag-region="deep">${header}${desktopWindowControlsMarkup()}</div>${workspaceContent()}</section></div>${workspaceProtectedSheetMarkup()}`;
  }
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

/**
 * The rail badge for one destination.
 *
 * `label.slice(0, 1)` gave People and Privacy the same "P", so for two of the
 * six destinations the badge distinguished nothing. The collision is detected
 * from the labels rather than hardcoded, so a seventh destination that starts
 * with an existing letter is disambiguated on sight instead of silently
 * duplicating an existing badge.
 */
export function primarySidebarBadge(label: string, labels: readonly string[]): string {
  const initial = label.slice(0, 1).toLocaleUpperCase("en-US");
  const shares = labels.filter((other) => other.slice(0, 1).toLocaleUpperCase("en-US") === initial).length > 1;
  return shares ? `${initial}${label.slice(1, 2).toLocaleLowerCase("en-US")}` : initial;
}

export function primarySidebarMarkup(): string {
  const destinationLabels = oslPrimaryDestinations.map((destination) => destination.label);
  const activeDestination = (id: OslPrimaryDestination): boolean => {
    if (id === "home") return (route === "home" || route === "arrange-tiles") && !friendsDialogOpen;
    if (id === "inbox") return route === "inbox" || route === "osl-chat" || route === "osl-mail";
    if (id === "people") return route === "people" || friendsDialogOpen || (route === "settings" && settingsSection === "whitelisting");
    if (id === "privacy") return route === "privacy" || route === "scrub" || (route === "settings" && (settingsSection === "privacy" || settingsSection === "scrub" || settingsSection === "cleanup" || settingsSection === "appearance"));
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
    return `<button class="primary-sidebar-item ${current ? "active" : ""}" type="button" data-primary-destination="${destination.id}" ${destinationAttributes(destination.id)} ${current ? 'aria-current="page"' : ""}><span class="primary-sidebar-icon" aria-hidden="true">${escapeHtml(primarySidebarBadge(destination.label, destinationLabels))}</span><span><strong>${escapeHtml(destination.label)}</strong><small>${escapeHtml(destination.userQuestion)}</small></span></button>`;
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
  return `<nav class="app-launcher-strip" aria-label="Your apps">${configured.map((app) => `<button class="app-launcher in-dom-tooltip-anchor ${activeHomeAppId === app.id ? "active" : ""} ${appLaunchPendingId === app.id ? "pending" : ""}" data-home-app="${app.id}" aria-label="Open ${escapeHtml(app.displayName)}" ${appLaunchPendingId ? "disabled" : ""}>${homeAppLogo(app)}${inDomTooltipMarkup(app.displayName)}</button>`).join("")}</nav>`;
}

function simpleDeviceStatusMarkup(): string {
  const coreReady = isCoreProtectionReady(core.readiness);
  const protection = identityProtectionStatus(core.readiness.storageMethod);
  const ready = coreReady && protection.state === "protected";
  const label = ready ? "Ready" : "Needs attention";
  // The detail line used to be assigned `label`, so the header read "Ready"
  // stacked on "Ready". It says what the state means instead, and never
  // repeats the label.
  const detail = ready ? "Protected on this device" : coreReady ? "Device protection not confirmed" : "Finish setup";
  return `<div class="trust-state ${ready ? "ready" : "pending"} ${coreReady && !ready ? "not-secure" : ""}" role="status" data-identity-protection="${protection.state}"><span class="dot"></span><span><strong>${escapeHtml(label)}</strong><small>${escapeHtml(detail)}</small></span></div>`;
}

function autoScrubRunServiceName(serviceId: ServiceId): string {
  return services.find((service) => service.id === serviceId)?.displayName ?? autoScrubServiceLabels[serviceId];
}

function fleetIndicatorMarkup(): string {
  // The pill is a live monitor for cleanup runs, so it is chrome only while
  // there is something to monitor. `autoScrubFleetStatus === null` means the
  // cleanup subsystem reported nothing at all -- the state a fresh install on a
  // build without AutoScrub sits in permanently -- and
  // projectAutoScrubFleetStatus() renders that as "Unavailable in this
  // build / No cleanup running". Shipping that as a permanent titlebar fixture
  // made a feature's absence the loudest element on first launch, above the
  // window controls, before the owner had done anything. It is not a status the
  // owner can act on and it never changes, so there is nothing to monitor and
  // the pill is omitted. Whether Scrub is available in this build is still
  // stated where it belongs: Settings -> Scrub, and the Scrub tile on Home.
  // The moment a real fleet status exists -- any run, any phase, including a
  // refusal -- the pill returns, so no live state is ever hidden by this.
  if (autoScrubFleetStatus === null) return "";
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
  return `<aside class="fleet-indicator fleet-indicator-${status.tone} in-dom-tooltip-anchor" data-fleet-indicator data-open-run-count="${openRunCount}" data-open-run-names="${escapeHtml(runNames)}" role="status" aria-label="${escapeHtml(ariaLabel)}"><span class="fleet-indicator-dot" aria-hidden="true"></span><span class="fleet-indicator-text"><strong>${escapeHtml(status.label)}</strong><small>${escapeHtml(runNames)}</small></span>${inDomTooltipMarkup(ariaLabel)}</aside>`;
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
  return `<span class="native-discord-composer-unreachable in-dom-tooltip-anchor" id="native-discord-composer-unreachable" role="alert" data-composer-input-state="unreachable"><span class="native-discord-composer-unreachable-mark" aria-hidden="true">!</span> Your typing is going to Discord, not OSL — check the cyan ring${inDomTooltipMarkup(`OSL's protected composer is visible but is not receiving keyboard input — ${cause}. Anything you type now goes to Discord unencrypted. Stop typing, click the composer with the cyan lock ring, and confirm the ring before every message.`)}</span>`;
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
    return `<div class="native-discord-header-controls" aria-label="Discord privacy controls">${composerUnreachableNotice}<button class="header-protection-control burn in-dom-tooltip-anchor" data-open-burn="chat" type="button" ${inactive}>Burn${inDomTooltipMarkup("Burn this local OSL chat")}</button>${coverWritingControlsMarkup("discord", { covertextEnabled: nativeDiscordCovertextEnabled, aiAvailable: true, aiSelected: nativeDiscordAiCovertextSelected, covertextId: "native-discord-covertext", aiCovertextId: "native-discord-ai-covertext" })}</div>`;
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
  const openPlaceAllowed = context === null || scopeApproved;
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
    ? '<span class="discord-qa-whitelist-warning" id="discord-qa-whitelist-warning" role="status" data-whitelist-state="revoked">Encryption revoked for this chat — sends will fail until you allow it again. Press the Off list button to allow this chat again. Until then, every message you send in it will fail to send.</span>'
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
  const transcriptVisibilityControl = openPlaceAllowed
    ? `<button class="discord-qa-icon-control ${transcriptVisible ? "visible" : "hidden"}${transcriptFailed ? " transcript-failed" : ""}" id="discord-qa-transcript-visibility" type="button" aria-pressed="${transcriptVisible}" data-transcript-mode="${transcriptMode}" data-transcript-state="${transcriptOutcome}" ${transcriptFailed ? 'aria-invalid="true" ' : ""}aria-label="${transcriptVisible ? "Hide protected transcript" : "Show protected transcript"}" title="${transcriptTitle}" ${!verifiedPeer || visibilityBusy ? "disabled" : ""}>${eye}</button>`
    : "";
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
  // Keep a missing-composer lock visible and explain why it is unavailable.
  // An already-open protected composer stays operable after navigation so the
  // operator can always turn protection back off.
  const composerAvailability = composerLockAvailability(
    openPlaceAllowed && discordMarkerAvailable,
    nativeDiscordProtectionActive,
  );
  const composerUnavailable = composerAvailability.unavailable;
  const composerControlLabel = composerUnavailable
    ? `Protected composer unavailable — ${composerAvailability.reason}`
    : composerProtectionLabel;
  const composerControl = `<button class="discord-qa-icon-control composer ${nativeDiscordProtectionActive ? "locked" : "unlocked"}${composerRefusal ? " composer-refused" : ""}${composerAvailability.className}" id="discord-qa-toggle-composer" type="button" aria-pressed="${nativeDiscordProtectionActive}" aria-label="${composerControlLabel}" title="${composerControlLabel}" ${discordQaComposerBusy || composerAvailability.disabled ? "disabled" : ""} data-lock-state="${composerLockState}"${composerRefusal ? ' aria-invalid="true"' : ""}>${lock}${composerRefusedMark}</button>`;
  // Persistent, plain-language refusal in the header strip — the one surface
  // that draws above the borrowed native Discord window. It stays until the
  // next operator attempt or a successful open, so a reason can no longer be
  // produced and lost, and it is never populated by an automatic retry.
  const composerRefusalNotice = composerRefusal
    ? `<span class="discord-qa-composer-refusal" id="discord-qa-composer-refusal" role="status" data-lock-state="refused">${escapeHtml(composerRefusal.message)} — ${escapeHtml(composerRefusal.reason)}</span>`
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
  return `<div class="native-discord-header-controls discord-qa-header-controls" aria-label="Discord QA privacy controls"><div class="discord-qa-header-left"><button class="discord-qa-control danger icon-only in-dom-tooltip-anchor" data-open-burn="account" type="button" aria-label="Account Burn">${accountBurnIcon}${inDomTooltipMarkup("Open Account Burn confirmation")}</button></div><button class="discord-qa-control danger icon-only discord-qa-discord-burn in-dom-tooltip-anchor" data-open-burn="app" type="button" aria-label="Discord Burn">${discordBurnIcon}${inDomTooltipMarkup("Open Discord Burn confirmation")}</button><div class="discord-qa-header-right">${rowProofControl}<div class="discord-qa-whitelist" role="group" aria-label="Connected verified peer whitelist"><button class="in-dom-tooltip-anchor" id="discord-qa-whitelist-roster" type="button" aria-haspopup="dialog" aria-expanded="${whitelistRosterOpen}" ${discordQaHeaderBusy ? "disabled" : ""}>Whitelist${inDomTooltipMarkup("Review who is whitelisted and where")}</button>${discordQaWhitelistButtonMarkup({ scopeApproved, protectionActive: nativeDiscordProtectionActive, verifiedPeer: Boolean(verifiedPeer), busy: whitelistBusy })}</div><button class="discord-qa-control danger icon-only chat-burn in-dom-tooltip-anchor" data-open-burn="chat" type="button" ${inactive} aria-label="Chat Burn">${flame}${inDomTooltipMarkup("Open Chat Burn confirmation")}</button>${composerUnreachableNotice}${composerRefusalNotice}${transcriptNotice}${transcriptVisibilityControl}${composerControl}${whitelistWarningNotice}</div></div>`;
}

function trustedHeader(): string {
  // Service controls stay compact; deeper setup remains progressively disclosed.
  if (route === "home" || route === "arrange-tiles" || route === "inbox" || route === "people" || route === "privacy" || route === "scrub" || route === "activity" || route === "connections" || route === "osl-chat" || route === "osl-mail" || route === "osl-mail-status" || route === "osl-notes-status") return homeHeader();
  if (route === "mullvad") {
    return `<div class="trusted-stack"><header class="workspace-header mullvad-host-header"><button class="button compact" id="mullvad-return" type="button">${mullvadReturnRoute === "onboarding" ? "Back to setup" : "Back to Home"}</button><div class="service-context"><span><strong>Mullvad</strong><small>Existing session · capture resistance does not cover Mullvad</small></span></div></header></div>`;
  }
  if (route === "service"
    && activeService
    && serviceGuideStep !== null
    && !(discordQaShell && activeHomeAppId === "discord")) {
    return `<div class="trusted-stack home-trusted-stack"><header class="home-header guide-header"><button class="home-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span class="home-brand-copy"><strong>OSL Privacy</strong></span></button><div class="guide-header-service">${serviceLogo(activeService.id)}<span><strong>${escapeHtml(activeService.displayName)}</strong><small>${isCoreProtectionReady(core.readiness) ? "Ready" : "Needs attention"}</small></span></div>${settingsButtonMarkup()}</header></div>`;
  }
  const localProtectionBusy = protectedSheetCloseBusy || nativeProtectBusy;
  const localProtection = route === "service" && activeService !== null
    ? `<button class="local-protected-toggle" id="local-protected-toggle" type="button" aria-expanded="${localProtectedSheet.open || peerProtectedSheet.open || nativeDiscordProtectionActive}" ${localProtectionBusy ? "disabled" : ""}>${localProtectionBusy ? "Working…" : "Protect"}</button>`
    : "";
  const mailScope = route === "service" ? mailComposerEncryptionScope(activeHomeApp()) : "";
  const webSurfaceCapabilities: readonly WebSurfaceCapability[] = activeEmbeddedHost
    ? ["L1", "L2", "L3"]
    : activeDefaultBrowserCompanion
      ? ["L1"]
      : [];
  const serviceSurfaceLabel = webSurfaceCapabilities.length > 0
    ? webSurfaceLabel(webSurfaceCapabilities)
    : activeNativeHostMode === "existingSession"
      ? "Native companion"
      : activeNativeHostId
        ? "OSL app window"
        : "Needs setup";
  const serviceControls = route === "service" && activeService ? `<div class="service-context"><span class="service-context-logo">${serviceLogo(activeService.id)}</span><span><strong>${escapeHtml(activeHomeAppName())}</strong><small>${serviceSurfaceLabel}</small></span>${mailScope}${localProtection}</div>` : "";
  const onboardingContinue = route === "service" && onboardingServiceSetup && (activeEmbeddedHost || activeNativeHostId || activeDefaultBrowserCompanion)
    ? `<button class="button compact primary" id="onboarding-service-continue">Continue setup</button>`
    : "";
  return `<div class="trusted-stack"><header class="workspace-header"><div class="hub-command"><button class="command-brand" data-route="home" aria-label="OSL Privacy home"><img class="osl-logo logo-treatment" src="${oslVectorLogoUrl}" alt=""/><span><strong>OSL Privacy</strong></span></button>${appLauncherStrip()}${simpleDeviceStatusMarkup()}</div>${nativeDiscordHeaderControls()}${serviceControls ? `<div class="context-command">${serviceControls}</div>` : ""}${onboardingContinue}${settingsButtonMarkup("workspace-settings")}</header>${updateBannerMarkup()}</div>`;
}

function homeHeader(): string {
  const friendRequests = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange).length;
  const notificationCount = notificationsEnabled ? visibleAppNotifications().length : 0;
  const activeIdentity = hubIdentities.find((identity) => identity.active);
  const profileName = activeIdentity?.label?.trim() || "OSL Profile";
  const profileInitial = profileName.slice(0, 1).toLocaleUpperCase();
  const control = (label: string, icon: string, markup: string, ariaLabel = label) => `<button class="home-command-icon in-dom-tooltip-anchor" ${markup} type="button" aria-label="${ariaLabel}">${homeCommandIcon(icon as "friends" | "notifications" | "settings")}${inDomTooltipMarkup(label)}<span class="home-command-label">${label}</span></button>`;
  return `<div class="trusted-stack home-trusted-stack"><header class="home-header home-command-bar"><button class="home-logo-button in-dom-tooltip-anchor" data-route="home" aria-label="OSL Privacy home"><img src="${oslVectorLogoUrl}" alt=""/><span class="home-command-label">OSL</span>${inDomTooltipMarkup("Home")}</button><nav class="home-command-actions" aria-label="Home controls"><span class="home-command-word">Search</span><span class="home-command-word">Messages</span>${control("Friends", "friends", "data-open-friends", `Friends${friendRequests ? `, ${friendRequests} pending` : ""}`)}${friendRequests ? `<span class="home-command-badge">${Math.min(friendRequests, 99)}</span>` : ""}${control("Notifications", "notifications", "data-notification-settings", `Notifications${notificationCount ? `, ${notificationCount} new` : ""}`)}${notificationCount ? `<span class="home-command-dot" aria-hidden="true"></span>` : ""}${control("Settings", "settings", 'data-route="settings"')}<button class="home-command-icon home-profile-command in-dom-tooltip-anchor" data-route="settings" data-profile-settings type="button" aria-label="Profile: ${escapeHtml(profileName)}"><span class="home-command-avatar" aria-hidden="true">${escapeHtml(profileInitial)}</span><span class="home-command-label">Profile</span>${inDomTooltipMarkup(profileName)}</button></nav></header>${updateBannerMarkup()}</div>`;
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

/**
 * The Home protection summary, tasks 0820–0827. The DERIVATION survives the
 * Home rebuild untouched — only its placement moved. PRODUCT.txt §2 keeps
 * exactly one status affordance on Home: "The data readout, always visible,
 * in the header", so the derived headline renders in the shared header and
 * the recommendation renders in the bell popover instead of as body rows.
 *
 * PLACEMENT IS SUBJECT TO OWNER TASK 0825 ("judge Home protection panel",
 * who: liam — still open): if the owner rules differently on where this
 * readout lives, move the markup, not this derivation.
 *
 * The headline is DERIVED from every check this screen reports. It must
 * never claim more than the checks can prove: an unrun check or an
 * unreviewed person forbids the word "Protected" (design rule 1: never imply
 * protection that isn't there).
 */
function homeStatusSnapshot(): { overall: ReturnType<typeof homeOverallStatus>; recommended: HomePrimaryActionPlan } {
  const coreReady = isCoreProtectionReady(core.readiness);
  const protection = identityProtectionStatus(core.readiness.storageMethod);
  const launchableApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const connectableApps = launchableApps.filter((app) => app.launchState === "available");
  const connectedApps = connectableApps.filter((app) => app.linked || savedNativeApps.has(app.id as NativeAppId));
  const connectedAppsState = homeProtectionState(linkedServicesChecked, connectedApps.length > 0, {
    enabled: "Ready",
    unavailable: "Unavailable",
  });
  const pendingFriendReviews = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange).length;
  const verifiedFriends = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange).length;
  const overall = homeOverallStatus({
    coreReady,
    coreDetail: coreReadinessLabel(core.readiness),
    storageProtected: protection.state === "protected",
    storageDetail: protection.detail,
    connectedApps: connectedAppsState,
    pendingFriendReviews,
    verifiedFriends,
  });
  return { overall, recommended: homePrimaryRecommendation() };
}

function relativeNotificationTime(createdAt: string): string {
  const at = new Date(createdAt).getTime();
  if (!Number.isFinite(at)) return "";
  const minutes = Math.max(0, Math.round((Date.now() - at) / 60000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.round(hours / 24)}d`;
}

/** The bell popover: NOTIFICATIONS list per the design, plus the protection
 * recommendation the old dashboard body used to carry as "Needs attention"
 * (re-homed here per DECISIONS.txt; derivation unchanged — see
 * homeStatusSnapshot / task 0825). */
function homeNotificationsPopoverMarkup(): string {
  if (!homeNotificationsOpen) return "";
  const { overall, recommended } = homeStatusSnapshot();
  const attention = overall.state !== "protected"
    ? `<div class="home-notification-item home-notification-attention" role="status"><div class="home-notification-row"><span>${escapeHtml(recommended.title)}</span></div><small>${escapeHtml(recommended.detail)}</small>${primaryActionButton(recommended, `data-home-primary-issue="${recommended.issue}"`)}</div>`
    : "";
  const notifications = notificationsEnabled ? visibleAppNotifications() : [];
  const items = notifications.slice(0, 6).map((item) => `<button class="home-notification-item" type="button" data-notification-settings><div class="home-notification-row"><span>${escapeHtml(item.title)}</span><time>${escapeHtml(relativeNotificationTime(item.createdAt))}</time></div><small>${escapeHtml(item.detail)}</small></button>`).join("");
  const empty = !attention && !items
    ? `<div class="home-notification-item home-notification-empty"><small>${notificationsEnabled ? "Nothing new." : "Local activity is off."}</small></div>`
    : "";
  return `<div class="home-notifications-popover" role="dialog" aria-label="Notifications"><header>Notifications</header>${attention}${autoScrubHomeActivityMarkup()}${items}${empty}</div>`;
}

function dataAllowanceLimitBytes(): number {
  return licenseState.access === "pro" || licenseState.access === "offlineGrace"
    ? DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.proBytes
    : DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.freeBytes;
}

function dataAllowanceTotalBytes(): number | null {
  const values = Object.values(dataAllowanceLedger);
  return values.every((value): value is number => value !== null)
    ? values.reduce((total, value) => total + value, 0)
    : null;
}

function formatAllowanceBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toLocaleString("en-US", { maximumFractionDigits: 2 })} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toLocaleString("en-US", { maximumFractionDigits: 1 })} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toLocaleString("en-US", { maximumFractionDigits: 1 })} KB`;
  return `${bytes} B`;
}

function dataAllowanceHeaderReadoutMarkup(): string {
  const total = dataAllowanceTotalBytes();
  const limit = formatAllowanceBytes(dataAllowanceLimitBytes());
  const headline = total === null ? "Data · waiting for meter" : `Data · ${formatAllowanceBytes(total)} / ${limit}`;
  const detail = total === null ? "Allowance ledger not connected" : `Monthly allowance · warns at ${DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.warningPercent}%`;
  return `<button class="home-status-readout data-allowance-readout" type="button" data-route="settings" data-settings="account" aria-label="${escapeHtml(headline)}. ${escapeHtml(detail)}"><strong>${escapeHtml(headline)}</strong><small>${escapeHtml(detail)}</small></button>`;
}

/** The shared 58px header (README §"Shared header"): logo with the cyan
 * radial glow (always → Home), the data readout, bell popover, gear, then the
 * divider and window controls appended by workspaceShellMarkup(). */
function homeLauncherHeader(): string {
  const { overall } = homeStatusSnapshot();
  const notificationCount = notificationsEnabled ? visibleAppNotifications().length : 0;
  const unread = notificationCount > 0 || overall.state !== "protected";
  const readout = dataAllowanceHeaderReadoutMarkup();
  return `<div class="trusted-stack home-trusted-stack"><header class="home-launcher-header"><div class="home-launcher-left"><button class="home-launcher-logo" type="button" data-route="home" aria-label="OSL home"><span class="home-logo-glow" aria-hidden="true"></span><img src="${oslVectorLogoUrl}" alt=""/></button><span class="home-launcher-brand">OSL</span></div><nav class="home-launcher-actions" aria-label="Home controls">${readout}<span class="home-bell-anchor"><button class="home-launcher-icon" type="button" data-toggle-home-notifications aria-expanded="${homeNotificationsOpen}" aria-label="Notifications${notificationCount ? `, ${notificationCount} new` : ""}">${homeCommandIcon("notifications")}${unread ? `<span class="home-command-dot" aria-hidden="true"></span>` : ""}</button>${homeNotificationsPopoverMarkup()}</span><button class="home-launcher-icon" type="button" data-route="settings" aria-label="Settings">${homeCommandIcon("settings")}</button></nav></header>${updateBannerMarkup()}</div>`;
}

/** The persistent Friends panel, right column (design: 340px, collapsible).
 * PENDING = people awaiting verification or with a changed key; VERIFIED =
 * everyone OSL will protect. Reuses the 0828–0835 plumbing: add via
 * #add-friend-form, Accept via data-verify-person, Decline via
 * data-remove-person, row → the per-friend management surface. */
function homeFriendsPanelMarkup(): string {
  const pending = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange);
  const verified = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const initialOf = (person: HubPerson): string => (person.alias ?? "?").slice(0, 1).toLocaleUpperCase("en-US");
  const pendingRows = pending.length
    ? pending.map((person) => `<div class="home-friends-row home-friends-pending-row"><span class="home-friends-avatar pending" aria-hidden="true">${escapeHtml(initialOf(person))}</span><span class="home-friends-name"><strong>${escapeHtml(person.alias ?? "Unnamed friend")}</strong><small>${escapeHtml(person.pendingKeyChange ? "Key changed — review before trusting" : "Wants to verify with you")}</small></span><span class="home-friends-row-actions"><button class="home-friends-accept" type="button" data-verify-person="${escapeHtml(person.personId)}">${person.pendingKeyChange ? "Review" : "Accept"}</button><button class="home-friends-decline" type="button" data-remove-person="${escapeHtml(person.personId)}">Decline</button></span></div>`).join("")
    : `<p class="home-friends-empty">No one is waiting.</p>`;
  const verifiedRows = verified.length
    ? verified.map((person) => `<button class="home-friends-row home-friends-verified-row" type="button" data-friend-settings="${escapeHtml(person.personId)}" aria-label="Open ${escapeHtml(person.alias ?? "Unnamed friend")}"><span class="home-friends-avatar" aria-hidden="true">${escapeHtml(initialOf(person))}</span><span class="home-friends-name"><strong>${escapeHtml(person.alias ?? "Unnamed friend")}</strong><small class="verified">Verified</small></span><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 6 6 6-6 6"/></svg></button>`).join("")
    : `<p class="home-friends-empty">No verified friends yet.</p>`;
  const footer = friendCode && friendDisplayId
    ? `Your invite: <code>${escapeHtml(compactFriendId(friendDisplayId))}</code>`
    : "Your invite appears after OSL is unlocked";
  return `<aside class="home-friends-panel" aria-label="Friends"><div class="home-friends-scroll"><header class="home-friends-head"><button class="home-friends-collapse" type="button" data-collapse-friends aria-label="Hide the Friends panel"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 6 6 6-6 6"/></svg></button><h2>Friends</h2></header><form id="add-friend-form" class="home-friends-add"><input id="friend-code-input" placeholder="Paste an invite or type a username" aria-label="Paste an invite or type a username" autocomplete="off" autocapitalize="none" spellcheck="false"/><button type="submit">Add</button></form><p class="form-status" id="friend-form-status" role="status"></p><h3 class="home-friends-label">Pending</h3><div class="home-friends-list">${pendingRows}</div><h3 class="home-friends-label">Verified</h3><div class="home-friends-list">${verifiedRows}</div></div><footer class="home-friends-foot">${footer}</footer></aside>`;
}

/** A focused, non-destructive arrangement surface; its exits always return Home. */
function arrangeTilesContent(): string {
  const launchableApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const modules = [
    { id: "osl-chats", name: "OSL Chats", available: true },
    { id: "osl-mail", name: "OSL Mail", available: false },
    { id: "osl-notes", name: "OSL Notes", available: false },
    { id: "scrub", name: "Scrub", available: true },
  ] as const;
  const appsById = new Map(launchableApps.map((app) => [app.id, app]));
  const modulesById = new Map(modules.map((module) => [module.id, module]));
  const arranged = normalizeHomeTileArrangement(
    [...launchableApps.map((app) => app.id), ...modules.map((module) => module.id)],
    { order: homeTileOrder, hidden: [...hiddenHomeTiles] },
  );
  const hidden = new Set(arranged.hidden);
  const tile = (id: string, isHidden: boolean): string => {
    const position = arranged.order.indexOf(id);
    const module = modulesById.get(id as typeof modules[number]["id"]);
    const app = appsById.get(id as HomeAppId);
    if (!module && !app) return "";
    const name = module?.name ?? app?.displayName ?? id;
    const icon = module ? homeModuleIcon(module.id) : homeAppLogo(app!);
    const state = module ? (module.available ? "Ready" : "Coming later") : (app!.launchState === "available" ? "Ready" : "Coming later");
    const visibilityAction = isHidden ? "Show" : "Hide";
    const visibilityLabel = isHidden ? `Show ${name} on Home` : `Hide ${name} from Home`;
    return `<article class="arrange-tile ${isHidden ? "is-hidden" : ""}" data-tile-id="${escapeHtml(id)}" data-arrange-tile-id="${escapeHtml(id)}" draggable="true" role="listitem" aria-label="${escapeHtml(name)} tile, position ${position + 1}${isHidden ? ", hidden" : ""}"><span class="arrange-tile-grip" aria-hidden="true">⠿</span><span class="app-logo-plate arrange-tile-logo" aria-hidden="true">${icon}</span><span class="arrange-tile-copy"><strong>${escapeHtml(name)}</strong><small>${state}${isHidden ? " · Hidden from Home" : ""}</small></span><span class="arrange-tile-actions"><button class="button compact" type="button" data-tile-move="${escapeHtml(id)}:-1" ${position === 0 ? "disabled" : ""} aria-label="Move ${escapeHtml(name)} up">Move up</button><button class="button compact" type="button" data-tile-move="${escapeHtml(id)}:1" ${position === arranged.order.length - 1 ? "disabled" : ""} aria-label="Move ${escapeHtml(name)} down">Move down</button><button class="button compact ${isHidden ? "primary" : ""}" type="button" data-tile-toggle="${escapeHtml(id)}" aria-label="${escapeHtml(visibilityLabel)}">${visibilityAction}</button></span></article>`;
  };
  const visible = arranged.order.filter((id) => !hidden.has(id));
  const hiddenIds = arranged.order.filter((id) => hidden.has(id));
  const notice = homeTileArrangementNotice ? `<p class="arrange-tile-notice" role="status" aria-live="polite">${escapeHtml(homeTileArrangementNotice)}</p>` : "";
  return `<main class="content-viewport arrange-tiles-screen" aria-labelledby="route-heading"><header class="arrange-tiles-header"><div><p class="eyebrow">Home</p><h1 id="route-heading" tabindex="-1">Arrange tiles</h1><p>Drag a tile or use the arrows to choose its place. Hidden tiles can be restored here whenever you need them.</p></div><div class="arrange-tiles-exits"><button class="button compact" type="button" data-arrange-back>Back to Home</button><button class="button primary" type="button" data-arrange-done>Done</button></div></header>${notice}<section class="arrange-tile-list" aria-labelledby="arrange-visible-heading"><header><div><h2 id="arrange-visible-heading">Shown on Home</h2><p>${visible.length} tile${visible.length === 1 ? "" : "s"} shown</p></div><span class="arrange-drag-note" aria-hidden="true">⠿ Drag to reorder</span></header><div class="arrange-tile-stack" role="list" aria-label="Tiles shown on Home">${visible.map((id) => tile(id, false)).join("")}</div></section>${hiddenIds.length ? `<section class="arrange-tile-list arrange-hidden-list" aria-labelledby="arrange-hidden-heading"><header><div><h2 id="arrange-hidden-heading">Hidden tiles</h2><p>Restoring a tile keeps its saved position.</p></div></header><div class="arrange-tile-stack" role="list" aria-label="Hidden tiles">${hiddenIds.map((id) => tile(id, true)).join("")}</div></section>` : ""}</main>`;
}

function workspaceContent(): string {
  syncHomeTilePreferencesForActiveProfile();
  if (route === "mullvad") return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="Your existing Mullvad window is open inside OSL"><span class="sr-only">Mullvad remains a separate foreign application. OSL does not read its account or VPN state.</span></main>`;
  if (route === "inbox") return inboxDestinationContent();
  if (route === "people") return peopleDestinationContent();
  if (route === "privacy") return privacyDestinationContent();
  if (route === "scrub") return scrubDestinationContent();
  if (route === "activity") return `${autoScrubActivityRecordMarkup(autoScrubOpenedActivityRecord)}${activityDestinationContent()}`;
  if (route === "connections") return connectionsDestinationContent();
  if (route === "osl-chat") return oslChatContent();
  if (route === "osl-mail") return oslMailContent();
  if (route === "osl-mail-status") return outOfReleaseStatusContent("OSL Mail");
  if (route === "osl-notes-status") return outOfReleaseStatusContent("OSL Notes");
  if (route === "osl-servers") return oslServersContent();
  if (route === "settings") return settingsContent();
  if (route === "service" && activeService) return serviceContent();
  if (route === "arrange-tiles") return arrangeTilesContent();
  const launchableHomeApps = homeAppsFromServices(services).filter((app) => app.visibility === "launch");
  const roadmapHomeApps = launchableHomeApps.filter((app) => app.launchState !== "available");
  const rememberedHomeApps = new Set<HomeAppId>(hasExplicitOnboardingAppSelection
    ? selectedOnboardingApps
    : [
        ...selectedOnboardingApps,
        ...launchableHomeApps.filter((app) => app.linked || savedNativeApps.has(app.id as NativeAppId)).map((app) => app.id),
      ]);
  const selectedHomeApps = hasExplicitOnboardingAppSelection || rememberedHomeApps.size
    ? launchableHomeApps.filter((app) => app.launchState === "available" && rememberedHomeApps.has(app.id))
    : launchableHomeApps.filter((app) => app.launchState === "available");
  const homeApps = [...selectedHomeApps, ...roadmapHomeApps.filter((app) => !selectedHomeApps.some((selected) => selected.id === app.id))];
  const modules = ([
    { id: "osl-chats", name: "OSL Chats", available: true, capabilityFacts: { placing: true, reading: true, opening: true, realTwoPersonProtectedMessaging: true } },
    { id: "osl-mail", name: "OSL Mail", available: false, capabilityFacts: { placing: false, reading: false, opening: false, realTwoPersonProtectedMessaging: false } },
    { id: "osl-notes", name: "OSL Notes", available: false, capabilityFacts: { placing: false, reading: false, opening: false, realTwoPersonProtectedMessaging: false } },
    { id: "scrub", name: "Scrub", available: true, capabilityFacts: { placing: false, reading: false, opening: true, realTwoPersonProtectedMessaging: false } },
  ] as const).map((module) => ({ ...module, generatedLabel: generatedCapabilityLabel(module.capabilityFacts) }));
  const byId = new Map(homeApps.map((app) => [app.id, app]));
  const moduleById = new Map(modules.map((module) => [module.id, module]));
  const defaultIds = [...homeApps.map((app) => app.id), ...modules.map((module) => module.id)];
  const arranged = normalizeHomeTileArrangement(defaultIds, {
    order: homeTileOrder,
    hidden: [...hiddenHomeTiles],
  });
  const orderedIds = arranged.order;
  const arrangedHidden = new Set(arranged.hidden);
  const renderHomeTile = (id: string, index: number): string => {
    const hidden = arrangedHidden.has(id);
    if (hidden && !homeEditMode) return "";
    const controls = homeEditMode ? `<span class="tile-edit-controls"><button class="tile-remove" type="button" data-tile-toggle="${escapeHtml(id)}" aria-label="${hidden ? "Show" : "Remove"} ${escapeHtml(id)}">${hidden ? "+" : "−"}</button><span class="tile-keyboard-controls"><button type="button" data-tile-move="${escapeHtml(id)}:-1" ${index === 0 ? "disabled" : ""} aria-label="Move before">←</button><button type="button" data-tile-move="${escapeHtml(id)}:1" ${index === orderedIds.length - 1 ? "disabled" : ""} aria-label="Move after">→</button></span></span>` : "";
    const module = moduleById.get(id as typeof modules[number]["id"]);
    if (module) {
      const controlLabel = module.id === "osl-chats" ? "Messages" : module.name;
      return `<article class="app-tile home-module ${module.available ? "" : "module-unavailable"} ${hidden ? "tile-hidden" : ""}" data-tile-id="${module.id}" draggable="${homeEditMode}" data-module-kind="${module.id}"><button class="in-dom-tooltip-anchor" type="button" data-home-module="${module.id}" ${module.available ? "" : "disabled"} aria-label="${escapeHtml(`${controlLabel}, ${module.generatedLabel}`)}"><span class="app-logo-plate osl-module-logo" aria-hidden="true">${homeModuleIcon(module.id)}</span><span class="app-tile-copy"><strong>${controlLabel}</strong><small data-generated-capability-label>${module.generatedLabel}</small></span>${inDomTooltipMarkup(`${controlLabel} · ${module.generatedLabel}`)}</button>${controls}</article>`;
    }
    const app = byId.get(id as HomeAppId);
    if (!app) return "";
    const pending = appLaunchPendingId === app.id;
    const available = app.launchState === "available";
    const disabled = !available || Boolean(appLaunchPendingId);
    // The tile's caption is the app's CLAIM, not a single hardcoded word.
    // "Coming soon" on every unlaunchable tile collapsed four different states
    // into one sentence, and said "planned" about surfaces OSL has already built
    // and driven (D-206) or measured and had refused (D-234). Where the backend
    // has a current claim for this app, that claim is what the tile says. A
    // catalog-only future value is rendered as today's factual unavailability.
    const claim = nativeApps.find((candidate) => candidate.id === app.id as NativeAppId);
    const caption = nativeAppTileLabel(claim?.supportStatus ?? null);
    const unavailableReason = app.unavailableReason ?? claim?.claimNote ?? "This service is unavailable.";
    const unavailableTitle = available ? "" : ` title="${escapeHtml(unavailableReason)}"`;
    const shownLabel = claim ? caption : app.generatedLabel;
    return `<article class="app-tile ${available ? "" : "app-unavailable"} ${hidden ? "tile-hidden" : ""} ${pending ? "pending" : ""}" data-tile-id="${app.id}" draggable="${homeEditMode}" data-service-kind="${app.serviceId ?? "none"}" data-launch-state="${app.launchState}" data-generated-capability="${escapeHtml(app.generatedLabel)}" data-claim-status="${claim ? claim.supportStatus : "unavailable"}" aria-disabled="${available ? "false" : "true"}"><button id="home-app-${app.id}" type="button" ${available ? `data-home-app="${app.id}"` : ""} aria-label="${escapeHtml(`${app.displayName}, ${pending ? "Opening" : shownLabel}`)}"${unavailableTitle} ${disabled ? "disabled" : ""}><span class="app-logo-plate">${homeAppLogo(app)}</span><span class="app-tile-copy"><strong>${escapeHtml(app.displayName)}</strong><small data-generated-capability-label>${escapeHtml(pending ? "Opening" : shownLabel)}</small></span></button>${controls}</article>`;
  };
  const socialIds = new Set(homeApps.filter((app) => app.provider === null).map((app) => app.id));
  const emailIds = new Set(homeApps.filter((app) => app.provider !== null).map((app) => app.id));
  const socialTiles = orderedIds.filter((id) => socialIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const emailTiles = orderedIds.filter((id) => emailIds.has(id as HomeAppId)).map(renderHomeTile).join("");
  const oslTiles = orderedIds.filter((id) => moduleById.has(id as typeof modules[number]["id"])).map(renderHomeTile).join("");
  const oslSection = oslTiles ? `<section class="home-app-section home-osl-section"><div class="app-grid" aria-label="OSL tools">${oslTiles}</div></section>` : "";
  // LAUNCHER BODY (rebuild per DECISIONS.txt "UI WORKSTREAMS" item 1): four
  // equal bordered OSL tiles, SOCIAL and EMAIL rows of circular brand tiles,
  // one tune icon for edit mode, Friends panel down the right. The old
  // protection-summary rows moved to the header/bell popover (see
  // homeStatusSnapshot); the fixed profile dock is gone — the design has no
  // profile control on Home.
  const tuneButton = `<button class="home-tune" type="button" data-edit-home aria-pressed="${homeEditMode}" aria-label="${homeEditMode ? "Finish arranging tiles" : "Arrange tiles"}"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h9M13 4v6M17 7h3M4 17h3M7 14v6M11 17h9"/></svg></button>`;
  const friendsReopen = homeFriendsPanelCollapsed
    ? `<button class="home-friends-reopen" type="button" data-open-friends aria-label="Show the Friends panel">${homeCommandIcon("friends")}</button>`
    : "";
  return `<main id="home-navigation" class="content-viewport home-dashboard home-launcher ${homeEditMode ? "editing" : ""}"><section class="home-primary"><div class="home-launcher-tools">${tuneButton}${friendsReopen}</div><div class="home-launcher-scroll"><div class="home-launcher-column">${oslSection}${socialTiles ? `<section class="home-app-section"><h2>Social</h2><div class="app-grid" aria-label="Social apps">${socialTiles}</div></section>` : ""}${emailTiles ? `<section class="home-app-section"><h2>Email</h2><div class="app-grid" aria-label="Email apps">${emailTiles}</div></section>` : ""}</div></div></section>${homeFriendsPanelCollapsed ? "" : homeFriendsPanelMarkup()}</main>`;
}

function parsedEnclaveAudiences(records: unknown[]): EnclaveAudience[] {
  return records
    .map((record) => parseEnclaveAudience(record))
    .filter((audience): audience is EnclaveAudience => audience !== null);
}

let privateEnclaveAudiences: EnclaveAudience[] = [];

function enclaveAudienceMembershipDetail(audience: EnclaveAudience): string {
  if (audience.membershipVisibility === "visible") {
    const names = audience.visibleMembers.map((member) => `${member.name}${member.verified ? " verified" : " needs review"}`).join(", ");
    return names ? `${audience.memberCount.toLocaleString("en-US")} people: ${names}` : `${audience.memberCount.toLocaleString("en-US")} people. Members are shown before posting.`;
  }
  if (audience.membershipVisibility === "count-only") return `${audience.memberCount.toLocaleString("en-US")} people. Names are shown during the final audience review before posting.`;
  return "Membership is hidden here. Posting stays refused until the audience is shown for review.";
}

function enclaveAudienceStatus(audience: EnclaveAudience): { label: "Ready" | "Refused"; detail: string } {
  if (audience.canPost) return { label: "Ready", detail: "Posts and comments are encrypted for the selected audience." };
  if (audience.refusal === "consent") return { label: "Refused", detail: "Review and approve this audience on this device before posting." };
  if (audience.refusal === "binding") return { label: "Refused", detail: "Choose the Enclave for this audience before posting." };
  return { label: "Refused", detail: "This account is not allowed to post to that audience." };
}

function enclavesDestinationContent(): string {
  const enclaveSurface = firstPartyOslSurfaceContract("osl-enclaves");
  if (enclaveSurface.state !== "available") {
    return `<section class="inbox-surface-card enclaves-destination unavailable" data-inbox-osl-surface="enclaves" data-enclave-state="${enclaveSurface.state}" aria-disabled="true"><strong>${escapeHtml(enclaveSurface.label)}</strong><small>Private audience feeds</small><p>${statusTag("Coming later")} ${escapeHtml(enclaveSurface.label)} is coming after small-group review. Private audience posts are unavailable.</p>${publicEnclavesUnavailableMarkup()}</section>`;
  }
  const audienceCards = privateEnclaveAudiences.map((audience) => {
    const status = enclaveAudienceStatus(audience);
    return `<article class="setting-line enclave-audience-card ${audience.canPost ? "" : "unavailable"}" data-enclave-audience="${escapeHtml(audience.audienceId)}" data-enclave-posting="${audience.canPost ? "ready" : "refused"}" data-enclave-refusal="${audience.refusal ?? "none"}" aria-disabled="${audience.canPost ? "false" : "true"}"><span><strong>${escapeHtml(audience.name)}</strong><small>${escapeHtml(enclaveAudienceMembershipDetail(audience))}</small></span>${statusTag(status.label)}<p>${escapeHtml(status.detail)}</p></article>`;
  }).join("");
  const feedItems = privateEnclaveAudiences.filter((audience) => audience.canPost).map((audience, index) => `<article class="inbox-row enclave-feed-item" data-enclave-feed-item="${index}" data-enclave-feed-order="chronological" data-enclave-audience="${escapeHtml(audience.audienceId)}"><span class="source-mark">${homeModuleIcon("osl-chats")}</span><div><strong>${escapeHtml(audience.name)}</strong><small>Chronological private feed · ${audience.memberCount.toLocaleString("en-US")} people · no ranking or behavioral advertising</small></div>${statusTag("Encrypted")}</article>`).join("");
  const audienceList = audienceCards
    ? `<div class="settings-list enclave-audience-list" aria-label="Private Enclave audiences">${audienceCards}</div><div class="enclave-feed-list" aria-label="Chronological private Enclave feeds">${feedItems}</div>`
    : `<div class="empty-state" data-enclave-audiences="none"><strong>No Enclave audiences yet</strong><p>An audience appears here after you create one on this device. Until then there is nobody to post to.</p></div>`;
  return `<section class="inbox-surface-card enclaves-destination" data-inbox-osl-surface="enclaves" data-enclave-state="${enclaveSurface.state}" data-enclave-feeds="private-audiences" data-enclave-audience-count="${privateEnclaveAudiences.length}"><strong>OSL Enclaves</strong><small>Private audience feeds</small><p>${statusTag("Private")} Posts and comments are encrypted for the selected audience. Audience membership is shown before posting.</p>${audienceList}${publicEnclavesUnavailableMarkup()}</section>`;
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
    return `<article class="inbox-surface-card unavailable" data-inbox-osl-surface="mail" data-osl-mail-stage-a="unavailable" data-osl-mail-protection="private-client" data-osl-mailbox-stage-c-gate="${mailboxGate.reason ?? "reviewed"}" data-mailbox-operations="refused" aria-disabled="true"><strong>OSL Mail</strong><small>Private client protection</small><p>${statusTag("Coming later")} OSL Mail client protection is unavailable until its desktop bridge exists.</p></article>`;
  }
  const capabilities = [
    "Connect an existing mailbox only after authorization",
    "Warn before send and label the protection scope",
    "Sanitize selected links and attachments",
    "Organize retention on this device",
  ];
  return `<article class="inbox-surface-card" data-inbox-osl-surface="mail" data-osl-mail-stage-a="available" data-osl-mail-protection="private-client" data-osl-mailbox-stage-c-gate="${mailboxGate.reason ?? "reviewed"}" data-mailbox-operations="${mailboxGate.operationsAllowed ? "allowed" : "refused"}" aria-disabled="false"><strong>OSL Mail</strong><small>Private client protection</small><p>${statusTag("Available")} Protect mailboxes you already control after explicit authorization.</p><ul>${capabilities.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><p>${statusTag(mailboxGate.label)} ${escapeHtml(mailboxGate.detail)} External email remains ordinary email unless a supported encrypted path is selected before send.</p></article>`;
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
  return `<article class="inbox-surface-card mail-stage-card" data-inbox-osl-surface="mail" data-osl-mail-stage-b="${state}" data-osl-mail-stage-b-after="client-protection" data-osl-mail-aliases="${ready ? "available" : "refused"}" data-osl-mail-relay="${ready ? "available" : "refused"}" aria-disabled="${ready ? "false" : "true"}"><strong>Aliases and relay</strong><small>After client protection</small><p>${statusTag(label)} ${escapeHtml(detail)}</p><ul>${capabilities.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul><p>External email remains ordinary email unless a supported encrypted path is selected before send.</p></article>`;
}

function publicEnclavesUnavailableMarkup(): string {
  return `<article class="inbox-surface-card unavailable" data-inbox-osl-surface="enclaves" data-public-enclaves-network="unavailable" aria-disabled="true"><strong>OSL Enclaves</strong><small>Private audience feeds</small><p>${statusTag("Unavailable")} Public Enclaves network unavailable. Private audience posts stay off until membership, posting, and moderation are complete.</p></article>`;
}

export function publicPostGuardCarrierPreviewMarkup(platform = "Public platforms"): string {
  const platformName = escapeHtml(platform);
  return `<section class="public-post-guard public-post-guard-preview" data-public-platform-preview="encrypted-audience-carrier" data-public-post-guard="encrypted-audience-carrier" aria-labelledby="public-post-guard-title"><header><span class="privacy-local-mark">PUBLIC POST GUARD</span><h2 id="public-post-guard-title">Encrypted-audience carrier preview</h2><p>${platformName}: still a public surface. OSL shows the public carrier text separately from the protected audience preview before anything is placed.</p></header><div class="privacy-policy-grid carrier-preview-grid" aria-label="Public platform carrier preview"><article class="privacy-policy-card" data-public-post-kind="ordinary" data-carrier-part="public">${statusTag("Public")}<h3>Public carrier</h3><p>Visible to the platform audience. Search, quoting, archiving, audience, location, and media metadata still need review. Visible carrier text stays visible and does not contain the protected message.</p></article><article class="privacy-policy-card" data-public-post-kind="encrypted-audience-carrier" data-carrier-part="protected-audience">${statusTag("Carrier preview")}<h3>Protected audience</h3><p>Plaintext is for the approved audience only, but the platform can still see the public carrier, timing, and engagement.</p></article></div><p class="scope-approval-note">If audience proof is missing or changes, OSL refuses the protected placement and keeps the draft local.</p></section>`;
}

export function inboxDestinationContent(): string {
  const verifiedPeople = hubPeople.filter(peerIsVerified);
  const requests = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange);
  const connectedApps = homeAppsFromServices(services)
    .filter((app) => app.visibility === "launch" && app.launchState === "available" && app.linked);
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
    ["enclaves", "OSL Enclaves", "Private audience feeds", "Create an enclave to begin"],
    ["mail", "OSL Mail", "Client protection", "External recipients are not OSL E2EE"],
  ] as const;
  const mailboxGate = oslMailboxStageCGate();
  const surfaceCards = oslSurfaces.map(([id, label, protection, detail]) => {
    if (id === "enclaves") return enclavesDestinationContent();
    if (id === "mail") {
      return `${oslMailStageAContent(oslMailStage("stageA"), mailboxGate)}${oslMailStageBContent(oslMailStage("stageB"))}`;
    }
    return `<article class="inbox-surface-card" data-inbox-osl-surface="${id}"><strong>${label}</strong><small>${protection}</small><p>${detail}</p></article>`;
  }).join("");
  const filterTabs = ([
    ["all", "All"],
    ["osl", "OSL"],
    ["connected", "Connected"],
    ["requests", "Requests"],
  ] as const).map(([filter, label]) => `<button type="button" data-inbox-filter="${filter}" aria-pressed="${inboxFilter === filter}">${label}</button>`).join("");
  const visiblePanels = [
    inboxFilter === "all" || inboxFilter === "osl"
      ? `<section class="inbox-panel" aria-labelledby="inbox-osl-heading"><h2 id="inbox-osl-heading">OSL</h2><div class="inbox-surface-grid">${surfaceCards}</div>${chatRows}</section>`
      : "",
    inboxFilter === "all" || inboxFilter === "connected"
      ? `<section class="inbox-panel" aria-labelledby="inbox-connected-heading"><h2 id="inbox-connected-heading">Connected</h2>${connectedRows}</section>`
      : "",
    inboxFilter === "all" || inboxFilter === "requests"
      ? `<section class="inbox-panel" aria-labelledby="inbox-requests-heading"><h2 id="inbox-requests-heading">Requests</h2>${requestRows}</section>`
      : "",
  ].join("");
  // `id="route-heading"` sat on this <main>, not on its heading, so the landmark
  // had no accessible name and the post-navigation focus move (see the
  // `#route-heading` focus call in the render path) landed on an unnamed region:
  // a screen reader announced nothing at all on arriving at Inbox. Every other
  // destination puts the id on its own <h1> and names the landmark with
  // aria-labelledby; Inbox now matches.
  return `<main class="content-viewport inbox-destination" aria-labelledby="route-heading"><header class="destination-header"><div><p class="eyebrow">Inbox</p><h1 id="route-heading" tabindex="-1">Conversations</h1><p>Optional views for OSL messages and connected accounts. OSL shows only supported conversations and refuses protected send when the conversation cannot be verified.</p></div><button class="button primary" data-inbox-start-private type="button">Start a private conversation</button></header><nav class="inbox-filter-tabs" aria-label="Inbox filters">${filterTabs}</nav><section class="inbox-grid">${visiblePanels}</section>${publicPostGuardCarrierPreviewMarkup("Public platforms")}</main>`;
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
        return `<article class="notification-event activity-proof-row ${index === 0 && activityAttentionReviewOpen ? "selected" : ""}" data-activity-item="${index}" data-activity-proof="${escapeHtml(item.id)}" ${needsAttention ? 'data-activity-attention="true"' : ""}>${statusTag(index === 0 && activityAttentionReviewOpen ? "Reviewing" : needsAttention ? "Needs review" : "Recorded")}<div><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")} · ${escapeHtml(item.createdAt)}</small></div></article>`;
      }).join("")
    : `<div class="empty-state"><strong>${notificationsEnabled ? "No activity needs attention" : "Activity is off"}</strong><p>${notificationsEnabled ? "Warnings, connection failures, cleanup checks, and verified outcomes appear here after OSL creates them on this device." : "Turn on local activity before OSL records local outcomes here."}</p></div>`;
  const attention = attentionItems.length
    ? attentionItems.slice(0, 5).map((item, index) => `<article class="setting-line" data-attention-review-item="${index}"><span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(notificationPreviewContent ? item.detail : "Private OSL activity")}</small></span>${statusTag("Review")}</article>`).join("")
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
  return `<article class="connection-device-card connection-card mullvad-card" data-connection-card="mullvad" data-connection-kind="mullvad" data-privacy-scope="${status.privacyScope}" data-connection-state="${status.availability}"><div>${statusTag(state)}<strong>Mullvad</strong><small>Network privacy only · ${state}</small><p>Network privacy signal only. Use your existing Mullvad session as a separate network tool. OSL does not read its account state, connection state, or app content. Platforms and recipients can still see ordinary content you send there.</p></div>${action}</article>`;
}

function androidWorkspaceConnectionCard(surface: AndroidSurface): string {
  if (surface.surface === "companion") {
    // The content is wrapped exactly like the sibling workspace card below.
    // Without the wrapper the card's own `justify-content: space-between`
    // treated the badge and the title as the two ends of one row and threw
    // "Android Companion" to the right while every sibling stayed left.
    return `<article class="connection-device-card" data-android-surface="${surface.id}" data-consent="${surface.consent}" data-binding="${surface.binding}"><div>${statusTag("Coming later")}<h3>${escapeHtml(surface.displayName)}</h3><p>Phone approvals and OSL-owned mobile experiences stay separate from desktop account control.</p></div></article>`;
  }
  return `<article class="connection-device-card connection-card android-workspace-card pro unavailable" data-android-surface="${surface.id}" data-android-workspace-consent="${surface.consent}" data-consent="${surface.consent}" data-binding="${surface.binding}" data-hosted-execution="${surface.hostedExecution}" data-workspace-runtime="${surface.workspace?.runtime ?? "localVirtualDevice"}" aria-disabled="true"><div>${statusTag("Coming later · Pro")}<strong>${escapeHtml(surface.displayName)}</strong><small>Future Pro isolation · Coming later</small><p>Future isolated local workspace with encrypted local virtual device storage. A separate mobile workspace threat model review and explicit consent are required before any local workspace starts.</p><ul><li>Encrypted local virtual device storage.</li><li>clipboard, files, notifications, camera, microphone, and location start denied.</li><li>${hostedAndroidWorkspaceGate()}</li><li>No hosted Android workspace runs from this card.</li></ul></div><button class="button compact" disabled>Consent required</button></article>`;
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
        const state = app.linked ? `${app.accountCount} local ${app.accountCount === 1 ? "profile" : "profiles"}` : app.launchState === "available" ? "Not set up" : app.id === "messenger" ? "Cannot send yet" : "Coming later";
        const action = app.launchState === "available"
          ? `<button class="button compact" data-home-app="${app.id}" type="button">${app.linked ? "Open" : "Set up"}</button>`
          : `<button class="button compact" disabled>${app.id === "messenger" ? "Cannot send yet" : "Coming later"}</button>`;
        return `<article class="connection-row connection-account-row" data-connection-app="${app.id}" data-connection-account="${app.id}"><div>${homeAppLogo(app)}<span><strong>${escapeHtml(app.displayName)}</strong><small>${escapeHtml(state)}</small></span></div>${action}</article>`;
      }).join("")
    : `<div class="empty-state"><strong>No account catalog loaded</strong><p>Reconnect when apps are available on this device.</p></div>`;
  // Two separate facts on one row, and they were being confused: whether the
  // app is INSTALLED on this device, and whether OSL claims anything about
  // protecting it. Detecting an app is not a support claim; the claim comes from
  // `claim_state` and ships with the sentence that justifies it.
  const nativeRows = nativeApps.length
    ? `${nativeApps.map((app) => `<article class="setting-line native-claim-line" data-device-connection="${app.id}"><span><strong>${escapeHtml(app.displayName)}</strong><small>${app.availability === "installed" ? "Installed native app" : app.availability === "installable" ? "Can be installed" : "Unavailable on this device"}</small>${nativeClaimMarkup(app)}</span>${statusTag(app.availability === "installed" ? "Ready" : app.availability === "installable" ? "Installable" : "Unavailable")}</article>`).join("")}${carrierReceiptCensusMarkup(nativeApps)}`
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
      previewVisible: chatPreviewHidingVisible(oslChatPreviewsVisible),
      unreadCount: oslChatUnread.get(person.personId) ?? 0,
      handshakeConfirmed: oslChatHandshakeConfirmed(messages),
      pendingKeyChange: person.pendingKeyChange,
      online: person.personId === activeOslChatPersonId,
      timeLabel: last?.timestampLabel ?? "",
    };
  });
  const settingsPerson = oslChatSettingsPersonId ? hubPeople.find((person) => person.personId === oslChatSettingsPersonId) ?? null : null;
  const settings = settingsPerson ? oslChatFriendSettingsMarkup(settingsPerson) : "";
  const attachmentCreation = pro
    ? `<button class="button compact" id="osl-chat-attach" type="button" ${oslChatBusy ? "disabled" : ""}>Choose file</button>`
    : `<span class="quiet-note">Pro is required to make an attachment.</span>`;
  const attachments = activeOslChatContext?.scopeApproved
    ? `<section class="osl-chat-attachments" aria-label="Encrypted attachments"><header><strong>Attachments</strong>${attachmentCreation}</header>${pro ? [...attachmentProgressByContext.values()].map(attachmentProgressMarkup).join("") : ""}${oslChatAttachments.length ? oslChatAttachments.map((item) => `<button class="setting-line" data-osl-chat-attachment="${escapeHtml(item.attachmentId)}" type="button"><span><strong>${escapeHtml(item.originalFilename)}</strong><small>${item.viewOnce ? "View once · " : ""}${item.plaintextSize.toLocaleString("en-US")} bytes</small></span>${statusTag("Open")}</button>`).join("") : `<p>No pending attachments.</p>`}<small>Opening a received view-once item is free. Images open in OSL's capture-resistant viewer. Other supported files open temporarily in their Windows viewer, which may allow capture.</small></section>`
    : "";
  const droppedFiles = oslChatDropTray.attachments.length
    ? attachmentTrayMarkup(oslChatDropTray)
    : "";
  const receipt = activeOslChatPersonId
    ? oslChatSenderReceiptMarkup(oslChatMessages.get(activeOslChatPersonId) ?? [])
    : "";
  const offlineStatus = oslRelayConnectionState() === "offline" ? offlineCapabilitiesMarkup() : "";
  const startSheet = startSomethingChoice !== null
    ? startSomethingSheetMarkup(startSomethingChoice, friendCode ?? "", startSomethingPeople(), startSomethingJoiningRule)
    : "";
  return `<main class="osl-chat-page" aria-label="OSL Chats">${oslChatsViewMarkup({
    friends,
    activePersonId: activeOslChatPersonId,
    messages: activeOslChatPersonId ? oslChatMessages.get(activeOslChatPersonId) ?? [] : [],
    draft: oslChatDraft,
    busy: oslChatBusy,
    viewOnce: oslChatViewOnce,
    viewOnceCreationAllowed: pro,
    profileDisplayName: claimedOslUsername || "OSL profile",
    conversationFilter: oslChatFilter,
    searchQuery: oslChatSearch,
    sendBlockedReason: oslChatSendBlockedReason,
    attachmentAvailable: Boolean(activeOslChatContext?.scopeApproved && pro),
    deletionUnconfirmed: oslChatDeletionUnconfirmed,
    buildIntegrity: buildIntegrityStatus,
    verificationWarningSurface: oslChatVerificationWarningSurface,
    buildWarning: installedBuildChatWarning,
  })}${offlineStatus}${receipt}${droppedFiles}${attachments}${settings}${startSheet}${chatSurfaceOverlays()}</main>`;
}

/** Hosts the already-built settings surfaces; their markup stays owned by each surface module. */
function chatSurfaceOverlays(): string {
  const profile = chatProfileAppearanceOpen ? '<div id="chat-profile-appearance-host"></div>' : "";
  const person = safetyNumberPanelPersonId ? hubPeople.find((candidate) => candidate.personId === safetyNumberPanelPersonId) ?? null : null;
  const safety = person
    ? `<dialog class="owned-confirmation-dialog" id="safety-number-dialog"><section class="owned-confirmation-card"><header><h2>Safety number</h2><button class="icon-button" data-close-safety-number type="button" aria-label="Close safety number">×</button></header><div id="safety-number-panel-host"></div></section></dialog>`
    : "";
  return `${profile}${safety}`;
}

function openSafetyNumberPanel(personId: string): void {
  const person = hubPeople.find((candidate) => candidate.personId === personId);
  if (!person) return;
  safetyNumberPanelPersonId = personId;
  render();
}

function mountChatSurfaceOverlays(): void {
  const profileHost = document.querySelector<HTMLElement>("#chat-profile-appearance-host");
  if (profileHost) {
    attachChatProfileAppearanceModal(profileHost, chatProfileAppearanceState, {
      onClose: () => { chatProfileAppearanceOpen = false; render(); },
    });
    profileHost.addEventListener("click", (event) => {
      const item = (event.target as Element | null)?.closest<HTMLElement>("[data-chat-appearance-item]")?.dataset.chatAppearanceItem;
      const editor = profileHost.querySelector<HTMLElement>(".chat-profile-editor");
      if (!item || !editor) return;
      if (item === "Chat background") {
        chatAppearancePane = "background";
        mountChatBackgroundPane(editor, activeOslChatPersonId ?? "osl-chats");
      } else if (item === "Messages") {
        chatAppearancePane = "messages";
        renderChatMessagesPane(editor);
      }
    });
    if (chatAppearancePane === "background") {
      const editor = profileHost.querySelector<HTMLElement>(".chat-profile-editor");
      if (editor) mountChatBackgroundPane(editor, activeOslChatPersonId ?? "osl-chats");
    } else if (chatAppearancePane === "messages") {
      const editor = profileHost.querySelector<HTMLElement>(".chat-profile-editor");
      if (editor) renderChatMessagesPane(editor);
    }
  }
  const safetyHost = document.querySelector<HTMLElement>("#safety-number-panel-host");
  const safetyDialog = document.querySelector<HTMLDialogElement>("#safety-number-dialog");
  const person = safetyNumberPanelPersonId ? hubPeople.find((candidate) => candidate.personId === safetyNumberPanelPersonId) ?? null : null;
  if (safetyHost && safetyDialog && person) {
    safetyHost.innerHTML = safetyNumberPanelMarkup({ id: person.personId, name: person.alias ?? "Verified friend", safetyNumber: person.safetyNumber, verified: person.safetyNumberVerified });
    if (!safetyDialog.open) safetyDialog.showModal();
    bindSafetyNumberPanel(safetyHost, { id: person.personId, name: person.alias ?? "Verified friend", safetyNumber: person.safetyNumber, verified: person.safetyNumberVerified });
  }
}

/** Browser offline is a reliable negative signal; any other state stays unknown. */
function oslRelayConnectionState(): OslConnectionState {
  return typeof navigator !== "undefined" && navigator.onLine === false ? "offline" : "unknown";
}

function refuseOfflineCapability(capability: OfflineUnavailableCapability): boolean {
  if (oslRelayConnectionState() !== "offline") return false;
  showToast(offlineCapabilityStatus(capability, "offline").detail);
  return true;
}

function bindAttachmentProgressEvents(): void {
  void listen<unknown>("osl://attachment-progress", (event) => {
    const progress = parseAttachmentProgressEvent(event.payload);
    if (!progress) return;
    attachmentProgressByContext.set(progress.contextId, progress);
    if (route === "osl-chat") renderWhenIdle();
  });
}

/**
 * D-135. How many remote attachment copies OSL asked the relay to delete and
 * could not confirm gone -- `DeletionDrainReport::retained`, reported by
 * `native_attachment_transport::report_deletion_drain`.
 *
 * This is the listener that must exist for that emit to be worth having: the
 * previous `osl://attachment-deletion-drain` advisory was deleted precisely
 * because it went to `emit_to("main", ...)` and no webview in the repo ever
 * subscribed. This page IS the `"main"` webview, so the pair is complete.
 *
 * 0 means "nothing OSL knows to be owed". It never means "confirmed gone" --
 * nothing reads back the relay -- which is why the composer copy states what
 * OSL asks for rather than what it achieved.
 */
let oslChatDeletionUnconfirmed = 0;

/** Reject malformed native events rather than rendering data from another boundary. */
export function parseAttachmentDeletionUnconfirmedEvent(value: unknown): number | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value as Record<string, unknown>);
  if (keys.length !== 1 || keys[0] !== "retained") return null;
  const retained = (value as { retained: unknown }).retained;
  if (typeof retained !== "number" || !Number.isSafeInteger(retained) || retained < 0 || retained > 100_000) return null;
  return retained;
}

function bindAttachmentDeletionEvents(): void {
  void listen<unknown>("osl://attachment-deletion-unconfirmed", (event) => {
    const retained = parseAttachmentDeletionUnconfirmedEvent(event.payload);
    // A malformed payload leaves the last known count standing. Silently
    // resetting it to 0 would turn a decode fault into "nothing is owed",
    // which is the exact claim this surface exists to stop OSL from making.
    if (retained === null) return;
    oslChatDeletionUnconfirmed = retained;
    if (route === "osl-chat") renderWhenIdle();
  });
}

/**
 * The sender sees only a receipt the peer app actually reported.
 *
 * D-136: which states can be reported at all, and why the two that cannot are
 * no longer branched on, is derived and cited in `senderReceiptStateFor`
 * (osl-chats-view.ts). `null` from it means "no receipt", which
 * `senderReceiptStatus` deliberately renders in the same words as every
 * pre-receipt state so that receipt opt-out stays indistinguishable from
 * not-yet-arrived.
 */
export function oslChatSenderReceiptMarkup(messages: readonly OslChatMessage[]): string {
  const receipt = senderReceiptStatus(senderReceiptStateFor(messages));
  return `<p class="setting-line osl-chat-receipt-status" data-osl-chat-receipt-confirmed="${receipt.confirmed}"><span><strong>Delivery receipt</strong><small>${receipt.label}</small></span></p>`;
}

function oslChatFriendSettingsMarkup(person: HubPerson): string {
  const isActive = activeOslChatPersonId === person.personId;
  const approved = isActive && activeOslChatContext?.scopeApproved === true;
  const verified = peerIsVerified(person);
  const notificationSettings = readOslChatNotificationSettings(localStorage, person.personId);
  const permissionDetail = !verified
    ? "Not verified. Verify the new safety number before changing this whitelist."
    : approved
      ? "This friend may exchange encrypted OSL messages with you."
      : "Open this friend to configure its exact chat permission.";
  const permissionControl = isActive
    ? `<button class="button compact ${approved ? "danger" : "primary"}" id="osl-chat-permission-toggle" type="button" ${oslChatBusy || !verified ? 'disabled aria-disabled="true"' : ""}>${approved ? "Revoke" : "Enable"}</button>`
    : `<button class="button compact" data-osl-chat-open="${escapeHtml(person.personId)}" type="button" ${verified ? "" : 'disabled aria-disabled="true"'}>Open chat</button>`;
  return `<dialog class="friends-dialog osl-chat-settings-dialog" id="osl-chat-settings-dialog" aria-labelledby="osl-chat-settings-title"><div class="friends-dialog-card"><header><div><span>Encrypted chat</span><h2 id="osl-chat-settings-title">${escapeHtml(person.alias ?? "Verified friend")}</h2></div><button class="icon-button" id="osl-chat-settings-close" type="button" aria-label="Close chat settings">×</button></header><div class="settings-list">${peerIntegrityMarkup("unknown")}<button class="setting-line interactive" data-open-safety-number="${escapeHtml(person.personId)}" type="button"><span><strong>Safety number</strong><small>Compare this number through a channel you already trust.</small></span></button>${oslChatNotificationSettingsMarkup(notificationSettings)}<div class="setting-line osl-chat-permission-row${verified ? "" : " is-not-verified"}" data-osl-chat-whitelist-state="${verified ? "available" : "not-verified"}" ${verified ? "" : 'aria-disabled="true"'}><span><strong>Chat permission</strong><small>${permissionDetail}</small></span>${permissionControl}</div></div></div></dialog>`;
}

function oslServersContent(): string {
  return oslServersViewMarkup((label) => statusTag(label), ownerRoleEditorMarkup(ownerRoleEditor.snapshot()));
}

/** The feature blueprints remain in the build, but these Home links must not imply a release commitment. */
function outOfReleaseStatusContent(name: "OSL Mail" | "OSL Notes"): string {
  return `<main class="content-viewport unavailable-status-page" aria-labelledby="route-heading" data-release-status="not-in-this-release"><section class="native-app-card unavailable-status-card"><h1 id="route-heading" tabindex="-1">${name}</h1><p>${name} is not in this release and is not being built for it.</p><button class="button" data-route="home" type="button">Back to Home</button></section></main>`;
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
    threadSyncUnavailable: oslMailThreadSyncUnavailable,
    composeDraft: oslMailComposeDraft,
    attachmentTrayMarkup: oslMailDropTray.getCards().length ? attachmentTrayScreenMarkup(oslMailDropTray.getCards()) : "",
  });
}

async function refreshOslMail(): Promise<void> {
  oslMailLoading = true;
  oslMailError = null;
  oslMailThreadSyncUnavailable = false;
  renderWhenIdle();
  const status = await loadOslMailStatus();
  oslMailStatus = status;
  oslMailThreads = [];
  oslMailActiveThread = null;
  if (status?.provisioned) {
    const threads = await listOslMailThreads();
    if (threads) {
      oslMailThreads = threads;
    } else {
      oslMailThreadSyncUnavailable = true;
      oslMailError = "Inbox sync was refused; messages are not being reported as empty";
    }
  }
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
  oslMailThreads = [];
  oslMailActiveThread = null;
  oslMailThreadSyncUnavailable = false;
  if (!oslMailStatus) oslMailError = "Mailbox setup was refused";
  else if (oslMailStatus.provisioned) await refreshOslMail();
  if (route === "osl-mail") render();
}

async function sendOslMailForm(form: HTMLFormElement, choice: OslMailSendChoice): Promise<void> {
  escapeAuditSendAttempts += 1;
  const recipient = form.querySelector<HTMLInputElement>("#osl-mail-to")?.value ?? "";
  const subject = form.querySelector<HTMLInputElement>("#osl-mail-subject")?.value ?? "";
  const body = form.querySelector<HTMLTextAreaElement>("#osl-mail-body")?.value ?? "";
  oslMailComposeDraft = { to: recipient, subject, body };
  const result = await sendOslMailWithChoice(choice, recipient, subject, body);
  oslMailSendReceipt = result.outcome === "sent" ? result.receipt : null;
  oslMailError = result.outcome === "sent" ? null : result.reason;
  if (result.outcome === "sent") oslMailComposeDraft = { to: "", subject: "", body: "" };
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
          : `${statusTag("Open a supported chat first")}`
        : `${statusTag("Verified")}`
      : `<button class="button compact" data-verify-person="${escapeHtml(person.personId)}">${person.pendingKeyChange ? "Re-verify key" : "Verify"}</button>`;
    if (mode === "home") {
      const lastMessage = oslChatMessages.get(person.personId)?.at(-1);
      const chatState = person.safetyNumberVerified && !person.pendingKeyChange
        ? (lastMessage?.body ?? "Open encrypted chat")
        : friendHandshakeSummary(person.safetyNumberVerified, person.pendingKeyChange);
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
    const whitelistControls = mode === "manage" ? friendWideWhitelistButtonsMarkup(person.personId, escapeHtml) : "";
    const futureAccountSwitch = mode === "manage"
      ? futureAccountSwitchMarkup({
        personId: person.personId,
        enabled: friendFutureAccountAutoWhitelist.get(person.personId) ?? false,
        busy: friendFutureAccountAutoWhitelistBusy.has(person.personId),
      })
      : "";
    const management = `<details class="friend-management"><summary>Manage</summary><div>${nicknameForm}<div class="friend-approvals"><span>Approved chats</span><div>${scopes}</div>${truncated}</div>${futureAccountSwitch}<details class="friend-security"><summary>Security details</summary><div><span>OSL ID</span><code>${escapeHtml(identity)}</code><span>Verification code</span><code>${escapeHtml(person.safetyNumber)}</code></div></details>${whitelistControls}${removeControl}</div></details>`;
    return `<article class="person-row person-profile"><header><div><strong>${escapeHtml(nickname)}</strong><small>${escapeHtml(friendHandshakeSummary(person.safetyNumberVerified, person.pendingKeyChange))}</small></div>${action}</header>${management}</article>`;
  }).join("");
}

function peopleDestinationContent(): string {
  const verified = hubPeople.filter(peerIsVerified);
  const needsReview = hubPeople.filter((person) => !person.safetyNumberVerified || person.pendingKeyChange);
  const reVerificationNotice = peopleReverificationNoticeMarkup(hubPeople.some((person) => !person.safetyNumberVerified && !person.pendingKeyChange));
  const approvedChats = verified.reduce((total, person) => total + person.whitelistCount, 0);
  const broaderReach = verified.filter((person) => person.reachBroadened).length;
  const addPrimaryTarget = peoplePrimaryActionFocus === "add" ? ' data-people-primary-target="add"' : "";
  const reviewPrimaryTarget = peoplePrimaryActionFocus === "verify" ? ' data-people-primary-target="verify"' : "";
  const reviewRows = needsReview.length
    ? needsReview.slice(0, 4).map((person) => {
      const nickname = person.alias ?? "Unnamed friend";
      const detail = friendHandshakeDetail(person.safetyNumberVerified, person.pendingKeyChange);
      return `<article class="people-review-row"><div><strong>${escapeHtml(nickname)}</strong><small>${detail}</small></div><button class="button compact" type="button" data-verify-person="${escapeHtml(person.personId)}">${person.pendingKeyChange ? "Review change" : "Verify"}</button></article>`;
    }).join("")
    : `<div class="empty-state compact"><strong>No people need review</strong><p>New people and changed verification appear here before OSL trusts them.</p></div>`;
  const peopleRows = hubPeople.length
    ? peopleListMarkup("manage")
    : `<div class="empty-state"><strong>No trusted people yet</strong><p>Add someone, compare verification another way, then approve each chat you want to protect.</p></div>`;
  const invite = friendCode && friendDisplayId
    ? friendInviteCardMarkup(compactFriendId(friendDisplayId), escapeHtml, { sectionClass: "friend-invite people-invite", labelId: "people-friend-id-label", friendCode })
    : `<div class="empty-inline friend-code-unavailable">Your invite appears after OSL is unlocked.</div>`;
  return `<main class="content-viewport people-destination" aria-labelledby="route-heading">${peopleDestinationHeaderMarkup()}<section class="people-summary-grid" aria-label="People trust summary"><article><strong>${verified.length.toLocaleString("en-US")}</strong><span>Trusted people</span></article><article><strong>${needsReview.length.toLocaleString("en-US")}</strong><span>Need review</span></article><article><strong>${approvedChats.toLocaleString("en-US")}</strong><span>Approved chats</span></article><article><strong>${broaderReach.toLocaleString("en-US")}</strong><span>Extended reach</span></article></section>${reVerificationNotice}<section class="people-rule-panel" aria-label="Trust rules"><h2>How trust works</h2><ul><li>No approval means OSL refuses protected sends for that chat.</li><li>Verifying a person does not approve every chat with them.</li><li>Each approval stays separate.</li><li>Groups and audiences never inherit trust from a similar name.</li><li>A changed verification returns the person to review before OSL protects new messages.</li></ul></section><section class="people-add-section" aria-labelledby="people-add-title"${addPrimaryTarget}><div><h2 id="people-add-title">Add or verify a person</h2><p>${friendHandshakeDetail(false, false)} Private chats stay off until you compare the verification code another way and approve a chat.</p></div><form id="add-friend-form" class="friend-add-form people-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add person</button></form><p class="form-status" id="friend-form-status" role="status"></p></section><section class="people-review-panel" aria-labelledby="people-review-title"${reviewPrimaryTarget}><header><h2 id="people-review-title">Needs review</h2></header><div class="people-review-list">${reviewRows}</div></section>${invite}<section class="people-list-panel" aria-labelledby="people-list-title"><header><h2 id="people-list-title">People you know</h2><p>Nicknames stay on this device. Open Manage on a person to edit trust for approved chats.</p></header><div class="people-list people-destination-list">${peopleRows}</div></section></main>`;
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
    ? friendInviteCardMarkup(compactFriendId(friendDisplayId), escapeHtml, { sectionClass: "friend-invite", labelId: "friend-id-label", friendCode })
    : `<div class="empty-inline friend-code-unavailable">Your invite appears after OSL is unlocked.</div>`;
  return `<dialog class="friends-dialog" id="friends-dialog" aria-labelledby="friends-dialog-title"><div class="friends-dialog-card"><header><h2 id="friends-dialog-title">Friends</h2><button class="icon-button" id="friends-dialog-close" aria-label="Close friends">×</button></header><form id="add-friend-form" class="friend-add-form"><label for="friend-code-input"><span>Paste their invite</span><input id="friend-code-input" placeholder="OSL invite" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><label for="friend-nickname-input"><span>Name them on this device</span><input id="friend-nickname-input" maxlength="48" placeholder="Nickname (optional)" autocomplete="off" spellcheck="false"/></label><button class="button primary">Add friend</button></form><p class="form-status" id="friend-form-status" role="status"></p><p class="scope-approval-note">Encrypted chats stay off after adding someone. Compare the verification code another way, then approve each chat separately.</p>${addFriendByNameBoxMarkup()}<div class="people-list home-people-list">${peopleListMarkup("manage", friendsDialogPageSize, pageStart)}</div>${pagination}${inviteCard}</div></dialog>`;
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
    return `<div class="whitelist-roster-scope"><span class="friend-scope">${escapeHtml(label)}${scope.userSpecific ? ` <small>only this person</small>` : ""}</span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>+${inDomTooltipMarkup("Already approved")}</button><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" ${!isActive || busy ? "disabled" : ""}>−${inDomTooltipMarkup(isActive ? "Revoke this chat now" : "Open this person's protected chat to revoke")}</button></div></div>`;
  }).join("");
  const narrowedRows = person.reachNarrowedScopes.slice(0, whitelistRosterScopeLimit).map((key) => {
    const label = narrowedScopeLabel(key);
    return `<div class="whitelist-roster-scope narrowed"><span class="friend-scope narrowed">${escapeHtml(label)} <small>taken back</small></span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>+${inDomTooltipMarkup("Approve this chat from inside it")}</button><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>−${inDomTooltipMarkup("Not approved")}</button></div></div>`;
  }).join("");
  const scopes = scopeRows || `<span class="friend-none">No chats approved</span>`;
  const truncated = hiddenScopeCount > 0 || person.whitelistedScopesTruncated
    ? `<small class="whitelist-roster-truncated">${hiddenScopeCount > 0 ? `${hiddenScopeCount} more approved ${hiddenScopeCount === 1 ? "chat is" : "chats are"}` : "More approved chats are"} stored locally and not listed here.</small>`
    : "";
  // Reach widens trust that already exists, so it needs a recorded approval
  // or the approved chat the user is standing in — the hub enforces the same rule.
  const reachDisabled = !isActive || busy || (!person.reachBroadened && person.whitelistCount === 0 && !activeScopeApproved);
  const reachButton = `<button class="button compact in-dom-tooltip-anchor" type="button" data-whitelist-reach="${escapeHtml(person.personId)}" data-whitelist-reach-next="${person.reachBroadened ? "off" : "on"}" aria-pressed="${person.reachBroadened}" ${reachDisabled ? "disabled" : ""}>${person.reachBroadened ? "Limit reach" : "Extend reach"}${inDomTooltipMarkup(person.reachBroadened ? "Withdraw reach across the chats you share" : "Extend this trust to the other chats you share")}</button>`;
  const reachNote = isActive ? "" : `<small class="whitelist-roster-note">Open this person's protected chat to change their reach or revoke a chat.</small>`;
  return `<article class="whitelist-roster-row person-row" data-whitelist-person="${escapeHtml(person.personId)}"><header><div><strong>${escapeHtml(nickname)}</strong><small>${escapeHtml(whitelistReachLine(person))}</small></div>${reachButton}</header><div class="whitelist-roster-scopes">${scopes}${narrowedRows}</div>${truncated}${reachNote}</article>`;
}

function whitelistRosterMarkup(): string {
  const active = activeVerifiedDiscordQaPeer();
  return whitelistDropdownMarkup({
    open: whitelistRosterOpen,
    people: hubPeople,
    activePersonId: active?.person.personId ?? null,
    activeScopeApproved: active?.context.scopeApproved === true,
    busy: discordQaHeaderBusy !== null,
  });
}

function nativeDiscordProtectPickerMarkup(): string {
  if (!nativeProtectPickerOpen || activeNativeHostId !== "discord") return "";
  const friends = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const choices = friends.length
    ? friends.map((person, index) => `<button ${friends.length === 1 && index === 0 ? 'id="native-protect-verified-peer" ' : ""}class="peer-friend-row" type="button" data-native-protect-person="${escapeHtml(person.personId)}" ${nativeProtectBusy ? "disabled" : ""}><span>${escapeHtml(person.alias ?? "Verified friend")}</span><small>Verified</small></button>`).join("")
    : `<p class="peer-empty">Verify a friend first.</p>`;
  return `<dialog class="unlock-dialog" id="native-protect-friend-dialog"><div class="unlock-card"><h2>Protect with</h2><p>OSL will open its own private panel. Discord is not read or controlled.</p><div class="peer-choice-list">${choices}</div><button class="button" id="native-protect-picker-close" type="button">Cancel</button></div></dialog>`;
}

function activeServiceContextTarget(): { serviceId: string; accountId: string } | null {
  if (!activeService) return null;
  const provider = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId)?.provider ?? null;
  const matching = activeService.accounts.filter((account) => provider === null || account.provider === provider);
  return matching.length === 1 ? { serviceId: activeService.id, accountId: matching[0].id } : null;
}

function burnScopeReason(scope: BurnScope): string | null {
  if (scope === "chat" && !activeContextToken) return "Open a supported chat first.";
  if (scope === "app") {
    if (!activeService) return "Open an app first.";
    if (!activeServiceContextTarget()) return "Choose one connected account first.";
    if (serviceBurnReadinessBusy) return "Checking complete local coverage…";
    if (!serviceBurnReadiness?.coverageComplete) return "OSL cannot prove complete coverage for this account yet.";
  }
  if (scope === "account" && !core.readiness.identityLoaded) return "Unlock an OSL account first.";
  return null;
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

/**
 * The peer-acknowledgement line of a chat burn.
 *
 * `data-revocation-acknowledged` is the machine-readable half: `false` means at
 * least one person who had access has not confirmed the revocation, and the
 * line renders in the app's refusal colour rather than the finished one.
 *
 * All styling is in styles.css. The app ships CSP `style-src 'self'`, which
 * drops runtime `<style>` elements and inline `style=` attributes alike, so an
 * inline rule here would silently render unstyled.
 */
function burnRevocationMarkup(revocation: BurnRevocationReceipt | undefined): string {
  if (!revocation) return "";
  return `<p class="burn-revocation-line ${revocation.acknowledged ? "acknowledged" : "outstanding"}" data-revocation-acknowledged="${revocation.acknowledged ? "true" : "false"}" data-revocation-outstanding="${revocation.outstanding}" role="status">${escapeHtml(revocation.line)}</p>`;
}

function burnGuaranteeMarkup(effects: string): string {
  return `<section class="burn-truth burn-guarantees" aria-labelledby="burn-guarantee-title"><strong id="burn-guarantee-title">Before you continue</strong><p>${escapeHtml(effects)}</p>${burnFeatureClaimsMarkup()}</section>`;
}

function burnScopeTruthMarkup(scope: BurnScope): string {
  const destroys = scope === "chat"
    ? ["OSL's local decrypt material and cached data for this chat.", "The local approval, display, and expiry settings for this chat."]
    : scope === "app"
      ? ["OSL's indexed local settings and caches for this connected account.", "Sent relay blobs only where OSL can request and confirm their cleanup."]
      : ["Every OSL identity, decrypt key, cache, and preference in OSL storage.", "The account key held in operating-system secure storage."];
  const survives = scope === "chat"
    ? ["Provider messages, screenshots, exports, backups, and other copies outside OSL.", "Anything the other person already opened."]
    : scope === "app"
      ? ["Your login profile, cookies, provider history, and other copies outside OSL.", "Anything the other person already opened."]
      : ["Third-party provider data, screenshots, exports, backups, and host-OS remnants outside OSL.", "Anything the other person already opened."];
  const list = (items: readonly string[]): string => `<ul>${items.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul>`;
  return `<section class="burn-truth burn-scope-truth"><strong>WHAT THIS DESTROYS</strong>${list(destroys)}</section><section class="burn-truth burn-scope-survives"><strong>WHAT SURVIVES</strong>${list(survives)}</section>`;
}

function burnDialogMarkup(): string {
  if (!burnDialogOpen) return "";
  if (burnResult) {
    const destructStatus = burnResult.destructServerStatus
      ? destructStatusMarkup({ action: "burn", local: "complete", server: burnResult.destructServerStatus })
      : "";
    return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card burn-result"><header><div><p class="eyebrow">Burn</p><h2 id="burn-dialog-title">${burnResult.tone === "success" ? "Finished" : burnResult.tone === "warning" ? "Needs attention" : "Nothing was claimed"}</h2></div><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><p class="burn-result-message ${burnResult.tone}" role="status">${escapeHtml(burnResult.message)}</p>${destructStatus}${burnRevocationMarkup(burnResult.revocation)}${burnResult.showUninstall ? `<div class="burn-uninstall"><strong>Uninstall is separate</strong><p>Your local OSL cleanup finished. Windows controls removal of the app itself.</p><a class="button" href="ms-settings:appsfeatures">Open Windows installed apps</a></div>` : ""}<footer><button class="button primary" data-close-burn>Done</button></footer></section></dialog>`;
  }

  const cards: Array<{ scope: BurnScope; title: string; detail: string }> = [
    { scope: "chat", title: "This chat", detail: activeProtectedContextKind === "peer" ? "Revoke this app account + friend scope." : "Forget this exact OSL conversation on this device." },
    { scope: "app", title: "This app", detail: "Remove indexed local OSL data and request relay cleanup." },
    { scope: "account", title: "Entire OSL account", detail: "Remove every OSL identity, key, cache, and setting held in OSL's own storage." },
  ];
  const selectedReason = burnScopeReason(burnScope);
  const scopeCards = cards.map((card) => {
    const reason = burnScopeReason(card.scope);
    return `<button class="burn-scope-card ${burnScope === card.scope ? "selected" : ""}" type="button" data-burn-scope="${card.scope}" ${reason ? "disabled" : ""} aria-pressed="${burnScope === card.scope}"><strong>${card.title}</strong><small>${card.detail}</small>${reason ? `<span>${escapeHtml(reason)}</span>` : ""}</button>`;
  }).join("");
  const effects = burnScope === "chat"
    ? activeProtectedContextKind === "peer"
      ? "OSL revokes local approval, display, and expiry settings for this app account + friend, then attempts to delete sent relay blobs. Provider messages and opened copies remain."
      : "OSL destroys local decrypt material and caches for this exact chat."
    : burnScope === "account"
      ? "OSL removes every identity, decrypt key, cache, and preference in its own storage directories, including the account key held in operating-system secure storage, and then checks those directories are empty before reporting success."
      : serviceBurnReadiness?.coverageComplete
        ? `OSL removes local settings and caches for ${serviceBurnReadiness.indexedScopes} indexed ${serviceBurnReadiness.indexedScopes === 1 ? "scope" : "scopes"} in this connected account, then attempts to delete their sent relay blobs. Login profile, cookies, provider history, and other copies remain.`
        : "OSL must prove complete local coverage before app-wide burn is available.";
  const pro = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const review = burnScope === "chat" ? burnReviewScreenMarkup(burnReviewScreenState) : "";
  return `<dialog class="burn-dialog" id="burn-dialog" aria-labelledby="burn-dialog-title"><section class="burn-card"><header><h2 id="burn-dialog-title">Burn local data</h2><button class="icon-button" data-close-burn aria-label="Close Burn">×</button></header><div class="burn-scope-grid" aria-label="Burn scope">${scopeCards}</div>${burnScopeTruthMarkup(burnScope)}${review}${burnGuaranteeMarkup(effects)}${burnScope === "account" ? freshStartLimitationsMarkup() : ""}<details class="burn-more"><summary>Other options</summary><div class="burn-options"><label class="setting-line unavailable"><span><strong>Provider messages</strong><small>Not removed. Burn changes only indexed local OSL data and sent relay records.</small></span><input type="checkbox" disabled/></label><label class="setting-line unavailable"><span><strong>Burn for friends · Pro</strong><small>${pro ? "Requires every recipient’s prior signed consent and an acknowledgment from each device." : "A Pro initiator may request this for Free recipients only after each recipient gives signed consent."} The consent-and-acknowledgment workflow is unavailable in this build.</small></span><input type="checkbox" disabled/></label>${burnScope === "account" ? `<label class="setting-line interactive"><span><strong>Uninstall after burn</strong><small>After a successful local burn, open Windows installed apps.</small></span><input id="burn-uninstall" type="checkbox"/></label>` : ""}</div></details><form id="burn-confirm-form" class="burn-confirm"><label class="burn-confirm-ack" for="burn-confirm-ack"><input id="burn-confirm-ack" type="checkbox" ${selectedReason ? "disabled" : ""}/><span>I understand this cannot be undone.</span></label><p class="form-status" id="burn-form-status" role="status">${selectedReason ? escapeHtml(selectedReason) : "Check the box to continue."}</p><footer><button class="button ghost" type="button" data-close-burn>Cancel</button><button class="button danger" id="burn-confirm-submit" type="submit" disabled>${burnBusy ? "Burning…" : "Burn now"}</button></footer></form></section></dialog>`;
}

function verificationDialogMarkup(copy: FriendVerificationCopy): string {
  return `<p>${escapeHtml(copy.heading)}</p><code class="verification-code" aria-label="Shared verification code for this friend">${escapeHtml(copy.code)}</code><label class="owned-confirmation-entry" for="friend-verification-input"><span>${escapeHtml(copy.instruction)}</span><input id="friend-verification-input" autocomplete="off" spellcheck="false" inputmode="numeric" autocapitalize="none" maxlength="96" placeholder="Spaces and grouping do not matter"/></label><p>${escapeHtml(copy.consequence)}</p><p>${escapeHtml(copy.invalidationNotice)}</p>`;
}

function ownedConfirmationMarkup(): string {
  if (!ownedConfirmation) return "";
  const request = ownedConfirmation;
  const verifying = request.kind === "verifyFriend";
  const removing = request.kind === "removeFriend";
  const person = verifying || removing ? hubPeople.find((candidate) => candidate.personId === request.personId) ?? null : null;
  const title = verifying ? "Verify this friend's key?" : removing ? "Remove friend?" : "Clear Pro activation?";
  const detail = request.kind === "verifyFriend"
    ? verificationDialogMarkup(friendVerificationCopy(person?.alias ?? null, person?.safetyNumber ?? null))
    : request.kind === "removeFriend"
      ? `<p>Removing ${escapeHtml(person?.alias ?? "this friend")} deletes this friend's keys from this device and withdraws every conversation approval they hold.</p><p>This cannot be undone.</p>`
    : `<p>Pro features will be unavailable on this device until you activate again.</p>`;
  return `<dialog class="owned-confirmation-dialog" id="owned-confirmation-dialog" aria-labelledby="owned-confirmation-title"><section class="owned-confirmation-card"><header><h2 id="owned-confirmation-title">${title}</h2><button class="icon-button" data-close-owned-confirmation aria-label="Cancel">×</button></header>${detail}<p class="form-status" role="status">${escapeHtml(ownedConfirmationError)}</p><footer><button class="button" data-close-owned-confirmation>Cancel</button><button class="button ${verifying ? "primary" : "danger"}" id="owned-confirmation-submit" type="button" ${ownedConfirmationSubmitDisabled(ownedConfirmationBusy) ? "disabled" : ""}>${ownedConfirmationBusy ? "Working…" : verifying ? "Accept key" : removing ? "Remove friend" : "Clear activation"}</button></footer></section></dialog>`;
}

function serviceContent(): string {
  const name = escapeHtml(activeHomeAppName());
  const mailScope = mailComposerEncryptionScope(activeHomeApp());
  if (activeService && serviceGuideStep !== null) return serviceGuideContent(activeService, serviceGuideStep);
  if (activeNativeHostId && activeNativeHostMode === "existingSession") {
    const protectionFailure = nativeProtectFailureNotice
      ? `<p class="form-status" role="status">${escapeHtml(nativeProtectFailureNotice)}</p>`
      : "";
    return `<main class="content-viewport native-app-page native-companion-page" id="route-heading" tabindex="-1"><section class="native-app-card native-companion-card"><span class="service-icon large">${activeService ? serviceLogo(activeService.id) : ""}</span><h1>${name} is open</h1><p>Signed-in window reused · session not copied</p>${discordQaHostStatusMarkup()}${protectionFailure}<button class="button primary" id="native-companion-focus" type="button" ${nativeActionBusy ? "disabled" : ""}>${nativeActionBusy ? "Opening…" : "Bring forward or reopen"}</button><div class="native-app-secondary"><button class="text-back" id="native-app-back">← Apps</button></div></section></main>`;
  }
  if (activeNativeHostId) return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="${name} is open in an OSL-specific native window"><span class="sr-only">${name} native client is open inside OSL.</span></main>`;
  if (activeDefaultBrowserCompanion) return `<main class="content-viewport host-viewport native-host-open" id="route-heading" tabindex="-1" aria-label="${name} is open in your default-browser companion"><span class="sr-only">${name} is open in an app-style normal-profile browser window. It is not capture-protected or shortcut-locked by OSL.</span></main>`;
  if (activeEmbeddedHost) return `<main class="content-viewport host-viewport host-open" id="route-heading" tabindex="-1" aria-label="${name} is open inside OSL"><div class="loading-host" aria-hidden="true"><span class="host-skeleton logo"></span><span class="host-skeleton title"></span></div></main>`;
  if (serviceAccountPickerOpen) return serviceAccountPickerContent();
  const claimedApp = activeNativeApp();
  if (claimedApp) {
    return tileStatusPageMarkup(claimedApp, {
      logo: activeService ? serviceLogo(activeService.id) : "",
      busy: nativeActionBusy,
    });
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
  const directNativeAccountChoice = activeHomeAppId !== null && supportedNativeAppIds.has(activeHomeAppId as NativeAppId);
  const directBrowserAccountChoice = defaultBrowserCompanionEligible(activeHomeAppId) && selectedBrowserHasImportReceipt();
  const selectedApp = homeAppsFromServices(services).find((app) => app.id === activeHomeAppId);
  const sessionChoices = directNativeAccountChoice && activeHomeAppId === "discord"
    ? discordSessionModeChoices()
      : directBrowserAccountChoice
        ? browserSessionModeChoices()
      : "";
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
  // The menu items come straight from settings-home.ts -- the one list that
  // declares the choices, their canonical labels, and their explanations. A
  // hand-copied array here once dropped "privacy" and the menu's own guard
  // then threw on every render, killing the whole Settings screen.
  const items = settingsHomeMenuItems();
  // These buttons pick a section WITHIN Settings, so they are not `page`.
  // Settings itself is the page, and the primary sidebar already marks it
  // `aria-current="page"`; marking a section button the same way put two
  // "current page" markers in one document and left a screen-reader user with no
  // way to tell which one was the destination. `aria-current="true"` is the
  // generic "this one is current in its own set".
  // The Settings home is this menu: every choice carries one line saying what
  // is behind it, because "Scrub" and "Cleanup" are indistinguishable to a
  // first-time user from their labels alone. settingsHomeMenuMarkup refuses to
  // render if any of the eight choices is missing.
  return `<main class="content-viewport settings-page" aria-labelledby="route-heading"><nav class="settings-sidebar settings-home" aria-label="Settings"><h1 id="route-heading" tabindex="-1">Settings</h1><p class="settings-home-intro">Choose what you want to change.</p>${settingsHomeMenuMarkup(items, settingsSection)}</nav><section class="settings-detail">${settingsSectionContent()}</section></main>`;
}

function settingsSectionContent(): string {
  if (settingsSection === "account") return removeEverythingScreenOpen
    ? removeEverythingScreenMarkup()
    : `${identitySettingsContent()}${settingsDivider()}${dataAllowanceSettingsContent()}${settingsDivider()}${passwordSecuritySettingsContent()}${accountAdvancedSettingsContent()}${renderRecoveryStatesSettings()}`;
  if (settingsSection === "apps") return `${serviceAccountsSettingsContent()}${optionalComponentsSettingsContent()}${sendingSettingsContent()}${messageDefaultsSettingsEntry()}`;
  if (settingsSection === "privacy") return `${privacySectionSettingsContent()}${settingsDivider()}${discoverySettingsContent()}`;
  if (settingsSection === "whitelisting") return whitelistingSettingsContent();
  // Legacy name: privacySettingsContent renders the SCRUB screen.
  if (settingsSection === "scrub") return privacySettingsContent();
  if (settingsSection === "cleanup") return massCleanupSettingsContent();
  if (settingsSection === "notifications") return notificationSettingsContent();
  if (settingsSection === "window-sounds") return windowSoundsSettingsMarkup(windowSoundsSettings);
  if (settingsSection === "appearance") return appearanceSettingsContent();
  return updateSettingsContent();
}

function dataAllowanceSettingsContent(): string {
  const limit = dataAllowanceLimitBytes();
  const total = dataAllowanceTotalBytes();
  const categories: ReadonlyArray<readonly [DataAllowanceCategory, string, string]> = [
    ["backgroundConnection", "Background connection", "The constant-rate cover connection on every device."],
    ["messages", "Messages", "Protected and plain text, including held and undelivered bytes."],
    ["attachments", "Attachments", "Files and other uploaded media."],
    ["storiesAndPosts", "Stories and posts", "Media in stories and posts."],
    ["deviceSync", "Device sync", "Traffic to keep this account in sync across devices."],
    ["voice", "Voice", "Voice traffic belongs in this same pool, not separate minutes."],
  ];
  const rows = categories.map(([id, label, detail]) => {
    const value = dataAllowanceLedger[id];
    return `<div class="setting-line data-allowance-row"><span><strong>${label}</strong><small>${detail}</small></span><strong>${value === null ? "Not measured" : formatAllowanceBytes(value)}</strong></div>`;
  }).join("");
  const balance = total === null ? "Waiting for all-traffic ledger" : `${formatAllowanceBytes(total)} / ${formatAllowanceBytes(limit)}`;
  const meter = total === null
    ? `<div class="data-allowance-meter is-unavailable" role="progressbar" aria-label="Data this month: waiting for an all-traffic ledger" aria-valuetext="Usage is not measured yet"><span></span></div>`
    : `<div class="data-allowance-meter" role="progressbar" aria-label="Data this month" aria-valuemin="0" aria-valuemax="${limit}" aria-valuenow="${total}"><span style="width:${Math.min(100, total / limit * 100)}%"></span></div>`;
  return `<section class="settings-section data-allowance-settings" data-data-allowance-source="storage-ruling-provisional"><header><div><h3>Data this month</h3><p>One monthly allowance for everything OSL carries.</p></div>${statusTag(total !== null && total / limit >= DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.warningPercent / 100 ? "90% warning" : "One allowance")}</header><div class="data-allowance-balance"><strong>${escapeHtml(balance)}</strong><small>Free ${formatAllowanceBytes(DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.freeBytes)} · Pro ${formatAllowanceBytes(DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.proBytes)} · warning at ${DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.warningPercent}%</small>${meter}</div><p class="data-allowance-ledger-note"><strong>Usage is not yet available.</strong> This build has no server ledger for held bytes, relay traffic, media, sync, or voice. It will not show a false zero or claim that uploads are being enforced.</p><div class="settings-list data-allowance-items">${rows}</div><details class="settings-disclosure data-allowance-refusal"><summary><span><strong>When a file would exceed the allowance</strong><small>Refuse before upload; never silently slow it.</small></span></summary><div><p>Once the all-traffic ledger is connected, OSL must refuse the upload before it starts and offer: <strong>send a smaller version</strong>, <strong>buy a top-up</strong>, or <strong>queue until reset</strong> with the reset countdown. Top-up payment and the queue are not connected in this build, so neither is presented as available.</p></div></details><details class="settings-disclosure data-allowance-source" open><summary><span><strong>Limit source and unresolved conflict</strong><small>${escapeHtml(DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.source)}</small></span></summary><div><p>These on-screen figures come only from <strong>${escapeHtml(DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.source)}</strong>.</p><p>${escapeHtml(DATA_ALLOWANCE_LIMITS_FROM_STORAGE_RULING.conflict)}</p><p>The relay attachment allowlist also caps attachment life at 7 days. A 30-day attachment-retention figure cannot be shown as available.</p></div></details></section>`;
}

async function changePrivacyConnectionRoute(choice: "tor" | "direct"): Promise<void> {
  const previous = torOnboarding;
  torOnboarding = chooseTorRoute(torOnboarding, choice);
  render();
  try {
    await invoke("set_tor_preference", { preference: choice });
    showToast(`${choice === "tor" ? "Tor" : "Direct"} connection saved`);
  } catch {
    torOnboarding = previous;
    showToast("Connection preference could not be saved");
  }
  render();
}

async function changePrivacyCoverInsertion(choice: CoverInsertionChoice): Promise<void> {
  const previous = coverInsertion;
  coverInsertion = chooseCoverInsertion(coverInsertion, choice);
  render();
  try {
    const saved = await saveOnboardingPreferences({ onboardingComplete, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode });
    coverInsertion = saved.coverInsertion;
    showToast("Cover text preference saved");
  } catch {
    coverInsertion = previous;
    showToast("Cover text preference could not be saved");
  }
  render();
}

// These controls expose every privacy choice this build can genuinely change.
// The before-send engine does not yet consume its three onboarding values, so
// it remains visibly unavailable instead of becoming a second false promise.
function privacySectionSettingsContent(): string {
  const routeChoice = torOnboarding.choice ?? "direct";
  const modeChoices: ReadonlyArray<readonly [SendMode, string]> = [["manual", "Manual"], ["clipboard", "Copy"], ["double", "Double Enter"], ["single", "Single Enter"]];
  const coverChoices: ReadonlyArray<readonly [CoverInsertionChoice, string, string]> = [["insert-on-send", "Insert on send", "The cover appears together."], ["type-naturally", "Type naturally", "OSL types the cover one character at a time."]];
  const routeOptions = (["direct", "tor"] as const).map((choice) => `<label class="setting-option privacy-choice"><input type="radio" name="settings-connection-route" value="${choice}" ${routeChoice === choice ? "checked" : ""}/><span><strong>${choice === "tor" ? "Use Tor" : "Connect directly"}</strong><small>${choice === "tor" ? "OSL traffic uses the bundled tunnel when it is available." : "OSL connects without an anonymity layer."}</small></span></label>`).join("");
  const sendOptions = modeChoices.map(([mode, label]) => `<button class="send-mode-option ${setup.sendMode === mode ? "selected" : ""}" type="button" data-settings-send-mode="${mode}" aria-pressed="${setup.sendMode === mode}"><span><strong>${label}</strong></span><small>${mode === "manual" ? "You place and send" : mode === "clipboard" ? "OSL never presses Send" : "Requires explicit risk acceptance"}</small></button>`).join("");
  const coverOptions = coverChoices.map(([choice, title, detail]) => `<label class="setting-option privacy-choice"><input type="radio" name="settings-cover-mode" value="${choice}" ${coverInsertion === choice ? "checked" : ""}/><span><strong>${title}</strong><small>${detail}</small></span></label>`).join("");
  return `<h2>Privacy</h2><p>${escapeHtml(settingsHomeExplanation("privacy"))}</p><section class="settings-section"><header><div><h3>Connection</h3><p>Choose OSL’s route; Mullvad is a separate network tool.</p></div>${statusTag(routeChoice === "tor" ? "Tor" : "Direct")}</header><div class="settings-list privacy-choice-list">${routeOptions}<div class="setting-line"><span><strong>Mullvad</strong><small>OSL cannot see whether it is connected. You can use both. Neither replaces the other.</small></span>${statusTag(mullvadStatus.availability === "installed" ? "Available" : "Not observed")}</div></div></section><section class="settings-section"><header><div><h3>Send mode</h3><p>How OSL prepares a protected carrier message.</p></div></header><div class="send-mode-list compact">${sendOptions}</div></section><section class="settings-section"><header><div><h3>Cover text</h3><p>How cover text reaches the other app’s composer.</p></div></header><div class="settings-list privacy-choice-list">${coverOptions}</div></section><section class="settings-section"><header><div><h3>Pre-send checks</h3><p>These selections are visible but not enforced at the send boundary in this build.</p></div>${statusTag("Not enforced")}</header><div class="settings-list"><label class="setting-line unavailable"><span><strong>Warn before an unprotected message</strong><small>${beforeSendChecks.warnUnprotected ? "Selected during setup" : "Not selected during setup"}</small></span><input type="checkbox" ${beforeSendChecks.warnUnprotected ? "checked" : ""} disabled/></label><label class="setting-line unavailable"><span><strong>Warn before a protected message</strong><small>${beforeSendChecks.warnProtected ? "Selected during setup" : "Not selected during setup"}</small></span><input type="checkbox" ${beforeSendChecks.warnProtected ? "checked" : ""} disabled/></label><div class="setting-line unavailable"><span><strong>File metadata</strong><small>${beforeSendChecks.cleanFiles === "always" ? "Remove before every send" : beforeSendChecks.cleanFiles === "ask" ? "Ask before removing" : "Do not remove"} · not enforced yet.</small></span>${statusTag("Not enforced")}</div></div></section><section class="settings-section"><header><div><h3>Incoming warnings</h3><p>Security changes from OSL friends appear as local activity.</p></div></header><div class="settings-list"><label class="setting-line interactive"><span><strong>Encryption-key changes</strong><small>Warn when a friend’s key needs verification. This never approves a chat.</small></span><input id="privacy-incoming-key-warnings" type="checkbox" ${notificationSecurityActivity ? "checked" : ""}/></label></div></section><section class="settings-section"><header><div><h3>Friending</h3><p>Only verified people can be approved for protected chats.</p></div><button class="button compact" type="button" data-settings-friending>Review people</button></header><div class="settings-list"><div class="setting-line"><span><strong>Protected chat approval</strong><small>Default deny. Verify a person and approve each chat explicitly; key changes revoke that approval.</small></span>${statusTag("Verify first")}</div></div></section>`;
}

// The allowed-conversation list the Whitelisting screen ticks. Every row is a
// chat the hub already knows about for a verified person: the ones in
// whitelistedScopes are allowed right now, the ones in reachNarrowedScopes are
// chats that were taken back. That is the whole saved answer -- this screen
// never invents a conversation the hub has not seen.
const whitelistingRowSeparator = "::";

function whitelistingConversationId(personId: string, storageKey: string): string {
  return `${personId}${whitelistingRowSeparator}${storageKey}`;
}

function whitelistingConversationParts(id: string): { personId: string; storageKey: string } | null {
  const at = id.indexOf(whitelistingRowSeparator);
  if (at <= 0) return null;
  return { personId: id.slice(0, at), storageKey: id.slice(at + whitelistingRowSeparator.length) };
}

function whitelistingRows(): { conversations: WhitelistingConversation[]; saved: string[] } {
  const conversations: WhitelistingConversation[] = [];
  const saved: string[] = [];
  for (const person of hubPeople.filter((candidate) => candidate.safetyNumberVerified && !candidate.pendingKeyChange)) {
    const nickname = person.alias ?? "Unnamed friend";
    for (const scope of person.whitelistedScopes.slice(0, whitelistRosterScopeLimit)) {
      const id = whitelistingConversationId(person.personId, scope.storageKey);
      conversations.push({
        id,
        account: `Approved for ${nickname}`,
        name: friendScopeLabel(scope),
        kind: scope.userSpecific ? "Only this person" : "Anyone verified here",
      });
      saved.push(id);
    }
    for (const key of person.reachNarrowedScopes.slice(0, whitelistRosterScopeLimit)) {
      conversations.push({
        id: whitelistingConversationId(person.personId, key),
        account: `Taken back for ${nickname}`,
        name: narrowedScopeLabel(key),
        kind: "Not allowed since you took it back",
      });
    }
  }
  return { conversations, saved };
}

function whitelistingScreenState(): WhitelistingScreenState {
  const { conversations, saved } = whitelistingRows();
  const known = new Set(conversations.map((conversation) => conversation.id));
  // A draft can outlive the row it ticked (the hub reloads, a friend is
  // removed). Dropping unknown ids keeps Save from writing to a chat that is no
  // longer on screen.
  const draft = whitelistingDraft === null ? saved : whitelistingDraft.filter((id) => known.has(id));
  return { conversations, saved, draft, search: whitelistingSearch, busy: whitelistingBusy || discordQaHeaderBusy !== null };
}

function setWhitelistingState(next: WhitelistingScreenState): void {
  whitelistingSearch = next.search;
  whitelistingDraft = next.draft;
  render();
}

// Save only ever removes. Approving a chat still has to happen from inside that
// chat, where OSL can see which scope the user is actually standing in, so a tick
// that turns ON is reported back as refused rather than silently written.
async function saveWhitelistingSelection(): Promise<void> {
  const state = whitelistingScreenState();
  const changes = whitelistingPendingChanges(state);
  if (changes.allow.length === 0 && changes.remove.length === 0) {
    showToast("Nothing to save");
    return;
  }
  const active = activeVerifiedDiscordQaPeer();
  whitelistingBusy = true;
  render();
  let removed = 0;
  const refused: string[] = [];
  for (const id of changes.remove) {
    const parts = whitelistingConversationParts(id);
    if (!parts || !active || active.person.personId !== parts.personId) {
      refused.push(id);
      continue;
    }
    const updated = await revokeActiveHubFriendScope(active.context.contextToken, parts.personId, parts.storageKey);
    if (!updated) {
      refused.push(id);
      continue;
    }
    removed += 1;
    hubPeople = hubPeople.map((person) => person.personId === parts.personId ? updated : person);
  }
  hubPeople = await listHubPeople() ?? hubPeople;
  whitelistingBusy = false;
  whitelistingDraft = null;
  render();
  const blocked = refused.length + changes.allow.length;
  showToast(blocked === 0
    ? `Saved. ${removed} ${removed === 1 ? "conversation is" : "conversations are"} no longer allowed.`
    : `Saved ${removed} of ${removed + blocked}. ${blocked} ${blocked === 1 ? "change needs" : "changes need"} that chat open first.`);
}

// The saved message defaults the Settings entry row summarises. Nothing else
// in main.ts is wired to the Message defaults screen yet, so this starts (and
// stays) at the factory defaults; the entry row referenced this state without
// declaring it and crashed the whole "apps" section at runtime.
let messageDefaultsScreen = initialMessageDefaultsScreenState();

/** The way in to the Message defaults screen from Settings. */
function messageDefaultsSettingsEntry(): string {
  const labels = savedMessageDefaultLabels(messageDefaultsScreen.saved);
  return `<div class="setting-line" data-message-defaults-entry><span><strong>Message defaults</strong><small>Timer ${escapeHtml(labels.timer)} · Burn ${escapeHtml(labels["burn-scope"])} · View once ${escapeHtml(labels["view-once-length"])} · ${escapeHtml(labels.writing)}</small></span><button class="button compact" type="button" data-route="message-defaults">Open</button></div>`;
}

function whitelistingSettingsContent(): string {
  const verified = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange);
  const active = activeVerifiedDiscordQaPeer();
  const approvedChats = verified.reduce((total, person) => total + person.whitelistCount, 0);
  const rows = verified.length
    ? verified.map((person) => whitelistRosterPersonMarkup(
      person,
      active?.person.personId ?? null,
      discordQaHeaderBusy !== null,
      active?.context.scopeApproved === true,
    )).join("")
    : `<div class="empty-state compact"><strong>No verified people yet</strong><p>Verify a friend before any chat can be whitelisted.</p></div>`;
  const roster = `<section class="settings-list whitelist-settings" data-settings-whitelisting aria-labelledby="whitelisting-people-title"><header><h2 id="whitelisting-people-title">Who is trusted where</h2><p>${verified.length.toLocaleString("en-US")} verified ${verified.length === 1 ? "person" : "people"} · ${approvedChats.toLocaleString("en-US")} approved ${approvedChats === 1 ? "chat" : "chats"}. A chat is approved from inside that chat; here you can review it or take it back.</p></header>${rows}</section>`;
  return `${whitelistingScreenMarkup(whitelistingScreenState())}${settingsDivider()}${roster}`;
}

function discoverySettingsContent(): string {
  const status = discoveryVisibilityStatus
    ? `<p class="discovery-status" role="status">${escapeHtml(discoveryVisibilityStatus)}</p>`
    : "";
  return `${discoveryVisibilityBody(discoveryVisibility)}${status}`;
}

function bindDiscoveryVisibilityControls(): void {
  document.querySelectorAll<HTMLInputElement>("[data-discovery-choice]").forEach((input) => input.addEventListener("change", () => {
    try {
      discoveryVisibility = selectDiscoveryChoice(discoveryVisibility, input.dataset.discoveryChoice ?? "");
    } catch (error) {
      discoveryVisibilityStatus = error instanceof Error ? error.message : String(error);
      render();
      return;
    }
    discoveryVisibilityStatus = null;
    localStorage.setItem(DISCOVERY_VISIBILITY_STORAGE_KEY, serializeDiscoveryVisibility(discoveryVisibility));
    render();
  }));
  document.querySelectorAll<HTMLInputElement>("[data-discovery-pings]").forEach((input) => input.addEventListener("change", () => {
    discoveryVisibility = setDiscoveryReplyToPings(discoveryVisibility, input.checked);
    discoveryVisibilityStatus = null;
    localStorage.setItem(DISCOVERY_VISIBILITY_STORAGE_KEY, serializeDiscoveryVisibility(discoveryVisibility));
    render();
  }));
}

function optionalComponentsSettingsContent(): string {
  const components = [
    { id: "local-ai-model", displayName: "Local AI model", measuredSizeBytes: 1_879_048_192, withoutIt: "The word-bank carrier still works." },
    { id: "tor", displayName: "Tor", measuredSizeBytes: 31_457_280, withoutIt: "OSL connects directly, without an anonymity layer." },
  ] as const;
  const picker = componentPickerScreen(components);
  const manager = componentManagerFromOnboarding(components.map(({ id, displayName, withoutIt }) => ({ id, featureName: displayName, fallback: withoutIt })), []);
  const scrub = decideAutoScrubInstall("autoscrub", null);
  const transfer = deviceTransferManifestScreen();
  const sourceChoice = oldDeviceCopyDecisionView(initialOldDeviceCopyDecision({ importConfirmed: false }));
  return `<details class="settings-disclosure" data-optional-components><summary><span><strong>${picker.title}</strong><small>${picker.introductoryCopy}</small></span></summary><div class="settings-list">${picker.components.map((item) => `<div class="setting-line"><span><strong>${escapeHtml(item.displayName)} · ${escapeHtml(item.size)}</strong><small>${escapeHtml(item.withoutIt)}</small></span></div>`).join("")}<p>${escapeHtml(autoScrubConsentPrompt("each service"))}</p><p data-autoscrub-install="${scrub.allowed ? "allowed" : "blocked"}">AutoScrub installation is blocked until separate explicit consent is recorded.</p>${manager.features.map((feature) => `<p>${escapeHtml(feature.detail)}</p>`).join("")}<h3>${escapeHtml(transfer.title)}</h3>${transfer.sections.map((section) => `<p><strong>${escapeHtml(section.heading)}:</strong> ${escapeHtml(section.items.join(", "))}</p>`).join("")}<p>${sourceChoice.mode === "unavailable" ? "Transfer source-copy choice appears only after a confirmed import." : ""}</p>${renderDeadmanScreen(selectDeadmanAction("lock", ""))}</div></details>`;
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
  const policyCards = policyGroups.map(([name, detail, state]) => `<article class="privacy-policy-card">${statusTag(state)}<h3>${name}</h3><p>${detail}</p></article>`).join("");
  const toolRows = tools.map(([name, detail], index) => `<article class="setting-line privacy-tool-row"><span><strong>${name}</strong><small>${detail}</small></span>${statusTag(index === 0 ? "Available" : proActive ? "Pro planned" : "Pro")}</article>`).join("");
  const privacyDisclosures = [
    ["What Scrub reads", "Scrub reads only the files, exports, and account views you choose for review. It does not read your other apps or accounts."],
    ["What happens to result text", "Result text is kept only in the encrypted Scrub index on this device, and it is removed when you clear results or cancel the import."],
    ["Card details", "OSL does not store card details. You type your card on the payment company's own checkout page, so it never reaches OSL."],
    ["What the payment company gets", "The payment company may receive checkout, billing, fraud, tax, and support data it needs to take the payment."],
  ] as const;
  const disclosureRows = privacyDisclosures.map(([name, detail]) => `<div class="setting-line"><span><strong>${name}</strong><small>${detail}</small></span></div>`).join("");
  const cleanupState = proActive ? "Manual queue planned" : "Pro manual queue";
  const protectionReview = privacyProtectionReviewOpen
    ? `<section class="privacy-review-card" data-privacy-protection-review><div><span class="privacy-local-mark">PROTECTION REVIEW</span><h2>Review or change protection</h2><p>Check the Balanced policy, app exceptions, cleanup limits, and local warning choices before OSL changes anything.</p></div><button class="button compact" data-route="settings" data-settings="scrub" type="button">Open detailed review</button></section>`
    : "";
  const presetCopy: Record<ProtectionPreset, { title: string; detail: string }> = {
    basic: {
      title: "Basic",
      detail: "Account health, email tracker blocking, attachment metadata warnings, and exposure alerts.",
    },
    balanced: {
      title: "Balanced",
      detail: "Basic account health plus local before-send warnings, attachment cleaning, monthly cleanup review, and private OSL suggestions for verified contacts.",
    },
    maximum: {
      title: "Maximum",
      detail: "Balanced protection plus stricter public-post checks, optional VPN-required actions, and OSL protection required for chosen contacts.",
    },
  };
  const activePreset = presetCopy[protectionPreset];
  return `<main class="content-viewport privacy-destination" aria-labelledby="route-heading"><header class="destination-header"><div><p class="eyebrow">Privacy</p><h1 id="route-heading" tabindex="-1">Privacy</h1><p>Review what OSL will do before it changes anything.</p></div><button class="button primary" data-privacy-primary-action data-route="${primary.route}" data-review-target="${primary.reviewTarget}" type="button">Review or change protection</button></header>${protectionReview}<section class="privacy-preset-panel" aria-labelledby="privacy-preset-title"><div><span class="privacy-local-mark">ACTIVE PRESET</span><h2 id="privacy-preset-title">${activePreset.title}</h2><p>${activePreset.detail}</p></div><button class="button compact" data-change-protection-preset type="button">Change preset</button></section><section class="privacy-policy-stack" id="privacy-protection-review" aria-labelledby="privacy-policy-title"><header><div><h2 id="privacy-policy-title">Global policy</h2><p>Inherited from ${activePreset.title} until you make an exception.</p></div>${statusTag("Deletion off")}</header><p class="privacy-policy-path">${activePreset.title} preset / app / account / conversation exception</p><div class="privacy-policy-grid">${policyCards}</div></section>${publicPostGuardCarrierPreviewMarkup()}<section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3><p>Choose a TXT, CSV, or JSON message export. OSL suggests items; you decide what to review. Nothing is deleted by this build.</p></div>${scanActions}</section>${scrubCategoryChooserMarkup(true)}${privacyScanResultsMarkup()}<section class="settings-list privacy-tools" aria-labelledby="privacy-tools-title"><header><h2 id="privacy-tools-title">Solo privacy tools</h2><p>Useful even when nobody else uses OSL.</p></header>${toolRows}</section><section class="settings-list privacy-disclosures" aria-labelledby="privacy-disclosures-title"><header><h2 id="privacy-disclosures-title">Data handling</h2><p>What this page depends on and what stays outside OSL.</p></header>${disclosureRows}</section><section class="settings-list privacy-limits" aria-labelledby="privacy-limits-title"><header><h2 id="privacy-limits-title">Proof and limits</h2><p>OSL refuses actions it cannot verify.</p></header><div class="setting-line"><span><strong>Cleanup</strong><small>${cleanupState}; every batch must be scanned, shown, previewed, confirmed, executed, and checked.</small></span>${statusTag("No auto delete")}</div><div class="setting-line"><span><strong>Service messages</strong><small>Apps, people, exports, backups, and opened copies may retain content.</small></span>${statusTag("Limit shown")}</div><div class="setting-line"><span><strong>Window protection</strong><small>Applied to OSL's own window when available. Cameras, malware, and modified recipients can still capture content.</small></span>${statusTag(screenshotProtectionEnabled ? "Active" : "Unavailable")}</div></section></main>`;
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

/**
 * The single authority on "is this session unlocked?" for Settings. Every
 * surface that makes a lock claim has to route through this, or two panels on
 * one page can disagree about the same session — which is exactly how Settings
 * came to render "Unlock OSL" directly above "Password configured and
 * unlocked".
 */
function accountUnlocked(): boolean {
  return core.readiness.bootstrapStatus !== "setupRequired"
    && core.readiness.bootstrapStatus !== "passwordRequired"
    // D-207: an account whose device key is gone is the least unlocked state
    // there is. Reading it as unlocked is how "Protected \u2014 Device
    // protection confirmed" came to sit above an account nothing could open.
    && core.readiness.bootstrapStatus !== "identityKeyLost";
}

function passwordSecuritySettingsContent(): string {
  const passwordAction = core.readiness.bootstrapStatus === "setupRequired"
    ? `<button class="button primary" data-onboarding-action="create">Create password</button>`
    : core.readiness.bootstrapStatus === "passwordRequired"
      ? `<button class="button primary" data-onboarding-action="unlock">Unlock OSL</button>`
      : core.readiness.bootstrapStatus === "identityKeyLost"
        ? `<span class="setting-status"><span class="dot"></span>This device can no longer open this account</span><button class="button primary" data-onboarding-action="import">Restore with recovery phrase</button>`
        : `<span class="setting-status"><span class="dot"></span>Password configured and unlocked</span><button class="button" type="button" data-lock-session="now">Lock now</button>`;
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
  return `<details class="account-advanced settings-disclosure"><summary>Advanced</summary><div class="danger-zone"><h3>Burn local data</h3><p>Review the scope and limits before anything changes.</p><button class="button danger" id="full-cleanup-button" type="button">Remove everything</button></div></details>`;
}

function serviceAccountsSettingsContent(): string {
  const rows = homeAppsFromServices(services).filter((app) => app.visibility === "launch").map((app) => {
    const state = app.linked ? `${app.accountCount} local ${app.accountCount === 1 ? "profile" : "profiles"}` : app.launchState === "available" ? "Not set up" : app.id === "messenger" ? "Cannot send yet" : "Coming later";
    const action = app.launchState === "available"
      ? `<button class="button compact" data-home-app="${app.id}" ${appLaunchPendingId ? "disabled" : ""}>${appLaunchPendingId === app.id ? "Opening…" : app.linked ? "Open" : "Set up"}</button>`
      : `<button class="button compact" disabled>${app.id === "messenger" ? "Cannot send yet" : "Coming later"}</button>`;
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
  if (!localScrubRouteOpened || !evaluateScrubConsentGate(localScrubConsentRequest, localScrubConsentState).allowed) {
    showToast("Confirm Scrub consent before scanning an export");
    return;
  }
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
      serviceId: localScrubScanServiceId,
      accountId: localScrubScanAccountId,
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
    persistedLocalScrubImportId = persisted.status.importId;
    selectedScrubFindings.clear();
    scrubResultsPage = 0;
    scrubReviewOpen = false;
    scrubReviewPage = 0;
    privacyScanFileName = file.name.slice(0, 96);
  } catch (failure) {
    privacyScanResult = null;
    privacyScanFileName = null;
    persistedLocalScrubImportId = null;
    showToast(localActionError(failure, "The export could not be scanned locally"));
  } finally {
    privacyScanBusy = false;
    render();
    void refreshScrubScopeFingerprint();
  }
}

/**
 * The one scope this build can scan: a message export the owner picked, read on
 * this device. These are the identifiers the findings are stamped with, and the
 * same pair the scope fingerprint is computed over, so the digest always
 * describes the scan it is shown next to.
 */
const localScrubScanServiceId = "local_import";
const localScrubScanAccountId = "manual-export";
const localScrubScanScope = "local message export chosen by the owner, read on this device";

function scrubScopeFingerprintInput(): ScrubScopeFingerprintInput | null {
  if (!privacyScanResult) return null;
  const categories = defaultScrubSignalGroups.filter((group) => enabledScrubSignals.has(group));
  if (!categories.length) return null;
  return {
    serviceId: localScrubScanServiceId,
    accountId: localScrubScanAccountId,
    scanScope: localScrubScanScope,
    findingCategories: categories,
  };
}

/**
 * Recomputes the scope digest whenever the reviewed scope changes.
 *
 * This is a description of what was looked at -- service, account, scan scope,
 * chosen categories -- and nothing else. It is deliberately not a deletion
 * receipt: this build deletes nothing, so the copy beside it says so.
 */
async function refreshScrubScopeFingerprint(): Promise<void> {
  const input = scrubScopeFingerprintInput();
  if (!input) {
    if (!scrubScopeFingerprint) return;
    scrubScopeFingerprint = null;
    render();
    return;
  }
  const key = JSON.stringify(input);
  if (scrubScopeFingerprint?.key === key) return;
  try {
    const value = await computeScopeFingerprint(input);
    if (JSON.stringify(scrubScopeFingerprintInput()) !== key) return;
    scrubScopeFingerprint = { key, value };
  } catch {
    // A refused input or an unavailable WebCrypto must show no digest at all
    // rather than a placeholder the owner could mistake for a real one.
    scrubScopeFingerprint = null;
  }
  render();
}

function scrubScopeFingerprintMarkup(): string {
  if (!scrubScopeFingerprint) return "";
  const digest = scrubScopeFingerprint.value;
  return `<p class="scrub-scope-fingerprint"><span class="scrub-scope-fingerprint-label">Scope fingerprint</span><code class="scrub-scope-fingerprint-digest" title="${escapeHtml(digest)}">${escapeHtml(digest.slice(0, 32))}</code><small>Identifies the exact export, account, and categories this review covers. It changes when you change the categories. It is not proof that anything was deleted.</small></p>`;
}

function scrubSignalGroupLabel(group: ScrubSignalGroup): string {
  return scrubSignalDefinitions.find((definition) => definition.id === group)?.label ?? "Review suggestion";
}

function scrubReviewRowDate(unixMs: number | null): string {
  if (unixMs === null) return "date unknown";
  const parsed = new Date(unixMs);
  return Number.isNaN(parsed.getTime()) ? "date unknown" : parsed.toLocaleDateString();
}

/**
 * The owner-review rows behind the flat suggestion list.
 *
 * The flat list shows one card per finding, so the same sentence posted five
 * times reads as five unrelated problems. Grouping collapses identical items on
 * the same account and site into one row with a count, which is what the owner
 * actually has to act on. It is a second view of the same findings: it selects
 * nothing, changes no selection, and deletes nothing.
 */
function scrubReviewRowsMarkup(rows: readonly ScrubReviewRow[]): string {
  if (!rows.length) return "";
  const items = rows.map((row) => {
    const groups = row.signalGroups.map((group) => escapeHtml(scrubSignalGroupLabel(group))).join(" · ");
    const count = `${row.findingCount} ${row.findingCount === 1 ? "item" : "items"}`;
    return `<li class="scrub-review-row"><div class="scrub-review-row-head"><strong>${escapeHtml(row.logicalHost)}</strong><span class="scrub-review-row-count">${count}</span></div><blockquote class="scrub-review-row-sample">${escapeHtml(row.sample.localPreview)}</blockquote><small class="scrub-review-row-meta">${escapeHtml(row.serviceId)} · ${escapeHtml(row.accountId)} · ${groups} · newest ${escapeHtml(scrubReviewRowDate(row.newestCreatedAtUnixMs))}</small></li>`;
  }).join("");
  return `<details class="scrub-review-rows"><summary>Grouped review rows (${rows.length})</summary><ul class="scrub-review-row-list">${items}</ul><p class="scrub-review-rows-note">Identical items on the same account and site are counted once here. This view is for reading only: choosing what to review still happens in the list above, and this build deletes nothing.</p></details>`;
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
  const selectedMode: SendMode = setup.sendMode;
  const modes: Array<[SendMode, string, string]> = [
    ["manual", "Manual", "Prepare only; you place and send"],
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
  return `<details class="settings-disclosure sending-settings"><summary><span><strong>Sending</strong><small>${escapeHtml(formatSendMode(selectedMode))}</small></span></summary><div class="sending-settings-body"><div class="send-mode-list compact">${modes.map(([mode, label, detail]) => `<button class="send-mode-option ${selectedMode === mode ? "selected" : ""}" type="button" data-settings-send-mode="${mode}" aria-pressed="${selectedMode === mode}"><span><strong>${label}</strong></span><small>${detail}</small></button>`).join("")}</div>${needsRiskAcceptance(selectedMode) ? `<div class="warning send-settings-warning"><strong>Experimental</strong><p>OSL must recheck the exact app, account, chat, and composer. If proof is unavailable or changes, it copies instead and sends nothing.</p></div>` : selectedMode === "manual" ? `<p class="send-settings-truth">OSL prepares the protected message. You place it and decide when to send.</p>` : `<p class="send-settings-truth">OSL encrypts and copies. You choose where and when to send.</p>`}${consentRows}${rnWirePolicySettingsMarkup(rnWirePolicyState(rnWirePolicyRequested))}</div></details>`;
}

async function changeSendingMode(mode: SendMode): Promise<void> {
  if (!["manual", "clipboard", "double", "single"].includes(mode)) return;
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
    const saved = await saveOnboardingPreferences({ onboardingComplete: true, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode });
    setup = saved.setup;
    windowCaptureEnabled = saved.windowCaptureEnabled;
    rnWirePolicyRequested = saved.rnWirePolicyRequested;
    showToast(`${formatSendMode(mode)} selected`);
  } catch {
    setup = previous;
    showToast("Sending preference could not be saved");
  }
  render();
}

/**
 * The Scrub destination. The full Scrub wizard -- consent gate, the
 * choose/scan/review route, local results, and the AutoScrub status card --
 * was built as `privacySettingsContent()` (a legacy name; it renders the
 * SCRUB screen) but was only reachable as the Settings "scrub" section, and
 * the Home tile pointed at the generic Privacy page, so the finished screen
 * could never be opened. That wizard implements the SUPERSEDED
 * delete-from-a-local-export model, so this route now mounts the canonical
 * two-column DISCOVERY console instead (design-export-2026-08-08/Scrub.dc.html
 * via scrub-discovery-screen.ts): per-account allow ticks and the
 * Discovery/AutoScrub mode list on the left, the streaming console on the
 * right. Discovery deletes nothing, so this route carries no consent gate; the
 * plain-sentence checkbox consent page (design rule 4) exists only on the
 * AutoScrub (Pro) path inside the screen module. The legacy wizard remains
 * reachable only as the Settings "scrub" section.
 */
function scrubDestinationContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  return `<main class="content-viewport scrub-destination" aria-labelledby="route-heading">${scrubDiscoveryScreenMarkup(proActive)}</main>`;
}

const oslHandleDiscoveryDisclosureSentence = "If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.";

function privacySettingsContent(): string {
  const proActive = licenseState.access === "pro" || licenseState.access === "offlineGrace";
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${privacyScanBusy ? "disabled" : ""}" for="privacy-export-input">${privacyScanBusy ? "Scanning…" : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${privacyScanBusy ? "disabled" : ""}/>${privacyScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const routeState: ScrubRouteState = {
    accounts: [{ id: "local-export", label: "Local message export", detail: "TXT, CSV, or JSON on this device" }],
    selectedAccountIds: localScrubRouteAccountSelected ? ["local-export"] : [],
    selectedCategories: [...localScrubRouteCategories],
    scan: { state: privacyScanBusy ? "scanning" : privacyScanResult ? "complete" : "not-started", findings: privacyScanResult?.findings.length ?? 0 },
  };
  const consent = evaluateScrubConsentGate(localScrubConsentRequest, localScrubConsentState);
  const gatedRoute = scrubConsentGatedRouteMarkup(
    localScrubConsentRequest,
    localScrubConsentState,
    routeState,
    localScrubRouteStep,
    localScrubRouteOpened,
  );
  const scanControls = consent.allowed && localScrubRouteOpened && localScrubRouteStep === "scan" ? scanActions : "";
  return `<h2>Scrub</h2><p class="scrub-local-promise"><strong>Your messages never leave this device.</strong> Every scan and review stays local.</p>${gatedRoute}${scanControls}${scrubCategoryChooserMarkup()}${privacyScanResultsMarkup()}${autoScrubAssistantMarkup(proActive)}<details class="safety-disclosure scrub-safety"><summary>Before deleting anything</summary><div><p><strong>Use at your own risk.</strong> Suggestions can be wrong. Check every message first.</p><p>Deletion can be irreversible. Scrub cannot undo copies, screenshots, or service records, or guarantee service permission. Only a service recheck can verify removal within its stated coverage.</p><p>Automatic deletion is unavailable in this build until the native one-shot reviewed-consent capability is available. Connect IMAP for read-only verification.</p><p>This build only gives manual directions. It does not delete app messages. You are responsible. Check the original app and delete each message yourself.</p></div></details><details class="privacy-technical settings-disclosure"><summary>Privacy and technical details</summary><div class="setting-line"><span>Default key expiry</span><strong>${timer}</strong></div><div class="setting-line"><span>Remote app access</span><strong>Blocked</strong></div><div class="setting-line"><span><strong>Windows capture resistance</strong><small>Always applied to OSL’s own window. Cameras, malware, and modified recipients can still capture content.</small></span><strong>${screenshotProtectionEnabled ? "Active" : "Unavailable"}</strong></div><p class="settings-disclosure-sentence">${escapeHtml(oslHandleDiscoveryDisclosureSentence)}</p></details>`;
}

function autoScrubAssistantMarkup(proActive: boolean): string {
  // This is the shipping projection of the tier contract. The open-source
  // build has no optional Pro module, so it must not call Pro "attended" or
  // imply an unattended runner is present when it is not.
  const tier = autoScrubTierStatus(proActive ? "pro" : "free", false);
  const autoScrubPlan = tier.tier === "pro" ? "PRO MODULE NOT INSTALLED" : "FREE · REVIEWED ONE-TIME FLOW";
  const status = projectAutoScrubFleetStatus(autoScrubFleetStatus);
  const actions = status.stopAvailable
    ? `<button class="button compact" id="autoscrub-stop" type="button" ${autoScrubStopPending ? "disabled" : ""}>${autoScrubStopPending ? "Stopping…" : "Stop"}</button>`
    : `<button class="button compact" id="autoscrub-refresh" type="button" ${autoScrubStatusLoading ? "disabled" : ""}>${autoScrubStatusLoading ? "Checking…" : status.label}</button>`;
  return `<details class="settings-disclosure autoscrub-disclosure"><summary><span><strong>AutoScrub assistant</strong><small>${autoScrubPlan}</small></span></summary><section class="autoscrub-card autoscrub-status-${status.tone}" aria-disabled="${status.stopAvailable ? "false" : "true"}"><header><div><span class="privacy-local-mark">LOCAL REVIEW</span><h3>${escapeHtml(status.label)}</h3></div>${actions}</header><p>${escapeHtml(tier.detail)} ${escapeHtml(status.detail)}</p><details><summary>Automation risks</summary><p>Future paced actions must stop on limits, challenges, changed content, or failed checks. Automation may break an app’s rules or restrict an account. Treat removal as unconfirmed until the app shows it is gone.</p></details></section></details>`;
}

function clearPrivacyScanState(): void {
  privacyScanResult = null;
  privacyScanFileName = null;
  persistedLocalScrubImportId = null;
  selectedScrubFindings.clear();
  scrubResultsPage = 0;
  scrubReviewOpen = false;
  scrubReviewPage = 0;
  scrubScopeFingerprint = null;
}

async function clearPrivacyScanResults(): Promise<void> {
  const importId = persistedLocalScrubImportId;
  try {
    const cleared = await clearPersistedLocalScrubExport(importId);
    if (!cleared.confirmedCleared) {
      showToast(cleared.detail);
      return;
    }
    clearPrivacyScanState();
    render();
  } catch (failure) {
    showToast(localActionError(failure, "Local Scrub index cleanup was not confirmed"));
  }
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
  const reviewRows = scrubReviewRowsMarkup(buildScrubReviewList(matching.map(({ finding }) => finding)));
  return `<section class="privacy-results" aria-live="polite"><header><div><strong>${matching.length} ${matching.length === 1 ? "suggestion" : "suggestions"}</strong><small>${privacyScanResult.messagesScanned} messages scanned${privacyScanFileName ? ` · ${escapeHtml(privacyScanFileName)}` : ""}</small></div><span class="privacy-local-mark">LOCAL · ENCRYPTED</span></header>${scrubScopeFingerprintMarkup()}${selectionControls}${items || `<div class="empty-state"><strong>No suggestions in the categories you chose</strong><p>OSL can miss things. Review important chats yourself too.</p></div>`}${pagination}${reviewRows}${items ? `<footer class="scrub-review-footer"><span>${selected} selected</span><button class="button" id="review-scrub-selection" type="button" ${selected ? "" : "disabled"}>Review selected</button></footer>` : ""}</section>`;
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
  // The canonical Scrub route (the discovery console). The legacy wizard
  // selectors below serve only the Settings "scrub" section now.
  const discoveryRoot = document.querySelector<HTMLElement>("#scrub-discovery");
  if (discoveryRoot) {
    bindScrubDiscoveryScreen(discoveryRoot, {
      invoke: (command, payload) => invoke(command, payload),
      requestRender: render,
      proActive: licenseState.access === "pro" || licenseState.access === "offlineGrace",
    });
  }
  document.querySelector<HTMLInputElement>(".scrub-consent-gate input[type=checkbox]")?.addEventListener("change", (event) => {
    localScrubConsentState = { ...localScrubConsentState, checked: (event.currentTarget as HTMLInputElement).checked };
    render();
  });
  document.querySelector<HTMLInputElement>("#scrub-consent-acknowledgement")?.addEventListener("input", (event) => {
    localScrubConsentState = { ...localScrubConsentState, typedAcknowledgement: (event.currentTarget as HTMLInputElement).value };
    render();
  });
  document.querySelector<HTMLButtonElement>(".scrub-consent-proceed")?.addEventListener("click", () => {
    if (!evaluateScrubConsentGate(localScrubConsentRequest, localScrubConsentState).allowed) return;
    localScrubRouteOpened = true;
    render();
  });
  document.querySelectorAll<HTMLInputElement>("[name=scrub-account]").forEach((input) => input.addEventListener("change", () => {
    localScrubRouteAccountSelected = input.checked;
    localScrubRouteStep = "choose";
    render();
  }));
  document.querySelectorAll<HTMLInputElement>("[name=scrub-category]").forEach((input) => input.addEventListener("change", () => {
    const category = input.value as ScrubSignalGroup;
    if (!defaultScrubSignalGroups.includes(category)) return;
    if (input.checked) localScrubRouteCategories.add(category); else localScrubRouteCategories.delete(category);
    localScrubRouteStep = "choose";
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-route-next]").forEach((button) => button.addEventListener("click", () => {
    localScrubRouteStep = button.dataset.scrubRouteNext as ScrubRouteStep;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-scrub-route-back]").forEach((button) => button.addEventListener("click", () => {
    localScrubRouteStep = button.dataset.scrubRouteBack as ScrubRouteStep;
    render();
  }));
  document.querySelector<HTMLButtonElement>("[data-scrub-route-scan]")?.addEventListener("click", () => {
    if (!evaluateScrubConsentGate(localScrubConsentRequest, localScrubConsentState).allowed) return;
    document.querySelector<HTMLInputElement>("#privacy-export-input")?.click();
  });
  document.querySelectorAll<HTMLInputElement>("[data-scrub-category]").forEach((input) => input.addEventListener("change", () => {
    const group = input.dataset.scrubCategory as ScrubSignalGroup;
    if (!defaultScrubSignalGroups.includes(group)) return;
    if (input.checked) enabledScrubSignals.add(group); else enabledScrubSignals.delete(group);
    localStorage.setItem(scrubSignalsStorageKey, JSON.stringify([...enabledScrubSignals]));
    selectedScrubFindings.clear();
    scrubResultsPage = 0;
    scrubReviewOpen = false;
    render();
    void refreshScrubScopeFingerprint();
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
  const muted = [...oslChatMutedPeople].flatMap((personId) => {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    return person ? [`<div class="setting-line"><span><strong>${escapeHtml(person.alias ?? "Verified friend")}</strong><small>Messages still arrive without a local alert.</small></span><button class="button compact" data-osl-chat-unmute="${escapeHtml(personId)}" type="button">Unmute</button></div>`] : [];
  }).join("");
  const previewsChecked = chatPreviewHidingVisible(oslChatPreviewsVisible);
  const previewText = "Hide message previews on this device.";
  const mutedDetails = muted ? `<details class="settings-disclosure" open><summary><span><strong>Muted OSL Chats</strong><small>${oslChatMutedPeople.size.toLocaleString("en-US")} muted</small></span></summary><div class="settings-list">${muted}</div></details>` : "";
  return `<section class="settings-list osl-chat-notification-settings" aria-label="OSL Chat controls"><label class="setting-line interactive"><span><strong>Encrypted chat alerts</strong><small>New-message activity from unmuted OSL friends.</small></span><input id="notification-chat-activity" type="checkbox" ${notificationChatActivity ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>OSL Chat previews</strong><small>${previewText}</small></span><input id="osl-chat-preview-toggle" type="checkbox" ${previewsChecked ? "checked" : ""}/></label></section>${mutedDetails}`;
}

function setNotificationAppPreference(id: ServiceId, enabled: boolean): void {
  notificationAppPreferences[id] = enabled;
  localStorage.setItem(notificationAppsStorageKey, JSON.stringify(notificationAppPreferences));
}

function visibleAppNotifications(): AppNotification[] {
  return (appNotifications ?? []).filter((item) => {
    if (item.appId && notificationAppPreferences[item.appId] === false) return false;
    return isPersistedOslChatNotification(item) ? notificationChatActivity : notificationSecurityActivity;
  });
}

type IdentityStorageProtection = "device" | "fallback" | "unknown";

/**
 * Classify a raw sealer method label (see the METHOD_* constants in
 * crates/keystore/src/sealer.rs — "tpm-pcp", "keyring", "noop-insecure",
 * "memory-ephemeral", "memory-test") into the three states the UI can
 * honestly show.
 *
 * Fail honest, not optimistic: only the two labels that name a persistent
 * platform-provided store count as "device". Every other non-null label — a
 * known software fallback, or a future label OSL does not recognize yet — is
 * "fallback", never silently treated as secure. `null` (nothing learned this
 * session, e.g. a plain unlock of a pre-existing identity, which the backend
 * does not echo a method for) is "unknown", which the UI renders with the same
 * not-secure weight as "fallback" — an unverified state must never render
 * as secure.
 *
 * This tier is deliberately NOT called "hardware". Only "tpm-pcp" is hardware.
 * "keyring" is whatever `keyring` 3.x resolved to for the target: Windows
 * Credential Manager, macOS Keychain, or — on Linux, the feature this repo
 * actually enables (`linux-native` => the `linux-keyutils` crate, see
 * crates/keystore/Cargo.toml) — the kernel keyring, which is ordinary kernel
 * memory with no hardware root of trust and is cleared by a reboot. One label
 * covers all of them, so the UI cannot tell them apart and must not claim the
 * strongest one. Until keystore emits a per-backend label, the honest claim is
 * the one both ends of the range support: the platform is holding the key, not
 * OSL.
 */
function classifyIdentityStorageProtection(method: string | null): IdentityStorageProtection {
  if (method === null) return "unknown";
  if (method === "tpm-pcp" || method === "keyring") return "device";
  return "fallback";
}

function identityStorageProtectionMarkup(protection: IdentityStorageProtection): string {
  if (protection === "device") {
    return `<div class="storage-protection-status secure" role="status"><strong>Protected by this device</strong><small>Your identity key is held by this device's TPM or operating-system credential store, not by OSL.</small></div>`;
  }
  if (protection === "fallback") {
    return `<div class="storage-protection-status insecure" role="alert"><strong>Software fallback storage</strong><small>Hardware protection is unavailable on this device. Your identity key is protected by software only and will not survive a restart.</small></div>`;
  }
  return `<div class="storage-protection-status insecure" role="alert"><strong>Storage protection unknown</strong><small>OSL has not verified hardware-backed storage for this identity in this session. Treat it as not securely stored until verified.</small></div>`;
}

function identitySettingsContent(): string {
  // A list that was never loaded (startup skipped it because the route was
  // still onboarding) is loaded from the surface that shows it. A refused load
  // is NOT retried here: it settles on "unavailable", whose own button asks
  // again, so a failing backend cannot drive render -> refresh -> render.
  if (!runningUnderVitest && accountUnlocked() && hubIdentitiesLoad === "pending") void refreshIdentitySlots(true);
  const identities = identityListMarkup();
  const recovery = newIdentityRecoveryPhrase
    ? recoveryCaptureGate.canRender()
      ? `<div class="warning recovery-secret"><strong>Save the new identity recovery phrase now</strong><code>${escapeHtml(newIdentityRecoveryPhrase)}</code><p>Visible only on this page. It clears if you leave or hide OSL.</p></div>`
      : `<div class="warning recovery-secret" role="alert"><strong>Recovery phrase hidden</strong><p>${RECOVERY_PROTECTION_REFUSAL}.</p><button class="button compact" id="retry-recovery-protection" type="button">Retry protection</button></div>`
    : "";
  const messageRecovery = forwardSecrecyMode === "protectPast"
    ? "Protect past messages. Restart begins a fresh chain and late messages are lost."
    : "Keep group delivery as today. A persisted snapshot can recover prior message keys.";
  return `<h2>Account</h2><p>One active identity on this device.</p>${identityStorageProtectionMarkup(classifyIdentityStorageProtection(identityStorageMethod))}<div class="identity-list">${identities}</div>${publicNamePage.render()}<div class="setting-line"><span><strong>Message recovery</strong><small>${messageRecovery}</small></span>${statusTag(forwardSecrecyMode === "protectPast" ? "Protect past" : "Keep delivery")}</div>${recovery}<form class="inline-form identity-create-form" id="identity-slot-form"><input id="identity-slot-label" maxlength="80" placeholder="New identity label" required/><button class="button primary">Create identity</button></form><details class="recovery-import settings-disclosure"><summary>Recover another identity</summary><form id="identity-recover-form" class="setup-surface"><input id="identity-recover-label" maxlength="80" placeholder="Identity label" required/><textarea id="identity-recover-phrase" rows="3" placeholder="12-word recovery phrase" required></textarea><button class="button">Recover identity</button></form></details>${activationSettingsContent()}`;
}

/**
 * Whether OSL is locked is answered by `core.readiness` — the same value the
 * "Password configured and unlocked" indicator reads. The identity list must
 * answer it from there too; inferring a lock from an empty array made Settings
 * tell the user to unlock a session that was already unlocked.
 */
function identityListState(): "locked" | "loading" | "unavailable" | "empty" | "list" {
  if (!accountUnlocked()) return "locked";
  if (hubIdentities.length) return "list";
  if (hubIdentitiesLoad === "pending") return "loading";
  if (hubIdentitiesLoad === "unavailable") return "unavailable";
  return "empty";
}

export const IDENTITY_LIST_UNAVAILABLE =
  "OSL is unlocked, but this device did not return a usable identity registry. Nothing was changed.";

function identityListMarkup(): string {
  const state = identityListState();
  if (state === "list") {
    return hubIdentities.map((identity) => `<article class="identity-row"><div><strong>${escapeHtml(identity.label)}</strong><small>${escapeHtml(identity.oslUserId)}</small></div>${identity.active ? `${statusTag("Active")}` : `<button class="button compact" data-switch-identity="${escapeHtml(identity.slotId)}">Switch</button>`}</article>`).join("");
  }
  if (state === "locked") {
    return `<div class="empty-state" data-identity-list="locked"><strong>Identity list locked</strong><p>Unlock OSL to manage encrypted identity slots.</p></div>`;
  }
  if (state === "loading") {
    return `<div class="empty-state" data-identity-list="loading"><strong>Reading identity slots</strong><p>OSL is opening the encrypted identity registry on this device.</p></div>`;
  }
  if (state === "unavailable") {
    // NEW-3: this screen held the answer and threw it away. The native side
    // refuses with a specific sentence -- "OSL main password must be
    // unlocked", "OSL identity migration failed: ...", "OSL identity registry
    // is unavailable" -- and the journal already has it, bounded by age and by
    // command. Repeating it verbatim is the difference between a dead end and
    // a diagnosis, and `withBackendReason` never invents detail the backend
    // chose not to give.
    const reason = withBackendReason(IDENTITY_LIST_UNAVAILABLE, "list_hub_identities");
    return `<div class="empty-state" data-identity-list="unavailable"><strong>Identity list could not be read</strong><p>${escapeHtml(reason)}</p><button class="button compact" id="retry-identity-list" type="button">Try again</button></div>`;
  }
  return `<div class="empty-state" data-identity-list="empty"><strong>No identity slots yet</strong><p>Create an identity below to start using OSL on this device.</p></div>`;
}

function activationSettingsContent(): string {
  if (discordQaShell) return "";
  const entitlement = entitlementView(licenseState, Math.floor(Date.now() / 1_000));
  const copy = entitlementCopy(entitlement);
  const pro = entitlement.tier === "pro" || entitlement.tier === "offlineGrace";
  const moduleAccess = pro
    ? "Optional Pro module: separately installed and licensed on this device."
    : "Optional Pro module: separate install and license required; base OSL stays available.";
  const clear = licenseState.status === "UNCONFIGURED" ? "" : `<button class="button compact" id="clear-activation-code" type="button">Clear activation</button>`;
  return `<details class="license-card settings-disclosure"><summary><span><strong>Plan</strong><small>${escapeHtml(copy.title)}</small></span>${statusTag(escapeHtml(licenseState.status === "UNCONFIGURED" ? "Free" : licenseState.status), pro ? "active" : "")}</summary><div data-entitlement-banner="${entitlement.banner}" data-entitlement-cta="${entitlement.cta}"><p>${escapeHtml(copy.detail)}</p><p>Paste the activation code shown after checkout. No email is required.</p><p class="quiet-note">${moduleAccess}</p><form id="activation-form" class="license-form"><label for="activation-code">Activation code</label><div><input id="activation-code" inputmode="text" maxlength="23" autocomplete="off" autocapitalize="characters" spellcheck="false" placeholder="OSL-XXXX-XXXX-XXXX-XXXX" required/><button class="button primary" type="submit">Activate Pro</button>${clear}</div></form></div></details>`;
}

function appearanceSettingsContent(): string {
  const theme = `<div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong>${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>`;
  const profile = appearanceProfileState.record("global");
  if (!profile) throw new Error("Appearance profile draft is missing");
  const colours = loadAppearanceColours(localStorage);
  const connectedServices: AppearancePreviewService[] = services
    .filter((service) => service.accounts.some((account) => account.state === "demoLinked"))
    .map((service) => ({ id: service.id, label: service.displayName, icon: serviceLogo(service.id) }));
  const profilePreview = `<section class="appearance-layout"><div class="appearance-controls">${appearanceColourRowsMarkup(colours)}<div data-settings-profile-block>${settingsProfileBlockMarkup(profile)}</div></div>${appearancePreviewMarkup(profile, colours, connectedServices)}</section>`;
  return `${lookScreenMarkup(lookState)}${appearanceSettingsMarkup(appearancePreferences)}<section class="appearance-theme"><h3>Theme</h3>${theme}</section>${profilePreview}`;
}

function previewLook(next: LookState): void {
  lookState = next;
  themeChoice = parseTheme(next.mode === "computer" ? "system" : next.mode);
  applyTheme(themeChoice);
  render();
}

function persistLook(): void {
  saveLookState(localStorage, lookState);
  localStorage.setItem(themeStorageKey, themeChoice);
  render();
}

function lookMode(value: string | undefined): LookMode | null {
  return value === "light" || value === "dark" || value === "computer" ? value : null;
}

function developerSettingsContent(): string {
  return `<details class="settings-disclosure developer-source"><summary>Developer source</summary><p>Open the fixed OSL repository through the trusted desktop command.</p><button class="button" data-source-repository type="button">Open source repository</button></details>`;
}

async function prepareServiceBurn(): Promise<void> {
  const target = activeServiceContextTarget();
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
  const saveReview = (): void => {
    void saveBurnReviewState(
      burnReviewScreenState.selectedSide,
      "chat:active",
      burnReviewScreenState.hideOtherPeople,
    );
  };
  document.querySelectorAll<HTMLButtonElement>("[data-burn-review-side]").forEach((button) => button.addEventListener("click", () => {
    const side = button.dataset.burnReviewSide as BurnReviewSide;
    if (side !== "your_side" && side !== "their_side" && side !== "both_sides") return;
    burnReviewScreenState = selectBurnReviewSide(burnReviewScreenState, side);
    saveReview();
    render();
  }));
  document.querySelector<HTMLInputElement>("#burn-review-hide-other-people")?.addEventListener("change", () => {
    burnReviewScreenState = toggleBurnReviewHideOtherPeople(burnReviewScreenState);
    saveReview();
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-burn-review-server-choice]").forEach((button) => button.addEventListener("click", () => {
    const choice = button.dataset.burnReviewServerChoice;
    if (choice !== "this_channel" && choice !== "whole_server") return;
    void saveBurnReviewState(choice, "chat:active", burnReviewScreenState.hideOtherPeople);
  }));
  document.querySelector<HTMLButtonElement>("#burn-review-back")?.addEventListener("click", () => {
    void backBurnReview();
    closeBurnDialog();
  });
  const acknowledgement = document.querySelector<HTMLInputElement>("#burn-confirm-ack");
  const submit = document.querySelector<HTMLButtonElement>("#burn-confirm-submit");
  const validate = (): void => {
    if (!acknowledgement || !submit) return;
    submit.disabled = burnBusy || !acknowledgement.checked || burnScopeReason(burnScope) !== null;
  };
  acknowledgement?.addEventListener("change", validate);
  document.querySelector<HTMLFormElement>("#burn-confirm-form")?.addEventListener("submit", (event) => void executeBurn(event));
}

function closeOwnedConfirmation(): void {
  ownedConfirmation = null;
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  render();
}

function closeTopmostClosableLayerForEscape(): string | null {
  if (ownedConfirmation) {
    closeOwnedConfirmation();
    return "owned-confirmation-dialog";
  }
  if (burnDialogOpen) {
    closeBurnDialog();
    return "burn-dialog";
  }
  if (scrubReviewOpen) {
    scrubReviewOpen = false;
    render();
    return "scrub-review-dialog";
  }
  if (whitelistRosterOpen) {
    whitelistRosterOpen = false;
    render();
    return "whitelist-roster-dialog";
  }
  if (nativeProtectPickerOpen) {
    nativeProtectPickerOpen = false;
    render();
    return "native-protect-friend-dialog";
  }
  if (oslChatSettingsPersonId) {
    oslChatSettingsPersonId = null;
    render();
    return "osl-chat-settings-dialog";
  }
  if (friendsDialogOpen) {
    friendsDialogOpen = false;
    friendsDialogPage = 0;
    render();
    return "friends-dialog";
  }
  const dialog = document.querySelector<HTMLDialogElement>("dialog[open]");
  if (dialog) {
    dialog.close();
    return dialog.id || "dialog";
  }
  return null;
}

function bindOwnedConfirmation(): void {
  if (!ownedConfirmation) return;
  document.querySelectorAll<HTMLButtonElement>("[data-close-owned-confirmation]").forEach((button) => button.addEventListener("click", closeOwnedConfirmation));
  const dialog = document.querySelector<HTMLDialogElement>("#owned-confirmation-dialog");
  dialog?.addEventListener("cancel", (event) => { event.preventDefault(); closeOwnedConfirmation(); });
  dialog?.addEventListener("close", () => { if (ownedConfirmation) closeOwnedConfirmation(); });
  const submit = document.querySelector<HTMLButtonElement>("#owned-confirmation-submit");
  // No `input` listener gates this button. Its state is a function of whether a
  // submit is in flight; what the field holds is read at submit time, so a value
  // that arrived by paste or programmatic fill works exactly like a typed one.
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
  if (!activeServiceContextTarget()) {
    showToast("Choose one connected account first.");
    return;
  }
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
  protectedSheetMode = activeEmbeddedHost ? "peer" : "local";
  peerProtectedSheet = blankPeerProtectedModel(activeEmbeddedHost !== null);
  localProtectedSheet = blankLocalProtectedModel(activeEmbeddedHost === null);
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

if (!runningUnderVitest && !fixedNoRecoverySecretFixture) {
  bindAttachmentProgressEvents();
  bindAttachmentDeletionEvents();
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

if (!runningUnderVitest && !fixedNoRecoverySecretFixture) {
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
  peerProtectedSheet.status = peerProtectedSheet.handshakeConfirmed
    ? "Protected text is ready. Your draft stays here until you send."
    : "Protected text is ready, but it is not readable for them until they finish their half. Your draft stays here until you send.";
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
  // Their message decrypted here, which is the only local proof that they
  // added this identity, verified it and approved this app + friend.
  peerProtectedSheet.handshakeConfirmed = true;
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
  const input = document.querySelector<HTMLInputElement>("#local-chat-label");
  const label = input?.value.trim() ?? "";
  await startLocalProtectedContextForLabel(label);
}

async function startLocalProtectedContextForLabel(label: string): Promise<void> {
  if (localProtectedSheet.busy) return;
  if (!validLocalChatLabel(label)) {
    localProtectedSheet.status = "Use a short chat name.";
    render();
    return;
  }
  const contextTarget = activeServiceContextTarget();
  if (!contextTarget) {
    localProtectedSheet.status = "Choose one connected account first.";
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
      contextTarget.serviceId,
      contextTarget.accountId,
    );
    const context = await activateLocalLoopbackContext(
      contextTarget.serviceId,
      contextTarget.accountId,
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
  const draft = document.querySelector<HTMLTextAreaElement>("#local-protected-draft");
  const ttl = document.querySelector<HTMLSelectElement>("#local-protected-ttl");
  const viewOnce = document.querySelector<HTMLInputElement>("#local-protected-view-once");
  const plaintext = draft?.value ?? "";
  const ttlSeconds = Number(ttl?.value ?? 3_600);
  await prepareLocalProtectedDraftFromValues(plaintext, ttlSeconds, viewOnce?.checked === true);
}

async function prepareLocalProtectedDraftFromValues(
  plaintext: string,
  ttlSeconds: number,
  viewOnce: boolean,
): Promise<void> {
  const contextToken = localProtectedSheet.context?.contextToken;
  if (!contextToken || !plaintext.trim() || !isLocalTtlSeconds(ttlSeconds)) {
    localProtectedSheet.status = "Write a message first.";
    render();
    return;
  }
  const sendContext = localProtectedSheet.context;
  if (needsRiskAcceptance(setup.sendMode)
    && (!setup.acceptedRisk || setup.acceptedRiskForMode !== setup.sendMode)) {
    localProtectedSheet.draft = plaintext;
    localProtectedSheet.status = `${formatSendMode(setup.sendMode)} requires experimental send risk acknowledgement before OSL prepares a fallback. Nothing was sent.`;
    render();
    return;
  }
  if (needsRiskAcceptance(setup.sendMode)
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
  localProtectedSheet.viewOnce = viewOnce;
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
  if (setup.sendMode === "manual") {
    localProtectedSheet.status = "Encrypted and ready for manual placement. Clipboard was not changed.";
    render();
    return;
  }
  try {
    await navigator.clipboard.writeText(prepared.capsule);
    localProtectedSheet.status = needsRiskAcceptance(setup.sendMode)
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
  bindProtectedTextBoxShortcutGuards(document);
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
    // Verification is checked again at the send boundary. Keep the arrow
    // clickable for a non-verified chat so it can explain the refusal in the
    // blocked panel instead of looking like a silent, inert control.
    send.disabled = !(hasDraft && withinLimit);
  }
  const count = document.querySelector<HTMLOutputElement>("#osl-chat-draft-count");
  if (count) {
    count.textContent = `${bytes.toLocaleString("en-US")} / ${OSL_CHAT_MAX_DRAFT_BYTES.toLocaleString("en-US")}`;
    count.classList.toggle("is-over", !withinLimit);
  }
}

function setOslChatDraft(nextDraft: string, syncElement = true): void {
  oslChatDraft = nextDraft;
  if (syncElement) applyOslChatDraftToElement(document.querySelector<HTMLTextAreaElement>("#osl-chat-draft"), nextDraft);
  syncOslChatComposer();
}

function startSomethingPeople(): StartSomethingPerson[] {
  return hubPeople.filter(peerIsVerified).map((person) => ({
    personId: person.personId,
    oslUserId: person.oslUserId,
    name: person.alias ?? "Verified friend",
  }));
}

function activeStartSomethingIdentity(): string {
  return core.readiness.activeOslUserId ?? hubIdentities.find((identity) => identity.active)?.oslUserId ?? "";
}

function selectedStartSomethingPeople(form: HTMLFormElement): string[] {
  return [...form.querySelectorAll<HTMLInputElement>("[data-start-something-person]:checked")]
    .map((input) => input.dataset.startSomethingPerson ?? "")
    .filter(Boolean);
}

function localStartSomethingDependencies(acceptedTarget?: StartSomethingPerson): StartSomethingDependencies {
  return {
    acceptDirectTarget: async () => acceptedTarget ?? null,
    // The records returned here are intentionally narrow UI receipts. The
    // native direct/group creation commands own durable membership; this
    // surface only needs the exact resulting member list before it opens it.
    createDirectConversation: async (_creator, memberIds) => ({ conversationId: `direct-${memberIds.join("-")}`, memberIds }),
    createGroupConversation: async (_name, memberIds) => ({ groupId: `group-${memberIds.join("-")}`, memberIds }),
    createEnclave: async (_name, memberIds, joiningRule) => ({ enclaveId: crypto.randomUUID().replace(/-/gu, ""), memberIds, joiningRule }),
  };
}

async function submitStartSomethingDirect(form: HTMLFormElement): Promise<void> {
  const target = (form.elements.namedItem("target") as HTMLInputElement | null)?.value ?? "";
  const before = new Set(hubPeople.map((person) => person.personId));
  const username = isNormalizedOslUsername(target.trim());
  const added = username
    ? await addOslFriendByUsername(target.trim())
    : await addOslFriend(target.trim());
  if (!added || ("added" in added && !added.added)) throw new Error("Their invite or username was not accepted");
  hubPeople = await listHubPeople() ?? hubPeople;
  const personId = username && "personId" in added
    ? added.personId
    : hubPeople.find((person) => !before.has(person.personId))?.personId;
  const person = hubPeople.find((candidate) => candidate.personId === personId);
  if (!person) throw new Error("Their invite was accepted but did not create one person");
  const created = await startDirectConversation(target, activeStartSomethingIdentity(), localStartSomethingDependencies({ personId: person.personId, oslUserId: person.oslUserId, name: person.alias ?? "Verified friend" }));
  if (created.memberIds.length !== 2) throw new Error("Direct conversation was not created");
  startSomethingChoice = null;
  if (peerIsVerified(person)) await openOslChat(person.personId); else { route = "osl-chat"; render(); }
  showToast("Direct message created");
}

async function submitStartSomethingGroup(form: HTMLFormElement): Promise<void> {
  const name = (form.elements.namedItem("name") as HTMLInputElement | null)?.value ?? "";
  const created = await startGroupConversation(name, activeStartSomethingIdentity(), startSomethingPeople(), selectedStartSomethingPeople(form), localStartSomethingDependencies());
  startSomethingChoice = null;
  render();
  showToast(`Group created with ${created.memberIds.length} members`);
}

async function submitStartSomethingEnclave(form: HTMLFormElement): Promise<void> {
  const name = (form.elements.namedItem("name") as HTMLInputElement | null)?.value ?? "";
  const joiningRule = (form.elements.namedItem("joiningRule") as HTMLSelectElement | null)?.value as EnclaveJoiningRule;
  const people = startSomethingPeople();
  const selected = selectedStartSomethingPeople(form);
  const created = await startEnclave(name, activeStartSomethingIdentity(), people, selected, joiningRule, localStartSomethingDependencies());
  privateEnclaveAudiences = [...privateEnclaveAudiences, {
    audienceId: created.enclaveId,
    name: name.trim(),
    memberCount: created.memberIds.length,
    membershipVisibility: "visible",
    visibleMembers: people.filter((person) => selected.includes(person.personId)).map((person) => ({ memberId: person.personId, name: person.name, verified: true })),
    canPost: true,
    refusal: null,
  }];
  startSomethingJoiningRule = created.joiningRule;
  startSomethingChoice = null;
  render();
  showToast(`Enclave created · ${created.joiningRule}`);
}

async function submitStartSomething(form: HTMLFormElement, path: "direct" | "group" | "enclave"): Promise<void> {
  if (startSomethingBusy) return;
  startSomethingBusy = true;
  render();
  try {
    if (path === "direct") await submitStartSomethingDirect(form);
    else if (path === "group") await submitStartSomethingGroup(form);
    else await submitStartSomethingEnclave(form);
  } catch (error) {
    showToast(error instanceof Error ? error.message : "Could not start something");
  } finally {
    startSomethingBusy = false;
  }
}

function bindWorkspace(): void {
  bindPasswordVisibility();
  bindLocalProtectedSheet();
  bindSavedAccountControls();
  bindDiscoveryVisibilityControls();
  document.querySelectorAll<HTMLInputElement>('input[name="window-position"]').forEach((input) => input.addEventListener("change", () => {
    if (!input.checked) return;
    windowSoundsSettings = { ...windowSoundsSettings, position: input.value as WindowPosition };
    saveWindowSoundsSettings(windowSoundsSettings);
    render();
  }));
  (Object.keys(defaultWindowSoundsSettings) as Array<keyof WindowSoundsSettings>).filter((key) => key !== "position").forEach((key) => {
    document.querySelector<HTMLInputElement>(`#window-sound-${key}`)?.addEventListener("change", (event) => {
      windowSoundsSettings = { ...windowSoundsSettings, [key]: (event.currentTarget as HTMLInputElement).checked };
      saveWindowSoundsSettings(windowSoundsSettings);
      render();
    });
  });
  document.querySelector<HTMLButtonElement>("#reset-window-sounds")?.addEventListener("click", () => {
    windowSoundsSettings = { ...defaultWindowSoundsSettings };
    saveWindowSoundsSettings(windowSoundsSettings);
    render();
  });
  document.querySelectorAll<HTMLInputElement>("[data-future-account-toggle]").forEach((input) => {
    input.addEventListener("change", (event) => void changeFriendFutureAccountSwitch(event.currentTarget as HTMLInputElement));
  });
  document.querySelector<HTMLButtonElement>("#start-something-pencil, [data-osl-chat-new]")?.addEventListener("click", () => {
    startSomethingChoice = "direct";
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-start-something-choice]").forEach((button) => button.addEventListener("click", () => {
    startSomethingChoice = button.dataset.startSomethingChoice as "direct" | "group" | "enclave";
    render();
  }));
  document.querySelector<HTMLButtonElement>("[data-start-something-close]")?.addEventListener("click", () => { startSomethingChoice = null; render(); });
  document.querySelector<HTMLButtonElement>("[data-start-something-copy-invite]")?.addEventListener("click", () => void copyFriendInvite());
  document.querySelector<HTMLFormElement>("[data-start-something-direct]")?.addEventListener("submit", (event) => { event.preventDefault(); void submitStartSomething(event.currentTarget as HTMLFormElement, "direct"); });
  document.querySelector<HTMLFormElement>("[data-start-something-group]")?.addEventListener("submit", (event) => { event.preventDefault(); void submitStartSomething(event.currentTarget as HTMLFormElement, "group"); });
  document.querySelector<HTMLFormElement>("[data-start-something-enclave]")?.addEventListener("submit", (event) => { event.preventDefault(); void submitStartSomething(event.currentTarget as HTMLFormElement, "enclave"); });
  const appearanceRoot = document.querySelector<HTMLElement>(".appearance-layout");
  const appearanceProfileMount = document.querySelector<HTMLElement>("[data-settings-profile-block]");
  const previewServices = (): AppearancePreviewService[] => services
    .filter((service) => service.accounts.some((account) => account.state === "demoLinked"))
    .map((service) => ({ id: service.id, label: service.displayName, icon: serviceLogo(service.id) }));
  const refreshAppearancePreview = (): void => {
    updateAppearancePreview(
      document.querySelector<HTMLElement>("[data-appearance-preview]"),
      appearanceProfileState.record("global") as ScopedProfileRecord,
      loadAppearanceColours(localStorage),
      previewServices(),
    );
  };
  if (appearanceRoot) bindAppearanceColourRows(appearanceRoot, localStorage, () => refreshAppearancePreview());
  if (appearanceProfileMount) attachSettingsProfileBlock(appearanceProfileMount, appearanceProfileState, { onProfileChange: refreshAppearancePreview });
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-open]").forEach((button) => button.addEventListener("click", () => {
    void openOslChat(button.dataset.oslChatOpen ?? "");
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-settings]").forEach((button) => button.addEventListener("click", () => {
    oslChatSettingsPersonId = button.dataset.oslChatSettings ?? null;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-filter]").forEach((button) => button.addEventListener("click", () => {
    const filter = button.dataset.oslChatFilter;
    if (filter === "direct" || filter === "groups" || filter === "enclaves") {
      oslChatFilter = filter;
      render();
    }
  }));
  document.querySelector<HTMLInputElement>("#osl-chat-search")?.addEventListener("input", (event) => {
    oslChatSearch = (event.currentTarget as HTMLInputElement).value;
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-osl-chat-blocked-close]")?.addEventListener("click", () => {
    oslChatSendBlockedReason = null;
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-osl-chat-profile]")?.addEventListener("click", () => {
    chatAppearancePane = "profile";
    chatProfileAppearanceOpen = true;
    render();
  });
  document.querySelector<HTMLButtonElement>(".osl-chat-emoji")?.addEventListener("click", () => {
    setOslChatDraft(`${oslChatDraft}🙂`);
    document.querySelector<HTMLTextAreaElement>("#osl-chat-draft")?.focus();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-open-chat-profile-appearance]").forEach((button) => button.addEventListener("click", () => {
    chatAppearancePane = "profile";
    chatProfileAppearanceOpen = true;
    render();
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-start-something]").forEach((button) => button.addEventListener("click", () => inboxPrimaryAction()));
  document.querySelectorAll<HTMLButtonElement>("[data-open-safety-number]").forEach((button) => button.addEventListener("click", () => openSafetyNumberPanel(button.dataset.openSafetyNumber ?? "")));
  document.querySelectorAll<HTMLButtonElement>("[data-close-safety-number]").forEach((button) => button.addEventListener("click", () => { safetyNumberPanelPersonId = null; render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-friend-settings]").forEach((button) => button.addEventListener("click", () => {
    route = "home";
    friendsDialogOpen = true;
    friendsDialogPage = Math.max(0, Math.floor(Math.max(0, hubPeople.findIndex((person) => person.personId === (button.dataset.friendSettings ?? ""))) / friendsDialogPageSize));
    render();
  }));
  const oslChatSettingsDialog = document.querySelector<HTMLDialogElement>("#osl-chat-settings-dialog");
  if (oslChatSettingsDialog && !oslChatSettingsDialog.open) oslChatSettingsDialog.showModal();
  document.querySelector<HTMLButtonElement>("#osl-chat-settings-close")?.addEventListener("click", () => { oslChatSettingsPersonId = null; render(); });
  document.querySelectorAll<HTMLInputElement>("[data-osl-chat-notification-switch]").forEach((input) => input.addEventListener("change", (event) => {
    const personId = oslChatSettingsPersonId;
    const key = input.dataset.oslChatNotificationSwitch as OslChatNotificationSwitch | undefined;
    if (!personId || !key) return;
    setOslChatNotificationSwitch(localStorage, personId, key, (event.currentTarget as HTMLInputElement).checked);
    render();
  }));
  document.querySelector<HTMLInputElement>("#osl-chat-mute-toggle")?.addEventListener("change", (event) => {
    const personId = oslChatSettingsPersonId;
    if (!personId) return;
    if ((event.currentTarget as HTMLInputElement).checked) oslChatMutedPeople.add(personId); else oslChatMutedPeople.delete(personId);
    persistOslChatMutedPeople();
    render();
  });
  document.querySelector<HTMLInputElement>("#osl-chat-preview-toggle")?.addEventListener("change", (event) => {
    oslChatPreviewsVisible = (event.currentTarget as HTMLInputElement).checked;
    persistOslChatPreviewVisibility();
    render();
  });
  document.querySelector<HTMLButtonElement>("#osl-chat-permission-toggle")?.addEventListener("click", () => void toggleOslChatPermission());
  document.querySelector<HTMLButtonElement>("#osl-chat-back")?.addEventListener("click", () => void closeOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-refresh")?.addEventListener("click", () => void refreshOslChat());
  document.querySelector<HTMLButtonElement>("#osl-chat-approve")?.addEventListener("click", () => void approveOslChat());
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-reaction]").forEach((button) => button.addEventListener("click", () => {
    void toggleOslChatReaction(
      button.dataset.oslChatReaction ?? "",
      button.dataset.oslChatEmoji ?? "",
      button.dataset.oslChatReactionMine === "true",
    );
  }));
  const oslChatDraftInput = document.querySelector<HTMLTextAreaElement>("#osl-chat-draft");
  oslChatDraftInput?.addEventListener("input", () => {
    // The Send button's disabled state and the byte counter are computed in
    // activeThread() at RENDER time. This listener used to only assign the draft, so
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
    setOslChatDraft(oslChatDraftInput.value, false);
  });
  oslChatDraftInput?.addEventListener("keydown", (event) => {
    if (!submitsOslChatDraft(event)) return;
    event.preventDefault();
    const form = oslChatDraftInput.closest<HTMLFormElement>("[data-osl-chat-compose]");
    const send = form?.querySelector<HTMLButtonElement>("button.osl-chat-send");
    if (!form || !send || send.disabled) return;
    if (typeof form.requestSubmit === "function") form.requestSubmit(send);
    else send.click();
  });
  const oslMailBodyInput = document.querySelector<HTMLTextAreaElement>("#osl-mail-body");
  if (oslMailBodyInput) bindPrivateTypingBoxDropTarget(oslMailBodyInput, oslMailDropTray, render);
  document.querySelector<HTMLInputElement>("#osl-chat-view-once")?.addEventListener("change", (event) => { oslChatViewOnce = (event.currentTarget as HTMLInputElement).checked; });
  document.querySelector<HTMLFormElement>("[data-osl-chat-compose]")?.addEventListener("submit", (event) => void sendOslChat(event));
  document.querySelector<HTMLButtonElement>("#osl-chat-attach")?.addEventListener("click", () => void sendOslChatAttachment());
  const oslChatComposerForm = document.querySelector<HTMLFormElement>("[data-osl-chat-compose]");
  if (oslChatComposerForm) attachOslChatComposerDragAndDrop(oslChatComposerForm, oslChatDropTray, () => render(), (refusal) => showToast(refusal));
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
    // Leaving the Scrub screen clears local scan results, whichever surface
    // it was opened from -- the top-level route or the Settings section.
    if (route === "scrub" || (route === "settings" && settingsSection === "scrub")) clearPrivacyScanState();
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
  // A `[data-service]` click binding used to sit here. Nothing in the repo emits
  // a bare `data-service` attribute (`data-service-kind`, `data-service-account`
  // and `data-service-current-session` are different attributes and have their
  // own handlers), so the selector matched no element and `openServiceRoute` is
  // reached through the `[data-home-app]` launch path instead.
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
    if (next !== "account") removeEverythingScreenOpen = false;
    settingsSection = next;
    render();
    if (next === "scrub") void refreshAutoScrubFleetStatus();
    if (next === "cleanup") void refreshMassCleanupCapabilities();
  }));
  document.querySelector<HTMLButtonElement>("#full-cleanup-button")?.addEventListener("click", () => {
    removeEverythingScreenOpen = true;
    render();
  });
  document.querySelector<HTMLButtonElement>("#remove-everything-cancel")?.addEventListener("click", () => {
    removeEverythingScreenOpen = false;
    render();
  });
  document.querySelector<HTMLButtonElement>("#remove-everything-confirm")?.addEventListener("click", () => {
    // Scope review is complete. The established typed confirmation is still
    // required before the account-wide cleanup command can run.
    removeEverythingScreenOpen = false;
    burnScope = "account";
    burnDialogOpen = true;
    burnResult = null;
    render();
  });
  document.querySelectorAll<HTMLInputElement>('input[name="window-position"]').forEach((input) => input.addEventListener("change", () => {
    if (!input.checked) return;
    windowSoundsSettings = { ...windowSoundsSettings, position: input.value as WindowPosition };
    saveWindowSoundsSettings(windowSoundsSettings);
    render();
  }));
  (Object.keys(defaultWindowSoundsSettings) as Array<keyof WindowSoundsSettings>).filter((key) => key !== "position").forEach((key) => {
    document.querySelector<HTMLInputElement>(`#window-sound-${key}`)?.addEventListener("change", (event) => {
      windowSoundsSettings = { ...windowSoundsSettings, [key]: (event.currentTarget as HTMLInputElement).checked };
      saveWindowSoundsSettings(windowSoundsSettings);
      render();
    });
  });
  document.querySelector<HTMLButtonElement>("#reset-window-sounds")?.addEventListener("click", () => {
    windowSoundsSettings = { ...defaultWindowSoundsSettings };
    saveWindowSoundsSettings(windowSoundsSettings);
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-settings-send-mode]").forEach((button) => button.addEventListener("click", () => {
    void changeSendingMode(button.dataset.settingsSendMode as SendMode);
  }));
  document.querySelectorAll<HTMLInputElement>('input[name="settings-connection-route"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && (input.value === "tor" || input.value === "direct")) void changePrivacyConnectionRoute(input.value);
  }));
  document.querySelectorAll<HTMLInputElement>('input[name="settings-cover-mode"]').forEach((input) => input.addEventListener("change", () => {
    if (input.checked && (input.value === "insert-on-send" || input.value === "type-naturally")) void changePrivacyCoverInsertion(input.value);
  }));
  document.querySelector<HTMLInputElement>("#privacy-incoming-key-warnings")?.addEventListener("change", (event) => {
    notificationSecurityActivity = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(notificationSecurityStorageKey, String(notificationSecurityActivity));
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-settings-friending]")?.addEventListener("click", () => {
    settingsSection = "whitelisting";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-inbox-start-private]")?.addEventListener("click", () => inboxPrimaryAction());
  document.querySelectorAll<HTMLButtonElement>("[data-inbox-filter]").forEach((button) => button.addEventListener("click", () => {
    inboxFilter = parseInboxFilter(button.dataset.inboxFilter);
    render();
  }));
  document.querySelector<HTMLButtonElement>("#osl-mail-retry")?.addEventListener("click", () => void refreshOslMail());
  document.querySelector<HTMLButtonElement>("#osl-mail-provision")?.addEventListener("click", () => void provisionOslMailFromProfile());
  document.querySelectorAll<HTMLButtonElement>("[data-mail-pane]").forEach((button) => button.addEventListener("click", () => {
    oslMailPane = button.dataset.mailPane as OslMailPane;
    render();
  }));
  document.querySelector<HTMLInputElement>("#osl-mail-notifications")?.addEventListener("change", (event) => {
    oslMailNotifications = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(oslMailNotificationsStorageKey, String(oslMailNotifications));
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-mail-thread]").forEach((button) => button.addEventListener("click", async () => {
    const threadId = button.dataset.mailThread ?? "";
    oslMailActiveThread = await retrieveOslMailThread(threadId);
    oslMailError = oslMailActiveThread ? null : "Message retrieval was refused";
    render();
  }));
  document.querySelector<HTMLButtonElement>("#osl-mail-ack")?.addEventListener("click", async () => {
    if (!oslMailActiveThread) return;
    oslMailDeleteReceipt = await acknowledgeOslMailRetrieval(oslMailActiveThread.retrievalId, oslMailActiveThread.messages.map((message) => message.messageId));
    oslMailError = oslMailDeleteReceipt ? null : "Retrieval acknowledgement was refused";
    render();
  });
  document.querySelector<HTMLFormElement>("#osl-mail-compose-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void sendOslMailForm(event.currentTarget as HTMLFormElement, "Enter");
  });
  document.querySelector<HTMLButtonElement>("#osl-mail-send")?.addEventListener("click", (event) => {
    const form = (event.currentTarget as HTMLButtonElement).form;
    if (form) void sendOslMailForm(form, "Send");
  });
  const syncOslMailComposeDraft = (): void => {
    oslMailComposeDraft = {
      to: document.querySelector<HTMLInputElement>("#osl-mail-to")?.value ?? oslMailComposeDraft.to,
      subject: document.querySelector<HTMLInputElement>("#osl-mail-subject")?.value ?? oslMailComposeDraft.subject,
      body: document.querySelector<HTMLTextAreaElement>("#osl-mail-body")?.value ?? oslMailComposeDraft.body,
    };
  };
  document.querySelector<HTMLInputElement>("#osl-mail-to")?.addEventListener("input", syncOslMailComposeDraft);
  document.querySelector<HTMLInputElement>("#osl-mail-subject")?.addEventListener("input", syncOslMailComposeDraft);
  document.querySelector<HTMLTextAreaElement>("#osl-mail-body")?.addEventListener("input", syncOslMailComposeDraft);
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
  document.querySelector<HTMLButtonElement>("#install-mullvad-from-connections")?.addEventListener("click", () => void runMullvadSetupAction("install", "connections"));
  document.querySelector<HTMLButtonElement>("[data-activity-primary-action]")?.addEventListener("click", () => {
    route = "activity";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-autoscrub-home-view-activity]")?.addEventListener("click", (event) => {
    const button = event.currentTarget as HTMLButtonElement;
    autoScrubOpenedActivityRecord = openAutoScrubHomeActivity(button.dataset.autoscrubHomeViewActivity ?? "");
    route = "activity";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-privacy-primary-action]")?.addEventListener("click", () => {
    route = "privacy";
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-change-protection-preset]")?.addEventListener("click", () => {
    route = "onboarding";
    onboardingRoute = "privacy";
    render();
  });
  document.querySelector<HTMLInputElement>("#rn-wire-policy-toggle")?.addEventListener("change", (event) => {
    const previous = rnWirePolicyRequested;
    rnWirePolicyRequested = (event.currentTarget as HTMLInputElement).checked;
    localStorage.setItem(rnWirePolicyStorageKey, String(rnWirePolicyRequested));
    render();
    void saveOnboardingPreferences({ onboardingComplete, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode }).then((saved) => {
      rnWirePolicyRequested = saved.rnWirePolicyRequested;
      render();
    }).catch(() => {
      rnWirePolicyRequested = previous;
      showToast("Message format preference could not be saved");
      render();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-notification-settings]").forEach((button) => button.addEventListener("click", () => { route = "settings"; settingsSection = "notifications"; render(); }));
  document.querySelector<HTMLButtonElement>("[data-privacy-primary-action]")?.addEventListener("click", privacyPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-activity-primary-action]")?.addEventListener("click", activityPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-connections-primary-action]")?.addEventListener("click", connectionsPrimaryAction);
  document.querySelector<HTMLButtonElement>("[data-people-primary-action]")?.addEventListener("click", peoplePrimaryAction);
  document.querySelectorAll<HTMLButtonElement>("[data-onboarding-action]").forEach((button) => button.addEventListener("click", () => { onboardingRoute = button.dataset.onboardingAction as OnboardingRoute; route = "onboarding"; render(); }));
  document.querySelector<HTMLInputElement>("#decrypt-display")?.addEventListener("change", (event) => void changeDecryptDisplay(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLInputElement>("#privacy-export-input")?.addEventListener("change", (event) => void scanPrivacyExport(event.currentTarget as HTMLInputElement));
  document.querySelector<HTMLButtonElement>("#clear-privacy-scan")?.addEventListener("click", () => void clearPrivacyScanResults());
  bindScrubControls();
  document.querySelector<HTMLFormElement>("#activation-form")?.addEventListener("submit", (event) => void activatePro(event));
  document.querySelectorAll<HTMLFormElement>("[data-password-role]").forEach((form) => form.addEventListener("submit", (event) => void submitPasswordRole(event)));
  document.querySelectorAll<HTMLButtonElement>("[data-lock-session]").forEach((button) => button.addEventListener("click", () => void lockSessionNow(button)));
  document.querySelector<HTMLInputElement>("#activation-code")?.addEventListener("pointerdown", (event) => {
    event.stopPropagation();
    (event.currentTarget as HTMLInputElement).focus({ preventScroll: true });
  });
  document.querySelector<HTMLButtonElement>("#clear-activation-code")?.addEventListener("click", requestClearProActivation);
  document.querySelector<HTMLButtonElement>("#retry-recovery-protection")?.addEventListener("click", async () => {
    await proveRecoveryCaptureProtection();
    render();
  });
  document.querySelector<HTMLButtonElement>("#retry-identity-list")?.addEventListener("click", () => {
    hubIdentitiesLoad = "pending";
    render();
    void refreshIdentitySlots(true);
  });
  const publicNameInput = document.querySelector<HTMLInputElement>("#public-name-input");
  const syncPublicNameControls = (): void => {
    const section = document.querySelector<HTMLElement>(".public-name-page");
    const status = document.querySelector<HTMLElement>("#public-name-status");
    const check = document.querySelector<HTMLButtonElement>("#public-name-check");
    const claim = document.querySelector<HTMLButtonElement>("#public-name-claim");
    const cancel = document.querySelector<HTMLButtonElement>("#public-name-cancel");
    if (section) {
      section.dataset.publicNamePhase = publicNamePage.phase;
      section.dataset.proofName = publicNamePage.proofName ?? "";
    }
    if (status) status.textContent = publicNamePage.message;
    if (check) check.disabled = !publicNamePage.canCheck;
    if (claim) claim.disabled = !publicNamePage.canClaim;
    if (cancel) cancel.disabled = publicNamePage.busy;
  };
  publicNameInput?.addEventListener("input", () => {
    void publicNamePage.enterName(publicNameInput.value).then(syncPublicNameControls);
    syncPublicNameControls();
  });
  document.querySelector<HTMLButtonElement>("#public-name-check")?.addEventListener("click", async () => {
    await publicNamePage.checkName();
    render();
  });
  document.querySelector<HTMLButtonElement>("#public-name-claim")?.addEventListener("click", async () => {
    const claim = await publicNamePage.claimName();
    if (claim) claimedOslUsername = claim.username;
    render();
  });
  document.querySelector<HTMLButtonElement>("#public-name-cancel")?.addEventListener("click", async () => {
    await publicNamePage.cancel();
    render();
  });
  document.querySelector<HTMLFormElement>("#identity-slot-form")?.addEventListener("submit", (event) => void createAdditionalIdentity(event));
  document.querySelector<HTMLFormElement>("#identity-recover-form")?.addEventListener("submit", (event) => void recoverAdditionalIdentity(event));
  document.querySelectorAll<HTMLButtonElement>("[data-switch-identity]").forEach((button) => button.addEventListener("click", () => void switchIdentity(button.dataset.switchIdentity ?? "")));
  document.querySelector<HTMLButtonElement>("#native-discord-covertext")?.addEventListener("click", () => {
    void invoke<boolean>("select_native_discord_covertext_writer").then((confirmed) => {
      nativeDiscordCovertextEnabled = confirmed;
      if (nativeDiscordAiCovertextSelected) {
        void invoke("set_ai_covertext_selected", { selected: false });
        nativeDiscordAiCovertextSelected = false;
      }
      render();
      showToast(nativeDiscordCovertextEnabled ? "Covertext wordbank selected" : "Covertext did not change");
    }).catch(() => showToast("Covertext did not change"));
  });
  document.querySelector<HTMLButtonElement>("#native-discord-ai-covertext")?.addEventListener("click", () => {
    const requested = !nativeDiscordAiCovertextSelected;
    void invoke<{ localModelReady: boolean; aiCovertextSelected: boolean }>("set_ai_covertext_selected", { selected: requested }).then((status) => {
      nativeDiscordAiCovertextSelected = status.localModelReady && status.aiCovertextSelected;
      render();
      showToast(nativeDiscordAiCovertextSelected ? "AI Covertext will write the next cover on this device" : "Covertext will use the built-in writer");
    }).catch(() => showToast("AI Covertext could not start the local model"));
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
  const whitelistToggle = document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-toggle");
  if (whitelistToggle) {
    connectDiscordQaWhitelistButton(whitelistToggle, () => {
      const active = activeVerifiedDiscordQaPeer();
      return active
        ? discordQaOpenPlace({
            serviceId: active.context.serviceId,
            accountId: active.context.accountId,
            personId: active.person.personId,
          })
        : null;
    }, {
      onCommand: (_command, allowed) => void setDiscordQaWhitelistPermission(allowed),
      onError: () => showToast("Whitelist change failed closed"),
    });
  }
  document.querySelector<HTMLButtonElement>("#discord-qa-whitelist-roster")?.addEventListener("click", () => {
    whitelistRosterOpen = !whitelistRosterOpen;
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
  // Whitelisting screen. render() rebuilds the section, so the search box puts
  // its own focus and caret back rather than dropping the user out of the field
  // after every keystroke.
  document.querySelector<HTMLInputElement>("#whitelisting-search")?.addEventListener("input", (event) => {
    const input = event.currentTarget as HTMLInputElement;
    const caret = input.selectionStart;
    setWhitelistingState(whitelistingSetSearch(whitelistingScreenState(), input.value));
    const restored = document.querySelector<HTMLInputElement>("#whitelisting-search");
    if (!restored) return;
    restored.focus({ preventScroll: true });
    if (caret !== null) restored.setSelectionRange(caret, caret);
  });
  document.querySelectorAll<HTMLInputElement>("[data-whitelisting-conversation]").forEach((tick) => tick.addEventListener("change", () => {
    setWhitelistingState(whitelistingToggleConversation(whitelistingScreenState(), tick.dataset.whitelistingConversation ?? "", tick.checked));
  }));
  document.querySelector<HTMLButtonElement>("[data-whitelisting-select-all]")?.addEventListener("click", () => {
    setWhitelistingState(whitelistingSelectAll(whitelistingScreenState()));
  });
  document.querySelector<HTMLButtonElement>("[data-whitelisting-clear-all]")?.addEventListener("click", () => {
    setWhitelistingState(whitelistingClearAll(whitelistingScreenState()));
  });
  document.querySelector<HTMLButtonElement>("[data-whitelisting-reset]")?.addEventListener("click", () => {
    setWhitelistingState(whitelistingReset(whitelistingScreenState()));
  });
  document.querySelector<HTMLButtonElement>("[data-whitelisting-save]")?.addEventListener("click", () => {
    void saveWhitelistingSelection();
  });
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
  document.querySelectorAll<HTMLButtonElement>("[data-look-mode]").forEach((button) => button.addEventListener("click", () => {
    const mode = lookMode(button.dataset.lookMode);
    if (mode) previewLook({ ...lookState, mode });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-appearance-accent]").forEach((button) => button.addEventListener("click", () => {
    const accent = button.dataset.appearanceAccent;
    if (accent && accentChoices.includes(accent as typeof accentChoices[number])) saveAppearance({ ...appearancePreferences, accent: accent as typeof accentChoices[number] });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-appearance-background]").forEach((button) => button.addEventListener("click", () => {
    const background = button.dataset.appearanceBackground;
    if (background && backgroundChoices.includes(background as typeof backgroundChoices[number])) saveAppearance({ ...appearancePreferences, background: background as typeof backgroundChoices[number] });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-appearance-avatar]").forEach((button) => button.addEventListener("click", () => {
    const avatar = button.dataset.appearanceAvatar;
    if (avatar && avatarChoices.includes(avatar as typeof avatarChoices[number])) saveAppearance({ ...appearancePreferences, avatar: avatar as typeof avatarChoices[number] });
  }));
  document.querySelector<HTMLSelectElement>("[data-appearance-window-position]")?.addEventListener("change", (event) => {
    const windowPosition = (event.currentTarget as HTMLSelectElement).value;
    if (windowPositionChoices.includes(windowPosition as typeof windowPositionChoices[number])) saveAppearance({ ...appearancePreferences, windowPosition: windowPosition as typeof windowPositionChoices[number] });
  });
  document.querySelector<HTMLInputElement>("[data-appearance-tray]")?.addEventListener("change", (event) => saveAppearance({ ...appearancePreferences, keepInTray: (event.currentTarget as HTMLInputElement).checked }));
  document.querySelector<HTMLInputElement>("[data-appearance-sounds]")?.addEventListener("change", (event) => saveAppearance({ ...appearancePreferences, sounds: (event.currentTarget as HTMLInputElement).checked }));
  document.querySelector<HTMLButtonElement>("[data-save-appearance]")?.addEventListener("click", () => {
    appearancePreferences = saveAppearancePreferences(localStorage, appearancePreferences);
    savedAppearancePreferences = { ...appearancePreferences };
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-cancel-appearance]")?.addEventListener("click", () => {
    appearancePreferences = { ...savedAppearancePreferences };
    render();
  });
  document.querySelector<HTMLButtonElement>("[data-reset-appearance]")?.addEventListener("click", () => {
    appearancePreferences = resetAppearancePreferences(localStorage);
    savedAppearancePreferences = { ...appearancePreferences };
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-look-named]").forEach((button) => button.addEventListener("click", () => {
    const named = button.dataset.lookNamed;
    if (named === "midnight" || named === "paper" || named === "signal") previewLook({ ...lookState, named });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-look-accent]").forEach((button) => button.addEventListener("click", () => {
    const accent = button.dataset.lookAccent;
    if (accent === "cyan" || accent === "violet" || accent === "amber") previewLook({ ...lookState, accent });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-look-corners]").forEach((button) => button.addEventListener("click", () => {
    const corners = button.dataset.lookCorners;
    if (corners === "square" || corners === "soft") previewLook({ ...lookState, corners });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-look-glow]").forEach((button) => button.addEventListener("click", () => {
    previewLook({ ...lookState, glow: button.dataset.lookGlow === "on" });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-look-text]").forEach((button) => button.addEventListener("click", () => {
    const text = button.dataset.lookText;
    if (text === "comfortable" || text === "large") previewLook({ ...lookState, text });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-look-spacing]").forEach((button) => button.addEventListener("click", () => {
    const spacing = button.dataset.lookSpacing;
    if (spacing === "compact" || spacing === "relaxed") previewLook({ ...lookState, spacing });
  }));
  document.querySelector<HTMLButtonElement>("[data-look-reset]")?.addEventListener("click", () => previewLook(defaultLookState));
  document.querySelector<HTMLButtonElement>("[data-look-save]")?.addEventListener("click", persistLook);
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
  document.querySelectorAll("[data-edit-home]").forEach((button) => button.addEventListener("click", () => { route = "arrange-tiles"; homeEditMode = false; render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-arrange-home]").forEach((button) => button.addEventListener("click", () => { route = "arrange-tiles"; homeEditMode = false; render(); }));
  document.querySelector<HTMLButtonElement>("[data-arrange-back]")?.addEventListener("click", () => { route = "home"; homeEditMode = false; render(); });
  document.querySelector<HTMLButtonElement>("[data-arrange-done]")?.addEventListener("click", () => { route = "home"; homeEditMode = false; render(); });
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
    // On Home the Friends surface is the persistent right panel, not a route.
    if (route === "home") {
      homeFriendsPanelCollapsed = false;
      homeNotificationsOpen = false;
      friendsDialogOpen = false;
      render();
      return;
    }
    route = "people";
    friendsDialogOpen = false;
    friendsDialogPage = 0;
    render();
  }));
  document.querySelectorAll<HTMLElement>("[data-toggle-home-notifications]").forEach((button) => button.addEventListener("click", () => {
    homeNotificationsOpen = !homeNotificationsOpen;
    render();
  }));
  document.querySelectorAll<HTMLElement>("[data-collapse-friends]").forEach((button) => button.addEventListener("click", () => {
    homeFriendsPanelCollapsed = true;
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
  bindAddFriendByNameForm(document, pendingFriendRequestsByName, { createRequest: createOslFriendRequestByOslName }, escapeHtml);
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
  document.querySelector<HTMLInputElement>("#notification-scope-suggestions")?.addEventListener("change", (event) => void setNotificationScopeSuggestions((event.currentTarget as HTMLInputElement).checked));
  document.querySelectorAll<HTMLInputElement>("[data-notification-app]").forEach((input) => input.addEventListener("change", () => { const id = input.dataset.notificationApp as ServiceId; notificationAppPreferences[id] = input.checked; localStorage.setItem(notificationAppsStorageKey, JSON.stringify(notificationAppPreferences)); }));
  document.querySelector<HTMLInputElement>("#notification-scope-suggestions")?.addEventListener("change", (event) => { notificationScopeSuggestions = (event.currentTarget as HTMLInputElement).checked; localStorage.setItem(notificationScopeStorageKey, String(notificationScopeSuggestions)); });
  document.querySelectorAll<HTMLInputElement>("[data-notification-app]").forEach((input) => input.addEventListener("change", () => { setNotificationAppPreference(input.dataset.notificationApp as ServiceId, input.checked); }));
  document.querySelectorAll<HTMLButtonElement>("[data-osl-chat-unmute]").forEach((button) => button.addEventListener("click", () => {
    oslChatMutedPeople.delete(button.dataset.oslChatUnmute ?? "");
    persistOslChatMutedPeople();
    render();
  }));
  bindBurnDialog();
  bindOwnedConfirmation();
  bindUpdateControls();
  bindOwnerRoleEditor(ownerRoleEditor, render);
}

async function openHomeAppFromLauncher(appId: HomeAppId, intent: number): Promise<void> {
  try {
    const refreshed = await withNativeDeadline(loadLinkedServices(), "Refresh apps", 450).catch(() => null);
    if (intent !== navigationIntentEpoch) return;
    if (refreshed) services = refreshed;
    const app = homeAppsFromServices(services).find((candidate) => candidate.id === appId);
    const service = app?.serviceId ? services.find((candidate) => candidate.id === app.serviceId) : null;
    if (!app || !service || app.launchState !== "available") {
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

type HomeTileArrangementSaveResult = {
  saved: boolean;
  error: string | null;
  hiddenIds: string[];
};

function checkHomeTileArrangement(hiddenIds: ReadonlySet<string>): string | null {
  const current = currentHomeTileIds();
  return current.some((id) => !hiddenIds.has(id)) ? null : HOME_TILE_ARRANGEMENT_REFUSAL;
}

function saveHomeTileArrangement(nextHiddenHomeTiles: ReadonlySet<string>): HomeTileArrangementSaveResult {
  const current = currentHomeTileIds();
  const hidden = new Set([...nextHiddenHomeTiles].filter((id) => current.includes(id)));
  const error = checkHomeTileArrangement(hidden);
  if (error) return { saved: false, error, hiddenIds: [...hidden] };
  hiddenHomeTiles = hidden;
  saveHomeTilePreferences();
  return { saved: true, error: null, hiddenIds: [...hiddenHomeTiles] };
}

function moveHomeTile(raw: string): void {
  const separator = raw.lastIndexOf(":");
  const id = raw.slice(0, separator);
  const delta = Number(raw.slice(separator + 1));
  const arranged = moveHomeTileArrangement(currentHomeTileIds(), {
    order: homeTileOrder,
    hidden: [...hiddenHomeTiles],
  }, id, delta);
  homeTileOrder = arranged.order;
  hiddenHomeTiles = new Set(arranged.hidden);
  saveHomeTilePreferences();
  const position = arranged.order.indexOf(id) + 1;
  homeTileArrangementNotice = position > 0 ? `Moved ${id} to position ${position}.` : "Tile order saved.";
  render();
}

function reorderHomeTile(sourceId: string | null, targetId: string | null): void {
  const arranged = dragHomeTileArrangement(currentHomeTileIds(), {
    order: homeTileOrder,
    hidden: [...hiddenHomeTiles],
  }, sourceId, targetId);
  homeTileOrder = arranged.order;
  hiddenHomeTiles = new Set(arranged.hidden);
  saveHomeTilePreferences();
  if (sourceId && targetId) homeTileArrangementNotice = `Moved ${sourceId} before ${targetId}.`;
  render();
}

function toggleHomeTile(id: string): void {
  if (!currentHomeTileIds().includes(id)) return;
  const nextHiddenHomeTiles = new Set(hiddenHomeTiles);
  if (nextHiddenHomeTiles.has(id)) nextHiddenHomeTiles.delete(id); else nextHiddenHomeTiles.add(id);
  const result = saveHomeTileArrangement(nextHiddenHomeTiles);
  if (!result.saved) {
    showToast(result.error ?? HOME_TILE_ARRANGEMENT_REFUSAL);
    return;
  }
  const arranged = toggleHomeTileVisibility(currentHomeTileIds(), {
    order: homeTileOrder,
    hidden: [...hiddenHomeTiles],
  }, id);
  homeTileOrder = arranged.order;
  hiddenHomeTiles = new Set(arranged.hidden);
  saveHomeTilePreferences();
  homeTileArrangementNotice = hiddenHomeTiles.has(id) ? `Hidden ${id} from Home.` : `Restored ${id} to Home.`;
  render();
}

export function inboxPrimaryAction(): void {
  const first = hubPeople.find(peerIsVerified);
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
    route = "osl-mail-status";
    render();
  } else if (id === "osl-notes") {
    route = "osl-notes-status";
    render();
  } else if (id === "osl-servers") {
    route = "osl-servers";
    render();
  } else if (id === "scrub") {
    route = "scrub";
    render();
  } else if (id === "activity") {
    route = "activity";
    render();
  }
}

function oslChatTimestamp(): string {
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date());
}

function decideOslChatVerificationWarning(personId: string, moment: "open-conversation" | "prepare-send"): VerificationWarningSurface {
  const checked = oslChatHandshakeConfirmed(oslChatMessages.get(personId) ?? []);
  return verificationWarningDecision(
    oslChatVerificationWarningSetting,
    { conversationId: personId, checked },
    moment,
    oslChatVerificationWarningMemory,
  ).surface;
}

function nowUnixSeconds(): number {
  return Math.floor(Date.now() / 1_000);
}

function pruneExpiredOslChatLocalCopies(nowSeconds = nowUnixSeconds()): void {
  const expiredIds = new Set<string>();
  for (const [personId, messages] of oslChatMessages) {
    const retained = pruneExpiredOslChatMessages(messages, nowSeconds);
    if (retained.length === messages.length) continue;
    for (const message of messages) {
      if (!retained.some((candidate) => candidate.messageId === message.messageId)) {
        expiredIds.add(message.messageId);
      }
    }
    oslChatMessages.set(personId, retained);
  }
  if (!expiredIds.size || !appNotifications) return;
  const retainedNotifications = appNotifications.filter((notice) => !expiredIds.has(notice.id));
  if (retainedNotifications.length === appNotifications.length) return;
  appNotifications = retainedNotifications;
  if (notificationsEnabled) persistOslChatNotifications();
}

async function openOslChat(personId: string): Promise<void> {
  pruneExpiredOslChatLocalCopies();
  const person = hubPeople.find((candidate) => candidate.personId === personId);
  if (!person || oslChatBusy) return;
  const queuedViewOnce = (oslChatUnread.get(personId) ?? 0) > 0
    ? (oslChatMessages.get(personId) ?? []).filter((message) => message.state === "opened")
    : [];
  const epoch = ++oslChatOperationEpoch;
  oslChatBusy = true;
  oslChatVerificationWarningSurface = "none";
  oslChatSettingsPersonId = null;
  oslChatSendBlockedReason = null;
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
    // A changed or not-yet-verified key may be inspected, but it is never
    // given a sending context. This keeps the verification state visible and
    // lets the composer explain its blocked send instead of hiding the chat.
    if (!peerIsVerified(person)) {
      activeOslChatPersonId = personId;
      activeOslChatContext = null;
      route = "osl-chat";
      return;
    }
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
            dateLabel: oslChatDateLabel(row.createdAt),
            reactions: row.reactions,
          };
        });
        oslChatMessages.set(personId, [...durableMessages, ...queuedViewOnce].slice(-200));
      }
      oslChatAttachments = await listOslChatAttachments() ?? [];
      shouldRefresh = true;
    }
    oslChatVerificationWarningSurface = decideOslChatVerificationWarning(personId, "open-conversation");
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
  const metadata = (appNotifications ?? []).filter(isPersistedOslChatNotification).slice(0, 20);
  void persistSensitiveOslChatJson(oslChatNotificationStorageKey, encodeOslChatNotifications(metadata));
}

function recordAppNotification(notification: AppNotification): void {
  appNotifications = [notification, ...(appNotifications ?? [])].slice(0, 20);
}

function mergePersistedOslChatNotifications(items: AppNotification[] | null): AppNotification[] {
  const chat = (appNotifications ?? []).filter(isPersistedOslChatNotification);
  const merged = [...chat, ...(items ?? [])];
  return merged.filter((item, index) => merged.findIndex((candidate) => candidate.id === item.id) === index).slice(0, 20);
}

async function refreshActiveOslChatApprovalSuggestion(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || context.scopeApproved) return;
  const answer = await answerHubChatApprovalSuggestion(context.contextToken, context.personId);
  if (!answer || activeOslChatContext?.contextToken !== context.contextToken) return;
  activeOslChatContext = {
    ...activeOslChatContext,
    suggestion: answer === "offer_approval" ? "offer_approval" : undefined,
  };
  render();
}

async function setNotificationScopeSuggestions(enabled: boolean): Promise<void> {
  notificationScopeSuggestions = enabled;
  localStorage.setItem(notificationScopeStorageKey, String(enabled));
  const saved = await setHubChatApprovalSuggestionChoice(enabled);
  if (saved) {
    notificationScopeSuggestions = saved === "on";
    localStorage.setItem(notificationScopeStorageKey, String(notificationScopeSuggestions));
  }
  await refreshActiveOslChatApprovalSuggestion();
}

function commitOslChatBatch(personId: string, batch: NativeDiscordOverlayOpenedBatch, background: boolean): void {
  const messages = pruneExpiredOslChatMessages(oslChatMessages.get(personId) ?? [], nowUnixSeconds());
  const notificationSettings = readOslChatNotificationSettings(localStorage, personId);
  const senderName = hubPeople.find((person) => person.personId === personId)?.alias ?? "Verified friend";
  for (const acknowledgment of batch.acknowledgments) {
    const message = messages.find((candidate) => candidate.messageId === acknowledgment.messageId);
    if (message) message.state = acknowledgment.status;
  }
  for (const incoming of batch.messages) {
    const localMessageId = `received-${crypto.randomUUID()}`;
    const received = receivedOslChatBatchMessage(localMessageId, incoming, oslChatHistoryTimestamp);
    messages.push(received);
    if (background) {
      oslChatUnread.set(personId, Math.min(10_000, (oslChatUnread.get(personId) ?? 0) + 1));
      const notification = oslChatNotificationPreview(notificationSettings, senderName, received.body);
      if (notificationsEnabled && notificationChatActivity && !oslChatMutedPeople.has(personId) && notification) {
        recordAppNotification({
          id: localMessageId,
          title: notification.title,
          detail: notification.messagePreview,
          createdAt: "Now",
        });
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

async function drainOslChatTextWithRefusal(): Promise<NativeDiscordOverlayOpenedBatch | null> {
  const before = Date.now();
  const batch = await openOslChatText();
  if (batch) {
    lastOslChatOpenRefusal = null;
  } else {
    const failure = lastBackendFailure("open_osl_chat_text");
    lastOslChatOpenRefusal = failure && failure.at >= before ? failure.message : null;
  }
  return batch;
}

function commitOslChatNotice(personId: string, message: OslChatMessage, background: boolean): void {
  const messages = [...(oslChatMessages.get(personId) ?? []), message].slice(-200);
  oslChatMessages.set(personId, messages);
  if (background) {
    oslChatUnread.set(personId, Math.min(10_000, (oslChatUnread.get(personId) ?? 0) + 1));
    void persistOslChatUnread();
  }
  renderWhenIdle();
}

// Delivery lives in ./osl-chat-runtime (T14-A0). This object is the only thing
// main.ts still owns of it: the binding between the runtime and this module's
// state. Note what is NOT in the preconditions — the route. A message must
// arrive on any screen (T14-B2); the remaining checks are session ownership of
// the single active OSL Chat context, not "the user is looking at Home".
const oslChatDeliveryHost: OslChatDeliveryHost = {
  identityLoaded: () => core.readiness.identityLoaded,
  foreignContextActive: () => Boolean(activeContextToken || activeNativeHostId || activeEmbeddedHost),
  openConversationId: () => activeOslChatPersonId,
  conversationBusy: () => oslChatBusy,
  friends: () => hubPeople,
  requestCaptureProtection: async () => {
    const applied = await setScreenshotProtection(true);
    if (applied) screenshotProtectionEnabled = true;
    return applied;
  },
  activateContext: async (personId) => {
    const context = await activateOslChatContext(personId);
    return context ? { personId, peerOslUserId: context.peerOslUserId, scopeApproved: context.scopeApproved } : null;
  },
  closeContext: () => closeOslChatContext(),
  drainInbox: () => drainOslChatTextWithRefusal(),
  drainRefusal: () => lastOslChatOpenRefusal,
  loadHistory: () => listOslChatHistory(),
  commitBatch: (personId, batch, background) => {
    commitOslChatBatch(personId, batch, background);
    // A conversation drained while the user is reading it renders in place.
    if (!background && batch.messages.length) renderWhenIdle();
  },
  commitNotice: commitOslChatNotice,
  commitHistory: (personId, rows, context) => {
    const openedViewOnce = pruneExpiredOslChatMessages(
      oslChatMessages.get(personId) ?? [],
      nowUnixSeconds(),
    ).filter((message) => message.state === "opened");
    oslChatMessages.set(personId, mergeOslChatTimeline(
      oslChatHistoryMessages(rows, context, oslChatHistoryTimestamp),
      openedViewOnce,
    ));
  },
};

function oslChatHistoryTimestamp(epochSeconds: number): string {
  return new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(new Date(epochSeconds * 1_000));
}

function oslChatDateLabel(epochSeconds: number): string {
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" })
    .format(new Date(epochSeconds * 1_000));
}

const oslChatDelivery = createOslChatDeliveryRuntime(oslChatDeliveryHost);

async function toggleOslChatPermission(): Promise<void> {
  const context = activeOslChatContext;
  if (!context || oslChatBusy || oslChatSettingsPersonId !== context.personId) return;
  const person = hubPeople.find((candidate) => candidate.personId === context.personId);
  if (!person || !peerIsVerified(person)) {
    showToast(person?.pendingKeyChange ? OSL_CHAT_KEY_CHANGED_REFUSAL_REASON : "Verify this friend before changing the chat whitelist.");
    render();
    return;
  }
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
  const person = hubPeople.find((candidate) => candidate.personId === context.personId);
  if (!person || !peerIsVerified(person)) {
    showToast(person?.pendingKeyChange ? OSL_CHAT_KEY_CHANGED_REFUSAL_REASON : "Verify this friend before changing the chat whitelist.");
    render();
    return;
  }
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
  if (!context?.scopeApproved || !personId || oslChatBusy || refuseOfflineCapability("receiveNewMessages")) return;
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
  const batch = await drainOslChatTextWithRefusal();
  oslChatAttachments = await listOslChatAttachments() ?? oslChatAttachments;
  // Draining is destructive at the relay. Commit any returned batch to this
  // conversation even if a later UI transition supersedes the render.
  if (batch) commitOslChatBatch(personId, batch, false);
  if (!batch && lastOslChatOpenRefusal) {
    const notice = oslChatOpenRefusalMessage(`open-refusal-${Date.now()}`, lastOslChatOpenRefusal, "Now");
    if (notice) commitOslChatNotice(personId, notice, false);
  }
  if (epoch === oslChatOperationEpoch) {
    oslChatBusy = false;
    render();
  }
}

async function toggleOslChatReaction(messageId: string, emoji: string, mine: boolean): Promise<void> {
  const context = activeOslChatContext;
  const personId = activeOslChatPersonId;
  if (!context?.scopeApproved || !personId || oslChatBusy) return;
  const result = mine
    ? await removeOslChatReaction(messageId, emoji)
    : await addOslChatReaction(messageId, emoji);
  if (!result || activeOslChatContext?.contextToken !== context.contextToken) {
    showToast("Reaction was not saved");
    return;
  }
  const messages = [...(oslChatMessages.get(personId) ?? [])];
  const message = messages.find((candidate) => candidate.messageId === result.messageId);
  if (!message) return;
  const reactions = [...(message.reactions ?? [])];
  const index = reactions.findIndex((reaction) => reaction.emoji === result.emoji);
  if (result.removed) {
    if (index >= 0) {
      const current = reactions[index]!;
      const count = Math.max(0, current.count - 1);
      if (count === 0) reactions.splice(index, 1);
      else reactions[index] = { ...current, count, mine: false };
    }
  } else if (result.added) {
    if (index >= 0) {
      const current = reactions[index]!;
      reactions[index] = { ...current, count: current.count + 1, mine: true };
    } else {
      reactions.push({ emoji: result.emoji, count: 1, mine: true });
    }
  }
  message.reactions = reactions;
  oslChatMessages.set(personId, messages);
  render();
}

async function sendOslChatAttachment(): Promise<void> {
  const person = hubPeople.find((candidate) => candidate.personId === activeOslChatPersonId);
  if (!person || !peerIsVerified(person)) {
    showToast(person?.pendingKeyChange ? OSL_CHAT_KEY_CHANGED_REFUSAL_REASON : "Sending is blocked until you verify this friend.");
    render();
    return;
  }
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

export const OSL_CHAT_SEND_ROUTES = ["enter", "send-button", "send-later", "queued-draft"] as const;
export type OslChatSendRoute = (typeof OSL_CHAT_SEND_ROUTES)[number];
const oslChatSendRouteAttempts = new Map<OslChatSendRoute, number>();

async function sendOslChatFromRoute(route: OslChatSendRoute): Promise<void> {
  escapeAuditSendAttempts += 1;
  oslChatSendRouteAttempts.set(route, (oslChatSendRouteAttempts.get(route) ?? 0) + 1);
  const context = activeOslChatContext;
  const personId = activeOslChatPersonId;
  const draft = oslChatDraft;
  const handshakeConfirmed = personId ? oslChatHandshakeConfirmed(oslChatMessages.get(personId) ?? []) : false;
  const person = personId ? hubPeople.find((candidate) => candidate.personId === personId) : null;
  if (!personId || !person || !person.safetyNumberVerified || person.pendingKeyChange) {
    oslChatSendBlockedReason = person?.pendingKeyChange
      ? OSL_CHAT_KEY_CHANGED_REFUSAL_REASON
      : "This chat is not verified yet. Verify the safety number before sending anything.";
    if (person?.pendingKeyChange) showToast(OSL_CHAT_KEY_CHANGED_REFUSAL_REASON);
    render();
    return;
  }
  if (!context?.scopeApproved || !handshakeConfirmed) {
    oslChatSendBlockedReason = !context?.scopeApproved
      ? "This encrypted chat is not turned on for this friend yet."
      : "OSL has not received a reply from this friend yet, so it cannot confirm they completed their side.";
    render();
    return;
  }
  if (oslChatBusy || !isHubPlaintext(draft) || refuseOfflineCapability("sendMessage")) return;
  const epoch = oslChatOperationEpoch;
  oslChatVerificationWarningSurface = decideOslChatVerificationWarning(personId, "prepare-send");
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
    dateLabel: oslChatDateLabel(Date.now() / 1_000),
    reactions: [],
  }];
  oslChatMessages.set(personId, messages);
  setOslChatDraft("");
  oslChatViewOnce = false;
  oslChatBusy = false;
  render();
}

async function sendOslChat(event: SubmitEvent): Promise<void> {
  event.preventDefault();
  const submitter = event.submitter as HTMLElement | null;
  const route: OslChatSendRoute = submitter?.dataset.oslChatSendRoute === "send-button" ? "send-button" : "enter";
  await sendOslChatFromRoute(route);
}

function resetOslChatUiState(clearMessages: boolean): void {
  oslChatOperationEpoch += 1;
  activeOslChatPersonId = null;
  activeOslChatContext = null;
  oslChatDraft = "";
  oslChatBusy = false;
  oslChatAttachments = [];
  oslChatVerificationWarningSurface = "none";
  oslChatSendBlockedReason = null;
  startSomethingChoice = null;
  startSomethingBusy = false;
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
  // Offline refusal stays FIRST: a username add needs a key lookup, so letting
  // the username branch run while offline would attempt exactly the capability
  // this guard exists to refuse.
  if (refuseOfflineCapability("lookUpNewContactKey")) {
    if (status) status.textContent = offlineCapabilityStatus("lookUpNewContactKey", "offline").detail;
    return;
  }
  const username = isNormalizedOslUsername(code) ? code : null;
  if (!username && !/^OSLFR1\.[A-Za-z0-9_-]{16,8192}$/.test(code)) {
    if (status) status.textContent = "Enter a valid OSL invite or username.";
    input?.focus();
    return;
  }
  if (button) button.disabled = true;
  if (status) status.textContent = username ? "Resolving username…" : "Saving request locally…";
  const resolved = username ? await addOslFriendByUsername(username, nicknameInput?.value ?? "") : null;
  const outcome = username
    ? (resolved ? { added: true, reason: null } : { added: false, reason: "username lookup was refused" })
    : await addOslFriend(code, nicknameInput?.value ?? "");
  if (button) button.disabled = false;
  if (!outcome.added) {
    if (status) status.textContent = addFriendFailureStatus(outcome.reason ?? "");
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

async function loadFriendFutureAccountSwitchStates(): Promise<void> {
  const personIds = hubPeople.map((person) => person.personId);
  const loaded = await loadFutureAccountSwitchStates(personIds, { isTauriRuntime, invoke, recordBackendFailure });
  friendFutureAccountAutoWhitelist.clear();
  for (const [personId, enabled] of loaded) friendFutureAccountAutoWhitelist.set(personId, enabled);
}

async function changeFriendFutureAccountSwitch(input: HTMLInputElement): Promise<void> {
  const personId = input.dataset.futureAccountToggle ?? "";
  if (!personId || friendFutureAccountAutoWhitelistBusy.has(personId)) return;
  const requested = input.checked;
  const previous = friendFutureAccountAutoWhitelist.get(personId) ?? false;
  friendFutureAccountAutoWhitelistBusy.add(personId);
  input.disabled = true;
  const saved = await saveFutureAccountSwitch(personId, requested, { isTauriRuntime, invoke, recordBackendFailure });
  friendFutureAccountAutoWhitelistBusy.delete(personId);
  if (saved) friendFutureAccountAutoWhitelist.set(saved.personId, saved.enabled);
  if (!saved) {
    input.checked = previous;
    showToast("Future-account approval was not saved · nothing changed");
  }
  render();
}

async function copyFriendInvite(): Promise<void> {
  if (!friendCode) { showToast("Friend invite is unavailable"); return; }
  const result = await copyHubFriendInvite(friendCode);
  showToast(result.copied ? "Invite copied" : inviteCopyFailureToast(result.reason));
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

async function refreshIdentitySlots(rerenderAccountSettings = false): Promise<void> {
  if (identityListRefreshInFlight) return;
  identityListRefreshInFlight = true;
  try {
    const loaded = await listHubIdentities();
    // `null` means the command failed or the response did not match the
    // expected shape. That is not an empty account and it is not a lock, so it
    // must never be collapsed into `[]` and reported as either one.
    hubIdentities = loaded ?? [];
    hubIdentitiesLoad = loaded ? "loaded" : "unavailable";
  } finally {
    identityListRefreshInFlight = false;
  }
  if (rerenderAccountSettings && route === "settings" && settingsSection === "account") renderWhenIdle();
}

async function refreshBuildIntegrityStatus(): Promise<void> {
  const status = await loadBuildIntegrityStatus();
  if (!status) return;
  buildIntegrityStatus = status;
  if (route === "osl-chat") renderWhenIdle();
}

async function refreshIdentityScopedState(): Promise<void> {
  if (activeOslChatContext && !(await closeOslChatContext())) {
    throw new Error("OSL Chat could not close before changing identity state");
  }
  resetOslChatUiState(true);
  const [nextCore, loadedIdentities, profile, people, linkedServices, notifications, buildWarning] = await Promise.all([
    loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration)),
    listHubIdentities(),
    loadFriendProfile().then(async (value) => {
      await getOslUsernameStatus("osl").catch(() => null);
      return value;
    }),
    listHubPeople().then((value) => value ?? []),
    loadLinkedServices().catch(() => []),
    notificationsEnabled ? loadAppNotifications() : Promise.resolve([]),
    loadInstalledBuildChatWarningStatus(),
  ]);
  core = nextCore;
  refreshActiveBrowserAccountsReady();
  hubIdentities = loadedIdentities ?? [];
  hubIdentitiesLoad = loadedIdentities ? "loaded" : "unavailable";
  friendCode = profile?.friendCode ?? null;
  friendDisplayId = profile?.oslUserId ?? null;
  hubPeople = people;
  await loadFriendFutureAccountSwitchStates();
  services = linkedServices;
  appNotifications = mergePersistedOslChatNotifications(notifications);
  installedBuildChatWarning = buildWarning;
  passwordRoleStatus = await loadHubPasswordRoleStatus().catch(() => null);
}

/// A7: manual "Lock now". The backend drops every live secret; the UI just has
/// to stop showing an unlocked session. Deliberately no confirmation dialog —
/// locking is non-destructive and hesitating is the wrong default when someone
/// is walking away from the machine.
async function lockSessionNow(button: HTMLButtonElement): Promise<void> {
  button.disabled = true;
  try {
    await lockHubSession();
    passwordRoleStatus = null;
    core = await loadCoreIntegration().catch(() => structuredClone(unavailableCoreIntegration));
    render();
    showToast("OSL locked. Your main password is required to continue.");
  } catch (failure) {
    button.disabled = false;
    showToast(localActionError(failure, "OSL could not be locked"));
  }
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
    if (route === "onboarding" && onboardingRoute === "pro" && licenseState.access !== "free") {
      proOnboardingReadyResult = true;
      proOnboardingCodeEntryRequested = false;
    }
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
  if (request.kind === "verifyFriend") {
    const submission = verificationSubmission(typedVerificationCode);
    if ("refusal" in submission) {
      ownedConfirmationError = submission.refusal;
      const blankStatus = document.querySelector<HTMLElement>("#owned-confirmation-dialog .form-status");
      if (blankStatus) blankStatus.textContent = submission.refusal;
      return;
    }
  }
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
      submit.disabled = false;
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
  const acknowledgement = document.querySelector<HTMLInputElement>("#burn-confirm-ack");
  if (!acknowledgement?.checked) return;
  const requestedUninstall = burnScope === "account" && document.querySelector<HTMLInputElement>("#burn-uninstall")?.checked === true;
  acknowledgement.disabled = true;
  burnBusy = true;
  const submit = document.querySelector<HTMLButtonElement>("#burn-confirm-submit");
  const status = document.querySelector<HTMLElement>("#burn-form-status");
  if (submit) { submit.disabled = true; submit.textContent = "Burning…"; }
  if (status) status.textContent = "Removing local OSL data…";

  if (burnScope === "chat") {
    const contextToken = activeContextToken;
    const contextKind = activeProtectedContextKind;
    const outcome = contextToken ? await burnActiveHubContext(contextToken) : null;
    if (!outcome) {
      burnBusy = false;
      burnResult = { tone: "error", message: "The chat burn failed closed. No deletion success is being claimed.", showUninstall: false };
      render();
      return;
    }
    burnBusy = false;
    resetLocalProtectedSheet();
    // The local half is done. Whether the OTHER side's access is actually gone
    // is a separate question with a separate answer, and a queued-but-
    // unacknowledged revocation must never be shown as a success --
    // `queue_scope_revocations_locked` in apps/osl-hub/src/security.rs.
    const revocation = burnRevocationReceipt(outcome, await getHubRevocationStatus(outcome.storageKey));
    const localLine = contextKind === "peer"
      ? "Local approval, display, and expiry settings for this app account + friend were revoked. OSL attempted relay cleanup. Provider messages and opened copies remain."
      : "Local OSL decrypt material and caches for this chat were removed. Native app history was not deleted.";
    burnResult = {
      tone: revocation.tone,
      message: localLine,
      showUninstall: false,
      revocation,
      destructServerStatus: revocation.acknowledged ? "confirmed" : "not-confirmed",
    };
    render();
    return;
  }

  if (burnScope === "app") {
    const target = activeServiceContextTarget();
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
      destructServerStatus: result.remoteCleanupComplete ? "confirmed" : "not-confirmed",
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
  const presentation = freshStartCleanupPresentation(result);
  if (!presentation.complete) {
    burnResult = { tone: presentation.tone, message: presentation.message, showUninstall: false };
    render();
    return;
  }
  localStorage.clear();
  identityStorageMethod = null;
  knownIdentityStorageMethods.clear();
  newIdentityRecoveryPhrase = null;
  recoveryBundle = null;
  recoverySavedAcknowledged = false;
  recoveryNoSecretAcknowledged = false;
  activeService = null;
  activeHomeAppId = null;
  await refreshIdentityScopedState();
  burnResult = {
    tone: presentation.tone,
    message: presentation.message,
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
  return `<aside class="update-banner" role="status"><span><strong>OSL ${escapeHtml(updateStatus.next)} is available</strong><small>Verified against OSL's updater key · installation requires your click</small></span><div><button class="button compact" data-update-read>Read more on GitHub</button><button class="button compact primary" data-update-modal ${updateStatus.state === "installing" ? "disabled" : ""}>Install</button></div></aside>`;
}

function updateDialogMarkup(): string {
  if (updateStatus.state !== "available" && updateStatus.state !== "installing") return "";
  const notes = updateStatus.notes ? escapeHtml(updateStatus.notes) : "No release notes were provided.";
  return `<dialog class="unlock-dialog update-dialog" id="update-dialog" aria-labelledby="update-dialog-title"><div class="unlock-card"><p class="eyebrow">OSL update</p><h2 id="update-dialog-title">Install ${escapeHtml(updateStatus.next)}?</h2><p class="update-notes">${notes}</p><p class="quiet-note">OSL will download, verify, install, and restart. Unsaved work may be lost. Nothing installs until you click Install & restart.</p><div class="control-row unlock-actions"><button class="button ghost" data-update-close>Not now</button><button class="button" data-update-read>Read more on GitHub</button><button class="button primary" data-update-install ${updateStatus.state === "installing" ? "disabled" : ""}>${updateStatus.state === "installing" ? "Installing…" : "Install & restart"}</button></div></div></dialog>`;
}

function updateSettingsContent(): string {
  const deviceReady = isCoreProtectionReady(core.readiness);
  const status = updateStatus.state === "checking" ? "Checking..."
    : updateStatus.state === "upToDate" ? `Up to date · ${escapeHtml(updateStatus.current)}`
    : updateStatus.state === "available" ? `Update available · ${escapeHtml(updateStatus.next)}`
    : updateStatus.state === "installing" ? "Downloading and verifying..."
    : updateStatus.state === "couldNotCheck" ? "Could not check for updates"
    : updateStatus.state === "error" ? "Update check result unreadable"
    : "Updater backend unavailable";
  const detail = updateStatus.state === "checking" ? "Contacting OSL's signed update channel."
    : updateStatus.state === "upToDate" ? "Last check reached OSL's updater. This install is current."
    : updateStatus.state === "available" ? "OSL can receive this signed update after you approve installation."
    : updateStatus.state === "installing" ? "Downloading and verifying the signed update package."
    : updateStatus.state === "couldNotCheck" || updateStatus.state === "error"
      ? "OSL cannot currently receive updates. Report problems manually instead of waiting for a fix."
      : "The desktop updater is not available in this build. Report problems manually instead of waiting for a fix.";
  const stateName = updateStatus.state.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
  const actions = updateStatus.state === "available" ? `<button class="button" data-update-read>Read more on GitHub</button><button class="button primary" data-update-modal>Install</button>` : "";
  return `<h2>About</h2><div class="update-status-card" data-update-state="${stateName}"><span class="dot"></span><div><strong>${status}</strong><small>${detail}</small></div></div><div class="settings-actions"><button class="button ${updateStatus.state === "available" ? "" : "primary"}" data-update-check ${updateStatus.state === "checking" || updateStatus.state === "installing" ? "disabled" : ""}>Check for updates</button>${actions}<button class="button" id="replay-onboarding-tour" type="button">Replay protected messaging tour</button></div><details class="settings-disclosure update-details"><summary>Update privacy</summary><p>Checks and installs use the trusted local updater. Every update package is verified against OSL's own signing key before it installs. Release notes are plain text; remote HTML is never rendered.</p><p>OSL is not code-signed by a publisher Windows recognises, so Windows may warn you about the installer. That is separate from the update check above, which does not rely on Windows.</p></details>${developerSettingsContent()}<details class="device-diagnostics settings-disclosure"><summary><span><strong>Device status</strong><small>${deviceReady ? "Ready" : "Needs attention"}</small></span></summary><p>${escapeHtml(coreReadinessLabel(core.readiness))}</p></details>`;
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
  document.querySelector<HTMLButtonElement>("#replay-onboarding-tour")?.addEventListener("click", () => {
    replayingOnboardingTour = true;
    onboardingTourStep = 0;
    onboardingRoute = "tutorial";
    route = "onboarding";
    render();
  });
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
  void setScreenshotProtection(windowCaptureEnabled).then(() => {
    screenshotProtectionEnabled = windowCaptureEnabled && captureProtectionEnforced();
    if (screenshotProtectionEnabled) return;
    if (!windowCaptureEnabled) return;
    if (route === "settings" && settingsSection === "scrub") render();
    showToast("Windows capture resistance is unavailable on this Windows session");
  });
  if (route === "onboarding") return;
  // The second half of the D-108 construction site. loadUiPreferences() runs
  // before the unlock screen, so on a password-gated profile the key command
  // refuses there and the store stays absent; by here the gate is open, so this
  // is the point at which such a profile actually gets migrated off plaintext.
  void ensureOslChatSecureLocalStore();
  void openMullvadOnStartup();
  void loadHubPasswordRoleStatus().then((status) => { passwordRoleStatus = status; if (route === "settings" && settingsSection === "account") renderWhenIdle(); }).catch(() => undefined);
  void loadInstalledBuildChatWarningStatus().then((warning) => {
    installedBuildChatWarning = warning;
    if (route === "osl-chat") renderWhenIdle();
  }).catch(() => undefined);
  void refreshUpdateStatus(true);
  void refreshAutoScrubFleetStatus();
  void getOslUsernameStatus("osl").catch(() => null);
  void loadFriendProfile().then((profile) => { friendCode = profile?.friendCode ?? null; friendDisplayId = profile?.oslUserId ?? null; if (route === "home") renderWhenIdle(); });
  void listHubPeople().then(async (people) => {
    hubPeople = people ?? [];
    await loadFriendFutureAccountSwitchStates();
    if (route === "home") renderWhenIdle();
  });
  if (notificationsEnabled) void setNotificationsEnabled(true).then(async (enabled) => {
    appNotifications = enabled ? mergePersistedOslChatNotifications(await loadAppNotifications()) : null;
    if (route === "home") renderWhenIdle();
  });
  void refreshIdentitySlots(true);
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
    const detectedAccountsRequest = withNativeDeadline(loadDetectedAccounts(), "Load detected accounts", bootSupportDeadlineMs).catch(() => null);
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
      coverInsertion: null,
      showPlaintextPreview: true,
      windowCaptureEnabled: true,
      rnWirePolicyRequested,
      forwardSecrecyMode: "keepGroupDelivery" as const,
    };
    await recoveryKitUnsavedFlag.load();
    if (attempt !== bootstrapEpoch) return;
    setup = preferences.setup;
    coverInsertion = preferences.coverInsertion;
    windowCaptureEnabled = preferences.windowCaptureEnabled;
    rnWirePolicyRequested = preferences.rnWirePolicyRequested;
    onboardingComplete = preferences.onboardingComplete;
    forwardSecrecyMode = preferences.forwardSecrecyMode;
    if (discordQaShell) {
      // Native startup has already loaded or created the device-sealed
      // disposable QA identity. Never route that identity through consumer
      // onboarding, even when saved preferences are missing or stale.
      route = "service";
      serviceGuideStep = null;
      void startDiscordQaShell();
    } else if (core.readiness.bootstrapStatus === "identityKeyLost") {
      // D-207/D-150. This branch sits ABOVE both of the others on purpose: the
      // account exists, so `welcome` would lie, and no password can open it, so
      // `unlock` would send the user to type something that cannot work.
      onboardingRoute = "keylost";
      route = "onboarding";
    } else if (core.readiness.bootstrapStatus === "setupRequired") {
      onboardingRoute = "welcome";
      route = "onboarding";
    } else if (core.readiness.bootstrapStatus === "passwordRequired") {
      onboardingRoute = "unlock";
      route = "onboarding";
    } else {
      // T15-A8: "Remind me later" is a real state, not a dismissal. While a
      // recovery kit is unsaved the launch lands back on the recovery step
      // even for an account that already finished onboarding.
      const recoveryKitOutstanding = recoveryKitUnsavedFlag.unsaved();
      route = preferences.onboardingComplete && !recoveryKitOutstanding ? "home" : "onboarding";
      if (route === "onboarding") onboardingRoute = pendingOnboardingRoute() ?? onboardingRouteForBuild("passwords");
    }
    // startDiscordQaShell paints the service route after loading only the two
    // catalogs it needs. Until then, retain the neutral loading screen rather
    // than flashing any consumer setup or home surface.
    if (!discordQaShell) renderNow();
    if (route === "onboarding" && onboardingRoute === "browser") void refreshBrowserImportReadiness();
    if (route === "onboarding" && onboardingRoute === "mullvad") void refreshMullvadSetup();
    startReadyWorkspaceLoads();
    void refreshBuildIntegrityStatus();
    void Promise.all([servicesRequest, detectedAccountsRequest, nativeAppsRequest, licenseRequest, browserCompanionRequest, browserProfilesRequest]).then(([linkedServices, loadedDetectedAccounts, nativeCatalog, currentLicenseState, currentBrowserCompanionStatus, profiles]) => {
      if (attempt !== bootstrapEpoch) return;
      if (linkedServices) {
        services = linkedServices;
        linkedServicesChecked = true;
      }
      if (loadedDetectedAccounts) {
        detectedAccounts = loadedDetectedAccounts;
        persistDetectedAccountOpeningChoices();
      }
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

if (!runningUnderVitest && !fixedNoRecoverySecretFixture) {
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", () => { if (themeChoice === "system") applyTheme("system"); });
  window.addEventListener("keydown", (event) => {
    if (event.key !== "F11" || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    runDesktopShortcutAction();
  });
  window.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (closeTopmostClosableLayerForEscape()) event.preventDefault();
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
  nativeHostRealignment.armHeartbeat();
  nativeHostResizeFrame = requestAnimationFrame(() => {
    nativeHostResizeFrame = 0;
    nativeHostRealignment.acknowledgeAnimationFrame();
    void validateNativeSurfaces();
  });
}
function scheduleOslChatBackgroundSync(delayMs = 30_000): void {
  oslChatDelivery.start(delayMs);
}

type OslHubUiTestStatePatch = {
  route?: Route;
  onboardingRoute?: OnboardingRoute;
  onboardingComplete?: boolean;
  onboardingTourStep?: number;
  setup?: Partial<SetupState>;
  silentVisibleMode?: SilentVisibleMode | null;
  coverInsertion?: CoverInsertionChoice | null;
  coreReady?: boolean;
  activeOslUserId?: string | null;
  storageMethod?: string | null;
  services?: LinkedService[];
  servicesChecked?: boolean;
  hubPeople?: Array<Partial<HubPerson> & { personId: string }>;
  notificationsEnabled?: boolean;
  notificationSecurityActivity?: boolean;
  notificationScopeSuggestions?: boolean;
  notificationChatActivity?: boolean;
  notificationPreviewContent?: boolean;
  notificationAppPreferences?: Partial<Record<ServiceId, boolean>>;
  oslChatPreviewsVisible?: boolean;
  oslChatMutedPeople?: string[];
  appNotifications?: AppNotification[];
  mullvadAvailability?: MullvadStatus["availability"];
  protectionPreset?: ProtectionPreset;
  inboxFilter?: InboxFilter;
  oslMailNotifications?: boolean;
  licenseAccess?: HubLicenseState["access"];
  forwardSecrecyChoice?: ForwardSecrecyChoice | null;
  forwardSecrecyMode?: "protectPast" | "keepGroupDelivery";
  autoScrubFleetStatus?: AutoScrubFleetStatus | null;
  hubIdentities?: HubIdentitySlot[];
  hubIdentitiesLoad?: IdentityListLoad;
  bootstrapStatus?: BootstrapStatus;
  identityDiscoveryChoice?: IdentityDiscoveryChoice | null;
  enclaveAudienceRecords?: unknown[];
  buildIntegrityStatus?: BuildIntegrityStatus | null;
  activeOslChatPersonId?: string | null;
  activeOslChatScopeApproved?: boolean;
  oslChatDraft?: string;
  recoveryBundle?: { userId: string; identityPhrase: string | null; passwordPhrase: string } | null;
  recoveryKitUnsaved?: boolean;
  recoveryCaptureProven?: boolean;
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

function browserAccountFinderAreaLabel(browserId: BrowserImportId, profile: string): string {
  const displayName = browserProfiles.find((candidate) =>
    candidate.browserId === browserId && candidate.profile === profile)?.displayName ?? profile;
  const browserName = browserImports.find((browser) => browser.id === browserId)?.displayName ?? browserId;
  return `${browserName} · ${displayName}`;
}

function browserAccountFinderSnapshotForTest(): {
  route: Route;
  onboardingRoute: OnboardingRoute;
  areaNames: string[];
  selectedAreaNames: string[];
  recordedAreaNames: string[];
  recordedAccounts: string[];
  accountCount: number;
  failureNotice: string;
} {
  const profileByKey = new Map(browserProfiles.map((profile) => [browserProfileKey(profile), profile]));
  return {
    route,
    onboardingRoute,
    areaNames: browserProfiles.map((profile) => browserAccountFinderAreaLabel(profile.browserId, profile.profile)),
    selectedAreaNames: [...selectedBrowserProfileKeys].flatMap((key) => {
      const profile = profileByKey.get(key);
      return profile ? [browserAccountFinderAreaLabel(profile.browserId, profile.profile)] : [];
    }),
    recordedAreaNames: browserFootprintImports.map((receipt) =>
      browserAccountFinderAreaLabel(receipt.browserId, receipt.profile)),
    recordedAccounts: browserFootprintImports.map((receipt) =>
      `${browserAccountFinderAreaLabel(receipt.browserId, receipt.profile)}=${receipt.account}`),
    accountCount: browserFootprintImports.length,
    failureNotice: browserImportFailureNotice,
  };
}

function applyTestCoreState(ready: boolean, storageMethod: string | null, bootstrapStatus?: BootstrapStatus, activeOslUserId?: string | null): void {
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
    activeOslUserId: ready ? activeOslUserId ?? "test-osl-user" : null,
    bootstrapStatus: bootstrapStatus ?? (ready ? "ready" : "notAttempted"),
    storageMethod,
  };
}

function applyOslHubUiTestState(patch: OslHubUiTestStatePatch = {}): void {
  oslChatSendRouteAttempts.clear();
  route = patch.route ?? "home";
  onboardingRoute = patch.onboardingRoute ?? "welcome";
  onboardingComplete = patch.onboardingComplete ?? false;
  onboardingTourStep = patch.onboardingTourStep ?? 0;
  replayingOnboardingTour = false;
  identityDiscoveryChoice = patch.identityDiscoveryChoice === undefined
    ? storedIdentityDiscoveryChoice()
    : patch.identityDiscoveryChoice;
  privateContactLink = null;
  privateContactLinkBusy = false;
  privateContactLinkError = "";
  identityChoiceError = "";
  setup = { ...defaultSetup, ...patch.setup };
  silentVisibleMode = patch.silentVisibleMode ?? null;
  coverInsertion = patch.coverInsertion ?? initialCoverInsertionChoice();
  deleteChoices = initialDeleteChoices();
  torOnboarding = initialTorOnboardingState();
  settingsSection = "account";
  activeService = null;
  activeHomeAppId = null;
  activeOslChatPersonId = patch.activeOslChatPersonId ?? null;
  activeOslChatContext = activeOslChatPersonId
    ? {
        contextToken: `test-chat-context-${activeOslChatPersonId}`,
        serviceId: "osl-chat",
        accountId: "osl-main",
        personId: activeOslChatPersonId,
        peerOslUserId: `OSLUSER-${activeOslChatPersonId}`,
        scopeApproved: patch.activeOslChatScopeApproved ?? true,
      }
    : null;
  activeOslChatPersonId = null;
  activeOslChatContext = null;
  oslChatBusy = false;
  serviceAccountPickerOpen = false;
  friendsDialogOpen = false;
  homeEditMode = false;
  homeNotificationsOpen = false;
  homeFriendsPanelCollapsed = false;
  homeTileOrder = [];
  hiddenHomeTiles.clear();
  homeTileArrangementNotice = "";
  ownedConfirmation = null;
  ownedConfirmationBusy = false;
  ownedConfirmationError = "";
  privacyProtectionReviewOpen = false;
  activityAttentionReviewOpen = false;
  peoplePrimaryActionFocus = null;
  nativeCatalogRefusal = null;
  protectionPreset = patch.protectionPreset ?? loadProtectionPreset();
  inboxFilter = patch.inboxFilter ?? "all";
  recoveryBundle = patch.recoveryBundle ?? null;
  recoverySavedAcknowledged = false;
  recoveryNoSecretAcknowledged = false;
  recoveryShownWithoutProtection = false;
  recoveryWordCheckState = initialRecoveryWordCheckState();
  recoveryWordCheckEpoch += 1;
  recoveryCaptureGate.invalidate();
  if (patch.recoveryCaptureProven) recoveryCaptureGate.accept(recoveryCaptureGate.checkpoint());
  resetAccountRecovery();
  recoveryBundle = patch.recoveryBundle ?? null;
  recoverySavedAcknowledged = false;
  recoveryShownWithoutProtection = false;
  void recoveryKitUnsavedFlag.set(patch.recoveryKitUnsaved ?? false);
  cleanDeviceRestoreState = initialCleanDeviceRestoreState;
  oslMailNotifications = patch.oslMailNotifications ?? (localStorage.getItem(oslMailNotificationsStorageKey) !== "false");
  oslMailThreadSyncUnavailable = false;
  oslMailThreads = [];
  oslMailActiveThread = null;
  oslMailError = null;
  oslMailDeleteReceipt = null;
  oslMailSendReceipt = null;
  oslMailBurnReceipt = null;
  oslMailComposeDraft = { to: "", subject: "", body: "" };
  escapeAuditSendAttempts = 0;
  services = patch.services ?? [];
  detectedAccounts = [];
  detectedAccountOpeningChoices.clear();
  linkedServicesChecked = patch.servicesChecked ?? patch.services !== undefined;
  hubPeople = (patch.hubPeople ?? []).map(testHubPerson);
  oslChatDraft = patch.oslChatDraft ?? "";
  friendFutureAccountAutoWhitelist.clear();
  friendFutureAccountAutoWhitelistBusy.clear();
  hubIdentities = patch.hubIdentities ?? [];
  hubIdentitiesLoad = patch.hubIdentitiesLoad ?? (patch.hubIdentities ? "loaded" : "pending");
  identityListRefreshInFlight = false;
  privateEnclaveAudiences = parsedEnclaveAudiences(patch.enclaveAudienceRecords ?? []);
  notificationsEnabled = patch.notificationsEnabled ?? false;
  notificationAppPreferences = { ...patch.notificationAppPreferences };
  notificationChatActivity = patch.notificationChatActivity ?? true;
  notificationSecurityActivity = patch.notificationSecurityActivity ?? true;
  notificationPreviewContent = patch.notificationPreviewContent ?? true;
  notificationScopeSuggestions = patch.notificationScopeSuggestions ?? true;
  oslChatPreviewsVisible = patch.oslChatPreviewsVisible ?? true;
  oslChatMutedPeople = new Set(patch.oslChatMutedPeople ?? []);
  appNotifications = patch.appNotifications ?? [];
  licenseState = { ...unconfiguredLicenseState, access: patch.licenseAccess ?? "free" };
  buildIntegrityStatus = patch.buildIntegrityStatus ?? null;
  proOnboardingReadyResult = false;
  proOnboardingCodeEntryRequested = false;
  forwardSecrecyOnboarding = { choice: patch.forwardSecrecyChoice ?? null };
  forwardSecrecyMode = patch.forwardSecrecyMode ?? "keepGroupDelivery";
  autoScrubFleetStatus = patch.autoScrubFleetStatus ?? null;
  autoScrubStatusLoading = false;
  autoScrubStopPending = false;
  mullvadStatus = {
    availability: patch.mullvadAvailability ?? "unavailable",
    integrationState: patch.mullvadAvailability === "installed" ? "availableToOpen" : patch.mullvadAvailability === "installable" ? "installable" : "unavailable",
    privacyScope: "networkOnly",
    connectionState: "notObserved",
  };
  mullvadSetupRoute = parseMullvadSetupRoute(localStorage.getItem(mullvadSetupRouteStorageKey));
  mullvadSetupNotice = "";
  mullvadBusy = false;
  applyTestCoreState(patch.coreReady ?? false, patch.storageMethod ?? null, patch.bootstrapStatus, patch.activeOslUserId);
  homeEditMode = false;
  homeTilePreferenceOwner = null;
  syncHomeTilePreferencesForActiveProfile();
}

export type BusyButtonAuditRow = {
  action: string;
  button: string;
  runningDisabled: boolean;
  secondPressCount: number;
  afterSuccessDisabled: boolean;
  afterFailureDisabled: boolean;
};

function testLinkedService(id: HomeAppId = "gmail"): LinkedService {
  const serviceId = id === "gmail" ? "email" : id;
  return {
    id: serviceId,
    displayName: id === "discord" ? "Discord" : "Gmail",
    sidebarGlyph: id.slice(0, 2).toUpperCase(),
    sidebarOrder: 0,
    category: "consumer",
    launchState: "available",
    supportsNativePreview: true,
    supportsProtectedPreview: true,
    accounts: [{
      id: "account-1",
      label: "Personal",
      provider: id === "gmail" ? "gmail" : null,
      connected: true,
    }],
  } as unknown as LinkedService;
}

function resetBusyButtonAuditState(): void {
  nativeCatalogBusy = false;
  browserReadinessBusy = false;
  browserImportBusy = false;
  browserImportCancelling = false;
  browserImportQueue = [];
  browserImportQueueIndex = 0;
  browserImportSourceSelected = false;
  mullvadBusy = false;
  appLaunchPendingId = null;
  serviceGuideStep = null;
  activeEmbeddedHost = null;
  activeNativeHostId = null;
  activeNativeHostMode = null;
  activeDefaultBrowserCompanion = false;
  backgroundInstallIds.clear();
  nativeActionBusy = false;
  protectedSheetCloseBusy = false;
  nativeProtectBusy = false;
  discordQaHeaderBusy = null;
  discordQaRowProofState = "idle";
  discordQaComposerBusy = false;
  discordQaOneClickBusy = false;
  oslChatBusy = false;
  oslChatMessages.clear();
  oslChatAttachments = [];
  oslChatSettingsPersonId = null;
  burnDialogOpen = false;
  updateStatus = { state: "upToDate", current: "0.1.0" };
}

function buttonStartTag(markup: string, marker: string): string {
  const markerIndex = markup.indexOf(marker);
  if (markerIndex < 0) throw new Error(`busy button audit marker not rendered: ${marker}`);
  const start = markup.lastIndexOf("<button", markerIndex);
  const end = markup.indexOf(">", markerIndex);
  if (start < 0 || end < markerIndex) throw new Error(`busy button audit marker is not inside a button: ${marker}`);
  return markup.slice(start, end + 1);
}

function buttonIsDisabled(markup: string, marker: string): boolean {
  return /\sdisabled(?:[\s=>]|$)/u.test(buttonStartTag(markup, marker));
}

function seedServiceForBusyButtonAudit(appId: HomeAppId = "gmail"): LinkedService {
  const service = testLinkedService(appId);
  services = [service];
  linkedServicesChecked = true;
  activeService = service;
  activeHomeAppId = appId;
  return service;
}

function seedOslChatForBusyButtonAudit(approved = true): void {
  const person = testHubPerson({
    personId: "person-1",
    alias: "Avery",
    safetyNumberVerified: true,
  });
  hubPeople = [person];
  activeOslChatPersonId = person.personId;
  activeOslChatContext = {
    contextToken: "context-1",
    serviceId: "osl-chat",
    accountId: "account-1",
    personId: person.personId,
    peerOslUserId: person.oslUserId,
    scopeApproved: approved,
  };
  oslChatDraft = "hello";
  oslChatMessages.set(person.personId, [{
    messageId: "incoming-1",
    direction: "incoming",
    body: "ready",
    state: "received",
    timestampLabel: "now",
  }]);
}

function longRunningButtonAuditForTest(): BusyButtonAuditRow[] {
  type Scenario = {
    action: string;
    button: string;
    marker: string;
    setup: () => void;
    setRunning: () => void;
    render: () => string;
    settledDisabled?: () => boolean;
  };
  const scenarios: Scenario[] = [
    {
      action: "check selected Windows apps",
      button: "#continue-app-choice",
      marker: 'id="continue-app-choice"',
      setup: () => {
        route = "onboarding";
        onboardingRoute = "tutorial";
        selectedOnboardingApps.clear();
        selectedOnboardingApps.add("discord");
      },
      setRunning: () => { nativeCatalogBusy = true; },
      render: () => chooseAppsOnboardingContent(),
    },
    {
      action: "check selected browser areas",
      button: "#import-saved-accounts",
      marker: 'id="import-saved-accounts"',
      setup: () => {
        route = "onboarding";
        onboardingRoute = "browser";
        browserProfiles = [{ browserId: "chrome", profile: "Default", displayName: "Default" }];
        browserImports = [{ id: "chrome", displayName: "Chrome", installed: true }];
        selectedBrowserProfileKeys = new Set([browserProfileKey(browserProfiles[0]!)]);
      },
      setRunning: () => { browserImportBusy = true; },
      render: () => browserImportContent(),
    },
    {
      action: "skip while browser area check is running",
      button: "#continue-browser-import",
      marker: 'id="continue-browser-import"',
      setup: () => {
        route = "onboarding";
        onboardingRoute = "browser";
        browserProfiles = [{ browserId: "chrome", profile: "Default", displayName: "Default" }];
        browserImports = [{ id: "chrome", displayName: "Chrome", installed: true }];
      },
      setRunning: () => { browserImportBusy = true; },
      render: () => browserImportContent(),
    },
    {
      action: "install Mullvad",
      button: "#install-mullvad",
      marker: 'id="install-mullvad"',
      setup: () => { mullvadStatus.availability = "installable"; },
      setRunning: () => { mullvadBusy = true; },
      render: () => mullvadSetupContent(),
    },
    {
      action: "open Mullvad",
      button: "#open-mullvad",
      marker: 'id="open-mullvad"',
      setup: () => { mullvadStatus.availability = "installed"; },
      setRunning: () => { mullvadBusy = true; },
      render: () => mullvadSetupContent(),
    },
    {
      action: "open app from launcher",
      button: "[data-home-app]",
      marker: 'data-home-app="discord"',
      setup: () => { route = "settings"; settingsSection = "apps"; seedServiceForBusyButtonAudit("discord"); },
      setRunning: () => { appLaunchPendingId = "discord"; },
      render: () => serviceAccountsSettingsContent(),
    },
    {
      action: "open embedded service",
      button: "#embedded-service-setup",
      marker: 'id="embedded-service-setup"',
      setup: () => { route = "service"; seedServiceForBusyButtonAudit("gmail"); },
      setRunning: () => { nativeActionBusy = true; },
      render: () => serviceContent(),
    },
    {
      action: "background install native app",
      button: "[data-background-install]",
      marker: 'data-background-install="discord"',
      setup: () => {
        route = "service";
        serviceGuideStep = 0;
        seedServiceForBusyButtonAudit("discord");
        nativeApps = [{
          id: "discord",
          displayName: "Discord",
          availability: "installable",
          supportStatus: "available",
          carrierEvidence: "builtNeverProvenLive",
          deliveryEvidence: "neverProvenLive",
          claimBlockers: [],
          claimNote: "Test native app",
          statusPage: {
            capability: "test native app",
            generatedLabel: "Available",
            explanation: "Test native app",
          },
          protectedMode: "assistOnly",
          isolatedProfileAvailable: false,
          supportsOverlay: false,
        }];
      },
      setRunning: () => { backgroundInstallIds.add("discord"); },
      render: () => serviceGuideContent(activeService!, serviceGuideStep!),
    },
    {
      action: "bring native companion forward",
      button: "#native-companion-focus",
      marker: 'id="native-companion-focus"',
      setup: () => {
        route = "service";
        seedServiceForBusyButtonAudit("discord");
        activeNativeHostId = "discord";
        activeNativeHostMode = "existingSession";
      },
      setRunning: () => { nativeActionBusy = true; },
      render: () => serviceContent(),
    },
    {
      action: "toggle protected sheet",
      button: "#local-protected-toggle",
      marker: 'id="local-protected-toggle"',
      setup: () => { route = "service"; seedServiceForBusyButtonAudit("gmail"); },
      setRunning: () => { protectedSheetCloseBusy = true; },
      render: () => trustedHeader(),
    },
    {
      action: "open native Discord protection",
      button: "#native-protect-verified-peer",
      marker: 'id="native-protect-verified-peer"',
      setup: () => {
        activeNativeHostId = "discord";
        nativeProtectPickerOpen = true;
        hubPeople = [testHubPerson({ personId: "person-1", alias: "Avery", safetyNumberVerified: true })];
      },
      setRunning: () => { nativeProtectBusy = true; },
      render: () => nativeDiscordProtectPickerMarkup(),
    },
    {
      action: "OSL Chat approval",
      button: "#osl-chat-approve",
      marker: 'id="osl-chat-approve"',
      setup: () => { route = "osl-chat"; seedOslChatForBusyButtonAudit(false); },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "OSL Chat refresh",
      button: "#osl-chat-refresh",
      marker: 'id="osl-chat-refresh"',
      setup: () => { route = "osl-chat"; seedOslChatForBusyButtonAudit(true); },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "OSL Chat send",
      button: ".osl-chat-send",
      marker: 'class="osl-chat-send"',
      setup: () => { route = "osl-chat"; seedOslChatForBusyButtonAudit(true); },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "OSL Chat choose attachment",
      button: "#osl-chat-attach",
      marker: 'id="osl-chat-attach"',
      setup: () => { route = "osl-chat"; licenseState.access = "pro"; seedOslChatForBusyButtonAudit(true); },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "OSL Chat open attachment",
      button: "[data-osl-chat-attachment]",
      marker: 'data-osl-chat-attachment="attachment-1"',
      setup: () => {
        route = "osl-chat";
        licenseState.access = "pro";
        seedOslChatForBusyButtonAudit(true);
        oslChatAttachments = [{ attachmentId: "attachment-1", originalFilename: "proof.png", plaintextSize: 12, viewOnce: false } as NativeOverlayPendingAttachment];
      },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "OSL Chat permission toggle",
      button: "#osl-chat-permission-toggle",
      marker: 'id="osl-chat-permission-toggle"',
      setup: () => {
        route = "osl-chat";
        seedOslChatForBusyButtonAudit(true);
        oslChatSettingsPersonId = "person-1";
      },
      setRunning: () => { oslChatBusy = true; },
      render: () => oslChatContent(),
    },
    {
      action: "owned confirmation",
      button: "#owned-confirmation-submit",
      marker: 'id="owned-confirmation-submit"',
      setup: () => { ownedConfirmation = { kind: "clearActivation" }; },
      setRunning: () => { ownedConfirmationBusy = true; },
      render: () => ownedConfirmationMarkup(),
    },
    {
      action: "burn confirmation",
      button: "#burn-confirm-submit",
      marker: 'id="burn-confirm-submit"',
      setup: () => { burnDialogOpen = true; burnScope = "account"; },
      setRunning: () => { burnBusy = true; },
      render: () => burnDialogMarkup(),
      settledDisabled: () => false,
    },
    {
      action: "check for updates",
      button: "[data-update-check]",
      marker: "data-update-check",
      setup: () => { settingsSection = "about"; updateStatus = { state: "upToDate", current: "0.1.0" }; },
      setRunning: () => { updateStatus = { state: "checking" }; },
      render: () => updateSettingsContent(),
    },
    {
      action: "install update",
      button: "[data-update-install]",
      marker: "data-update-install",
      setup: () => { updateStatus = { state: "available", current: "0.1.0", next: "0.1.1", notes: "Notes" }; },
      setRunning: () => { updateStatus = { state: "installing", current: "0.1.0", next: "0.1.1", notes: "Notes" }; },
      render: () => updateDialogMarkup(),
    },
  ];

  return scenarios.map((scenario) => {
    applyOslHubUiTestState({ coreReady: true });
    resetBusyButtonAuditState();
    scenario.setup();
    scenario.setRunning();
    const runningDisabled = buttonIsDisabled(scenario.render(), scenario.marker);
    const secondPressCount = runningDisabled ? 0 : 1;

    applyOslHubUiTestState({ coreReady: true });
    resetBusyButtonAuditState();
    scenario.setup();
    const afterSuccessDisabled = scenario.settledDisabled?.() ?? buttonIsDisabled(scenario.render(), scenario.marker);

    applyOslHubUiTestState({ coreReady: true });
    resetBusyButtonAuditState();
    scenario.setup();
    const afterFailureDisabled = scenario.settledDisabled?.() ?? buttonIsDisabled(scenario.render(), scenario.marker);

    return {
      action: scenario.action,
      button: scenario.button,
      runningDisabled,
      secondPressCount,
      afterSuccessDisabled,
      afterFailureDisabled,
    };
  });
}

type OslChatEnterAuditState = {
  name: string;
  category: "screen" | "dialog" | "text-box" | "disabled-send" | "enabled-send";
  mark: string;
  found: boolean;
  deliveries: number;
  markDisposition: "retained" | "refused" | "cleared-by-send";
};

type OslChatEnterAuditReport = {
  markerPrefix: string;
  statesFound: number;
  statesTried: number;
  deliberateDeliveries: number;
  accidentalDeliveries: number;
  states: OslChatEnterAuditState[];
};

function seedOslChatEnterAuditState(options: {
  friend?: Partial<HubPerson> & { personId: string };
  active?: boolean;
  approved?: boolean;
  busy?: boolean;
  draft?: string;
  settingsOpen?: boolean;
}): void {
  const friend = options.friend ? testHubPerson(options.friend) : null;
  hubPeople = friend ? [friend] : [];
  route = "osl-chat";
  activeOslChatPersonId = options.active && friend ? friend.personId : null;
  activeOslChatContext = options.approved && friend
    ? {
        contextToken: `audit-context-${friend.personId}`,
        serviceId: "osl-chat",
        accountId: "osl-main",
        personId: friend.personId,
        peerOslUserId: friend.oslUserId,
        scopeApproved: true,
      } as ManualPeerContext
    : null;
  oslChatBusy = options.busy ?? false;
  oslChatSettingsPersonId = options.settingsOpen && friend ? friend.personId : null;
  oslChatDraft = options.draft ?? "";
  oslChatMessages.clear();
}

function auditSendButtonDisabled(markup: string): boolean {
  return /class="osl-chat-send"[^>]*\bdisabled\b/u.test(markup);
}

async function auditOslChatSubmitEnter(): Promise<number> {
  const before = activeOslChatPersonId ? (oslChatMessages.get(activeOslChatPersonId) ?? []).length : 0;
  await sendOslChat({ preventDefault: () => undefined } as SubmitEvent);
  const after = activeOslChatPersonId ? (oslChatMessages.get(activeOslChatPersonId) ?? []).length : 0;
  return after - before;
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
  setOslChatForTest(personId: string, draft: string, scopeApproved = true): void {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    if (!person) throw new Error(`Unknown test chat person: ${personId}`);
    route = "osl-chat";
    activeOslChatPersonId = personId;
    oslChatSettingsPersonId = personId;
    activeOslChatContext = {
      contextToken: `test-context-${personId}`,
      serviceId: "osl-chat",
      accountId: "local",
      personId,
      peerOslUserId: person.oslUserId,
      scopeApproved,
    };
    oslChatBusy = false;
    setOslChatDraft(draft);
  },
  renderOslChatForTest(): string {
    route = "osl-chat";
    return oslChatContent();
  },
  sendOslChatForTest(route: OslChatSendRoute = "enter"): Promise<void> {
    return sendOslChatFromRoute(route);
  },
  sendOslChatAttachmentForTest(): Promise<void> {
    return sendOslChatAttachment();
  },
  async verifyHubPersonAndRefreshForTest(personId: string, safetyNumber: string): Promise<boolean> {
    const verified = await verifyHubPerson(personId, safetyNumber);
    if (verified) hubPeople = await listHubPeople() ?? hubPeople;
    return verified;
  },
  renderOslChatSettingsForTest(personId: string): string {
    const person = hubPeople.find((candidate) => candidate.personId === personId);
    if (!person) throw new Error(`Unknown test chat person: ${personId}`);
    oslChatSettingsPersonId = personId;
    return oslChatFriendSettingsMarkup(person);
  },
  toggleOslChatPermissionForTest(): Promise<void> {
    return toggleOslChatPermission();
  },
  oslChatSendRouteAttemptsForTest(): Record<OslChatSendRoute, number> {
    return Object.fromEntries(OSL_CHAT_SEND_ROUTES.map((route) => [route, oslChatSendRouteAttempts.get(route) ?? 0])) as Record<OslChatSendRoute, number>;
  },
  pasteOslChatClipboardImage(event: ClipboardEvent): Promise<void> {
    return pasteOslChatClipboardImage(event);
  },
  oslChatAttachmentCards(): string[] {
    return [...attachmentProgressByContext.values()].map((event) => event.job.metadata.mediaType);
  },
  renderUpdateScreenForTest(current: string, next: string, notes: string): string {
    updateStatus = { state: "available", current, next, notes };
    return updateDialogMarkup();
  },
  pressUpdateNowForTest(): Promise<void> {
    return installUpdateAfterClick();
  },
  homeTileIdsForTest(): string[] {
    return currentHomeTileIds();
  },
  setHomeEditModeForTest(enabled: boolean): void {
    homeEditMode = enabled;
  },
  toggleHomeTileForTest(id: string): void {
    toggleHomeTile(id);
  },
  reloadHomeTilesForTest(): void {
    homeTilePreferenceOwner = null;
    syncHomeTilePreferencesForActiveProfile();
  },
  /** The route a Home launcher tile actually opens, so a test can pin it. */
  openHomeModuleForTest(id: string): Route {
    openHomeModule(id);
    return route;
  },
  saveHomeTileArrangementForTest(hiddenIds: string[]): HomeTileArrangementSaveResult {
    return saveHomeTileArrangement(new Set(hiddenIds));
  },
  /** Opens/closes the Home bell popover so a test can assert the re-homed
   * protection recommendation really renders (homeStatusSnapshot, task 0825). */
  setHomeNotificationsOpenForTest(open: boolean): void {
    homeNotificationsOpen = open;
  },
  renderSettingsSection(section: SettingsSection): string {
    route = "settings";
    settingsSection = section;
    return workspaceContent();
  },
  /** The crash view's markup, so a test can pin its machine-readable marker. */
  renderRecoveryMarkupForTest(): string {
    return renderRecoveryMarkup();
  },
  renderRouteShell(destination: Route): string {
    route = destination;
    return destination === "onboarding" ? onboardingShellMarkup() : workspaceShellMarkup();
  },
  paintOnboardingRouteForTest(destination: OnboardingRoute): void {
    route = "onboarding";
    onboardingRoute = onboardingRouteForBuild(destination);
    forceOnboardingPaint = true;
    renderOnboarding();
  },
  flushRenderForTest(): void {
    renderNow();
  },
  renderOnboardingSetupShell(destination: OnboardingRoute): string {
    route = "onboarding";
    onboardingRoute = onboardingRouteForBuild(destination);
    renderOnboarding();
    return root.innerHTML;
  },
  renderOnboardingCaptureShell(destination: OnboardingRoute): string {
    route = "onboarding";
    onboardingRoute = onboardingRouteForBuild(destination);
    return onboardingShellMarkup(onboardingSetupNavigationMarkup());
  },
  bindOnboarding(): void {
    bindOnboarding();
  },
  quickTourSnapshot(): QuickTourControlResult {
    return quickTourControlResult("Set card", true);
  },
  setQuickTourCardNumber(cardNumber: number): QuickTourControlResult {
    return setQuickTourCardNumber(cardNumber);
  },
  chooseAppsFromQuickTour(): QuickTourControlResult {
    return chooseAppsFromQuickTour();
  },
  nextQuickTour(): QuickTourControlResult {
    return nextQuickTour();
  },
  backQuickTour(): QuickTourControlResult {
    return backQuickTour();
  },
  cleanDeviceRestoreSnapshot(): CleanDeviceRestoreState {
    return { ...cleanDeviceRestoreState };
  },
  recoveryKitSnapshot(): {
    onboardingRoute: OnboardingRoute;
    bundle: typeof recoveryBundle;
    savedAcknowledged: boolean;
    wordCheck: RecoveryWordCheckState;
  } {
    return {
      onboardingRoute,
      bundle: recoveryBundle ? { ...recoveryBundle } : null,
      savedAcknowledged: recoverySavedAcknowledged,
      wordCheck: recoveryWordCheckState,
    };
  },
  bindWorkspace(): void {
    bindWorkspace();
    mountChatSurfaceOverlays();
  },
  /** Render the real service header without needing a companion window. */
  /**
   * Render the real service header without a companion window.
   *
   * Accepts either a full LinkedService plus its app id, OR just an app id --
   * L1 "encrypt and copy" must be reachable for EVERY launchable app, so tests
   * that sweep all shipping app ids should not each have to hand-build a
   * service. Passing only the id synthesises a minimal one for that app.
   */
  renderServiceHeader(
    serviceOrAppId: LinkedService | HomeAppId,
    homeAppId?: HomeAppId,
  ): string {
    const idOnly = typeof serviceOrAppId === "string";
    const appId = (idOnly ? serviceOrAppId : homeAppId) as HomeAppId;
    const service: LinkedService = idOnly
      ? {
          id: appId,
          displayName: appId,
          sidebarGlyph: appId.slice(0, 2).toUpperCase(),
          sidebarOrder: 0,
          category: "consumer",
          launchState: "available",
          generatedLabel: "Not started",
          supportsNativePreview: true,
          supportsProtectedPreview: true,
          accounts: [],
        } as unknown as LinkedService
      : serviceOrAppId;
    route = "service";
    activeService = service;
    activeHomeAppId = appId;
    activeEmbeddedHost = null;
    activeNativeHostId = null;
    activeNativeHostMode = null;
    activeDefaultBrowserCompanion = false;
    return trustedHeader();
  },
  openLocalProtection(): Promise<void> {
    return toggleLocalProtectedSheet();
  },
  /** Configure the native Discord branch for routing tests without a window. */
  useNativeDiscordProtectionForTest(): void {
    activeEmbeddedHost = null;
    activeNativeHostId = "discord";
    activeNativeHostMode = "dedicated";
    activeDefaultBrowserCompanion = false;
    nativeDiscordProtectionActive = false;
    nativeProtectPickerOpen = false;
    localProtectedSheet = blankLocalProtectedModel();
    peerProtectedSheet = blankPeerProtectedModel();
  },
  startLocalProtection(label: string): Promise<void> {
    return startLocalProtectedContextForLabel(label);
  },
  prepareLocalProtectedDraft(
    plaintext: string,
    options: { ttlSeconds?: number; viewOnce?: boolean } = {},
  ): Promise<void> {
    return prepareLocalProtectedDraftFromValues(
      plaintext,
      options.ttlSeconds ?? localProtectedSheet.ttlSeconds,
      options.viewOnce ?? localProtectedSheet.viewOnce,
    );
  },
  renderProtectedSheets(): string {
    return workspaceProtectedSheetMarkup();
  },
  renderDialogSurfaceForTest(
    name: "friends" | "people-in-chat" | "whitelist-roster" | "native-protect-friend" | "scrub-review" | "burn" | "owned-confirmation" | "update" | "osl-chat-settings",
  ): string {
    friendsDialogOpen = false;
    whitelistRosterOpen = false;
    nativeProtectPickerOpen = false;
    scrubReviewOpen = false;
    burnDialogOpen = false;
    ownedConfirmation = null;
    updateStatus = { state: "unavailable" };
    oslChatSettingsPersonId = null;
    activeNativeHostId = null;
    activeNativeHostMode = null;
    activeService = null;
    activeHomeAppId = null;

    if (name === "friends") {
      route = "home";
      friendsDialogOpen = true;
      return friendsDialogMarkup();
    }
    if (name === "people-in-chat") {
      this.renderServiceHeader("discord");
      return peopleDialogMarkup();
    }
    if (name === "whitelist-roster") {
      whitelistRosterOpen = true;
      return whitelistRosterMarkup();
    }
    if (name === "native-protect-friend") {
      activeNativeHostId = "discord";
      activeNativeHostMode = "dedicated";
      nativeProtectPickerOpen = true;
      return nativeDiscordProtectPickerMarkup();
    }
    if (name === "scrub-review") {
      scrubReviewOpen = true;
      return scrubReviewDialogMarkup();
    }
    if (name === "burn") {
      burnDialogOpen = true;
      burnScope = "account";
      return burnDialogMarkup();
    }
    if (name === "owned-confirmation") {
      ownedConfirmation = { kind: "clearActivation" };
      return ownedConfirmationMarkup();
    }
    if (name === "update") {
      updateStatus = { state: "available", current: "0.1.0", next: "0.1.1", notes: "Focused keyboard travel fixture." };
      return updateDialogMarkup();
    }

    const friend = testHubPerson({ personId: "friend-1", alias: "Verified friend", safetyNumberVerified: true });
    hubPeople = [friend];
    route = "osl-chat";
    activeOslChatPersonId = friend.personId;
    activeOslChatContext = {
      contextToken: "test-context",
      serviceId: "osl-chat",
      accountId: "local",
      personId: friend.personId,
      peerOslUserId: friend.oslUserId,
      scopeApproved: true,
    };
    oslChatSettingsPersonId = friend.personId;
    return oslChatFriendSettingsMarkup(friend);
  },
  /** D80: the rendered onboarding screen, markup only, for the unlock-screen
   * advertisement audit in `unlock-screen-single-credential.test.ts`. */
  renderOnboardingRoute(destination: OnboardingRoute): string {
    route = "onboarding";
    onboardingRoute = onboardingRouteForBuild(destination);
    return onboardingContent();
  },
  renderOnboardingTourStepForTest(step: number): string {
    route = "onboarding";
    onboardingRoute = "tutorial";
    onboardingTourStep = step;
    return onboardingContent();
  },
  renderOnboardingShellForTest(destination: OnboardingRoute): string {
    route = "onboarding";
    onboardingRoute = onboardingRouteForBuild(destination);
    const setupNavigation = isSetupOnboardingRoute(onboardingRoute) ? setupOnboardingNavigationMarkup() : "";
    return onboardingShellMarkup(setupNavigation);
  },
  /** Drive TASK 0311 through the same handlers the two setup buttons use. */
  chooseNoPublicName(): Promise<void> {
    return chooseNoPublicName();
  },
  continueFromPrivateContactLink(): boolean {
    return continueFromPrivateContactLink();
  },
  finishOnboarding(): Promise<void> {
    return completeSixStepOnboarding();
  },
  identityDiscoverySnapshot(): {
    choice: IdentityDiscoveryChoice | null;
    linkCreated: boolean;
    route: OnboardingRoute;
  } {
    return {
      choice: identityDiscoveryChoice,
      linkCreated: privateContactLink !== null,
      route: onboardingRoute,
    };
  },
  /**
   * Supply the account-recovery back end. The shipping build has none (no Tauri
   * command turns a password recovery phrase into a recovery token), so tests
   * inject one to exercise the flow the two forms are now bound to.
   */
  setAccountRecoveryDependencies(
    recovery: AccountRecoveryDependencies,
    migration?: RecoveryMigrationDependencies,
  ): void {
    accountRecoveryDependencies = recovery;
    if (migration) recoveryMigrationDependencies = migration;
  },
  accountRecoverySnapshot(): { step: AccountRecoveryFlow["step"]; error: string | null; migration: LegacyRecoveryMigration["kind"] | null } {
    return {
      step: accountRecoveryFlow.step,
      error: accountRecoveryFlow.error,
      migration: legacyRecoveryMigration?.kind ?? null,
    };
  },
  /** Seed detected browser areas before rendering the consent route in UI tests. */
  setBrowserProfilesForTest(profiles: BrowserProfileDescriptor[]): void {
    setBrowserProfiles(profiles);
  },
  setSelectedBrowserProfilesForTest(keys: string[]): void {
    selectedBrowserProfileKeys.clear();
    for (const key of keys) {
      if (browserProfiles.some((profile) => browserProfileKey(profile) === key)) {
        selectedBrowserProfileKeys.add(key);
      }
    }
  },
  setBrowserFootprintForTest(hydration: BrowserFootprintHydration): void {
    browserFootprintOwner = core.readiness.activeOslUserId;
    applyNativeBrowserFootprint(hydration);
  },
  setDetectedAccountsForTest(accounts: DetectedAccount[]): void {
    detectedAccounts = accounts;
    persistDetectedAccountOpeningChoices();
  },
  chooseDetectedAccountOpeningForTest(account: DetectedAccount, choice: "windowsApp" | "browser"): void {
    if (!account.openChoices.some((candidate) => candidate.kind === choice)) return;
    detectedAccountOpeningChoices = chooseDetectedAccountOpening(detectedAccountOpeningChoices, account, choice);
    persistDetectedAccountOpeningChoices();
  },
  renderDetectedAppsForTest(): string {
    route = "onboarding";
    onboardingRoute = "detected";
    return detectedAppsContent();
  },
  finishSetupChoicesForTest(): void {
    persistCombinedHomeChoices();
  },
  savedAccountModeForTest(): SavedAccountMode {
    return savedAccountMode;
  },
  setDeleteChoicesForTest(choices: DeleteChoices | null): void {
    deleteChoices = choices;
  },
  seedBrowserFootprintsForTest(receipts: NativeBrowserImportReceipt[]): void {
    applyNativeBrowserFootprint({
      imports: receipts,
      observations: receipts.map((receipt, index) => ({
        browserId: receipt.browserId,
        browserProfileAccount: receipt.account,
        browserProfileId: receipt.profile,
        importRunId: receipt.runId,
        observedAtUnixMs: index + 1,
      })),
    });
  },
  browserAccountFinderSnapshotForTest(): ReturnType<typeof browserAccountFinderSnapshotForTest> {
    return browserAccountFinderSnapshotForTest();
  },
  async callBrowserAccountFinderControlForTest(
    control: "areaTick" | "checkSelected" | "deleteArea" | "notNow" | "back",
    options: { areaKey?: string; checked?: boolean } = {},
  ): Promise<ReturnType<typeof browserAccountFinderSnapshotForTest> & { accepted: boolean }> {
    route = "onboarding";
    onboardingRoute = "browser";
    let accepted = false;
    if (control === "areaTick") {
      accepted = tickBrowserAccountFinderArea(options.areaKey ?? "", options.checked ?? true);
    } else if (control === "checkSelected") {
      accepted = await checkSelectedBrowserAccountFinderAreas();
    } else if (control === "deleteArea") {
      accepted = await deleteBrowserAccountFinderArea(options.areaKey ?? "");
    } else if (control === "notNow") {
      accepted = await leaveBrowserAccountFinderForApps();
    } else {
      onboardingRoute = previousSetupRoute(onboardingRoute);
      render();
      accepted = true;
    }
    return { accepted, ...browserAccountFinderSnapshotForTest() };
  },
  /** D80: binds the real unlock form handler against a caller-supplied DOM so
   * the credential path can be driven end to end rather than string-matched. */
  bindUnlockForm(): void {
    bindPasswordForm();
  },
  /**
   * The real "Choose apps" panel, which `tutorialContent()` returns verbatim once
   * the tour runs out of steps. Exposed for D-190: the refusal and its escape have
   * to be observable on the panel, not merely present in the source.
   */
  renderChooseAppsForTest(): string {
    route = "onboarding";
    onboardingRoute = "tutorial";
    return chooseAppsOnboardingContent();
  },
  renderOnboardingSendModes(sendMode: SendMode = "manual"): string {
    route = "onboarding";
    onboardingRoute = "sending";
    setup = { ...defaultSetup, sendMode };
    return sendingSetupContent();
  },
  renderSilentVisible(): string {
    route = "onboarding";
    onboardingRoute = "silent-visible";
    return silentVisibleSetupContent();
  },
  confirmMullvadFoundSession(): boolean {
    return confirmMullvadFoundSession();
  },
  openMullvadInstallPage(): Promise<void> {
    return openMullvadInstallPage();
  },
  continueMullvadSetup(): boolean {
    return continueMullvadSetup();
  },
  skipMullvadSetup(): void {
    skipMullvadSetup();
  },
  backFromMullvadSetup(): void {
    onboardingRoute = "cover";
    render();
  },
  callWelcomeActionForTest(actionRoute: string): { accepted: boolean; route: Route; onboardingRoute: OnboardingRoute; markup: string } {
    route = "onboarding";
    onboardingRoute = "welcome";
    const accepted = handleOnboardingRouteAction(actionRoute);
    return { accepted, route, onboardingRoute, markup: onboardingContent() };
  },
  persistOslChatNotifications(): void {
    persistOslChatNotifications();
  },
  changeAppNotificationTick(appId: ServiceId, enabled: boolean): void {
    setNotificationAppPreference(appId, enabled);
  },
  sendTestAppActivity(notification: AppNotification): void {
    recordAppNotification(notification);
  },
  localNoticeCount(appId?: ServiceId): number {
    const notices = visibleAppNotifications();
    return appId ? notices.filter((item) => item.appId === appId).length : notices.length;
  },
  handleUnhandledRejection(event: PromiseRejectionEvent): void {
    handleUnhandledRejection(event);
  },
  longRunningButtonAudit(): BusyButtonAuditRow[] {
    return longRunningButtonAuditForTest();
  },
  escapeAuditComposerScreens(): readonly string[] {
    return ["osl-chat", "osl-mail-compose"];
  },
  escapeAuditDialogs(): readonly string[] {
    return [
      "friends-dialog",
      "osl-chat-settings-dialog",
      "whitelist-roster-dialog",
      "native-protect-friend-dialog",
      "scrub-review-dialog",
      "burn-dialog",
      "owned-confirmation-dialog",
      "update-dialog",
    ];
  },
  escapeAuditTypeHalfMessage(screen: "osl-chat" | "osl-mail-compose", halfMessage: string): void {
    if (screen === "osl-chat") {
      const person = testHubPerson({
        personId: "escape-audit-peer",
        alias: "Escape Audit Peer",
        oslUserId: "escape-audit-osl-user",
        safetyNumber: "1111 2222",
        safetyNumberVerified: true,
      });
      route = "osl-chat";
      hubPeople = [person];
      activeOslChatPersonId = person.personId;
      activeOslChatContext = {
        contextToken: "escape-audit-chat-context",
        serviceId: "osl-chat",
        accountId: "local",
        personId: person.personId,
        peerOslUserId: person.oslUserId,
        scopeApproved: true,
      };
      oslChatMessages.clear();
      setOslChatDraft(halfMessage);
      return;
    }
    route = "osl-mail";
    oslMailPane = "compose";
    oslMailLoading = false;
    oslMailStatus = {
      available: true,
      provisioned: true,
      address: "escape-audit@oslprivacy.com",
      unreadCount: 0,
      retentionSeconds: 86_400,
    };
    oslMailComposeDraft = {
      to: "reader@oslprivacy.com",
      subject: "Escape audit",
      body: halfMessage,
    };
  },
  escapeAuditDraft(screen: "osl-chat" | "osl-mail-compose"): string {
    return screen === "osl-chat" ? oslChatDraft : oslMailComposeDraft.body;
  },
  escapeAuditOpenDialog(dialog: string): void {
    if (!hubPeople.length) {
      hubPeople = [testHubPerson({
        personId: "escape-audit-peer",
        alias: "Escape Audit Peer",
        oslUserId: "escape-audit-osl-user",
        safetyNumber: "1111 2222",
        safetyNumberVerified: true,
      })];
    }
    const person = hubPeople[0];
    if (dialog === "friends-dialog") {
      friendsDialogOpen = true;
      friendsDialogPage = 0;
    } else if (dialog === "osl-chat-settings-dialog") {
      route = "osl-chat";
      oslChatSettingsPersonId = person.personId;
    } else if (dialog === "whitelist-roster-dialog") {
      whitelistRosterOpen = true;
    } else if (dialog === "native-protect-friend-dialog") {
      activeNativeHostId = "discord";
      activeNativeHostMode = "dedicated";
      nativeProtectPickerOpen = true;
    } else if (dialog === "scrub-review-dialog") {
      scrubReviewOpen = true;
      scrubReviewPage = 0;
    } else if (dialog === "burn-dialog") {
      burnDialogOpen = true;
      burnScope = "chat";
      burnResult = null;
    } else if (dialog === "owned-confirmation-dialog") {
      ownedConfirmation = { kind: "verifyFriend", personId: person.personId };
      ownedConfirmationBusy = false;
      ownedConfirmationError = "";
    } else if (dialog === "update-dialog") {
      updateStatus = { state: "available", current: "0.0.0", next: "0.0.1", notes: "Escape audit" };
    } else {
      throw new Error(`unknown Escape audit dialog: ${dialog}`);
    }
  },
  escapeAuditPressEscape(): string | null {
    return closeTopmostClosableLayerForEscape();
  },
  escapeAuditState(): {
    route: Route;
    oslChatDraft: string;
    oslMailBody: string;
    openLayers: string[];
    sendAttempts: number;
  } {
    const openLayers = [
      friendsDialogOpen ? "friends-dialog" : "",
      oslChatSettingsPersonId ? "osl-chat-settings-dialog" : "",
      whitelistRosterOpen ? "whitelist-roster-dialog" : "",
      nativeProtectPickerOpen ? "native-protect-friend-dialog" : "",
      scrubReviewOpen ? "scrub-review-dialog" : "",
      burnDialogOpen ? "burn-dialog" : "",
      ownedConfirmation ? "owned-confirmation-dialog" : "",
    ].filter(Boolean);
    return {
      route,
      oslChatDraft,
      oslMailBody: oslMailComposeDraft.body,
      openLayers,
      sendAttempts: escapeAuditSendAttempts,
    };
  },
  /** Run one OSL Chat delivery tick, exactly as the cadence would. */
  deliverOslChats(): Promise<void> {
    return oslChatDelivery.sync();
  },
  async auditOslChatEnterNeverAccidentalSendForTest(): Promise<OslChatEnterAuditReport> {
    const markerPrefix = "OSL3550-ENTER-MARK";
    const states: OslChatEnterAuditState[] = [];
    const record = (
      name: string,
      category: OslChatEnterAuditState["category"],
      mark: string,
      found: boolean,
      deliveries: number,
      markDisposition: OslChatEnterAuditState["markDisposition"],
    ): void => {
      states.push({
        name,
        category,
        mark,
        found,
        deliveries,
        markDisposition,
      });
    };

    seedOslChatEnterAuditState({});
    record(
      "screen-empty-thread",
      "screen",
      `${markerPrefix}-screen-empty-thread`,
      oslChatContent().includes('class="osl-chat-thread is-empty"'),
      0,
      "refused",
    );

    const verifiedFriend = {
      personId: "task-3550-peer",
      alias: "Task 3550 Peer",
      safetyNumberVerified: true,
      pendingKeyChange: false,
    };
    const unverifiedFriend = { ...verifiedFriend, safetyNumberVerified: false };

    seedOslChatEnterAuditState({
      friend: unverifiedFriend,
      active: true,
      draft: `${markerPrefix}-screen-unverified-friend`,
    });
    record(
      "screen-unverified-friend",
      "screen",
      `${markerPrefix}-screen-unverified-friend`,
      oslChatContent().includes("Unverified"),
      await auditOslChatSubmitEnter(),
      oslChatDraft.includes(`${markerPrefix}-screen-unverified-friend`) ? "retained" : "refused",
    );

    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      draft: `${markerPrefix}-screen-not-ready`,
    });
    record(
      "screen-not-ready",
      "screen",
      `${markerPrefix}-screen-not-ready`,
      oslChatContent().includes("Chat is not ready."),
      await auditOslChatSubmitEnter(),
      oslChatDraft.includes(`${markerPrefix}-screen-not-ready`) ? "retained" : "refused",
    );

    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      approved: true,
      draft: `${markerPrefix}-dialog-chat-settings`,
      settingsOpen: true,
    });
    record(
      "dialog-chat-settings",
      "dialog",
      `${markerPrefix}-dialog-chat-settings`,
      oslChatContent().includes('id="osl-chat-settings-dialog"'),
      0,
      oslChatDraft.includes(`${markerPrefix}-dialog-chat-settings`) ? "retained" : "refused",
    );

    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      approved: true,
      draft: `${markerPrefix}-textbox-composer\n`,
    });
    record(
      "textbox-composer",
      "text-box",
      `${markerPrefix}-textbox-composer`,
      oslChatContent().includes('id="osl-chat-draft"'),
      0,
      oslChatDraft.includes(`${markerPrefix}-textbox-composer`) ? "retained" : "refused",
    );

    seedOslChatEnterAuditState({ friend: verifiedFriend, active: true, approved: true, draft: "" });
    record(
      "disabled-send-empty-draft",
      "disabled-send",
      `${markerPrefix}-disabled-send-empty-draft`,
      auditSendButtonDisabled(oslChatContent()),
      await auditOslChatSubmitEnter(),
      "refused",
    );

    const oversizeMark = `${markerPrefix}-disabled-send-oversize-draft`;
    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      approved: true,
      draft: `${oversizeMark}${"x".repeat(OSL_CHAT_MAX_DRAFT_BYTES)}`,
    });
    record(
      "disabled-send-oversize-draft",
      "disabled-send",
      oversizeMark,
      auditSendButtonDisabled(oslChatContent()),
      await auditOslChatSubmitEnter(),
      oslChatDraft.includes(oversizeMark) ? "retained" : "refused",
    );

    const busyMark = `${markerPrefix}-disabled-send-busy`;
    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      approved: true,
      busy: true,
      draft: busyMark,
    });
    record(
      "disabled-send-busy",
      "disabled-send",
      busyMark,
      auditSendButtonDisabled(oslChatContent()),
      await auditOslChatSubmitEnter(),
      oslChatDraft.includes(busyMark) ? "retained" : "refused",
    );

    const deliberateMark = `${markerPrefix}-deliberate-enabled-send`;
    seedOslChatEnterAuditState({
      friend: verifiedFriend,
      active: true,
      approved: true,
      draft: deliberateMark,
    });
    record(
      "deliberate-enabled-send",
      "enabled-send",
      deliberateMark,
      !auditSendButtonDisabled(oslChatContent()),
      await auditOslChatSubmitEnter(),
      oslChatDraft.length === 0 ? "cleared-by-send" : "retained",
    );

    return {
      markerPrefix,
      statesFound: states.filter((state) => state.found).length,
      statesTried: states.length,
      deliberateDeliveries: states
        .filter((state) => state.name === "deliberate-enabled-send")
        .reduce((sum, state) => sum + state.deliveries, 0),
      accidentalDeliveries: states
        .filter((state) => state.name !== "deliberate-enabled-send")
        .reduce((sum, state) => sum + state.deliveries, 0),
      states,
    };
  },
  /** The rendered timeline for one conversation. */
  oslChatConversation(personId: string): OslChatMessage[] {
    pruneExpiredOslChatLocalCopies();
    return [...(oslChatMessages.get(personId) ?? [])];
  },
  expireOslChatLocalCopiesForTest(nowSeconds: number): void {
    pruneExpiredOslChatLocalCopies(nowSeconds);
  },
  oslChatUnreadCount(personId: string): number {
    return oslChatUnread.get(personId) ?? 0;
  },
  openOslChatConversation(personId: string): Promise<void> {
    return openOslChat(personId);
  },
  setChatApprovalSuggestionChoice(enabled: boolean): Promise<void> {
    return setNotificationScopeSuggestions(enabled);
  },
  /** Stand in for a live Discord-overlay / native-host protected context. */
  setForeignProtectedContextForTest(token: string | null): void {
    activeContextToken = token;
  },
  /**
   * Render ONE whitelist-roster person row, without opening the dialog.
   *
   * The roster's +/- pair is an ACL surface: which control is OFFERED decides
   * whether a scope can be approved or revoked from here. That property has to
   * be checked by EXECUTING the row, because pinning its spelling is exactly
   * how the claim was lost once already -- `10bb61381 t7-25 replace native
   * title tooltips` moved `title="..."` into `inDomTooltipMarkup(...)`, and the
   * five source-text assertions that carried the disabled rules were deleted
   * rather than re-anchored (D-272). A rendered row cannot be moved by a
   * cosmetic tooltip change.
   *
   * Pure in its arguments and reads no module state, so it needs no `reset()`.
   */
  renderWhitelistRosterPerson(
    person: Partial<HubPerson> & { personId: string },
    options: { active?: boolean; busy?: boolean; activeScopeApproved?: boolean } = {},
  ): string {
    const full = testHubPerson(person);
    return whitelistRosterPersonMarkup(
      full,
      options.active === false ? null : full.personId,
      options.busy ?? false,
      options.activeScopeApproved ?? false,
    );
  },
  snapshot(): {
    route: Route;
    onboardingRoute: OnboardingRoute;
    onboardingTourCardNumber: number | null;
    completedTourCardCount: number;
    quickTourScreen: QuickTourScreen;
    settingsSection: SettingsSection;
    homePrimaryIssue: HomePrimaryIssue;
    privacyProtectionReviewOpen: boolean;
    activityAttentionReviewOpen: boolean;
    peoplePrimaryActionFocus: PeoplePrimaryActionFocus | null;
    protectionPreset: ProtectionPreset;
    inboxFilter: InboxFilter;
    oslMailNotifications: boolean;
    mullvadSetupNotice: string;
    mullvadSetupRoute: MullvadSetupRoute | null;
    mullvadAvailability: MullvadStatus["availability"];
    ownedConfirmationKind: OwnedConfirmation["kind"] | null;
    ownedConfirmationPersonId: string | null;
    silentVisibleMode: SilentVisibleMode | null;
    coverInsertion: CoverInsertionChoice | null;
    forwardSecrecyChoice: ForwardSecrecyChoice | null;
    forwardSecrecyMode: "protectPast" | "keepGroupDelivery";
    setup: SetupState;
    windowCaptureEnabled: boolean;
    torChoice: TorOnboardingState["choice"];
  } {
    return {
      route,
      onboardingRoute,
      onboardingTourCardNumber: quickTourCardNumber(),
      completedTourCardCount: completedQuickTourCardCount(),
      quickTourScreen: quickTourScreen(),
      settingsSection,
      homePrimaryIssue: homePrimaryRecommendation().issue,
      privacyProtectionReviewOpen,
      activityAttentionReviewOpen,
      peoplePrimaryActionFocus,
      protectionPreset,
      inboxFilter,
      oslMailNotifications,
      mullvadSetupNotice,
      mullvadSetupRoute,
      mullvadAvailability: mullvadStatus.availability,
      ownedConfirmationKind: ownedConfirmation?.kind ?? null,
      ownedConfirmationPersonId: ownedConfirmation?.kind === "verifyFriend" || ownedConfirmation?.kind === "removeFriend"
        ? ownedConfirmation.personId
        : null,
      silentVisibleMode,
      coverInsertion,
      forwardSecrecyChoice: forwardSecrecyOnboarding.choice,
      forwardSecrecyMode,
      setup: { ...setup },
      windowCaptureEnabled,
      torChoice: torOnboarding.choice,
    };
  },
};

const skipAutoBootstrap = Boolean(
  (globalThis as { __OSL_HUB_SKIP_AUTO_BOOTSTRAP?: unknown }).__OSL_HUB_SKIP_AUTO_BOOTSTRAP,
);

if (!runningUnderVitest && !skipAutoBootstrap) {
  if (fixedNoRecoverySecretFixture) {
    applyOslHubUiTestState({
      route: "onboarding",
      onboardingRoute: "recovery",
      recoveryBundle: null,
      recoveryKitUnsaved: false,
    });
    render();
  } else {
    const desktopWindow = getCurrentWindow();
    bindWindowLifecycleRealignment(
      window,
      desktopWindow,
      document,
      scheduleNativeHostRealignment,
    );
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
    window.addEventListener(unhandledRejectionEventType, handleUnhandledRejection);
    void bootstrap();
    scheduleOslChatBackgroundSync(1_000);
  }
}

async function pasteOslChatClipboardImage(event: ClipboardEvent): Promise<void> {
  if (!activeOslChatContext?.scopeApproved || oslChatBusy) return;
  const items = event.clipboardData?.items;
  if (!items) return;
  const imageItem = [...items].find((item) => item.kind === "file" && item.type.startsWith("image/"));
  if (!imageItem) return;
  const file = imageItem.getAsFile();
  if (!file) return;
  event.preventDefault();
  const card = await acceptOslChatClipboardImageAttachment(file.type, new Uint8Array(await file.arrayBuffer()));
  if (!card) { showToast("Pasted image could not be attached"); return; }
  attachmentProgressByContext.set(card.contextId, card);
  render();
}
