import "./chat-background-pane.css";
import { choiceRadio, onOffToggle } from "./onboarding-controls";

/** Settings saved for either one conversation or the default for all chats. */
export type ChatBackgroundName = "none" | "ink" | "slate" | "deep-teal" | "dusk" | "moss" | "ember" | "grid" | "upload";
export type ChatBackgroundScope = "chat" | "every-chat";

export type ChatBackgroundSettings = {
  background: ChatBackgroundName;
  uploadedImage?: string;
  blur: boolean;
  motion: boolean;
  alsoSetForThem: boolean;
  scope: ChatBackgroundScope;
};

export const CHAT_BACKGROUND_GLOBAL_STORAGE_KEY = "osl-chat-background-global-v1";
export const CHAT_BACKGROUND_BY_CHAT_STORAGE_KEY = "osl-chat-background-by-chat-v1";

export const CHAT_BACKGROUND_TILES: ReadonlyArray<{ value: ChatBackgroundName; label: string }> = [
  { value: "none", label: "None" },
  { value: "ink", label: "Ink" },
  { value: "slate", label: "Slate" },
  { value: "deep-teal", label: "Deep teal" },
  { value: "dusk", label: "Dusk" },
  { value: "moss", label: "Moss" },
  { value: "ember", label: "Ember" },
  { value: "grid", label: "Grid" },
];

export const DEFAULT_CHAT_BACKGROUND_SETTINGS: ChatBackgroundSettings = {
  background: "none",
  blur: false,
  motion: true,
  alsoSetForThem: false,
  scope: "every-chat",
};

type StorageLike = Pick<Storage, "getItem" | "setItem">;
type SavedByChat = Record<string, ChatBackgroundSettings>;

function clone(settings: ChatBackgroundSettings): ChatBackgroundSettings {
  return { ...settings };
}

function validSettings(value: unknown): value is Partial<ChatBackgroundSettings> {
  return typeof value === "object" && value !== null;
}

function normaliseSettings(value: unknown, fallback: ChatBackgroundSettings): ChatBackgroundSettings {
  if (!validSettings(value)) return clone(fallback);
  const candidate = value as Partial<ChatBackgroundSettings>;
  const validBackground = CHAT_BACKGROUND_TILES.some((tile) => tile.value === candidate.background) || candidate.background === "upload";
  return {
    background: validBackground ? candidate.background as ChatBackgroundName : fallback.background,
    uploadedImage: typeof candidate.uploadedImage === "string" ? candidate.uploadedImage : undefined,
    blur: typeof candidate.blur === "boolean" ? candidate.blur : fallback.blur,
    motion: typeof candidate.motion === "boolean" ? candidate.motion : fallback.motion,
    alsoSetForThem: typeof candidate.alsoSetForThem === "boolean" ? candidate.alsoSetForThem : fallback.alsoSetForThem,
    scope: candidate.scope === "chat" || candidate.scope === "every-chat" ? candidate.scope : fallback.scope,
  };
}

function readJson(storage: StorageLike, key: string): unknown {
  try { return JSON.parse(storage.getItem(key) ?? "null"); } catch { return null; }
}

function readGlobalSettings(storage: StorageLike): ChatBackgroundSettings {
  return normaliseSettings(readJson(storage, CHAT_BACKGROUND_GLOBAL_STORAGE_KEY), DEFAULT_CHAT_BACKGROUND_SETTINGS);
}

/** Loads a chat override when one exists; otherwise it loads the saved all-chat default. */
export function loadChatBackgroundSettings(chatId: string, storage: StorageLike = localStorage): ChatBackgroundSettings {
  const global = readGlobalSettings(storage);
  const saved = readJson(storage, CHAT_BACKGROUND_BY_CHAT_STORAGE_KEY);
  if (!validSettings(saved)) return global;
  const fromChat = (saved as SavedByChat)[chatId];
  return fromChat ? normaliseSettings(fromChat, global) : global;
}

/** Saves to the selected scope. A chat setting remains independent from the all-chat default. */
export function saveChatBackgroundSettings(chatId: string, settings: ChatBackgroundSettings, storage: StorageLike = localStorage): void {
  const clean = normaliseSettings(settings, DEFAULT_CHAT_BACKGROUND_SETTINGS);
  if (clean.scope === "every-chat") {
    storage.setItem(CHAT_BACKGROUND_GLOBAL_STORAGE_KEY, JSON.stringify(clean));
    return;
  }
  const existing = readJson(storage, CHAT_BACKGROUND_BY_CHAT_STORAGE_KEY);
  const byChat: SavedByChat = validSettings(existing) ? existing as SavedByChat : {};
  byChat[chatId] = clean;
  storage.setItem(CHAT_BACKGROUND_BY_CHAT_STORAGE_KEY, JSON.stringify(byChat));
}

function escapeHtml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
}

