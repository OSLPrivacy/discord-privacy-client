import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import type { ServiceId } from "./services";

export type MassCleanupAvailability = "unavailable" | "discoveryOnly" | "available";
export type MassCleanupAction =
  | "leaveAndRemoveChat"
  | "clearHistoryForSelf"
  | "leaveServer"
  | "closeConversation"
  | "archiveConversation"
  | "deleteConversationForSelf";

export interface ServiceMassCleanupCapability {
  serviceId: ServiceId;
  availability: MassCleanupAvailability;
  discoverySupported: boolean;
  mutationSupported: boolean;
  plannedActions: MassCleanupAction[];
  status: string;
}

export interface MassCleanupCapabilityManifest {
  proRequired: true;
  reviewRequiredEveryBatch: true;
  typedConfirmationRequiredEveryBatch: true;
  unattendedExecutionAllowed: false;
  services: ServiceMassCleanupCapability[];
}

const serviceIds: readonly ServiceId[] = [
  "discord", "telegram", "instagram", "snapchat", "email", "x", "slack", "linkedin", "teams", "messenger", "signal", "whatsapp",
];
const availabilities: readonly MassCleanupAvailability[] = ["unavailable", "discoveryOnly", "available"];
const actions: readonly MassCleanupAction[] = [
  "leaveAndRemoveChat", "clearHistoryForSelf", "leaveServer", "closeConversation", "archiveConversation", "deleteConversationForSelf",
];

function exactRecord(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length && actual.every((key, index) => key === expected[index]);
}

function parseServiceCapability(raw: unknown): ServiceMassCleanupCapability {
  if (!exactRecord(raw, ["serviceId", "availability", "discoverySupported", "mutationSupported", "plannedActions", "status"])) {
    throw new Error("invalid Mass Cleanup capability");
  }
  if (!serviceIds.includes(raw.serviceId as ServiceId)
    || !availabilities.includes(raw.availability as MassCleanupAvailability)
    || typeof raw.discoverySupported !== "boolean"
    || typeof raw.mutationSupported !== "boolean"
    || !Array.isArray(raw.plannedActions)
    || raw.plannedActions.length > actions.length
    || raw.plannedActions.some((action) => !actions.includes(action as MassCleanupAction))
    || typeof raw.status !== "string"
    || raw.status.length < 1
    || raw.status.length > 160) {
    throw new Error("invalid Mass Cleanup capability");
  }
  const availability = raw.availability as MassCleanupAvailability;
  const validReadiness = availability === "unavailable"
    ? raw.discoverySupported === false && raw.mutationSupported === false
    : availability === "discoveryOnly"
      ? raw.discoverySupported === true && raw.mutationSupported === false
      : raw.discoverySupported === true && raw.mutationSupported === true;
  if (!validReadiness) throw new Error("invalid Mass Cleanup capability");
  return raw as unknown as ServiceMassCleanupCapability;
}

export function parseMassCleanupCapabilities(raw: unknown): MassCleanupCapabilityManifest {
  if (!exactRecord(raw, ["proRequired", "reviewRequiredEveryBatch", "typedConfirmationRequiredEveryBatch", "unattendedExecutionAllowed", "services"])
    || raw.proRequired !== true
    || raw.reviewRequiredEveryBatch !== true
    || raw.typedConfirmationRequiredEveryBatch !== true
    || raw.unattendedExecutionAllowed !== false
    || !Array.isArray(raw.services)
    || raw.services.length !== serviceIds.length) {
    throw new Error("invalid Mass Cleanup manifest");
  }
  const services = raw.services.map(parseServiceCapability);
  if (new Set(services.map((service) => service.serviceId)).size !== services.length) {
    throw new Error("invalid Mass Cleanup manifest");
  }
  return { ...raw, services } as MassCleanupCapabilityManifest;
}

export async function loadMassCleanupCapabilities(): Promise<MassCleanupCapabilityManifest | null> {
  if (!isTauriRuntime()) return null;
  return parseMassCleanupCapabilities(await invoke<unknown>("get_mass_cleanup_capabilities"));
}
