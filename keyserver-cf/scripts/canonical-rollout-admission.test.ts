import {
  createHash,
  generateKeyPairSync,
  sign,
} from "node:crypto";
import {
  chmod,
  link,
  mkdtemp,
  readFile,
  rm,
  symlink,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  CANONICAL_ROLLOUT_ADMISSION_FORMAT,
  CANONICAL_ROLLOUT_COMPLETION_FORMAT,
  CANONICAL_PREKEY_MIGRATION,
  CANONICAL_ROLLOUT_DATABASE,
  CANONICAL_ROLLOUT_EVIDENCE_FORMAT,
  CANONICAL_ROLLOUT_EVIDENCE_DOMAIN,
  CANONICAL_ROLLOUT_EVIDENCE_ENVELOPE_FORMAT,
  CANONICAL_ROLLOUT_MIGRATION_GATE_FORMAT,
  CANONICAL_ROLLOUT_MIGRATION,
  CANONICAL_ROLLOUT_SOURCE_PATHS,
  createCanonicalRolloutProvisioningAdmission,
  validateCanonicalRolloutMigrationGate,
  validateCanonicalRolloutCompletionEvidence,
  validateCanonicalRolloutProvisioningReceipt,
  validateCanonicalRolloutSourceClosure,
} from "./canonical-rollout-admission-contract.mjs";
import {
  parseCanonicalRolloutAdmissionArgs,
  runCanonicalRolloutAdmissionCli,
} from "./create-canonical-rollout-admission.mjs";
import {
  buildGenesisProvisioning,
  CANONICAL_GENESIS_RECOVERY_PATH,
  assertCurrentCanonicalRolloutSource,
  loadCanonicalRecoveryManifest,
  provisionSenderFilterGenesis,
  reserveCanonicalRecoveryManifest,
  runGenesisProvisioningCli,
  runWranglerGenesisProvision,
  writeDerivedProvisioningReceipt,
} from "./provision-sender-filter-rollout-genesis.mjs";

const ROOT = path.resolve(import.meta.dirname, "../..");
const NOW = Date.parse("2026-07-27T20:00:00.000Z");
const COMMIT = "1".repeat(40);
const REPOSITORY_TREE = "2".repeat(40);
const KEYSERVER_TREE = "3".repeat(40);
const VERSION = "11111111-2222-4333-8444-555555555555";
const DEPLOYMENT = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const NONCE = "99999999-8888-4777-8666-555555555555";
const PRODUCER_KEY_ID = "test-canonical-rollout-verifier";
const producerPair = generateKeyPairSync("ed25519");
const TRUSTED_PRODUCERS = {
  [PRODUCER_KEY_ID]: {
    identity: "test://canonical-rollout-verifier",
    public_key_spki_b64: producerPair.publicKey
      .export({ format: "der", type: "spki" })
      .toString("base64"),
  },
};

function canonical(value: any): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function signEvidencePayload(payload: Record<string, unknown>) {
  const payloadBytes = Buffer.from(canonical(payload));
  return {
    format: CANONICAL_ROLLOUT_EVIDENCE_ENVELOPE_FORMAT,
    producer_key_id: PRODUCER_KEY_ID,
    payload,
    payload_sha256: sha256(payloadBytes),
    signature_b64: sign(
      null,
      Buffer.concat([
        Buffer.from(CANONICAL_ROLLOUT_EVIDENCE_DOMAIN),
        payloadBytes,
      ]),
      producerPair.privateKey,
    ).toString("base64"),
  };
}

async function sourceValues() {
  return Object.fromEntries(
    await Promise.all(
      CANONICAL_ROLLOUT_SOURCE_PATHS.map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(ROOT, sourcePath)),
      ]),
    ),
  );
}

function sha256(bytes: Buffer): string {
  return createHash("sha256").update(bytes).digest("hex");
}

