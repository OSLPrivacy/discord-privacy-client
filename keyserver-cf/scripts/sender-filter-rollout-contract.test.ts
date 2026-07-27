import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ROLLOUT_CALL_SITES,
  ROLLOUT_SOURCE_PATHS,
  VERSION_SKEW_MATRIX,
  admitSenderFilterRolloutPlan,
  classifyCapability,
  consumeClientResponse,
  createSenderFilterPhaseReceipt,
  evaluateVersionSkewScenario,
  evaluateWorkerRequest,
  validateRolloutSourceClosure,
  verifySenderFilterPhaseReceipt,
  workerHealth,
} from "./sender-filter-rollout-contract.mjs";

const REPO_ROOT = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);
const NOW = Date.parse("2026-07-27T12:00:00.000Z");
const SENDER = "sender-positive";
const EXPECTED_COMMIT = "a".repeat(40);
const WORKER_COMMIT = "b".repeat(40);
const CLIENT_COMMIT = "c".repeat(40);
const WORKER_VERSION = "11111111-2222-4333-8444-555555555555";
const DEPLOYMENT_ID = "66666666-7777-4888-9999-aaaaaaaaaaaa";

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

const PHASE_FIXTURES = {
  "legacy-pre-0031": {
    capability: ["legacy", 200, null],
    legacy: ["legacy", 200, 2, null, 0],
    filtered: ["none", null, 0, null, 0],
  },
  "legacy-0031": {
    capability: ["legacy", 200, null],
    legacy: ["legacy", 200, 2, null, 0],
    filtered: ["none", null, 0, null, 0],
  },
  "artifact-a-pre-0031": {
    capability: ["transitional", 200, 0],
    legacy: ["none", null, 0, null, 0],
    filtered: ["none", null, 0, null, 0],
  },
  "artifact-a-0031": {
    capability: ["transitional", 200, 0],
    legacy: ["none", null, 0, null, 0],
    filtered: ["none", null, 0, null, 0],
  },
  "artifact-b-pre-0031": {
    capability: ["unavailable", 503, 0],
    legacy: ["none", null, 0, null, 0],
    filtered: ["none", null, 0, null, 0],
  },
  "artifact-b-0031": {
    capability: ["filtered", 200, 1],
    legacy: ["legacy", 200, 2, null, 0],
    filtered: ["filtered", 200, 1, SENDER, 0],
  },
} as const;

function boundProbe(
  phase: keyof typeof PHASE_FIXTURES,
  values: readonly [string, number | null, number, string | null, number],
) {
  return {
    phase,
    worker_commit: WORKER_COMMIT,
    worker_version: WORKER_VERSION,
    deployment_id: DEPLOYMENT_ID,
    client_commit: CLIENT_COMMIT,
    request_mode: values[0],
    status: values[1],
    item_count: values[2],
    echo: values[3],
    cross_sender_count: values[4],
  };
}

