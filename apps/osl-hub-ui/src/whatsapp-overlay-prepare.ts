export interface WhatsAppPreparedCarrier {
  provider: "whatsapp";
  status: "readyForExplicitPlacement";
  coverText: string;
  expiresAt: number;
  personToPersonE2ee: true;
  contextBindingSha256: string;
  automaticPlacement: false;
  realMessageSent: false;
}

export function parseWhatsAppPreparedCarrier(raw: unknown): WhatsAppPreparedCarrier {
  const keys = [
    "provider", "status", "coverText", "expiresAt", "personToPersonE2ee",
    "contextBindingSha256", "automaticPlacement", "realMessageSent",
  ];
  if (
    typeof raw !== "object"
    || raw === null
    || Array.isArray(raw)
    || Object.keys(raw).sort().join(",") !== [...keys].sort().join(",")
  ) {
    throw new Error("invalid WhatsApp protected-carrier receipt");
  }
  const value = raw as Record<string, unknown>;
  if (
    value.provider !== "whatsapp"
    || value.status !== "readyForExplicitPlacement"
    || typeof value.coverText !== "string"
    || value.coverText.length < 1
    || value.coverText.length > 16 * 1024
    || value.coverText.includes("\0")
    || !Number.isSafeInteger(value.expiresAt)
    || Number(value.expiresAt) <= 0
    || value.personToPersonE2ee !== true
    || typeof value.contextBindingSha256 !== "string"
    || !/^[a-f0-9]{64}$/.test(value.contextBindingSha256)
    || value.automaticPlacement !== false
    || value.realMessageSent !== false
  ) {
    throw new Error("invalid WhatsApp protected-carrier receipt");
  }
  return value as unknown as WhatsAppPreparedCarrier;
}
