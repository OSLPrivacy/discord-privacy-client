/**
 * TASK 6804 — "Upload recovery kit" on Forgot Password and Restore Account.
 *
 * Both screens used to offer one free-text box for a twelve-word phrase and
 * nothing else, so the only way to use a saved recovery kit was to retype
 * twenty-four words from a screenshot. This module is the other way in: pick
 * the kit file with the operating system's own picker, prove it is a kit, and
 * fill the twelve numbered boxes from it.
 *
 * Four rules shape everything below.
 *
 * 1. **The renderer never names a file.** `pickFile` returns a selection that
 *    the native side has already frozen — a path it resolved, a length it
 *    measured and a digest it took, all at pick time. The renderer re-digests
 *    the bytes it is about to validate and refuses unless they match that
 *    frozen digest, so the bytes that were weighed are provably the bytes that
 *    are read. A file swapped under the picker between the dialog closing and
 *    the read landing is caught here rather than imported.
 *
 * 2. **Nothing happens until everything has passed.** Cancel, a non-kit file, a
 *    wrong version, a single corrupt byte and a kit belonging to another
 *    account all return before a word is produced, so the boxes, the account
 *    and the backend are untouched. The identity comparison in particular runs
 *    *before* the phrase reaches the authenticated importer: a wrong-identity
 *    kit must not spend a recovery attempt or move a lockout counter.
 *
 * 3. **A refusal is a sentence, not a diagnosis.** Every message comes from the
 *    frozen table in `recovery-kit-file-6804.ts`. Nothing interpolates a word,
 *    a path, a digest or a caught exception's own text.
 *
 * 4. **The identity comparison cannot be vacuous.** A load is refused unless at
 *    least one authority — the local account this device already holds, or the
 *    identity the backend derives from the kit's own identity phrase — reported
 *    a user id, and every authority that reported one agrees with the kit. An
 *    unknown identity is a refusal, not a pass.
 */

import {
  RECOVERY_KIT_MAX_FILE_BYTES,
  RECOVERY_KIT_REFUSALS,
  RECOVERY_KIT_WORD_COUNT,
  parseRecoveryKitFile,
  type DigestHex,
  type RecoveryKitDocument,
  type RecoveryKitRefusal,
} from "./recovery-kit-file-6804";

/** Which of the kit's two phrases a screen is asking for. */
export type RecoveryKitSlot = "password" | "identity";

export const RECOVERY_KIT_UPLOAD_CHIP_LABEL = "Upload recovery kit";

/**
 * What the native picker froze at selection time. `path` and `sha256` are the
 * freeze: they are decided once, by Rust, and never recomputed from anything
 * the renderer could influence.
 */
export interface PickedRecoveryKitFile {
  readonly path: string;
  readonly sha256: string;
  readonly sizeBytes: number;
  readonly bytesBase64: string;
  /** The account this device already holds, or null when it holds none. */
  readonly localUserId: string | null;
}

/** The frozen receipt a successful load carries forward to the importer. */
export interface FrozenRecoveryKitSelection {
  readonly path: string;
  readonly sha256: string;
  readonly sizeBytes: number;
}

/**
 * Who is allowed to say which account a kit belongs to. Both fields come from
 * the backend; the renderer supplies neither.
 */
export interface RecoveryKitIdentityCheck {
  /** The account already on this device, or null on a device with none. */
  readonly localUserId: string | null;
  /** The account the backend derives from the kit's identity phrase, or null. */
  readonly derivedUserId: string | null;
}

export interface RecoveryKitUploadDependencies {
  /** The installed native picker. `null` means the person cancelled. */
  pickFile(): Promise<PickedRecoveryKitFile | null>;
  /** The byte reader: the frozen selection in, its exact bytes out. */
  readBytes(picked: PickedRecoveryKitFile): Promise<Uint8Array>;
  /** Lowercase hex SHA-256. */
  digestHex: DigestHex;
  /** The identity comparison's authorities, asked only after the file passed. */
  resolveIdentity(
    document: RecoveryKitDocument,
    picked: PickedRecoveryKitFile,
  ): Promise<RecoveryKitIdentityCheck>;
}

export type RecoveryKitUploadOutcome =
  | { readonly kind: "cancelled" }
  | { readonly kind: "refused"; readonly message: RecoveryKitRefusal }
  | {
    readonly kind: "loaded";
    readonly slot: RecoveryKitSlot;
    readonly words: readonly string[];
    readonly userId: string;
    readonly selection: FrozenRecoveryKitSelection;
  };

function refuse(message: RecoveryKitRefusal): RecoveryKitUploadOutcome {
  return { kind: "refused", message };
}

