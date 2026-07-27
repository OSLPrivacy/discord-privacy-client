import "@fontsource-variable/inter/wght.css";
import "./overlay.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { checkedBackendResponse, lastBackendFailure, recordBackendFailure, recordInvalidBackendResponse } from "./backend-failure";
import { burnNativeDiscordOverlayChat, getNativeDiscordOverlayQaDiagnostic, getNativeDiscordOverlayState, listNativeDiscordOverlayAttachments, openNativeDiscordOverlayAttachment, openNativeDiscordOverlayText, prepareNativeDiscordOverlayText, revealNativeDiscordOverlayViewOnce, selectNativeDiscordOverlayAttachment, sendNativeDiscordOverlayCarrier, sendNativeDiscordQaAtomicText, sendNativeDiscordQaProbe, setNativeDiscordOverlaySecurity, type NativeDiscordCarrierLayout, type NativeDiscordCarrierMode } from "./native-overlay-adapter";
import { boundedProtectedDraft, MAX_PROTECTED_DRAFT_BYTES, NATIVE_OVERLAY_TTL_OPTIONS, overlayExpiryDelayMs, PROTECTED_DRAFT_WARNING_BYTES, type NativeOverlayTtlSeconds, type NativeSurfaceCapture, utf8Length } from "./overlay-state";
import { OverlaySendGesture, type OverlaySendMode } from "./overlay-send-gesture";
import { CoarseTypingRate } from "./coarse-typing-rate";
import { TwoStepBurnConfirmation } from "./two-step-burn";
import { shouldPollDiscordOverlay } from "./discord-qa-receive-policy";
import { recordDiscordQaSendStage } from "./discord-qa-send-stage";
import {
  createDiscordProtectedTranscript,
  type DiscordProtectedTranscriptRow,
  type VerifiedOslTranscriptIdentity,
} from "./discord-protected-transcript";
import {
  defaultDiscordVisualRecipe,
  discordTranscriptTheme,
  discordVisualCssVariables,
  parseDiscordVisualRecipe,
  type DiscordVisualRecipe,
} from "./discord-visual-recipe";
import {
  applyCarrierRowGeometry,
  clearCarrierRowGeometry,
  type NativeDiscordCarrierRowBinding,
} from "./discord-carrier-row-binding";

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error("Trusted composer overlay is incomplete");
  return element;
}

const draft = requireElement<HTMLTextAreaElement>("#protected-draft");
const counter = requireElement<HTMLElement>("#draft-bytes");
const friendLabel = requireElement<HTMLElement>("#friend-label");
const ttl = requireElement<HTMLSelectElement>("#protected-ttl");
const viewOnce = requireElement<HTMLInputElement>("#protected-view-once");
const sendMode = requireElement<HTMLSelectElement>("#protected-send-mode");
const placementMode = requireElement<HTMLSelectElement>("#protected-placement-mode");
const decryptDisplay = requireElement<HTMLInputElement>("#protected-decrypt-display");
const currentExpiry = requireElement<HTMLElement>("#current-expiry");
const prepare = requireElement<HTMLButtonElement>("#prepare-protected");
const chooseAttachment = requireElement<HTMLButtonElement>("#choose-attachment");
const coverText = requireElement<HTMLButtonElement>("#covertext-mode");
const burnChat = requireElement<HTMLButtonElement>("#burn-protected-chat");
const status = requireElement<HTMLOutputElement>("#overlay-status");
// Persistent, unlike `status` above: #overlay-status is a shared transient
// line with 30+ writers (typing feedback, periodic "Verifying protected
// Discord…" polls, etc.), so a failed-send notice written only there can be
// overwritten before the user ever reads it. This element is written to by
// exactly two things -- a failed send, and (only once success is confirmed
// and no failure applies) the softer "not yet acknowledged" caution -- and is
// cleared or rewritten only at the two sites marked below in sendDraft(),
// never by a timer and never by keystrokes. The caution never overwrites a
// failure notice: the failure branch returns before the caution is ever
// evaluated.
const sendWarning = requireElement<HTMLOutputElement>("#overlay-send-warning");
// The seen half of that same signal, and the reason this exists at all: the
// persistent notice above is announced but never displayed. It sits in
// `.composer-toolbar`, and this window is sized natively to Discord's measured
// composer rectangle -- 736x58 logical pixels, live -- where the toolbar is
// flex-shrunk to nothing and, under `data-native-composer-capture`, set to
// `display: none` outright. A send that never reached Discord therefore looked
// exactly like one that did.
//
// This banner is a child of `.composer-box`, the one rectangle OSL paints, so
// it needs no window height that does not exist and adds no opaque pixel over
// the operator's real Discord; `.composer-box::after` still draws the cyan lock
// ring above it. It is deliberately NOT a second live region -- the
// announcement stays on the `role="alert"` element above, so the failure is
// spoken exactly once -- and, exactly like that element, it carries fixed
// literals only and is written by nothing but the marked sites in sendDraft()
// and the operator's own dismissal below.
const failureBanner = requireElement<HTMLElement>("#overlay-send-failure");
const failureBannerText = requireElement<HTMLElement>("#overlay-send-failure-text");
const failureBannerDismiss = requireElement<HTMLButtonElement>("#overlay-send-failure-dismiss");
const messageList = requireElement<HTMLElement>("#osl-message-list");
const discordQaShell = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1";
const PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT = "osl://protected-display-visibility-changed";
const NATIVE_SURFACE_CHANGED_EVENT = "osl://native-surface-changed";
const OVERLAY_REFOCUS_EVENT = "osl://native-discord-overlay-refocus";
// Discord's transcript band moved, resized or came to rest, so every row
// rectangle this renderer holds is now pointing at the wrong pixels. No payload:
// it is a bare "re-read", raised by the native guard loop on the tick something
// actually changed. This is the one scroll/geometry edge the renderer cannot see
// for itself -- OSL owns pixels only where OSL draws, and Discord is a different
// process -- so it is handled here rather than looked for.
const NATIVE_DISCORD_ROWS_MOVED_EVENT = "osl://native-discord-rows-moved";
// `true` means the window this renderer draws into now covers Discord's transcript
// rows ONLY, and has vacated the composer band entirely -- Discord's real message
// box is uncovered and taking keystrokes, so OSL must stop drawing a composer into
// a window that no longer sits over one. A bare boolean about geometry, on the
// edges only, re-sent on every reveal of the retained WebView.
//
// This is a native fact and not something this renderer can derive. The surface it
// lays out in is a window whose size and origin it is never told, and re-deriving
// the state from the lock is the previously-rejected failure (see the rule in
// overlay.css). The writer that actually vacated the band is the only thing that
// can say so.
const NATIVE_DISCORD_COMPOSER_BAND_SURRENDERED_EVENT = "osl://native-discord-composer-band-surrendered";
// This WebView is built once and retained across lock toggles so the protected
// composer can appear immediately. `false` means the protected session ended
// and every byte of it must be discarded before the retained window can ever
// be reused; `true` means a freshly verified session is ready to read.
const OVERLAY_SESSION_EVENT = "osl://native-discord-overlay-session";
document.documentElement.dataset.discordQaShell = String(discordQaShell);

let composing = false;
let overlayReady = false;
let overlayInitRetryMs = 250;
let overlayInitTimer: number | undefined;
let decryptDisplayEnabled = false;
let viewOnceEnabled = false;
let attachmentsEnabled = false;
let discordMarkerAvailable = false;
let coverTextEnabled = true;
let confirmedTtlSeconds: NativeOverlayTtlSeconds = NATIVE_OVERLAY_TTL_OPTIONS[0];
let securityBusy = false;
let receiveBusy = false;
let sendBusy = false;
let attachmentBusy = false;
let draftTooLarge = false;
// Ledger for the honest send-side caution: OSL never learns whether the peer
// has an OSL identity (no handle -> identity index exists, and the control
// inbox gives senders no delivery signal by design), but it DOES own the
// acknowledgement record for messages sent through this very overlay. These
// two flags mirror that record in aggregate -- "sent at least one" and "at
// least one ever advanced past Sent" -- using only the fixed Received/Opened
// enum already carried by acknowledgments, never draft or transcript text.
// Reset only where the rest of the conversation's state is discarded, in
// discardProtectedSession() below.
let anyProtectedMessageSent = false;
let anyProtectedMessageAcknowledged = false;
let receiveTimer: number | undefined;
let gestureTimer: number | undefined;
let burnTimer: number | undefined;
let idlePollMs = 2_000;
const sendGesture = new OverlaySendGesture();
if (discordQaShell) {
  sendMode.value = "single";
  sendGesture.setMode("single");
}
const typingRate = new CoarseTypingRate();
const burnConfirmation = new TwoStepBurnConfirmation();
/**
 * The decrypted text of each row OSL can decrypt. In memory for the session,
 * nothing more, and only ever read to paint while the eye is on.
 *
 * There is deliberately no companion map of cover prose any more. With the eye
 * off OSL paints nothing at all, so Discord's own row is what the operator sees
 * and it already says exactly what Discord has: there is nothing for OSL to
 * reproduce, and nothing it could get wrong.
 */
const messagePlaintext = new Map<string, string>();
const messageExpiryTimers = new Map<HTMLLIElement, number>();
const viewOnceBubbles = new Set<HTMLLIElement>();
const receivedPlaintextBubbles = new Set<HTMLLIElement>();
const outgoingBubbles = new Map<string, HTMLLIElement>();
/**
 * Received messages by their backend correlation handle.
 *
 * The receive batch used to carry no handle at all, so an inbound bubble was
 * anonymous: it could not be matched to the Discord row it belongs over, and a
 * message the backend surfaced twice appended twice. This is the renderer's half
 * of that handle -- in-memory only, cleared with the bubble it names, and never
 * written anywhere.
 */
const incomingBubbles = new Map<string, HTMLLIElement>();
let verifiedCarrierRows: readonly NativeDiscordCarrierRowBinding[] = [];
/**
 * Every Discord row OSL can currently decrypt AND place, whoever sent it.
 *
 * This is what the eye is keyed to. It used to be keyed to `outgoingBubbles` --
 * the messages this client happened to send during this session -- which is why
 * the eye could never work on history: a row the operator received last week is
 * decryptable and was simply never a member of that map.
 *
 * `decodedRows` is the transcript layer for those rows and `decodedRowBindings`
 * is where each of them is inside OSL's own window. Both are rebuilt wholesale
 * from one backend read; nothing here accumulates across reads, so a row that
 * scrolled away or stopped decoding disappears rather than lingering over
 * whatever Discord has put in its place.
 */
const decodedRows: DiscordProtectedTranscriptRow[] = [];
const decodedRowBindings = new Map<string, NativeDiscordCarrierRowBinding>();
const pendingAttachmentIds = new Set<string>();
const pendingViewOnceIds = new Set<string>();
const transcriptRows: DiscordProtectedTranscriptRow[] = [];
const transcriptActions = new Map<string, () => void>();
let transcriptSequence = 0;
let verifiedFriendIdentity: VerifiedOslTranscriptIdentity = {
  id: "verified-friend",
  displayName: "Private message",
  avatarFallback: "?",
  provenance: "verified-osl",
};
const localIdentity: VerifiedOslTranscriptIdentity = {
  id: "local-user",
  displayName: "You",
  avatarFallback: "Y",
  provenance: "verified-osl",
};
let activeVisualRecipe: DiscordVisualRecipe | null = null;
// The last native composer measurement this renderer was given, kept because a
// decoded Discord row has to be painted in Discord's own family, size and
// weight and the transcript read itself measures none of those. Content-free:
// an image data URL, four rectangles, a colour and four font facts.
let activeNativeSurface: NativeSurfaceCapture | undefined;
let lockEngaged = true;

/**
 * The lock is encryption only, and it governs exactly one thing on screen: who
 * owns the message box. Lock on, OSL's composer is over Discord's, because the
 * operator's plaintext may never enter Discord's real box. Lock off, OSL has no
 * composer at all and they type into Discord normally.
 *
 * It deliberately does not touch the transcript. What is DISPLAYED is the eye's
 * business and nothing else's.
 */
function applyLockEngaged(engaged: boolean): void {
  lockEngaged = engaged;
  document.documentElement.dataset.oslLockEngaged = String(engaged);
  // Lock down: OSL owns no message box, the operator is typing into Discord's
  // own again, and whatever caret this renderer was given belongs to an
  // engagement that is over. The next raise is a fresh one and gets its own.
  if (!engaged) caretGrantedForEngagement = false;
  else focusEngagedProtectedDraft();
}

