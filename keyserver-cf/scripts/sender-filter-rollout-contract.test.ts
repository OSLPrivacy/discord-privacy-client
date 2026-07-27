import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  VERSION_SKEW_MATRIX,
  admitSenderFilterRolloutPlan,
  classifyCapability,
  consumeClientResponse,
  evaluateVersionSkewScenario,
  evaluateWorkerRequest,
  validateRolloutSourceClosure,
  workerHealth,
} from "./sender-filter-rollout-contract.mjs";

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

function positivePlan(overrides: Record<string, unknown> = {}) {
  return {
    worker: "artifact-b",
    schema: "0031",
    traffic: "active",
    capabilityProbe: workerHealth("artifact-b", "0031"),
    legacyProbe: { status: 200, item_count: 2 },
    filteredProbe: {
      status: 200,
      echo: SENDER,
      item_count: 1,
      cross_sender_count: 0,
    },
    ...overrides,
  };
}

async function sourceClosure() {
  const paths = [
    "keyserver-cf/src/endpoints/control-inbox.ts",
    "keyserver-cf/src/endpoints/healthz.ts",
    "keyserver-cf/src/readiness/bridge/control-inbox.ts",
    "keyserver-cf/src/readiness/bridge/healthz.ts",
    "keyserver-cf/src/lib/canonical.ts",
  ];
  return Object.fromEntries(
    await Promise.all(
      paths.map(async (sourcePath) => [
        sourcePath,
        await readFile(path.join(REPO_ROOT, sourcePath), "utf8"),
      ]),
    ),
  );
}

