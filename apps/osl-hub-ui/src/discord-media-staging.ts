import { utf8Length } from "./overlay-state";

export const DISCORD_MEDIA_MAX_SIZE = 512 * 1024 * 1024;
export const DISCORD_MEDIA_CAPTION_MAX_BYTES = 4_096;
export const DISCORD_MEDIA_PROGRESS_THROTTLE_MS = 500;
export const DISCORD_MEDIA_PROGRESS_STEPS = [0, 25, 50, 75, 100] as const;

export type DiscordMediaStage =
  | "selected"
  | "protecting"
  | "uploading"
  | "delivering"
  | "sent"
  | "failed"
  | "cancelled";

type ActiveDiscordMediaStage = "protecting" | "uploading" | "delivering";
export type DiscordMediaFailureReason = "offline" | "protection" | "upload" | "delivery" | "unknown";

export interface DiscordMediaMetadata {
  filename: string;
  mediaType: string;
  size: number;
}

/**
 * Renderer-safe state. The native broker owns file handles, paths, contents,
 * encryption material, and upload capabilities; none belong in this shape.
 */
export interface DiscordMediaStagedItem {
  jobId: string;
  metadata: DiscordMediaMetadata;
  caption: string;
  viewOnce: boolean;
  stage: DiscordMediaStage;
  progress: typeof DISCORD_MEDIA_PROGRESS_STEPS[number];
  progressUpdatedAt: number;
  retryFrom: ActiveDiscordMediaStage | null;
  errorLabel: string | null;
}

export type DiscordMediaTrayState = DiscordMediaStagedItem | null;

export interface DiscordMediaProtectionContract {
  protocol: "osl-discord-media-v1";
  jobId: string;
  metadata: DiscordMediaMetadata;
  caption: string;
  viewOnce: boolean;
  authenticatedFields: readonly ["jobId", "metadata", "caption", "viewOnce"];
}

export interface DiscordMediaTrayView {
  label: string;
  status: string;
  progressLabel: string | null;
  canEdit: boolean;
  canCancel: boolean;
  canRetry: boolean;
  canRemove: boolean;
  canPrivatePreview: boolean;
  motion: "coarse" | "none";
}

const exactKeys = (value: Record<string, unknown>, expected: readonly string[]): boolean => {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return actual.length === sortedExpected.length && actual.every((key, index) => key === sortedExpected[index]);
};

const record = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

export function isOpaqueDiscordMediaJobId(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{22,128}$/u.test(value);
}

export function sanitizeDiscordMediaFilename(value: string): string {
  const leaf = value.split(/[\\/]/u).at(-1) ?? "";
  const visible = leaf.replace(/[\u0000-\u001f\u007f]/gu, "").replace(/\s+/gu, " ").trim();
  let result = "";
  for (const character of visible.normalize("NFC")) {
    if (utf8Length(result + character) > 255) break;
    result += character;
  }
  return result;
}