function phaseReceipt(
  phase: keyof typeof PHASE_FIXTURES,
  closure: ReturnType<typeof validateRolloutSourceClosure>,
  overrides: Record<string, unknown> = {},
) {
  const fixture = PHASE_FIXTURES[phase];
  const [mode, status, version] = fixture.capability;
  return createSenderFilterPhaseReceipt({
    expected_commit: EXPECTED_COMMIT,
    worker_commit: WORKER_COMMIT,
    client_commit: CLIENT_COMMIT,
    worker_version: WORKER_VERSION,
    deployment_id: DEPLOYMENT_ID,
    migration_0031_sha256:
      closure.file_sha256[
        "keyserver-cf/migrations/0031_control_inbox_sender_retention.sql"
      ],
    source_closure_sha256: closure.source_closure_sha256,
    call_sites: ROLLOUT_CALL_SITES,
    phase,
    traffic: phase.startsWith("artifact-a") ? "quiesced" : "active",
    captured_at: new Date(NOW).toISOString(),
    capability_probe: {
      phase,
      worker_commit: WORKER_COMMIT,
      worker_version: WORKER_VERSION,
      deployment_id: DEPLOYMENT_ID,
      client_commit: CLIENT_COMMIT,
      mode,
      status,
      version,
    },
    legacy_probe: boundProbe(phase, fixture.legacy),
    filtered_probe: boundProbe(phase, fixture.filtered),
    ...overrides,
  });
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
    expect(closure.source_closure_sha256).toMatch(/^[1-9a-f][0-9a-f]{63}$/);
    expect(Object.keys(closure.file_sha256)).toEqual(ROLLOUT_SOURCE_PATHS);

    const commentDecoy = structuredClone(files);
    commentDecoy["crates/keystore/src/client.rs"] = commentDecoy[
      "crates/keystore/src/client.rs"
    ].replace(
      "load_sender_filter_capability_floor(identity)?",
      "SenderFilterCapabilityFloor::NeverObserved /* load_sender_filter_capability_floor(identity)? */",
    );
    expect(() => validateRolloutSourceClosure(commentDecoy)).toThrow(
      /semantic source contract/,
    );

    const brokerBypass = structuredClone(files);
    brokerBypass["apps/osl-hub/src/broker.rs"] = brokerBypass[
      "apps/osl-hub/src/broker.rs"
    ].replace(
      "client.get_control_inbox_compatible_from(identity, peer_osl_user_id)",
      "client.get_control_inbox_from(identity, peer_osl_user_id)",
    );
    expect(() => validateRolloutSourceClosure(brokerBypass)).toThrow(
      /semantic source contract/,
    );

    const resettable = structuredClone(files);
    resettable["crates/keystore/src/sender_filter_rollout.rs"] = resettable[
      "crates/keystore/src/sender_filter_rollout.rs"
    ].replace(".create_new(true)", ".create(true)");
    expect(() => validateRolloutSourceClosure(resettable)).toThrow(
      /semantic source contract/,
    );

    const loweringApi = structuredClone(files);
    loweringApi["crates/keystore/src/sender_filter_rollout.rs"] =
      loweringApi["crates/keystore/src/sender_filter_rollout.rs"].replace(
        "#[cfg(test)]",
        "fn reset_floor(path: &Path) { let _ = std::fs::remove_file(path); }\n\n#[cfg(test)]",
      );
    expect(() => validateRolloutSourceClosure(loweringApi)).toThrow(
      /lowering path/,
    );
  });

  it("accepts nonempty exact phase receipts but never authorizes execution", async () => {
    const closure = validateRolloutSourceClosure(await sourceFiles());
    const expectedSelections = {
      "legacy-pre-0031": "quiesce-traffic",
      "legacy-0031": "artifact-b",
      "artifact-a-pre-0031": "migrations-0030-0031",
      "artifact-a-0031": "artifact-b",
      "artifact-b-pre-0031": "none",
      "artifact-b-0031": "stable-compatible",
    };
    for (const phase of Object.keys(PHASE_FIXTURES) as Array<
      keyof typeof PHASE_FIXTURES
    >) {
      const plan = admitSenderFilterRolloutPlan({
        phaseReceipt: phaseReceipt(phase, closure),
        sourceClosure: closure,
        nowMs: NOW,
      });
      expect(plan).toMatchObject({
        plan_admitted: phase !== "artifact-b-pre-0031",
        next_selection: expectedSelections[phase],
        direct_deploy_permitted: false,
        execution_authorized: false,
      });
    }
  });

  it("refuses stale, cross-phase, source-mismatched, empty, and leaky receipts", async () => {
    const closure = validateRolloutSourceClosure(await sourceFiles());
    const stale = phaseReceipt("artifact-b-0031", closure, {
      captured_at: "2026-07-27T11:50:00.000Z",
    });
    expect(() =>
      verifySenderFilterPhaseReceipt(stale, closure, NOW),
    ).toThrow(/stale or future-dated/);

    const wrongSource = phaseReceipt("artifact-b-0031", closure, {
      source_closure_sha256: "d".repeat(64),
    });
    expect(() =>
      verifySenderFilterPhaseReceipt(wrongSource, closure, NOW),
    ).toThrow(/source or migration closure mismatch/);

    const crossPhase = phaseReceipt("artifact-b-0031", closure);
    crossPhase.filtered_probe.phase = "legacy-0031";
    const resignedCrossPhase =
      createSenderFilterPhaseReceipt(crossPhase);
    expect(() =>
      verifySenderFilterPhaseReceipt(resignedCrossPhase, closure, NOW),
    ).toThrow(/not bound to the receipt phase/);

    const empty = phaseReceipt("artifact-b-0031", closure);
    empty.filtered_probe.item_count = 0;
    const resignedEmpty = createSenderFilterPhaseReceipt(empty);
    expect(() =>
      verifySenderFilterPhaseReceipt(resignedEmpty, closure, NOW),
    ).toThrow(/filtered probe phase mismatch/);

    const leaky = phaseReceipt("artifact-b-0031", closure);
    leaky.filtered_probe.cross_sender_count = 1;
    const resignedLeaky = createSenderFilterPhaseReceipt(leaky);
    expect(() =>
      verifySenderFilterPhaseReceipt(resignedLeaky, closure, NOW),
    ).toThrow(/filtered isolation probe mismatch/);
  });

  it("keeps Artifact A active traffic and Worker-first B fail closed", async () => {
    const closure = validateRolloutSourceClosure(await sourceFiles());
    const activeA = phaseReceipt("artifact-a-pre-0031", closure, {
      traffic: "active",
    });
    expect(
      admitSenderFilterRolloutPlan({
        phaseReceipt: activeA,
        sourceClosure: closure,
        nowMs: NOW,
      }).reasons,
    ).toContain("artifact-a-requires-quiesced-traffic");
    expect(
      admitSenderFilterRolloutPlan({
        phaseReceipt: phaseReceipt(
          "artifact-b-pre-0031",
          closure,
        ),
        sourceClosure: closure,
        nowMs: NOW,
      }).reasons,
    ).toContain("worker-first-artifact-b-refused");
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