// ENGAGE EDGE: the caret, not just the window.
//
// Raising the lock is the operator moving from typing into Discord to typing
// into OSL, and this window is placed exactly on top of Discord's own message
// box. The native guard already gives this WINDOW input focus once, on the
// first open of a session (`active_focus_overlay`, called from the `!ready`
// branch of the protected-overlay guard in
// `apps/osl-hub/src/native_discord_overlay.rs`). Nothing ever gave the trusted
// textarea DOM focus, so `document.activeElement` stayed `<body>`: the composer
// was on screen, above Discord, foreground -- and held no caret.
//
// That does not fail safe. The operator has to click before they can type, the
// two composers are the same few hundred pixels of screen, and a click that
// misses OSL by a few pixels lands in Discord's real message box, where Enter
// sends the draft in the clear. Being the foreground window and holding the
// caret are two different contracts, exactly as being above Discord and
// receiving its keyboard focus are -- and only the first of each was asserted.
//
// The guarantee this adds is deliberately narrow:
//   * DOM-only. `HTMLElement.focus()` cannot raise, move, show or foreground a
//     window, so it can never yank an operator who is mid-action in another
//     application: if this window is not foreground the caret simply waits
//     inside it until it is. The window half of the handshake stays where it
//     already lives, in the native first-open acquisition.
//   * Only where the operator asked for it, and once per raise. A verified
//     session re-announces itself on every later reveal of an already-ready
//     one (a restored composer re-emits `OVERLAY_SESSION_EVENT`), the eye and
//     every re-measure edge re-apply the same lock state, and initialisation
//     retries on a backoff besides. Focusing on any of those is focus movement
//     the operator did not ask for, and would fight whatever control in this
//     window they are actually using.
//   * Only when there is something to type into. `overlayReady` false is a
//     composer whose every control is disabled and whose Enter is refused, so
//     it may not hold the caret either.
//
// The flag therefore means "the current raise has already been given the
// caret". It is cleared at every site that proves there is no raised, readable
// lock left to hold one -- the lock coming down, the session being discarded,
// both non-ready exits of `initializeOverlay`, and a display refresh that finds
// no active session -- so a re-engage is focused again even if any single one
// of those announcements is missed.
//
// The module-level `applyLockEngaged(true)` below is deliberately harmless: it
// runs on a retained WebView with no verified session, where `overlayReady` is
// false and the grant refuses.
let caretGrantedForEngagement = false;

function focusEngagedProtectedDraft(): void {
  if (!lockEngaged || !overlayReady || caretGrantedForEngagement) return;
  caretGrantedForEngagement = true;
  draft.focus({ preventScroll: true });
  // A recovered draft is text the operator already wrote; typing continues it
  // rather than being dropped in front of it.
  const caret = draft.value.length;
  try {
    draft.setSelectionRange(caret, caret);
  } catch {
    /* A textarea always exposes a selection; never fail a raise over it. */
  }
}

applyLockEngaged(true);

function discordComposerPlaceholder(friend: string): string {
  const normalized = friend.replace(/\s+/gu, " ").trim().replace(/^@/u, "");
  return normalized ? `Message @${normalized}` : "Message";
}

/**
 * Surface fills the visual recipe is not allowed to guess at.
 *
 * `discordVisualCssVariables()` derives these two from a four-entry theme-pack
 * table (dark/light/midnight/ash), so every custom Discord theme in existence
 * collapses onto whichever of the four it is nearest -- and "nearest" is not
 * "the same". On an operator running a near-black theme, `--osl-composer-bg`
 * arrived as Discord's default `#383a40` and `.composer-box` painted it as a
 * visibly lighter slab inside the real message box: a highlight rectangle
 * behind the placeholder text.
 *
 * A fill that is nearly right is worse here than no fill at all, because the
 * protected window sits directly over Discord's own composer: painting nothing
 * shows Discord's real colour through, exactly, for free. The composer's fill
 * therefore comes only from `--osl-native-edit-background`, the colour the
 * backend sampled out of this operator's own Discord, and `--osl-composer-fill`
 * in overlay.css falls back to `transparent` rather than to any constant.
 *
 * Only the guessed *fills* are skipped. Everything else in the recipe -- the
 * measured line height, composer width, zoom and DPI scale, and the transcript
 * tokens, which are consumed only by rows whose colours the carrier binding
 * overwrites with measured ones -- is applied unchanged.
 */
const GUESSED_SURFACE_FILL_VARIABLES: ReadonlySet<string> = new Set([
  "--osl-overlay-bg",
  "--osl-composer-bg",
]);

function installDiscordVisualRecipe(value: unknown): DiscordVisualRecipe {
  activeVisualRecipe = parseDiscordVisualRecipe(value);
  const recipe = activeVisualRecipe ?? defaultDiscordVisualRecipe();
  for (const [name, setting] of Object.entries(discordVisualCssVariables(recipe))) {
    if (GUESSED_SURFACE_FILL_VARIABLES.has(name)) continue;
    document.documentElement.style.setProperty(name, setting);
  }
  document.documentElement.dataset.discordTheme = recipe.theme;
  document.documentElement.dataset.discordDensity = recipe.density;
  document.documentElement.dataset.discordHighContrast = String(recipe.highContrast);
  document.documentElement.dataset.discordReducedMotion = String(recipe.reducedMotion);
  document.documentElement.dataset.discordVisualRecipe = activeVisualRecipe ? "verified" : "default";
  return recipe;
}

const initialVisualRecipe = installDiscordVisualRecipe(undefined);

const transcript = createDiscordProtectedTranscript({
  document,
  ariaLabel: "Messages prepared or opened in this OSL panel",
  preferences: {
    theme: discordTranscriptTheme(initialVisualRecipe.theme),
    density: initialVisualRecipe.density,
    zoom: initialVisualRecipe.zoom,
  },
  window: { rows: [], startIndex: 0, totalRowCount: 0 },
  onAction(actionId) { transcriptActions.get(actionId)?.(); },
});
for (const [name, setting] of Object.entries(discordVisualCssVariables(initialVisualRecipe))) {
  if (name.startsWith("--osl-transcript-")) transcript.root.style.setProperty(name, setting);
}
messageList.append(transcript.root);

function applyDiscordVisualRecipe(value: unknown): DiscordVisualRecipe {
  const recipe = installDiscordVisualRecipe(value);
  transcript.updatePreferences({
    theme: discordTranscriptTheme(recipe.theme),
    density: recipe.density,
    zoom: recipe.zoom,
  });
  for (const [name, setting] of Object.entries(discordVisualCssVariables(recipe))) {
    if (name.startsWith("--osl-transcript-")) transcript.root.style.setProperty(name, setting);
  }
  return recipe;
}

const nativeSurfaceCssVariables = [
  "--osl-native-composer-image",
  "--osl-native-edit-left",
  "--osl-native-edit-top",
  "--osl-native-edit-width",
  "--osl-native-edit-height",
  "--osl-native-edit-background",
  "--osl-native-edit-padding",
  "--osl-native-edit-font-family",
  "--osl-native-edit-font-size",
  "--osl-native-edit-font-weight",
  "--osl-native-edit-line-height",
  "--osl-native-composer-aspect-ratio",
] as const;

function applyNativeSurfaceCapture(capture?: NativeSurfaceCapture): void {
  activeNativeSurface = capture;
  const root = document.documentElement;
  delete root.dataset.nativeComposerCapture;
  for (const name of nativeSurfaceCssVariables) root.style.removeProperty(name);
  if (!capture) return;
  const percent = (value: number, total: number) => `${(value / total) * 100}%`;
  root.style.setProperty("--osl-native-composer-image", `url("${capture.imageDataUrl}")`);
  root.style.setProperty("--osl-native-edit-left", percent(capture.textLeftPx, capture.widthPx));
  root.style.setProperty("--osl-native-edit-top", percent(capture.textTopPx, capture.heightPx));
  root.style.setProperty("--osl-native-edit-width", percent(capture.textWidthPx, capture.widthPx));
  root.style.setProperty("--osl-native-edit-height", percent(capture.textHeightPx, capture.heightPx));
  root.style.setProperty("--osl-native-edit-background", capture.inputBackground);
  root.style.setProperty("--osl-native-edit-padding", "0");
  // Every measurement that came back is used, verbatim, on its own.
  //
  // This was one all-four-or-nothing `if`, and that was a restriction with no
  // upside. The four measurements are independent -- the backend can read
  // Discord's size and line height off the composer while the family name comes
  // back unusable, or vice versa -- and withholding a measured 15px size
  // because a *different* property was missing does not fall back to nothing:
  // it falls back to the hardcoded 14px written into overlay.css, i.e. it
  // substitutes a guess for a value OSL actually knows. That is the exact
  // failure mode this surface cannot have, since these glyphs land pixels away
  // from Discord's own on the same screen.
  //
  // Partial application is therefore strictly better than none: each property
  // that was measured is matched exactly, and only the properties that were
  // genuinely not measured take the documented per-declaration fallback in
  // overlay.css. No value is clamped, floored or rounded on the way through --
  // `capture.fontSizePx` may be fractional and is written as-is.
  //
  // Note the removal above is unconditional, so a property that stops being
  // measured is *cleared* rather than left stale from an earlier capture.
  if (capture.fontFamily !== null) {
    root.style.setProperty("--osl-native-edit-font-family", JSON.stringify(capture.fontFamily));
  }
  if (capture.fontSizePx !== null) {
    root.style.setProperty("--osl-native-edit-font-size", `${capture.fontSizePx}px`);
  }
  if (capture.fontWeight !== null) {
    root.style.setProperty("--osl-native-edit-font-weight", String(capture.fontWeight));
  }
  if (capture.lineHeightPx !== null) {
    root.style.setProperty("--osl-native-edit-line-height", `${capture.lineHeightPx}px`);
  }
  // The native capture is measured in physical screen pixels while WebView2
  // lays out CSS pixels. Preserve the physical aspect ratio so Windows DPI
  // scaling cancels out instead of making the composer row too tall.
  root.style.setProperty(
    "--osl-native-composer-aspect-ratio",
    `${capture.widthPx} / ${capture.heightPx}`,
  );
  root.dataset.nativeComposerCapture = "true";
}

/** One Discord row's rectangle inside OSL's own protected window, in CSS px. */
interface DecodedDiscordRowRect {
  leftPx: number;
  topPx: number;
  widthPx: number;
  heightPx: number;
}

/**
 * One row of the Discord transcript as the backend read it back.
 *
 * `plaintext` is `null` for every row OSL cannot open -- an ordinary message, a
 * cover whose blob is gone, a view-once message, a chunk of a multi-row message,
 * or this conversation's decrypted display being off. `row` is `null` when OSL
 * cannot presently say where the row is. Either null means OSL paints nothing
 * there and the operator sees Discord's own row, untouched.
 */
interface RehydratedDiscordRow {
  flagtext: string;
  plaintext: string | null;
  /**
   * Which end of the conversation wrote this row, as PROVEN by the backend --
   * it is the orientation whose signature verified the wire, not a guess from
   * position or from whose conversation is open.
   *
   * Non-null exactly when `plaintext` is non-null. The renderer never invents
   * it: a row with text and no proven author is a malformed response, and the
   * whole read is refused so Discord's own rows stay visible.
   */
  orientation: "incoming" | "outgoing" | null;
  row: DecodedDiscordRowRect | null;
}

interface RehydratedDiscordTranscript {
  /** False exactly when the backend's minimum-interval floor refused this read. */
  read: boolean;
  /** How much of that floor is left. Always 0 when `read` is true. */
  retryAfterMs: number;
  rows: RehydratedDiscordRow[];
}

const MAX_REHYDRATED_ROWS = 32;
// Matches REHYDRATE_MAX_ROW_TEXT_BYTES in native_discord_adapter.rs: the walk
// truncates a longer row rather than returning it, so a longer one is a
// malformed response.
const MAX_ROW_FLAGTEXT_BYTES = 2_000;
const MAX_OVERLAY_EDGE_PX = 16_384;
// The floor is the backend's; this only bounds what a response may claim, so a
// malformed retry can never park the eye for an unbounded time.
const MAX_REHYDRATE_RETRY_MS = 5_000;

function exactKeys(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && expected.every((key, index) => key === actual[index]);
}

function parseDecodedDiscordRowRect(value: unknown): DecodedDiscordRowRect | null {
  if (!exactKeys(value, ["leftPx", "topPx", "widthPx", "heightPx"])) return null;
  const { leftPx, topPx, widthPx, heightPx } = value as Record<string, unknown>;
  if (![leftPx, topPx, widthPx, heightPx].every((edge) => typeof edge === "number" && Number.isFinite(edge))) {
    return null;
  }
  const rect = value as unknown as DecodedDiscordRowRect;
  // Same bounds the just-sent carrier binding is held to, for the same reason: a
  // rectangle OSL cannot vouch for must not become decrypted text painted over
  // the wrong Discord row.
  if (rect.leftPx < 0 || rect.topPx < 0 || rect.widthPx < 1 || rect.heightPx < 12
    || rect.leftPx + rect.widthPx > MAX_OVERLAY_EDGE_PX
    || rect.topPx + rect.heightPx > MAX_OVERLAY_EDGE_PX) return null;
  return rect;
}

function parseRehydratedDiscordRow(value: unknown): RehydratedDiscordRow | null {
  if (!exactKeys(value, ["flagtext", "plaintext", "orientation", "row"])) return null;
  const record = value as Record<string, unknown>;
  if (typeof record.flagtext !== "string" || utf8Length(record.flagtext) > MAX_ROW_FLAGTEXT_BYTES) return null;
  if (record.plaintext !== null
    && (typeof record.plaintext !== "string" || utf8Length(record.plaintext) > MAX_PROTECTED_DRAFT_BYTES)) return null;
  // Authorship is accepted only as one of the two proven answers, and only
  // together with the text it describes. Text with no proven author would have
  // to be attributed by guessing, and an author with no text describes nothing
  // -- both are refused here rather than reconciled downstream.
  if (record.orientation !== null && record.orientation !== "incoming" && record.orientation !== "outgoing") return null;
  if ((record.plaintext === null) !== (record.orientation === null)) return null;
  const row = record.row === null ? null : parseDecodedDiscordRowRect(record.row);
  if (record.row !== null && row === null) return null;
  return {
    flagtext: record.flagtext,
    plaintext: record.plaintext as string | null,
    orientation: record.orientation as "incoming" | "outgoing" | null,
    row,
  };
}