export function isSanitizedDiscordMediaType(value: unknown): value is string {
  return typeof value === "string" && value.length <= 127
    && /^[a-z0-9][a-z0-9!#$&^_.+-]*\/[a-z0-9][a-z0-9!#$&^_.+-]*$/u.test(value);
}

export function parseDiscordMediaSelection(value: unknown): { jobId: string; metadata: DiscordMediaMetadata } | null {
  if (!record(value) || !exactKeys(value, ["jobId", "metadata"]) || !isOpaqueDiscordMediaJobId(value.jobId)
    || !record(value.metadata) || !exactKeys(value.metadata, ["filename", "mediaType", "size"])) return null;
  const { filename, mediaType, size } = value.metadata;
  if (typeof filename !== "string" || filename.length === 0 || sanitizeDiscordMediaFilename(filename) !== filename
    || !isSanitizedDiscordMediaType(mediaType) || !Number.isSafeInteger(size)
    || Number(size) <= 0 || Number(size) > DISCORD_MEDIA_MAX_SIZE) return null;
  return { jobId: value.jobId, metadata: { filename, mediaType, size: Number(size) } };
}

export function boundedDiscordMediaCaption(value: string): string | null {
  if (value.includes("\0") || utf8Length(value) > DISCORD_MEDIA_CAPTION_MAX_BYTES) return null;
  return value;
}

export function stageDiscordMedia(selection: unknown, now: number): DiscordMediaStagedItem | null {
  const parsed = parseDiscordMediaSelection(selection);
  if (!parsed || !Number.isFinite(now) || now < 0) return null;
  return {
    ...parsed,
    caption: "",
    viewOnce: false,
    stage: "selected",
    progress: 0,
    progressUpdatedAt: now,
    retryFrom: null,
    errorLabel: null,
  };
}

function editable(stage: DiscordMediaStage): boolean {
  return stage === "selected" || stage === "failed" || stage === "cancelled";
}

export function setDiscordMediaCaption(item: DiscordMediaStagedItem, caption: string): DiscordMediaStagedItem {
  const bounded = boundedDiscordMediaCaption(caption);
  return editable(item.stage) && bounded !== null ? { ...item, caption: bounded } : item;
}

export function setDiscordMediaViewOnce(item: DiscordMediaStagedItem, viewOnce: boolean): DiscordMediaStagedItem {
  return editable(item.stage) && typeof viewOnce === "boolean" ? { ...item, viewOnce } : item;
}

export function beginDiscordMediaProtection(item: DiscordMediaStagedItem, now: number): DiscordMediaStagedItem {
  if (!editable(item.stage) || !Number.isFinite(now) || now < 0) return item;
  return { ...item, stage: "protecting", progress: 0, progressUpdatedAt: now, retryFrom: null, errorLabel: null };
}

export function advanceDiscordMediaStage(
  item: DiscordMediaStagedItem,
  stage: ActiveDiscordMediaStage | "sent",
  now: number,
): DiscordMediaStagedItem {
  const allowed = (item.stage === "protecting" && stage === "uploading")
    || (item.stage === "uploading" && stage === "delivering")
    || (item.stage === "delivering" && stage === "sent");
  if (!allowed || !Number.isFinite(now) || now < item.progressUpdatedAt) return item;
  return { ...item, stage, progress: stage === "sent" ? 100 : 0, progressUpdatedAt: now, retryFrom: null, errorLabel: null };
}

function coarseProgress(value: number): typeof DISCORD_MEDIA_PROGRESS_STEPS[number] {
  if (!Number.isFinite(value)) return 0;
  const bounded = Math.max(0, Math.min(100, Math.floor(value)));
  if (bounded >= 100) return 100;
  if (bounded >= 75) return 75;
  if (bounded >= 50) return 50;
  if (bounded >= 25) return 25;
  return 0;
}

/** Accepts broker-reported truth but reveals only throttled, 25-point buckets. */
export function reportDiscordMediaProgress(item: DiscordMediaStagedItem, reportedPercent: number, now: number): DiscordMediaStagedItem {
  if (!(["protecting", "uploading", "delivering"] as const).includes(item.stage as ActiveDiscordMediaStage)
    || !Number.isFinite(now) || now - item.progressUpdatedAt < DISCORD_MEDIA_PROGRESS_THROTTLE_MS) return item;
  const next = coarseProgress(reportedPercent);
  if (next <= item.progress || (next === 100 && item.stage !== "delivering")) return item;
  return { ...item, progress: next, progressUpdatedAt: now };
}

const failureLabels: Record<DiscordMediaFailureReason, string> = {
  offline: "Connection unavailable. Try again when online.",
  protection: "Could not protect this file.",
  upload: "Protected upload did not finish.",
  delivery: "Protected media was not delivered.",
  unknown: "Could not send protected media.",
};

export function failDiscordMedia(item: DiscordMediaStagedItem, reason: DiscordMediaFailureReason): DiscordMediaStagedItem {
  if (!(item.stage === "protecting" || item.stage === "uploading" || item.stage === "delivering")) return item;
  return { ...item, stage: "failed", retryFrom: item.stage, errorLabel: failureLabels[reason] };
}

export function cancelDiscordMedia(item: DiscordMediaStagedItem): DiscordMediaStagedItem {
  if (item.stage === "sent" || item.stage === "cancelled") return item;
  const retryFrom = item.stage === "protecting" || item.stage === "uploading" || item.stage === "delivering"
    ? item.stage
    : item.retryFrom;
  return { ...item, stage: "cancelled", retryFrom, errorLabel: null };
}

export function retryDiscordMedia(item: DiscordMediaStagedItem, now: number): DiscordMediaStagedItem {
  if ((item.stage !== "failed" && item.stage !== "cancelled") || !Number.isFinite(now) || now < 0) return item;
  // Encryption and upload ownership stays native. The renderer requests a safe
  // restart and preserves only the metadata/caption/options contract.
  return { ...item, stage: "protecting", progress: 0, progressUpdatedAt: now, retryFrom: null, errorLabel: null };
}

export function removeDiscordMedia(item: DiscordMediaStagedItem): DiscordMediaTrayState {
  return item.stage === "protecting" || item.stage === "uploading" || item.stage === "delivering" ? item : null;
}

export function discordMediaProtectionContract(item: DiscordMediaStagedItem): DiscordMediaProtectionContract | null {
  if (item.stage !== "selected" && item.stage !== "failed" && item.stage !== "cancelled") return null;
  if (boundedDiscordMediaCaption(item.caption) === null) return null;
  return {
    protocol: "osl-discord-media-v1",
    jobId: item.jobId,
    metadata: { ...item.metadata },
    caption: item.caption,
    viewOnce: item.viewOnce,
    authenticatedFields: ["jobId", "metadata", "caption", "viewOnce"],
  };
}

export function discordMediaPrivatePreviewRequest(item: DiscordMediaStagedItem): Readonly<{ jobId: string }> | null {
  return editable(item.stage) ? { jobId: item.jobId } : null;
}

const stageStatus: Record<DiscordMediaStage, string> = {
  selected: "Ready to protect",
  protecting: "Protecting",
  uploading: "Uploading protected media",
  delivering: "Delivering",
  sent: "Sent",
  failed: "Send failed",
  cancelled: "Cancelled",
};

export function discordMediaTrayView(item: DiscordMediaStagedItem, reducedMotion: boolean): DiscordMediaTrayView {
  const active = item.stage === "protecting" || item.stage === "uploading" || item.stage === "delivering";
  return {
    label: `Protected media: ${item.metadata.filename}`,
    status: item.errorLabel ?? stageStatus[item.stage],
    progressLabel: active ? `${stageStatus[item.stage]}, ${item.progress}%` : null,
    canEdit: editable(item.stage),
    canCancel: item.stage !== "sent" && item.stage !== "cancelled",
    canRetry: item.stage === "failed" || item.stage === "cancelled",
    canRemove: !active,
    canPrivatePreview: editable(item.stage),
    motion: reducedMotion ? "none" : "coarse",
  };
}
