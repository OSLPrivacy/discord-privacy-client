/**
 * TASK 6804 — the on-disk recovery-kit file, and the only reader of it.
 *
 * A recovery kit is the one artefact that carries both twelve-word phrases off
 * the machine that made them. Loading one is therefore the single richest
 * opportunity to hand an attacker a hostile parser, and the single richest
 * opportunity to leak the words that are the account. Both are answered here:
 *
 *   * **Nothing about the file is trusted.** The reader is handed raw bytes and
 *     must decide, from those bytes alone, that this is an OSL recovery kit, of
 *     a version it understands, whose contents were not altered by one byte.
 *     Only then does it produce words.
 *   * **No refusal ever contains a word.** Every refusal in this module is a
 *     frozen constant. There is no template, no interpolation, and no `catch`
 *     that re-throws a parser's own message — a JSON parse error carries the
 *     offending text, which for this file is a recovery phrase.
 *
 * ## The format
 *
 * ```text
 * OSL-RECOVERY-KIT v1 sha256:<64 lowercase hex>\n{"userId":…,"identityWords":[…],"passwordWords":[…]}
 * ```
 *
 * One header line, then the payload, and nothing after it. The digest covers
 * the payload bytes exactly as they sit on disk rather than the parsed fields,
 * so a flipped byte anywhere in the payload — inside a word, inside the JSON
 * punctuation, or appended to the end — fails integrity. Digesting the parsed
 * fields instead would have let whitespace and trailing bytes through, which is
 * precisely the corruption a torn download or a half-written USB write leaves.
 */

/** The literal that must open the file. Mirrored in `apps/osl-hub/src/recovery_kit_upload.rs`. */
export const RECOVERY_KIT_FILE_TYPE = "OSL-RECOVERY-KIT";
/** The only version this build reads or writes. */
export const RECOVERY_KIT_FILE_VERSION = 1;
/** The extension the native picker filters on. */
export const RECOVERY_KIT_FILE_EXTENSION = "oslkit";
/** Both phrases are twelve words. This is the count the boxes are drawn from. */
export const RECOVERY_KIT_WORD_COUNT = 12;
/**
 * A kit is roughly 400 bytes. The ceiling exists so a picked 4 GB file is
 * refused by size before anything reads it into memory.
 */
export const RECOVERY_KIT_MAX_FILE_BYTES = 8192;

const HEADER_DIGEST_PREFIX = "sha256:";
const HEX_DIGEST_LENGTH = 64;

export interface RecoveryKitDocument {
  readonly userId: string;
  readonly identityWords: readonly string[];
  readonly passwordWords: readonly string[];
}

/**
 * Every refusal the load path can produce, frozen.
 *
 * Each one says what OSL will not do and states that nothing changed, and none
 * of them can carry a recovery word, a file path, or a parser's own text.
 */
export const RECOVERY_KIT_REFUSALS = {
  notAKit: "That file is not an OSL recovery kit. Nothing was changed.",
  wrongVersion: "That recovery kit was written by a different version of OSL. Nothing was changed.",
  damaged: "That recovery kit file is damaged. Nothing was changed.",
  tooLarge: "That file is too large to be an OSL recovery kit. Nothing was changed.",
  unreadable: "OSL could not read that file. Nothing was changed.",
  movedFile: "That file changed while OSL was reading it. Nothing was changed.",
  wrongIdentity: "That recovery kit belongs to a different OSL account. Nothing was changed.",
  unknownIdentity: "OSL could not confirm which account that recovery kit belongs to. Nothing was changed.",
} as const;

export type RecoveryKitRefusal = (typeof RECOVERY_KIT_REFUSALS)[keyof typeof RECOVERY_KIT_REFUSALS];

export type RecoveryKitParse =
  | { readonly ok: true; readonly document: RecoveryKitDocument }
  | { readonly ok: false; readonly refusal: RecoveryKitRefusal };

/** A digest function over raw bytes, returning lowercase hex. */
export type DigestHex = (bytes: Uint8Array) => Promise<string>;

export function isRecoveryKitWord(value: unknown): value is string {
  return typeof value === "string" && /^[a-z]{3,12}$/u.test(value);
}