async function positiveEvidence() {
  const migration = await readFile(
    path.join(
      ROOT,
      "keyserver-cf/migrations",
      CANONICAL_ROLLOUT_MIGRATION,
    ),
  );
  const prekeyMigration = await readFile(
    path.join(
      ROOT,
      "keyserver-cf/migrations",
      CANONICAL_PREKEY_MIGRATION,
    ),
  );
  const payload = {
    format: CANONICAL_ROLLOUT_EVIDENCE_FORMAT,
    database: { ...CANONICAL_ROLLOUT_DATABASE },
    capture: {
      started_at: "2026-07-27T19:59:50.000Z",
      finished_at: "2026-07-27T19:59:59.000Z",
    },
    migration: {
      name: CANONICAL_ROLLOUT_MIGRATION,
      sha256: sha256(migration),
      applied_order: 33,
      applied_at: "2026-07-27T19:59:51.000Z",
      database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
    },
    prekey_migration: {
      name: CANONICAL_PREKEY_MIGRATION,
      sha256: sha256(prekeyMigration),
      applied_order: 34,
      applied_at: "2026-07-27T19:59:52.000Z",
      database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
    },
    worker: {
      name: "oslprivacy-keyserver",
      commit: COMMIT,
      repository_tree: REPOSITORY_TREE,
      keyserver_tree: KEYSERVER_TREE,
      version_id: VERSION,
      deployment_id: DEPLOYMENT,
      deployed_at: "2026-07-27T19:59:55.000Z",
      binding: CANONICAL_ROLLOUT_DATABASE.binding,
      database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
      provider_observation: {
        provider: "cloudflare-workers-api",
        account_fingerprint_sha256: "9".repeat(64),
        script_name: "oslprivacy-keyserver",
        version_id: VERSION,
        deployment_id: DEPLOYMENT,
        traffic_percent: 100,
        observed_at: "2026-07-27T19:59:57.000Z",
      },
    },
  };
  return signEvidencePayload(payload);
}

async function migrationGateInputs() {
  const sourceFiles = validateCanonicalRolloutSourceClosure(
    await sourceValues(),
  );
  const evidence = await positiveEvidence();
  return {
    migrationRows: [
      evidence.payload.migration,
      evidence.payload.prekey_migration,
    ],
    sourceFiles,
    worker: {
      name: "oslprivacy-keyserver",
      deployed_at: "2026-07-27T19:59:55.000Z",
      binding: CANONICAL_ROLLOUT_DATABASE.binding,
      database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
    },
  };
}

async function positiveReceipt() {
  return createCanonicalRolloutProvisioningAdmission({
    anchor: {
      commit: COMMIT,
      repository_tree: REPOSITORY_TREE,
      keyserver_tree: KEYSERVER_TREE,
    },
    fileValues: await sourceValues(),
    evidence: await positiveEvidence(),
    nowMs: NOW,
    receiptNonce: NONCE,
    trustedProducers: TRUSTED_PRODUCERS,
  });
}

function completionEvidence(receipt: Awaited<ReturnType<typeof positiveReceipt>>) {
  const nonceSha = "4".repeat(64);
  return {
    format: CANONICAL_ROLLOUT_COMPLETION_FORMAT,
    admission_receipt: {
      row_count: 1,
      sha256: receipt.payload_sha256,
    },
    genesis: {
      row_count: 1,
      singleton: 1,
      nonce_sha256: nonceSha,
      provisioned_at_ms: NOW,
      consumed_at_ms: NOW + 1,
    },
    recovery_manifest: {
      state: "retained-after-ambiguous-failure",
      file_mode: 0o600,
      raw_nonce_present: true,
      nonce_sha256: nonceSha,
      provisioning_admission_sha256: receipt.payload_sha256,
      manifest_sha256: "5".repeat(64),
    },
    root: {
      row_count: 1,
      singleton: 1,
      root_user_id: `osl1_${"a".repeat(52)}`,
      root_ed25519_pub: "A".repeat(44),
      identity_bundle_sha256: "6".repeat(64),
      capability_version: 1,
      monotonic_version: 2,
      last_observation_sha256: "7".repeat(64),
      provisioned_at_ms: NOW + 1,
      updated_at_ms: NOW + 2,
    },
    cas: {
      expected_monotonic_version: 1,
      next_monotonic_version: 2,
      applied_changes: 1,
      stale_expected_monotonic_version: 1,
      stale_changes: 0,
    },
    negative_controls: {
      duplicate_genesis_refused: true,
      second_root_refused: true,
      reset_root_refused: true,
      delete_root_refused: true,
      delete_genesis_refused: true,
      replayed_receipt_refused: true,
      delete_receipt_refused: true,
    },
  };
}

