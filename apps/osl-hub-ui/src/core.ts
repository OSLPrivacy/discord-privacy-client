import { invoke } from "./dev-preview";
import { isTauriRuntime } from "./preferences";

export interface CoreReadiness {
  originalCoreLinked: boolean;
  identityLoaded: boolean;
  keyserverInitialised: boolean;
  cloudRegistrationState: "notAttempted" | "pending" | "registered" | "conflict" | "offline";
  groupSenderKeysEnabled: boolean;
  remoteServiceHasNativeAccess: boolean;
  bootstrapAttempted: boolean;
  passwordGateRequired: boolean;
  unlocked: boolean;
  activeOslUserId: string | null;
  bootstrapStatus: BootstrapStatus;
}

export type BootstrapStatus = "notAttempted" | "setupRequired" | "inProgress" | "passwordRequired" | "ready" | "failed";

export interface CoreFeature {
  id: string;
  group: string;
  label: string;
  bridgeState: "source-linked" | "guarded" | "refactor-required" | "shell-adapter-required";
}

export interface CoreIntegration {
  readiness: CoreReadiness;
  features: CoreFeature[];
}

export interface HubIdentitySetupResult {
  userId: string;
  identityRecoveryPhrase: string | null;
  storageMethod: string;
  passwordSetupRequired: boolean;
}

export interface HubPasswordReadiness {
  accessState: string;
  identityLoaded: boolean;
  mainPasswordSet: boolean;
  unlocked: boolean;
  serviceNeutralIdentitySupported: boolean;
  canCreateIdentity: boolean;
  canImportIdentityPhrase: boolean;
  passwordAttemptsUsed: number;
  passwordLockoutSecondsRemaining: number;
}

export interface HubMainPasswordSetupResult {
  passwordRecoveryPhrase: string;
  encryptedStateReloadComplete: boolean;
  encryptedStateReloadIssueCount: number;
  readiness: HubPasswordReadiness;
}

export interface HubPasswordRoleStatus {
  mainPasswordSet: boolean;
  stealthPasswordSet: boolean;
  burnPasswordSet: boolean;
  unlocked: boolean;
  stealthActionWired: boolean;
  burnActionWired: boolean;
}

export interface HubGateBurnResult {
  localCleanupComplete: boolean;
  removedTargets: string[];
  failedTargets: string[];
  remoteUnregister: {
    identitiesFound: number;
    succeeded: number;
    failed: number;
    unavailable: number;
  };
  restartRequired: boolean;
  originalDiscordDataUntouched: true;
}

export interface HubGateUnlockResult {
  outcome: "unlocked" | "decoy" | "burned" | "wrong";
  lockoutSecondsRemaining: number;
  attemptsUsed: number;
  readiness: CoreReadiness | null;
  burn: HubGateBurnResult | null;
}

export type HubLicenseAccess = "free" | "pro" | "offlineGrace";

export interface HubLicenseState {
  access: HubLicenseAccess;
  status: "UNCONFIGURED" | "ACTIVE" | "CANCELLED" | "GRACE" | "EXPIRED" | "REVOKED" | "UNKNOWN" | "PENDING";
  currentPeriodEnd: number | null;
  lastValidatedAt: number | null;
}

export const unconfiguredLicenseState: HubLicenseState = {
  access: "free",
  status: "UNCONFIGURED",
  currentPeriodEnd: null,
  lastValidatedAt: null,
};

export const unavailableCoreIntegration: CoreIntegration = {
  readiness: {
    originalCoreLinked: false,
    identityLoaded: false,
    keyserverInitialised: false,
    cloudRegistrationState: "notAttempted",
    groupSenderKeysEnabled: false,
    remoteServiceHasNativeAccess: false,
    bootstrapAttempted: false,
    passwordGateRequired: true,
    unlocked: false,
    activeOslUserId: null,
    bootstrapStatus: "notAttempted",
  },
  features: [],
};

