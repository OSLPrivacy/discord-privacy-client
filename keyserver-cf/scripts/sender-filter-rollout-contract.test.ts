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
  verifyDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-contract.mjs";
import {
  consumeDeploymentEvidenceOnce,
} from "./deployment-evidence-receipt-io.mjs";
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

async function authenticatedDeployment(
  artifact: "A" | "B",
  closure: ReturnType<typeof validateRolloutSourceClosure>,
) {
  const migrationDigest =
    closure.file_sha256[
      "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
    ];
  const expectation = structuredClone(
    deploymentEvidenceExpectation(artifact),
  );
  expectation.expectedMigrations[1].sha256 = migrationDigest;
  const payload = deploymentEvidencePayload(artifact);
  if (artifact === "B") payload.migrations[1].sha256 = migrationDigest;
  const producerReceipt = signDeploymentEvidencePayload(payload);
  const verifier = createDeploymentEvidenceVerifierStoreFixture(artifact);
  const verified = verifyDeploymentEvidenceReceipt(
    producerReceipt,
    expectation,
    {
      trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
      nowMs: DEPLOYMENT_FIXTURE_NOW,
    },
  );
  await consumeDeploymentEvidenceOnce(verified, {
    verifierStore: verifier.store,
  });
  return { expectation, producerReceipt, verifier };
}

