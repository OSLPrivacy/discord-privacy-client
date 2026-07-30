import {
  createHash,
  createPublicKey,
  generateKeyPairSync,
  sign,
} from "node:crypto";
import {
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  canonicalJson,
  CROSS_LANGUAGE_CASE_IDS,
  DOWNGRADE_CASE_IDS,
  parseScheme1ClientEvidencePayload,
  RESTART_CASE_IDS,
  SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT,
  SCHEME1_CROSS_LANGUAGE_RECEIPT_FORMAT,
  SCHEME1_DOWNGRADE_RECEIPT_FORMAT,
  SCHEME1_RESTART_RECEIPT_FORMAT,
} from "./scheme1-client-evidence-dto.mjs";
import { repetitiveClientEvidenceMutations } from
  "./scheme1-client-evidence-test-fixture.mjs";
import {
  createScheme1ClientDeploymentPreflight,
  SCHEME1_FROZEN_RUST_CLIENT_PRODUCER_KEY_ID,
  SCHEME1_CLIENT_EVIDENCE_DOMAIN,
  SCHEME1_CLIENT_EVIDENCE_ENVELOPE_FORMAT,
  SCHEME1_CLIENT_MINIMUM_TEST_COUNT,
  SCHEME1_CLIENT_PREFLIGHT_FORMAT,
  SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS,
  SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS,
  SCHEME1_FROZEN_SERVER_CONTRACT,
  SCHEME1_RUST_CLIENT_CONTRACT_BINDING,
  TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS,
  validateFrozenScheme1Fixture,
  validateScheme1ClientEvidenceEnvelope,
  validateScheme1ClientPreflightReceipt,
} from "./scheme1-client-admission-contract.mjs";
import {
  loadScheme1ClientPreflightInputs,
  parseScheme1ClientPreflightArgs,
  runScheme1ClientPreflightCli,
} from "./create-scheme1-client-preflight.mjs";
import { productionActionRefusal } from
  "./refuse-unadmitted-production-action.mjs";

const ROOT = path.resolve(import.meta.dirname, "../..");
const NOW = Date.parse("2026-07-27T22:00:00.000Z");
const CLIENT_COMMIT = "a".repeat(40);
const CLIENT_TREE = "b".repeat(40);
const DEPLOYMENT_COMMIT = "c".repeat(40);
const DEPLOYMENT_TREE = "d".repeat(40);
const DEPLOYMENT_KEYSERVER_TREE = "e".repeat(40);
const CHALLENGE = "11111111-2222-4333-8444-555555555555";
const PRODUCER_KEY_ID = "rust-client-release-test";
const producerPair = generateKeyPairSync("ed25519");
const TRUSTED_PRODUCERS = {
  [PRODUCER_KEY_ID]: {
    identity: "test://rust-client-release-test",
    minimum_sequence: 7,
    public_key_spki_b64: producerPair.publicKey
      .export({ format: "der", type: "spki" })
      .toString("base64"),
    key_epoch: 3,
  },
};

function sha256(value: Buffer | string): string {
  return createHash("sha256").update(value).digest("hex");
}

function witness(label: string): string {
  return sha256(`scheme1-client-test:${label}`);
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value));
}

function withReceiptDigest<T extends Record<string, unknown>>(
  body: T,
): T & { receipt_sha256: string } {
  return {
    ...body,
    receipt_sha256: sha256(Buffer.from(canonicalJson(body))),
  };
}

function refreshReceiptDigests(payload: any): any {
  for (const field of [
    "cross_language_receipt",
    "restart_receipt",
    "downgrade_receipt",
  ]) {
    const receipt = payload[field];
    const { receipt_sha256: _old, ...body } = receipt;
    receipt.receipt_sha256 = sha256(Buffer.from(canonicalJson(body)));
  }
  return payload;
}