function parseRehydratedDiscordTranscript(value: unknown): RehydratedDiscordTranscript | null {
  if (!exactKeys(value, ["read", "retryAfterMs", "rows"])) return null;
  const record = value as Record<string, unknown>;
  if (typeof record.read !== "boolean"
    || !Number.isInteger(record.retryAfterMs)
    || Number(record.retryAfterMs) < 0 || Number(record.retryAfterMs) > MAX_REHYDRATE_RETRY_MS
    || (record.read === true && record.retryAfterMs !== 0)
    || !Array.isArray(record.rows) || record.rows.length > MAX_REHYDRATED_ROWS) return null;
  const rows: RehydratedDiscordRow[] = [];
  for (const candidate of record.rows) {
    const row = parseRehydratedDiscordRow(candidate);
    if (!row) return null;
    rows.push(row);
  }
  return { read: record.read, retryAfterMs: Number(record.retryAfterMs), rows };
}

/**
 * Ask the backend for one bounded read of the rows Discord is displaying.
 *
 * Fail-closed and detail-free like every other adapter on this surface: a
 * rejection, a malformed response or a thrown error all answer `null`, and the
 * eye then paints exactly what it painted before rather than guessing.
 *
 * The `null` stays exactly as it was; what no longer happens is throwing the
 * reason away with it. This read is now driven by a native edge nobody clicked,
 * so a backend that starts refusing it is otherwise completely silent -- the eye
 * simply keeps painting the old rows. The refusal goes to the journal in
 * ./backend-failure.ts instead (renderer memory only, never persisted), with the
 * scope passed as a content argument purely so any fragment of it the error
 * echoed back is redacted.
 */
async function rehydrateNativeDiscordOverlayHistory(
  scope: string,
): Promise<RehydratedDiscordTranscript | null> {
  if (typeof scope !== "string" || !scope || utf8Length(scope) > 256) return null;
  try {
    return checkedBackendResponse(
      "rehydrate_native_discord_overlay_history",
      parseRehydratedDiscordTranscript(
        await invoke<unknown>("rehydrate_native_discord_overlay_history", { scope }),
      ),
    );
  } catch (error) {
    recordBackendFailure("rehydrate_native_discord_overlay_history", error, [scope]);
    return null;
  }
}

function syncTranscript(): void {
  // One list, two sources: OSL's own session rows and the decodable Discord rows
  // read back off screen. Only the ones the backend proved a rectangle for are
  // ever visible -- see paintBoundRows().
  const rows = [...transcriptRows, ...decodedRows];
  transcript.updateWindow({ rows, startIndex: 0, totalRowCount: rows.length });
  paintBoundRows();
}

/**
 * Paint per row, or not at all.
 *
 * `clearCarrierRowGeometry` hides a row, so a rendered row is only ever visible
 * once the backend has proven where the exact Discord row it belongs to is on
 * screen. Everything OSL cannot place -- ordinary chat, undecodable rows,
 * anything it cannot currently locate -- is simply not painted, and Discord's
 * own row shows through untouched because OSL owns no pixel there.
 *
 * Two sources, in this order:
 *
 * 1. Decodable Discord rows, from the bounded transcript read. This is the eye:
 *    it works on ANY row OSL can decrypt, including history this client never
 *    sent and never received through this session's inbox.
 * 2. The QA shell's just-sent carrier rows, which the backend measures per row
 *    at send time. Absent in a shipping build, where `visibleCarrierRows` is not
 *    a field at all.
 *
 * The eye, and only the eye, decides whether anything is painted. The lock is
 * deliberately absent: it is encryption only.
 */
function paintBoundRows(): void {
  for (const item of transcript.root.querySelectorAll<HTMLLIElement>("[data-row-key]")) {
    clearCarrierRowGeometry(item);
  }
  if (!decryptDisplayEnabled) return;
  for (const [key, binding] of decodedRowBindings) {
    const item = transcript.root.querySelector<HTMLLIElement>(`[data-row-key="${CSS.escape(key)}"]`);
    if (item) applyCarrierRowGeometry(item, binding);
  }
  for (const binding of verifiedCarrierRows) {
    const item = outgoingBubbles.get(binding.messageId);
    if (item) applyCarrierRowGeometry(item, binding);
  }
}

function applyVerifiedCarrierRows(
  bindings: readonly NativeDiscordCarrierRowBinding[] | undefined,
): void {
  verifiedCarrierRows = decryptDisplayEnabled ? bindings ?? [] : [];
  paintBoundRows();
}

/**
 * The typography and fill a decoded row is painted in.
 *
 * The bounded transcript read measures geometry only -- it walks MSAA, which
 * exposes no font or colour -- so the presentation comes from the two
 * measurements OSL already publishes for this exact Discord: the sampled native
 * composer (Discord's own family, size, weight, measured out of the operator's
 * running client) and the verified visual recipe (its measured line height,
 * zoom and DPI scale, and the theme pack its surfaces resolved to).
 *
 * The fill must be opaque. OSL is painting over a Discord row that still says
 * the cover sentence underneath; a translucent fill would show both at once.
 */
function decodedRowPresentation(): Omit<NativeDiscordCarrierRowBinding,
  "messageId" | "nativeLocatorSha256" | "carrierSha256" | "leftPx" | "topPx" | "widthPx" | "heightPx"> {
  const recipe = activeVisualRecipe ?? defaultDiscordVisualRecipe();
  const tokens = discordVisualCssVariables(recipe);
  const lineHeightPx = Math.min(Math.max(recipe.lineHeightPx, 10), 128);
  // Discord's own cozy ratio, used only when the native composer capture could
  // not supply a measured size. Never a substitute for a measurement OSL has.
  const fallbackFontSizePx = lineHeightPx / 1.375;
  const fontSizePx = Math.min(Math.max(activeNativeSurface?.fontSizePx ?? fallbackFontSizePx, 8), 128);
  return {
    backgroundColor: tokens["--osl-transcript-bg"] ?? "#313338",
    foregroundColor: tokens["--osl-transcript-text"] ?? "#dbdee1",
    fontFamily: activeNativeSurface?.fontFamily ?? "gg sans",
    fontSizePx,
    fontWeight: activeNativeSurface?.fontWeight ?? 400,
    lineHeightPx,
    letterSpacingPx: 0,
    zoom: Math.min(Math.max(recipe.zoom, 0.5), 4),
    density: Math.min(Math.max(recipe.dpiScale, 0.7), 3),
  };
}

/**
 * Rebuild the eye from one completed transcript read.
 *
 * Wholesale, never incremental: the rows Discord is showing, where they are, and
 * which of them OSL can open are all facts about *this instant*, and a row kept
 * from a previous read is decrypted text sitting over whatever Discord has since
 * scrolled into its place.
 *
 * PRIVACY: the decrypted text lives in this DOM and in `messagePlaintext` for
 * the lifetime of the session and nowhere else. Nothing here logs, hashes,
 * persists or transports a plaintext or a cover, and the row's own `flagtext` is
 * deliberately not rendered -- with the eye on OSL shows the message; with it
 * off OSL shows nothing and Discord's row already says the cover itself.
 */
function applyDecodedTranscript(rows: readonly RehydratedDiscordRow[]): void {
  for (const key of decodedRowBindings.keys()) messagePlaintext.delete(key);
  decodedRows.length = 0;
  decodedRowBindings.clear();
  const presentation = decodedRowPresentation();
  let index = 0;
  for (const row of rows) {
    // Undecodable, or unplaceable. Either way OSL owns no pixel over it.
    if (row.plaintext === null || row.row === null) continue;
    // And authorship must have been PROVEN. The parser already refuses a row
    // that has text without it, so this is the second half of the same
    // fail-closed rule rather than a new one: OSL leaves the carrier visible
    // rather than painting text it cannot name the author of.
    if (row.orientation === null) continue;
    // Positional keys, so a row that is still the nth decodable row keeps its
    // DOM node across reads instead of being destroyed and rebuilt on a scroll.
    const key = `decoded-${index}`;
    index += 1;
    messagePlaintext.set(key, row.plaintext);
    decodedRows.push({
      key,
      kind: "text",
      // From the backend's proof, never from the surface. Stamping every
      // opened row `incoming` showed the operator their OWN sent messages
      // attributed to their friend.
      direction: row.orientation,
      author: row.orientation === "outgoing" ? localIdentity : verifiedFriendIdentity,
      timestamp: transcriptTimestamp(),
      plaintext: row.plaintext,
      // Born in whichever mode the eye is already in, so a read that lands with
      // the eye off never flashes plaintext.
      plaintextHidden: !decryptDisplayEnabled,
    });
    decodedRowBindings.set(key, {
      ...presentation,
      // A decoded row has no OSL message id and no carrier proof -- it is a row
      // on screen that OSL was able to open, which may be years old and may have
      // been sent from another device entirely. The row key is its identity
      // here; the two hash fields exist for the just-sent carrier path and are
      // deliberately empty rather than invented. Nothing reads them back.
      messageId: key,
      nativeLocatorSha256: "",
      carrierSha256: "",
      leftPx: row.row.leftPx,
      topPx: row.row.topPx,
      widthPx: row.row.widthPx,
      heightPx: row.row.heightPx,
    });
  }
  syncTranscript();
}

function clearDecodedTranscript(): void {
  if (decodedRows.length === 0 && decodedRowBindings.size === 0) return;
  for (const key of decodedRowBindings.keys()) messagePlaintext.delete(key);
  decodedRows.length = 0;
  decodedRowBindings.clear();
  syncTranscript();
}

async function refreshVerifiedCarrierRows(): Promise<void> {
  const state = await getNativeDiscordOverlayState();
  applyVerifiedCarrierRows(state?.visibleCarrierRows);
}

// ---------------------------------------------------------------------------
// The eye's refresh cadence. Read this before adding anything that calls
// scheduleTranscriptRehydrate().
//
// THERE IS NO TIMER HERE, and there must never be one. Not a repeating
// interval, not a self-rescheduling timeout, not a "backstop" tick, not a
// poll -- this file is asserted to contain no interval timer at all. A previous
// implementation re-resolved Discord's accessibility tree once per poll and
// froze this app for 19,207 ms; the recorded fix was caching plus probe
// rejection, and re-introducing a periodic read would walk straight back into
// it. Every read below is caused by something that actually happened.
//
// The one setTimeout in this section is a trailing-edge COALESCER, the same
// shape as scheduleNativeSurfaceHeal() above: it exists only because an edge
// fired, a pending one is always replaced and never queued behind itself, and
// it never re-arms itself. Its two properties are (a) a burst of edges -- a
// resize drag, a wheel spin -- costs one read rather than one per event, and
// (b) it makes the interval between two reads at least as long as the backend's
// own floor, so an edge is rarely refused in the first place.
//
// The edges, and why each one is a real change on screen rather than a tick:
//
//   * a verified session became ready, or a scope changed  -- different
//     conversation, so every painted row is now the wrong row;
//   * the eye was switched on                              -- nothing was
//     painted before and something must be now;
//   * the backend re-measured the native surface           -- Discord moved,
//     re-themed, re-zoomed or changed DPI, so every rectangle moved with it;
//   * this window resized                                  -- includes the
//     guard loop growing the overlay over the rows OSL just proved, which is
//     the edge that turns the first read of a session into a visible one;
//   * a protected message was sent, received or opened     -- there is a new
//     Discord row that was not there before;
//   * the wheel turned over an OSL-owned pixel             -- the rows moved;
//   * the native guard loop said the rows moved            -- Discord itself
//     moved, resized or settled, which no renderer event reports at all.
//
// The wheel is the weakest of these: OSL only sees it where OSL owns pixels, so
// a scroll delivered entirely to Discord's own window is invisible to this
// renderer. The last edge is the native half of it, raised by the guard loop
// that already ticks over Discord's geometry (NATIVE_DISCORD_ROWS_MOVED_EVENT,
// handled below beside the other native announcements). It closes every case
// where Discord's window moved. It deliberately does NOT close a pure
// mouse-wheel scroll inside Discord: that moves no window, so the guard loop has
// nothing to report, and the only thing that would see it is a repeating read --
// which is exactly the per-tick accessibility re-resolve that once froze this
// app for 19,207 ms. That gap stays open on purpose; nothing here polls for it.
const REHYDRATE_COALESCE_MS = 800;
// Longest a single backend read may be in flight before the eye stops waiting for
// it. Sized above the sum of the backend's own bounds -- 2,000 ms detached
// accessibility leg plus a 2,000 ms decode budget -- so a slow-but-alive read is
// never abandoned and a dead one cannot latch the eye off. See
// runTranscriptRehydrate().
const REHYDRATE_IN_FLIGHT_BUDGET_MS = 6_000;
let rehydrateTimer: number | undefined;
let rehydrateBusy = false;
// At most one replacement may be armed for an edge the backend's floor refused.
// Cleared by any real edge and by a completed read, so this can never chain.
let rehydrateReplacementArmed = false;
// An edge that arrived while a read was already in flight.
//
// THE defect that stopped the eye ever painting: growing the overlay window over
// the rows is itself what makes those rows placeable, and the guard emits its
// `rows-moved` edge on the tick it grows -- which lands *during* the very read
// that cached the rectangles it grew from. `runTranscriptRehydrate` returned at
// `rehydrateBusy` and nothing re-armed, so the one edge that closes the loop was
// the one edge guaranteed to be dropped, and placement sat at zero for the whole
// session while decode, growth and shipping all worked.
let rehydratePending = false;
// The conversation the renderer believes it is showing. It only ever widens the
// backend's own binding, so switching the displayed surface counts as a change
// even when the native binding has not moved.
let rehydrateScope = "";

