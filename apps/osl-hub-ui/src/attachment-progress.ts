/** Renderer-safe shape emitted by `osl://attachment-progress`. */
export interface AttachmentProgressEvent {
  contextId: string;
  job: AttachmentProgressJob;
}

export interface AttachmentProgressJob {
  jobId: string;
  metadata: { filename: string; mediaType: string; size: number };
  caption: string;
  viewOnce: boolean;
  stage: AttachmentProgressStage;
  progress: number;
  retryFrom: AttachmentProgressStage | null;
  failure: AttachmentProgressFailure | null;
}

export type AttachmentProgressStage = "selected" | "protecting" | "uploading" | "delivering" | "sent" | "failed" | "cancelled";
export type AttachmentProgressFailure = "offline" | "protection" | "upload" | "delivery" | "unknown";

const stages = ["selected", "protecting", "uploading", "delivering", "sent", "failed", "cancelled"] as const;
const failures = ["offline", "protection", "upload", "delivery", "unknown"] as const;
const progressSteps = new Set([0, 25, 50, 75, 100]);

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const hasExactKeys = (value: Record<string, unknown>, expected: readonly string[]): boolean => {
  const actual = Object.keys(value).sort();
  const expectedSorted = [...expected].sort();
  return actual.length === expectedSorted.length && actual.every((key, index) => key === expectedSorted[index]);
};

const isStage = (value: unknown): value is AttachmentProgressStage =>
  typeof value === "string" && (stages as readonly string[]).includes(value);

const isFailure = (value: unknown): value is AttachmentProgressFailure =>
  typeof value === "string" && (failures as readonly string[]).includes(value);

const isSafeText = (value: unknown, maximum: number): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f]/u.test(value);

/** Reject malformed native events rather than rendering data from another boundary. */
export function parseAttachmentProgressEvent(value: unknown): AttachmentProgressEvent | null {
  if (!isRecord(value) || !hasExactKeys(value, ["contextId", "job"]) || !isSafeText(value.contextId, 256) || !isRecord(value.job)) return null;
  const job = value.job;
  if (!hasExactKeys(job, ["jobId", "metadata", "caption", "viewOnce", "stage", "progress", "retryFrom", "failure"])
    || !isSafeText(job.jobId, 128) || !isRecord(job.metadata)
    || !hasExactKeys(job.metadata, ["filename", "mediaType", "size"])
    || !isSafeText(job.metadata.filename, 255) || !isSafeText(job.metadata.mediaType, 127)
    || !Number.isSafeInteger(job.metadata.size) || job.metadata.size <= 0
    || typeof job.caption !== "string" || job.caption.length > 4_096 || typeof job.viewOnce !== "boolean"
    || !isStage(job.stage) || !Number.isInteger(job.progress) || !progressSteps.has(job.progress)
    || !(job.retryFrom === null || isStage(job.retryFrom)) || !(job.failure === null || isFailure(job.failure))) return null;

  return {
    contextId: value.contextId,
    job: {
      jobId: job.jobId,
      metadata: { filename: job.metadata.filename, mediaType: job.metadata.mediaType, size: job.metadata.size },
      caption: job.caption,
      viewOnce: job.viewOnce,
      stage: job.stage,
      progress: job.progress,
      retryFrom: job.retryFrom,
      failure: job.failure,
    },
  };
}

const stageLabels: Readonly<Record<AttachmentProgressStage, string>> = {
  selected: "Ready to protect attachment",
  protecting: "Protecting attachment",
  uploading: "Uploading protected attachment",
  delivering: "Delivering attachment",
  sent: "Attachment sent",
  failed: "Attachment failed",
  cancelled: "Attachment cancelled",
};

const escapeHtml = (value: string): string => value.replace(/[&<>"']/gu, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[character] ?? character);

/**
 * Renders the safe DTO only. Visual presentation belongs to styles.css because
 * the app's CSP rejects runtime style elements and inline style attributes.
 */
export function attachmentProgressMarkup(event: AttachmentProgressEvent): string {
  const { job } = event;
  const stage = stageLabels[job.stage];
  return `<section class="attachment-progress" data-attachment-context="${escapeHtml(event.contextId)}" data-attachment-stage="${job.stage}" role="status" aria-live="polite"><header class="attachment-progress__header"><strong class="attachment-progress__name">${escapeHtml(job.metadata.filename)}</strong><span class="attachment-progress__percent">${job.progress}%</span></header><p class="attachment-progress__stage">${stage}</p><progress class="attachment-progress__bar" value="${job.progress}" max="100" aria-label="${stage}: ${job.progress}%"></progress></section>`;
}
