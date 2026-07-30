import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ROLLOUT_SOURCE_PATHS,
  VERSION_SKEW_MATRIX,
  admitSenderFilterRolloutPlan,
  classifyCapability,
  consumeClientResponse,
  deriveAuthenticatedSenderFilterPhaseReceipt,
  evaluateVersionSkewScenario,
  evaluateWorkerRequest,
  validateRolloutSourceClosure,
  workerHealth,
} from "./sender-filter-rollout-contract.mjs";
import {
  DEPLOYMENT_FIXTURE_NOW,
  TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
  createDeploymentEvidenceVerifierStoreFixture,
  deploymentEvidenceExpectation,
  deploymentEvidencePayload,
  signDeploymentEvidencePayload,
} from "./deployment-evidence-test-fixture.js";

const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);
const SENDER = "sender-positive";

function continuityRows() {
  return [
    { id: "selected-1", sender_id: SENDER },
    { id: "foreign-1", sender_id: "sender-foreign" },
  ];
}

function blockedRows() {
  return [
    ...Array.from({ length: 64 }, (_, index) => ({
      id: `foreign-${index}`,
      sender_id: `sender-foreign-${Math.floor(index / 32)}`,
    })),
    { id: "selected-after-page", sender_id: SENDER },
  ];
}

async function sourceFiles() {
  return Object.fromEntries(
    await Promise.all(
      ROLLOUT_SOURCE_PATHS.map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(REPO_ROOT, sourcePath), "utf8"),
      ]),
    ),
  );
}

function deploymentFixture(
  artifact: "A" | "B",
  closure: ReturnType<typeof validateRolloutSourceClosure>,
) {
  const migration0031Digest =
    closure.file_sha256[
      "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
    ];
  const migration0032Digest =
    closure.file_sha256[
      "keyserver-cf/migrations/0032_sender_filter_capability_floor.sql"
    ];
  const expectation = structuredClone(
    deploymentEvidenceExpectation(artifact),
  );
  expectation.expectedMigrations[1].sha256 = migration0031Digest;
  expectation.expectedMigrations[2].sha256 = migration0032Digest;
  const payload = deploymentEvidencePayload(artifact);
  if (artifact === "B") {
    payload.migrations[1].sha256 = migration0031Digest;
    payload.migrations[2].sha256 = migration0032Digest;
  }
  const producerReceipt = signDeploymentEvidencePayload(payload);
  const verifier = createDeploymentEvidenceVerifierStoreFixture(artifact);
  return { expectation, producerReceipt, verifier };
}