const readinessKeys = [
  "originalCoreLinked",
  "identityLoaded",
  "keyserverInitialised",
  "groupSenderKeysEnabled",
  "remoteServiceHasNativeAccess",
] as const;
const cloudRegistrationStates: readonly CoreReadiness["cloudRegistrationState"][] = [
  "notAttempted", "pending", "registered", "conflict", "offline",
];
const extendedReadinessKeys = [
  "bootstrapAttempted",
  "passwordGateRequired",
  "unlocked",
  "activeOslUserId",
  "bootstrapStatus",
] as const;
const bootstrapStatuses: readonly BootstrapStatus[] = ["notAttempted", "setupRequired", "inProgress", "passwordRequired", "ready", "failed"];
const featureKeys = ["id", "group", "label", "bridgeState"] as const;
const bridgeStates: CoreFeature["bridgeState"][] = [
  "source-linked",
  "guarded",
  "refactor-required",
  "shell-adapter-required",
];

export async function loadCoreIntegration(): Promise<CoreIntegration> {
  if (!isTauriRuntime()) return structuredClone(unavailableCoreIntegration);
  return loadCoreIntegrationFromNative((command) => invoke<unknown>(command));
}

type NativeCoreInvoke = (command: string) => Promise<unknown>;

export async function loadCoreIntegrationFromNative(nativeInvoke: NativeCoreInvoke): Promise<CoreIntegration> {
  const rawReadiness = await nativeInvoke("get_core_readiness");
  return {
    readiness: parseCoreReadiness(rawReadiness),
    features: [],
  };
}

export function isValidMainPassword(password: string): boolean {
  return /^[\x20-\x7e]{6,128}$/.test(password);
}

export function isValidNewMainPassword(password: string): boolean {
  return /^[\x20-\x7e]{6,128}$/.test(password);
}

export async function unlockHubPasswordGate(password: string): Promise<HubGateUnlockResult> {
  if (!isTauriRuntime() || !isValidMainPassword(password)) throw new Error("unlock unavailable");
  return parseHubGateUnlockResult(await invoke<unknown>("unlock_hub_password_gate", { password }));
}

export async function loadHubPasswordRoleStatus(): Promise<HubPasswordRoleStatus> {
  if (!isTauriRuntime()) throw new Error("password roles unavailable");
  return parseHubPasswordRoleStatus(await invoke<unknown>("get_hub_password_role_status"));
}

export async function setHubAlternatePassword(role: "stealth" | "burn", currentMain: string, alternate: string): Promise<HubPasswordRoleStatus> {
  if (!isTauriRuntime() || !isValidMainPassword(currentMain) || !isValidNewMainPassword(alternate) || currentMain === alternate) {
    throw new Error("password role unavailable");
  }
  const command = role === "stealth" ? "set_hub_stealth_password" : "set_hub_burn_password";
  const key = role === "stealth" ? "newStealth" : "newBurn";
  return parseHubPasswordRoleStatus(await invoke<unknown>(command, { currentMain, [key]: alternate }));
}

export async function removeHubAlternatePassword(role: "stealth" | "burn", currentMain: string): Promise<HubPasswordRoleStatus> {
  if (!isTauriRuntime() || !isValidMainPassword(currentMain)) throw new Error("password role unavailable");
  const command = role === "stealth" ? "remove_hub_stealth_password" : "remove_hub_burn_password";
  return parseHubPasswordRoleStatus(await invoke<unknown>(command, { currentMain }));
}

export async function createHubOslIdentity(): Promise<HubIdentitySetupResult> {
  if (!isTauriRuntime()) throw new Error("identity creation unavailable");
  return parseIdentitySetupResult(await invoke<unknown>("create_hub_osl_identity"));
}

export async function importHubOslIdentityPhrase(recoveryPhrase: string): Promise<HubIdentitySetupResult> {
  if (!isTauriRuntime() || !isRecoveryPhrase(recoveryPhrase)) throw new Error("identity import unavailable");
  return parseIdentitySetupResult(await invoke<unknown>("import_hub_osl_identity_phrase", { recoveryPhrase: recoveryPhrase.trim() }));
}

