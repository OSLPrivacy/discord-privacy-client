import { invoke } from "@tauri-apps/api/core";

export type ScrubImapTlsMode = "tls";

export interface ScrubImapAccountConfig {
  accountId: string;
  emailAddress: string;
  serverHost: string;
  serverPort: number;
  tlsMode: ScrubImapTlsMode;
  credentialRef: string;
  scopeFingerprint: string;
  liveAuthEpoch: number;
}

export interface ScrubImapCapability {
  accountId: string;
  capabilityId: string;
  scopeFingerprint: string;
  authEpoch: number;
  canScan: true;
  canDelete: false;
  providerAccess: "imap_read_only";
  liveAuthEpochRequired: true;
}

export interface ScrubImapReauthRequest {
  accountId: string;
  previousAuthEpoch: number;
  proofNonce: string;
}

export interface ScrubImapPort {
  configure(config: ScrubImapAccountConfig): Promise<unknown>;
  capability(accountId: string, scopeFingerprint: string, liveAuthEpoch: number): Promise<unknown>;
  reauth(request: ScrubImapReauthRequest): Promise<unknown>;
}

const productionPort: ScrubImapPort = {
  configure: (config) => invoke("configure_scrub_imap_account", { config }),
  capability: (accountId, scopeFingerprint, liveAuthEpoch) => invoke("get_scrub_imap_capability", {
    accountId,
    scopeFingerprint,
    liveAuthEpoch,
  }),
  reauth: (request) => invoke("reauth_scrub_imap_account", { request }),
};

export async function configureScrubImapAccount(
  config: ScrubImapAccountConfig,
  port: ScrubImapPort = productionPort,
): Promise<ScrubImapCapability> {
  validateConfig(config);
  return parseScrubImapCapability(await port.configure(config), config.accountId, config.scopeFingerprint, config.liveAuthEpoch);
}

export async function getScrubImapCapability(
  accountId: string,
  scopeFingerprint: string,
  liveAuthEpoch: number,
  port: ScrubImapPort = productionPort,
): Promise<ScrubImapCapability | null> {
  if (!validAccountId(accountId) || !validHex(scopeFingerprint, 64) || !positiveInteger(liveAuthEpoch)) {
    throw new Error("invalid scrub IMAP capability request");
  }
  const raw = await port.capability(accountId, scopeFingerprint, liveAuthEpoch);
  return raw === null ? null : parseScrubImapCapability(raw, accountId, scopeFingerprint, liveAuthEpoch);
}

export async function reauthScrubImapAccount(
  request: ScrubImapReauthRequest,
  port: ScrubImapPort = productionPort,
): Promise<number> {
  if (!validAccountId(request.accountId)
    || !positiveInteger(request.previousAuthEpoch)
    || !validHex(request.proofNonce, 64)) {
    throw new Error("invalid scrub IMAP reauthorization request");
  }
  const raw = await port.reauth(request);
  if (!exactRecord(raw, ["accountId", "authEpoch"])
    || raw.accountId !== request.accountId
    || !positiveInteger(raw.authEpoch)
    || raw.authEpoch <= request.previousAuthEpoch) {
    throw new Error("invalid scrub IMAP reauthorization response");
  }
  return raw.authEpoch;
}

function validateConfig(config: ScrubImapAccountConfig): void {
  if (!validAccountId(config.accountId)
    || !boundedText(config.emailAddress, 254)
    || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/u.test(config.emailAddress)
    || !validHost(config.serverHost)
    || !Number.isSafeInteger(config.serverPort)
    || config.serverPort < 1
    || config.serverPort > 65_535
    || config.tlsMode !== "tls"
    || !validToken(config.credentialRef)
    || !validHex(config.scopeFingerprint, 64)
    || !positiveInteger(config.liveAuthEpoch)) {
    throw new Error("invalid scrub IMAP account configuration");
  }
}

function parseScrubImapCapability(
  raw: unknown,
  accountId: string,
  scopeFingerprint: string,
  liveAuthEpoch: number,
): ScrubImapCapability {
  if (!exactRecord(raw, [
    "accountId",
    "capabilityId",
    "scopeFingerprint",
    "authEpoch",
    "canScan",
    "canDelete",
    "providerAccess",
    "liveAuthEpochRequired",
  ])
    || raw.accountId !== accountId
    || !validHex(raw.capabilityId, 32)
    || raw.scopeFingerprint !== scopeFingerprint
    || raw.authEpoch !== liveAuthEpoch
    || raw.canScan !== true
    || raw.canDelete !== false
    || raw.providerAccess !== "imap_read_only"
    || raw.liveAuthEpochRequired !== true) {
    throw new Error("invalid scrub IMAP capability");
  }
  return raw as unknown as ScrubImapCapability;
}

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function positiveInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

function validHex(value: unknown, length: number): value is string {
  return typeof value === "string" && value.length === length && /^[a-f0-9]+$/u.test(value);
}

function validAccountId(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$/u.test(value);
}

function validToken(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9][a-z0-9_-]{0,63}$/u.test(value);
}

function validHost(value: unknown): value is string {
  return typeof value === "string"
    && value.length <= 253
    && /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/u.test(value);
}

function boundedText(value: unknown, maxBytes: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && new TextEncoder().encode(value).length <= maxBytes
    && !/[\u0000-\u001f\u007f]/u.test(value);
}
