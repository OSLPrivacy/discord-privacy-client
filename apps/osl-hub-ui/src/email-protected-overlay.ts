import "./email-protected-overlay.css";

/**
 * TASK 1228 -- the shared protected email overlay: a draft (compose) overlay
 * and a reading overlay, both scoped to OSL's protected email path.
 *
 * The two gates this connects to:
 *
 * - TASK 1220 (`email_protection_check` in apps/osl-hub/src/privacy_scan.rs):
 *   visible recipients (To/Cc) are named, hidden recipients (Bcc) are counted
 *   but never named, and the total is deduplicated. `emailRecipientSummary`
 *   below is that same arithmetic, mirrored in JS so the draft overlay's send
 *   review panel cannot show a recipient count the backend would disagree
 *   with.
 * - TASK 1223 (`apply_email_composer_input` in
 *   crates/ipc/src/email_send_modes.rs): Enter always inserts a line, in
 *   every one of the five send modes -- it is the named Send control, never
 *   Enter, that opens send review. `applyEmailDraftEnter` and
 *   `openEmailSendReview` mirror that split.
 */

export type EmailSendModeId =
  | "manual"
  | "double_enter"
  | "experimental_single_enter"
  | "instant"
  | "match_typing";

export interface EmailSendMode {
  readonly id: EmailSendModeId;
  readonly name: string;
}

/** Exact ids and display names of `EmailSendMode::ALL` in email_send_modes.rs. */
export const EMAIL_SEND_MODES: readonly EmailSendMode[] = [
  { id: "manual", name: "Manual" },
  { id: "double_enter", name: "Double Enter" },
  { id: "experimental_single_enter", name: "Experimental Single Enter" },
  { id: "instant", name: "Instant" },
  { id: "match_typing", name: "Match typing" },
];

export function emailSendModeById(id: EmailSendModeId): EmailSendMode {
  const found = EMAIL_SEND_MODES.find((mode) => mode.id === id);
  if (!found) throw new Error(`OSL: unknown email send mode '${id}'`);
  return found;
}

export type EmailOverlayTtlSeconds = 300 | 3600 | 86400 | 604800;

export const EMAIL_OVERLAY_TTL_OPTIONS: readonly EmailOverlayTtlSeconds[] = [300, 3600, 86400, 604800];

export function formatEmailOverlayTtl(seconds: EmailOverlayTtlSeconds): string {
  switch (seconds) {
    case 300: return "5 minutes";
    case 3600: return "1 hour";
    case 86400: return "1 day";
    case 604800: return "7 days";
    default: {
      const exhaustive: never = seconds;
      throw new Error(`OSL: unknown email overlay TTL '${exhaustive}'`);
    }
  }
}