function cancelTranscriptRehydrate(): void {
  if (rehydrateTimer !== undefined) window.clearTimeout(rehydrateTimer);
  rehydrateTimer = undefined;
  rehydrateReplacementArmed = false;
  rehydratePending = false;
}

/**
 * Coalesce one edge into at most one bounded backend read.
 *
 * Refuses outright when there is nothing to paint -- no session, or the eye off
 * -- so an edge that arrives with the eye down costs zero accessibility work.
 */
function scheduleTranscriptRehydrate(): void {
  rehydrateReplacementArmed = false;
  // A scheduled read serves whatever was outstanding, so the latch is spent
  // here and never survives into a later read as a phantom extra one.
  rehydratePending = false;
  if (!overlayReady || !decryptDisplayEnabled) return;
  if (rehydrateTimer !== undefined) window.clearTimeout(rehydrateTimer);
  rehydrateTimer = window.setTimeout(() => {
    rehydrateTimer = undefined;
    void runTranscriptRehydrate();
  }, REHYDRATE_COALESCE_MS);
}

async function runTranscriptRehydrate(): Promise<void> {
  if (!overlayReady || !decryptDisplayEnabled || !rehydrateScope) return;
  // Remembered, not dropped. The in-flight read was measured against geometry
  // from before this edge, so its answer cannot serve it; the edge is replayed
  // the moment that read finishes. Exactly one bit, so an edge storm still
  // costs exactly one extra read.
  if (rehydrateBusy) {
    rehydratePending = true;
    return;
  }
  rehydrateBusy = true;
  // `rehydrateBusy` is the eye's only concurrency guard, and it was unbounded: an
  // invoke that never settles leaves it latched true and every subsequent edge --
  // eye on, scroll, resize, new message -- returns at the first condition above,
  // in silence, for the rest of the session. One read that never came back was
  // therefore enough to turn the whole feature off with no error anywhere.
  //
  // This is a per-request timeout, not a poll: it is armed by a read that
  // started, is cleared by that read finishing, and never re-arms itself. The
  // budget is longer than every bound the backend leg has (a 1,200 ms
  // accessibility read inside a 2,000 ms detached leg, then a 2,000 ms decode
  // budget), so it can only fire when the answer is genuinely never coming.
  let abandoned = false;
  const watchdog = window.setTimeout(() => {
    abandoned = true;
    rehydrateBusy = false;
    recordInvalidBackendResponse(
      "rehydrate_native_discord_overlay_history",
      "the transcript read never settled",
    );
    // The edge that caused this read is still unserved, so ask again rather than
    // leaving the eye dark. Bounded by the same coalescer as every other edge.
    scheduleTranscriptRehydrate();
  }, REHYDRATE_IN_FLIGHT_BUDGET_MS);
  try {
    const result = await rehydrateNativeDiscordOverlayHistory(rehydrateScope);
    // Abandoned: a later read owns the paint now, and this answer was measured
    // against geometry old enough that the watchdog gave up on it. Applying it
    // would put decrypted text over whatever Discord has since scrolled into
    // place, which is worse than painting nothing.
    if (abandoned) return;
    // A refused, failed or malformed read leaves the previous paint exactly as
    // it was. It is the only honest option: OSL has no newer fact to paint.
    if (!result) return;
    if (!result.read) {
      // The floor refused this edge. Replace it once -- the edge was real, and
      // dropping it silently is what would leave decrypted text over the wrong
      // rows until the operator happened to cause another one. This cannot
      // chain: by the time it runs the floor has elapsed, and the replacement
      // itself is not allowed to arm a second replacement.
      if (rehydrateReplacementArmed || !decryptDisplayEnabled) return;
      rehydrateReplacementArmed = true;
      if (rehydrateTimer !== undefined) window.clearTimeout(rehydrateTimer);
      rehydrateTimer = window.setTimeout(() => {
        rehydrateTimer = undefined;
        void runTranscriptRehydrate();
      }, result.retryAfterMs);
      return;
    }
    rehydrateReplacementArmed = false;
    applyDecodedTranscript(result.rows);
  } finally {
    window.clearTimeout(watchdog);
    if (!abandoned) {
      rehydrateBusy = false;
      // Replay an edge that arrived mid-read -- but only when nothing is already
      // scheduled. A live `rehydrateTimer` is either the floor's replacement,
      // which must keep the backend's own `retryAfterMs`, or a newer edge's
      // coalesced read; both are a guaranteed read and neither may be restarted
      // at this function's shorter delay, which is what could chain.
      if (rehydratePending) {
        rehydratePending = false;
        if (rehydrateTimer === undefined) scheduleTranscriptRehydrate();
      }
    }
  }
}

function transcriptTimestamp(epochMs = Date.now()) {
  return {
    epochMs,
    label: new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" }).format(epochMs),
  };
}

function receiptStatus(label: string): "sent" | "received" | "opened" | "expired" {
  if (/opened/iu.test(label)) return "opened";
  if (/received/iu.test(label)) return "received";
  if (/expired/iu.test(label)) return "expired";
  return "sent";
}

// The only claim OSL can honestly make here is about its own acknowledgement
// ledger for this overlay, never about the peer's identity: OSL has no
// handle -> OSL-identity index and the control inbox gives senders no
// delivery signal, so it can never know whether the other Discord account
// has OSL at all. This is a fixed literal on purpose -- it must never be
// built from draft text, transcript text, or any per-message detail.
const PROTECTED_SEND_CAUTION =
  "Nothing you've sent in this chat has been confirmed opened yet. If they aren't using OSL, they can't read any of it.";

// Evaluated fresh on every confirmed successful send, inside sendDraft(),
// at the same site that clears any stale failure notice: returns the
// caution once the user has sent at least one protected message this
// conversation and none of them has ever advanced past Sent (i.e. none
// reached Received or Opened), and "" otherwise.
function protectedSendCaution(): string {
  return anyProtectedMessageSent && !anyProtectedMessageAcknowledged ? PROTECTED_SEND_CAUTION : "";
}

/**
 * Raise or take down the visible failed-send band over the composer.
 *
 * `notice` is only ever a fixed literal composed inside sendDraft() (optionally
 * plus the backend's fixed carrier-status enum label), so no draft byte,
 * plaintext or Discord row text can reach this element -- the same absolute
 * rule the announced notice follows. Nothing is logged, hashed or persisted
 * here; the text lives in this DOM node and nowhere else.
 *
 * `""` takes the band down and hands the composer back the rows it borrowed.
 * The root flag is what those rows are actually returned by (see
 * `[data-osl-send-failure="true"]` in overlay.css): the band is an absolutely
 * positioned overlay, so without it the operator would lose sight of the very
 * draft the notice just promised was kept.
 *
 * Failures only, never the softer not-yet-opened caution. That caution follows
 * a *successful* send, and a band that seized composer rows after every send
 * would train the operator to swat it away -- which is exactly how the failure
 * signal this fixes would go unread again.
 */
/**
 * The friendly sentence, plus what the backend actually said.
 *
 * The narrow adapters still fail closed and still answer `null`, so nothing
 * about a refusal's *handling* changes here -- only how much of it the operator
 * is allowed to read. The reason used to be caught and discarded in the adapter,
 * so a real cause reached this surface as an unrelated generic sentence.
 *
 * This surfaces exactly what Rust chose to say and never more: several native
 * refusals deliberately share one uniform sentence so the wire cannot reveal
 * which check failed, and this makes none of them more specific. The journal is
 * renderer memory, already sanitized and content-redacted, and nothing here
 * writes to it -- no draft byte, plaintext or transcript row can reach it
 * through this function, because none of them is in scope at either call site.
 */
function withBackendReason(notice: string, command: string): string {
  const reason = lastBackendFailure(command)?.message;
  return reason ? `${notice} ${reason}` : notice;
}

function renderSendFailureBanner(notice: string): void {
  failureBannerText.textContent = notice;
  failureBanner.hidden = notice === "";
  document.documentElement.dataset.oslSendFailure = String(notice !== "");
}

renderSendFailureBanner("");

// The operator's own dismissal is the only way down that is not a send: not a
// keystroke, not a timer, not a poll -- the same rule that keeps the announced
// notice from being cleared before it has been read.
failureBannerDismiss.addEventListener("click", () => {
  renderSendFailureBanner("");
});

function rowElement(key: string): HTMLLIElement {
  const item = transcript.root.querySelector<HTMLLIElement>(`[data-row-key="${CSS.escape(key)}"]`);
  if (!item) throw new Error("Protected transcript row was not rendered");
  return item;
}

function reconcileDraft(): void {
  const bounded = boundedProtectedDraft(draft.value);
  if (bounded !== draft.value) draft.value = bounded;
  const bytes = utf8Length(bounded);
  draftTooLarge = bytes > MAX_PROTECTED_DRAFT_BYTES;
  counter.textContent = draftTooLarge
    ? "Message is too large to send privately."
    : bytes >= PROTECTED_DRAFT_WARNING_BYTES
      ? `${Math.ceil((MAX_PROTECTED_DRAFT_BYTES - bytes) / 1024)} KiB remaining.`
      : "";
  refreshControls();
}

draft.addEventListener("compositionstart", () => { composing = true; });
draft.addEventListener("compositionend", () => { composing = false; reconcileDraft(); });
draft.addEventListener("input", (event) => {
  if (event.isTrusted) {
    typingRate.recordTrustedInput(performance.now(), true, Array.from(draft.value).length);
  }
  if (!composing) reconcileDraft();
  if (!draft.value) typingRate.reset();
});

function refreshControls(): void {
  prepare.disabled = sendBusy || !overlayReady || draftTooLarge;
  chooseAttachment.hidden = !attachmentsEnabled;
  chooseAttachment.disabled = sendBusy || attachmentBusy || !overlayReady || !attachmentsEnabled;
  coverText.disabled = sendBusy || !overlayReady || !discordMarkerAvailable;
  burnChat.disabled = sendBusy || !overlayReady;
  sendMode.disabled = sendBusy || !overlayReady;
  placementMode.disabled = sendBusy || !overlayReady || !discordMarkerAvailable;
  ttl.disabled = sendBusy || securityBusy || !overlayReady;
  decryptDisplay.disabled = sendBusy || securityBusy || !overlayReady;
  viewOnce.disabled = sendBusy || !overlayReady || !viewOnceEnabled;
}

function setBusy(busy: boolean): void {
  sendBusy = busy;
  refreshControls();
}

setBusy(true);

function removeBubble(item: HTMLLIElement): void {
  const timer = messageExpiryTimers.get(item);
  if (timer !== undefined) window.clearTimeout(timer);
  messageExpiryTimers.delete(item);
  viewOnceBubbles.delete(item);
  receivedPlaintextBubbles.delete(item);
  for (const [messageId, bubble] of outgoingBubbles) {
    if (bubble === item) outgoingBubbles.delete(messageId);
  }
  // The correlation handle goes with the bubble, for the same reason its
  // plaintext does below: a removed message must leave nothing behind naming it.
  for (const [messageId, bubble] of incomingBubbles) {
    if (bubble === item) incomingBubbles.delete(messageId);
  }
  const attachmentId = item.dataset.attachmentId;
  if (attachmentId) pendingAttachmentIds.delete(attachmentId);
  const viewOnceId = item.dataset.viewOnceId;
  if (viewOnceId) pendingViewOnceIds.delete(viewOnceId);
  for (const action of item.querySelectorAll<HTMLElement>("[data-transcript-action]")) {
    const actionId = action.dataset.transcriptAction;
    if (!actionId) continue;
    transcriptActions.delete(actionId);
    if (actionId.startsWith("attachment:")) pendingAttachmentIds.delete(actionId.slice("attachment:".length));
    if (actionId.startsWith("view-once:")) pendingViewOnceIds.delete(actionId.slice("view-once:".length));
  }
  const key = item.dataset.rowKey;
  if (key) {
    // The secret goes with the bubble, in the same statement as the row itself:
    // a removed bubble must not leave its plaintext reachable in this renderer.
    messagePlaintext.delete(key);
    const index = transcriptRows.findIndex((row) => row.key === key);
    if (index >= 0) transcriptRows.splice(index, 1);
  }
  item.textContent = "";
  syncTranscript();
}

function removeViewOnceBubbles(): void {
  for (const item of [...viewOnceBubbles]) removeBubble(item);
}

/**
 * The eye is the ONLY control over what the operator sees.
 *
 * Off: OSL displays nothing over Discord's conversation. The transcript layer
 * leaves the DOM entirely, so the operator is looking at native Discord -- real
 * history, everyone's messages, scrollback, and the cover sentence exactly as
 * Discord received it. Nothing is "revealed"; there was never anything on top.
 *
 * On: OSL paints its decrypted text over the specific rows the backend has
 * proven the position of, and over nothing else.
 *
 * The lock is deliberately absent from this function. The lock is encryption
 * only and changes nothing about what is displayed.
 */
