/**
 * In-process journal of what the native backend actually said when it refused.
 *
 * Every narrow adapter in this app fails closed: it returns `null` or `false` so
 * a refused command can never be mistaken for a completed one. That stays
 * exactly as it was. What used to be thrown away is *why* the backend refused --
 * the Rust error string was caught and discarded, so a real cause (a keyserver
 * quota, a composer calibration refusal) reached the operator as an unrelated
 * generic sentence and could only be recovered by rebuilding Rust with
 * temporary instrumentation.
 *
 * Scope and privacy rules, deliberately narrow:
 *
 * - This is renderer memory only. Nothing here is persisted, hashed, uploaded,
 *   or attached to any request. `clearBackendFailures()` empties it.
 * - Only error strings are accepted. Draft text, decrypted plaintext, capsules,
 *   attachment bytes, nicknames, friend codes and recovery phrases are never
 *   recorded. `recordBackendFailure` takes the content arguments a command was
 *   given for one reason only: so it can redact any fragment of them that an
 *   error string echoed back.
 * - It does not make the backend more specific. Several native refusals share
 *   one deliberately uniform sentence so the wire cannot reveal which check
 *   failed; whatever Rust chose to say is what is recorded, unchanged.
 */

export type BackendFailureKind = "rejected" | "invalidResponse";

export interface BackendFailure {
  /** The Tauri command name, never an argument value. */
  readonly command: string;
  /** Sanitized backend message, or a fixed label when there was none. */
  readonly message: string;
  /** `rejected`: the command threw. `invalidResponse`: it resolved unusably. */
  readonly kind: BackendFailureKind;
  /** `Date.now()` of the most recent occurrence. */
  readonly at: number;
  /** Consecutive identical occurrences, so polling loops cannot flood. */
  readonly count: number;
}

export const BACKEND_FAILURE_WITHOUT_MESSAGE = "The backend refused without a readable message";
export const BACKEND_INVALID_RESPONSE = "The backend response failed OSL's own validation";

const MAX_MESSAGE_CHARS = 240;
const MAX_ENTRIES = 32;
const MAX_SCANNED_CONTENT_CHARS = 4_096;
const MIN_CONTENT_FRAGMENT_CHARS = 8;
const MIN_SHORT_CONTENT_CHARS = 3;
const REDACTED = "[redacted]";
/** Long unbroken runs are opaque material (capsules, codes, digests), not prose. */
const OPAQUE_TOKEN = /[A-Za-z0-9+/_-]{32,}/gu;
const CREDENTIAL_LABEL = /\b(Bearer|token|password|secret|private[ _-]?key|passphrase|recovery[ _-]?phrase)\b\s*[:=]?\s*\S+/giu;

function backendErrorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return "";
}

/**
 * Redact every run of `message` that also occurs in `content`. Bounded on both
 * sides: `message` is already short, and only the leading window of `content` is
 * scanned, because anything long enough for the tail to matter is opaque
 * material that the token rule above has already removed.
 */
