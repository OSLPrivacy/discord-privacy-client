import {
  whitelistingClearAll,
  whitelistingMatches,
  whitelistingSelectAll,
  whitelistingSetSearch,
  whitelistingToggleConversation,
  type WhitelistingConversation,
} from "./whitelisting-screen";

export type WhitelistSetupRule = "deny" | "ask";

export interface WhitelistSetupSavedState {
  allowedConversationIds: readonly string[];
  newlyFoundConversationRule: WhitelistSetupRule;
}

export interface WhitelistSetupDraft {
  readonly conversations: readonly WhitelistingConversation[];
  saved: readonly string[];
  draft: readonly string[];
  search: string;
  busy: boolean;
  newlyFoundConversationRule: WhitelistSetupRule;
}

export interface WhitelistSetupStore {
  load(): Promise<WhitelistSetupSavedState | null>;
  save(state: WhitelistSetupSavedState): Promise<void>;
}

export interface WhitelistSetupCallbacks {
  onChange?: (draft: WhitelistSetupDraft) => void;
  onContinue: (saved: WhitelistSetupSavedState) => void;
  onBack: () => void;
  onError?: (message: string) => void;
}

type WhitelistSetupRoot = Pick<ParentNode, "querySelector" | "querySelectorAll">;

export const DEFAULT_WHITELIST_SETUP_RULE: WhitelistSetupRule = "deny";

export function initialWhitelistSetupDraft(
  conversations: readonly WhitelistingConversation[],
  saved: WhitelistSetupSavedState | null = null,
): WhitelistSetupDraft {
  const savedAllowed = new Set(saved?.allowedConversationIds ?? []);
  const allowed = conversations.map((conversation) => conversation.id).filter((id) => savedAllowed.has(id));
  return {
    conversations,
    saved: allowed,
    draft: allowed,
    search: "",
    busy: false,
    newlyFoundConversationRule: saved?.newlyFoundConversationRule ?? DEFAULT_WHITELIST_SETUP_RULE,
  };
}

export async function openWhitelistSetup(
  conversations: readonly WhitelistingConversation[],
  store: WhitelistSetupStore,
): Promise<WhitelistSetupDraft> {
  const saved = await store.load();
  if (saved !== null && !isWhitelistSetupRule(saved.newlyFoundConversationRule)) {
    throw new Error("Whitelist setup returned an invalid rule");
  }
  return initialWhitelistSetupDraft(conversations, saved);
}

export function whitelistSetupSelectedCount(draft: WhitelistSetupDraft): number {
  const selected = new Set(draft.draft);
  return draft.conversations.filter((conversation) => selected.has(conversation.id)).length;
}

export function whitelistSetupMarkup(draft: WhitelistSetupDraft): string {
  const selected = new Set(draft.draft);
  const matches = whitelistingMatches(draft.conversations, draft.search);
  const rows = matches.map((conversation) => {
    const checked = selected.has(conversation.id);
    return `<label class="whitelisting-row" data-whitelist-setup-row="${escapeHtml(conversation.id)}"><input type="checkbox" name="whitelist-setup-conversation" value="${escapeHtml(conversation.id)}"${checked ? " checked" : ""}${draft.busy ? " disabled" : ""}/><span><strong>${escapeHtml(conversation.name)}</strong><small>${escapeHtml(conversation.account)} · ${escapeHtml(conversation.kind)}</small></span></label>`;
  }).join("");
  const selectedCount = whitelistSetupSelectedCount(draft);
  return `<section class="whitelist-setup whitelisting-screen" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1">Whitelist setup</h1>
    <p class="compact-lead">Choose the conversations OSL may protect and what happens when a new conversation appears.</p>
    <label for="whitelist-setup-search">Search conversations</label>
    <input id="whitelist-setup-search" type="search" autocomplete="off" maxlength="64" value="${escapeHtml(draft.search)}"${draft.busy ? " disabled" : ""}/>
    <div class="whitelisting-bulk" role="group" aria-label="Change every conversation the search is showing"><button class="button compact" id="select-all-whitelist-setup" type="button"${draft.busy || matches.every((conversation) => selected.has(conversation.id)) ? " disabled" : ""}>Select all</button><button class="button compact" id="clear-all-whitelist-setup" type="button"${draft.busy || !matches.some((conversation) => selected.has(conversation.id)) ? " disabled" : ""}>Clear all</button></div>
    <div class="whitelisting-list" data-whitelist-setup-list>${rows || `<p class="empty-state compact">No conversations match this search.</p>`}</div>
    <p id="whitelist-setup-count" role="status">${selectedCount} selected ${selectedCount === 1 ? "conversation" : "conversations"}</p>
    <fieldset><legend>New conversations</legend><label><input type="radio" name="whitelist-setup-rule" value="deny"${draft.newlyFoundConversationRule === "deny" ? " checked" : ""}${draft.busy ? " disabled" : ""}/>Deny</label><label><input type="radio" name="whitelist-setup-rule" value="ask"${draft.newlyFoundConversationRule === "ask" ? " checked" : ""}${draft.busy ? " disabled" : ""}/>Ask</label></fieldset>
    <p id="whitelist-setup-error" role="alert"></p>
    <div class="setup-footer onboarding-actions"><button class="button ghost" id="back-whitelist-setup" type="button"${draft.busy ? " disabled" : ""}>Back</button><button class="button primary" id="continue-whitelist-setup" type="button"${draft.busy ? " disabled" : ""}>${draft.busy ? "Saving…" : "Continue"}</button></div>
  </section>`;
}

