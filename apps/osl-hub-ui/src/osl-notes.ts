import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";

export interface OslNote {
  id: string;
  title: string;
  body: string;
  createdAt: number;
  updatedAt: number;
}

export interface OslNoteInput {
  id: string | null;
  title: string;
  body: string;
}

const noteKeys = ["id", "title", "body", "createdAt", "updatedAt"];
const noteId = /^[a-f0-9]{32}$/u;

export function parseOslNote(value: unknown): OslNote | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const note = value as Record<string, unknown>;
  if (Object.keys(note).length !== noteKeys.length || !Object.keys(note).every((key) => noteKeys.includes(key))) return null;
  if (typeof note.id !== "string" || !noteId.test(note.id)) return null;
  if (typeof note.title !== "string" || note.title.length > 240 || note.title.includes("\0")) return null;
  if (typeof note.body !== "string" || note.body.length > 262_144 || note.body.includes("\0")) return null;
  if (!Number.isSafeInteger(note.createdAt) || Number(note.createdAt) <= 0) return null;
  if (!Number.isSafeInteger(note.updatedAt) || Number(note.updatedAt) < Number(note.createdAt)) return null;
  return note as unknown as OslNote;
}

export function parseOslNotes(value: unknown): OslNote[] | null {
  if (!Array.isArray(value) || value.length > 5_000) return null;
  const parsed = value.map(parseOslNote);
  if (parsed.some((note) => note === null)) return null;
  const notes = parsed as OslNote[];
  return new Set(notes.map((note) => note.id)).size === notes.length ? notes : null;
}

export async function listOslNotes(): Promise<OslNote[] | null> {
  if (!isTauriRuntime()) return null;
  return parseOslNotes(await invoke("list_osl_notes"));
}

export async function saveOslNote(input: OslNoteInput): Promise<OslNote | null> {
  if (!isTauriRuntime()) return null;
  return parseOslNote(await invoke("save_osl_note", { input }));
}

export async function deleteOslNote(id: string): Promise<boolean> {
  if (!isTauriRuntime() || !noteId.test(id)) return false;
  return await invoke("delete_osl_note", { noteId: id }) === true;
}

export function notesWorkspaceMarkup(
  notes: OslNote[],
  activeId: string | null,
  loading: boolean,
  error: string,
): string {
  const escape = (text: string) => text.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
  const active = notes.find((note) => note.id === activeId) ?? notes[0] ?? null;
  const list = notes.map((note) => `<button class="notes-list-item ${note.id === active?.id ? "active" : ""}" data-note-id="${note.id}" type="button"><strong>${escape(note.title || "Untitled")}</strong><small>${new Date(note.updatedAt).toLocaleDateString()}</small></button>`).join("");
  const editor = active
    ? `<form class="notes-editor" data-notes-editor><label>Title<input id="note-title" maxlength="240" value="${escape(active.title)}" autocomplete="off"/></label><label>Note<textarea id="note-body" maxlength="262144" spellcheck="true">${escape(active.body)}</textarea></label><footer><span id="notes-save-state" role="status">Saved locally</span><button class="button danger compact" data-note-delete="${active.id}" type="button">Delete</button></footer></form>`
    : `<div class="notes-empty"><strong>No notes yet</strong><p>Create one to start a private, encrypted notebook.</p><button class="button primary" data-notes-new type="button">Create note</button></div>`;
  return `<main class="content-viewport notes-page" id="route-heading" tabindex="-1"><header class="notes-header"><div><button class="text-button" data-route="home" type="button">Back</button><h1>OSL Notes</h1><p>Encrypted on this device and scoped to the unlocked identity.</p></div><button class="button primary" data-notes-new type="button">New note</button></header>${error ? `<p class="form-status error" role="alert">${escape(error)}</p>` : ""}<div class="notes-workspace"><aside aria-label="Notes">${loading ? `<p>Opening encrypted notes…</p>` : list || `<p>No notes</p>`}</aside>${editor}</div></main>`;
}
