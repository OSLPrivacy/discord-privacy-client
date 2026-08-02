/**
 * The recovery phrase is deliberately insufficient for markers written before
 * the phrase file-key wrap existed.  This small state machine keeps that
 * refusal explicit: a caller must supply the current password to repair the
 * marker, or take the separately-confirmed Fresh Start path.
 */
export type LegacyRecoveryMigration =
  | { kind: "recoverable" }
  | { kind: "needs-current-password"; phraseVerified: true }
  | { kind: "fresh-start" };

export interface RecoveryMigrationDependencies {
  /** Adds the phrase wrap while preserving all encrypted local files. */
  addPhraseWrap: (currentPassword: string) => Promise<void>;
  /** Deliberately removes local account state after the owner chooses it. */
  freshStart: () => Promise<void>;
}

export const LEGACY_MARKER_REFUSAL = "cannot complete recovery";

/** The native error is intentionally recognized narrowly, never by absence. */
export function legacyMarkerRecoveryRefused(error: unknown): boolean {
  return error instanceof Error && error.message.toLowerCase().includes(LEGACY_MARKER_REFUSAL);
}

export function legacyRecoveryMigrationMarkup(state: LegacyRecoveryMigration): string {
  if (state.kind === "recoverable") return "";
  if (state.kind === "fresh-start") {
    return `<section class="setup-surface recovery-migration" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">Start over</h1><p>Your local OSL account will be removed. This loses your burn list and encrypted local state.</p></section>`;
  }
  return `<section class="setup-surface recovery-migration" aria-labelledby="route-heading"><h1 id="route-heading" tabindex="-1">This account needs its current password</h1><p>Your recovery phrase is valid, but this older account protects encrypted local state with a marker that cannot be recovered safely by phrase alone.</p><form data-recovery-add-phrase-wrap novalidate><label for="recovery-migration-current-password">Re-enter your CURRENT password now so we can add the wrap</label><input id="recovery-migration-current-password" name="currentPassword" type="password" autocomplete="current-password" required/><button class="button primary" type="submit">Add recovery protection</button></form><button class="text-back" type="button" data-recovery-fresh-start>Start over and lose your burn list</button></section>`;
}

/**
 * Repair is non-destructive: the dependency must rotate/write the marker only
 * after proving the current password.  A blank password never reaches native.
 */
export async function addLegacyPhraseWrap(
  currentPassword: string,
  dependencies: RecoveryMigrationDependencies,
): Promise<LegacyRecoveryMigration> {
  if (!currentPassword) return { kind: "needs-current-password", phraseVerified: true };
  await dependencies.addPhraseWrap(currentPassword);
  return { kind: "recoverable" };
}

export async function chooseLegacyFreshStart(
  dependencies: RecoveryMigrationDependencies,
): Promise<LegacyRecoveryMigration> {
  await dependencies.freshStart();
  return { kind: "fresh-start" };
}