/** Connects every control on the rendered setup page to one live draft. */
export function bindWhitelistSetupControls(
  root: WhitelistSetupRoot,
  draft: WhitelistSetupDraft,
  store: WhitelistSetupStore,
  callbacks: WhitelistSetupCallbacks,
): void {
  const search = root.querySelector<HTMLInputElement>("#whitelist-setup-search");
  const selectAll = root.querySelector<HTMLButtonElement>("#select-all-whitelist-setup");
  const clearAll = root.querySelector<HTMLButtonElement>("#clear-all-whitelist-setup");
  const ticks = [...root.querySelectorAll<HTMLInputElement>('input[name="whitelist-setup-conversation"]')];
  const rules = [...root.querySelectorAll<HTMLInputElement>('input[name="whitelist-setup-rule"]')];
  const count = root.querySelector<HTMLElement>("#whitelist-setup-count");
  const error = root.querySelector<HTMLElement>("#whitelist-setup-error");
  const continueButton = root.querySelector<HTMLButtonElement>("#continue-whitelist-setup");
  const backButton = root.querySelector<HTMLButtonElement>("#back-whitelist-setup");
  let busy = false;

  const notifyChange = (): void => callbacks.onChange?.(copyDraft(draft));
  const syncControls = (): void => {
    const selected = new Set(draft.draft);
    const matching = new Set(whitelistingMatches(draft.conversations, draft.search).map((conversation) => conversation.id));
    for (const tick of ticks) {
      tick.checked = selected.has(tick.value);
      tick.disabled = busy;
      const row = root.querySelector<HTMLElement>(`[data-whitelist-setup-row="${cssAttributeValue(tick.value)}"]`);
      if (row) row.hidden = !matching.has(tick.value);
    }
    for (const rule of rules) {
      rule.checked = rule.value === draft.newlyFoundConversationRule;
      rule.disabled = busy;
    }
    const matchingIds = [...matching];
    if (search) search.disabled = busy;
    if (selectAll) selectAll.disabled = busy || matchingIds.length === 0 || matchingIds.every((id) => selected.has(id));
    if (clearAll) clearAll.disabled = busy || !matchingIds.some((id) => selected.has(id));
    if (continueButton) continueButton.disabled = busy;
    if (backButton) backButton.disabled = busy;
    if (count) {
      const selectedCount = whitelistSetupSelectedCount(draft);
      count.textContent = `${selectedCount} selected ${selectedCount === 1 ? "conversation" : "conversations"}`;
    }
  };

  search?.addEventListener("input", () => {
    Object.assign(draft, whitelistingSetSearch(draft, search.value));
    syncControls();
    notifyChange();
  });
  selectAll?.addEventListener("click", () => {
    if (busy) return;
    Object.assign(draft, whitelistingSelectAll(draft));
    syncControls();
    notifyChange();
  });
  clearAll?.addEventListener("click", () => {
    if (busy) return;
    Object.assign(draft, whitelistingClearAll(draft));
    syncControls();
    notifyChange();
  });
  for (const tick of ticks) tick.addEventListener("change", () => {
    if (busy) return;
    Object.assign(draft, whitelistingToggleConversation(draft, tick.value, tick.checked));
    syncControls();
    notifyChange();
  });
  for (const rule of rules) rule.addEventListener("change", () => {
    if (busy || !rule.checked || !isWhitelistSetupRule(rule.value)) return;
    draft.newlyFoundConversationRule = rule.value;
    syncControls();
    notifyChange();
  });
  continueButton?.addEventListener("click", async () => {
    if (busy) return;
    const submitted: WhitelistSetupSavedState = {
      allowedConversationIds: [...draft.draft],
      newlyFoundConversationRule: draft.newlyFoundConversationRule,
    };
    busy = true;
    draft.busy = true;
    if (error) error.textContent = "";
    syncControls();
    try {
      await store.save(submitted);
      draft.saved = [...submitted.allowedConversationIds];
      callbacks.onContinue(submitted);
    } catch (failure) {
      const message = failure instanceof Error ? failure.message : "Whitelist setup could not be saved";
      if (error) error.textContent = message;
      callbacks.onError?.(message);
    } finally {
      busy = false;
      draft.busy = false;
      syncControls();
    }
  });
  backButton?.addEventListener("click", () => {
    if (!busy) callbacks.onBack();
  });
  syncControls();
}

function copyDraft(draft: WhitelistSetupDraft): WhitelistSetupDraft {
  return { ...draft, saved: [...draft.saved], draft: [...draft.draft] };
}

function isWhitelistSetupRule(value: unknown): value is WhitelistSetupRule {
  return value === "deny" || value === "ask";
}

function cssAttributeValue(value: string): string {
  return value.replace(/["\\]/gu, "\\$&");
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}
