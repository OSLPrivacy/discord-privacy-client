import type { SendMode } from "./state";

/**
 * TASK 0760 - the Apps and sending screen.
 *
 * One screen answers four questions per app: WHICH ACCOUNT it uses, HOW you get
 * into it (open / set up / remove), HOW OSL puts a message into it, and WHETHER
 * the next-generation protected message format is in use.
 *
 * The rules below are the point of the module. A screen that just prints what a
 * fixture claims can show sending choices for an app that is not connected, two
 * "active" styles at once, or "On" for a next-generation switch that this build
 * cannot honour. All three are states the app can genuinely reach, so all three
 * are derived here instead of trusted:
 *
 * - sending choices belong to a CONNECTED app; a not-connected app has none;
 * - "active" means exactly one, never zero and never two;
 * - the next-generation state mirrors `rnWirePolicyState` in main.ts: it is on
 *   only when the build allows it AND the setting asks for it. That pairing is
 *   what TASK 0713 proved the broker actually reads.
 */

export type AppConnectionState = "connected" | "notConnected";
export type SendStyleId = SendMode;
export type NextGenerationState = "on" | "off" | "unavailable";
export type SendStyleRisk = "" | "warn" | "danger";

export interface AppAccountChoice {
  id: string;
  label: string;
  detail: string;
  selected: boolean;
}

export interface SendStyleChoice {
  id: SendStyleId;
  name: string;
  detail: string;
  /** Empty unless pressing keys for you can reach the wrong chat. */
  tag: string;
  risk: SendStyleRisk;
  active: boolean;
}

export interface NextGenerationPolicy {
  /** What the saved setting asks for. */
  requested: boolean;
  /** What this build can actually do. */
  buildEnabled: boolean;
  detail: string;
}

export interface AppsAndSendingApp {
  id: string;
  name: string;
  state: AppConnectionState;
  /** Short line under the app name, e.g. how it is reached. */
  detail: string;
  accounts: AppAccountChoice[];
  sendStyles: SendStyleChoice[];
  nextGeneration: NextGenerationPolicy;
}

