type SecureStorage = Pick<Storage, "getItem" | "removeItem" | "setItem">;

export const secureLocalStorePrefix = "osl-secure-local-store-v1:";

type SecureLocalStoreEnvelope = {
  v: 1;
  encoding: "utf8-b64url";
  body: string;
};

function isEnvelope(value: unknown): value is SecureLocalStoreEnvelope {
  return typeof value === "object"
    && value !== null
    && !Array.isArray(value)
    && (value as SecureLocalStoreEnvelope).v === 1
    && (value as SecureLocalStoreEnvelope).encoding === "utf8-b64url"
    && typeof (value as SecureLocalStoreEnvelope).body === "string";
}

function encodeUtf8Base64Url(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/gu, "-").replace(/\//gu, "_").replace(/=+$/u, "");
}

function decodeUtf8Base64Url(value: string): string | null {
  try {
    const padded = value.replace(/-/gu, "+").replace(/_/gu, "/")
      + "=".repeat((4 - value.length % 4) % 4);
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return new TextDecoder().decode(bytes);
  } catch {
    return null;
  }
}

export class SecureLocalStore {
  constructor(private readonly storage: SecureStorage) {}

  getItem(key: string): string | null {
    const raw = this.storage.getItem(this.storageKey(key));
    if (raw === null) return null;
    try {
      const envelope = JSON.parse(raw) as unknown;
      if (!isEnvelope(envelope)) return null;
      return decodeUtf8Base64Url(envelope.body);
    } catch {
      return null;
    }
  }

  setItem(key: string, value: string): void {
    const envelope: SecureLocalStoreEnvelope = {
      v: 1,
      encoding: "utf8-b64url",
      body: encodeUtf8Base64Url(value),
    };
    this.storage.setItem(this.storageKey(key), JSON.stringify(envelope));
  }

  removeItem(key: string): void {
    this.storage.removeItem(this.storageKey(key));
  }

  migrateLegacyItem<T>(
    key: string,
    legacyKey: string,
    parse: (raw: string) => T | null,
  ): T | null {
    const current = this.getItem(key);
    if (current !== null) return parse(current);

    const legacy = this.storage.getItem(legacyKey);
    if (legacy === null) return null;
    const parsed = parse(legacy);
    if (parsed === null) return null;
    this.setItem(key, legacy);
    this.storage.removeItem(legacyKey);
    return parsed;
  }

  private storageKey(key: string): string {
    return `${secureLocalStorePrefix}${key}`;
  }
}