describe("sender-filter Worker/client version-skew contract", () => {
  it("covers every schema × Worker × client state with nonempty continuity fixtures", () => {
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
          active_sender_reachable: true,
          request_mode: "legacy",
          server_status: 200,
        });
      } else if (entry.worker === "artifact-a") {
        expect(result.accepted).toBe(false);
        expect(result.fail_closed).toBe(true);
      } else if (entry.schema === "pre-0031") {
        expect(result.accepted).toBe(false);
        expect(result.fail_closed).toBe(true);
      } else {
        expect(result).toMatchObject({
          accepted: true,
          active_sender_reachable: true,
          server_status: 200,
          request_mode:
            entry.client === "legacy" ? "legacy" : "filtered",
        });
      }
    }
  });

  it("keeps client-first rollout on the byte-identical legacy request until capability 1", () => {
    const clientFirst = evaluateVersionSkewScenario({
      worker: "legacy",
      schema: "pre-0031",
      client: "sender-filter",
      senderId: SENDER,
      rows: continuityRows(),
    });
    expect(clientFirst).toMatchObject({
      accepted: true,
      request_mode: "legacy",
      active_sender_reachable: true,
    });

    expect(
      classifyCapability(workerHealth("artifact-b", "0031"), false),
    ).toEqual({ mode: "filtered", reason: null });
    expect(
      classifyCapability(workerHealth("legacy", "pre-0031"), true),
    ).toEqual({ mode: "refuse", reason: "capability-downgrade" });
    expect(
      classifyCapability(
        {
          status: 200,
          body: {
            ok: true,
            capabilities: { control_inbox_sender_disposition: 2 },
          },
        },
        false,
      ),
    ).toEqual({
      mode: "refuse",
      reason: "capability-shape-or-version-mismatch",
    });
  });

  it("preserves old-client unfiltered drains and fixes new-client head-of-line starvation", () => {
    const oldClient = evaluateVersionSkewScenario({
      worker: "artifact-b",
      schema: "0031",
      client: "legacy",
      senderId: SENDER,
      rows: blockedRows(),
    });
    expect(oldClient).toMatchObject({
      accepted: true,
      request_mode: "legacy",
      server_status: 200,
      active_sender_reachable: false,
      cross_sender_leakage: false,
    });

    const newClient = evaluateVersionSkewScenario({
      worker: "artifact-b",
      schema: "0031",
      client: "sender-filter",
      senderId: SENDER,
      rows: blockedRows(),
    });
    expect(newClient).toMatchObject({
      accepted: true,
      request_mode: "filtered",
      active_sender_reachable: true,
      cross_sender_leakage: false,
    });
  });

  it("refuses malformed, unsigned, stripped, substituted, and old-Worker filtered requests", () => {
    const base = {
      worker: "artifact-b",
      schema: "0031",
      rows: continuityRows(),
    };
    const cases = [
      {
        request: {
          sender_param: "bad\nsender",
          signed_sender: "bad\nsender",
          signature_valid: true,
        },
        status: 400,
        refusal: "malformed-sender",
      },
      {
        request: {
          sender_param: SENDER,
          signed_sender: null,
          signature_valid: true,
        },
        status: 401,
        refusal: "sender-signature-mismatch",
      },
      {
        request: {
          sender_param: null,
          signed_sender: SENDER,
          signature_valid: true,
        },
        status: 401,
        refusal: "sender-signature-mismatch",
      },
      {
        request: {
          sender_param: "sender-other",
          signed_sender: SENDER,
          signature_valid: true,
        },
        status: 401,
        refusal: "sender-signature-mismatch",
      },
    ];
    for (const mutation of cases) {
      expect(
        evaluateWorkerRequest({ ...base, request: mutation.request }),
      ).toMatchObject({
        status: mutation.status,
        items: [],
        refusal: mutation.refusal,
      });
    }
    expect(
      evaluateWorkerRequest({
        ...base,
        worker: "legacy",
        request: {
          sender_param: SENDER,
          signed_sender: SENDER,
          signature_valid: true,
        },
      }),
    ).toMatchObject({
      status: 401,
      refusal: "sender-signature-mismatch",
    });

    expect(
      evaluateWorkerRequest({
        ...base,
        request: {
          sender_param: null,
          signed_sender: null,
          signature_valid: true,
        },
      }),
    ).toMatchObject({
      status: 200,
      filtered_sender_id: null,
    });
  });

  it("new clients reject missing/mismatched echo, cross-sender rows, and filtered starvation", () => {
    const base = {
      mode: "filtered",
      senderId: SENDER,
      sourceRows: continuityRows(),
    };
    const cases = [
      {
        response: {
          status: 200,
          items: [{ id: "selected-1", sender_id: SENDER }],
          filtered_sender_id: null,
        },
        reason: "filtered-sender-echo-mismatch",
      },
      {
        response: {
          status: 200,
          items: [{ id: "foreign", sender_id: "sender-foreign" }],
          filtered_sender_id: SENDER,
        },
        reason: "cross-sender-filter-response",
      },
      {
        response: {
          status: 200,
          items: [],
          filtered_sender_id: SENDER,
        },
        reason: "filtered-positive-starved",
      },
    ];
    for (const mutation of cases) {
      expect(
        consumeClientResponse({ ...base, response: mutation.response }),
      ).toMatchObject({
        accepted: false,
        fail_closed: true,
        reason: mutation.reason,
        cross_sender_leakage: false,
      });
    }
  });

  it("models migration-first as compatible but never authorizes the mutation", () => {
    const before = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "legacy",
        schema: "pre-0031",
        traffic: "quiesced",
        capabilityProbe: workerHealth("legacy", "pre-0031"),
        filteredProbe: {
          status: 401,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(before).toMatchObject({
      plan_admitted: true,
      next_selection: "artifact-a",
      direct_deploy_permitted: false,
      execution_authorized: false,
    });

    const migrationFirst = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "legacy",
        schema: "0031",
        capabilityProbe: workerHealth("legacy", "0031"),
        filteredProbe: {
          status: 401,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(migrationFirst).toMatchObject({
      plan_admitted: true,
      next_selection: "artifact-b",
      direct_deploy_permitted: false,
      execution_authorized: false,
    });
  });

  it("refuses worker-first B, active-traffic A, empty legacy, and leaky filtered probes", () => {
    const workerFirst = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "artifact-b",
        schema: "pre-0031",
        capabilityProbe: workerHealth("artifact-b", "pre-0031"),
        filteredProbe: {
          status: 503,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(workerFirst).toMatchObject({
      plan_admitted: false,
      direct_deploy_permitted: false,
      execution_authorized: false,
      reasons: expect.arrayContaining(["worker-first-artifact-b-refused"]),
    });

    const artifactAActive = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "artifact-a",
        schema: "pre-0031",
        capabilityProbe: workerHealth("artifact-a", "pre-0031"),
        filteredProbe: {
          status: 503,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(artifactAActive.reasons).toContain(
      "artifact-a-requires-quiesced-traffic",
    );

    expect(
      admitSenderFilterRolloutPlan(
        positivePlan({ legacyProbe: { status: 200, item_count: 0 } }),
      ).reasons,
    ).toContain("legacy-inbox-continuity-unproved");
    expect(
      admitSenderFilterRolloutPlan(
        positivePlan({
          filteredProbe: {
            status: 200,
            echo: SENDER,
            item_count: 1,
            cross_sender_count: 1,
          },
        }),
      ).reasons,
    ).toContain("filtered-isolation-unproved");
  });

  it("advances only selection labels through quiesced A to stable B", () => {
    const bridgeBefore = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "artifact-a",
        schema: "pre-0031",
        traffic: "quiesced",
        capabilityProbe: workerHealth("artifact-a", "pre-0031"),
        filteredProbe: {
          status: 503,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(bridgeBefore).toMatchObject({
      plan_admitted: true,
      next_selection: "migrations-0030-0031",
      direct_deploy_permitted: false,
    });
    const bridgeAfter = admitSenderFilterRolloutPlan(
      positivePlan({
        worker: "artifact-a",
        schema: "0031",
        traffic: "quiesced",
        capabilityProbe: workerHealth("artifact-a", "0031"),
        filteredProbe: {
          status: 503,
          echo: null,
          item_count: 0,
          cross_sender_count: 0,
        },
      }),
    );
    expect(bridgeAfter).toMatchObject({
      plan_admitted: true,
      next_selection: "artifact-b",
      execution_authorized: false,
    });
    expect(admitSenderFilterRolloutPlan(positivePlan())).toMatchObject({
      plan_admitted: true,
      next_selection: "stable-compatible",
      direct_deploy_permitted: false,
      execution_authorized: false,
    });
  });

  it("binds the model to nonempty committed Worker/canonical source seams", async () => {
    const files = await sourceClosure();
    expect(validateRolloutSourceClosure(files)).toBe(true);
    for (const sourcePath of Object.keys(files)) {
      const missing = structuredClone(files);
      missing[sourcePath] = "";
      expect(() => validateRolloutSourceClosure(missing)).toThrow(
        /rollout source is empty/,
      );
    }
    const drift = structuredClone(files);
    drift["keyserver-cf/src/endpoints/control-inbox.ts"] = drift[
      "keyserver-cf/src/endpoints/control-inbox.ts"
    ].replace("filtered_sender_id: senderFilter", "filtered_sender_id: null");
    expect(() => validateRolloutSourceClosure(drift)).toThrow(
      /rollout source contract missing/,
    );
  });

  it("keeps the executable plan and package entrypoint non-deploying", async () => {
    const contract = await readFile(
      path.join(
        REPO_ROOT,
        "keyserver-cf/scripts/sender-filter-rollout-contract.mjs",
      ),
      "utf8",
    );
    expect(contract).not.toMatch(/wrangler\s+deploy/);
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
    expect(runbook).toContain("all 12");
    expect(runbook).toContain("pre-existing 64-row head-of-line");
    expect(runbook).toContain("direct_deploy_permitted=false");
    expect(runbook).not.toMatch(/(?:npx|npm exec)\s+wrangler/);
  });
});
