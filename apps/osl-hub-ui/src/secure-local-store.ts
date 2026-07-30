type RawLocalStore = Pick<Storage, "getItem" | "setItem">;
type VitestApi = Pick<typeof import("vitest"), "describe" | "expect" | "it">;

declare global {
  interface ImportMeta {
    readonly vitest?: VitestApi;
  }
}

export interface SecureLocalStoreOptions {
  readonly storage: RawLocalStore;
  readonly key: CryptoKey;
  readonly namespace?: string;
  readonly subtle?: SubtleCrypto;
  readonly randomBytes?: (bytes: Uint8Array) => Uint8Array;
}

interface SecureLocalStoreEnvelope {
  readonly v: 1;
  readonly alg: "AES-256-GCM";
  readonly nonce: string;
  readonly ciphertext: string;
}

const ENVELOPE_VERSION = 1;
const ALGORITHM = "AES-256-GCM";
const AES_GCM_NONCE_BYTES = 12;
const RAW_KEY_BYTES = 32;
const MAX_LOGICAL_KEY_BYTES = 512;
const MAX_PLAINTEXT_BYTES = 1024 * 1024;
const MAX_CIPHERTEXT_BYTES = MAX_PLAINTEXT_BYTES + 16;
const DEFAULT_NAMESPACE = "osl-secure-local-store-v1";

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder();

export class SecureLocalStoreError extends Error {
  constructor() {
    super("secure local store refused");
    this.name = "SecureLocalStoreError";
  }
}

export class SecureLocalStore {
  readonly #storage: RawLocalStore;
  readonly #key: CryptoKey;
  readonly #namespace: string;
  readonly #subtle: SubtleCrypto;
  readonly #randomBytes: (bytes: Uint8Array) => Uint8Array;

  constructor(options: SecureLocalStoreOptions) {
    this.#storage = options.storage;
    this.#key = options.key;
    this.#namespace = options.namespace ?? DEFAULT_NAMESPACE;
    this.#subtle = options.subtle ?? globalThis.crypto?.subtle;
    this.#randomBytes = options.randomBytes ?? ((bytes) => globalThis.crypto.getRandomValues(bytes));
    validateNamespace(this.#namespace);
    if (!this.#subtle) throw new SecureLocalStoreError();
  }

  static async importRawKey(
    rawKey: Uint8Array,
    subtle: SubtleCrypto = globalThis.crypto?.subtle,
  ): Promise<CryptoKey> {
    if (!subtle || rawKey.byteLength !== RAW_KEY_BYTES) throw new SecureLocalStoreError();
    try {
      return await subtle.importKey("raw", toArrayBuffer(rawKey), { name: "AES-GCM", length: 256 }, false, [
        "decrypt",
        "encrypt",
      ]);
    } catch {
      throw new SecureLocalStoreError();
    }
  }

  async setItem(logicalKey: string, plaintext: string): Promise<void> {
    const plaintextBytes = textEncoder.encode(plaintext);
    if (plaintextBytes.byteLength > MAX_PLAINTEXT_BYTES) throw new SecureLocalStoreError();

    const nonce = this.#randomBytes(new Uint8Array(AES_GCM_NONCE_BYTES));
    if (nonce.byteLength !== AES_GCM_NONCE_BYTES) throw new SecureLocalStoreError();

    try {
      const ciphertext = new Uint8Array(await this.#subtle.encrypt(
        {
          name: "AES-GCM",
          iv: toArrayBuffer(nonce),
          additionalData: toArrayBuffer(this.#aad(logicalKey)),
          tagLength: 128,
        },
        this.#key,
        toArrayBuffer(plaintextBytes),
      ));
      if (ciphertext.byteLength > MAX_CIPHERTEXT_BYTES) throw new SecureLocalStoreError();
      const envelope: SecureLocalStoreEnvelope = {
        v: ENVELOPE_VERSION,
        alg: ALGORITHM,
        nonce: encodeBase64Url(nonce),
        ciphertext: encodeBase64Url(ciphertext),
      };
      this.#storage.setItem(this.#storageKey(logicalKey), JSON.stringify(envelope));
    } catch (error) {
      if (error instanceof SecureLocalStoreError) throw error;
      throw new SecureLocalStoreError();
    }
  }

  async getItem(logicalKey: string): Promise<string | null> {
    const raw = this.#storage.getItem(this.#storageKey(logicalKey));
    if (raw === null) return null;

    try {
      const envelope = parseEnvelope(raw);
      const nonce = decodeBase64Url(envelope.nonce, AES_GCM_NONCE_BYTES);
      const ciphertext = decodeBase64Url(envelope.ciphertext, MAX_CIPHERTEXT_BYTES);
      const plaintext = await this.#subtle.decrypt(
        {
          name: "AES-GCM",
          iv: toArrayBuffer(nonce),
          additionalData: toArrayBuffer(this.#aad(logicalKey)),
          tagLength: 128,
        },
        this.#key,
        toArrayBuffer(ciphertext),
      );
      if (plaintext.byteLength > MAX_PLAINTEXT_BYTES) throw new SecureLocalStoreError();
      return textDecoder.decode(plaintext);
    } catch (error) {
      if (error instanceof SecureLocalStoreError) throw error;
      throw new SecureLocalStoreError();
    }
  }

  #storageKey(logicalKey: string): string {
    validateLogicalKey(logicalKey);
    return `${this.#namespace}:${encodeBase64Url(textEncoder.encode(logicalKey))}`;
  }

  #aad(logicalKey: string): Uint8Array {
    validateLogicalKey(logicalKey);
    return textEncoder.encode(`${this.#namespace}\u0000${logicalKey}`);
  }
}

