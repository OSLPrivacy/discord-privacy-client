import type { ServiceId } from "./services";

export type ServiceGuideStep = 0 | 1 | 2;

export interface ServiceGuideState {
  serviceId: ServiceId;
  step: ServiceGuideStep;
}

const serviceIds = new Set<ServiceId>([
  "discord", "telegram", "instagram", "snapchat", "email", "x",
  "slack", "linkedin", "teams", "messenger", "signal", "whatsapp",
]);

export function parseServiceGuideState(raw: string | null): ServiceGuideState | null {
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as unknown;
    if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
    const record = value as Record<string, unknown>;
    if (Object.keys(record).length !== 2) return null;
    if (!serviceIds.has(record.serviceId as ServiceId)) return null;
    if (!Number.isInteger(record.step) || (record.step as number) < 0 || (record.step as number) > 3) return null;
    // Step 3 was the final page in the older four-step guide. Resume it at the
    // final page of the simplified three-step guide instead of discarding it.
    const step = record.step === 3 ? 2 : record.step as ServiceGuideStep;
    return { serviceId: record.serviceId as ServiceId, step };
  } catch {
    return null;
  }
}

export function nextServiceGuideStep(step: ServiceGuideStep): ServiceGuideStep {
  return Math.min(2, step + 1) as ServiceGuideStep;
}

export function previousServiceGuideStep(step: ServiceGuideStep): ServiceGuideStep {
  return Math.max(0, step - 1) as ServiceGuideStep;
}
