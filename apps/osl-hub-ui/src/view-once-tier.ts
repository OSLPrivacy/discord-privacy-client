/**
 * TASK 0594 - why the view once control is off on a free account.
 *
 * 0590 made *creating* a view once message Pro-only in native code and 0591
 * kept *opening* one free for everybody, whatever the reader's tier. A control
 * that is merely greyed out states neither half, so every view-once control in
 * this UI is drawn through this module: when it is off because the account is
 * Free it carries both sentences, in plain words, next to the control itself.
 *
 * The audit helpers below exist so "no view-once control is off without an
 * explanation" is a number a test can print, not a claim in a comment.
 */

/** Making one is the Pro half (native refusal: `view_once_messages_require_pro`). */
export const VIEW_ONCE_PRO_CREATE_SENTENCE = "Making one needs Pro.";
/** Opening one is the free half; there is no tier check on the reading path. */
export const VIEW_ONCE_FREE_OPEN_SENTENCE = "Opening one is free, always, on any account.";
/** Both halves, in the order a reader needs them. */
export const VIEW_ONCE_TIER_EXPLANATION = `${VIEW_ONCE_PRO_CREATE_SENTENCE} ${VIEW_ONCE_FREE_OPEN_SENTENCE}`;

/** The two sentences that must appear wherever the control is off for tier. */
export const VIEW_ONCE_TIER_SENTENCES = [
  VIEW_ONCE_PRO_CREATE_SENTENCE,
  VIEW_ONCE_FREE_OPEN_SENTENCE,
] as const;

/**
 * Off for a reason that is not money. Said separately so a transient block is
 * never mistaken for a paywall -- and so no off state is ever left unexplained.
 */
export const VIEW_ONCE_UNAVAILABLE_SENTENCE = "Not while this chat is busy.";

/** What view once does and does not bound, shared by the protected sheets. */
export const VIEW_ONCE_DISPLAY_TRUTH = "Display is bounded on cooperating OSL clients; cameras are outside OSL's control.";

/** The access levels the hub license state reports. */
export type ViewOnceAccess = "free" | "pro" | "offlineGrace";

/** Only an active (or offline-grace) Pro entitlement may create a view once message. */
export function viewOnceCreationAllowed(access: ViewOnceAccess | string): boolean {
  return access === "pro" || access === "offlineGrace";
}

export interface ViewOnceControlState {
  /** The control is not operable. */
  off: boolean;
  /** Why it is off: the tier split, a transient reason, or nothing. */
  reason: "tier" | "unavailable" | "";
  /** The sentences to show beside the control; empty when it is on. */
  explanation: string;
}

/**
 * The single decision every view-once control makes. `creationAllowed` is the
 * tier half; `unavailable` covers busy/not-ready/no-context reasons that are
 * not about money and must not be dressed up as a paywall.
 */
export function viewOnceControlState(input: {
  creationAllowed: boolean;
  unavailable?: boolean;
  unavailableSentence?: string;
}): ViewOnceControlState {
  if (!input.creationAllowed) {
    return { off: true, reason: "tier", explanation: VIEW_ONCE_TIER_EXPLANATION };
  }
  if (input.unavailable) {
    return {
      off: true,
      reason: "unavailable",
      explanation: input.unavailableSentence ?? VIEW_ONCE_UNAVAILABLE_SENTENCE,
    };
  }
  return { off: false, reason: "", explanation: "" };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[character] ?? character));
}

/** `compact` is the protected-sheet row; `composer` is the OSL Chats send bar. */
export type ViewOnceControlLayout = "compact" | "composer";

export interface ViewOnceControlOptions {
  /** The checkbox element id, e.g. `peer-protected-view-once`. */
  id: string;
  layout: ViewOnceControlLayout;
  checked: boolean;
  /** Pro (or offline grace) on this device. */
  creationAllowed: boolean;
  /** Off for a reason that is not the tier split (busy, not ready). */
  unavailable?: boolean;
  /** The surface's own truth line about what view once does here. */
  detail: string;
  className?: string;
  /** Optional inline icon markup for the composer layout. */
  iconMarkup?: string;
  title?: string;
}

