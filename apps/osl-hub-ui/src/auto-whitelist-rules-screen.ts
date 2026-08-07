/**
 * The auto-whitelist rules screen: one rule per place kind.
 *
 * A rule says what OSL does the first time a place of that kind shows up:
 * leave it out, ask, add it, or add it only when the other side is already a
 * friend. The four choices and the place kinds themselves are the renderer-side
 * mirror of `crates/ipc/src/auto_whitelist_rules.rs` -- `AutoWhitelistChoice`
 * for the choices, the per-app `*WhitelistKind` enums plus `X_PLACE_KINDS` and
 * the two email kinds for the rows. `auto-whitelist-rules-screen.test.ts` reads
 * that Rust file and fails if this catalogue drifts from it, so "every place
 * kind" is a measured claim rather than a hand-kept list.
 *
 * The module holds no state of its own: a caller passes the state in and gets
 * markup back. `attachAutoWhitelistRulesScreen` is the thin controller that
 * keeps one state value for a mounted screen and redraws it after a change,
 * Save or Reset.
 */

/** Renderer-side mirror of Rust `AutoWhitelistChoice`. */
export type AutoWhitelistChoiceId = "never" | "ask_me" | "always" | "only_if_a_friend";

export interface AutoWhitelistChoice {
  id: AutoWhitelistChoiceId;
  /** Rust `AutoWhitelistChoice::label`. */
  label: string;
  /** The short explanation the screen shows under the choice name. */
  explanation: string;
}

/** Rust `AutoWhitelistChoice::ALL`, in that order. */
export const AUTO_WHITELIST_CHOICES: readonly AutoWhitelistChoice[] = [
  {
    id: "never",
    label: "never",
    explanation: "The place stays out. Nothing is added without you.",
  },
  {
    id: "ask_me",
    label: "ask me",
    explanation: "OSL asks you first and waits for your answer.",
  },
  {
    id: "always",
    label: "always",
    explanation: "The place is added the moment it turns up.",
  },
  {
    id: "only_if_a_friend",
    label: "only if a friend",
    explanation: "Added only when the other side is already a friend.",
  },
] as const;

/** Rust `AutoWhitelistChoice::default()`, and what Reset puts every row back to. */
export const DEFAULT_AUTO_WHITELIST_CHOICE: AutoWhitelistChoiceId = "never";

/** One kind of place a rule can be set for. */
export interface AutoWhitelistPlaceKind {
  /** App id, lowercase, as the native side spells it. */
  app: string;
  /** App name as the screen shows it. */
  appLabel: string;
  /** Place kind id, Rust `<App>WhitelistKind::id`. */
  kind: string;
  /** Place kind name, Rust `<App>WhitelistKind::name`. */
  kindLabel: string;
  /** The key a saved rule is stored under, Rust `*_auto_whitelist_rule_key`. */
  ruleKey: string;
}

/** The place kinds of one app, drawn as one block. */
export interface AutoWhitelistPlaceGroup {
  app: string;
  appLabel: string;
  /** Which of the screen's two columns this block sits in. */
  column: number;
  kinds: readonly AutoWhitelistPlaceKind[];
}

/**
 * A rule key is not the same shape for every app. Chat apps that carry a kind
 * enum use `app:kind` (Rust `discord_auto_whitelist_rule_key` and friends), X
 * uses the scoped `app/kind` form from `scoped_rule_key`, and the two email
 * kinds are stored under their bare kind id by
 * `auto_whitelist_app_kind_for_place`.
 */
type RuleKeyShape = "colon" | "slash" | "bare";

interface GroupSpec {
  app: string;
  appLabel: string;
  column: number;
  ruleKeyShape: RuleKeyShape;
  kinds: readonly (readonly [kind: string, label: string])[];
}