export async function setupHubMainPassword(password: string): Promise<HubMainPasswordSetupResult> {
  if (!isTauriRuntime() || !isValidNewMainPassword(password)) throw new Error("setup unavailable");
  return parseMainPasswordSetupResult(await invoke<unknown>("setup_hub_main_password", { password }));
}

export function isActivationCode(value: string): boolean {
  return /^OSL-[0-9A-Z]{4}(?:-[0-9A-Z]{4}){3}$/.test(value.trim().toUpperCase());
}

export async function loadHubLicenseState(): Promise<HubLicenseState> {
  if (!isTauriRuntime()) return structuredClone(unconfiguredLicenseState);
  return parseHubLicenseState(await invoke<unknown>("get_hub_license_state"));
}

export async function validateHubActivationCode(activationCode: string): Promise<HubLicenseState> {
  const normalized = activationCode.trim().toUpperCase();
  if (!isTauriRuntime() || !isActivationCode(normalized)) throw new Error("activation unavailable");
  return parseHubLicenseState(await invoke<unknown>("validate_hub_activation_code", { activationCode: normalized }));
}

export async function clearHubActivationCode(): Promise<HubLicenseState> {
  if (!isTauriRuntime()) throw new Error("activation unavailable");
  return parseHubLicenseState(await invoke<unknown>("clear_hub_activation_code"));
}

export function parseHubLicenseState(raw: unknown): HubLicenseState {
  const keys = ["access", "status", "currentPeriodEnd", "lastValidatedAt"] as const;
  const accesses: readonly HubLicenseAccess[] = ["free", "pro", "offlineGrace"];
  const statuses: readonly HubLicenseState["status"][] = ["UNCONFIGURED", "ACTIVE", "CANCELLED", "GRACE", "EXPIRED", "REVOKED", "UNKNOWN", "PENDING"];
  if (!isExactRecord(raw, keys)
    || !accesses.includes(raw.access as HubLicenseAccess)
    || !statuses.includes(raw.status as HubLicenseState["status"])
    || !isOptionalUnixSeconds(raw.currentPeriodEnd)
    || !isOptionalUnixSeconds(raw.lastValidatedAt)
  ) throw new Error("invalid activation state response");
  if (raw.access === "pro" && !["ACTIVE", "CANCELLED", "GRACE"].includes(String(raw.status))) throw new Error("invalid activation state response");
  if (raw.access === "offlineGrace" && !["ACTIVE", "CANCELLED", "GRACE", "UNKNOWN"].includes(String(raw.status))) throw new Error("invalid activation state response");
  return raw as unknown as HubLicenseState;
}

export function parseHubPasswordRoleStatus(raw: unknown): HubPasswordRoleStatus {
  const keys = ["mainPasswordSet", "stealthPasswordSet", "burnPasswordSet", "unlocked", "stealthActionWired", "burnActionWired"] as const;
  if (!isExactRecord(raw, keys) || keys.some((key) => typeof raw[key] !== "boolean")) {
    throw new Error("invalid password-role response");
  }
  return raw as unknown as HubPasswordRoleStatus;
}

export function parseHubGateUnlockResult(raw: unknown): HubGateUnlockResult {
  if (!isExactRecord(raw, ["outcome", "lockoutSecondsRemaining", "attemptsUsed", "readiness", "burn"])) {
    throw new Error("invalid password-gate response");
  }
  const outcomes: readonly HubGateUnlockResult["outcome"][] = ["unlocked", "decoy", "burned", "wrong"];
  if (
    !outcomes.includes(raw.outcome as HubGateUnlockResult["outcome"])
    || !Number.isSafeInteger(raw.lockoutSecondsRemaining)
    || (raw.lockoutSecondsRemaining as number) < 0
    || !Number.isSafeInteger(raw.attemptsUsed)
    || (raw.attemptsUsed as number) < 0
  ) throw new Error("invalid password-gate response");

  const readiness = raw.readiness === null ? null : parseCoreReadiness(raw.readiness);
  const burn = raw.burn === null ? null : parseHubGateBurnResult(raw.burn);
  if (
    (raw.outcome === "unlocked") !== (readiness !== null)
    || (raw.outcome === "burned") !== (burn !== null)
  ) throw new Error("invalid password-gate response");
  return { ...raw, readiness, burn } as HubGateUnlockResult;
}