function positivePayload() {
  const cross = withReceiptDigest({
    format: SCHEME1_CROSS_LANGUAGE_RECEIPT_FORMAT,
    case_count: CROSS_LANGUAGE_CASE_IDS.length,
    cases: CROSS_LANGUAGE_CASE_IDS.map((id) => ({
      id,
      witness_sha256: witness(`cross:${id}`),
    })),
  });
  const restart = withReceiptDigest({
    format: SCHEME1_RESTART_RECEIPT_FORMAT,
    pre_process_sha256: witness("restart:process-before"),
    post_process_sha256: witness("restart:process-after"),
    state_sha256: witness("restart:persisted-state"),
    identity_scheme: 1,
    lifecycle_generation: 9,
    generation_batch: Buffer.alloc(32, 7).toString("base64"),
    cases: RESTART_CASE_IDS.map((id) => ({
      id,
      witness_sha256: witness(`restart:${id}`),
    })),
  });
  const downgrade = withReceiptDigest({
    format: SCHEME1_DOWNGRADE_RECEIPT_FORMAT,
    cases: DOWNGRADE_CASE_IDS.map((id) => ({
      id,
      disposition: "refused",
      witness: witness(`downgrade:${id}`),
    })),
  });
  return {
    format: SCHEME1_CLIENT_EVIDENCE_PAYLOAD_FORMAT,
    client: {
      commit: CLIENT_COMMIT,
      repository_tree: CLIENT_TREE,
    },
    contract: { ...SCHEME1_RUST_CLIENT_CONTRACT_BINDING },
    run: {
      runner_name: "cargo-nextest-scheme1-contract",
      runner_binary_sha256: witness("runner-binary"),
      command_argv: [
        "cargo",
        "test",
        "-p",
        "keystore",
        "scheme1_contract",
      ],
      started_at: "2026-07-27T21:58:00.000Z",
      finished_at: "2026-07-27T21:59:00.000Z",
      exit_code: 0,
      test_count: SCHEME1_CLIENT_MINIMUM_TEST_COUNT,
    },
    cross_language_receipt: cross,
    restart_receipt: restart,
    downgrade_receipt: downgrade,
  };
}

function signedEnvelope(
  payload = positivePayload(),
  overrides: Record<string, unknown> = {},
) {
  const normalized = parseScheme1ClientEvidencePayload(payload);
  const signedFields = {
    format: SCHEME1_CLIENT_EVIDENCE_ENVELOPE_FORMAT,
    producer_key_id: PRODUCER_KEY_ID,
    producer_key_epoch: 3,
    producer_sequence: 7,
    issued_at: "2026-07-27T21:59:30.000Z",
    expires_at: "2026-07-27T22:10:00.000Z",
    challenge_nonce: CHALLENGE,
    payload: normalized,
    payload_sha256: sha256(Buffer.from(canonicalJson(normalized))),
    ...overrides,
  };
  return {
    ...signedFields,
    signature_b64: sign(
      null,
      Buffer.concat([
        Buffer.from(SCHEME1_CLIENT_EVIDENCE_DOMAIN),
        Buffer.from(canonicalJson(signedFields)),
      ]),
      producerPair.privateKey,
    ).toString("base64"),
  };
}

async function sourceValues() {
  return Object.fromEntries(
    await Promise.all(
      SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS.map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(ROOT, sourcePath)),
      ]),
    ),
  );
}

async function frozenFixtureBytes() {
  return readFile(
    path.join(ROOT, SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path),
  );
}

async function frozenSourceValues() {
  return Object.fromEntries(
    await Promise.all(
      SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS.map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(ROOT, sourcePath)),
      ]),
    ),
  );
}

async function positiveReceipt(
  action: "migrate-0033-0034" | "activate-scheme1-worker",
) {
  return createScheme1ClientDeploymentPreflight({
    action,
    deploymentAnchor: {
      commit: DEPLOYMENT_COMMIT,
      repository_tree: DEPLOYMENT_TREE,
      keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
    },
    clientEvidence: signedEnvelope(),
    expectedClientCommit: CLIENT_COMMIT,
    expectedClientTree: CLIENT_TREE,
    expectedChallenge: CHALLENGE,
    fileValues: await sourceValues(),
    frozenFixtureBytes: await frozenFixtureBytes(),
    frozenContractFileValues: await frozenSourceValues(),
    nowMs: NOW,
    trustedProducers: TRUSTED_PRODUCERS,
  });
}

