export const PRO_ATTACHMENT_LIMIT_BYTES = 1_073_741_824;

export type ProAttachmentTooLargeAction = Readonly<{
  id: "choose-smaller-file";
  label: "Choose a smaller file";
}>;

export type ProAttachmentTooLargeScreen = Readonly<{
  title: "File is too large";
  message: string;
  limitBytes: typeof PRO_ATTACHMENT_LIMIT_BYTES;
  fileSizeBytes: number;
  actions: readonly ProAttachmentTooLargeAction[];
}>;

export function proAttachmentTooLargeScreen(fileSizeBytes: number): ProAttachmentTooLargeScreen {
  if (!Number.isSafeInteger(fileSizeBytes) || fileSizeBytes <= PRO_ATTACHMENT_LIMIT_BYTES) {
    throw new Error("Pro attachment too-large screen needs a file size above the Pro limit.");
  }
  return {
    title: "File is too large",
    message: proAttachmentTooLargeMessage(fileSizeBytes),
    limitBytes: PRO_ATTACHMENT_LIMIT_BYTES,
    fileSizeBytes,
    actions: [{ id: "choose-smaller-file", label: "Choose a smaller file" }],
  };
}

export function proAttachmentTooLargeMessage(fileSizeBytes: number): string {
  if (!Number.isSafeInteger(fileSizeBytes) || fileSizeBytes <= PRO_ATTACHMENT_LIMIT_BYTES) {
    throw new Error("Pro attachment too-large message needs a file size above the Pro limit.");
  }
  return `This file is ${formatAttachmentSize(fileSizeBytes)} (${formatBytes(fileSizeBytes)} bytes). Pro files are limited to 1 GB (${formatBytes(PRO_ATTACHMENT_LIMIT_BYTES)} bytes).`;
}

function formatAttachmentSize(bytes: number): string {
  const gb = bytes / PRO_ATTACHMENT_LIMIT_BYTES;
  return `${gb.toLocaleString("en-US", { maximumFractionDigits: 1 })} GB`;
}

function formatBytes(bytes: number): string {
  return bytes.toLocaleString("en-US");
}