/** Whole seconds only; a countdown never shows a fractional second. */
export function formatEmailOverlayCountdown(secondsRemaining: number): string {
  const clamped = Math.max(0, Math.floor(secondsRemaining));
  const hours = Math.floor(clamped / 3600);
  const minutes = Math.floor((clamped % 3600) / 60);
  const seconds = clamped % 60;
  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours}h`);
  if (hours > 0 || minutes > 0) parts.push(`${minutes}m`);
  parts.push(`${seconds}s`);
  return parts.join(" ");
}

export interface EmailOverlayAttachment {
  readonly name: string;
  readonly sizeLabel: string;
}

export interface EmailRecipientSummary {
  readonly visibleRecipients: readonly string[];
  readonly hiddenCount: number;
  readonly distinctRecipientCount: number;
}

/**
 * Mirrors `email_protection_check` (TASK 1220): every visible (To/Cc)
 * recipient is named, hidden (Bcc) recipients are counted only, and the
 * total is the deduplicated union of both -- so Bcc still raises the total
 * without ever appearing in `visibleRecipients`.
 */
export function emailRecipientSummary(
  to: readonly string[],
  cc: readonly string[],
  bcc: readonly string[],
): EmailRecipientSummary {
  return {
    visibleRecipients: Array.from(new Set([...to, ...cc])),
    hiddenCount: bcc.length,
    distinctRecipientCount: new Set([...to, ...cc, ...bcc]).size,
  };
}

export interface EmailDraftOverlayState {
  readonly to: readonly string[];
  readonly cc: readonly string[];
  readonly bcc: readonly string[];
  readonly subject: string;
  readonly body: string;
  readonly ttlSeconds: EmailOverlayTtlSeconds;
  readonly sendMode: EmailSendModeId;
  readonly attachments: readonly EmailOverlayAttachment[];
  readonly reviewOpen: boolean;
}

export function initialEmailDraftOverlayState(): EmailDraftOverlayState {
  return {
    to: ["friend@example.osl"],
    cc: [],
    bcc: [],
    subject: "",
    body: "",
    ttlSeconds: 86400,
    sendMode: "manual",
    attachments: [],
    reviewOpen: false,
  };
}

/** Enter never sends. It inserts a line, identically, in every send mode. */
export function applyEmailDraftEnter(state: EmailDraftOverlayState): EmailDraftOverlayState {
  return { ...state, body: `${state.body}\n` };
}

/** Only the named Send control reaches here, and it opens review rather than sending. */
export function openEmailSendReview(state: EmailDraftOverlayState): EmailDraftOverlayState {
  return { ...state, reviewOpen: true };
}

export interface EmailReadingOverlayState {
  readonly sender: string;
  readonly subject: string;
  readonly body: string;
  readonly remainingSeconds: number;
  readonly attachments: readonly EmailOverlayAttachment[];
  readonly opened: boolean;
}

export function initialEmailReadingOverlayState(): EmailReadingOverlayState {
  return {
    sender: "friend@example.osl",
    subject: "Protected update",
    body: "This message only opens inside OSL.",
    remainingSeconds: 3_600,
    attachments: [],
    opened: true,
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function attachmentRow(attachment: EmailOverlayAttachment, keyPrefix: string): string {
  return `<li class="email-overlay-file" data-file-name="${escapeHtml(attachment.name)}">
        <span class="email-overlay-file-name">${escapeHtml(attachment.name)}</span>
        <span class="email-overlay-file-size">${escapeHtml(attachment.sizeLabel)}</span>
      </li>`;
}

function attachmentList(attachments: readonly EmailOverlayAttachment[], keyPrefix: string, empty: string): string {
  if (attachments.length === 0) return `<li class="email-overlay-files-empty">${empty}</li>`;
  return attachments.map((attachment) => attachmentRow(attachment, keyPrefix)).join("\n      ");
}

/**
 * The draft (compose) overlay. `aria-label` and the visible heading are both
 * the literal string "draft overlay" -- the exact name TASK 1228's finish
 * line requires on screen, in the page text and in the accessibility tree.
 */
export function emailDraftOverlayMarkup(state: EmailDraftOverlayState): string {
  const summary = emailRecipientSummary(state.to, state.cc, state.bcc);
  const mode = emailSendModeById(state.sendMode);
  return `<section class="email-draft-overlay" aria-label="draft overlay" data-element="draft-overlay">
  <header class="email-overlay-head">
    <h2 class="email-overlay-title">draft overlay</h2>
    <p class="email-overlay-lead">Protected until you choose Send. Nothing leaves this device before then.</p>
  </header>
  <div class="email-overlay-recipients">
    <span class="email-overlay-recipients-to">To: ${state.to.map(escapeHtml).join(", ") || "(no recipients yet)"}</span>
    ${state.cc.length ? `<span class="email-overlay-recipients-cc">Cc: ${state.cc.map(escapeHtml).join(", ")}</span>` : ""}
    ${state.bcc.length ? `<span class="email-overlay-recipients-bcc" data-hidden-count="${state.bcc.length}">Bcc: ${state.bcc.length} hidden</span>` : ""}
  </div>
  <input class="email-overlay-subject" type="text" value="${escapeHtml(state.subject)}" placeholder="Subject" aria-label="Subject" />
  <textarea class="email-overlay-body" id="email-draft-body" aria-label="Message text" placeholder="Write your protected message">${escapeHtml(state.body)}</textarea>
  <div class="email-overlay-controls">
    <label class="email-overlay-timer">Delete after
      <select class="email-overlay-ttl" id="email-draft-ttl" aria-label="Delete after">
        ${EMAIL_OVERLAY_TTL_OPTIONS.map((ttl) => `<option value="${ttl}"${ttl === state.ttlSeconds ? " selected" : ""}>${formatEmailOverlayTtl(ttl)}</option>`).join("\n        ")}
      </select>
    </label>
    <label class="email-overlay-send-mode">Send on Enter
      <select class="email-overlay-send-mode-select" id="email-draft-send-mode" aria-label="Send on Enter">
        ${EMAIL_SEND_MODES.map((candidate) => `<option value="${candidate.id}"${candidate.id === state.sendMode ? " selected" : ""}>${candidate.name}</option>`).join("\n        ")}
      </select>
    </label>
  </div>
  <ul class="email-overlay-files" aria-label="Attached files">
    ${attachmentList(state.attachments, "draft", "No files attached.")}
    <li class="email-overlay-files-add"><button class="button email-overlay-add-file" type="button" id="email-draft-add-file">Add file</button></li>
  </ul>
  <div class="email-overlay-send-review" data-open="${state.reviewOpen}" aria-label="Send review">
    <h3 class="email-overlay-send-review-title">Review before sending</h3>
    <p class="email-overlay-send-review-recipients">${summary.visibleRecipients.length} visible recipient(s), ${summary.hiddenCount} hidden, ${summary.distinctRecipientCount} total.</p>
    <p class="email-overlay-send-review-mode">Send mode: ${escapeHtml(mode.name)}</p>
    <button class="button primary email-overlay-send" type="button" id="email-draft-send">Send</button>
  </div>
