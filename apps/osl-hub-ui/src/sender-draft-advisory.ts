import { englishCatalogue } from "./catalogue/en";

/** A sender-selected local projection of the signed filter bundle. */
export const SENDER_FILTER_SETS = ["off", "basic", "strict"] as const;
export type SenderFilterSet = typeof SENDER_FILTER_SETS[number];

export interface SenderDraftAdvisory {
  readonly draft: string;
  readonly matchedRule: string;
}

interface SenderFilterRule {
  readonly id: string;
  readonly keyword: string;
  readonly set: Exclude<SenderFilterSet, "off">;
}

// These stable identifiers are the UI projection of the signed 6960 bundle.
// This module retains no outcome and has no transport, telemetry, or member
// dependency: it decides only whether this renderer should offer the sender a
// choice before it asks the native side to encrypt.
const SIGNED_BUNDLE_RULES: readonly SenderFilterRule[] = [
  { id: "slur-faggot", keyword: "faggot", set: "basic" },
  { id: "slur-kike", keyword: "kike", set: "basic" },
  { id: "slur-spic", keyword: "spic", set: "basic" },
  { id: "slur-retard", keyword: "retard", set: "basic" },
  { id: "sexual-porn", keyword: "porn", set: "basic" },
  { id: "sexual-nudes", keyword: "nudes", set: "basic" },
  { id: "sexual-onlyfans", keyword: "onlyfans", set: "basic" },
  { id: "profanity-damn", keyword: "damn", set: "strict" },
  { id: "profanity-shit", keyword: "shit", set: "strict" },
  { id: "profanity-fuck", keyword: "fuck", set: "strict" },
];

function ruleIsActive(rule: SenderFilterRule, filterSet: SenderFilterSet): boolean {
  return filterSet === "strict" || filterSet === rule.set;
}

/**
 * Return the first stable rule name that the sender's active local set finds.
 * `off` is deliberately a no-op; it neither changes the draft nor contacts a
 * service. A modified client can skip this helper, which is why its UI copy is
 * deliberately advisory rather than a delivery claim.
 */
export function advisoryForSenderDraft(
  draft: string,
  filterSet: SenderFilterSet,
): SenderDraftAdvisory | null {
  if (filterSet === "off") return null;
  const normalized = draft.toLocaleLowerCase("en-US");
  const rule = SIGNED_BUNDLE_RULES.find((candidate) => (
    ruleIsActive(candidate, filterSet) && normalized.includes(candidate.keyword)
  ));
  return rule ? { draft, matchedRule: rule.id } : null;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character]!);
}

/** The exact shipping advisory rendered over the sender's own composer. */
export function senderDraftAdvisoryMarkup(advisory: SenderDraftAdvisory | null): string {
  if (!advisory) return "";
  return `<dialog class="sender-draft-advisory" id="sender-draft-advisory" open aria-labelledby="sender-draft-advisory-title"><section class="sender-draft-advisory-card"><header><p>Draft advisory</p><h2 id="sender-draft-advisory-title">Your draft matched a local filter</h2></header><p>Matched rule: <code>${escapeHtml(advisory.matchedRule)}</code></p><p>${escapeHtml(englishCatalogue.senderDraftFilterAdvisory)}</p><footer><button class="button" id="sender-draft-edit" type="button">Edit draft</button><button class="button primary" id="sender-draft-send-anyway" type="button">Send anyway</button></footer></section></dialog>`;
}