/** Decode base64 without pulling the bytes through a string the length of the file. */
export function decodeRecoveryKitBytes(base64: string): Uint8Array | null {
  if (!/^[A-Za-z0-9+/]*={0,2}$/u.test(base64)) return null;
  try {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    return bytes;
  } catch {
    return null;
  }
}

/**
 * Every present authority must agree with the kit, and at least one must be
 * present. Silence from both is `unknownIdentity`, never a pass.
 */
export function compareRecoveryKitIdentity(
  document: RecoveryKitDocument,
  check: RecoveryKitIdentityCheck,
): RecoveryKitRefusal | null {
  const claims = [check.localUserId, check.derivedUserId].filter(
    (value): value is string => typeof value === "string" && value.length > 0,
  );
  if (claims.length === 0) return RECOVERY_KIT_REFUSALS.unknownIdentity;
  if (claims.some((claim) => claim !== document.userId)) return RECOVERY_KIT_REFUSALS.wrongIdentity;
  return null;
}

/**
 * The whole load: pick, read, prove the bytes are the picked bytes, prove the
 * bytes are a kit of this version, prove the kit is this account's, and only
 * then hand back twelve words.
 */
export async function loadRecoveryKitFile(
  slot: RecoveryKitSlot,
  dependencies: RecoveryKitUploadDependencies,
): Promise<RecoveryKitUploadOutcome> {
  let picked: PickedRecoveryKitFile | null;
  try {
    picked = await dependencies.pickFile();
  } catch {
    return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  }
  if (picked === null) return { kind: "cancelled" };
  if (
    typeof picked.path !== "string" || picked.path.length === 0
    || typeof picked.sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(picked.sha256)
    || !Number.isSafeInteger(picked.sizeBytes) || picked.sizeBytes <= 0
    || typeof picked.bytesBase64 !== "string"
  ) {
    return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  }
  if (picked.sizeBytes > RECOVERY_KIT_MAX_FILE_BYTES) return refuse(RECOVERY_KIT_REFUSALS.tooLarge);

  let bytes: Uint8Array;
  try {
    bytes = await dependencies.readBytes(picked);
  } catch {
    return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  }
  if (!(bytes instanceof Uint8Array) || bytes.length === 0) return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  // The freeze. Length and digest were both decided at pick time; a mismatch
  // in either means the bytes about to be validated are not the bytes that
  // were chosen.
  if (bytes.length !== picked.sizeBytes) return refuse(RECOVERY_KIT_REFUSALS.movedFile);
  let readDigest: string;
  try {
    readDigest = await dependencies.digestHex(bytes);
  } catch {
    return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  }
  if (readDigest !== picked.sha256) return refuse(RECOVERY_KIT_REFUSALS.movedFile);

  const parsed = await parseRecoveryKitFile(bytes, dependencies.digestHex).catch(() => null);
  if (parsed === null) return refuse(RECOVERY_KIT_REFUSALS.unreadable);
  if (!parsed.ok) return refuse(parsed.refusal);

  let check: RecoveryKitIdentityCheck;
  try {
    check = await dependencies.resolveIdentity(parsed.document, picked);
  } catch {
    return refuse(RECOVERY_KIT_REFUSALS.unknownIdentity);
  }
  const mismatch = compareRecoveryKitIdentity(parsed.document, check);
  if (mismatch !== null) return refuse(mismatch);

  const words = slot === "password" ? parsed.document.passwordWords : parsed.document.identityWords;
  return {
    kind: "loaded",
    slot,
    words: Object.freeze([...words]),
    userId: parsed.document.userId,
    selection: { path: picked.path, sha256: picked.sha256, sizeBytes: picked.sizeBytes },
  };
}

/* ------------------------------------------------------------------ boxes */

/**
 * The twelve numbered boxes, as state.
 *
 * `loadedFrom` is the frozen receipt of the file the current words came from,
 * and it is null the moment anything is typed by hand. It exists so a caller
 * can say which file is on screen without holding the words a second time.
 */
export interface RecoveryWordBoxes {
  readonly words: readonly string[];
  readonly refusal: string;
  readonly loadedFrom: FrozenRecoveryKitSelection | null;
}

export function emptyRecoveryWordBoxes(): RecoveryWordBoxes {
  return { words: Object.freeze(Array<string>(RECOVERY_KIT_WORD_COUNT).fill("")), refusal: "", loadedFrom: null };
}

export function typeRecoveryWord(boxes: RecoveryWordBoxes, position: number, value: string): RecoveryWordBoxes {
  if (!Number.isInteger(position) || position < 1 || position > RECOVERY_KIT_WORD_COUNT) return boxes;
  const words = [...boxes.words];
  words[position - 1] = value.trim().toLowerCase();
  return { words: Object.freeze(words), refusal: "", loadedFrom: null };
}