const GROUP_SPECS: readonly GroupSpec[] = [
  {
    app: "discord",
    appLabel: "Discord",
    column: 1,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["group_dm", "group DM"],
      ["server", "server"],
      ["server_channel", "server channel"],
      ["channel", "channel"],
      ["thread", "thread"],
    ],
  },
  {
    app: "whatsapp",
    appLabel: "WhatsApp",
    column: 1,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["channel", "channel"],
      ["community", "community"],
      ["community_group", "community group"],
      ["broadcast_list", "broadcast list"],
    ],
  },
  {
    app: "telegram",
    appLabel: "Telegram",
    column: 1,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["channel", "channel"],
      ["public_post", "public post"],
      ["supergroup", "supergroup"],
      ["saved_messages", "saved messages"],
    ],
  },
  {
    app: "signal",
    appLabel: "Signal",
    column: 2,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["group", "group"],
      ["note_to_self", "note to self"],
      ["story", "story"],
    ],
  },
  {
    app: "instagram",
    appLabel: "Instagram",
    column: 2,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["public_post", "public post"],
      ["comment", "comment"],
      ["story", "story"],
    ],
  },
  {
    app: "messenger",
    appLabel: "Messenger",
    column: 2,
    ruleKeyShape: "colon",
    kinds: [
      ["direct_message", "direct message"],
      ["group_chat", "group chat"],
      ["community", "community"],
    ],
  },
  {
    app: "x",
    appLabel: "X",
    column: 2,
    ruleKeyShape: "slash",
    kinds: [
      ["direct_message", "direct message"],
      ["group_direct_message", "group direct message"],
      ["public_post", "public post"],
      ["reply", "reply"],
    ],
  },
  {
    app: "email",
    appLabel: "Email",
    column: 2,
    ruleKeyShape: "bare",
    kinds: [
      ["email_address", "email address"],
      ["email_domain", "email domain"],
    ],
  },
] as const;

function ruleKeyFor(spec: GroupSpec, kind: string): string {
  if (spec.ruleKeyShape === "slash") return `${spec.app}/${kind}`;
  if (spec.ruleKeyShape === "bare") return kind;
  return `${spec.app}:${kind}`;
}

/** Every place kind, grouped by app, in screen order. */
export const AUTO_WHITELIST_PLACE_GROUPS: readonly AutoWhitelistPlaceGroup[] = GROUP_SPECS.map(
  (spec) => ({
    app: spec.app,
    appLabel: spec.appLabel,
    column: spec.column,
    kinds: spec.kinds.map(([kind, kindLabel]) => ({
      app: spec.app,
      appLabel: spec.appLabel,
      kind,
      kindLabel,
      ruleKey: ruleKeyFor(spec, kind),
    })),
  }),
);

/** Every place kind, flat, in screen order. */
export const AUTO_WHITELIST_PLACE_KINDS: readonly AutoWhitelistPlaceKind[] =
  AUTO_WHITELIST_PLACE_GROUPS.flatMap((group) => group.kinds);

const RULE_KEYS = new Set(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.ruleKey));
const CHOICE_IDS = new Set<string>(AUTO_WHITELIST_CHOICES.map((choice) => choice.id));

/** A rule as it would be handed to the native side. */
export interface SavedAutoWhitelistRule {
  ruleKey: string;
  app: string;
  kind: string;
  choice: AutoWhitelistChoiceId;
}

export type AutoWhitelistSelection = Readonly<Record<string, AutoWhitelistChoiceId>>;

/**
 * What the screen is showing: the rules already kept, and the draft the user is
 * editing. They start equal and part company on the first change; Save makes
 * the draft the kept set, Reset puts the draft back to the defaults.
 */
export interface AutoWhitelistRulesState {
  readonly saved: AutoWhitelistSelection;
  readonly draft: AutoWhitelistSelection;
}

function completeSelection(partial: Readonly<Record<string, string>>): AutoWhitelistSelection {
  for (const ruleKey of Object.keys(partial)) {
    if (!RULE_KEYS.has(ruleKey)) {
      throw new Error(`Unknown place rule key: ${ruleKey}`);
    }
    if (!CHOICE_IDS.has(partial[ruleKey])) {
      throw new Error(`Unknown rule choice for ${ruleKey}: ${partial[ruleKey]}`);
    }
  }
  const selection: Record<string, AutoWhitelistChoiceId> = {};
  for (const place of AUTO_WHITELIST_PLACE_KINDS) {
    selection[place.ruleKey] =
      (partial[place.ruleKey] as AutoWhitelistChoiceId | undefined) ?? DEFAULT_AUTO_WHITELIST_CHOICE;
  }
  return selection;
}

/** A state whose kept rules are the ones handed in and whose draft matches. */
export function autoWhitelistRulesState(
  saved: Readonly<Record<string, string>> = {},
): AutoWhitelistRulesState {
  const selection = completeSelection(saved);
  return { saved: selection, draft: selection };
}

/** The draft with one place kind changed. Every other row is untouched. */
export function setPlaceRule(
  state: AutoWhitelistRulesState,
  ruleKey: string,
  choice: string,
): AutoWhitelistRulesState {
  if (!RULE_KEYS.has(ruleKey)) throw new Error(`Unknown place rule key: ${ruleKey}`);
  if (!CHOICE_IDS.has(choice)) throw new Error(`Unknown rule choice for ${ruleKey}: ${choice}`);
  return { saved: state.saved, draft: { ...state.draft, [ruleKey]: choice as AutoWhitelistChoiceId } };
}