function parseHubGateBurnResult(raw: unknown): HubGateBurnResult {
  if (!isExactRecord(raw, ["localCleanupComplete", "removedTargets", "failedTargets", "remoteUnregister", "restartRequired", "originalDiscordDataUntouched"])) {
    throw new Error("invalid password-gate response");
  }
  if (
    typeof raw.localCleanupComplete !== "boolean"
    || !isSafeTextArray(raw.removedTargets, 64, 64)
    || !isSafeTextArray(raw.failedTargets, 64, 64)
    || typeof raw.restartRequired !== "boolean"
    || raw.originalDiscordDataUntouched !== true
    || !isExactRecord(raw.remoteUnregister, ["identitiesFound", "succeeded", "failed", "unavailable"])
    || Object.values(raw.remoteUnregister).some((value) => !Number.isSafeInteger(value) || (value as number) < 0)
  ) throw new Error("invalid password-gate response");
  return raw as unknown as HubGateBurnResult;
}

export function isRecoveryPhrase(value: string): boolean {
  const words = value.trim().split(/\s+/u);
  return words.length === 12 && words.every((word) => /^[a-z]+$/u.test(word));
}

export function parseIdentitySetupResult(raw: unknown): HubIdentitySetupResult {
  if (!isExactRecord(raw, ["userId", "identityRecoveryPhrase", "storageMethod", "passwordSetupRequired"])) throw new Error("invalid identity setup response");
  if (
    !isSafeText(raw.userId, 96)
    || (raw.identityRecoveryPhrase !== null && !isSafeText(raw.identityRecoveryPhrase, 256))
    || !isSafeText(raw.storageMethod, 32)
    || typeof raw.passwordSetupRequired !== "boolean"
  ) throw new Error("invalid identity setup response");
  return raw as unknown as HubIdentitySetupResult;
}

export function parseMainPasswordSetupResult(raw: unknown): HubMainPasswordSetupResult {
  if (!isExactRecord(raw, ["passwordRecoveryPhrase", "encryptedStateReloadComplete", "encryptedStateReloadIssueCount", "readiness"])) throw new Error("invalid password setup response");
  if (
    !isSafeText(raw.passwordRecoveryPhrase, 512)
    || typeof raw.encryptedStateReloadComplete !== "boolean"
    || !Number.isSafeInteger(raw.encryptedStateReloadIssueCount)
    || (raw.encryptedStateReloadIssueCount as number) < 0
    || typeof raw.readiness !== "object"
    || raw.readiness === null
    || Array.isArray(raw.readiness)
  ) throw new Error("invalid password setup response");
  return raw as unknown as HubMainPasswordSetupResult;
}