function applyDecryptDisplayVisibility(visible: boolean): void {
  if (!visible) removeViewOnceBubbles();
  messageList.hidden = !visible;
  if (!visible) {
    // Eye off: OSL displays nothing over Discord's conversation, so every
    // decrypted Discord row it was painting is dropped along with its text and
    // no further read is claimed. Turning the eye back on is an edge that reads
    // again from scratch.
    cancelTranscriptRehydrate();
    clearDecodedTranscript();
  }
  for (const row of [...transcriptRows, ...decodedRows]) {
    if (row.kind !== "text" && row.kind !== "reply") continue;
    // Only ever the plaintext. A row OSL cannot decrypt or cannot place is not
    // painted at all, so there is no covered state to render and no cover prose
    // for this renderer to guess at.
    row.plaintext = messagePlaintext.get(row.key) ?? "";
    row.plaintextHidden = !visible;
  }
  for (const row of transcriptRows) {
    if (row.kind === "receipt" && row.action?.id.startsWith("view-once:")) row.action.disabled = !visible;
  }
  syncTranscript();
  for (const item of receivedPlaintextBubbles) {
    const body = item.querySelector("p");
    if (body) body.hidden = !visible;
  }
  for (const item of transcript.root.querySelectorAll<HTMLLIElement>('[data-row-kind="receipt"]')) {
    const reveal = item.querySelector<HTMLButtonElement>("button");
    if (reveal) reveal.disabled = !visible;
  }
}

function clearMessageBubbles(): void {
  for (const item of [...messageExpiryTimers.keys()]) removeBubble(item);
  receivedPlaintextBubbles.clear();
  incomingBubbles.clear();
  // `messagePlaintext` holds the decoded Discord rows' text too, so the rows
  // themselves have to go with it rather than being left rendering nothing.
  decodedRows.length = 0;
  decodedRowBindings.clear();
  messagePlaintext.clear();
  transcriptRows.splice(0);
  transcriptActions.clear();
  syncTranscript();
}