/** Rule keys whose draft choice is not the kept one, in screen order. */
export function changedRuleKeys(state: AutoWhitelistRulesState): string[] {
  return AUTO_WHITELIST_PLACE_KINDS.filter(
    (place) => state.draft[place.ruleKey] !== state.saved[place.ruleKey],
  ).map((place) => place.ruleKey);
}

/** Every rule the screen would hand to the native side, in screen order. */
export function autoWhitelistRulePayload(
  selection: AutoWhitelistSelection,
): SavedAutoWhitelistRule[] {
  return AUTO_WHITELIST_PLACE_KINDS.map((place) => ({
    ruleKey: place.ruleKey,
    app: place.app,
    kind: place.kind,
    choice: selection[place.ruleKey],
  }));
}

/** Save rules: the draft becomes the kept set, and is what gets written. */
export function saveRules(state: AutoWhitelistRulesState): {
  state: AutoWhitelistRulesState;
  saved: SavedAutoWhitelistRule[];
} {
  return {
    state: { saved: state.draft, draft: state.draft },
    saved: autoWhitelistRulePayload(state.draft),
  };
}

/**
 * Reset: every place kind back to `never`, the Rust default. It changes the
 * draft only -- nothing is kept until Save, so a mis-click costs nothing.
 */
export function resetRules(state: AutoWhitelistRulesState): AutoWhitelistRulesState {
  return { saved: state.saved, draft: completeSelection({}) };
}

/** One drawn row. */
export interface AutoWhitelistRuleRow extends AutoWhitelistPlaceKind {
  choice: AutoWhitelistChoiceId;
  choiceLabel: string;
  changed: boolean;
}

export function placeRuleRows(state: AutoWhitelistRulesState): AutoWhitelistRuleRow[] {
  return AUTO_WHITELIST_PLACE_KINDS.map((place) => {
    const choice = state.draft[place.ruleKey];
    return {
      ...place,
      choice,
      choiceLabel: choiceLabel(choice),
      changed: choice !== state.saved[place.ruleKey],
    };
  });
}

export function choiceLabel(choice: AutoWhitelistChoiceId): string {
  const found = AUTO_WHITELIST_CHOICES.find((candidate) => candidate.id === choice);
  if (!found) throw new Error(`Unknown rule choice: ${choice}`);
  return found.label;
}