export function parseCoreReadiness(raw: unknown): CoreReadiness {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return structuredClone(unavailableCoreIntegration.readiness);
  const record = raw as Record<string, unknown>;
  const allowedKeys = [...readinessKeys, "cloudRegistrationState", ...extendedReadinessKeys];
  const actualKeys = Object.keys(record);
  if (actualKeys.some((key) => !allowedKeys.includes(key as typeof allowedKeys[number])) || readinessKeys.some((key) => !(key in record))) {
    return structuredClone(unavailableCoreIntegration.readiness);
  }
  if (readinessKeys.some((key) => typeof record[key] !== "boolean")) return structuredClone(unavailableCoreIntegration.readiness);
  const cloudRegistrationState = record.cloudRegistrationState ?? "notAttempted";
  if (!cloudRegistrationStates.includes(cloudRegistrationState as CoreReadiness["cloudRegistrationState"])) {
    return structuredClone(unavailableCoreIntegration.readiness);
  }

  const extendedCount = extendedReadinessKeys.filter((key) => key in record).length;
  if (extendedCount !== 0 && extendedCount !== extendedReadinessKeys.length) return structuredClone(unavailableCoreIntegration.readiness);
  if (extendedCount === 0) {
    return {
      ...(record as unknown as Pick<CoreReadiness, typeof readinessKeys[number]>),
      cloudRegistrationState: cloudRegistrationState as CoreReadiness["cloudRegistrationState"],
      bootstrapAttempted: false,
      passwordGateRequired: true,
      unlocked: false,
      activeOslUserId: null,
      bootstrapStatus: "notAttempted",
    };
  }

  if (
    typeof record.bootstrapAttempted !== "boolean"
    || typeof record.passwordGateRequired !== "boolean"
    || typeof record.unlocked !== "boolean"
    || (record.activeOslUserId !== null && !isSafeText(record.activeOslUserId, 160))
    || !bootstrapStatuses.includes(record.bootstrapStatus as BootstrapStatus)
  ) return structuredClone(unavailableCoreIntegration.readiness);

  if (
    (record.unlocked === true && record.originalCoreLinked !== true)
    || (record.bootstrapStatus === "passwordRequired" && record.unlocked === true)
    || (record.bootstrapStatus === "ready" && (
      record.unlocked !== true
      || record.identityLoaded !== true
      || record.keyserverInitialised !== true
      || cloudRegistrationState !== "registered"
      || record.activeOslUserId === null
    ))
  ) {
    return structuredClone(unavailableCoreIntegration.readiness);
  }
  return {
    ...(record as unknown as CoreReadiness),
    cloudRegistrationState: cloudRegistrationState as CoreReadiness["cloudRegistrationState"],
  };
}

export function isCoreProtectionReady(readiness: CoreReadiness): boolean {
  return readiness.identityLoaded
    && readiness.keyserverInitialised
    && readiness.cloudRegistrationState === "registered"
    && readiness.unlocked;
}

export function coreReadinessLabel(readiness: CoreReadiness): string {
  if (isCoreProtectionReady(readiness)) return "OSL core ready · locally unlocked";
  if (readiness.cloudRegistrationState === "pending") return "Connecting to OSL cloud";
  if (readiness.cloudRegistrationState === "offline") return "OSL cloud is unavailable";
  if (readiness.cloudRegistrationState === "conflict") return "Cloud identity conflict · encryption disabled";
  if (readiness.originalCoreLinked) return "source linked · bootstrap required";
  return "OSL source unavailable";
}

export function parseCoreFeatures(raw: unknown): CoreFeature[] {
  if (!Array.isArray(raw) || raw.length > 64) return [];
  const ids = new Set<string>();
  const parsed: CoreFeature[] = [];
  for (const value of raw) {
    if (!isExactRecord(value, featureKeys)) return [];
    if (
      !isSafeText(value.id, 64)
      || !/^[a-z0-9-]+$/.test(value.id)
      || ids.has(value.id)
      || !isSafeText(value.group, 64)
      || !isSafeText(value.label, 120)
      || !bridgeStates.includes(value.bridgeState as CoreFeature["bridgeState"])
    ) return [];
    ids.add(value.id);
    parsed.push(value as unknown as CoreFeature);
  }
  return parsed;
}

function isExactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value);
  return actual.length === keys.length && actual.every((key) => keys.includes(key));
}

function isSafeText(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength && !/[\u0000-\u001f\u007f]/.test(value);
}

function isSafeTextArray(value: unknown, maxItems: number, maxLength: number): value is string[] {
  return Array.isArray(value)
    && value.length <= maxItems
    && value.every((item) => isSafeText(item, maxLength));
}

function isOptionalUnixSeconds(value: unknown): value is number | null {
  return value === null || (Number.isSafeInteger(value) && Number(value) >= 0 && Number(value) <= 32_503_680_000);
}
