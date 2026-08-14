export const RESTORE_CHECKS = [
  "recovery-phrase-format",
  "recovery-package-identity",
  "recovery-package-integrity",
  "account-protection",
  "account-readiness",
] as const;

export type RestoreCheck = (typeof RESTORE_CHECKS)[number];

export type CleanDeviceRestorePhase =
  | "restore"
  | "checking-input"
  | "verifying-package"
  | "protecting-account"
  | "confirming-ready"
  | "account-ready"
  | "refused";

export interface CleanDeviceRestoreState {
  phase: CleanDeviceRestorePhase;
  rejectedCheck: RestoreCheck | null;
}

export const initialCleanDeviceRestoreState: CleanDeviceRestoreState = {
  phase: "restore",
  rejectedCheck: null,
};

const CHECK_LABELS: Readonly<Record<RestoreCheck, string>> = {
  "recovery-phrase-format": "Recovery phrase format check",
  "recovery-package-identity": "Recovery package identity check",
  "recovery-package-integrity": "Recovery package integrity check",
  "account-protection": "Account protection check",
  "account-readiness": "Account readiness check",
};

const PROGRESS_STEPS = [
  { phase: "checking-input", label: "Check recovery input" },
  { phase: "verifying-package", label: "Verify recovery package" },
  { phase: "protecting-account", label: "Protect restored account" },
  { phase: "confirming-ready", label: "Confirm account ready" },
] as const;

const PHASE_INDEX: Readonly<Partial<Record<CleanDeviceRestorePhase, number>>> = {
  "checking-input": 0,
  "verifying-package": 1,
  "protecting-account": 2,
  "confirming-ready": 3,
  "account-ready": 4,
};

export function restoreProgress(phase: Exclude<CleanDeviceRestorePhase, "refused">): CleanDeviceRestoreState {
  return { phase, rejectedCheck: null };
}

export function refuseRestore(check: RestoreCheck): CleanDeviceRestoreState {
  return { phase: "refused", rejectedCheck: check };
}

export function restoreCheckLabel(check: RestoreCheck): string {
  return CHECK_LABELS[check];
}

/** Extract only a fixed check tag. Untrusted error detail is never rendered. */
export function restoreCheckFromFailure(failure: unknown, fallback: RestoreCheck): RestoreCheck {
  const text = typeof failure === "string"
    ? failure
    : failure instanceof Error
      ? failure.message
      : "";
  const match = /\[restore-check:([a-z-]+)\]/u.exec(text);
  return match && RESTORE_CHECKS.includes(match[1] as RestoreCheck)
    ? match[1] as RestoreCheck
    : fallback;
}

export function renderCleanDeviceRestoreStatus(state: CleanDeviceRestoreState): string {
  const activeIndex = PHASE_INDEX[state.phase] ?? -1;
  const steps = PROGRESS_STEPS.map((step, index) => {
    const status = state.phase === "account-ready" || index < activeIndex
      ? "complete"
      : index === activeIndex
        ? "active"
        : "pending";
    const marker = status === "complete" ? "✓" : String(index + 1);
    return `<li data-restore-step="${step.phase}" data-step-state="${status}"><span aria-hidden="true">${marker}</span>${step.label}</li>`;
  }).join("");
  const refusal = state.phase === "refused" && state.rejectedCheck
    ? `<p class="restore-refusal" role="alert" data-restore-refusal data-rejected-check="${state.rejectedCheck}"><strong>Restore refused</strong><span>${restoreCheckLabel(state.rejectedCheck)} rejected this import. The account was not made ready.</span></p>`
    : "";
  const ready = state.phase === "account-ready"
    ? '<p class="restore-ready" role="status" data-account-ready><strong>Account ready</strong><span>Your restored account is protected on this device.</span></p>'
    : "";
  return `<section class="restore-journey" data-restore-phase="${state.phase}" aria-label="Restore progress"><ol>${steps}</ol>${refusal}${ready}</section>`;
}

export function assertBadRestoreFixtureResults(
  expectedBadFixtureIds: readonly string[],
  results: readonly { fixtureId: string; reachedReady: boolean; accountsCreated: number; refusalCount: number; rejectedCheck: string | null }[],
): void {
  const expected = new Set(expectedBadFixtureIds);
  const byId = new Map(results.map((result) => [result.fixtureId, result]));
  if (expected.size !== expectedBadFixtureIds.length || byId.size !== results.length) {
    throw new Error("TASK0457 bad-import fixture ids must be unique");
  }
  for (const fixtureId of expected) {
    const result = byId.get(fixtureId);
    if (!result) throw new Error(`TASK0457 bad-import fixture ${fixtureId} was not run`);
    if (result.reachedReady || result.accountsCreated !== 0 || result.refusalCount !== 1 || !result.rejectedCheck) {
      throw new Error(`TASK0457 bad-import fixture ${fixtureId} was quietly accepted or lacked one named refusal`);
    }
  }
  for (const result of results) {
    if (!expected.has(result.fixtureId)) throw new Error(`TASK0457 unexpected bad-import result ${result.fixtureId}`);
  }
}
