import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import {
  oslChatsViewMarkup,
  type OslChatFriend,
  type OslChatsViewModel,
} from "./osl-chats-view";

const REPOSITORY = path.resolve(import.meta.dirname, "../../..");
const MANIFEST = path.join(REPOSITORY, "tools/task-3096-build-proof/Cargo.toml");
const FIXTURES = path.join(REPOSITORY, "tools/task-3096-build-proof/tests/fixtures");
const PROOF_NAME = "FRIEND-LAPTOP-3100.proof.json";
const BUILD_FINGERPRINT = `3100${"a".repeat(60)}`;
const CHECKED_AT = "1787000000";
const MISSING_PROOF_WORDING =
  "OSL cannot check this person's app because its proof is missing.";
const temporary = mkdtempSync(path.join(tmpdir(), "osl-task-3100-"));
const proofPath = path.join(temporary, PROOF_NAME);

afterAll(() => rmSync(temporary, { recursive: true, force: true }));

function cargoRun(binary: string, args: readonly string[]): string {
  return execFileSync(
    "cargo",
    ["run", "--quiet", "--manifest-path", MANIFEST, "--bin", binary, "--", ...args],
    {
      cwd: REPOSITORY,
      encoding: "utf8",
      env: process.env,
      maxBuffer: 1024 * 1024,
    },
  ).trim();
}

function signProof(): string {
  return cargoRun("osl-sign-build-proof", [
    "--signing-key-file",
    path.join(FIXTURES, "task3097-trusted-secret.base64"),
    "--build-fingerprint",
    BUILD_FINGERPRINT,
    "--device-id",
    "device:friend-laptop-3100",
    "--person-id",
    "person:friend-3100",
    "--made-at-unix-seconds",
    "1786000000",
    "--stops-counting-at-unix-seconds",
    "1788000000",
  ]);
}

function checkProof(): string {
  return cargoRun("osl-check-build-proof", [
    "--proof-file",
    proofPath,
    "--trusted-public-key-file",
    path.join(FIXTURES, "task3097-trusted-public.base64"),
    "--build-fingerprint",
    BUILD_FINGERPRINT,
    "--at-unix-seconds",
    CHECKED_AT,
  ]);
}

function friend(answer: string, verificationTwoWay = true): OslChatFriend {
  return {
    personId: "friend-3100",
    nickname: "Two-Way Friend 3100",
    verified: true,
    ready: true,
    preview: "Both facts stay separate",
    previewVisible: true,
    unreadCount: 0,
    verificationTwoWay,
    buildProof: { proofName: PROOF_NAME, answer },
  };
}

function render(answer: string, verificationTwoWay = true): string {
  const namedFriend = friend(answer, verificationTwoWay);
  const model: OslChatsViewModel = {
    friends: [namedFriend],
    activePersonId: namedFriend.personId,
    messages: [],
    draft: "",
    busy: false,
  };
  return oslChatsViewMarkup(model);
}

function count(markup: string, attribute: string): number {
  return markup.split(attribute).length - 1;
}

function marks(markup: string) {
  return {
    consent: count(markup, 'data-osl-consent-mark="two-way"'),
    unmodifiedBuild: count(markup, 'data-osl-build-mark="unmodified"'),
    cannotTell: count(markup, 'data-osl-build-mark="cannot-tell"'),
  };
}

describe("TASK 3100 separate consent and build marks", () => {
  it("keeps consent present while one named proof is removed and restored byte-for-byte", () => {
    const signedProof = signProof();
    writeFileSync(proofPath, signedProof);
    const originalBytes = readFileSync(proofPath);

    const initialAnswer = checkProof();
    const initial = marks(render(initialAnswer));
    expect(initialAnswer).toBe("unmodified");
    expect(initial).toEqual({ consent: 2, unmodifiedBuild: 2, cannotTell: 0 });

    unlinkSync(proofPath);
    const missingAnswer = checkProof();
    const removed = marks(render(missingAnswer));
    expect(missingAnswer).toBe(MISSING_PROOF_WORDING);
    expect(removed).toEqual({ consent: 2, unmodifiedBuild: 0, cannotTell: 2 });

    writeFileSync(proofPath, originalBytes);
    const restoredAnswer = checkProof();
    const restoredBytes = readFileSync(proofPath);
    const restored = marks(render(restoredAnswer));
    expect(restoredAnswer).toBe("unmodified");
    expect(restoredBytes.equals(originalBytes)).toBe(true);
    expect(restored).toEqual({ consent: 2, unmodifiedBuild: 2, cannotTell: 0 });

    const consentOnly = marks(oslChatsViewMarkup({
      friends: [{ ...friend(restoredAnswer), buildProof: undefined }],
      activePersonId: "friend-3100",
      messages: [],
      draft: "",
      busy: false,
    }));
    const buildOnly = marks(render(restoredAnswer, false));
    expect(consentOnly).toEqual({ consent: 2, unmodifiedBuild: 0, cannotTell: 0 });
    expect(buildOnly).toEqual({ consent: 0, unmodifiedBuild: 2, cannotTell: 0 });

    console.log(
      `TASK3100_INITIAL friend="Two-Way Friend 3100" proof=${PROOF_NAME} checker=${initialAnswer} consent_mark_count=${initial.consent} unmodified_build_mark_count=${initial.unmodifiedBuild} cannot_tell_count=${initial.cannotTell}`,
    );
    console.log(
      `TASK3100_REMOVED proof=${PROOF_NAME} proof_file_count=0 checker="${missingAnswer}" consent_mark_count=${removed.consent} unmodified_build_mark_count=${removed.unmodifiedBuild} cannot_tell_count=${removed.cannotTell}`,
    );
    console.log(
      `TASK3100_RESTORED proof=${PROOF_NAME} proof_file_count=1 same_proof_bytes=${restoredBytes.equals(originalBytes)} checker=${restoredAnswer} consent_mark_count=${restored.consent} unmodified_build_mark_count=${restored.unmodifiedBuild} cannot_tell_count=${restored.cannotTell}`,
    );
    console.log(
      `TASK3100_INDEPENDENCE consent_only_build_mark_count=${consentOnly.unmodifiedBuild} build_only_consent_mark_count=${buildOnly.consent}`,
    );
  }, 120_000);
});