function validateNamespace(value: string): void {
  if (!/^[A-Za-z0-9._:-]{1,96}$/u.test(value)) throw new SecureLocalStoreError();
}

function validateLogicalKey(value: string): void {
  if (
    value.length === 0
    || textEncoder.encode(value).byteLength > MAX_LOGICAL_KEY_BYTES
    || /[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]/u.test(value)
  ) {
    throw new SecureLocalStoreError();
  }
}

function parseEnvelope(raw: string): SecureLocalStoreEnvelope {
  const parsed: unknown = JSON.parse(raw);
  if (
    typeof parsed !== "object"
    || parsed === null
    || Object.keys(parsed).sort().join(",") !== "alg,ciphertext,nonce,v"
  ) {
    throw new SecureLocalStoreError();
  }
  const envelope = parsed as Record<string, unknown>;
  if (
    envelope.v !== ENVELOPE_VERSION
    || envelope.alg !== ALGORITHM
    || typeof envelope.nonce !== "string"
    || typeof envelope.ciphertext !== "string"
  ) {
    throw new SecureLocalStoreError();
  }
  return envelope as unknown as SecureLocalStoreEnvelope;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

function encodeBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/gu, "-").replace(/\//gu, "_").replace(/=+$/u, "");
}

function decodeBase64Url(value: string, maxBytes: number): Uint8Array {
  if (!/^[A-Za-z0-9_-]*$/u.test(value)) throw new SecureLocalStoreError();
  const padded = `${value.replace(/-/gu, "+").replace(/_/gu, "/")}${"=".repeat((4 - value.length % 4) % 4)}`;
  const binary = atob(padded);
  if (binary.length > maxBytes) throw new SecureLocalStoreError();
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function registerSecureLocalStoreTests({ describe, expect, it }: VitestApi): void {
  class MemoryStorage {
    readonly values = new Map<string, string>();
    getItem(key: string): string | null { return this.values.get(key) ?? null; }
    setItem(key: string, value: string): void { this.values.set(key, value); }
  }

  async function testStore(randomByte = 7): Promise<{ storage: MemoryStorage; store: SecureLocalStore }> {
    const storage = new MemoryStorage();
    const rawKey = new Uint8Array(RAW_KEY_BYTES);
    rawKey.fill(42);
    const key = await SecureLocalStore.importRawKey(rawKey);
    const store = new SecureLocalStore({
      storage,
      key,
      randomBytes: (bytes) => {
        bytes.fill(randomByte);
        return bytes;
      },
    });
    return { storage, store };
  }

  describe("SecureLocalStore", () => {
    it("stores an AES-GCM envelope instead of plaintext", async () => {
      const { storage, store } = await testStore();
      await store.setItem("draft", "private message");

      const entries = [...storage.values.entries()];
      expect(entries).toHaveLength(1);
      expect(entries[0][0]).not.toBe("draft");
      expect(entries[0][1]).not.toContain("private message");
      expect(JSON.parse(entries[0][1])).toMatchObject({ v: 1, alg: "AES-256-GCM" });
      await expect(store.getItem("draft")).resolves.toBe("private message");
    });

    it("binds ciphertext to its logical key and refuses swapped entries", async () => {
      const { storage, store } = await testStore();
      await store.setItem("one", "secret");
      const encrypted = storage.values.values().next().value as string;

      await store.setItem("two", "other");
      const twoKey = [...storage.values.keys()].find((key) => key.endsWith("dHdv"));
      expect(twoKey).toBeTruthy();
      storage.values.set(twoKey as string, encrypted);

      await expect(store.getItem("two")).rejects.toThrow(SecureLocalStoreError);
    });

    it("returns null only for absent items, not malformed stored data", async () => {
      const { storage, store } = await testStore();
      await expect(store.getItem("missing")).resolves.toBeNull();
      storage.values.set("osl-secure-local-store-v1:YnJva2Vu", "{\"v\":1}");
      await expect(store.getItem("broken")).rejects.toThrow(SecureLocalStoreError);
    });

    it("refuses invalid keys, non-256-bit raw keys, and bad nonce sources", async () => {
      await expect(SecureLocalStore.importRawKey(new Uint8Array(31))).rejects.toThrow(SecureLocalStoreError);

      const rawKey = new Uint8Array(RAW_KEY_BYTES);
      rawKey.fill(9);
      const key = await SecureLocalStore.importRawKey(rawKey);
      const badNonceStore = new SecureLocalStore({
        storage: new MemoryStorage(),
        key,
        randomBytes: () => new Uint8Array(8),
      });

      await expect(badNonceStore.setItem("ok", "value")).rejects.toThrow(SecureLocalStoreError);
      await expect(badNonceStore.getItem("bad\u202e")).rejects.toThrow(SecureLocalStoreError);
    });
  });
}

if (import.meta.vitest) {
  registerSecureLocalStoreTests(import.meta.vitest);
}
