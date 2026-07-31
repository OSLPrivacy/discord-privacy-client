import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  D2_CONSUME_CHALLENGE_SQL,
  consumeD2AdmissionChallengeInD1,
  loadD2AdmissionChallengeFromD1,
} from "../scripts/d2-0010-authoritative-admission.js";
import {
  createD2DurableAuthorityFixture,
  insertD2DurableChallenge,
  readD2DurableChallengeRow,
  type D2DurableAuthorityFixture,
  type D2DurableChallengeFixture,
} from "./helpers/d2-0010-durable-d1-authority.js";

const NOW_MS = 2_000_000_000_000;
const CHALLENGE: D2DurableChallengeFixture = {
  challenge_id: "a".repeat(64),
  sequence: 17,
  issued_at_ms: NOW_MS - 10_000,
  expires_at_ms: NOW_MS + 60_000,
  expected_evidence_sha256: "b".repeat(64),
  expected_transcript_root_sha256: "c".repeat(64),
  expected_authority_snapshot_sha256: "d".repeat(64),
};

function exactConsumeInput(
  challenge: D2DurableChallengeFixture = CHALLENGE,
) {
  return {
    challenge_id: challenge.challenge_id,
    sequence: challenge.sequence,
    evidence_sha256: challenge.expected_evidence_sha256,
    transcript_root_sha256: challenge.expected_transcript_root_sha256,
    authority_snapshot_sha256:
      challenge.expected_authority_snapshot_sha256,
    consumed_at_ms: NOW_MS,
    expires_at_ms: challenge.expires_at_ms,
  };
}

async function withFixture(
  run: (fixture: D2DurableAuthorityFixture) => Promise<void>,
): Promise<void> {
  const fixture = createD2DurableAuthorityFixture();
  try {
    await run(fixture);
  } finally {
    fixture.cleanup();
  }
}

function expectUnconsumed(
  fixture: D2DurableAuthorityFixture,
  challengeId = CHALLENGE.challenge_id,
): void {
  expect(readD2DurableChallengeRow(
    fixture.connection.raw,
    challengeId,
  )).toMatchObject({
    consumed_at_ms: null,
    consumed_evidence_sha256: null,
    consumed_transcript_root_sha256: null,
    consumed_authority_snapshot_sha256: null,
  });
}

function runCrossProcessContender(
  databasePath: string,
  barrierPath: string,
  input: ReturnType<typeof exactConsumeInput>,
): Promise<boolean> {
  const contender = new URL(
    "./helpers/d2-0010-durable-d1-contender.mjs",
    import.meta.url,
  );
  const encoded = Buffer.from(JSON.stringify({
    database_path: databasePath,
    barrier_path: barrierPath,
    sql: D2_CONSUME_CHALLENGE_SQL,
    ...input,
  })).toString("base64url");
  return new Promise((resolve, reject) => {
    // `URL.pathname` of a file: URL is "/D:/a/..." on Windows, and spawn
    // resolves that against the drive root as "D:\\D:\\a\\...". Only
    // `fileURLToPath` produces a path Node can actually open on both
    // platforms.
    const child = spawn(process.execPath, [fileURLToPath(contender), encoded], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error("cross-process D1 contender timed out"));
    }, 15_000);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      if (code !== 0) {
        reject(new Error(
          `cross-process contender failed code=${code} signal=${signal}: ${stderr}`,
        ));
        return;
      }
      try {
        resolve(JSON.parse(stdout.trim()) as boolean);
      } catch {
        reject(new Error(`invalid contender output: ${stdout}`));
      }
    });
  });
}