describe("shipping sender-filter Worker/client rollout closure", () => {
  it("covers all 18 version-skew cells without cross-sender leakage", () => {
    expect(VERSION_SKEW_MATRIX).toHaveLength(18);
    expect(
      new Set(
        VERSION_SKEW_MATRIX.map(
          (entry) => `${entry.schema}/${entry.worker}/${entry.client}`,
        ),
    ).size,
    ).toBe(18);
    for (const entry of VERSION_SKEW_MATRIX) {
      const result = evaluateVersionSkewScenario({
        ...entry,
        senderId: SENDER,
        rows: continuityRows(),
      });
      expect(result.cross_sender_leakage).toBe(false);
      if (entry.worker === "legacy" && entry.client === "legacy") {
        expect(result).toMatchObject({
          accepted: true,
          request_mode: "legacy",
          server_status: 200,
        });
      } else if (
        entry.worker === "artifact-a" ||
        entry.schema === "pre-0031" ||
        entry.client === "sender-filter" &&
          (entry.worker !== "artifact-b" ||
            entry.schema !== "0031+0032")
      ) {
        expect(result).toMatchObject({
          accepted: false,
          fail_closed: true,
        });
      } else {
        expect(result.accepted).toBe(true);
      }
    }
  });

  it("matches historical old-Worker malformed and appended-sender behavior", () => {
    const base = {
      worker: "legacy",
      schema: "pre-0031",
      rows: continuityRows(),
    };
    expect(
      evaluateWorkerRequest({
        ...base,
        request: {
          sender_param: "bad\nsender",
          signed_sender: "bad\nsender",
          signature_valid: true,
        },
      }),
    ).toMatchObject({ status: 401, refusal: "sender-signature-mismatch" });
    expect(
      evaluateWorkerRequest({
        ...base,
        request: {
          sender_param: "bad\nsender",
          signed_sender: null,
          signature_valid: true,
        },
      }),
    ).toMatchObject({ status: 200, refusal: null });
    expect(
      evaluateWorkerRequest({
        worker: "artifact-b",
        schema: "0031",
        rows: continuityRows(),
        request: {
          sender_param: "bad\nsender",
          signed_sender: "bad\nsender",
          signature_valid: true,
        },
      }),
    ).toMatchObject({ status: 400, refusal: "malformed-sender" });
  });

  it("models the shipping probe choice and durable downgrade refusal", () => {
    expect(classifyCapability(workerHealth("legacy", "pre-0031"), false))
      .toEqual({
        mode: "legacy",
        reason: "capability-not-yet-advertised",
      });
    expect(classifyCapability(workerHealth("artifact-b", "0031"), false))
      .toEqual({ mode: "filtered", reason: null });
    expect(classifyCapability(workerHealth("legacy", "pre-0031"), true))
      .toEqual({ mode: "refuse", reason: "capability-downgrade" });
  });

  it("does not use omniscient undisclosed server rows to classify an empty response", () => {
    expect(
      consumeClientResponse({
        mode: "filtered",
        senderId: SENDER,
        response: {
          status: 200,
          items: [],
          filtered_sender_id: SENDER,
        },
      }),
    ).toMatchObject({
      accepted: true,
      active_sender_reachable: false,
    });
    expect(
      consumeClientResponse({
        mode: "filtered",
        senderId: SENDER,
        response: {
          status: 200,
          items: [{ id: "foreign", sender_id: "sender-foreign" }],
          filtered_sender_id: SENDER,
        },
      }),
    ).toMatchObject({
      accepted: false,
      reason: "cross-sender-filter-response",
    });
  });

  it("preserves the known legacy page limit while filtered B reaches the peer", () => {
    expect(
      evaluateVersionSkewScenario({
        worker: "artifact-b",
        schema: "0031",
        client: "legacy",
        senderId: SENDER,
        rows: blockedRows(),
      }),
    ).toMatchObject({
      accepted: true,
      request_mode: "legacy",
      active_sender_reachable: false,
    });
    expect(
      evaluateVersionSkewScenario({
        worker: "artifact-b",
        schema: "0031+0032",
        client: "sender-filter",
        senderId: SENDER,
        rows: blockedRows(),
      }),
    ).toMatchObject({
      accepted: true,
      request_mode: "filtered",
      active_sender_reachable: true,
    });
    expect(
      evaluateVersionSkewScenario({
        worker: "artifact-b",
        schema: "0031",
        client: "sender-filter",
        senderId: SENDER,
        rows: blockedRows(),
      }),
    ).toMatchObject({
      accepted: false,
      fail_closed: true,
      refusal: "authoritative-floor-unavailable",
    });
  });

  it("semantically binds migration, Worker, client state, and broker call sites", async () => {
    const files = await sourceFiles();
    const closure = validateRolloutSourceClosure(files);
    expect(closure.source_closure_sha256).toMatch(
      /^(?!0{64}$)[0-9a-f]{64}$/,
    );
    expect(Object.keys(closure.file_sha256)).toEqual(ROLLOUT_SOURCE_PATHS);

    const blockCommentedMigration = structuredClone(files);
    const migrationPath =
      "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql";
    const floorMigrationPath =
      "keyserver-cf/migrations/0032_sender_filter_capability_floor.sql";
    blockCommentedMigration[migrationPath] =
      `/*\n${blockCommentedMigration[migrationPath]}\n*/`;
    expect(() =>
      validateRolloutSourceClosure(blockCommentedMigration),
    ).toThrow(/migration 0031 semantic contract/);

    const blockCommentedFloorMigration = structuredClone(files);
    blockCommentedFloorMigration[floorMigrationPath] =
      `/*\n${blockCommentedFloorMigration[floorMigrationPath]}\n*/`;
    expect(() =>
      validateRolloutSourceClosure(blockCommentedFloorMigration),
    ).toThrow(/migration 0032 authority contract/);

    const ignoredProbe = structuredClone(files);
    ignoredProbe["crates/keystore/src/client.rs"] = ignoredProbe[
      "crates/keystore/src/client.rs"
    ].replace(
      "let capability = self.probe_control_inbox_sender_filter_capability()?;",
      "let _ignored = self.probe_control_inbox_sender_filter_capability()?;\n        let capability = ControlInboxSenderFilterCapability::Version1;",
    );
    expect(() => validateRolloutSourceClosure(ignoredProbe)).toThrow(
      /live-tail measured floor|legacy\/direct bypass/,
    );

    const deadMeasuredFlow = structuredClone(files);
    deadMeasuredFlow["crates/keystore/src/client.rs"] = deadMeasuredFlow[
      "crates/keystore/src/client.rs"
    ].replace(
      "let capability = self.probe_control_inbox_sender_filter_capability()?;\n        let measured_floor = self.observe_sender_filter_capability_floor(identity)?;\n        match (capability, measured_floor) {",
      "if false {\n            let capability = self.probe_control_inbox_sender_filter_capability()?;\n            let measured_floor = self.observe_sender_filter_capability_floor(identity)?;\n            let _ = match (capability, measured_floor) {",
    ).replace(
      "            }\n        }\n    }\n\n    fn probe_control_inbox_sender_filter_capability",
      "            };\n        }\n        self.get_control_inbox_from(identity, sender_id)\n    }\n\n    fn probe_control_inbox_sender_filter_capability",
    );
    expect(() => validateRolloutSourceClosure(deadMeasuredFlow)).toThrow(
      /live-tail measured floor|legacy\/direct bypass/,
    );

    const falseMeasuredFloor = structuredClone(files);
    falseMeasuredFloor["crates/keystore/src/client.rs"] =
      falseMeasuredFloor["crates/keystore/src/client.rs"].replace(
        "let measured_floor = self.observe_sender_filter_capability_floor(identity)?;",
        "let _ignored = self.observe_sender_filter_capability_floor(identity)?;\n        let measured_floor = SenderFilterCapabilityFloor::Version1;",
      );
    expect(() =>
      validateRolloutSourceClosure(falseMeasuredFloor),
    ).toThrow(/live-tail measured floor|legacy\/direct bypass/);

    const filteredLegacyCall = structuredClone(files);
    filteredLegacyCall["crates/keystore/src/client.rs"] =
      filteredLegacyCall["crates/keystore/src/client.rs"].replace(
        'Err(Error::Transport(\n                    "control-inbox sender-filter capability downgrade refused".into(),\n                ))',
        "Ok(FilteredControlInbox { delivery: ControlInboxDeliveryDisposition { live: 0, retryable: 0, quarantined: 0, retired: 0 }, items: self.get_control_inbox(identity)?.into_iter().filter(|item| item.sender_id == sender_id).collect() })",
      );
    expect(() =>
      validateRolloutSourceClosure(filteredLegacyCall),
    ).toThrow(/live-tail measured floor|legacy\/direct bypass/);

    const brokerBypass = structuredClone(files);
    brokerBypass["apps/osl-hub/src/broker.rs"] = brokerBypass[
      "apps/osl-hub/src/broker.rs"
    ].replace(
      "client.get_control_inbox_compatible_from(identity, peer_osl_user_id)",
      "if false { let _ = client.get_control_inbox_compatible_from(identity, peer_osl_user_id); }\n    client.get_control_inbox_from(identity, peer_osl_user_id)",
    );
    expect(() => validateRolloutSourceClosure(brokerBypass)).toThrow(
      /dead branch or direct bypass/,
    );

    const callerWrittenFloor = structuredClone(files);
    callerWrittenFloor[
      "keyserver-cf/src/endpoints/sender-filter-capability-floor.ts"
    ] = callerWrittenFloor[
      "keyserver-cf/src/endpoints/sender-filter-capability-floor.ts"
    ].replace(
      "VALUES (?, 1, 1, ?)",
      "VALUES (?, Number(url.searchParams.get('floor')), 1, ?)",
    );
    expect(() => validateRolloutSourceClosure(callerWrittenFloor)).toThrow(
      /authority SQL is missing or caller-controlled/,
    );

    const resettableMigration = structuredClone(files);
    resettableMigration[floorMigrationPath] =
      resettableMigration[floorMigrationPath].replace(
        "CREATE TRIGGER sender_filter_capability_floor_no_delete",
        "CREATE TRIGGER sender_filter_capability_floor_delete_allowed",
      );
    expect(() => validateRolloutSourceClosure(resettableMigration)).toThrow(
      /migration 0032 authority contract/,
    );

    const falseNeverObserved = structuredClone(files);
    falseNeverObserved["crates/keystore/src/sender_filter_rollout.rs"] =
      falseNeverObserved[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replace(
        "pub(crate) enum SenderFilterCapabilityFloor {\n    Version1,\n}",
        "pub(crate) enum SenderFilterCapabilityFloor {\n    NeverObserved,\n    Version1,\n}",
      );
    expect(() => validateRolloutSourceClosure(falseNeverObserved)).toThrow(
      /reopen or bypass monotonic authority/,
    );

    const futureMonotonicVersion = structuredClone(files);
    futureMonotonicVersion["crates/keystore/src/sender_filter_rollout.rs"] =
      futureMonotonicVersion[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replace(
        "observation.monotonic_version != 1",
        "observation.monotonic_version == 0",
      );
    expect(() =>
      validateRolloutSourceClosure(futureMonotonicVersion),
    ).toThrow(/monotonic_version|reopen or bypass monotonic authority/);

    const typedLoweringAlias = structuredClone(files);
    typedLoweringAlias["crates/keystore/src/sender_filter_rollout.rs"] =
      typedLoweringAlias[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replace(
        "#[cfg(test)]",
        "type FloorMutation = fn(&std::path::Path) -> std::io::Result<()>;\nfn typed_floor_mutation(path: &std::path::Path) { let mutate = std::fs::remove_file as FloorMutation; let _ = mutate(path); }\n\n#[cfg(test)]",
      );
    expect(() => validateRolloutSourceClosure(typedLoweringAlias)).toThrow(
      /typed lowering path/,
    );

    for (const [label, injected] of [
      [
        "cast-through-type alias",
        "type FloorMutation = fn(&std::path::Path) -> std::io::Result<()>;\nfn mutate_floor(path: &std::path::Path) { let mutate = std::fs::remove_file as FloorMutation; let _ = mutate(path); }\n",
      ],
      [
        "const function-pointer alias",
        "const FLOOR_MUTATION: fn(&std::path::Path) -> std::io::Result<()> = std::fs::remove_file;\n",
      ],
      [
        "static function-pointer alias",
        "static FLOOR_MUTATION: fn(&std::path::Path) -> std::io::Result<()> = std::fs::remove_file;\n",
      ],
      [
        "parenthesized local alias",
        "fn mutate_floor(path: &std::path::Path) { let mutate = (std::fs::remove_file); let _ = mutate(path); }\n",
      ],
      [
        "parenthesized transitive alias",
        "fn mutate_floor(path: &std::path::Path) { let first = (std::fs::remove_file); let second = (first); let third = (second); let _ = third(path); }\n",
      ],
      [
        "renamed import and parenthesized alias",
        "use std::fs::remove_file as erase_floor;\nfn mutate_floor(path: &std::path::Path) { let mutate = (erase_floor); let _ = mutate(path); }\n",
      ],
      [
        "higher-order function pointer",
        "fn invoke_floor_mutation(mutate: fn(&std::path::Path) -> std::io::Result<()>, path: &std::path::Path) { let _ = mutate(path); }\nfn mutate_floor(path: &std::path::Path) { invoke_floor_mutation(std::fs::remove_file, path); }\n",
      ],
    ] as const) {
      const mutation = structuredClone(files);
      mutation["crates/keystore/src/sender_filter_rollout.rs"] =
        mutation[
          "crates/keystore/src/sender_filter_rollout.rs"
        ].replace("#[cfg(test)]", `${injected}\n#[cfg(test)]`);
      expect(
        () => validateRolloutSourceClosure(mutation),
        label,
      ).toThrow(/direct, aliased, or typed lowering path/);
    }

    const earlyDirectReturn = structuredClone(files);
    earlyDirectReturn["crates/keystore/src/client.rs"] =
      earlyDirectReturn["crates/keystore/src/client.rs"].replace(
        "let capability = self.probe_control_inbox_sender_filter_capability()?;",
        "return self.get_control_inbox_from(identity, sender_id);\n        let capability = self.probe_control_inbox_sender_filter_capability()?;",
      );
    expect(() => validateRolloutSourceClosure(earlyDirectReturn)).toThrow(
      /live-tail measured floor|validation prefix/,
    );

    const safeComments = structuredClone(files);
    safeComments[migrationPath] +=
      "\n/* ALTER TABLE ignored_decoy ADD COLUMN ignored; */\n";
    safeComments["crates/keystore/src/sender_filter_rollout.rs"] =
      safeComments[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replace(
        "#[cfg(test)]",
        "// std::fs::remove_file as TypedReset is forbidden in production.\n#[cfg(test)]",
      );
    expect(() => validateRolloutSourceClosure(safeComments)).not
      .toThrow();
  });

  it("keeps producer trust and verifier storage internal and unprovisioned", async () => {
    const files = await sourceFiles();
    const closure = validateRolloutSourceClosure(files);
    for (const artifact of ["A", "B"] as const) {
      const fixture = deploymentFixture(artifact, closure);
      const base = {
        producerReceipt: fixture.producerReceipt,
        deploymentExpectation: fixture.expectation,
        sourceFiles: files,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      };
      await expect(
        deriveAuthenticatedSenderFilterPhaseReceipt(base),
      ).rejects.toThrow(/not independently trusted/);
      await expect(
        deriveAuthenticatedSenderFilterPhaseReceipt({
          ...base,
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        }),
      ).rejects.toThrow(/fields are not exact/);
      await expect(
        deriveAuthenticatedSenderFilterPhaseReceipt({
          ...base,
          verifierStore: fixture.verifier.store,
        }),
      ).rejects.toThrow(/fields are not exact/);
      await expect(
        admitSenderFilterRolloutPlan({
          ...base,
          trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
          verifierStore: fixture.verifier.store,
        }),
      ).rejects.toThrow(/fields are not exact/);
    }
  });

  it("keeps package entrypoints non-deploying", async () => {
    const packageJson = JSON.parse(
      await readFile(path.join(REPO_ROOT, "keyserver-cf/package.json"), "utf8"),
    );
    expect(packageJson.scripts.deploy).toBe(
      "node scripts/refuse-unadmitted-production-action.mjs deploy",
    );
    const runbook = await readFile(
      path.join(REPO_ROOT, "keyserver-cf/SENDER_FILTER_ROLLOUT.md"),
      "utf8",
    );
    expect(runbook).toContain("durable");
    expect(runbook).toContain("phase receipt");
    expect(runbook).toContain("direct_deploy_permitted=false");
    expect(runbook).not.toMatch(/(?:npx|npm exec)\s+wrangler/);
  });
});
