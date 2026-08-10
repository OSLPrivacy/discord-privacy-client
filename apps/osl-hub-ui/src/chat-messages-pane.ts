/**
 * Standalone Messages settings pane.  The chat thread applies these saved
 * choices separately; this module owns only the settings surface and its
 * durable device-local preference record.
 */

export const CHAT_TEXT_SIZE = {
  default: 15.5,
  min: 13,
  max: 20,
} as const;

export const CHAT_CORNER_ROUNDING = { min: 0, max: 20, default: 12 } as const;
export const CHAT_SPACING = { min: 0, max: 16, default: 8 } as const;

export const CHAT_MESSAGE_FONTS = [
  { id: "system", label: "System", family: "system-ui, -apple-system, BlinkMacSystemFont, sans-serif" },
  { id: "segoe", label: "Segoe UI", family: '"Segoe UI", "Segoe UI Variable", sans-serif' },
  { id: "source-sans", label: "Source Sans", family: '"Source Sans 3 Variable", "Source Sans 3", sans-serif' },
  { id: "georgia", label: "Georgia", family: "Georgia, serif" },
  { id: "consolas", label: "Consolas", family: "Consolas, \"Cascadia Mono\", monospace" },
] as const;

export type ChatMessageFont = typeof CHAT_MESSAGE_FONTS[number]["id"];

export const CHAT_MESSAGE_COLOURS = ["#7166d9", "#3f9ad8", "#3ca87c", "#d6874d", "#c55f88"] as const;
export type ChatMessageColour = typeof CHAT_MESSAGE_COLOURS[number] | string;

export interface ChatMessagesPreferences {
  density: "cosy" | "compact";
  bubbles: boolean;
  font: ChatMessageFont;
  textSize: number;
  cornerRounding: number;
  spacing: number;
  messageColour: ChatMessageColour;
}

export const DEFAULT_CHAT_MESSAGES_PREFERENCES: Readonly<ChatMessagesPreferences> = {
  density: "cosy",
  bubbles: true,
  font: "system",
  textSize: CHAT_TEXT_SIZE.default,
  cornerRounding: CHAT_CORNER_ROUNDING.default,
  spacing: CHAT_SPACING.default,
  messageColour: CHAT_MESSAGE_COLOURS[0],
};

export const CHAT_MESSAGES_PREFERENCES_KEY = "osl.chat.messages.preferences.v1";

function bounded(value: unknown, min: number, max: number, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? Math.min(max, Math.max(min, value)) : fallback;
}

function isFont(value: unknown): value is ChatMessageFont {
  return CHAT_MESSAGE_FONTS.some((font) => font.id === value);
}

function isHex(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/iu.test(value);
}

export function normaliseChatMessagesPreferences(value: Partial<ChatMessagesPreferences> = {}): ChatMessagesPreferences {
  return {
    density: value.density === "compact" ? "compact" : "cosy",
    bubbles: typeof value.bubbles === "boolean" ? value.bubbles : DEFAULT_CHAT_MESSAGES_PREFERENCES.bubbles,
    font: isFont(value.font) ? value.font : DEFAULT_CHAT_MESSAGES_PREFERENCES.font,
    textSize: bounded(value.textSize, CHAT_TEXT_SIZE.min, CHAT_TEXT_SIZE.max, CHAT_TEXT_SIZE.default),
    cornerRounding: bounded(value.cornerRounding, CHAT_CORNER_ROUNDING.min, CHAT_CORNER_ROUNDING.max, CHAT_CORNER_ROUNDING.default),
    spacing: bounded(value.spacing, CHAT_SPACING.min, CHAT_SPACING.max, CHAT_SPACING.default),
    messageColour: isHex(value.messageColour) ? value.messageColour : DEFAULT_CHAT_MESSAGES_PREFERENCES.messageColour,
  };
}

export function loadChatMessagesPreferences(storage: Pick<Storage, "getItem"> | null = globalThis.localStorage): ChatMessagesPreferences {
  try {
    const saved = storage?.getItem(CHAT_MESSAGES_PREFERENCES_KEY);
    return normaliseChatMessagesPreferences(saved ? JSON.parse(saved) : {});
  } catch {
    return { ...DEFAULT_CHAT_MESSAGES_PREFERENCES };
  }
}

export function saveChatMessagesPreferences(preferences: ChatMessagesPreferences, storage: Pick<Storage, "setItem"> | null = globalThis.localStorage): ChatMessagesPreferences {
  const saved = normaliseChatMessagesPreferences(preferences);
  try { storage?.setItem(CHAT_MESSAGES_PREFERENCES_KEY, JSON.stringify(saved)); } catch { /* settings remain usable without storage */ }
  return saved;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}

function displayNumber(value: number): string { return Number.isInteger(value) ? String(value) : value.toFixed(1); }

function sliderMarkup(id: "textSize" | "cornerRounding" | "spacing", label: string, value: number, min: number, max: number, step = 1): string {
  return `<label class="chat-messages-pane__slider-row" for="chat-messages-${id}"><span>${label}</span><output data-chat-messages-value="${id}">${displayNumber(value)}</output><input id="chat-messages-${id}" data-chat-messages-control="${id}" type="range" min="${min}" max="${max}" step="${step}" value="${value}" aria-label="${label}" /></label>`;
}