function appendBubble(
  direction: "outgoing" | "incoming",
  plaintext: string,
  receipt: string,
  expiresAt: number,
  viewOnceMessage: boolean,
): HTMLLIElement {
  const key = `message-${++transcriptSequence}`;
  // Read only to paint, and only while the eye is on.
  messagePlaintext.set(key, plaintext);
  transcriptRows.push({
    key,
    kind: "text",
    direction,
    author: direction === "outgoing" ? localIdentity : verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    plaintext,
    // A row is born in whichever mode the eye is already in, so opening a
    // session with the eye off never flashes plaintext.
    plaintextHidden: !decryptDisplayEnabled,
    receipt: { status: receiptStatus(receipt), label: receipt },
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add(direction);
  const body = item.querySelector<HTMLParagraphElement>(".osl-discord-transcript__plaintext");
  if (direction === "incoming" && !viewOnceMessage) {
    receivedPlaintextBubbles.add(item);
    if (body) body.hidden = !decryptDisplayEnabled;
  }
  if (viewOnceMessage) viewOnceBubbles.add(item);
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
  while (transcriptRows.length > 24) {
    // Explicitly this session's own oldest row, never "the first row in the
    // DOM": the decoded Discord rows share this list now, and trimming one of
    // those would silently punch a hole in the operator's history instead of
    // dropping a bubble OSL itself created.
    const oldestKey = transcriptRows[0]?.key;
    const oldest = oldestKey
      ? transcript.root.querySelector<HTMLLIElement>(`[data-row-key="${CSS.escape(oldestKey)}"]`)
      : null;
    if (oldest) removeBubble(oldest);
    else break;
  }
  item.scrollIntoView({ block: "nearest" });
  return item;
}

function applyAcknowledgment(messageId: string, receipt: "received" | "opened"): void {
  // This function is only ever called with a genuine Received/Opened
  // acknowledgment from the backend's own ledger, so its mere invocation
  // proves at least one sent message advanced past Sent -- true regardless of
  // whether a local bubble for it still exists to update below.
  anyProtectedMessageAcknowledged = true;
  const item = outgoingBubbles.get(messageId);
  const state = item?.querySelector<HTMLElement>(".osl-discord-transcript__receipt");
  if (!item || !state) return;
  const label = receipt === "opened" ? "Opened in OSL" : "Received by OSL";
  state.textContent = label;
  const row = transcriptRows.find((candidate) => candidate.key === item.dataset.rowKey);
  if ((row?.kind === "text" || row?.kind === "reply") && row.receipt) {
    row.receipt = { status: receipt, label };
    syncTranscript();
  }
  if (receipt === "opened") outgoingBubbles.delete(messageId);
}

function appendPendingAttachment(attachment: { attachmentId: string; originalFilename: string; plaintextSize: number; expiresAt: number; viewOnce: boolean }): void {
  if (pendingAttachmentIds.has(attachment.attachmentId)) return;
  pendingAttachmentIds.add(attachment.attachmentId);
  const key = `attachment-${++transcriptSequence}`;
  const actionId = `attachment:${attachment.attachmentId}`;
  const sizeLabel = attachment.plaintextSize >= 1024 * 1024
    ? `${(attachment.plaintextSize / (1024 * 1024)).toFixed(1)} MB`
    : `${Math.ceil(attachment.plaintextSize / 1024)} KB`;
  transcriptRows.push({
    key,
    kind: "text",
    direction: "incoming",
    author: verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    plaintext: attachment.originalFilename,
    receipt: { status: "received", label: `${sizeLabel}${attachment.viewOnce ? " · view once" : ""}` },
    media: [{
      key: attachment.attachmentId,
      kind: "file",
      label: attachment.originalFilename,
      detail: sizeLabel,
      state: "available",
      action: { id: actionId, label: "Open privately" },
    }],
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add("incoming", "attachment");
  item.dataset.attachmentId = attachment.attachmentId;
  transcriptActions.set(actionId, () => void (async () => {
    if (attachmentBusy) return;
    const open = item.querySelector<HTMLButtonElement>("button");
    if (!open) return;
    attachmentBusy = true;
    open.disabled = true;
    refreshControls();
    status.textContent = "Authenticating attachment…";
    const result = await openNativeDiscordOverlayAttachment(attachment.attachmentId);
    attachmentBusy = false;
    refreshControls();
    if (!result) {
      open.disabled = false;
      status.textContent = withBackendReason(
        "That attachment could not be opened safely.",
        "open_native_discord_overlay_attachment",
      );
      return;
    }
    removeBubble(item);
    status.textContent = result.viewOnceConsumed
      ? "Opened once in OSL's protected viewer."
      : "Opened in OSL's private viewer.";
  })());
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(attachment.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function appendPendingViewOnce(message: { messageId: string; expiresAt: number }): void {
  if (pendingViewOnceIds.has(message.messageId)) return;
  pendingViewOnceIds.add(message.messageId);
  const key = `view-once-${++transcriptSequence}`;
  const actionId = `view-once:${message.messageId}`;
  const body = document.createElement("p");
  body.textContent = "View-once message";
  transcriptRows.push({
    key,
    kind: "receipt",
    direction: "incoming",
    author: verifiedFriendIdentity,
    timestamp: transcriptTimestamp(),
    status: "received",
    label: `${body.textContent} · Received by OSL · unopened`,
    action: { id: actionId, label: "Reveal once", disabled: !decryptDisplayEnabled },
  });
  syncTranscript();
  const item = rowElement(key);
  item.classList.add("incoming", "view-once-pending");
  item.dataset.viewOnceId = message.messageId;
  transcriptActions.set(actionId, () => void (async () => {
    if (receiveBusy || !decryptDisplayEnabled) return;
    const reveal = item.querySelector<HTMLButtonElement>("button");
    if (!reveal) return;
    reveal.textContent = "Reveal once";
    receiveBusy = true;
    reveal.disabled = !decryptDisplayEnabled;
    reveal.disabled = true;
    status.textContent = "Opening view-once message…";
    const opened = await revealNativeDiscordOverlayViewOnce(message.messageId);
    receiveBusy = false;
    if (!opened || !opened.viewOnceConsumed) {
      reveal.disabled = false;
      status.textContent = withBackendReason(
        "That view-once message could not be opened safely.",
        "reveal_native_discord_overlay_view_once",
      );
      scheduleReceivePoll(idlePollMs);
      return;
    }
    removeBubble(item);
    // Registered under the same handle the pending placeholder used, so the
    // revealed text and the entry it replaces name one message rather than two.
    incomingBubbles.set(
      opened.messageId,
      appendBubble("incoming", opened.plaintext, "Received · opened once", opened.expiresAt, true),
    );
    status.textContent = "View-once message opened in OSL.";
    scheduleReceivePoll(0);
  })());
  const expiryTimer = window.setTimeout(() => removeBubble(item), overlayExpiryDelayMs(message.expiresAt, Date.now()));
  messageExpiryTimers.set(item, expiryTimer);
}

function scheduleReceivePoll(delayMs: number): void {
  if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
  receiveTimer = undefined;
  if (!shouldPollDiscordOverlay({
    overlayReady,
    decryptDisplayEnabled,
    documentHidden: document.hidden,
    discordQaShell,
  })) return;
  receiveTimer = window.setTimeout(() => void pollReceived(), delayMs);
}

async function pollReceived(): Promise<void> {
  if (receiveBusy || !shouldPollDiscordOverlay({
    overlayReady,
    decryptDisplayEnabled,
    documentHidden: document.hidden,
    discordQaShell,
  })) return;
  receiveBusy = true;
  try {
    const batch = await openNativeDiscordOverlayText();
    if (!batch) throw new Error("invalid receive response");
    // The two states an empty batch used to hide. Neither short-circuits the rest
    // of this poll: receipts and attachments keep flowing in both of them, and an
    // early return here would trade one silent loss for another.
    if (!batch.decryptDisplayEnabled) {
      // This renderer only polls with the eye on, so a batch reporting opening as
      // OFF means its copy of that setting is stale. Previously unknowable: a
      // suppressed batch was byte-identical to an empty inbox.
      recordInvalidBackendResponse("open_native_discord_overlay_text",
        "the backend reports decrypted display off for this conversation");
    }
    if (batch.deferredRows > 0) {
      // Rows the backend could not resolve against the protected message store
      // and deliberately left in place. This is "incomplete, retry", not "nothing
      // arrived", and it used to be reported as the latter.
      recordInvalidBackendResponse("open_native_discord_overlay_text",
        "the backend deferred rows it could not resolve");
    }
    let opened = 0;
    for (const message of batch.messages) {
      // Named by its correlation handle, so a message the backend surfaces a
      // second time -- a row it could not delete, a receipt replay -- updates
      // nothing instead of appending a second bubble for the same text.
      if (incomingBubbles.has(message.messageId)) continue;
      const item = appendBubble("incoming", message.plaintext, message.viewOnceConsumed ? "Received · opened once" : "Received · opened", message.expiresAt, message.viewOnceConsumed);
      incomingBubbles.set(message.messageId, item);
      opened += 1;
    }
    for (const message of batch.pendingViewOnce) appendPendingViewOnce(message);
    for (const acknowledgment of batch.acknowledgments) {
      applyAcknowledgment(acknowledgment.messageId, acknowledgment.status);
    }
    await refreshVerifiedCarrierRows();
    // NEW-MESSAGE EDGE. Deliberately conditional: this function is itself a
    // poll, so scheduling a transcript read unconditionally here would make the
    // eye poll Discord's accessibility tree once per receive tick -- exactly the
    // per-poll re-resolve that froze this app. A read is claimed only when the
    // backend actually reported something new, i.e. when a Discord row that was
    // not there before now is.
    if (batch.messages.length > 0 || batch.pendingViewOnce.length > 0
      || batch.acknowledgments.length > 0) {
      scheduleTranscriptRehydrate();
    }
    const attachments = attachmentsEnabled ? await listNativeDiscordOverlayAttachments() : [];
    if (!attachments) throw new Error("invalid attachment response");
    for (const attachment of attachments) appendPendingAttachment(attachment);
    // A deferred row keeps the poll brisk on purpose: backing off while the store
    // is unreachable is how a transient outage turns into a ten-second-deep hole.
    idlePollMs = opened > 0 || batch.pendingViewOnce.length > 0 || batch.deferredRows > 0 || attachments.length > 0 ? 2_000 : Math.min(idlePollMs * 2, 10_000);
    // Fixed sentences only, and only ever about counts and states -- never a
    // fragment of what arrived.
    if (opened > 0) status.textContent = `${opened} private ${opened === 1 ? "message" : "messages"} received through OSL.`;
    else if (batch.deferredRows > 0) status.textContent = "OSL could not reach the protected message store. Retrying.";
    else if (!batch.decryptDisplayEnabled) status.textContent = "Decrypted text is off for this conversation.";
  } catch (error) {
    // This was the last swallowed failure in the file: a bare `catch` that only
    // doubled the poll interval, so even the one error the backend did return was
    // invisible. Whatever the adapters refuse they already journal themselves;
    // what reaches here is this loop's own validation, so it is journalled under
    // the command the loop exists to call.
    recordBackendFailure("open_native_discord_overlay_text", error);
    idlePollMs = Math.min(idlePollMs * 2, 10_000);
  } finally {
    receiveBusy = false;
    scheduleReceivePoll(idlePollMs);
  }
}

document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    removeViewOnceBubbles();
    if (!discordQaShell) {
      if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
      receiveTimer = undefined;
    } else {
      idlePollMs = 2_000;
      scheduleReceivePoll(0);
    }
  } else {
    idlePollMs = 2_000;
    scheduleReceivePoll(0);
  }
});
window.addEventListener("blur", removeViewOnceBubbles);

function measuredCarrierLayout(): NativeDiscordCarrierLayout | undefined {
  if (activeVisualRecipe) {
    return {
      contentWidthPx: activeVisualRecipe.messageColumnWidthPx,
      averageGraphemeWidthPx: activeVisualRecipe.averageGraphemeWidthPx,
      lineHeightPx: activeVisualRecipe.lineHeightPx,
      zoom: activeVisualRecipe.zoom,
      density: activeVisualRecipe.dpiScale,
      padding: "shapeMatched",
      rowKind: "plainText",
    };
  }
  const style = window.getComputedStyle(draft);
  const fontSize = Number.parseFloat(style.fontSize);
  const lineHeight = Number.parseFloat(style.lineHeight);
  const paddingInline = Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
  const contentWidth = draft.clientWidth - paddingInline;
  if (![fontSize, lineHeight, paddingInline, contentWidth].every(Number.isFinite)
    || fontSize <= 0 || lineHeight <= 0 || contentWidth <= 0) return undefined;
  return {
    contentWidthPx: contentWidth,
    // A bounded font-only estimate; no draft text or per-character cadence is measured.
    averageGraphemeWidthPx: fontSize * 0.56,
    lineHeightPx: lineHeight,
    zoom: 1,
    density: 1,
    padding: "shapeMatched",
    rowKind: "plainText",
  };
}

async function sendDraft(): Promise<void> {
  // A refusal here must always be visible. Returning silently is how a real
  // Enter looked identical to no Enter at all: no status, no stage, no receipt.
  if (!overlayReady) {
    recordDiscordQaSendStage("renderer_send_refused_not_ready");
    status.textContent = "Protected Discord is still being verified. Your draft is still here.";
    return;
  }
  // Encryption is the lock's whole job. With it down OSL has no composer on
  // screen at all, so this is only reachable through a stale gesture.
  if (!lockEngaged) {
    recordDiscordQaSendStage("renderer_send_refused_not_ready");
    status.textContent = "Encryption is off. Turn the lock on to send privately.";
    return;
  }
  if (sendBusy) {
    recordDiscordQaSendStage("renderer_send_refused_busy");
    status.textContent = "OSL is still finishing the previous private send. Your draft is still here.";
    return;
  }
  const plaintext = boundedProtectedDraft(draft.value);
  if (!plaintext) {
    recordDiscordQaSendStage("renderer_send_refused_empty_draft");
    status.textContent = "Write a message first.";
    return;
  }
  if (utf8Length(plaintext) > MAX_PROTECTED_DRAFT_BYTES) {
    recordDiscordQaSendStage("renderer_send_refused_too_large");
    status.textContent = "This private message is too large.";
    return;
  }
  setBusy(true);
  status.textContent = "Encrypting…";
  // Clear point 1 of 2 (see the other beside `draft.value = ""` below): a
  // genuine new send attempt (every early-return guard above already passed)
  // is the "user edited the draft and is sending again" signal -- it is
  // deliberately NOT a keystroke or a timer. If this very attempt fails again
  // too, the `!markerSent` branch further down repopulates this immediately
  // with the current failure detail, so nothing is ever silently lost.
  sendWarning.textContent = "";
  // The visible band comes down with it, at the same instant and for the same
  // reason, so the composer is back to its full measured height while this
  // attempt runs and a stale failure can never be mistaken for this one's.
  renderSendFailureBanner("");
  recordDiscordQaSendStage("renderer_send_started");
  const requestedViewOnce = viewOnce.checked;
  try {
    const refreshedState = await getNativeDiscordOverlayState();
    if (!refreshedState) {
      recordDiscordQaSendStage("renderer_send_refused_state_unavailable");
      throw new Error("overlay state changed");
    }
    applyDiscordVisualRecipe(refreshedState.visualRecipe);
    applyNativeSurfaceCapture(refreshedState.nativeSurface);
    coverTextEnabled = refreshedState.covertextEnabled;
    // Marker availability is proven by the same freshly verified backend state
    // as covertext. Reading a value cached at overlay-init time could refuse a
    // send the backend has since calibrated, or accept one it has since lost.
    discordMarkerAvailable = refreshedState.discordMarkerAvailable;
    let markerSent = false;
    let carrierStatusLabel: string | undefined;
    let immediateCarrierRow: NativeDiscordCarrierRowBinding | undefined;
    let result: Awaited<ReturnType<typeof prepareNativeDiscordOverlayText>>;
    if (discordQaShell) {
      if (!discordMarkerAvailable || !coverTextEnabled) {
        recordDiscordQaSendStage(discordMarkerAvailable
          ? "renderer_send_refused_covertext_off"
          : "renderer_send_refused_marker_unavailable");
        throw new Error("Discord carrier is unavailable");
      }
      const requestedPlacement: NativeDiscordCarrierMode = placementMode.value === "compatibility" ? "compatibility" : "atomic";
      const charsPerSecond = typingRate.charsPerSecond();
      recordDiscordQaSendStage("renderer_send_command_invoked");
      const atomic = await sendNativeDiscordQaAtomicText(
        plaintext,
        requestedViewOnce,
        requestedPlacement,
        charsPerSecond,
        measuredCarrierLayout(),
      );
      if (!atomic) {
        recordDiscordQaSendStage("renderer_send_command_rejected");
        throw new Error("atomic protected send failed");
      }
      recordDiscordQaSendStage("renderer_send_command_accepted");
      result = atomic.prepared;
      carrierStatusLabel = atomic.carrier.status;
      markerSent = atomic.carrier.status === "sent"
        && atomic.carrier.placed
        && atomic.carrier.enterSent
        && atomic.visibleCarrierRow !== undefined;
      immediateCarrierRow = atomic.visibleCarrierRow;
    } else {
      result = await prepareNativeDiscordOverlayText(plaintext, requestedViewOnce);
      if (result && discordMarkerAvailable && coverTextEnabled) {
        const requestedPlacement: NativeDiscordCarrierMode = placementMode.value === "compatibility" ? "compatibility" : "atomic";
        const charsPerSecond = typingRate.charsPerSecond();
        const carrier = await sendNativeDiscordOverlayCarrier(requestedPlacement, charsPerSecond, measuredCarrierLayout());
        carrierStatusLabel = carrier?.status;
        markerSent = carrier?.status === "sent" && carrier.placed && carrier.enterSent;
      }
    }
    if (!result || result.viewOnce !== requestedViewOnce) {
      recordDiscordQaSendStage("renderer_send_refused_invalid_response");
      throw new Error("invalid protected response");
    }
    // The encrypted OSL inbox commit above is NOT a substitute for Discord
    // actually receiving the flag-message carrier. The command can return
    // Ok(...) even when Discord never got it (see send_native_discord_qa_atomic_text
    // in main.rs), so markerSent is checked explicitly here: this must never
    // read as success, and the draft must survive so the user can retry.
    if (!markerSent) {
      recordDiscordQaSendStage("renderer_send_failed");
      // carrierStatusLabel is a fixed backend enum string (e.g. "ContextChanged"),
      // never draft text -- this composes only fixed literals plus that enum
      // value, so no draft content, plaintext, or Discord row text ever lands
      // in this message, in a log, or in any persisted state.
      const failureNotice = carrierStatusLabel
        ? `Discord did not receive this message (carrier status: ${carrierStatusLabel}). Your draft is still here.`
        : "Discord did not receive this message. Your draft is still here.";
      status.textContent = failureNotice;
      // Written to the persistent notice too (not instead of `status`, so the
      // existing aria-live="polite" announcement still fires immediately) --
      // this copy survives the churn on `status` described above, and stays
      // up until one of the two clear points documented at their call sites.
      sendWarning.textContent = failureNotice;
      // ...and to the band that the natively sized 736x58 window can actually
      // show, because everything above this line is announcement only: the
      // element it was written to has no room in this window, which is how a
      // failed send came to look identical to a successful one.
      renderSendFailureBanner(failureNotice);
      return;
    }
    const outgoing = appendBubble("outgoing", plaintext, result.viewOnce
      ? `Sent to OSL · view once${markerSent ? " · Discord marked" : " · OSL only"}`
      : `Sent to OSL${markerSent ? " · Discord marked" : " · OSL only"}`,
      result.expiresAt, result.viewOnce);
    outgoingBubbles.set(result.messageId, outgoing);
    anyProtectedMessageSent = true;
    if (immediateCarrierRow) {
      verifiedCarrierRows = [
        ...verifiedCarrierRows.filter((row) =>
          row.messageId !== immediateCarrierRow.messageId
          && row.nativeLocatorSha256 !== immediateCarrierRow.nativeLocatorSha256),
        immediateCarrierRow,
      ];
      applyVerifiedCarrierRows(verifiedCarrierRows);
    } else {
      await refreshVerifiedCarrierRows();
    }
    // NEW-MESSAGE EDGE, the outgoing half: Discord has just accepted a carrier
    // row that was not on screen a moment ago, and it is one this account can
    // decrypt. One edge per confirmed send -- `markerSent` is already proven
    // above, so a send Discord never received raises nothing.
    scheduleTranscriptRehydrate();
    draft.value = "";
    typingRate.reset();
    reconcileDraft();
    // Clear point 2 of 2 (see clear point 1 above, near "Encrypting…"): this
    // line is only reached once markerSent is confirmed true, i.e. a send
    // that actually succeeded. Restated explicitly here (clear point 1 above
    // already ran earlier in this same attempt) so the guarantee "a
    // successful send always leaves no stale failure notice behind" holds on
    // its own, independent of anything earlier in the function.
    sendWarning.textContent = "";
    // The band comes down on the same guarantee: a confirmed success must never
    // leave a failure notice standing on the composer, and the draft gets its
    // full measured height back in the same breath.
    renderSendFailureBanner("");
    // Immediately after that guarantee runs, this confirmed success is also
    // exactly the moment this conversation's aggregate ack ledger could have
    // changed, so this is where the softer caution is (re)read.
    // `protectedSendCaution()` leaves the blank above alone when it does not
    // apply, so the guarantee above still holds on its own either way.
    sendWarning.textContent = protectedSendCaution();
    status.textContent = !discordMarkerAvailable
      ? "Sent privately through OSL only. No Discord marker was attempted."
      : !coverTextEnabled
        ? "Sent privately through OSL only. Covertext off · private messages travel through OSL only."
      : markerSent
      ? "Sent privately through OSL. Discord received only the private-message marker."
      : "Sent privately through OSL. Discord changed, so its marker was not sent.";
    recordDiscordQaSendStage("renderer_send_complete");
  } catch {
    // The draft is deliberately left untouched above, so a stopped send always
    // keeps the user's protected text.
    recordDiscordQaSendStage("renderer_send_failed");
    // The same defect, one branch over: this is `renderer_send_failed` just as
    // much as the marker check is, no bubble was appended and nothing left for
    // Discord, so it must not be told only to the shared transient line either.
    // A fixed literal, like every other notice on this path -- the thrown error
    // is never read, so no draft byte and no backend detail can reach the DOM.
    const stoppedNotice = "Protection stopped safely. Nothing was sent. Your draft is still here.";
    status.textContent = stoppedNotice;
    sendWarning.textContent = stoppedNotice;
    renderSendFailureBanner(stoppedNotice);
  } finally {
    typingRate.reset();
    setBusy(false);
  }
}

prepare.addEventListener("click", () => void sendDraft());

// Window-level and capture-phase on purpose: the disposable QA build must be
// able to prove that an Enter reached the protected WebView even when the
// trusted textarea does not hold DOM focus, because that is the one case where
// the gesture listener on the textarea can never run. Only Enter is recorded,
// so ordinary typing never touches the stage trail.
if (discordQaShell) {
  window.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    recordDiscordQaSendStage("renderer_keydown_observed");
    if (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey
      || event.isComposing || event.repeat) return;
    recordDiscordQaSendStage("renderer_enter_recognised");
    // The textarea's own listener already owns this keystroke.
    if (event.target === draft) return;
    // Route it through the exact configured single-Enter gesture rather than
    // dropping it. Every identity, scope, geometry, and context check still
    // runs in the backend below.
    if (sendMode.value !== "single") return;
    recordDiscordQaSendStage("renderer_enter_refocused_draft");
    event.preventDefault();
    draft.focus({ preventScroll: true });
    void sendDraft();
  }, true);
}

// The disposable QA build can exercise the real P2P encryption and inbox path
// with one synthetic message by posting F12 to the overlay renderer. This
// avoids all WebView2 descendant UIA calls. Production bundles do not install
// the gesture, and production native binaries do not contain its command.
if (discordQaShell) {
  window.addEventListener("keydown", (event) => {
    if (event.key !== "F12" || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    if (sendBusy || !overlayReady) return;
    void (async () => {
      setBusy(true);
      status.textContent = "Sending QA probe…";
      try {
        const result = await sendNativeDiscordQaProbe();
        if (!result || !result.personToPersonE2ee || result.viewOnce || !result.deliveredToOslInbox) {
          throw new Error("invalid QA probe receipt");
        }
        const outgoing = appendBubble(
          "outgoing",
          "OSL Discord QA probe",
          "Sent to OSL · deterministic QA probe",
          result.expiresAt,
          false,
        );
        outgoingBubbles.set(result.messageId, outgoing);
        status.textContent = "QA probe sent privately through OSL.";
      } catch {
        status.textContent = "QA probe stopped safely.";
      } finally {
        setBusy(false);
      }
    })();
  });
}

coverText.addEventListener("click", () => {
  if (!overlayReady || !discordMarkerAvailable || sendBusy) return;
  status.textContent = "Use Covertext in the trusted OSL header.";
});

chooseAttachment.addEventListener("click", () => void (async () => {
  if (attachmentBusy || !overlayReady || !attachmentsEnabled) return;
  attachmentBusy = true;
  refreshControls();
  status.textContent = "Choose a file up to 500 MB…";
  const result = await selectNativeDiscordOverlayAttachment(viewOnce.checked);
  attachmentBusy = false;
  refreshControls();
  if (result === "cancelled") {
    status.textContent = "Attachment canceled.";
    return;
  }
  if (!result) {
    status.textContent = "Attachment protection stopped safely.";
    return;
  }
  appendBubble(
    "outgoing",
    result.originalFilename,
    result.viewOnce ? "Sent to OSL · attachment · view once" : "Sent to OSL · attachment",
    result.expiresAt,
    result.viewOnce,
  );
  status.textContent = "Attachment sent privately through OSL.";
})());

function resetBurnConfirmation(): void {
  burnConfirmation.reset();
  if (burnTimer !== undefined) window.clearTimeout(burnTimer);
  burnTimer = undefined;
  burnChat.textContent = "Burn";
}

burnChat.addEventListener("click", (event) => {
  const step = burnConfirmation.step(performance.now(), event.isTrusted);
  if (step === "ignored") return;
  if (step === "armed") {
    burnChat.textContent = "Confirm burn";
    status.textContent = "This removes this OSL chat locally and tries to delete its remote OSL blobs. Discord history and recipient copies stay untouched. Click again to confirm.";
    if (burnTimer !== undefined) window.clearTimeout(burnTimer);
    burnTimer = window.setTimeout(() => {
      burnTimer = undefined;
      if (burnConfirmation.expire(performance.now())) {
        burnChat.textContent = "Burn";
        status.textContent = "Burn confirmation expired. Nothing was deleted.";
      }
    }, 10_000);
    return;
  }
  void (async () => {
    if (burnTimer !== undefined) window.clearTimeout(burnTimer);
    burnTimer = undefined;
    setBusy(true);
    burnChat.textContent = "Burning…";
    status.textContent = "Burning this OSL chat…";
    const result = await burnNativeDiscordOverlayChat();
    if (!result) {
      resetBurnConfirmation();
      setBusy(false);
      status.textContent = "Burn stopped safely. Review this chat before trying again.";
      return;
    }
    const remote = result.remoteBlobDeletionsFailed === 0
      ? `${result.remoteBlobsDeleted} remote OSL blobs deleted.`
      : `${result.remoteBlobsDeleted} remote OSL blobs deleted; ${result.remoteBlobDeletionsFailed} could not be deleted and remain tracked for retry.`;
    clearMessageBubbles();
    status.textContent = `OSL chat burned. ${result.localProtectedRowsDestroyed} local protected rows removed. ${remote} Discord history and recipient copies were not deleted.`;
  })();
});

function clearGestureTimer(): void {
  if (gestureTimer !== undefined) window.clearTimeout(gestureTimer);
  gestureTimer = undefined;
}

sendMode.addEventListener("change", () => {
  const mode = sendMode.value;
  if (mode !== "button" && mode !== "double" && mode !== "single") {
    sendMode.value = "button";
    sendGesture.setMode("button");
  } else {
    sendGesture.setMode(mode as OverlaySendMode);
  }
  clearGestureTimer();
  status.textContent = mode === "single" ? "Single Enter is experimental. Shift+Enter always adds a line." : mode === "double" ? "Press Enter twice to send. Shift+Enter adds a line." : "Use Send privately. Enter adds a line.";
});

function keyboardGesture(event: KeyboardEvent) {
  return {
    key: event.key,
    shiftKey: event.shiftKey,
    repeat: event.repeat,
    // WebView2 marks our disposable QA harness's OS-injected key event as
    // untrusted. Accept it only in the compile-gated QA shell so the same
    // one-Enter path can be exercised end to end. Production remains strict.
    isTrusted: event.isTrusted || discordQaShell,
    isComposing: event.isComposing,
    now: performance.now(),
  };
}

draft.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    sendGesture.cancel();
    clearGestureTimer();
    status.textContent = "Send canceled. Your draft is still here.";
    return;
  }
  const mode = sendMode.value as OverlaySendMode;
  const plainTrustedEnter = event.key === "Enter"
    && !event.shiftKey
    && (event.isTrusted || discordQaShell)
    && !event.isComposing;
  const result = sendGesture.keydown(keyboardGesture(event));
  if (plainTrustedEnter && mode !== "button") event.preventDefault();
  if (result === "send") {
    sendGesture.cancel();
    clearGestureTimer();
    void sendDraft();
  }
});

