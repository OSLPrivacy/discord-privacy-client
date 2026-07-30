import type { LocalPrivacyFinding } from "./adapters";
import { scrubSignalGroupFor, type ScrubSignalGroup } from "./scrub";

export type ScrubReviewDedupKey = string & { readonly __scrubReviewDedupKey: unique symbol };

export interface ScrubReviewRow {
  dedupKey: ScrubReviewDedupKey;
  serviceId: string;
  accountId: string;
  logicalHost: string;
  signalGroups: ScrubSignalGroup[];
  findingCount: number;
  newestCreatedAtUnixMs: number | null;
  sample: LocalPrivacyFinding;
}

export function buildScrubReviewList(findings: readonly LocalPrivacyFinding[]): ScrubReviewRow[] {
  const rows = new Map<string, ScrubReviewRow>();
  for (const finding of findings) {
    const logicalHost = logicalHostForFinding(finding);
    const dedupKey = scrubReviewDedupKey(finding.serviceId, finding.accountId, logicalHost, finding.localPreview);
    const signalGroup = scrubSignalGroupFor(finding.category);
    const existing = rows.get(dedupKey);
    if (!existing) {
      rows.set(dedupKey, {
        dedupKey,
        serviceId: finding.serviceId,
        accountId: finding.accountId,
        logicalHost,
        signalGroups: [signalGroup],
        findingCount: 1,
        newestCreatedAtUnixMs: finding.createdAtUnixMs,
        sample: finding,
      });
      continue;
    }
    existing.findingCount += 1;
    if (!existing.signalGroups.includes(signalGroup)) existing.signalGroups.push(signalGroup);
    existing.signalGroups.sort();
    existing.newestCreatedAtUnixMs = newerTimestamp(existing.newestCreatedAtUnixMs, finding.createdAtUnixMs);
  }
  return [...rows.values()].sort((a, b) =>
    (b.newestCreatedAtUnixMs ?? -1) - (a.newestCreatedAtUnixMs ?? -1)
    || a.serviceId.localeCompare(b.serviceId)
    || a.accountId.localeCompare(b.accountId)
    || a.logicalHost.localeCompare(b.logicalHost));
}

export function scrubReviewDedupKey(
  serviceId: string,
  accountId: string,
  logicalHost: string,
  localPreview: string,
): ScrubReviewDedupKey {
  return `${stablePart(serviceId)}\0${stablePart(accountId)}\0${stablePart(logicalHost)}\0${stablePart(localPreview)}` as ScrubReviewDedupKey;
}

export function logicalHostForFinding(finding: LocalPrivacyFinding): string {
  const host = hostFromText(finding.messageLocator)
    ?? hostFromText(finding.localPreview)
    ?? hostFromText(finding.reason);
  return host ? registrableHost(stripHostAlias(host)) : finding.serviceId;
}

function newerTimestamp(a: number | null, b: number | null): number | null {
  if (a === null) return b;
  if (b === null) return a;
  return Math.max(a, b);
}

function stablePart(value: string): string {
  return value.normalize("NFKC").trim().toLowerCase().replace(/\s+/gu, " ");
}

function hostFromText(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed || /[\u0000-\u001f\u007f]/u.test(trimmed)) return null;
  try {
    const url = new URL(trimmed.includes("://") ? trimmed : `https://${trimmed}`);
    return url.hostname.toLowerCase().replace(/\.$/u, "");
  } catch {
    const match = trimmed.match(/\b(?:https?:\/\/)?([a-z0-9](?:[a-z0-9-]{0,62}\.)+[a-z]{2,63})\b/iu);
    return match?.[1]?.toLowerCase().replace(/\.$/u, "") ?? null;
  }
}

function stripHostAlias(host: string): string {
  const labels = host.split(".").filter(Boolean);
  while (labels.length > 2 && ["www", "m", "mobile", "amp", "touch"].includes(labels[0])) {
    labels.shift();
  }
  return labels.join(".");
}

function registrableHost(host: string): string {
  const labels = host.split(".").filter(Boolean);
  if (labels.length <= 2) return host;
  const suffix = labels.slice(-2).join(".");
  if (labels.length >= 3 && ["co.uk", "org.uk", "ac.uk", "com.au", "com.br"].includes(suffix)) {
    return labels.slice(-3).join(".");
  }
  return labels.slice(-2).join(".");
}