describe("shipping sender-filter Worker/client rollout closure", () => {
  it("covers all 12 version-skew cells without cross-sender leakage", () => {
    expect(VERSION_SKEW_MATRIX).toHaveLength(12);
    expect(
      new Set(
        VERSION_SKEW_MATRIX.map(
          (entry) => `${entry.schema}/${entry.worker}/${entry.client}`,
        ),
      ).size,
    ).toBe(12);
    for (const entry of VERSION_SKEW_MATRIX) {
      const result = evaluateVersionSkewScenario({
        ...entry,
        senderId: SENDER,
        rows: continuityRows(),
      });
      expect(result.cross_sender_leakage).toBe(false);
      if (entry.worker === "legacy") {
        expect(result).toMatchObject({
          accepted: true,
          request_mode: "legacy",
          server_status: 200,
        });
      } else if (
        entry.worker === "artifact-a" ||
        entry.schema === "pre-0031"
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
        schema: "0031",
        client: "sender-filter",
        senderId: SENDER,
        rows: blockedRows(),
      }),
    ).toMatchObject({
      accepted: true,
      request_mode: "filtered",
      active_sender_reachable: true,
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
    blockCommentedMigration[migrationPath] =
      `/*\n${blockCommentedMigration[migrationPath]}\n*/`;
    expect(() =>
      validateRolloutSourceClosure(blockCommentedMigration),
    ).toThrow(/migration 0031 semantic contract/);

    const ignoredProbe = structuredClone(files);
    ignoredProbe["crates/keystore/src/client.rs"] = ignoredProbe[
      "crates/keystore/src/client.rs"
    ].replace(
      "match (self.probe_control_inbox_sender_filter_capability()?, floor) {",
      "let _ignored = self.probe_control_inbox_sender_filter_capability()?;\n        match (ControlInboxSenderFilterCapability::Version1, floor) {",
    );
    expect(() => validateRolloutSourceClosure(ignoredProbe)).toThrow(
      /ignores or bypasses capability probe dataflow/,
    );

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

    const deletionReset = structuredClone(files);
    deletionReset["crates/keystore/src/sender_filter_rollout.rs"] =
      deletionReset["crates/keystore/src/sender_filter_rollout.rs"].replace(
        '_ => Err(Error::Transport(\n            "sender-filter capability floor or identity anchor is absent".into(),\n        )),',
        "_ => Ok(SenderFilterCapabilityFloor::NeverObserved),",
      );
    expect(() => validateRolloutSourceClosure(deletionReset)).toThrow(
      /one-sided identity-anchor absence/,
    );

    const missingParentFsync = structuredClone(files);
    missingParentFsync["crates/keystore/src/sender_filter_rollout.rs"] =
      missingParentFsync[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replaceAll("sync_parent(path)?;", "let _ = path;");
    expect(() => validateRolloutSourceClosure(missingParentFsync)).toThrow(
      /sync_parent|parent durability/,
    );

    const loweringAlias = structuredClone(files);
    loweringAlias["crates/keystore/src/sender_filter_rollout.rs"] =
      loweringAlias["crates/keystore/src/sender_filter_rollout.rs"].replace(
        "#[cfg(test)]",
        "use std::fs::remove_file as erase_floor;\nfn reset_floor(path: &Path) { let erase_again = erase_floor; let _ = erase_again(path); }\n\n#[cfg(test)]",
      );
    expect(() => validateRolloutSourceClosure(loweringAlias)).toThrow(
      /aliased lowering path/,
    );

    const safeCommentAndAlias = structuredClone(files);
    safeCommentAndAlias[migrationPath] +=
      "\n/* ALTER TABLE ignored_decoy ADD COLUMN ignored; */\n";
    safeCommentAndAlias["crates/keystore/src/sender_filter_rollout.rs"] =
      safeCommentAndAlias[
        "crates/keystore/src/sender_filter_rollout.rs"
      ].replace(
        "#[cfg(test)]",
        "use std::fs::metadata as inspect_floor;\nfn harmless_inspection(path: &Path) { let _ = inspect_floor(path); }\n\n#[cfg(test)]",
      );
    expect(() => validateRolloutSourceClosure(safeCommentAndAlias)).not
      .toThrow();

    const nonAtomic = structuredClone(files);
    nonAtomic["crates/keystore/src/sender_filter_rollout.rs"] = nonAtomic[
      "crates/keystore/src/sender_filter_rollout.rs"
    ].replace("fs::rename(&temporary, path)?;", "fs::write(path, bytes)?;");
    expect(() => validateRolloutSourceClosure(nonAtomic)).toThrow(
      /atomic replacement/,
    );
  });

  it("derives nonempty A/B phases only from signed, consumed producer lineage", async () => {
    const files = await sourceFiles();
    const closure = validateRolloutSourceClosure(files);
    for (const artifact of ["A", "B"] as const) {
      const authenticated = await authenticatedDeployment(artifact, closure);
      const receipt = await deriveAuthenticatedSenderFilterPhaseReceipt({
        producerReceipt: authenticated.producerReceipt,
        deploymentExpectation: authenticated.expectation,
        trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
        verifierStore: authenticated.verifier.store,
        sourceFiles: files,
        nowMs: DEPLOYMENT_FIXTURE_NOW,
      });
      expect(receipt).toMatchObject({
        artifact,
        phase:
          artifact === "B"
            ? "artifact-b-0031"
            : "artifact-a-pre-0031",
        database_environment: "production",
      });
      expect(receipt.producer_receipt_sha256).toMatch(
        /^(?!0{64}$)[0-9a-f]{64}$/,
      );
    }
  });

  it("refuses caller-authored, unsigned, unconsumed, stale, and superseded phases", async () => {
    const files = await sourceFiles();
    const closure = validateRolloutSourceClosure(files);
    const authenticated = await authenticatedDeployment("B", closure);
    const base = {
      producerReceipt: authenticated.producerReceipt,
      deploymentExpectation: authenticated.expectation,
      trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
      verifierStore: authenticated.verifier.store,
      sourceFiles: files,
      nowMs: DEPLOYMENT_FIXTURE_NOW,
    };

    await expect(
      deriveAuthenticatedSenderFilterPhaseReceipt({
        ...base,
        producerReceipt: {
          format: "caller-authored",
          payload: { artifact: "B", phase: "artifact-b-0031" },
          payload_sha256: "f".repeat(64),
          producer_key_id: "caller",
          signature_b64: Buffer.alloc(64).toString("base64"),
        },
      }),
    ).rejects.toThrow(/envelope|trusted/);

    const unsigned = structuredClone(authenticated.producerReceipt);
    unsigned.payload.worker.sender_filter_route.response.items[0].sender_id =
      "forged";
    await expect(
      deriveAuthenticatedSenderFilterPhaseReceipt({
        ...base,
        producerReceipt: unsigned,
      }),
    ).rejects.toThrow(/payload digest|signature/);

    const unconsumed =
      createDeploymentEvidenceVerifierStoreFixture("B");
    await expect(
      deriveAuthenticatedSenderFilterPhaseReceipt({
        ...base,
        verifierStore: unconsumed.store,
      }),
    ).rejects.toThrow(/consumed head/);

    await expect(
      deriveAuthenticatedSenderFilterPhaseReceipt({
        ...base,
        nowMs: DEPLOYMENT_FIXTURE_NOW + 10 * 60_000,
      }),
    ).rejects.toThrow(/stale or future-dated/);

    const current = authenticated.verifier.current()!;
    await authenticated.verifier.store.compareAndSwap({
      expected_monotonic_version: current.monotonic_version,
      next_monotonic_version: current.monotonic_version + 1,
      next_state: {
        ...current.state,
        state_epoch: current.monotonic_version + 1,
        sequence: current.state.sequence + 1,
        receipt_sha256: "e".repeat(64),
      },
      producer_key_id: current.state.producer_key_id,
    });
    await expect(
      deriveAuthenticatedSenderFilterPhaseReceipt(base),
    ).rejects.toThrow(/consumed head/);
  });

  it("admits only authenticated stable B planning and never direct deployment", async () => {
    const files = await sourceFiles();
    const closure = validateRolloutSourceClosure(files);
    const artifactA = await authenticatedDeployment("A", closure);
    const aPlan = await admitSenderFilterRolloutPlan({
      producerReceipt: artifactA.producerReceipt,
      deploymentExpectation: artifactA.expectation,
      trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
      verifierStore: artifactA.verifier.store,
      sourceFiles: files,
      nowMs: DEPLOYMENT_FIXTURE_NOW,
    });
    expect(aPlan).toMatchObject({
      plan_admitted: false,
      next_selection: "none",
      direct_deploy_permitted: false,
      execution_authorized: false,
    });
    expect(aPlan.reasons).toContain(
      "artifact-a-traffic-quiescence-is-not-authenticated",
    );

    const artifactB = await authenticatedDeployment("B", closure);
    const bPlan = await admitSenderFilterRolloutPlan({
      producerReceipt: artifactB.producerReceipt,
      deploymentExpectation: artifactB.expectation,
      trustedProducers: TEST_TRUSTED_DEPLOYMENT_PRODUCERS,
      verifierStore: artifactB.verifier.store,
      sourceFiles: files,
      nowMs: DEPLOYMENT_FIXTURE_NOW,
    });
    expect(bPlan).toMatchObject({
      plan_admitted: true,
      next_selection: "stable-compatible",
      direct_deploy_permitted: false,
      execution_authorized: false,
    });
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