function redactContentFragments(message: string, content: string): string {
  if (content.length < MIN_SHORT_CONTENT_CHARS) return message;
  const needle = (content.length > MAX_SCANNED_CONTENT_CHARS
    ? content.slice(0, MAX_SCANNED_CONTENT_CHARS)
    : content).toLowerCase();
  const lower = message.toLowerCase();
  if (content.length < MIN_CONTENT_FRAGMENT_CHARS) {
    return lower.includes(needle) ? message.split(new RegExp(escapeRegExp(needle), "giu")).join(REDACTED) : message;
  }
  const parts: string[] = [];
  let index = 0;
  while (index < message.length) {
    if (index + MIN_CONTENT_FRAGMENT_CHARS <= message.length
      && needle.includes(lower.slice(index, index + MIN_CONTENT_FRAGMENT_CHARS))) {
      let end = index + MIN_CONTENT_FRAGMENT_CHARS;
      while (end < message.length && needle.includes(lower.slice(index, end + 1))) end += 1;
      parts.push(REDACTED);
      index = end;
      continue;
    }
    parts.push(message[index] as string);
    index += 1;
  }
  return parts.join("");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

/**
 * Turn whatever the backend threw into one bounded, single-line string that
 * cannot carry user content back to a developer surface.
 */
export function sanitizeBackendMessage(
  error: unknown,
  contentArguments: readonly unknown[] = [],
  fallback: string = BACKEND_FAILURE_WITHOUT_MESSAGE,
): string {
  let text = backendErrorText(error)
    .replace(/[\u0000-\u001f\u007f]+/gu, " ")
    .replace(CREDENTIAL_LABEL, "$1 " + REDACTED)
    .replace(OPAQUE_TOKEN, REDACTED);
  for (const value of contentArguments) {
    if (typeof value === "string" && value.length > 0) text = redactContentFragments(text, value);
  }
  const collapsed = text.replace(/\s+/gu, " ").trim();
  if (!collapsed) return fallback;
  const characters = Array.from(collapsed);
  return characters.length <= MAX_MESSAGE_CHARS
    ? collapsed
    : `${characters.slice(0, MAX_MESSAGE_CHARS).join("")}…`;
}

const journal: BackendFailure[] = [];
const listeners = new Set<(failure: BackendFailure) => void>();
// Off in a shipping build, on for development and the Discord QA shell.
//
// Nothing here ever leaves the device, and the recorded strings are OSL's own
// refusals with draft content already redacted -- so this is not a leak. It is
// off in release anyway because a privacy tool should not write a running
// commentary of which internal checks are failing into a surface it does not
// need: the in-memory journal (`lastBackendFailure` / `onBackendFailure`)
// already gives every consumer the same detail without emitting anything.
//
// `VITE_OSL_DISCORD_QA_SHELL` is the same discriminator main.ts uses for the QA
// header strip, and the QA shell is itself a production-mode vite build, so
// `DEV` alone would not cover it.
let consoleMirrorEnabled =
  import.meta.env.MODE !== "test"
  && (import.meta.env.DEV || import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1");

/** Command names are compile-time literals; this only keeps that true. */
function safeCommandName(command: string): string {
  const cleaned = typeof command === "string" ? command.replace(/[^a-z0-9_]/giu, "").slice(0, 64) : "";
  return cleaned || "unknown_command";
}

function record(command: string, message: string, kind: BackendFailureKind): BackendFailure {
  const safeCommand = safeCommandName(command);
  const previous = journal[journal.length - 1];
  const repeat = previous !== undefined
    && previous.command === safeCommand
    && previous.message === message
    && previous.kind === kind;
  const failure: BackendFailure = {
    command: safeCommand,
    message,
    kind,
    at: Date.now(),
    count: repeat ? (previous as BackendFailure).count + 1 : 1,
  };
  if (repeat) journal[journal.length - 1] = failure;
  else {
    journal.push(failure);
    if (journal.length > MAX_ENTRIES) journal.shift();
    if (consoleMirrorEnabled) {
      console.error(`[osl] ${safeCommand} ${kind === "rejected" ? "was refused" : "returned an unusable response"}: ${message}`);
    }
  }
  for (const listener of listeners) {
    // A broken developer surface must never turn into a second failure.
    try { listener(failure); } catch { /* ignored */ }
  }
  return failure;
}

/** Record the backend's own refusal message for a command that threw. */
export function recordBackendFailure(
  command: string,
  error: unknown,
  contentArguments: readonly unknown[] = [],
): BackendFailure {
  return record(command, sanitizeBackendMessage(error, contentArguments), "rejected");
}

/**
 * Record that a command resolved but its response failed OSL's validation.
 * `detail` must be a fixed developer label chosen here, never response data.
 */
export function recordInvalidBackendResponse(command: string, detail?: string): BackendFailure {
  return record(command, sanitizeBackendMessage(detail ?? "", [], BACKEND_INVALID_RESPONSE), "invalidResponse");
}

/** Pass a parsed response through, recording it when validation rejected it. */
export function checkedBackendResponse<T>(command: string, value: T | null, detail?: string): T | null {
  if (value === null) recordInvalidBackendResponse(command, detail);
  return value;
}

/** Fail closed on an unusable response while keeping why it was unusable. */
export function rejectBackendResponse(command: string, detail?: string): null {
  recordInvalidBackendResponse(command, detail);
  return null;
}

/** Newest last. A copy, so a developer surface cannot mutate the journal. */
export function backendFailures(): BackendFailure[] {
  return journal.slice();
}

export function lastBackendFailure(command?: string): BackendFailure | null {
  for (let index = journal.length - 1; index >= 0; index -= 1) {
    const failure = journal[index] as BackendFailure;
    if (command === undefined || failure.command === command) return failure;
  }
  return null;
}

export function clearBackendFailures(): void {
  journal.length = 0;
}

/** Subscribe a developer surface. Returns its unsubscribe function. */
export function onBackendFailure(listener: (failure: BackendFailure) => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

/** Override the console mirror. Off in release builds by default; nothing leaves the device either way. */
export function setBackendFailureConsole(enabled: boolean): void {
  consoleMirrorEnabled = enabled;
}