draft.addEventListener("keyup", (event) => {
  const result = sendGesture.keyup(keyboardGesture(event));
  if (result === "send") {
    clearGestureTimer();
    void sendDraft();
  } else if (result === "armed") {
    clearGestureTimer();
    status.textContent = "Press Enter again to send. Escape cancels.";
    gestureTimer = window.setTimeout(() => {
      gestureTimer = undefined;
      if (sendGesture.expire(performance.now())) status.textContent = "Double Enter expired. Your draft is still here.";
    }, 1_200);
  }
});

function expiryLabel(seconds: NativeOverlayTtlSeconds): string {
  if (seconds === 3_600) return "1 hour";
  if (seconds === 86_400) return "1 day";
  if (seconds === 259_200) return "3 days";
  return "7 days";
}

async function saveSecurity(): Promise<void> {
  if (!overlayReady || securityBusy) return;
  const requestedTtl = Number(ttl.value);
  if (!NATIVE_OVERLAY_TTL_OPTIONS.includes(requestedTtl as NativeOverlayTtlSeconds)) {
    ttl.value = String(confirmedTtlSeconds);
    return;
  }
  const previousTtl = confirmedTtlSeconds;
  const previousDecrypt = decryptDisplayEnabled;
  const requestedDecrypt = decryptDisplay.checked;
  if (!requestedDecrypt) {
    // Hiding is immediate and conservative; a failed save restores the exact
    // prior visibility below. Do not allow a receive poll to race the toggle.
    decryptDisplayEnabled = false;
    applyDecryptDisplayVisibility(false);
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  }
  securityBusy = true;
  refreshControls();
  status.textContent = "Saving protection…";
  const saved = await setNativeDiscordOverlaySecurity(requestedTtl as NativeOverlayTtlSeconds, decryptDisplay.checked);
  securityBusy = false;
  if (!saved) {
    ttl.value = String(previousTtl);
    decryptDisplay.checked = previousDecrypt;
    decryptDisplayEnabled = previousDecrypt;
    applyDecryptDisplayVisibility(previousDecrypt);
    if (previousDecrypt) scheduleReceivePoll(0);
    status.textContent = "That change was not saved. The previous protection stays active.";
    refreshControls();
    return;
  }
  confirmedTtlSeconds = saved.ttlSeconds;
  decryptDisplayEnabled = saved.decryptDisplayEnabled;
  viewOnceEnabled = saved.viewOnceEnabled;
  attachmentsEnabled = saved.attachmentsEnabled;
  discordMarkerAvailable = saved.discordMarkerAvailable;
  ttl.value = String(saved.ttlSeconds);
  decryptDisplay.checked = saved.decryptDisplayEnabled;
  currentExpiry.textContent = `Current: ${expiryLabel(saved.ttlSeconds)}`;
  // Existing non-view-once plaintext stays in this bounded DOM lifetime and
  // is revealed synchronously before polling resumes. No message is reopened.
  applyDecryptDisplayVisibility(decryptDisplayEnabled);
  refreshControls();
  status.textContent = "Protection updated.";
  if (decryptDisplayEnabled) {
    scheduleReceivePoll(0);
    // EYE-ON EDGE. Nothing was painted a moment ago and something must be now,
    // so this is the read that puts the operator's decryptable history on
    // screen. Turning the eye off raises no edge at all: applyDecryptDisplay-
    // Visibility() above already cancelled the pending read and dropped the rows.
    scheduleTranscriptRehydrate();
  } else {
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  }
}

async function refreshProtectedDisplayVisibility(): Promise<void> {
  const state = await getNativeDiscordOverlayState();
  if (!state || !state.active) {
    // No readable session, so there is nothing left to paint -- but that is NOT
    // evidence the eye was turned off. Forcing it off here is what made lowering
    // the lock also switch the operator's decrypted display off. Drop the rows
    // and the sampled surface, keep the eye exactly as it was.
    applyNativeSurfaceCapture();
    applyVerifiedCarrierRows([]);
    // Nothing readable to place against, so nothing may stay painted either.
    cancelTranscriptRehydrate();
    clearDecodedTranscript();
    // ...but "unreadable" is not "over". `null` here means the state command
    // itself refused or threw -- and that command shares ONE non-blocking native
    // accessibility gate with the eye's own read, so any other in-flight
    // accessibility operation is enough to produce it. Cancelling the armed read
    // on that answer is how a single unlucky refresh could disarm the eye for the
    // rest of the session: nothing repaints it, because every edge that would
    // have is the edge that was just cancelled.
    //
    // A session the backend positively reports as `active: false` really is over
    // and stays cancelled. An unreadable one is asked again instead, through the
    // same coalescer as every other edge -- no new timer, no chain, and refused
    // outright when the eye is down.
    if (!state) scheduleTranscriptRehydrate();
    // No readable session is also no engagement, so the caret grant belongs to
    // whichever raise comes next rather than to the one that just stopped.
    caretGrantedForEngagement = false;
    return;
  }
  applyLockEngaged(state.lockEngaged ?? true);
  applyDiscordVisualRecipe(state.visualRecipe);
  applyNativeSurfaceCapture(state.nativeSurface);
  applyVerifiedCarrierRows(state.visibleCarrierRows);
  rehydrateScope = state.friendLabel;
  decryptDisplayEnabled = state.decryptDisplayEnabled;
  decryptDisplay.checked = state.decryptDisplayEnabled;
  applyDecryptDisplayVisibility(decryptDisplayEnabled);
  refreshControls();
  if (decryptDisplayEnabled) {
    scheduleReceivePoll(0);
    // RE-MEASURE EDGE. Everything that reaches this function -- a native-surface
    // change, a resize, a DPI step, a theme switch, the fonts settling -- has
    // just invalidated every rectangle OSL was painting against, so the rows it
    // is painting have to be located again before they mean anything.
    scheduleTranscriptRehydrate();
  } else {
    if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
    receiveTimer = undefined;
  }
}

ttl.addEventListener("change", () => void saveSecurity());
decryptDisplay.addEventListener("change", () => void saveSecurity());

// Readiness must be self-healing. `overlayReady` can only be set by a successful
// `initializeOverlay`, and the backend announces a verified session once, at its
// phase transition. A retained WebView that is not listening at that instant --
// or that discarded an ended session afterwards -- would otherwise sit refusing
// every Enter forever while the composer is on screen and the session is ready.
// Exactly one verification poll is therefore pending whenever this renderer is
// not ready, and none once it is.
function scheduleOverlayInit(): void {
  if (overlayInitTimer !== undefined) window.clearTimeout(overlayInitTimer);
  overlayInitTimer = window.setTimeout(() => void initializeOverlay(), overlayInitRetryMs);
}

function cancelOverlayInit(): void {
  if (overlayInitTimer !== undefined) window.clearTimeout(overlayInitTimer);
  overlayInitTimer = undefined;
}

async function initializeOverlay(): Promise<void> {
  try {
    const qaDiagnostic = import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"
      ? await getNativeDiscordOverlayQaDiagnostic()
      : null;
    const state = qaDiagnostic?.state ?? (qaDiagnostic === null ? await getNativeDiscordOverlayState() : null);
    if (!state) {
      status.textContent = qaDiagnostic?.rejection
        ? `QA backend rejection · ${qaDiagnostic.rejection.command} · ${qaDiagnostic.rejection.message}`
        : "Verifying protected Discord…";
      // Not readable: whatever session held the caret is not this one.
      caretGrantedForEngagement = false;
      scheduleOverlayInit();
      overlayInitRetryMs = Math.min(overlayInitRetryMs * 2, 1_000);
      return;
    }
    overlayInitRetryMs = 250;
    // Verified: nothing left to poll for until this session ends.
    cancelOverlayInit();
    applyLockEngaged(state.lockEngaged ?? true);
    applyDiscordVisualRecipe(state.visualRecipe);
    applyNativeSurfaceCapture(state.nativeSurface);
    applyVerifiedCarrierRows(state.visibleCarrierRows);
    friendLabel.textContent = state.friendLabel;
    // The conversation the renderer believes it is showing. A change here is a
    // SCOPE-CHANGE edge for the backend: a different conversation means every
    // row OSL was painting belongs to a chat that is no longer on screen, so
    // that read is never floored.
    const scopeChanged = rehydrateScope !== state.friendLabel;
    rehydrateScope = state.friendLabel;
    const exactDiscordPlaceholder = discordComposerPlaceholder(state.friendLabel);
    draft.setAttribute("aria-label", exactDiscordPlaceholder);
    // The bounded native capture already contains Discord's exact localized
    // conversation placeholder. A single invisible placeholder keeps
    // :placeholder-shown active so that exact captured text remains visible
    // until the protected draft has content.
    draft.placeholder = state.nativeSurface ? " " : exactDiscordPlaceholder;
    verifiedFriendIdentity = {
      id: "verified-friend",
      displayName: state.friendLabel,
      avatarFallback: Array.from(state.friendLabel.trim())[0]?.toUpperCase() || "?",
      provenance: "verified-osl",
    };
    ttl.value = String(state.ttlSeconds);
    confirmedTtlSeconds = state.ttlSeconds;
    currentExpiry.textContent = `Current: ${expiryLabel(state.ttlSeconds)}`;
    overlayReady = true;
    decryptDisplayEnabled = state.decryptDisplayEnabled;
    decryptDisplay.checked = state.decryptDisplayEnabled;
    applyDecryptDisplayVisibility(decryptDisplayEnabled);
    viewOnceEnabled = state.viewOnceEnabled;
    attachmentsEnabled = state.attachmentsEnabled;
    discordMarkerAvailable = state.discordMarkerAvailable;
    coverTextEnabled = state.covertextEnabled;
    coverText.setAttribute("aria-pressed", String(coverTextEnabled));
    setBusy(false);
    status.textContent = !state.discordMarkerAvailable
      ? "Ready for OSL-only messages. Discord marker placement is unavailable."
      : state.decryptDisplayEnabled
        ? "Ready."
        : "Receiving private text is off for this friend.";
    if (decryptDisplayEnabled) {
      scheduleReceivePoll(0);
      // SESSION-READY / SCOPE-CHANGE EDGE. A verified session just became
      // readable, so this is the read that puts the operator's decryptable
      // history back on screen from behind the capture shield. A scope change
      // additionally drops whatever was painted for the previous conversation
      // first, so no row from the old chat survives even for one frame.
      if (scopeChanged) clearDecodedTranscript();
      scheduleTranscriptRehydrate();
    }
    // Last, and only once the session is genuinely readable and every control
    // above has been enabled: put the caret where the operator just asked to
    // type. See `focusEngagedProtectedDraft` for why this is DOM-only.
    focusEngagedProtectedDraft();
  } catch {
    status.textContent = "Verifying protected Discord…";
    // Same as the not-readable exit above: nothing here is a session that has
    // been handed the caret.
    caretGrantedForEngagement = false;
    scheduleOverlayInit();
    overlayInitRetryMs = Math.min(overlayInitRetryMs * 2, 1_000);
  }
}

