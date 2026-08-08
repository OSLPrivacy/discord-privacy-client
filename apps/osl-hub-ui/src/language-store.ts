/**
 * TASK 3161 - connects every screen to the chosen language.
 *
 * Screens no longer hold their own words: they call `getScreenWords(screen)`
 * (backed by `loadScreenWords`) and render whatever comes back. This module
 * is the ONE place that remembers the current language and the words each
 * loaded screen last received for it.
 *
 * The whole point of `setLanguage` re-fetching every already-loaded screen
 * and notifying subscribers, rather than a caller reloading the app, is the
 * task's second bar: a language change must change what is shown WITHOUT a
 * restart. A subscriber is normally a screen's own re-render function, so
 * calling `setLanguage` mid-session repaints every screen currently on
 * screen with the new language's words, in the same running app instance.
 */
import { invoke } from "@tauri-apps/api/core";

export interface ScreenWords {
  language: string;
  screen: string;
  words: Record<string, string>;
}

type Listener = () => void;

let currentLanguage = "en";
const wordsByScreen = new Map<string, Record<string, string>>();
const listeners = new Set<Listener>();

function notify(): void {
  for (const listener of listeners) listener();
}

/** Registers a callback fired after every language or word-set change. Returns an unsubscribe function. */
export function subscribeLanguage(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getLanguage(): string {
  return currentLanguage;
}

/** The most recently loaded words for `screen`, or `undefined` if it has never been loaded. */
export function getScreenWords(screen: string): Record<string, string> | undefined {
  return wordsByScreen.get(screen);
}

/** Reads the persisted language choice from the backend and adopts it as current. Call once at startup. */
export async function initLanguage(): Promise<string> {
  currentLanguage = await invoke<string>("osl_get_language_choice");
  return currentLanguage;
}

/** Fetches `screen`'s words in the current language, caches them, and notifies subscribers. */
export async function loadScreenWords(screen: string): Promise<Record<string, string>> {
  const result = await invoke<ScreenWords>("osl_read_screen_words", { screen });
  wordsByScreen.set(screen, result.words);
  notify();
  return result.words;
}

/**
 * Saves the new language choice, then re-fetches words for every screen
 * that was already loaded, so a screen already on screen updates without
 * anyone re-navigating to it or reloading the app.
 */
export async function setLanguage(language: string): Promise<string> {
  const saved = await invoke<string>("osl_save_language_choice", { language });
  currentLanguage = saved;
  await Promise.all([...wordsByScreen.keys()].map((screen) => loadScreenWords(screen)));
  notify();
  return saved;
}

/** Test-only: clears in-memory state so each test starts from a fresh store. */
export function resetLanguageStoreForTest(): void {
  currentLanguage = "en";
  wordsByScreen.clear();
  listeners.clear();
}
