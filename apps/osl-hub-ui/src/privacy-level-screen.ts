import "./privacy-level-screen.css";

// TASK 0720: the privacy level setting screen. Three choices - Basic, Balanced,
// Maximum - each listing one short line per real effect of that level.
//
// The effects here are not marketing copy: PRIVACY_LEVEL_RULES below is a
// mirror of `PrivacyLevelRuleSet::for_level` in
// crates/ipc/src/app_preferences.rs, and task_0720_privacy_level_screen.test.ts
// proves the mirror equal to what the real `osl_read_privacy_level_rule_set`
// command returns (driven through the committed task_0720_privacy_level_cli
// example). If the backend rule set changes, that test fails before this screen
// can show a stale effect.

export type PrivacyLevelId = "basic" | "balanced" | "maximum";

export const PRIVACY_LEVEL_IDS: readonly PrivacyLevelId[] = ["basic", "balanced", "maximum"];

export interface PrivacyLevelRules {
  beforeSendWarnings: boolean;
  attachmentCleaning: boolean;
  cleanupReviewDays: number;
  publicPostChecks: boolean;
  vpnRequiredActions: boolean;
  protectedContactsRequired: boolean;
}

export const PRIVACY_LEVEL_LABELS: Record<PrivacyLevelId, string> = {
  basic: "Basic",
  balanced: "Balanced",
  maximum: "Maximum",
};

// One neutral line per level. The judge bar for this screen (task 0721) is
// that nothing here pressures a choice, so these describe, they do not rank.
export const PRIVACY_LEVEL_SUMMARIES: Record<PrivacyLevelId, string> = {
  basic: "The fewest checks. OSL steps in only when you ask.",
  balanced: "Everyday checks before things leave this device.",
  maximum: "Every check on, with the shortest review times.",
};

export const PRIVACY_LEVEL_RULES: Record<PrivacyLevelId, PrivacyLevelRules> = {
  basic: {
    beforeSendWarnings: false,
    attachmentCleaning: false,
    cleanupReviewDays: 0,
    publicPostChecks: false,
    vpnRequiredActions: false,
    protectedContactsRequired: false,
  },
  balanced: {
    beforeSendWarnings: true,
    attachmentCleaning: true,
    cleanupReviewDays: 30,
    publicPostChecks: false,
    vpnRequiredActions: false,
    protectedContactsRequired: false,
  },
  maximum: {
    beforeSendWarnings: true,
    attachmentCleaning: true,
    cleanupReviewDays: 7,
    publicPostChecks: true,
    vpnRequiredActions: true,
    protectedContactsRequired: true,
  },
};

export interface PrivacyLevelEffectLine {
  effect: "warnings" | "public-posts" | "attachments" | "cleanup-review" | "vpn" | "contacts";
  line: string;
}

// One short line per real rule, on or off, so a level never hides an effect by
// omission: all six rules appear on every card.
export function privacyLevelEffectLines(rules: PrivacyLevelRules): PrivacyLevelEffectLine[] {
  return [
    {
      effect: "warnings",
      line: rules.beforeSendWarnings
        ? "Warns you before a risky send."
        : "No warnings before you send.",
    },
    {
      effect: "public-posts",
      line: rules.publicPostChecks
        ? "Checks public posts before they go out."
        : "Public posts are not checked.",
    },
    {
      effect: "attachments",
      line: rules.attachmentCleaning
        ? "Removes hidden details from attachments first."
        : "Attachments are sent as they are.",
    },
    {
      effect: "cleanup-review",
      line: rules.cleanupReviewDays > 0
        ? `Offers a cleanup review every ${rules.cleanupReviewDays} days.`
        : "No cleanup reviews are offered.",
    },
    {
      effect: "vpn",
      line: rules.vpnRequiredActions
        ? "Risky actions wait until your VPN is on."
        : "Nothing waits for a VPN.",
    },
    {
      effect: "contacts",
      line: rules.protectedContactsRequired
        ? "Protected sends need protected contacts."
        : "Protected contacts stay optional.",
    },
  ];
}

function privacyLevelCardMarkup(id: PrivacyLevelId, selected: boolean): string {
  const inputId = `privacy-level-${id}`;
  const effects = privacyLevelEffectLines(PRIVACY_LEVEL_RULES[id])
    .map((entry) => `<li data-privacy-level-effect="${entry.effect}">${entry.line}</li>`)
    .join("");
  return `<article class="privacy-level-card${selected ? " selected" : ""}" data-privacy-level-card="${id}" data-selected="${selected}">
    <div class="privacy-level-head">
      <input type="radio" id="${inputId}" name="privacy-level" value="${id}" data-privacy-level-choice="${id}" aria-describedby="${inputId}-effects" ${selected ? "checked" : ""}/>
      <label for="${inputId}"><strong>${PRIVACY_LEVEL_LABELS[id]}</strong><small>${PRIVACY_LEVEL_SUMMARIES[id]}</small></label>
      ${selected ? '<span class="privacy-level-selected-mark">Selected</span>' : ""}
    </div>
    <ul class="privacy-level-effects" id="${inputId}-effects" aria-label="What ${PRIVACY_LEVEL_LABELS[id]} does">${effects}</ul>
  </article>`;
}

export function renderPrivacyLevelScreen(selected: PrivacyLevelId): string {
  const cards = PRIVACY_LEVEL_IDS.map((id) => privacyLevelCardMarkup(id, id === selected)).join("");
  return `<main class="content-viewport privacy-level-screen" aria-labelledby="route-heading">
    <header class="destination-header">
      <div>
        <p class="eyebrow">Privacy</p>
        <h1 id="route-heading" tabindex="-1">Privacy level</h1>
        <p>Each level lists exactly what it changes. You can change this at any time.</p>
      </div>
      <button class="button compact" id="privacy-level-back" type="button">Back to Privacy</button>
    </header>
    <fieldset class="privacy-level-choices" role="radiogroup" aria-label="Privacy level">${cards}</fieldset>
  </main>`;
}
