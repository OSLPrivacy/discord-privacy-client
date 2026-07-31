export const WHATSAPP_VISUAL_BINDING_REGIONS = [
  "accountHeader",
  "chatHeader",
  "composer",
  "transcript",
] as const;

export type WhatsAppVisualBindingRegion = typeof WHATSAPP_VISUAL_BINDING_REGIONS[number];
export interface WhatsAppVisualBindingBeginReceipt {
  provider: "whatsapp";
  status: "awaitingConfirmation";
  captureId: string;
  regions: WhatsAppVisualBindingRegion[];
  fixedRegionsOnly: true;
  contentPersisted: false;
  privateStorageRead: false;
  foregroundChanged: false;
}

export interface WhatsAppVisualBindingConfirmReceipt {
  provider: "whatsapp";
  status: "verified";
  captureId: string;
  selectorRevision: "whatsapp-visual-confirmed-v1";
  contextBindingSha256: string;
  recipientSetSha256: string;
  windowGeneration: number;
  windowRect: [number, number, number, number];
  composerRect: [number, number, number, number];
  transcriptRect: [number, number, number, number];
  accountVerified: true;
  chatVerified: true;
  recipientSetVerified: true;
  composerVerified: true;
  transcriptVerified: true;
  protectedControlsAvailable: true;
  contentPersisted: false;
  privateStorageRead: false;
  foregroundChanged: false;
}

const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  typeof value === "object"
  && value !== null
  && !Array.isArray(value)
  && Object.keys(value).sort().join(",") === [...keys].sort().join(",");

const safeCaptureId = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9_-]{16,128}$/.test(value);

const sha256 = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);

const rect = (value: unknown): value is [number, number, number, number] =>
  Array.isArray(value)
  && value.length === 4
  && value.every(Number.isSafeInteger)
  && value[2] > value[0]
  && value[3] > value[1];

export function parseWhatsAppVisualBindingBeginReceipt(raw: unknown): WhatsAppVisualBindingBeginReceipt {
  const keys = [
    "provider", "status", "captureId", "regions", "fixedRegionsOnly",
    "contentPersisted", "privateStorageRead", "foregroundChanged",
  ];
  if (
    !exact(raw, keys)
    || raw.provider !== "whatsapp"
    || raw.status !== "awaitingConfirmation"
    || !safeCaptureId(raw.captureId)
    || !Array.isArray(raw.regions)
    || raw.regions.length !== WHATSAPP_VISUAL_BINDING_REGIONS.length
    || raw.regions.some((region, index) => region !== WHATSAPP_VISUAL_BINDING_REGIONS[index])
    || raw.fixedRegionsOnly !== true
    || raw.contentPersisted !== false
    || raw.privateStorageRead !== false
    || raw.foregroundChanged !== false
  ) throw new Error("invalid WhatsApp visual-binding start receipt");
  return raw as unknown as WhatsAppVisualBindingBeginReceipt;
}

export function parseWhatsAppVisualBindingConfirmReceipt(raw: unknown): WhatsAppVisualBindingConfirmReceipt {
  const keys = [
    "provider", "status", "captureId", "selectorRevision", "contextBindingSha256",
    "recipientSetSha256", "windowGeneration", "windowRect", "composerRect",
    "transcriptRect", "accountVerified",
    "chatVerified", "recipientSetVerified", "composerVerified", "transcriptVerified",
    "protectedControlsAvailable", "contentPersisted", "privateStorageRead", "foregroundChanged",
  ];
  if (
    !exact(raw, keys)
    || raw.provider !== "whatsapp"
    || raw.status !== "verified"
    || !safeCaptureId(raw.captureId)
    || raw.selectorRevision !== "whatsapp-visual-confirmed-v1"
    || !sha256(raw.contextBindingSha256)
    || !sha256(raw.recipientSetSha256)
    || !Number.isSafeInteger(raw.windowGeneration)
    || Number(raw.windowGeneration) <= 0
    || !rect(raw.windowRect)
    || !rect(raw.composerRect)
    || !rect(raw.transcriptRect)
    || raw.contentPersisted !== false
    || raw.privateStorageRead !== false
    || raw.foregroundChanged !== false
  ) throw new Error("invalid WhatsApp visual-binding confirmation receipt");

  const flags = [
    raw.accountVerified,
    raw.chatVerified,
    raw.recipientSetVerified,
    raw.composerVerified,
    raw.transcriptVerified,
    raw.protectedControlsAvailable,
  ];
  if (!flags.every((flag) => flag === true)) {
    throw new Error("incomplete WhatsApp visual binding");
  }
  return raw as unknown as WhatsAppVisualBindingConfirmReceipt;
}

export function isCompleteWhatsAppVisualBinding(
  receipt: WhatsAppVisualBindingConfirmReceipt,
  captureId: string,
): boolean {
  return receipt.status === "verified"
    && receipt.captureId === captureId
    && receipt.accountVerified
    && receipt.chatVerified
    && receipt.recipientSetVerified
    && receipt.composerVerified
    && receipt.transcriptVerified
    && receipt.protectedControlsAvailable
    && sha256(receipt.contextBindingSha256)
    && sha256(receipt.recipientSetSha256);
}