/** The line under the buttons, saying whether anything is waiting to be kept. */
export function ruleStatusLine(state: AutoWhitelistRulesState): string {
  const changed = changedRuleKeys(state).length;
  if (changed === 0) return `All ${AUTO_WHITELIST_PLACE_KINDS.length} place rules saved.`;
  const noun = changed === 1 ? "place rule" : "place rules";
  return `${changed} ${noun} changed - not saved yet.`;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function choiceMarkup(row: AutoWhitelistRuleRow, choice: AutoWhitelistChoice): string {
  const on = row.choice === choice.id;
  const name = `rule-${row.ruleKey}`;
  return [
    `<label class="rule-choice" data-choice="${choice.id}" data-state="${on ? "on" : "off"}">`,
    `<input type="radio" class="rule-choice-input" name="${escapeHtml(name)}" value="${choice.id}"`,
    ` data-rule-key="${escapeHtml(row.ruleKey)}"${on ? " checked" : ""} />`,
    `<span class="rule-choice-marker" aria-hidden="true"></span>`,
    `<span class="rule-choice-label">${escapeHtml(choice.label)}</span>`,
    `</label>`,
  ].join("");
}

function rowMarkup(row: AutoWhitelistRuleRow): string {
  const group = `${row.appLabel} ${row.kindLabel}`;
  return [
    `<li class="rule-row" data-rule-key="${escapeHtml(row.ruleKey)}" data-app="${escapeHtml(row.app)}"`,
    ` data-kind="${escapeHtml(row.kind)}" data-selected="${row.choice}"`,
    ` data-changed="${row.changed ? "yes" : "no"}">`,
    `<span class="rule-row-name">${escapeHtml(row.kindLabel)}</span>`,
    `<span class="rule-row-choices" role="radiogroup" aria-label="${escapeHtml(group)} rule">`,
    AUTO_WHITELIST_CHOICES.map((choice) => choiceMarkup(row, choice)).join(""),
    `</span>`,
    `<span class="rule-row-selected" aria-label="${escapeHtml(group)} is set to ${escapeHtml(row.choiceLabel)}">`,
    escapeHtml(row.choiceLabel),
    `</span>`,
    `</li>`,
  ].join("");
}

function groupMarkup(group: AutoWhitelistPlaceGroup, rows: AutoWhitelistRuleRow[]): string {
  const mine = rows.filter((row) => row.app === group.app);
  const count = mine.length === 1 ? "1 place kind" : `${mine.length} place kinds`;
  return [
    `<section class="rule-group" data-app="${escapeHtml(group.app)}" aria-label="${escapeHtml(group.appLabel)} place rules">`,
    `<h3 class="rule-group-heading">${escapeHtml(group.appLabel)}<span class="rule-group-count">${escapeHtml(count)}</span></h3>`,
    `<ul class="rule-group-rows">${mine.map(rowMarkup).join("")}</ul>`,
    `</section>`,
  ].join("");
}

function legendMarkup(): string {
  return [
    `<section class="rule-legend" aria-label="What each choice does">`,
    AUTO_WHITELIST_CHOICES.map((choice) =>
      [
        `<div class="rule-legend-item" data-choice="${choice.id}">`,
        `<span class="rule-legend-name">${escapeHtml(choice.label)}</span>`,
        `<span class="rule-legend-text">${escapeHtml(choice.explanation)}</span>`,
        `</div>`,
      ].join(""),
    ).join(""),
    `</section>`,
  ].join("");
}

/** Buttons carry their own one-line explanation, so neither is a guess. */
export const SAVE_RULES_EXPLANATION = "Keeps these rules for every new place from now on.";
export const RESET_RULES_EXPLANATION = "Puts every place kind back to never. Nothing is kept until you save.";

function actionsMarkup(state: AutoWhitelistRulesState): string {
  return [
    `<section class="rule-actions" aria-label="Save or reset the place rules">`,
    `<div class="rule-action">`,
    `<button type="button" class="rule-action-button" data-rule-action="save">Save rules</button>`,
    `<p class="rule-action-text">${escapeHtml(SAVE_RULES_EXPLANATION)}</p>`,
    `</div>`,
    `<div class="rule-action">`,
    `<button type="button" class="rule-action-button rule-action-button-quiet" data-rule-action="reset">Reset</button>`,
    `<p class="rule-action-text">${escapeHtml(RESET_RULES_EXPLANATION)}</p>`,
    `</div>`,
    `<p class="rule-status" role="status" data-changed="${changedRuleKeys(state).length}">${escapeHtml(ruleStatusLine(state))}</p>`,
    `</section>`,
  ].join("");
}

const COLUMNS = [1, 2];

/** The whole screen. Styling lives in `auto-whitelist-rules-screen.css`. */
export function renderAutoWhitelistRulesScreen(state: AutoWhitelistRulesState): string {
  const rows = placeRuleRows(state);
  const columns = COLUMNS.map((column) =>
    [
      `<div class="rule-column" data-column="${column}">`,
      AUTO_WHITELIST_PLACE_GROUPS.filter((group) => group.column === column)
        .map((group) => groupMarkup(group, rows))
        .join(""),
      `</div>`,
    ].join(""),
  ).join("");
  return [
    `<section class="rules-screen" aria-label="Auto-whitelist rules">`,
    `<header class="rules-screen-header">`,
    `<h2 class="rules-screen-heading">Auto-whitelist rules</h2>`,
    `<p class="rules-screen-intro">What OSL does the first time a new place of each kind turns up. ${escapeHtml(String(rows.length))} place kinds, one rule each.</p>`,
    `</header>`,
    legendMarkup(),
    `<div class="rule-columns">${columns}</div>`,
    actionsMarkup(state),
    `</section>`,
  ].join("");
}

/**
 * Mount the screen on an element and keep its state. Every change, Save and
 * Reset redraws from the state, so what a row's marker shows and what the
 * screen would save are the same value, not two that can drift.
 */
export function attachAutoWhitelistRulesScreen(
  mount: HTMLElement,
  saved: Readonly<Record<string, string>> = {},
  onSave: (rules: SavedAutoWhitelistRule[]) => void = () => {},
): void {
  let state = autoWhitelistRulesState(saved);
  const draw = (): void => {
    mount.innerHTML = renderAutoWhitelistRulesScreen(state);
  };
  mount.addEventListener("change", (event) => {
    const input = event.target as HTMLInputElement | null;
    if (!input || !input.classList.contains("rule-choice-input")) return;
    const ruleKey = input.dataset.ruleKey;
    if (!ruleKey) return;
    state = setPlaceRule(state, ruleKey, input.value);
    draw();
  });
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest?.("[data-rule-action]") as HTMLElement | null;
    if (!button) return;
    if (button.dataset.ruleAction === "save") {
      const result = saveRules(state);
      state = result.state;
      onSave(result.saved);
      draw();
      return;
    }
    if (button.dataset.ruleAction === "reset") {
      state = resetRules(state);
      draw();
    }
  });
  draw();
}