function isWordList(value: unknown): value is string[] {
  return Array.isArray(value)
    && value.length === RECOVERY_KIT_WORD_COUNT
    && value.every((word) => isRecoveryKitWord(word));
}

function isUserId(value: unknown): value is string {
  return typeof value === "string" && /^osl_[0-9a-f]{16,64}$/u.test(value);
}

/**
 * The payload exactly as it is written and exactly as it is digested.
 *
 * Key order is part of the format: a reader that re-serialised the parsed
 * object to check the digest would have to agree with the writer about key
 * order anyway, so the order is stated once, here.
 */
export function recoveryKitPayloadText(document: RecoveryKitDocument): string {
  return JSON.stringify({
    userId: document.userId,
    identityWords: [...document.identityWords],
    passwordWords: [...document.passwordWords],
  });
}

/** Build the bytes an exporter writes. The reader below is its exact inverse. */
export async function buildRecoveryKitFileBytes(
  document: RecoveryKitDocument,
  digestHex: DigestHex,
): Promise<Uint8Array> {
  const payload = new TextEncoder().encode(recoveryKitPayloadText(document));
  const digest = await digestHex(payload);
  const header = new TextEncoder().encode(
    `${RECOVERY_KIT_FILE_TYPE} v${RECOVERY_KIT_FILE_VERSION} ${HEADER_DIGEST_PREFIX}${digest}\n`,
  );
  const bytes = new Uint8Array(header.length + payload.length);
  bytes.set(header, 0);
  bytes.set(payload, header.length);
  return bytes;
}

function decodeUtf8Strictly(bytes: Uint8Array): string | null {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

/**
 * Read a picked file's bytes as a recovery kit.
 *
 * The order of the checks is the order of the refusals a person should see:
 * "this is not a kit" before "this kit is from another version" before "this
 * kit is damaged". Reversing them would tell somebody holding a JPEG that
 * their recovery kit was damaged.
 */
export async function parseRecoveryKitFile(
  bytes: Uint8Array,
  digestHex: DigestHex,
): Promise<RecoveryKitParse> {
  if (bytes.length > RECOVERY_KIT_MAX_FILE_BYTES) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.tooLarge };

  const newline = bytes.indexOf(0x0a);
  if (newline < 0) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.notAKit };
  const header = decodeUtf8Strictly(bytes.subarray(0, newline));
  if (header === null) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.notAKit };

  const fields = header.split(" ");
  if (fields.length !== 3 || fields[0] !== RECOVERY_KIT_FILE_TYPE) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.notAKit };
  }
  const version = fields[1] ?? "";
  if (!/^v[0-9]{1,4}$/u.test(version)) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.notAKit };
  if (Number(version.slice(1)) !== RECOVERY_KIT_FILE_VERSION) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.wrongVersion };
  }
  const declared = fields[2] ?? "";
  if (!declared.startsWith(HEADER_DIGEST_PREFIX)) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.notAKit };
  const declaredDigest = declared.slice(HEADER_DIGEST_PREFIX.length);
  if (declaredDigest.length !== HEX_DIGEST_LENGTH || !/^[0-9a-f]+$/u.test(declaredDigest)) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  }

  // Integrity before shape. A damaged payload must never reach the JSON
  // parser: `JSON.parse` puts the offending text in its own error message, and
  // the offending text here is a recovery phrase.
  const payload = bytes.subarray(newline + 1);
  if (payload.length === 0) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  const actualDigest = await digestHex(payload);
  if (actualDigest !== declaredDigest) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };

  const text = decodeUtf8Strictly(payload);
  if (text === null) return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  }
  const record = raw as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  const expected = ["identityWords", "passwordWords", "userId"];
  if (keys.length !== expected.length || keys.some((key, index) => key !== expected[index])) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  }
  if (!isUserId(record.userId) || !isWordList(record.identityWords) || !isWordList(record.passwordWords)) {
    return { ok: false, refusal: RECOVERY_KIT_REFUSALS.damaged };
  }
  return {
    ok: true,
    document: {
      userId: record.userId,
      identityWords: Object.freeze([...record.identityWords]),
      passwordWords: Object.freeze([...record.passwordWords]),
    },
  };
}