</section>`;
}

/**
 * The reading overlay for an opened protected email. `aria-label` and the
 * visible heading are both the literal string "reading overlay".
 */
export function emailReadingOverlayMarkup(state: EmailReadingOverlayState): string {
  return `<section class="email-reading-overlay" aria-label="reading overlay" data-element="reading-overlay">
  <header class="email-overlay-head">
    <h2 class="email-overlay-title">reading overlay</h2>
    <p class="email-overlay-lead">From ${escapeHtml(state.sender)}</p>
  </header>
  <h3 class="email-overlay-subject-display">${escapeHtml(state.subject)}</h3>
  <p class="email-overlay-body-display" id="email-reading-body">${escapeHtml(state.body)}</p>
  <div class="email-overlay-timer-display" aria-label="Time remaining" data-remaining-seconds="${Math.max(0, Math.floor(state.remainingSeconds))}">
    Deletes in ${formatEmailOverlayCountdown(state.remainingSeconds)}
  </div>
  <ul class="email-overlay-files" aria-label="Attached files">
    ${attachmentList(state.attachments, "reading", "No files attached.")}
  </ul>
</section>`;
}

export interface EmailProtectedOverlayFixtureState {
  readonly draft: EmailDraftOverlayState;
  readonly reading: EmailReadingOverlayState;
}

export function initialEmailProtectedOverlayFixtureState(): EmailProtectedOverlayFixtureState {
  return {
    draft: initialEmailDraftOverlayState(),
    reading: initialEmailReadingOverlayState(),
  };
}

/** Both overlays together, the shape TASK 1228's fixture screen renders. */
export function emailProtectedOverlayMarkup(state: EmailProtectedOverlayFixtureState): string {
  return `<div class="email-overlay-stage">
  ${emailDraftOverlayMarkup(state.draft)}
  ${emailReadingOverlayMarkup(state.reading)}
</div>`;
}
