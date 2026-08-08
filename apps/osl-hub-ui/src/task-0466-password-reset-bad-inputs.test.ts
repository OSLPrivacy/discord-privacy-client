import { describe, expect, it, vi } from "vitest";

import {
  initialAccountRecoveryFlow,
  submitRecoveredPassword,
  submitRecoveryPhrase,
  type AccountRecoveryDependencies,
  type AccountRecoveryFlow,
} from "./account-recovery";

const RECORD = "RUBY-0466";
const OLD_PASSWORD = "old-password-0466";
const NEW_PASSWORD = "new-password-0466";
const RIGHT_PHRASE = "abandon ability able about above absent absorb abstract absurd abuse access accident";

type ResetCopy = {
  records: string[];
  password: string;
  savedPhrase: string;
  input: {
    phrase: string;
    newPassword: string;
    confirmation: string;
    phrasePresent: boolean;
  };
};

function freshCopy(): ResetCopy {
  return {
    records: [RECORD],
    password: OLD_PASSWORD,
    savedPhrase: RIGHT_PHRASE,
    input: {
      phrase: RIGHT_PHRASE,
      newPassword: NEW_PASSWORD,
      confirmation: NEW_PASSWORD,
      phrasePresent: true,
    },
  };
}

const lockoutStatus = {
  passwordLockedUntil: null,
  passwordAttemptsUsed: 0,
  phraseLockedUntil: null,
  phraseAttemptsUsed: 0,
  now: 0,
};

function dependencies(copy: ResetCopy): AccountRecoveryDependencies & {
  verifyPhrase: ReturnType<typeof vi.fn<AccountRecoveryDependencies["verifyPhrase"]>>;
  setPassword: ReturnType<typeof vi.fn<AccountRecoveryDependencies["setPassword"]>>;
} {
  return {
    verifyPhrase: vi.fn(async (phrase: string) => phrase === copy.savedPhrase
      ? { ok: true as const, recoveryToken: "task-0466-approved", lockoutStatus }
      : { ok: false as const, lockoutStatus: { ...lockoutStatus, phraseAttemptsUsed: 1 } }),
    setPassword: vi.fn(async (newPassword: string) => { copy.password = newPassword; }),
  };
}

async function approvedFlow(copy: ResetCopy, deps: AccountRecoveryDependencies): Promise<AccountRecoveryFlow> {
  return submitRecoveryPhrase(
    initialAccountRecoveryFlow,
    copy.input.phrasePresent ? copy.input.phrase : "",
    deps,
  );
}

describe("TASK 0466 password reset bad inputs", () => {
  it("changes only the good copy and refuses phrase, confirmation, and phrase-presence changes", async () => {
    const good = freshCopy();
    const badPhrase = structuredClone(good);
    const badConfirmation = structuredClone(good);
    const badPhrasePresence = structuredClone(good);
    badPhrase.input.phrase = RIGHT_PHRASE.replace(/^abandon/u, "ability");
    badConfirmation.input.confirmation = "different-password-0466";
    badPhrasePresence.input.phrasePresent = false;

    expect(good.records).toEqual([RECORD]);
    console.log(`TASK0466_UI before record=${good.records[0]} count=${good.records.length}`);

    const goodDeps = dependencies(good);
    const goodApproved = await approvedFlow(good, goodDeps);
    const goodResult = await submitRecoveredPassword(
      goodApproved,
      good.input.newPassword,
      good.input.confirmation,
      goodDeps,
    );
    expect(goodResult.step).toBe("complete");
    expect(good.password).toBe(NEW_PASSWORD);
    expect(good.records).toEqual([RECORD]);
    console.log(`TASK0466_UI good_reset=password changed record=${good.records[0]} count=${good.records.length}`);

    const goodSnapshot = structuredClone(good);

    const phraseDeps = dependencies(badPhrase);
    const phraseResult = await approvedFlow(badPhrase, phraseDeps);
    expect(phraseResult.step).toBe("phrase");
    expect(phraseDeps.setPassword).not.toHaveBeenCalled();
    expect(badPhrase.password).toBe(OLD_PASSWORD);
    expect(badPhrase.records).toEqual([RECORD]);
    expect(good).toEqual(goodSnapshot);
    console.log(`TASK0466_UI bad_copy=phrase refusal=phrase wrong old_password=reads record=${badPhrase.records[0]} count=${badPhrase.records.length} good_copy_unchanged=true`);

    const confirmationDeps = dependencies(badConfirmation);
    const confirmationApproved = await approvedFlow(badConfirmation, confirmationDeps);
    const confirmationResult = await submitRecoveredPassword(
      confirmationApproved,
      badConfirmation.input.newPassword,
      badConfirmation.input.confirmation,
      confirmationDeps,
    );
    expect(confirmationResult.error).toBe("The new passwords do not match.");
    expect(confirmationDeps.setPassword).not.toHaveBeenCalled();
    expect(badConfirmation.password).toBe(OLD_PASSWORD);
    expect(badConfirmation.records).toEqual([RECORD]);
    expect(good).toEqual(goodSnapshot);
    console.log(`TASK0466_UI bad_copy=confirmation refusal=passwords differ old_password=reads record=${badConfirmation.records[0]} count=${badConfirmation.records.length} good_copy_unchanged=true`);

    const presenceDeps = dependencies(badPhrasePresence);
    const presenceResult = await approvedFlow(badPhrasePresence, presenceDeps);
    expect(presenceResult.error).toBe("Enter your password recovery phrase.");
    expect(presenceDeps.verifyPhrase).not.toHaveBeenCalled();
    expect(presenceDeps.setPassword).not.toHaveBeenCalled();
    expect(badPhrasePresence.password).toBe(OLD_PASSWORD);
    expect(badPhrasePresence.records).toEqual([RECORD]);
    expect(good).toEqual(goodSnapshot);
    console.log(`TASK0466_UI bad_copy=phrase-presence refusal=phrase required old_password=reads record=${badPhrasePresence.records[0]} count=${badPhrasePresence.records.length} good_copy_unchanged=true`);
  });
});
