import type { DetectedAccount, DetectedAccountOpenChoiceKind } from "./services";

export function detectedOpeningChoiceKey(account: Pick<DetectedAccount, "serviceId" | "accountId">): string {
  return `${account.serviceId}:${account.accountId}`;
}

export function chooseDetectedAccountOpening(
  current: ReadonlyMap<string, DetectedAccountOpenChoiceKind>,
  account: DetectedAccount,
  choice: DetectedAccountOpenChoiceKind,
): Map<string, DetectedAccountOpenChoiceKind> {
  const next = new Map(current);
  if (account.openChoices.some((candidate) => candidate.kind === choice)) next.set(detectedOpeningChoiceKey(account), choice);
  return next;
}