describe("canonical rollout predeploy and provisioning admission", () => {
  it("binds exact current source, migration-before-worker order, D1 identity, and a non-authorizing provisioning receipt", async () => {
    const sources = await sourceValues();
    const evidence = await positiveEvidence();
    const files = validateCanonicalRolloutSourceClosure(sources);
    expect(files).toHaveLength(CANONICAL_ROLLOUT_SOURCE_PATHS.length);
    expect(files.every((entry) => entry.bytes > 0)).toBe(true);
    expect(() => createCanonicalRolloutProvisioningAdmission({
      anchor: {
        commit: COMMIT,
        repository_tree: REPOSITORY_TREE,
        keyserver_tree: KEYSERVER_TREE,
      },
      fileValues: sources,
      evidence,
      nowMs: NOW,
      receiptNonce: NONCE,
    })).toThrow(/not independently trusted/);

    const receipt = await positiveReceipt();
    expect(receipt.format).toBe(CANONICAL_ROLLOUT_ADMISSION_FORMAT);
    expect(receipt.provisioning_admitted).toBe(true);
    expect(receipt.deployment_admitted).toBe(false);
    expect(receipt.execution_authorized).toBe(false);
    expect(receipt.source).toEqual({
      commit: COMMIT,
      repository_tree: REPOSITORY_TREE,
      keyserver_tree: KEYSERVER_TREE,
    });
    expect(receipt.database).toEqual(CANONICAL_ROLLOUT_DATABASE);
    expect(receipt.payload_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(
      validateCanonicalRolloutProvisioningReceipt(receipt, {
        expectedCommit: COMMIT,
        expectedRepositoryTree: REPOSITORY_TREE,
        expectedKeyserverTree: KEYSERVER_TREE,
        nowMs: NOW,
        trustedProducers: TRUSTED_PRODUCERS,
      }),
    ).toEqual(receipt);
  });

  it("refuses worker-first, stale, wrong-database, wrong-source, and empty evidence", async () => {
    const files = await sourceValues();
    const make = async (mutate: (evidence: any) => void) => {
      const evidence = await positiveEvidence();
      mutate(evidence);
      const resigned = signEvidencePayload(evidence.payload);
      return () => createCanonicalRolloutProvisioningAdmission({
        anchor: {
          commit: COMMIT,
          repository_tree: REPOSITORY_TREE,
          keyserver_tree: KEYSERVER_TREE,
        },
        fileValues: files,
        evidence: resigned,
        nowMs: NOW,
        receiptNonce: NONCE,
        trustedProducers: TRUSTED_PRODUCERS,
      });
    };

    expect(await make((e) => {
      e.payload.worker.deployed_at = "2026-07-27T19:59:50.000Z";
    })).toThrow(/worker-first/);
    expect(await make((e) => {
      e.payload.database.database_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    })).toThrow(/database_id mismatch/);
    expect(await make((e) => {
      e.payload.worker.commit = "f".repeat(40);
    })).toThrow(/Worker source or D1 binding mismatch/);
    expect(await make((e) => {
      e.payload.capture.started_at = "2026-07-27T19:50:00.000Z";
      e.payload.capture.finished_at = "2026-07-27T19:50:01.000Z";
    })).toThrow(/stale or invalid/);
    expect(await make((e) => {
      delete e.payload.migration;
    })).toThrow(/fields are not exact/);
  });

  it("keeps scheme-1 Worker activation refused until migrations 0033/0034 are deployed in exact order", async () => {
    const inputs = await migrationGateInputs();
    expect(() => validateCanonicalRolloutMigrationGate({
      ...inputs,
      migrationRows: [],
    })).toThrow(/0033\/0034 are undeployed/);
    expect(() => validateCanonicalRolloutMigrationGate({
      ...inputs,
      migrationRows: [inputs.migrationRows[0]],
    })).toThrow(/0033\/0034 are undeployed/);
    expect(() => validateCanonicalRolloutMigrationGate({
      ...inputs,
      migrationRows: [
        { ...inputs.migrationRows[0], applied_order: 34 },
        { ...inputs.migrationRows[1], applied_order: 33 },
      ],
    })).toThrow(/applied order is not exact/);
    expect(() => validateCanonicalRolloutMigrationGate({
      ...inputs,
      migrationRows: [
        inputs.migrationRows[0],
        {
          ...inputs.migrationRows[1],
          applied_at: "2026-07-27T19:59:50.000Z",
        },
      ],
    })).toThrow(/timestamp order is invalid/);
    expect(() => validateCanonicalRolloutMigrationGate({
      ...inputs,
      worker: {
        ...inputs.worker,
        deployed_at: "2026-07-27T19:59:51.000Z",
      },
    })).toThrow(/activation before migrations 0033\/0034 is refused/);

    const gate = validateCanonicalRolloutMigrationGate(inputs);
    expect(gate.format).toBe(CANONICAL_ROLLOUT_MIGRATION_GATE_FORMAT);
    expect(gate.migrations_deployed).toBe(true);
    expect(gate.worker_activation_precondition_satisfied).toBe(true);
    expect(gate.execution_authorized).toBe(false);
    expect(gate.migration_execution_authorized).toBe(false);
    expect(gate.worker_activation_authorized).toBe(false);
    expect(gate.migration.name).toBe(CANONICAL_ROLLOUT_MIGRATION);
    expect(gate.prekey_migration.name).toBe(CANONICAL_PREKEY_MIGRATION);
  });

  it("refuses migration-byte substitution and receipt tampering", async () => {
    const evidence = await positiveEvidence();
    const files = await sourceValues();
    evidence.payload.migration.sha256 = "f".repeat(64);
    const resigned = signEvidencePayload(evidence.payload);
    expect(() => createCanonicalRolloutProvisioningAdmission({
      anchor: {
        commit: COMMIT,
        repository_tree: REPOSITORY_TREE,
        keyserver_tree: KEYSERVER_TREE,
      },
      fileValues: files,
      evidence: resigned,
      nowMs: NOW,
      receiptNonce: NONCE,
      trustedProducers: TRUSTED_PRODUCERS,
    })).toThrow(/applied migration digest/);

    const receipt = structuredClone(await positiveReceipt());
    receipt.worker.commit = "f".repeat(40);
    expect(() => validateCanonicalRolloutProvisioningReceipt(receipt, {
      expectedCommit: COMMIT,
      expectedRepositoryTree: REPOSITORY_TREE,
      expectedKeyserverTree: KEYSERVER_TREE,
      nowMs: NOW,
      trustedProducers: TRUSTED_PRODUCERS,
    })).toThrow(/digest mismatch|Worker source mismatch/);
  });

  it("makes receipt consumption and nonempty post-provision readback one D1 singleton mutation", async () => {
    const receipt = await positiveReceipt();
    const built = buildGenesisProvisioning(
      Buffer.alloc(32, 0x42),
      NOW,
      receipt,
    );
    expect(built.sql).not.toMatch(/\b(?:BEGIN|COMMIT)\b/);
    expect(built.sql).toContain(
      "INSERT INTO sender_filter_rollout_genesis (singleton",
    );
    expect(built.sql).toContain("admission_receipt_sha256");
    expect(built.sql).toContain("RETURNING singleton");
    expect(built.sql).toContain("worker_commit, repository_tree, keyserver_tree");
    expect(built.sql).toContain(receipt.payload_sha256);
    expect(built.sql).not.toContain(built.manifest.genesis_nonce);

    const wranglerCommands: string[] = [];
    const accepted = runWranglerGenesisProvision(
      built.sql,
      built.expectedReadback,
      ((...spawnArgs: any[]) => {
        const args = spawnArgs[1] as string[];
        const command = args[args.indexOf("--command") + 1]!;
        wranglerCommands.push(command);
        return {
          status: 0,
          stdout: JSON.stringify([{
            success: true,
            // Only the second, independent SELECT is authoritative.
            results: command.startsWith("SELECT")
              ? [built.expectedReadback]
              : [],
          }]),
          stderr: "",
        };
      }) as any,
    );
    expect(accepted).toEqual(built.expectedReadback);
    expect(wranglerCommands).toHaveLength(2);
    expect(wranglerCommands[0]).toBe(built.sql);
    expect(wranglerCommands[1]).toMatch(
      /^SELECT singleton, nonce_sha256, admission_receipt_sha256,/,
    );

    let calls = 0;
    expect(() => runWranglerGenesisProvision(
      built.sql,
      built.expectedReadback,
      (() => {
        calls += 1;
        return {
          status: 0,
          stdout: JSON.stringify([{
            success: true,
            results: calls === 1 ? [built.expectedReadback] : [],
          }]),
          stderr: "",
        };
      }) as any,
    )).toThrow(/readback is empty or mismatched/);
  });

  it("requires a signed provider observation of the exact active version at 100% traffic", async () => {
    const files = await sourceValues();
    for (const mutate of [
      (payload: any) => { delete payload.worker.provider_observation; },
      (payload: any) => {
        payload.worker.provider_observation.version_id =
          "99999999-2222-4333-8444-555555555555";
      },
      (payload: any) => {
        payload.worker.provider_observation.traffic_percent = 99;
      },
      (payload: any) => {
        payload.worker.provider_observation.observed_at =
          "2026-07-27T19:59:54.000Z";
      },
    ]) {
      const evidence = await positiveEvidence();
      mutate(evidence.payload);
      const resigned = signEvidencePayload(evidence.payload);
      expect(() => createCanonicalRolloutProvisioningAdmission({
        anchor: {
          commit: COMMIT,
          repository_tree: REPOSITORY_TREE,
          keyserver_tree: KEYSERVER_TREE,
        },
        fileValues: files,
        evidence: resigned,
        nowMs: NOW,
        receiptNonce: NONCE,
        trustedProducers: TRUSTED_PRODUCERS,
      })).toThrow();
    }
  });

  it("serializes concurrent and sequential alternate-report attempts at one canonical recovery inode", async () => {
    const directory = await mkdtemp(
      path.join(tmpdir(), "osl-canonical-rollout-"),
    );
    const recoveryPath = path.join(directory, "canonical.json");
    const recoveryOptions = {
      expectedPath: recoveryPath,
      expectedUid: process.getuid(),
    };
    try {
      const receipt = await positiveReceipt();
      const first = buildGenesisProvisioning(
        Buffer.alloc(32, 0x11),
        NOW,
        receipt,
      );
      const second = buildGenesisProvisioning(
        Buffer.alloc(32, 0x22),
        NOW + 1,
        receipt,
      );
      let remoteMutations = 0;
      const results = await Promise.allSettled([
        provisionSenderFilterGenesis(
          recoveryPath,
          first.manifest,
          first.sql,
          () => { remoteMutations += 1; },
          recoveryOptions,
        ),
        provisionSenderFilterGenesis(
          recoveryPath,
          second.manifest,
          second.sql,
          () => { remoteMutations += 1; },
          recoveryOptions,
        ),
      ]);
      expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
      expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
      expect(remoteMutations).toBe(1);

      const retained = await loadCanonicalRecoveryManifest(
        recoveryPath,
        recoveryOptions,
      );
      expect([
        first.manifest.genesis_nonce,
        second.manifest.genesis_nonce,
      ]).toContain(retained.genesis_nonce);

      await expect(provisionSenderFilterGenesis(
        recoveryPath,
        first.manifest,
        first.sql,
        () => { remoteMutations += 1; },
        recoveryOptions,
      )).rejects.toMatchObject({ code: "EEXIST" });
      expect(remoteMutations).toBe(1);

      const outputA = path.join(directory, "report-a.json");
      const outputB = path.join(directory, "report-b.json");
      await writeDerivedProvisioningReceipt(
        outputA,
        retained,
        first.expectedReadback,
      );
      await writeDerivedProvisioningReceipt(
        outputB,
        retained,
        first.expectedReadback,
      );
      for (const output of [outputA, outputB]) {
        const report = await readFile(output, "utf8");
        expect(report).not.toContain(retained.genesis_nonce);
        expect(JSON.parse(report).genesis_nonce_sha256).toBe(
          retained.genesis_nonce_sha256,
        );
      }
      expect(remoteMutations).toBe(1);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("refuses wrong mode/owner, symlink, and hardlink replacement recovery authority", async () => {
    const directory = await mkdtemp(
      path.join(tmpdir(), "osl-canonical-rollout-path-"),
    );
    const recoveryPath = path.join(directory, "canonical.json");
    const options = {
      expectedPath: recoveryPath,
      expectedUid: process.getuid(),
    };
    const receipt = await positiveReceipt();
    const built = buildGenesisProvisioning(
      Buffer.alloc(32, 0x33),
      NOW,
      receipt,
    );
    try {
      await chmod(directory, 0o755);
      await expect(reserveCanonicalRecoveryManifest(
        recoveryPath,
        built.manifest,
        options,
      )).rejects.toThrow(/owner-only/);
      await chmod(directory, 0o700);
      await expect(reserveCanonicalRecoveryManifest(
        recoveryPath,
        built.manifest,
        { ...options, expectedUid: process.getuid() + 1 },
      )).rejects.toThrow(/owner-only/);

      const symlinkPath = path.join(directory, "symlink.json");
      await symlink(recoveryPath, symlinkPath);
      await expect(loadCanonicalRecoveryManifest(
        symlinkPath,
        { ...options, expectedPath: symlinkPath },
      )).rejects.toThrow(/single-link regular file/);
      await rm(symlinkPath);

      await reserveCanonicalRecoveryManifest(
        recoveryPath,
        built.manifest,
        options,
      );
      const hardlinkPath = path.join(directory, "replacement.json");
      await link(recoveryPath, hardlinkPath);
      await expect(loadCanonicalRecoveryManifest(
        recoveryPath,
        options,
      )).rejects.toThrow(/single-link regular file/);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("executes shipping CLI recovery at one fixed path across alternate output receipts and reuses the retained nonce", async () => {
    const receipt = await positiveReceipt();
    let retained: any = null;
    let remoteMutations = 0;
    const reports: Array<{ output: string; nonce: string }> = [];
    const common = [
      "--apply",
      "--admission", "/private/admission.json",
      "--expected-commit", COMMIT,
      "--expected-tree", REPOSITORY_TREE,
      "--expected-keyserver-tree", KEYSERVER_TREE,
    ];
    const dependencies = {
      assertCurrentSource: () => undefined,
      readAdmission: async () => receipt,
      loadRecovery: async (recoveryPath: string) => {
        expect(recoveryPath).toBe(CANONICAL_GENESIS_RECOVERY_PATH);
        return retained;
      },
      randomBytes: () => Buffer.alloc(32, 0x7b),
      now: () => NOW,
      provision: async (
        recoveryPath: string,
        manifest: any,
        _sql: string,
        _mutate: unknown,
      ) => {
        expect(recoveryPath).toBe(CANONICAL_GENESIS_RECOVERY_PATH);
        retained = structuredClone(manifest);
        remoteMutations += 1;
        throw new Error("ambiguous transport failure");
      },
      resumeProvision: async (
        recoveryPath: string,
        manifest: any,
        _sql: string,
        mutate: (sql: string) => unknown,
      ) => {
        expect(recoveryPath).toBe(CANONICAL_GENESIS_RECOVERY_PATH);
        expect(manifest.genesis_nonce).toBe(retained.genesis_nonce);
        // A retry may call D1, but the same singleton/digest can mutate
        // remotely at most once. The authoritative readback recovers success.
        return mutate("same-nonce-retry");
      },
      wranglerProvision: () => ({
        singleton: 1,
        nonce_sha256: retained.genesis_nonce_sha256,
        admission_receipt_sha256: receipt.payload_sha256,
        worker_commit: COMMIT,
        repository_tree: REPOSITORY_TREE,
        keyserver_tree: KEYSERVER_TREE,
        provisioned_at_ms: NOW,
        consumed_at_ms: null,
      }),
      writeReceipt: async (output: string, manifest: any) => {
        reports.push({ output, nonce: manifest.genesis_nonce });
      },
      write: () => undefined,
    };
    await expect(runGenesisProvisioningCli(
      [...common, "--output", "/private/report-a.json"],
      dependencies,
    )).rejects.toThrow(/ambiguous/);
    const retainedNonce = retained.genesis_nonce;
    await expect(runGenesisProvisioningCli(
      [...common, "--output", "/private/report-b.json"],
      dependencies,
    )).resolves.toMatchObject({
      manifest: { genesis_nonce: retainedNonce },
    });
    expect(remoteMutations).toBe(1);
    expect(reports).toEqual([{
      output: "/private/report-b.json",
      nonce: retainedNonce,
    }]);
  });

  it("syncs the recovery file before its directory entry in the shipping reservation", async () => {
    const receipt = await positiveReceipt();
    const built = buildGenesisProvisioning(
      Buffer.alloc(32, 0x7c),
      NOW,
      receipt,
    );
    const events: string[] = [];
    const directoryStat = {
      isDirectory: () => true,
      isFile: () => false,
      isSymbolicLink: () => false,
      mode: 0o700,
      uid: process.getuid(),
      nlink: 1,
      dev: 1,
      ino: 10,
      size: 0,
    };
    const fileStat = {
      isDirectory: () => false,
      isFile: () => true,
      isSymbolicLink: () => false,
      mode: 0o600,
      uid: process.getuid(),
      nlink: 1,
      dev: 1,
      ino: 11,
      size: 100,
    };
    const recoveryPath = "/fixed/canonical-rollout-genesis.json";
    const io = {
      open: async (_target: string, flags: string) => {
        if (flags === "r") {
          events.push("directory-open");
          return {
            stat: async () => directoryStat,
            sync: async () => { events.push("directory-sync"); },
            close: async () => { events.push("directory-close"); },
          };
        }
        events.push("file-open");
        return {
          writeFile: async () => { events.push("file-write"); },
          sync: async () => { events.push("file-sync"); },
          stat: async () => fileStat,
          close: async () => { events.push("file-close"); },
        };
      },
      lstat: async (target: string) =>
        target === recoveryPath ? fileStat : directoryStat,
    };
    await reserveCanonicalRecoveryManifest(
      recoveryPath,
      built.manifest,
      {
        expectedPath: recoveryPath,
        expectedUid: process.getuid(),
        io,
      },
    );
    expect(events.indexOf("file-write")).toBeLessThan(
      events.indexOf("file-sync"),
    );
    expect(events.indexOf("file-sync")).toBeLessThan(
      events.indexOf("directory-sync"),
    );
    expect(events.filter((event) => event === "file-sync")).toHaveLength(1);
    expect(events.filter((event) => event === "directory-sync")).toHaveLength(1);

    let directoryReads = 0;
    const replacementEvents: string[] = [];
    const replacementIo = {
      open: async (_target: string, flags: string) => {
        if (flags === "r") {
          return {
            stat: async () => directoryStat,
            sync: async () => { replacementEvents.push("directory-sync"); },
            close: async () => undefined,
          };
        }
        return {
          writeFile: async () => undefined,
          sync: async () => { replacementEvents.push("file-sync"); },
          stat: async () => fileStat,
          close: async () => undefined,
        };
      },
      lstat: async (target: string) => {
        if (target === recoveryPath) return fileStat;
        directoryReads += 1;
        return directoryReads === 1
          ? directoryStat
          : { ...directoryStat, ino: 12 };
      },
    };
    await expect(reserveCanonicalRecoveryManifest(
      recoveryPath,
      built.manifest,
      {
        expectedPath: recoveryPath,
        expectedUid: process.getuid(),
        io: replacementIo,
      },
    )).rejects.toThrow(/single-link file/);
    expect(replacementEvents).toContain("file-sync");
    expect(replacementEvents).not.toContain("directory-sync");
  });

  it("requires retained ambiguous-failure recovery, one root, one CAS, and all reset/replay refusals", async () => {
    const receipt = await positiveReceipt();
    const evidence = completionEvidence(receipt);
    const admitted = validateCanonicalRolloutCompletionEvidence({
      receipt,
      evidence,
    });
    expect(admitted.completion_evidence_admitted).toBe(true);
    expect(admitted.deployment_admitted).toBe(false);

    for (const mutation of [
      (value: any) => { value.genesis.row_count = 0; },
      (value: any) => { value.root.root_user_id = ""; },
      (value: any) => { value.cas.stale_changes = 1; },
      (value: any) => { value.recovery_manifest.raw_nonce_present = false; },
      (value: any) => { value.negative_controls.replayed_receipt_refused = false; },
    ]) {
      const broken = structuredClone(evidence);
      mutation(broken);
      expect(() => validateCanonicalRolloutCompletionEvidence({
        receipt,
        evidence: broken,
      })).toThrow();
    }
  });

  it("keeps the CLI on exact commit/tree/evidence inputs and production package reachability", async () => {
    expect(parseCanonicalRolloutAdmissionArgs([
      "--expected-commit", COMMIT,
      "--expected-tree", REPOSITORY_TREE,
      "--evidence", "/tmp/evidence.json",
    ])).toEqual({
      expectedCommit: COMMIT,
      expectedTree: REPOSITORY_TREE,
      evidencePath: "/tmp/evidence.json",
    });
    expect(() => parseCanonicalRolloutAdmissionArgs([
      "--expected-commit", "HEAD",
      "--expected-tree", REPOSITORY_TREE,
      "--evidence", "/tmp/evidence.json",
    ])).toThrow(/usage/);

    let output = "";
    const receipt = await runCanonicalRolloutAdmissionCli(
      [
        "--expected-commit", COMMIT,
        "--expected-tree", REPOSITORY_TREE,
        "--evidence", "/tmp/evidence.json",
      ],
      {
        loadInputs: async () => ({
          anchor: {
            commit: COMMIT,
            repository_tree: REPOSITORY_TREE,
            keyserver_tree: KEYSERVER_TREE,
          },
          fileValues: await sourceValues(),
          evidence: await positiveEvidence(),
        }),
        now: () => NOW,
        randomUuid: () => NONCE,
        trustedProducers: TRUSTED_PRODUCERS,
        write: (text: string) => { output += text; },
      },
    );
    expect(JSON.parse(output).payload_sha256).toBe(receipt.payload_sha256);
    const packageJson = await readFile(
      path.join(ROOT, "keyserver-cf/package.json"),
      "utf8",
    );
    expect(packageJson).toContain("canonical-rollout:admit-provisioning");
    expect(packageJson).toContain("sender-filter:provision-genesis");

    const gitRun = (args: string[]) => {
      if (args.join(" ") === "rev-parse HEAD") return `${COMMIT}\n`;
      if (args.join(" ") === "rev-parse HEAD^{tree}") {
        return `${REPOSITORY_TREE}\n`;
      }
      if (args.join(" ") === "rev-parse HEAD:keyserver-cf") {
        return `${KEYSERVER_TREE}\n`;
      }
      throw new Error(`unexpected git command: ${args.join(" ")}`);
    };
    expect(assertCurrentCanonicalRolloutSource({
      expectedCommit: COMMIT,
      expectedRepositoryTree: REPOSITORY_TREE,
      expectedKeyserverTree: KEYSERVER_TREE,
    }, gitRun)).toEqual({
      commit: COMMIT,
      repository_tree: REPOSITORY_TREE,
      keyserver_tree: KEYSERVER_TREE,
    });
    expect(() => assertCurrentCanonicalRolloutSource({
      expectedCommit: "f".repeat(40),
      expectedRepositoryTree: REPOSITORY_TREE,
      expectedKeyserverTree: KEYSERVER_TREE,
    }, gitRun)).toThrow(/does not match/);
  });
});
