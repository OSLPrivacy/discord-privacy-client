/**
 * The Account screen: seven controls, and a Reset only where putting the
 * control back costs nothing.
 *
 * The seven are identity, password, recovery, lock, stealth, burn password and
 * Pro code. Five of them stand for a secret -- the sign-in password, the
 * recovery phrase, the stealth and burn passwords, and the Pro activation code
 * -- and the rule this module is built around is that the screen never gets to
 * see any of them. `accountSecretFacts` is the only door: it takes the secrets
 * and hands back set/not-set and a word count, and everything that draws takes
 * that view instead. There is no code path from a secret string to markup,
 * which is why the screenshot can be searched for the fixture password,
 * recovery phrase and Pro code and come back empty. The masked slots draw
 * `ACCOUNT_MASK` -- a fixed run of bullets, the same length whatever the secret
 * is, so even the length does not leak.
 *
 * "Reset where safe" is a per-control rule, not a judgement call at the button:
 * `ACCOUNT_RESET_RULES` says for each control either what Reset puts it back to
 * or why there is no Reset, `resetAccountControl` throws rather than reset an
 * unsafe one, and the screen draws the reason where the button would have been.
 * Four are safe -- identity, lock, stealth, burn password -- because a handle,
 * a lock delay, an empty stealth workspace and an unset burn trigger can all be
 * had back by typing them again. Three are not: OSL cannot re-derive a
 * password, a fresh recovery kit makes the words already written down useless,
 * and a Pro code it cannot show cannot be put back.
 *
 * The module holds no state of its own. `attachAccountScreen` is the thin
 * controller that keeps one state value for a mounted screen and redraws after
 * a change, a Reset or a Save.
 */

export type AccountControlId =
  | "identity"
  | "password"
  | "recovery"
  | "lock"
  | "stealth"
  | "burn-password"
  | "pro-code";

/** The seven controls, in the order the screen draws them. */
export const ACCOUNT_CONTROL_IDS: readonly AccountControlId[] = [
  "identity",
  "password",
  "recovery",
  "lock",
  "stealth",
  "burn-password",
  "pro-code",
] as const;

/** The name each control is known by, on the screen and in the checks. */
export const ACCOUNT_CONTROL_LABELS: Readonly<Record<AccountControlId, string>> = {
  identity: "Identity",
  password: "Password",
  recovery: "Recovery",
  lock: "Lock",
  stealth: "Stealth",
  "burn-password": "Burn password",
  "pro-code": "Pro code",
};

/**
 * What a masked slot draws.
 *
 * Fixed length on purpose: a mask as long as the secret would put the length of
 * the password on screen, and a screenshot is a thing people send to support.
 */
export const ACCOUNT_MASK_LENGTH = 12;
export const ACCOUNT_MASK = "•".repeat(ACCOUNT_MASK_LENGTH);

/** The secrets behind the account. Nothing in here reaches the screen. */
export interface AccountSecrets {
  readonly password: string;
  readonly recoveryPhrase: string;
  readonly proCode: string;
  /** Empty means no stealth password is set. */
  readonly stealthPassword: string;
  /** Empty means no burn password is set. */
  readonly burnPassword: string;
}

/** All the screen is allowed to know about the secrets. */
export interface AccountSecretFacts {
  readonly passwordSet: boolean;
  readonly recoveryWordCount: number;
  readonly proCodeSet: boolean;
  readonly stealthSet: boolean;
  readonly burnSet: boolean;
}

export const ACCOUNT_NO_PASSWORD_ERROR = "An account with no password cannot be drawn.";
export const ACCOUNT_NO_RECOVERY_ERROR = "An account with no recovery phrase cannot be drawn.";
export const ACCOUNT_NO_PRO_CODE_ERROR = "An account with no Pro code cannot be drawn.";

function words(phrase: string): string[] {
  return phrase.trim().split(/\s+/u).filter((word) => word.length > 0);
}

/**
 * Reduce the secrets to the facts about them, and refuse to carry any of the
 * secret text across.
 *
 * The three the screen is measured on must be present: a view built from empty
 * strings would let a capture claim it found no password because there was no
 * password. The last check is the invariant this module exists for, asserted
 * rather than assumed.
 */