export interface AppsAndSendingModel {
  title: string;
  subtitle: string;
  apps: AppsAndSendingApp[];
  saveNote: string;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

export function connectedApps(model: AppsAndSendingModel): AppsAndSendingApp[] {
  return model.apps.filter((app) => app.state === "connected");
}

/** The one chosen account, or null when the fixture chose none or more than one. */
export function selectedAccount(app: AppsAndSendingApp): AppAccountChoice | null {
  const chosen = app.accounts.filter((account) => account.selected);
  return chosen.length === 1 ? chosen[0] : null;
}

/**
 * The one live sending style. Null for a not-connected app: there is nothing to
 * send into yet, so claiming an active style there would be a false statement.
 */
export function activeSendStyle(app: AppsAndSendingApp): SendStyleChoice | null {
  if (app.state !== "connected") return null;
  const active = app.sendStyles.filter((style) => style.active);
  return active.length === 1 ? active[0] : null;
}

export function nextGenerationState(policy: NextGenerationPolicy): NextGenerationState {
  if (!policy.buildEnabled) return "unavailable";
  return policy.requested ? "on" : "off";
}

export function nextGenerationStateLabel(state: NextGenerationState): string {
  if (state === "on") return "On";
  return state === "off" ? "Off" : "Unavailable";
}

function accountsMarkup(app: AppsAndSendingApp): string {
  // An app you have not set up has no accounts to choose between, so it gets no
  // chooser rather than an empty one.
  if (app.accounts.length === 0) return "";
  const chosen = selectedAccount(app);
  const choices = app.accounts
    .map((account) => {
      const isChosen = chosen !== null && account.id === chosen.id;
      return `<button
          class="apps-sending-choice apps-sending-account${isChosen ? " selected" : ""}"
          type="button"
          role="radio"
          aria-checked="${isChosen}"
          data-account-choice="${escapeHtml(account.id)}"
          data-account-selected="${isChosen}"
        ><strong class="apps-sending-choice-name">${escapeHtml(account.label)}</strong><small class="apps-sending-choice-detail">${escapeHtml(account.detail)}</small></button>`;
    })
    .join("");
  return `<div class="apps-sending-field" data-field="account">
      <span class="apps-sending-field-label">Account</span>
      <div class="apps-sending-choices" role="radiogroup" aria-label="${escapeHtml(app.name)} account">${choices}</div>
    </div>`;
}

function actionsMarkup(app: AppsAndSendingApp): string {
  const connected = app.state === "connected";
  const open = `<button class="button compact apps-sending-action" type="button" data-app-action="open" id="apps-sending-open-${escapeHtml(app.id)}"${connected ? "" : " disabled"}>Open ${escapeHtml(app.name)}</button>`;
  const setUp = `<button class="button compact apps-sending-action" type="button" data-app-action="set-up" id="apps-sending-set-up-${escapeHtml(app.id)}">Set up</button>`;
  // Nothing to remove until something is connected, so the button is not drawn.
  const remove = connected
    ? `<button class="button compact danger apps-sending-action" type="button" data-app-action="remove" id="apps-sending-remove-${escapeHtml(app.id)}">Remove app</button>`
    : "";
  return `<div class="apps-sending-actions" role="group" aria-label="${escapeHtml(app.name)} actions">${open}${setUp}${remove}</div>`;
}

function sendStyleMarkup(app: AppsAndSendingApp): string {
  const active = activeSendStyle(app);
  const choices = app.sendStyles
    .map((style) => {
      const isActive = active !== null && style.id === active.id;
      const tag = style.tag
        ? `<em class="apps-sending-choice-tag" data-risk="${escapeHtml(style.risk)}">${escapeHtml(style.tag)}</em>`
        : "";
      return `<button
          class="apps-sending-choice apps-sending-send-style${isActive ? " selected" : ""}"
          type="button"
          role="radio"
          aria-checked="${isActive}"
          data-send-style="${escapeHtml(style.id)}"
          data-send-style-active="${isActive}"
        ><strong class="apps-sending-choice-name">${escapeHtml(style.name)}${tag}</strong><small class="apps-sending-choice-detail">${escapeHtml(style.detail)}</small></button>`;
    })
    .join("");
  return `<div class="apps-sending-field" data-field="send-style" data-active-send-style="${escapeHtml(active?.id ?? "none")}">
      <span class="apps-sending-field-label">Send messages</span>
      <div class="apps-sending-choices" role="radiogroup" aria-label="Send messages with ${escapeHtml(app.name)}">${choices}</div>
    </div>`;
}

function nextGenerationMarkup(app: AppsAndSendingApp): string {
  const state = nextGenerationState(app.nextGeneration);
  return `<label class="apps-sending-switch" data-next-generation="${state}" data-app-id="${escapeHtml(app.id)}">
      <span class="apps-sending-switch-copy"><strong>Next-generation messages</strong><small>${escapeHtml(app.nextGeneration.detail)}</small></span>
      <span class="apps-sending-switch-control">
        <input
          class="apps-sending-switch-input"
          id="apps-sending-next-generation-${escapeHtml(app.id)}"
          type="checkbox"
          ${state === "on" ? "checked" : ""}
          ${state === "unavailable" ? "disabled" : ""}
        />
        <span class="apps-sending-switch-state" data-next-generation-state="${state}">${escapeHtml(nextGenerationStateLabel(state))}</span>
      </span>
    </label>`;
}

function appMarkup(app: AppsAndSendingApp): string {
  const connected = app.state === "connected";
  const chosen = selectedAccount(app);
  const status = connected ? "Connected" : "Not connected";
  // A not-connected app gets the head, the account list and the actions only.
  // Its sending choices do not exist yet and are not drawn as if they did.
  const sending = connected
    ? `${sendStyleMarkup(app)}${nextGenerationMarkup(app)}`
    : `<p class="apps-sending-pending">Set this app up to choose how OSL sends into it.</p>`;
  return `<li class="apps-sending-app" data-app-id="${escapeHtml(app.id)}" data-app-state="${connected ? "connected" : "not-connected"}">
      <div class="apps-sending-app-head">
        <span class="apps-sending-app-mark" aria-hidden="true">${escapeHtml(app.name.slice(0, 1).toUpperCase())}</span>
        <span class="apps-sending-app-identity">
          <strong class="apps-sending-app-name">${escapeHtml(app.name)}</strong>
          <small class="apps-sending-app-detail">${escapeHtml(chosen ? `${chosen.label} · ${app.detail}` : app.detail)}</small>
        </span>
        <span class="apps-sending-app-status" data-app-status="${connected ? "connected" : "not-connected"}">${status}</span>
      </div>
      ${accountsMarkup(app)}
      ${actionsMarkup(app)}
      ${sending}
    </li>`;
}

export function appsAndSendingScreenMarkup(model: AppsAndSendingModel): string {
  const connected = connectedApps(model);
  const notConnected = model.apps.filter((app) => app.state !== "connected");
  const list = (apps: AppsAndSendingApp[]) =>
    `<ul class="apps-sending-list">${apps.map(appMarkup).join("")}</ul>`;
  const others = notConnected.length > 0
    ? `<section class="apps-sending-group apps-sending-group-quiet" aria-labelledby="apps-sending-other-heading">
        <h2 class="apps-sending-group-title" id="apps-sending-other-heading">Other apps</h2>
        ${list(notConnected)}
      </section>`
    : "";
  return `<section
      class="apps-sending-screen"
      data-apps-sending-screen="task-0760"
      data-connected-count="${connected.length}"
      aria-labelledby="apps-sending-title"
    >
    <header class="apps-sending-header">
      <h1 class="apps-sending-title" id="apps-sending-title" tabindex="-1">${escapeHtml(model.title)}</h1>
      <p class="apps-sending-subtitle">${escapeHtml(model.subtitle)}</p>
    </header>
    <section class="apps-sending-group" aria-labelledby="apps-sending-connected-heading">
      <h2 class="apps-sending-group-title" id="apps-sending-connected-heading">Connected apps</h2>
      <p class="apps-sending-group-note" id="apps-sending-connected-count">${connected.length} of ${model.apps.length} apps connected</p>
      ${list(connected)}
    </section>
    ${others}
    <footer class="apps-sending-footer">
      <p class="apps-sending-save-note">${escapeHtml(model.saveNote)}</p>
      <span class="apps-sending-footer-buttons">
        <button class="button apps-sending-action" type="button" id="apps-sending-reset">Reset</button>
        <button class="button primary apps-sending-action" type="button" id="apps-sending-save">Save</button>
      </span>
    </footer>
  </section>`;
}