/**
 * Apply a load outcome to the boxes.
 *
 * Cancellation returns the very same object: not a copy, not a cleared
 * refusal, nothing. A bad file returns the same words with a refusal beside
 * them, so a person who had typed eleven words by hand does not lose them for
 * having opened the wrong file.
 */
export function applyRecoveryKitOutcome(
  boxes: RecoveryWordBoxes,
  outcome: RecoveryKitUploadOutcome,
): RecoveryWordBoxes {
  if (outcome.kind === "cancelled") return boxes;
  if (outcome.kind === "refused") return { ...boxes, refusal: outcome.message };
  if (outcome.words.length !== RECOVERY_KIT_WORD_COUNT) return { ...boxes, refusal: RECOVERY_KIT_REFUSALS.damaged };
  return { words: Object.freeze([...outcome.words]), refusal: "", loadedFrom: outcome.selection };
}

/** The phrase the existing submit paths already know how to send. */
export function recoveryWordBoxesPhrase(boxes: RecoveryWordBoxes): string {
  return boxes.words.every((word) => word.length > 0) ? boxes.words.join(" ") : "";
}

export function recoveryWordBoxesComplete(boxes: RecoveryWordBoxes): boolean {
  return recoveryWordBoxesPhrase(boxes).length > 0;
}

/* ----------------------------------------------------------------- markup */

function escapeAttribute(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

export interface RecoveryWordBoxesConfig {
  /** Which phrase this screen wants out of the kit. */
  readonly slot: RecoveryKitSlot;
  /** Prefix for the twelve box ids, e.g. `identity-recovery`. */
  readonly idPrefix: string;
  /** The id the existing submit path already reads the whole phrase from. */
  readonly phraseFieldId: string;
  /** The form field name the existing submit path already reads. */
  readonly phraseFieldName: string;
  readonly label: string;
  readonly hint: string;
}

/**
 * Twelve numbered boxes, the upload chip, one refusal line, and the hidden
 * aggregate the existing submit paths read.
 *
 * The aggregate is deliberately the *same* id and name the single textarea used
 * to carry, so every caller, test and ledger entry that already reaches for the
 * phrase keeps working while the visible surface becomes twelve boxes.
 */
export function recoveryWordBoxesMarkup(
  config: RecoveryWordBoxesConfig,
  boxes: RecoveryWordBoxes,
): string {
  const cells = Array.from({ length: RECOVERY_KIT_WORD_COUNT }, (_unused, index) => {
    const position = index + 1;
    const id = `${config.idPrefix}-word-${position}`;
    const value = escapeAttribute(boxes.words[index] ?? "");
    return `<label class="recovery-word-box" for="${id}"><span class="recovery-word-number" aria-hidden="true">${position}</span>`
      + `<input id="${id}" class="recovery-word-input" data-recovery-word="${position}" data-recovery-word-slot="${config.slot}"`
      + ` name="recoveryWord${position}" type="text" inputmode="text" autocomplete="off" autocapitalize="none"`
      + ` spellcheck="false" maxlength="12" aria-label="Recovery word ${position}" value="${value}"/></label>`;
  }).join("");

  const refusal = boxes.refusal
    ? `<p class="unlock-error recovery-kit-refusal" data-recovery-kit-refusal="${config.slot}" role="alert">${escapeAttribute(boxes.refusal)}</p>`
    : `<p class="unlock-error recovery-kit-refusal" data-recovery-kit-refusal="${config.slot}" role="alert" hidden></p>`;

  return `<span class="restore-label-row"><label for="${config.idPrefix}-word-1">${config.label}</label><em>${config.hint}</em></span>`
    + `<div class="recovery-word-grid" data-recovery-word-grid="${config.slot}">${cells}</div>`
    + `<div class="recovery-kit-chip-row"><button class="recovery-kit-chip" type="button" data-recovery-kit-upload="${config.slot}">`
    + `<svg class="recovery-kit-chip-icon" viewBox="0 0 20 20" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">`
    + `<path d="M10 13.5V3.5"/><path d="M6.25 7.25 10 3.5l3.75 3.75"/><path d="M3.5 13v2.5a1 1 0 0 0 1 1h11a1 1 0 0 0 1-1V13"/></svg>`
    + `<span>${RECOVERY_KIT_UPLOAD_CHIP_LABEL}</span></button></div>`
    + refusal
    + `<input type="hidden" id="${config.phraseFieldId}" name="${config.phraseFieldName}" value="${escapeAttribute(recoveryWordBoxesPhrase(boxes))}"/>`;
}