function tileMarkup(tile: { value: ChatBackgroundName; label: string }, selected: ChatBackgroundName): string {
  return `<label class="chat-background-tile chat-background-${tile.value}${tile.value === selected ? " selected" : ""}">`
    + `<input class="sr-only" type="radio" name="chat-background" value="${tile.value}" data-chat-background-tile ${tile.value === selected ? "checked" : ""}/>`
    + `<span class="chat-background-swatch" aria-hidden="true"></span><span>${tile.label}</span></label>`;
}

/** Markup can be used in a route, sheet, or a standalone fixture. */
export function chatBackgroundPaneMarkup(settings: ChatBackgroundSettings): string {
  const uploadStyle = settings.uploadedImage ? ` style="background-image:url('${escapeHtml(settings.uploadedImage)}')"` : "";
  const tiles = CHAT_BACKGROUND_TILES.map((tile) => tileMarkup(tile, settings.background)).join("");
  return `<section class="chat-background-pane" aria-labelledby="chat-background-title">`
    + `<header><h2 id="chat-background-title">Chat background</h2><p>Choose what sits behind this conversation.</p></header>`
    + `<div class="chat-background-tiles" role="radiogroup" aria-label="Chat background">`
    + `<label class="chat-background-upload${settings.background === "upload" ? " selected" : ""}" data-chat-background-upload${uploadStyle}>`
    + `<input class="sr-only" type="radio" name="chat-background" value="upload" data-chat-background-tile ${settings.background === "upload" ? "checked" : ""}/>`
    + `<input class="sr-only" type="file" accept="image/*" data-chat-background-file/>`
    + `<span class="chat-background-upload-icon" aria-hidden="true">+</span><span>Upload</span></label>${tiles}</div>`
    + `<div class="chat-background-options">`
    + `<label class="chat-background-option"><span><strong>Blur</strong><small>Soften the background behind messages.</small></span>${onOffToggle("chat-background-blur", settings.blur, "Blur background")}</label>`
    + `<label class="chat-background-option"><span><strong>Motion</strong><small>Let animated backgrounds move.</small></span>${onOffToggle("chat-background-motion", settings.motion, "Background motion")}</label>`
    + `<label class="chat-background-option"><span><strong>Also set it for them</strong><small>Offer this background to the other person in the chat.</small></span>${onOffToggle("chat-background-for-them", settings.alsoSetForThem, "Also set it for them")}</label>`
    + `</div><fieldset class="chat-background-scope"><legend>Apply to</legend>`
    + `<label class="chat-background-scope-choice${settings.scope === "chat" ? " selected" : ""}"><input class="sr-only" type="radio" name="chat-background-scope" value="chat" data-chat-background-scope ${settings.scope === "chat" ? "checked" : ""}/>${choiceRadio()}<span>This chat only</span></label>`
    + `<label class="chat-background-scope-choice${settings.scope === "every-chat" ? " selected" : ""}"><input class="sr-only" type="radio" name="chat-background-scope" value="every-chat" data-chat-background-scope ${settings.scope === "every-chat" ? "checked" : ""}/>${choiceRadio()}<span>Every chat</span></label>`
    + `</fieldset><p class="chat-background-saved" role="status">Saved</p></section>`;
}

/** Renders and wires the pane. Each interaction saves immediately and survives a remount. */
export function mountChatBackgroundPane(container: HTMLElement, chatId: string, storage: StorageLike = localStorage): () => void {
  let settings = loadChatBackgroundSettings(chatId, storage);
  const render = (): void => {
    container.innerHTML = chatBackgroundPaneMarkup(settings);
    const save = (): void => { saveChatBackgroundSettings(chatId, settings, storage); };
    container.querySelectorAll<HTMLInputElement>("[data-chat-background-tile]").forEach((input) => input.addEventListener("change", () => {
      settings = { ...settings, background: input.value as ChatBackgroundName }; save(); render();
    }));
    (["blur", "motion", "for-them"] as const).forEach((name) => {
      const input = container.querySelector<HTMLInputElement>(`#chat-background-${name}`);
      input?.addEventListener("change", () => { settings = { ...settings, [name === "for-them" ? "alsoSetForThem" : name]: input.checked }; save(); });
    });
    container.querySelectorAll<HTMLInputElement>("[data-chat-background-scope]").forEach((input) => input.addEventListener("change", () => {
      settings = { ...settings, scope: input.value as ChatBackgroundScope }; save(); render();
    }));
    const file = container.querySelector<HTMLInputElement>("[data-chat-background-file]");
    file?.addEventListener("change", () => {
      const selected = file.files?.[0];
      if (!selected) return;
      const reader = new FileReader();
      reader.addEventListener("load", () => {
        if (typeof reader.result !== "string") return;
        settings = { ...settings, background: "upload", uploadedImage: reader.result }; save(); render();
      });
      reader.readAsDataURL(selected);
    });
  };
  render();
  return () => { container.replaceChildren(); };
}