/**
 * The only place a view-once checkbox is written. Emits the marker attributes
 * the audit reads, and the two sentences whenever the control is off for tier.
 */
export function viewOnceControlMarkup(options: ViewOnceControlOptions): string {
  const state = viewOnceControlState(options);
  const note = state.explanation
    ? `<small class="view-once-tier-note" data-osl-view-once-tier-note="1">${escapeHtml(state.explanation)}</small>`
    : "";
  const input = `<input id="${escapeHtml(options.id)}" type="checkbox" ${options.checked ? "checked" : ""} ${state.off ? "disabled" : ""}/>`;
  const detail = `<small>${escapeHtml(options.detail)}</small>`;
  // The note is always a direct child of the label: the composer's <span> is
  // visually clipped for screen readers, so a reason nested there would be a
  // reason nobody sighted ever reads.
  const inner = options.layout === "composer"
    ? `${input}${options.iconMarkup ?? ""}<span><strong>View once</strong>${detail}</span>${note}`
    : `<span>View once</span>${input}${detail}${note}`;
  const attributes = [
    options.className ? `class="${escapeHtml(options.className)}"` : "",
    options.title ? `title="${escapeHtml(options.title)}"` : "",
    `data-osl-view-once-control="${escapeHtml(options.id)}"`,
    `data-osl-view-once-state="${state.off ? "off" : "on"}"`,
    `data-osl-view-once-off-reason="${state.reason}"`,
  ].filter(Boolean).join(" ");
  return `<label ${attributes}>${inner}</label>`;
}

export interface ViewOnceControlAuditEntry {
  /** The checkbox id declared by `data-osl-view-once-control`. */
  id: string;
  /** The control is drawn off. */
  off: boolean;
  /** Why the markup says it is off. */
  reason: string;
  /** A reason the reader can see is present, and it matches why it is off. */
  explained: boolean;
}

const CONTROL_TAG = /<label\b[^>]*data-osl-view-once-control="([^"]+)"[^>]*>/giu;
const VIEW_ONCE_CHECKBOX = /<input\b[^>]*id="([^"]*view-once[^"]*)"[^>]*>/giu;

function attribute(tag: string, name: string): string {
  const match = new RegExp(`${name}="([^"]*)"`, "iu").exec(tag);
  return match?.[1] ?? "";
}

/** Every view-once control found in a rendered surface, with its drawn state. */
export function viewOnceControlAudit(html: string): ViewOnceControlAuditEntry[] {
  const entries: ViewOnceControlAuditEntry[] = [];
  for (const match of html.matchAll(CONTROL_TAG)) {
    const start = (match.index ?? 0) + match[0].length;
    const end = html.indexOf("</label>", start);
    const segment = html.slice(start, end === -1 ? html.length : end);
    const checkbox = new RegExp(`<input\\b[^>]*id="${match[1]}"[^>]*>`, "iu").exec(segment)?.[0] ?? "";
    const reason = attribute(match[0], "data-osl-view-once-off-reason");
    const note = /data-osl-view-once-tier-note="1"[^>]*>([^<]*)</iu.exec(segment)?.[1]?.trim() ?? "";
    entries.push({
      id: match[1],
      off: /\bdisabled\b/iu.test(checkbox),
      reason,
      explained: reason === "tier"
        ? VIEW_ONCE_TIER_SENTENCES.every((sentence) => note.includes(sentence))
        : note.length > 0,
    });
  }
  return entries;
}

/**
 * The number the finish line asks for: the ids of every view-once control that
 * a reader would see switched off with nothing telling them why. A checkbox
 * that carries no `data-osl-view-once-control` wrapper at all counts too --
 * that is exactly how an unexplained control gets added by accident.
 */
export function viewOnceControlsOffWithoutExplanation(html: string): string[] {
  const audited = viewOnceControlAudit(html);
  const unexplained = audited.filter((entry) => entry.off && !entry.explained).map((entry) => entry.id);
  const known = new Set(audited.map((entry) => entry.id));
  for (const match of html.matchAll(VIEW_ONCE_CHECKBOX)) {
    if (known.has(match[1])) continue;
    if (/\bdisabled\b/iu.test(match[0])) unexplained.push(match[1]);
  }
  return unexplained;
}