// Self-healing measurements, in every build.
//
// Everything OSL paints against Discord's own glyphs is a measurement the
// backend took at some earlier instant: the composer draft's family, size,
// weight and line height, the sampled composer fill, and every carrier row's
// rectangle, colours and typography. A Discord zoom step, a Windows DPI change,
// a monitor move, a theme switch or a re-mount invalidates all of it at once,
// and a stale measurement is exactly the visible seam this surface exists to
// avoid -- so nothing here may assume a measurement stays true for the life of
// the session. Re-read the backend's state whenever the environment says it may
// have changed, and drop the painted rows first so that in the window before
// the answer arrives OSL paints nothing rather than painting the wrong thing.
//
// This used to be gated to the QA shell, which is the same defect class as the
// per-row painting being QA-gated: the shipping build had no way back to a
// correct measurement short of a restart.
const NATIVE_SURFACE_HEAL_DELAY_MS = 120;
/**
 * How long a burst may hold the trailing edge off before one heal runs anyway.
 *
 * A trailing-edge coalescer that is re-armed on every event is STARVABLE: while
 * the events keep coming the timer keeps being pushed out and the heal never
 * runs. Dragging OSL is exactly such a burst, and one that also crosses a
 * monitor boundary resizes this window continuously while every measurement it
 * is painting against is already wrong. So the first deferred heal fixes a
 * deadline; past it the armed timer is allowed to fire instead of being cleared
 * again. That bounds the burst to one round trip per this interval rather than
 * per event, and it never touches the composer -- only carrier rows, the
 * decoded transcript, and the backend's own re-proof of the surface.
 */
const NATIVE_SURFACE_HEAL_MAX_DEFER_MS = 600;
let surfaceHealTimer: number | undefined;
let surfaceHealDeferralDeadline = 0;

/**
 * Coalesced re-measure. `resize` fires per frame while a window is dragged and
 * this ends in an IPC round trip, so only the trailing edge of a burst is read;
 * a pending heal is always replaced, never queued behind itself, and a burst
 * that never stops cannot defer it past NATIVE_SURFACE_HEAL_MAX_DEFER_MS.
 *
 * Nothing here reads, logs or transports draft or transcript text: it clears
 * geometry and asks the backend to prove the current surface again.
 */
function scheduleNativeSurfaceHeal(): void {
  const now = Date.now();
  if (surfaceHealTimer === undefined) {
    surfaceHealDeferralDeadline = now + NATIVE_SURFACE_HEAL_MAX_DEFER_MS;
  } else if (now + NATIVE_SURFACE_HEAL_DELAY_MS > surfaceHealDeferralDeadline) {
    // Deadline passed: leave the armed timer alone so this burst produces a
    // heal instead of postponing one forever.
    return;
  }
  if (surfaceHealTimer !== undefined) window.clearTimeout(surfaceHealTimer);
  surfaceHealTimer = window.setTimeout(() => {
    surfaceHealTimer = undefined;
    // Geometry/DPI/fullscreen changes invalidate every sampled native row
    // immediately. Backend revalidation must prove the exact runtime id,
    // current bounds, pixels, and typography before any paint returns.
    applyVerifiedCarrierRows([]);
    // The decoded Discord rows are measured against the same invalidated
    // geometry, so they come down with the carrier rows rather than sitting a
    // few pixels off Discord's own until the answer arrives.
    clearDecodedTranscript();
    void refreshProtectedDisplayVisibility();
  }, NATIVE_SURFACE_HEAL_DELAY_MS);
}

// `resize` is also the edge that makes the FIRST read of a session visible. The
// protected overlay window is sized to Discord's composer until the native guard
// loop has grown it over the rows OSL proved it is painting, so a first read can
// legitimately answer "I know where every row is, and none of them is inside my
// window yet". The window then grows, which resizes this one, which heals, which
// reads again -- and that read lands inside the grown window. No timer is
// involved: the growth is caused by the previous read.
window.addEventListener("resize", scheduleNativeSurfaceHeal);
window.addEventListener("focus", scheduleNativeSurfaceHeal);
document.addEventListener("visibilitychange", () => {
  // Coming back on screen is the cheapest moment to notice that Discord was
  // re-themed or re-zoomed while this window was hidden.
  if (!document.hidden) scheduleNativeSurfaceHeal();
});

// SCROLL EDGE. Discord's message rows move when the transcript scrolls, so every
// rectangle OSL is painting against moves with them.
//
// Passive and capture-phase: this observes, it never preventDefault()s and never
// consumes the gesture, so the wheel still reaches whatever is under the pointer.
// Nothing is hidden, moved or restyled to make this work.
//
// KNOWN GAP, deliberately not worked around here: this only fires where OSL owns
// pixels. A scroll delivered entirely to Discord's own window is invisible to
// this renderer. The native half of it now arrives on
// NATIVE_DISCORD_ROWS_MOVED_EVENT below, which covers every case where Discord's
// window moved, resized or settled; a pure wheel inside Discord moves no window,
// so nothing reports it and nothing here goes looking. See the edge list above
// REHYDRATE_COALESCE_MS.
window.addEventListener("wheel", () => scheduleTranscriptRehydrate(), { capture: true, passive: true });

// A DPI change does not have to resize this window -- moving between monitors
// of different scale factors can leave the CSS pixel box identical while every
// physical measurement the backend published becomes wrong. A resolution media
// query is the one signal WebView2 gives for it, and it only fires while it
// disagrees with the current ratio, so it is re-armed against the new value
// each time rather than left matching a stale one.
function watchDevicePixelRatio(): void {
  const query = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
  const onChange = (): void => {
    query.removeEventListener("change", onChange);
    scheduleNativeSurfaceHeal();
    watchDevicePixelRatio();
  };
  query.addEventListener("change", onChange);
}

watchDevicePixelRatio();

// Discord's measured family is a webfont inside Discord's own renderer, so in
// practice these surfaces render in the bundled Inter Variable -- whose metrics
// are only correct once the webfont has actually loaded. Until then the first
// paint uses a fallback face at the measured size, which is a different set of
// advance widths. One re-application when the font set settles is enough.
void document.fonts.ready.then(() => {
  scheduleNativeSurfaceHeal();
});

void listen<boolean>(PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT, ({ payload }) => {
  if (discordQaShell && typeof payload === "boolean") {
    decryptDisplayEnabled = payload;
    decryptDisplay.checked = payload;
    applyDecryptDisplayVisibility(payload);
    if (payload) {
      idlePollMs = 2_000;
      scheduleReceivePoll(0);
      // EYE-ON EDGE, announced rather than clicked.
      scheduleTranscriptRehydrate();
    } else {
      if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
      receiveTimer = undefined;
    }
    refreshControls();
    return;
  }
  void refreshProtectedDisplayVisibility();
});
void listen<void>(NATIVE_SURFACE_CHANGED_EVENT, () => {
  void refreshProtectedDisplayVisibility();
});
void listen<void>(OVERLAY_REFOCUS_EVENT, () => {
  draft.focus({ preventScroll: true });
});
// NATIVE SCROLL/GEOMETRY EDGE. The completion of the wheel listener above: the
// guard loop watching Discord's window raises this on the tick the transcript
// band actually changed shape or came to rest, which is the only way this
// renderer can learn that a different process moved the rows it is painting over.
//
// It goes through scheduleTranscriptRehydrate() and nothing else, deliberately.
// That is the established bound for this read and it is already three deep --
// the 800 ms trailing coalescer here, the backend's own 750 ms floor behind it
// (a refused edge is replaced exactly once, never chained), and runTranscript-
// Rehydrate()'s refusal to start a second read while one is in flight. An
// unpaced re-read is not a small cost: it is a bounded-but-real accessibility
// walk across Discord's tree, and one per tick is what froze this app for
// 19,207 ms. Adding a second scheduler in front of a native edge that is itself
// emitted on edges only would pace nothing and hide the bound that matters.
//
// The scheduler also refuses outright with no session or the eye down, so an
// event that arrives with nothing painted costs zero cross-process work.
void listen<void>(NATIVE_DISCORD_ROWS_MOVED_EVENT, () => {
  scheduleTranscriptRehydrate();
});

// NATIVE BAND-SPLIT EDGE. The window went from covering Discord's composer to
// covering its transcript rows only, or back. Nothing but the attribute is set:
// the composer's presence on this surface is one CSS rule keyed on this native
// fact, so there is no second copy of the decision to disagree with the window.
void listen<boolean>(NATIVE_DISCORD_COMPOSER_BAND_SURRENDERED_EVENT, ({ payload }) => {
  // Geometry this renderer cannot check for itself, so an off-contract payload is
  // not evidence either way. Ignoring it leaves the composer drawn, which is the
  // state this renderer has always been in and the one an operator can see.
  if (typeof payload !== "boolean") return;
  document.documentElement.dataset.oslComposerBandSurrendered = String(payload);
  // The composer is no longer on screen, so whatever caret this renderer was
  // given belongs to an engagement it can no longer serve. Same reasoning as the
  // lock coming down in applyLockEngaged(): the next raise is a fresh one and
  // gets its own grant.
  if (payload) caretGrantedForEngagement = false;
});

// The WebView is retained so the lock toggle can show it instantly. Nothing
// from an ended session may survive into the next one, so the draft, every
// rendered plaintext row, the sampled native surface and the ready state are
// all discarded here. Nothing is logged, hashed or persisted on this path.
function discardProtectedSession(): void {
  overlayReady = false;
  setBusy(true);
  draft.value = "";
  draftTooLarge = false;
  counter.textContent = "";
  for (const item of [...transcript.root.querySelectorAll<HTMLLIElement>("[data-row-key]")]) {
    removeBubble(item);
  }
  transcriptRows.length = 0;
  transcriptActions.clear();
  messagePlaintext.clear();
  outgoingBubbles.clear();
  viewOnceBubbles.clear();
  receivedPlaintextBubbles.clear();
  pendingAttachmentIds.clear();
  pendingViewOnceIds.clear();
  verifiedCarrierRows = [];
  // Every decrypted Discord row this session put on screen goes with it, along
  // with any read still pending for it. `messagePlaintext.clear()` above already
  // took their text; these drop the rows themselves and the geometry that placed
  // them, so nothing from the ended conversation can be painted into the next.
  decodedRows.length = 0;
  decodedRowBindings.clear();
  cancelTranscriptRehydrate();
  rehydrateScope = "";
  // This ended session's send/ack ledger must not leak into the next
  // conversation, same as everything else discarded above.
  anyProtectedMessageSent = false;
  anyProtectedMessageAcknowledged = false;
  // The caret grant belongs to the session that just ended. The next engage is
  // a new one and gets its own.
  caretGrantedForEngagement = false;
  syncTranscript();
  applyNativeSurfaceCapture();
  // Every byte of the ended session is gone above. The eye is a stored per-scope
  // policy, not a property of an encryption session, so it is deliberately NOT
  // reset here -- doing that is what made a lock toggle silently close the
  // operator's decrypted display.
  applyDecryptDisplayVisibility(decryptDisplayEnabled);
  if (receiveTimer !== undefined) window.clearTimeout(receiveTimer);
  receiveTimer = undefined;
  status.textContent = "Verifying protected Discord…";
  // Discarding readiness must never leave this renderer with no way back to it.
  // The next session announces itself, but that announcement can be missed by a
  // retained WebView, and a renderer that refuses every keystroke while the
  // composer is on screen is indistinguishable from a broken app.
  overlayInitRetryMs = 250;
  scheduleOverlayInit();
}

void listen<boolean>(OVERLAY_SESSION_EVENT, ({ payload }) => {
  if (payload !== true) {
    discardProtectedSession();
    return;
  }
  // A verified session is ready. Read it now instead of waiting out the
  // retry backoff, so the retained window paints as soon as it is revealed.
  cancelOverlayInit();
  overlayInitRetryMs = 250;
  void initializeOverlay();
});

void initializeOverlay();