/** Returns markup that can be inserted into any settings route without dependencies. */
export function chatMessagesPaneMarkup(preferences: Partial<ChatMessagesPreferences> = {}): string {
  const state = normaliseChatMessagesPreferences(preferences);
  return `<section class="chat-messages-pane" data-chat-messages-pane aria-labelledby="chat-messages-title">
    <header><h2 id="chat-messages-title">Messages</h2></header>
    <div class="chat-messages-pane__control"><span class="chat-messages-pane__label">DENSITY</span><div class="chat-messages-pane__segmented" role="group" aria-label="Message density"><button type="button" data-chat-messages-density="cosy" aria-pressed="${state.density === "cosy"}">Cosy</button><button type="button" data-chat-messages-density="compact" aria-pressed="${state.density === "compact"}">Compact</button></div></div>
    <label class="chat-messages-pane__toggle"><span><strong>Bubbles</strong><small>Show messages in bubbles</small></span><input data-chat-messages-control="bubbles" type="checkbox" role="switch" ${state.bubbles ? "checked" : ""} /></label>
    <div class="chat-messages-pane__control"><span class="chat-messages-pane__label">FONT</span><div class="chat-messages-pane__fonts" role="group" aria-label="Message font">${CHAT_MESSAGE_FONTS.map((font) => `<button type="button" data-chat-messages-font="${font.id}" aria-pressed="${state.font === font.id}" style="font-family:${font.family}">${font.label}</button>`).join("")}</div></div>
    <div class="chat-messages-pane__sliders">${sliderMarkup("textSize", "TEXT SIZE", state.textSize, CHAT_TEXT_SIZE.min, CHAT_TEXT_SIZE.max, .5)}${sliderMarkup("cornerRounding", "CORNER ROUNDING", state.cornerRounding, CHAT_CORNER_ROUNDING.min, CHAT_CORNER_ROUNDING.max)}${sliderMarkup("spacing", "SPACING", state.spacing, CHAT_SPACING.min, CHAT_SPACING.max)}</div>
    <div class="chat-messages-pane__control"><span class="chat-messages-pane__label">YOUR MESSAGE COLOUR</span><div class="chat-messages-pane__colours" role="group" aria-label="Your message colour">${CHAT_MESSAGE_COLOURS.map((colour) => `<button type="button" data-chat-messages-colour="${colour}" aria-label="Use ${colour}" aria-pressed="${state.messageColour.toLowerCase() === colour}"><i style="background:${colour}"></i></button>`).join("")}<label class="chat-messages-pane__custom-colour"><span>Hex</span><input data-chat-messages-control="messageColour" type="text" value="${escapeHtml(state.messageColour)}" pattern="#[0-9A-Fa-f]{6}" maxlength="7" spellcheck="false" aria-label="Custom message colour hex" /></label></div></div>
  </section>`;
}

export interface ChatMessagesPaneOptions {
  preferences?: Partial<ChatMessagesPreferences>;
  storage?: Pick<Storage, "getItem" | "setItem"> | null;
  onChange?: (preferences: ChatMessagesPreferences) => void;
}

/** Mounts, persists, and updates the standalone Messages pane. */
export function renderChatMessagesPane(container: HTMLElement, options: ChatMessagesPaneOptions = {}): () => void {
  const storage = options.storage === undefined ? globalThis.localStorage : options.storage;
  let state = normaliseChatMessagesPreferences({ ...loadChatMessagesPreferences(storage), ...options.preferences });
  const draw = () => { container.innerHTML = chatMessagesPaneMarkup(state); };
  const save = () => { state = saveChatMessagesPreferences(state, storage); options.onChange?.(state); draw(); };
  const onClick = (event: Event) => {
    const target = (event.target as Element | null)?.closest<HTMLElement>("[data-chat-messages-density], [data-chat-messages-font], [data-chat-messages-colour]");
    if (!target) return;
    const density = target.dataset.chatMessagesDensity;
    const font = target.dataset.chatMessagesFont;
    const colour = target.dataset.chatMessagesColour;
    if (density === "cosy" || density === "compact") state = { ...state, density };
    if (isFont(font)) state = { ...state, font };
    // A colour is a bubble treatment, so selecting one always enables bubbles.
    if (isHex(colour)) state = { ...state, messageColour: colour, bubbles: true };
    save();
  };
  const onInput = (event: Event) => {
    const input = event.target as HTMLInputElement;
    switch (input.dataset.chatMessagesControl) {
      case "bubbles": state = { ...state, bubbles: input.checked }; break;
      case "textSize": state = { ...state, textSize: Number(input.value) }; break;
      case "cornerRounding": state = { ...state, cornerRounding: Number(input.value) }; break;
      case "spacing": state = { ...state, spacing: Number(input.value) }; break;
      // Custom colours, like swatches, explicitly turn bubbles on.
      case "messageColour":
        if (!isHex(input.value)) return;
        state = { ...state, messageColour: input.value, bubbles: true };
        break;
      default: return;
    }
    save();
  };
  draw();
  container.addEventListener("click", onClick);
  container.addEventListener("input", onInput);
  return () => { container.removeEventListener("click", onClick); container.removeEventListener("input", onInput); };
}