export function accountSecretFacts(secrets: AccountSecrets): AccountSecretFacts {
  if (!secrets.password.trim()) throw new Error(ACCOUNT_NO_PASSWORD_ERROR);
  if (words(secrets.recoveryPhrase).length === 0) throw new Error(ACCOUNT_NO_RECOVERY_ERROR);
  if (!secrets.proCode.trim()) throw new Error(ACCOUNT_NO_PRO_CODE_ERROR);
  const facts: AccountSecretFacts = {
    passwordSet: true,
    recoveryWordCount: words(secrets.recoveryPhrase).length,
    proCodeSet: true,
    stealthSet: secrets.stealthPassword.trim().length > 0,
    burnSet: secrets.burnPassword.trim().length > 0,
  };
  const carried = JSON.stringify(facts);
  for (const [name, secret] of Object.entries(secrets)) {
    // Short enough and a secret is a substring of ordinary English: a one-letter
    // password is inside the word `passwordSet`. Below eight characters the
    // check says nothing, so it is not run rather than made to fail.
    if (secret.trim().length >= 8 && carried.includes(secret)) {
      throw new Error(`The account view carried the ${name} across.`);
    }
  }
  return facts;
}

/** The parts of the account that are not secret and can be shown as they are. */
export interface AccountSettings {
  /** The name this account shows to you. Reset puts it back to the handle. */
  readonly displayName: string;
  readonly handle: string;
  readonly lockMinutes: number;
  readonly passwordChanged: string;
  readonly recoverySaved: string;
  readonly proCodeEntered: string;
  readonly proPlan: string;
}

/** Everything one drawing of the screen is made from. No secrets. */
export interface AccountView {
  readonly settings: AccountSettings;
  readonly facts: AccountSecretFacts;
}

export interface AccountScreenState {
  readonly saved: AccountView;
  readonly draft: AccountView;
  /** The latest payment event the buyer still needs to acknowledge. */
  readonly buyerNoticeEvent: AccountBuyerNoticeEvent | null;
}

export type AccountBuyerNoticeEvent = "refund" | "chargeback";

/** Exact buyer-facing copy chosen in TASK 3692. */
export const ACCOUNT_BUYER_NOTICES: Readonly<
  Record<AccountBuyerNoticeEvent, { readonly title: string; readonly notice: string }>
> = {
  refund: {
    title: "Refund notice",
    notice: "OSL does not offer refunds. Your purchase and stored data are unchanged.",
  },
  chargeback: {
    title: "Chargeback notice",
    notice:
      "Your payment was charged back, so Pro has ended. Your messages are unchanged. Pro files remain downloadable for 7 days, then expire.",
  },
};

export const DEFAULT_LOCK_MINUTES = 5;

export interface AccountLockChoice {
  readonly minutes: number;
  readonly label: string;
}

export const ACCOUNT_LOCK_CHOICES: readonly AccountLockChoice[] = [
  { minutes: 1, label: "After 1 minute" },
  { minutes: 5, label: "After 5 minutes" },
  { minutes: 15, label: "After 15 minutes" },
  { minutes: 30, label: "After 30 minutes" },
  { minutes: 60, label: "After 1 hour" },
] as const;

export function lockLabel(minutes: number): string {
  const found = ACCOUNT_LOCK_CHOICES.find((choice) => choice.minutes === minutes);
  if (!found) throw new Error(`Unknown lock delay: ${minutes}`);
  return found.label;
}

/** Either what Reset puts a control back to, or why the control has no Reset. */
export type AccountResetRule =
  | { readonly safe: true; readonly to: string; readonly explanation: string }
  | { readonly safe: false; readonly reason: string };

export const ACCOUNT_RESET_RULES: Readonly<Record<AccountControlId, AccountResetRule>> = {
  identity: {
    safe: true,
    to: "your handle",
    explanation: "Puts the shown name back to your handle. The handle itself does not change.",
  },
  password: {
    safe: false,
    reason:
      "There is no reset. OSL cannot read the password you have, so it cannot put it back, and clearing it would leave this workspace open to whoever opens the lid.",
  },
  recovery: {
    safe: false,
    reason:
      "There is no reset. A new kit makes the words you already wrote down useless, and the old ones cannot be brought back.",
  },
  lock: {
    safe: true,
    to: lockLabel(DEFAULT_LOCK_MINUTES).toLowerCase(),
    explanation: "Puts the lock delay back to five minutes. Nothing else changes.",
  },
  stealth: {
    safe: true,
    to: "off",
    explanation: "Clears the stealth password. The stealth workspace holds no messages, so nothing is lost.",
  },
  "burn-password": {
    safe: true,
    to: "off",
    explanation: "Clears the burn password, so it can never be typed in by accident. It erases nothing.",
  },
  "pro-code": {
    safe: false,
    reason:
      "There is no reset. Removing the code puts this device back to free, and OSL cannot show you the code again to put it back.",
  },
};