describe("durable D1-backed D2 admission challenge authority", () => {
  it("loads the exact challenge and consumes it exactly once", async () => {
    await withFixture(async (fixture) => {
      insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);

      await expect(loadD2AdmissionChallengeFromD1(
        fixture.connection.d1,
        CHALLENGE.challenge_id,
      )).resolves.toEqual(CHALLENGE);

      const consume = exactConsumeInput();
      await expect(consumeD2AdmissionChallengeInD1(
        fixture.connection.d1,
        consume,
      )).resolves.toBe(true);
      await expect(consumeD2AdmissionChallengeInD1(
        fixture.connection.d1,
        consume,
      )).resolves.toBe(false);

      expect(readD2DurableChallengeRow(
        fixture.connection.raw,
        CHALLENGE.challenge_id,
      )).toEqual({
        ...CHALLENGE,
        consumed_at_ms: consume.consumed_at_ms,
        consumed_evidence_sha256: consume.evidence_sha256,
        consumed_transcript_root_sha256: consume.transcript_root_sha256,
        consumed_authority_snapshot_sha256:
          consume.authority_snapshot_sha256,
      });
    });
  });

  it.each([
    ["evidence", (input: ReturnType<typeof exactConsumeInput>) => {
      input.evidence_sha256 = "e".repeat(64);
    }],
    ["transcript root", (input: ReturnType<typeof exactConsumeInput>) => {
      input.transcript_root_sha256 = "e".repeat(64);
    }],
    ["authority snapshot", (input: ReturnType<typeof exactConsumeInput>) => {
      input.authority_snapshot_sha256 = "e".repeat(64);
    }],
  ] as const)(
    "refuses a wrong %s digest without consuming or poisoning the challenge",
    async (_label, mutate) => {
      await withFixture(async (fixture) => {
        insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);
        const wrong = exactConsumeInput();
        mutate(wrong);

        await expect(consumeD2AdmissionChallengeInD1(
          fixture.connection.d1,
          wrong,
        )).resolves.toBe(false);
        expectUnconsumed(fixture);

        await expect(consumeD2AdmissionChallengeInD1(
          fixture.connection.d1,
          exactConsumeInput(),
        )).resolves.toBe(true);
      });
    },
  );

  it("retains consumed refusal and exact receipt digests after close/reopen", async () => {
    await withFixture(async (fixture) => {
      insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);
      const consume = exactConsumeInput();
      await expect(consumeD2AdmissionChallengeInD1(
        fixture.connection.d1,
        consume,
      )).resolves.toBe(true);
      fixture.connection.close();

      const reopened = fixture.openConnection();
      await expect(loadD2AdmissionChallengeFromD1(
        reopened.d1,
        CHALLENGE.challenge_id,
      )).resolves.toEqual(CHALLENGE);
      await expect(consumeD2AdmissionChallengeInD1(
        reopened.d1,
        consume,
      )).resolves.toBe(false);
      expect(readD2DurableChallengeRow(
        reopened.raw,
        CHALLENGE.challenge_id,
      )).toEqual({
        ...CHALLENGE,
        consumed_at_ms: consume.consumed_at_ms,
        consumed_evidence_sha256: CHALLENGE.expected_evidence_sha256,
        consumed_transcript_root_sha256:
          CHALLENGE.expected_transcript_root_sha256,
        consumed_authority_snapshot_sha256:
          CHALLENGE.expected_authority_snapshot_sha256,
      });
    });
  });

  it("allows exactly one winner across two independent SQLite connections", async () => {
    await withFixture(async (fixture) => {
      insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);
      const second = fixture.openConnection();
      const consume = exactConsumeInput();

      const results = await Promise.all([
        consumeD2AdmissionChallengeInD1(fixture.connection.d1, consume),
        consumeD2AdmissionChallengeInD1(second.d1, consume),
      ]);
      expect(results.filter(Boolean)).toHaveLength(1);
      expect(results.filter((result) => !result)).toHaveLength(1);
      expect(readD2DurableChallengeRow(
        second.raw,
        CHALLENGE.challenge_id,
      )).toMatchObject({
        consumed_at_ms: consume.consumed_at_ms,
        consumed_evidence_sha256: consume.evidence_sha256,
        consumed_transcript_root_sha256: consume.transcript_root_sha256,
        consumed_authority_snapshot_sha256:
          consume.authority_snapshot_sha256,
      });
    });
  });

  it("allows exactly one winner across two separate Node processes", async () => {
    await withFixture(async (fixture) => {
      insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);
      const barrierPath = `${fixture.databasePath}.start`;
      const input = exactConsumeInput();
      const first = runCrossProcessContender(
        fixture.databasePath,
        barrierPath,
        input,
      );
      const second = runCrossProcessContender(
        fixture.databasePath,
        barrierPath,
        input,
      );
      writeFileSync(barrierPath, "go", { flag: "wx" });
      const results = await Promise.all([first, second]);

      expect(results.filter(Boolean)).toHaveLength(1);
      expect(results.filter((result) => !result)).toHaveLength(1);
      expect(readD2DurableChallengeRow(
        fixture.connection.raw,
        CHALLENGE.challenge_id,
      )).toMatchObject({
        consumed_at_ms: input.consumed_at_ms,
        consumed_evidence_sha256: input.evidence_sha256,
        consumed_transcript_root_sha256: input.transcript_root_sha256,
        consumed_authority_snapshot_sha256:
          input.authority_snapshot_sha256,
      });
    });
  });

  it("refuses a second challenge with a duplicate durable receipt sequence", async () => {
    await withFixture(async (fixture) => {
      insertD2DurableChallenge(fixture.connection.raw, CHALLENGE);
      expect(() => insertD2DurableChallenge(fixture.connection.raw, {
        ...CHALLENGE,
        challenge_id: "e".repeat(64),
      })).toThrow(/UNIQUE/);
      expectUnconsumed(fixture);
    });
  });

  it("refuses an expired challenge and leaves it durably unconsumed", async () => {
    await withFixture(async (fixture) => {
      const expired = {
        ...CHALLENGE,
        issued_at_ms: NOW_MS - 60_000,
        expires_at_ms: NOW_MS - 1,
      };
      insertD2DurableChallenge(fixture.connection.raw, expired);

      await expect(consumeD2AdmissionChallengeInD1(
        fixture.connection.d1,
        exactConsumeInput(expired),
      )).rejects.toThrow(/stale|malformed/);
      expectUnconsumed(fixture, expired.challenge_id);
    });
  });
});