function validationOptions(
  action: "migrate-0033-0034" | "activate-scheme1-worker",
  files: Awaited<ReturnType<typeof sourceValues>>,
  frozenFiles: Awaited<ReturnType<typeof frozenSourceValues>>,
) {
  return {
    action,
    expectedDeploymentCommit: DEPLOYMENT_COMMIT,
    expectedDeploymentTree: DEPLOYMENT_TREE,
    expectedKeyserverTree: DEPLOYMENT_KEYSERVER_TREE,
    expectedClientCommit: CLIENT_COMMIT,
    expectedClientTree: CLIENT_TREE,
    expectedChallenge: CHALLENGE,
    fileValues: files,
    frozenContractFileValues: frozenFiles,
    nowMs: NOW,
    trustedProducers: TRUSTED_PRODUCERS,
  };
}

describe("scheme-1 Rust-client deployment admission", () => {
  it("freezes the exact server descriptor and fixture while keeping the Rust private blob version nonnormative", async () => {
    const result = validateFrozenScheme1Fixture(
      await frozenFixtureBytes(),
    );
    expect(result).toEqual({
      bytes: expect.any(Number),
      sha256:
        "8cebfb7bd94a3614178d6cce1af8ed8d36d776978ac57fdc5f407b6e0ce97a6f",
      descriptor_sha256:
        "8041c9c14f841935c6b42e74829e39747e6b8915bf4c929c80ff1d3e189dffaa",
    });
    expect(SCHEME1_FROZEN_SERVER_CONTRACT.commit).toBe(
      "6cb2b5a275b3676869d2a69fc10a8b6544dbc9e1",
    );
  });

  it.each([
    "migrate-0033-0034",
    "activate-scheme1-worker",
  ] as const)(
    "admits exact signed client evidence for %s but never authorizes execution",
    async (action) => {
      const files = await sourceValues();
      const frozenFiles = await frozenSourceValues();
      const first = await positiveReceipt(action);
      const replay = await positiveReceipt(action);
      expect(first).toEqual(replay);
      expect(first.format).toBe(SCHEME1_CLIENT_PREFLIGHT_FORMAT);
      expect(first.client_contract_admitted).toBe(true);
      expect(first.execution_authorized).toBe(false);
      expect(first.migration_execution_authorized).toBe(false);
      expect(first.worker_activation_authorized).toBe(false);
      expect(first.source_files).toHaveLength(
        SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS.length,
      );
      expect(first.source_files.every((entry) => entry.bytes > 0)).toBe(true);
      expect(() =>
        validateScheme1ClientPreflightReceipt(
          first,
          validationOptions(action, files, frozenFiles),
        )).not.toThrow();
    },
  );

  it("enrolls the frozen shipping Rust client producer in the committed registry", () => {
    const entries = Object.entries(TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS);
    expect(entries).toHaveLength(1);
    const [keyId, producer] = entries[0];
    expect(keyId).toBe(SCHEME1_FROZEN_RUST_CLIENT_PRODUCER_KEY_ID);
    expect(keyId).not.toMatch(/test|fixture|localhost|example/i);
    expect(producer).toEqual({
      identity: "osl://scheme1-rust-client/frozen-shipping/2026-07-27",
      minimum_sequence: 1,
      public_key_spki_b64:
        "MCowBQYDK2VwAyEArqLJqLipE68NG6DRgdUTUlPduDx4S/b0rVkHER2tH6s=",
      key_epoch: 1,
    });
    expect(producer.identity).not.toMatch(/test|fixture|localhost|example/i);
    const publicKey = createPublicKey({
      key: Buffer.from(producer.public_key_spki_b64, "base64"),
      format: "der",
      type: "spki",
    });
    expect(publicKey.asymmetricKeyType).toBe("ed25519");
    expect(
      publicKey.export({ format: "der", type: "spki" }).toString("base64"),
    ).toBe(producer.public_key_spki_b64);
  });

  it("enroll the frozen shipping Rust client as a trusted evidence producer", () => {
    const producer =
      TRUSTED_SCHEME1_CLIENT_EVIDENCE_PRODUCERS[
        SCHEME1_FROZEN_RUST_CLIENT_PRODUCER_KEY_ID
      ];
    expect(producer).toBeDefined();
    expect(Object.isFrozen(producer)).toBe(true);
    expect(producer.minimum_sequence).toBeGreaterThan(0);
    expect(producer.key_epoch).toBeGreaterThan(0);
    expect(producer.identity).toBe(
      "osl://scheme1-rust-client/frozen-shipping/2026-07-27",
    );
    const publicKey = createPublicKey({
      key: Buffer.from(producer.public_key_spki_b64, "base64"),
      format: "der",
      type: "spki",
    });
    expect(publicKey.asymmetricKeyType).toBe("ed25519");
  });

  it("refuses untrusted Rust-client evidence by default", async () => {
    const files = await sourceValues();
    const fixture = await frozenFixtureBytes();
    const frozenFiles = await frozenSourceValues();
    expect(() =>
      createScheme1ClientDeploymentPreflight({
          action: "migrate-0033-0034",
          deploymentAnchor: {
            commit: DEPLOYMENT_COMMIT,
            repository_tree: DEPLOYMENT_TREE,
            keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
          },
          clientEvidence: signedEnvelope(),
          expectedClientCommit: CLIENT_COMMIT,
          expectedClientTree: CLIENT_TREE,
          expectedChallenge: CHALLENGE,
          fileValues: files,
          frozenFixtureBytes: fixture,
          frozenContractFileValues: frozenFiles,
          nowMs: NOW,
        })).toThrow(/trusted scheme-1 client evidence producer/);
  });

  it("rejects every DTO/receipt mutation after refreshing subordinate hashes", () => {
    for (const mutation of repetitiveClientEvidenceMutations(
      positivePayload(),
    )) {
      const payload = refreshReceiptDigests(mutation.payload);
      expect(
        () =>
          validateScheme1ClientEvidenceEnvelope(signedEnvelope(payload), {
            expectedClientCommit: CLIENT_COMMIT,
            expectedClientTree: CLIENT_TREE,
            expectedChallenge: CHALLENGE,
            nowMs: NOW,
            trustedProducers: TRUSTED_PRODUCERS,
          }),
        mutation.name,
      ).toThrow();
    }
  });

  it("rejects receipt-digest substitution and cross-class witness reuse", () => {
    const badDigest = positivePayload();
    badDigest.restart_receipt.receipt_sha256 = "f".repeat(64);
    expect(() =>
      validateScheme1ClientEvidenceEnvelope(signedEnvelope(badDigest), {
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        expectedChallenge: CHALLENGE,
        nowMs: NOW,
        trustedProducers: TRUSTED_PRODUCERS,
      })).toThrow(/restart receipt digest mismatch/);

    const reused = positivePayload();
    reused.restart_receipt.cases[0].witness_sha256 =
      reused.cross_language_receipt.cases[0].witness_sha256;
    refreshReceiptDigests(reused);
    expect(() =>
      validateScheme1ClientEvidenceEnvelope(signedEnvelope(reused), {
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        expectedChallenge: CHALLENGE,
        nowMs: NOW,
        trustedProducers: TRUSTED_PRODUCERS,
      })).toThrow(/reuses a witness/);
  });

  it("rejects stale, replay-floor, wrong-epoch, wrong-challenge, and noncanonical signed evidence", () => {
    const cases = [
      signedEnvelope(positivePayload(), {
        issued_at: "2026-07-27T21:00:00.000Z",
        expires_at: "2026-07-27T21:10:00.000Z",
      }),
      signedEnvelope(positivePayload(), { producer_sequence: 6 }),
      signedEnvelope(positivePayload(), { producer_key_epoch: 2 }),
      signedEnvelope(positivePayload(), {
        challenge_nonce: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      }),
    ];
    const noncanonical = signedEnvelope();
    noncanonical.signature_b64 = `${noncanonical.signature_b64}\n`;
    cases.push(noncanonical);

    const tampered = signedEnvelope();
    tampered.producer_sequence = 8;
    cases.push(tampered);

    for (const evidence of cases) {
      expect(() =>
        validateScheme1ClientEvidenceEnvelope(evidence, {
          expectedClientCommit: CLIENT_COMMIT,
          expectedClientTree: CLIENT_TREE,
          expectedChallenge: CHALLENGE,
          nowMs: NOW,
          trustedProducers: TRUSTED_PRODUCERS,
        })).toThrow();
    }
  });

  it("rejects a non-Ed25519 trusted producer key even when the envelope is otherwise exact", () => {
    const rsa = generateKeyPairSync("rsa", { modulusLength: 2048 });
    const wrongKeyRegistry = {
      [PRODUCER_KEY_ID]: {
        ...TRUSTED_PRODUCERS[PRODUCER_KEY_ID],
        public_key_spki_b64: rsa.publicKey
          .export({ format: "der", type: "spki" })
          .toString("base64"),
      },
    };
    expect(() =>
      validateScheme1ClientEvidenceEnvelope(signedEnvelope(), {
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        expectedChallenge: CHALLENGE,
        nowMs: NOW,
        trustedProducers: wrongKeyRegistry,
      })).toThrow(/canonical Ed25519 SPKI/);
  });

  it("recomputes source closure so co-mutated receipt expectations cannot pass", async () => {
    const files = await sourceValues();
    const frozenFiles = await frozenSourceValues();
    const receipt: any = clone(
      await positiveReceipt("activate-scheme1-worker"),
    );
    receipt.source_files[0].sha256 = "1".repeat(64);
    const { payload_sha256: _old, ...body } = receipt;
    receipt.payload_sha256 = sha256(Buffer.from(canonicalJson(body)));
    expect(() =>
      validateScheme1ClientPreflightReceipt(
        receipt,
        validationOptions(
          "activate-scheme1-worker",
          files,
          frozenFiles,
        ),
      )).toThrow(/does not match exact bytes/);
  });

  it("refuses an empty or substituted source and frozen-fixture closure", async () => {
    const files = await sourceValues();
    const frozenFixture = await frozenFixtureBytes();
    const frozenFiles = await frozenSourceValues();
    files[SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS[0]] = Buffer.alloc(0);
    expect(() =>
      createScheme1ClientDeploymentPreflight({
        action: "migrate-0033-0034",
        deploymentAnchor: {
          commit: DEPLOYMENT_COMMIT,
          repository_tree: DEPLOYMENT_TREE,
          keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
        },
        clientEvidence: signedEnvelope(),
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        expectedChallenge: CHALLENGE,
        fileValues: files,
        frozenFixtureBytes: frozenFixture,
        frozenContractFileValues: frozenFiles,
        nowMs: NOW,
        trustedProducers: TRUSTED_PRODUCERS,
      })).toThrow(/source is empty/);

    const driftedFiles = await sourceValues();
    const driftPath = SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS[0];
    driftedFiles[driftPath] = Buffer.concat([
      driftedFiles[driftPath],
      Buffer.from("\n-- drift\n"),
    ]);
    expect(() =>
      createScheme1ClientDeploymentPreflight({
        action: "migrate-0033-0034",
        deploymentAnchor: {
          commit: DEPLOYMENT_COMMIT,
          repository_tree: DEPLOYMENT_TREE,
          keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
        },
        clientEvidence: signedEnvelope(),
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        expectedChallenge: CHALLENGE,
        fileValues: driftedFiles,
        frozenFixtureBytes: frozenFixture,
        frozenContractFileValues: frozenFiles,
        nowMs: NOW,
        trustedProducers: TRUSTED_PRODUCERS,
      })).toThrow(/frozen source contract drift/);

    const fixture = Buffer.from(frozenFixture);
    fixture[0] ^= 1;
    expect(() => validateFrozenScheme1Fixture(fixture)).toThrow(
      /fixture digest mismatch/,
    );
  });

  it("keeps CLI arguments exact and emits only a non-authorizing receipt", async () => {
    const args = [
      "--action",
      "migrate-0033-0034",
      "--expected-server-commit",
      DEPLOYMENT_COMMIT,
      "--expected-server-tree",
      DEPLOYMENT_TREE,
      "--expected-client-commit",
      CLIENT_COMMIT,
      "--expected-client-tree",
      CLIENT_TREE,
      "--challenge",
      CHALLENGE,
      "--evidence",
      "/tmp/scheme1-client-evidence.json",
    ];
    expect(parseScheme1ClientPreflightArgs(args)).toEqual({
      action: "migrate-0033-0034",
      expectedServerCommit: DEPLOYMENT_COMMIT,
      expectedServerTree: DEPLOYMENT_TREE,
      expectedClientCommit: CLIENT_COMMIT,
      expectedClientTree: CLIENT_TREE,
      challenge: CHALLENGE,
      evidencePath: "/tmp/scheme1-client-evidence.json",
    });
    expect(() =>
      parseScheme1ClientPreflightArgs([
        ...args,
        "--action",
        "activate-scheme1-worker",
      ])).toThrow(/usage/);

    let output = "";
    const receipt = await runScheme1ClientPreflightCli(args, {
      loadInputs: async () => ({
        deploymentAnchor: {
          commit: DEPLOYMENT_COMMIT,
          repository_tree: DEPLOYMENT_TREE,
          keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
        },
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        fileValues: await sourceValues(),
        frozenFixtureBytes: await frozenFixtureBytes(),
        frozenContractFileValues: await frozenSourceValues(),
        clientEvidence: signedEnvelope(),
      }),
      now: () => NOW,
      trustedProducers: TRUSTED_PRODUCERS,
      write: (text: string) => {
        output += text;
      },
    });
    expect(JSON.parse(output)).toEqual(receipt);
    expect(receipt.execution_authorized).toBe(false);
  });

  it("run scheme1 client-preflight admission to generate admitted contract v", async () => {
    const files = await sourceValues();
    const frozenFiles = await frozenSourceValues();
    const receipt = await positiveReceipt("migrate-0033-0034");

    expect(receipt.client_contract_admitted).toBe(true);
    expect(receipt.execution_authorized).toBe(false);
    expect(receipt.client.commit).toBe(CLIENT_COMMIT);
    expect(receipt.client.repository_tree).toBe(CLIENT_TREE);
    expect(receipt.challenge_nonce).toBe(CHALLENGE);
    expect(() =>
      validateScheme1ClientPreflightReceipt(
        receipt,
        validationOptions("migrate-0033-0034", files, frozenFiles),
      )).not.toThrow();
  });

  it("loads only exact server HEAD/tree, exact client tree, frozen fixture, and a regular evidence file", async () => {
    const temporary = await mkdtemp(
      path.join(tmpdir(), "osl-scheme1-client-loader-"),
    );
    try {
      const evidencePath = path.join(temporary, "evidence.json");
      await writeFile(evidencePath, "{}\n", { mode: 0o600 });
      const sources = await sourceValues();
      const frozenSources = await frozenSourceValues();
      const fakeGit = (
        _repoRoot: string,
        args: string[],
        encoding: string | null = "utf8",
      ) => {
        const expression = args.at(-1);
        let value: Buffer | string;
        if (
          args[0] === "rev-parse" &&
          args[1] === "--verify" &&
          expression === `${DEPLOYMENT_COMMIT}^{commit}`
        ) {
          value = DEPLOYMENT_COMMIT;
        } else if (
          args[0] === "rev-parse" &&
          args[1] === "--verify" &&
          expression === `${CLIENT_COMMIT}^{commit}`
        ) {
          value = CLIENT_COMMIT;
        } else if (args.join(" ") === "rev-parse HEAD") {
          value = DEPLOYMENT_COMMIT;
        } else if (expression === `${DEPLOYMENT_COMMIT}^{tree}`) {
          value = DEPLOYMENT_TREE;
        } else if (expression === `${CLIENT_COMMIT}^{tree}`) {
          value = CLIENT_TREE;
        } else if (
          expression === `${DEPLOYMENT_COMMIT}:keyserver-cf`
        ) {
          value = DEPLOYMENT_KEYSERVER_TREE;
        } else if (args[0] === "show") {
          const separator = expression!.indexOf(":");
          const sourcePath = expression!.slice(separator + 1);
          value = expression!.startsWith(
            `${SCHEME1_FROZEN_SERVER_CONTRACT.commit}:`,
          )
            ? frozenSources[sourcePath]
            : sources[sourcePath];
        } else {
          throw new Error(`unexpected git args: ${args.join(" ")}`);
        }
        if (Buffer.isBuffer(value)) {
          return encoding === null ? value : value.toString(encoding as any);
        }
        return `${value}\n`;
      };
      const options = {
        expectedServerCommit: DEPLOYMENT_COMMIT,
        expectedServerTree: DEPLOYMENT_TREE,
        expectedClientCommit: CLIENT_COMMIT,
        expectedClientTree: CLIENT_TREE,
        evidencePath,
      };
      const loaded = await loadScheme1ClientPreflightInputs(
        ROOT,
        options,
        fakeGit as any,
      );
      expect(loaded.deploymentAnchor).toEqual({
        commit: DEPLOYMENT_COMMIT,
        repository_tree: DEPLOYMENT_TREE,
        keyserver_tree: DEPLOYMENT_KEYSERVER_TREE,
      });
      expect(loaded.expectedClientTree).toBe(CLIENT_TREE);
      expect(loaded.clientEvidence).toEqual({});
      expect(Object.keys(loaded.frozenContractFileValues)).toEqual(
        SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS,
      );

      const symlinkPath = path.join(temporary, "evidence-link.json");
      await symlink(evidencePath, symlinkPath);
      await expect(
        loadScheme1ClientPreflightInputs(
          ROOT,
          { ...options, evidencePath: symlinkPath },
          fakeGit as any,
        ),
      ).rejects.toThrow(/bounded regular file/);

      const wrongHeadGit = (
        repoRoot: string,
        args: string[],
        encoding: string | null = "utf8",
      ) =>
        args.join(" ") === "rev-parse HEAD"
          ? `${"f".repeat(40)}\n`
          : fakeGit(repoRoot, args, encoding);
      await expect(
        loadScheme1ClientPreflightInputs(
          ROOT,
          options,
          wrongHeadGit as any,
        ),
      ).rejects.toThrow(/not exact current HEAD/);
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  });

  it("leaves production migration and Worker commands hard-refused after preflight", async () => {
    const packageJson = JSON.parse(
      await readFile(path.join(ROOT, "keyserver-cf/package.json"), "utf8"),
    );
    expect(packageJson.scripts["scheme1:client-preflight"]).toBe(
      "node scripts/create-scheme1-client-preflight.mjs",
    );
    expect(packageJson.scripts.deploy).toBe(
      "node scripts/refuse-unadmitted-production-action.mjs deploy",
    );
    expect(packageJson.scripts["db:migrate:prod"]).toBe(
      "node scripts/refuse-unadmitted-production-action.mjs migrate",
    );
    expect(productionActionRefusal("deploy")).toMatch(
      /Rust-client preflight receipt/,
    );
    expect(productionActionRefusal("migrate")).toMatch(
      /migrations 0033\/0034.*Rust-client preflight receipt/,
    );
  });
});