/** The controls Reset is offered on, in screen order. */
export const ACCOUNT_SAFE_RESET_IDS: readonly AccountControlId[] = ACCOUNT_CONTROL_IDS.filter(
  (id) => ACCOUNT_RESET_RULES[id].safe,
);

export function unsafeResetError(id: AccountControlId): string {
  const rule = ACCOUNT_RESET_RULES[id];
  if (rule.safe) throw new Error(`${ACCOUNT_CONTROL_LABELS[id]} is safe to reset.`);
  return `Resetting ${ACCOUNT_CONTROL_LABELS[id]} is not safe: ${rule.reason}`;
}

export function accountView(secrets: AccountSecrets, settings: AccountSettings): AccountView {
  lockLabel(settings.lockMinutes);
  return { settings, facts: accountSecretFacts(secrets) };
}

export function accountScreenState(
  secrets: AccountSecrets,
  settings: AccountSettings,
  buyerNoticeEvent: AccountBuyerNoticeEvent | null = null,
): AccountScreenState {
  const view = accountView(secrets, settings);
  return { saved: view, draft: view, buyerNoticeEvent };
}

/** One drawn control. */
export interface AccountControl {
  readonly id: AccountControlId;
  readonly label: string;
  /** What the value slot shows. `ACCOUNT_MASK` wherever a secret stands. */
  readonly value: string;
  /** True when the value slot stands for a secret rather than showing one. */
  readonly secret: boolean;
  /** The line under the name. Never carries a secret either. */
  readonly detail: string;
  /** The button that hands this control's job to the rest of the app. */
  readonly action: string | null;
  readonly reset: AccountResetRule;
  readonly changed: boolean;
}

function readingOf(view: AccountView, id: AccountControlId): string {
  const { settings, facts } = view;
  switch (id) {
    case "identity":
      return settings.displayName;
    case "password":
      return facts.passwordSet ? "set" : "off";
    case "recovery":
      return `${facts.recoveryWordCount} words`;
    case "lock":
      return lockLabel(settings.lockMinutes);
    case "stealth":
      return facts.stealthSet ? "set" : "off";
    case "burn-password":
      return facts.burnSet ? "set" : "off";
    case "pro-code":
      return facts.proCodeSet ? "set" : "off";
  }
}

function valueOf(view: AccountView, id: AccountControlId): { value: string; secret: boolean } {
  const { settings, facts } = view;
  switch (id) {
    case "identity":
      return { value: settings.displayName, secret: false };
    case "lock":
      return { value: lockLabel(settings.lockMinutes), secret: false };
    case "password":
      return { value: ACCOUNT_MASK, secret: true };
    case "recovery":
      return { value: ACCOUNT_MASK, secret: true };
    case "pro-code":
      return { value: ACCOUNT_MASK, secret: true };
    case "stealth":
      return facts.stealthSet ? { value: ACCOUNT_MASK, secret: true } : { value: "Off", secret: false };
    case "burn-password":
      return facts.burnSet ? { value: ACCOUNT_MASK, secret: true } : { value: "Off", secret: false };
  }
}

function detailOf(view: AccountView, id: AccountControlId): string {
  const { settings, facts } = view;
  switch (id) {
    case "identity":
      return `Signed in as ${settings.handle}.`;
    case "password":
      return `Set, and last changed ${settings.passwordChanged}. It is never shown, here or anywhere.`;
    case "recovery":
      return `${facts.recoveryWordCount} words, written down ${settings.recoverySaved}. Shown only on the recovery screen, after your password.`;
    case "lock":
      return "Locks this workspace when you step away from it.";
    case "stealth":
      return facts.stealthSet
        ? "Set. Entered at sign in, it opens an empty workspace."
        : "Off. Sign in opens your own workspace only.";
    case "burn-password":
      return facts.burnSet
        ? "Set. Entered at sign in, it permanently erases OSL data from this device. No recovery."
        : "Off. Nothing typed at sign in can erase this device.";
    case "pro-code":
      return `${settings.proPlan}, entered ${settings.proCodeEntered}. The code is never shown again.`;
  }
}

/** Only the controls whose job belongs to another screen carry a button. */
const ACTIONS: Readonly<Record<AccountControlId, string | null>> = {
  identity: null,
  password: "Change password",
  recovery: "Show recovery kit",
  lock: null,
  stealth: "Change stealth password",
  "burn-password": "Change burn password",
  "pro-code": "Replace Pro code",
};

