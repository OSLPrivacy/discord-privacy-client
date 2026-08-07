export interface OslMailReplyAllSource {
  readonly to: readonly string[];
  readonly cc: readonly string[];
  readonly bcc: readonly string[];
}

export interface OslMailReplyAllOptions {
  readonly includeBcc: boolean;
}

export interface OslMailReplyAllResult {
  readonly protectedRecipients: readonly string[];
  readonly protectedRecipientCount: number;
  readonly refusal: "BCC excluded from Reply All" | null;
}

export function planOslMailReplyAll(
  source: OslMailReplyAllSource,
  options: OslMailReplyAllOptions,
): OslMailReplyAllResult {
  const protectedRecipients = uniqueRecipients([...source.to, ...source.cc]);

  return Object.freeze({
    protectedRecipients: Object.freeze(protectedRecipients),
    protectedRecipientCount: protectedRecipients.length,
    refusal: options.includeBcc ? "BCC excluded from Reply All" : null,
  });
}

function uniqueRecipients(recipients: readonly string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const recipient of recipients) {
    if (seen.has(recipient)) continue;
    seen.add(recipient);
    result.push(recipient);
  }
  return result;
}
