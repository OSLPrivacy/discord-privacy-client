export interface WhatsAppProtectedOpenReceipt {
  provider: "whatsapp";
  status: "opened";
  plaintext: string;
  personToPersonE2ee: true;
  contextVerified: true;
  contextBindingSha256: string;
  providerHistoryChanged: false;
  providerStorageRead: false;
}

const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  typeof value === "object"
  && value !== null
  && !Array.isArray(value)
  && Object.keys(value).sort().join(",") === [...keys].sort().join(",");

export function parseWhatsAppProtectedOpenReceipt(raw: unknown): WhatsAppProtectedOpenReceipt {
  const keys = [
    "provider", "status", "plaintext", "personToPersonE2ee", "contextVerified",
    "contextBindingSha256", "providerHistoryChanged", "providerStorageRead",
  ];
  if (
    !exact(raw, keys)
    || raw.provider !== "whatsapp"
    || raw.status !== "opened"
    || typeof raw.plaintext !== "string"
    || raw.plaintext.length === 0
    || new TextEncoder().encode(raw.plaintext).length > 1000
    || raw.personToPersonE2ee !== true
    || raw.contextVerified !== true
    || typeof raw.contextBindingSha256 !== "string"
    || !/^[a-f0-9]{64}$/.test(raw.contextBindingSha256)
    || raw.providerHistoryChanged !== false
    || raw.providerStorageRead !== false
  ) {
    throw new Error("invalid WhatsApp protected-open receipt");
  }
  return raw as unknown as WhatsAppProtectedOpenReceipt;
}