export function accountControls(state: AccountScreenState): AccountControl[] {
  return ACCOUNT_CONTROL_IDS.map((id) => {
    const { value, secret } = valueOf(state.draft, id);
    return {
      id,
      label: ACCOUNT_CONTROL_LABELS[id],
      value,
      secret,
      detail: detailOf(state.draft, id),
      action: ACTIONS[id],
      reset: ACCOUNT_RESET_RULES[id],
      changed: readingOf(state.draft, id) !== readingOf(state.saved, id),
    };
  });
}

export function setDisplayName(state: AccountScreenState, name: string): AccountScreenState {
  const trimmed = name.trim();
  return {
    ...state,
    draft: {
      ...state.draft,
      settings: {
        ...state.draft.settings,
        displayName: trimmed.length > 0 ? trimmed : state.draft.settings.handle,
      },
    },
  };
}

export function setLockMinutes(state: AccountScreenState, minutes: number): AccountScreenState {
  lockLabel(minutes);
  return {
    ...state,
    draft: { ...state.draft, settings: { ...state.draft.settings, lockMinutes: minutes } },
  };
}

/**
 * Put one control back, and refuse when putting it back would cost something.
 *
 * The refusal is the point: three of the seven have no Reset on the screen, and
 * a caller that goes round the screen and asks anyway gets an error rather than
 * a cleared password.
 */
export function resetAccountControl(
  state: AccountScreenState,
  id: AccountControlId,
): AccountScreenState {
  const rule = ACCOUNT_RESET_RULES[id];
  if (!rule) throw new Error(`Unknown account control: ${id}`);
  if (!rule.safe) throw new Error(unsafeResetError(id));
  const { settings, facts } = state.draft;
  const draft: AccountView = (() => {
    switch (id) {
      case "identity":
        return { settings: { ...settings, displayName: settings.handle }, facts };
      case "lock":
        return { settings: { ...settings, lockMinutes: DEFAULT_LOCK_MINUTES }, facts };
      case "stealth":
        return { settings, facts: { ...facts, stealthSet: false } };
      case "burn-password":
        return { settings, facts: { ...facts, burnSet: false } };
      default:
        throw new Error(unsafeResetError(id));
    }
  })();
  return { ...state, draft };
}

/** Reset every control that is safe to reset, and leave the other three. */
export function resetSafeAccountControls(state: AccountScreenState): AccountScreenState {
  return ACCOUNT_SAFE_RESET_IDS.reduce(
    (current, id) => resetAccountControl(current, id),
    state,
  );
}

/** What the screen would hand to the native side. Facts only, never secrets. */
export interface SavedAccountSetting {
  readonly id: AccountControlId;
  readonly label: string;
  readonly reading: string;
  readonly secret: boolean;
}

export function accountSavePayload(view: AccountView): SavedAccountSetting[] {
  return ACCOUNT_CONTROL_IDS.map((id) => {
    const { secret } = valueOf(view, id);
    return { id, label: ACCOUNT_CONTROL_LABELS[id], reading: readingOf(view, id), secret };
  });
}

export function saveAccount(state: AccountScreenState): {
  state: AccountScreenState;
  saved: SavedAccountSetting[];
} {
  return {
    state: { ...state, saved: state.draft, draft: state.draft },
    saved: accountSavePayload(state.draft),
  };
}

export function changedAccountControls(state: AccountScreenState): AccountControlId[] {
  return ACCOUNT_CONTROL_IDS.filter(
    (id) => readingOf(state.draft, id) !== readingOf(state.saved, id),
  );
}

export function accountStatusLine(state: AccountScreenState): string {
  const changed = changedAccountControls(state).length;
  if (changed === 0) return `All ${ACCOUNT_CONTROL_IDS.length} account controls saved.`;
  const noun = changed === 1 ? "control" : "controls";
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

function valueMarkup(control: AccountControl): string {
  if (control.id === "identity") {
    return [
      `<input class="account-value account-input" type="text" id="account-value-identity"`,
      ` data-account-input="identity" autocomplete="off" spellcheck="false"`,
      ` aria-label="Identity, the name this account shows to you"`,
      ` value="${escapeHtml(control.value)}" />`,
    ].join("");
  }
  if (control.id === "lock") {
    const options = ACCOUNT_LOCK_CHOICES.map(
      (choice) =>
        `<option value="${choice.minutes}"${choice.label === control.value ? " selected" : ""}>${escapeHtml(choice.label)}</option>`,
    ).join("");
    return [
      `<select class="account-value account-select" id="account-value-lock"`,
      ` data-account-select="lock" aria-label="Lock this workspace after">${options}</select>`,
    ].join("");
  }
  if (control.secret) {
    return [
      `<span class="account-value account-value-masked" data-account-masked="${control.id}"`,
      ` aria-label="${escapeHtml(control.label)} is set and is not shown">`,
      `${ACCOUNT_MASK}</span>`,
    ].join("");
  }
  return `<span class="account-value account-value-plain">${escapeHtml(control.value)}</span>`;
}

function resetMarkup(control: AccountControl): string {
  if (control.reset.safe) {
    return [
      `<span class="account-reset" data-account-reset="safe">`,
      `<button type="button" class="account-reset-button" data-account-action="reset"`,
      ` data-account-control="${escapeHtml(control.id)}"`,
      ` aria-label="Reset ${escapeHtml(control.label)} to ${escapeHtml(control.reset.to)}">Reset</button>`,
      `<span class="account-reset-text">${escapeHtml(control.reset.explanation)}</span>`,
      `</span>`,
    ].join("");
  }
  return [
    `<span class="account-reset" data-account-reset="unsafe">`,
    `<span class="account-reset-none">No reset</span>`,
    `<span class="account-reset-text">${escapeHtml(control.reset.reason)}</span>`,
    `</span>`,
  ].join("");
}

function controlMarkup(control: AccountControl): string {
  const action = control.action
    ? [
        `<button type="button" class="account-action" data-account-action="request"`,
        ` data-account-control="${escapeHtml(control.id)}">${escapeHtml(control.action)}</button>`,
      ].join("")
    : "";
  return [
    `<li class="account-control" data-account-control="${escapeHtml(control.id)}"`,
    ` data-secret="${control.secret ? "yes" : "no"}" data-changed="${control.changed ? "yes" : "no"}"`,
    ` data-reset="${control.reset.safe ? "safe" : "unsafe"}">`,
    `<span class="account-control-head">`,
    `<span class="account-name">${escapeHtml(control.label)}</span>`,
    `<span class="account-detail">${escapeHtml(control.detail)}</span>`,
    `</span>`,
    `<span class="account-value-slot">${valueMarkup(control)}</span>`,
    `<span class="account-action-slot">${action}</span>`,
    resetMarkup(control),
    `</li>`,
  ].join("");
}

/** The payment notice is a semantic status with two keyboard-accessible controls. */
export function accountBuyerNoticeMarkup(event: AccountBuyerNoticeEvent | null): string {
  if (!event) return "";
  const copy = ACCOUNT_BUYER_NOTICES[event];
  return [
    `<section class="account-buyer-notice" role="status" aria-live="polite"`,
    ` aria-labelledby="account-buyer-notice-title" data-account-buyer-event="${event}">`,
    `<div class="account-buyer-notice-copy">`,
    `<h2 id="account-buyer-notice-title">${escapeHtml(copy.title)}</h2>`,
    `<p>${escapeHtml(copy.notice)}</p>`,
    `</div>`,
    `<div class="account-buyer-notice-actions" aria-label="${escapeHtml(copy.title)} actions">`,
    `<button type="button" data-account-action="contact-support">Contact support</button>`,
    `<button type="button" data-account-action="close-buyer-notice">Close</button>`,
    `</div>`,
    `</section>`,
  ].join("");
}

export function closeAccountBuyerNotice(state: AccountScreenState): AccountScreenState {
  return { ...state, buyerNoticeEvent: null };
}

export const SAVE_ACCOUNT_EXPLANATION = "Keeps the identity and lock changes, and any control you reset.";
export const RESET_SAFE_EXPLANATION =
  `Puts back only what can be put back: ${ACCOUNT_SAFE_RESET_IDS.map((id) => ACCOUNT_CONTROL_LABELS[id].toLowerCase()).join(", ")}.`;

export const ACCOUNT_SCREEN_TITLE = "Account";

/** The whole screen. Styling lives in `account-screen.css`. */
export function renderAccountScreen(state: AccountScreenState, lastRequest: string = ""): string {
  const controls = accountControls(state);
  return [
    `<section class="account-screen" aria-label="Account">`,
    `<header class="account-screen-header">`,
    `<h1 class="account-screen-heading">${ACCOUNT_SCREEN_TITLE}</h1>`,
    `<p class="account-screen-intro">${ACCOUNT_CONTROL_IDS.length} controls. The five that stand for a secret show that it is set and nothing else, and Reset is offered only where putting the control back costs nothing.</p>`,
    `</header>`,
    accountBuyerNoticeMarkup(state.buyerNoticeEvent),
    `<ul class="account-controls">${controls.map(controlMarkup).join("")}</ul>`,
    `<section class="account-actions" aria-label="Save the account, or reset what is safe">`,
    `<div class="account-action-group">`,
    `<button type="button" class="account-action-button" data-account-action="save">Save account</button>`,
    `<p class="account-action-text">${escapeHtml(SAVE_ACCOUNT_EXPLANATION)}</p>`,
    `</div>`,
    `<div class="account-action-group">`,
    `<button type="button" class="account-action-button account-action-button-quiet" data-account-action="reset-safe">Reset safe settings</button>`,
    `<p class="account-action-text">${escapeHtml(RESET_SAFE_EXPLANATION)}</p>`,
    `</div>`,
    `<p class="account-status" role="status" data-changed="${changedAccountControls(state).length}">`,
    `${escapeHtml(accountStatusLine(state))}${lastRequest ? ` ${escapeHtml(lastRequest)}` : ""}</p>`,
    `</section>`,
    `<nav class="account-screen-links" aria-label="Account links">`,
    `<span>Profile</span><span>Sign out</span><span>Delete account</span>`,
    `</nav>`,
    `</section>`,
  ].join("");
}

/** What the status line says after a control hands its job on. */
export function accountRequestLine(id: AccountControlId): string {
  return `${ACCOUNT_CONTROL_LABELS[id]}: asked for on another screen.`;
}

/**
 * Mount the screen and keep its state. The secrets are read once, here, and
 * only the view survives the call -- the mounted screen has no reference to
 * them at all.
 */
export function attachAccountScreen(
  mount: HTMLElement,
  secrets: AccountSecrets,
  settings: AccountSettings,
  handlers: {
    onSave?: (saved: SavedAccountSetting[]) => void;
    onRequest?: (id: AccountControlId) => void;
    onContactSupport?: (event: AccountBuyerNoticeEvent) => void;
    onCloseBuyerNotice?: (event: AccountBuyerNoticeEvent) => void;
    buyerNoticeEvent?: AccountBuyerNoticeEvent | null;
  } = {},
): void {
  let state = accountScreenState(secrets, settings, handlers.buyerNoticeEvent ?? null);
  let lastRequest = "";
  const draw = (): void => {
    mount.innerHTML = renderAccountScreen(state, lastRequest);
  };
  const controlOf = (element: HTMLElement): AccountControlId => {
    const id = element.dataset.accountControl as AccountControlId | undefined;
    if (!id || !ACCOUNT_CONTROL_IDS.includes(id)) throw new Error(`Unknown account control: ${id}`);
    return id;
  };
  mount.addEventListener("change", (event) => {
    const target = event.target as HTMLInputElement | HTMLSelectElement | null;
    if (!target) return;
    if (target.dataset.accountInput === "identity") {
      state = setDisplayName(state, target.value);
      draw();
      return;
    }
    if (target.dataset.accountSelect === "lock") {
      state = setLockMinutes(state, Number(target.value));
      draw();
    }
  });
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest?.("[data-account-action]") as HTMLElement | null;
    if (!button) return;
    const action = button.dataset.accountAction;
    if (action === "contact-support") {
      if (state.buyerNoticeEvent) handlers.onContactSupport?.(state.buyerNoticeEvent);
      return;
    }
    if (action === "close-buyer-notice") {
      const eventName = state.buyerNoticeEvent;
      if (!eventName) return;
      state = closeAccountBuyerNotice(state);
      handlers.onCloseBuyerNotice?.(eventName);
      draw();
      return;
    }
    if (action === "reset") {
      state = resetAccountControl(state, controlOf(button));
      lastRequest = "";
      draw();
      return;
    }
    if (action === "reset-safe") {
      state = resetSafeAccountControls(state);
      lastRequest = "";
      draw();
      return;
    }
    if (action === "request") {
      const id = controlOf(button);
      lastRequest = accountRequestLine(id);
      handlers.onRequest?.(id);
      draw();
      return;
    }
    if (action === "save") {
      const result = saveAccount(state);
      state = result.state;
      lastRequest = "";
      handlers.onSave?.(result.saved);
      draw();
    }
  });
  draw();
}
