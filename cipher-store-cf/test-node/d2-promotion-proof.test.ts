import { describe, expect, it } from "vitest";
import {
  AGGREGATE_SQL,
  CYCLE_MARKER,
  NATURAL_CRON,
  assertAggregateSql,
  assertCleanCipherStatus,
  assertReviewedCommandPlan,
  parseAggregateOutput,
  parseCycleWitness,
  parseDeploymentAndVersion,
  reviewedCommandPlan,
  sourceTag,
  validateProof,
  type AggregateSnapshot,
  type CycleWitness,
  type SourceFacts,
} from "../scripts/d2-promotion-proof.js";

const SOURCE: SourceFacts = {
  commit_sha: "a".repeat(40),
  cipher_store_tree_sha: "b".repeat(40),
};
const VERSION_ID = "12345678-1234-4234-8234-123456789abc";

function snapshot(
  patch: Partial<AggregateSnapshot> = {},
): AggregateSnapshot {
  return {
    migration_0006_count: 1,
    migration_0007_count: 1,
    content_expiry_column_count: 1,
    attachment_parts_table_count: 1,
    stale_total: 1,
    stale_unmarked: 1,
    stale_retryable: 0,
    ...patch,
  };
}

function d1Output(
  row: AggregateSnapshot | Record<string, unknown> = snapshot(),
  meta: Record<string, unknown> = {
    changes: 0,
    changed_db: false,
    rows_written: 0,
  },
): string {
  return JSON.stringify([{ results: [row], success: true, meta }]);
}

function workerMetadata(tag = sourceTag(SOURCE), percentage = 100) {
  return {
    deployment: JSON.stringify({
      versions: [{ version_id: VERSION_ID, percentage }],
      author_email: "must-not-be-retained@example.invalid",
    }),
    version: JSON.stringify({
      id: VERSION_ID,
      annotations: { "workers/tag": tag },
      metadata: { author_email: "must-not-be-retained@example.invalid" },
      resources: {
        script: { handlers: ["fetch", "scheduled"] },
        script_runtime: { compatibility_date: "2026-05-13" },
      },
    }),
  };
}

function cycle(scheduled = 2_000): CycleWitness {
  return {
    scheduled_time_ms: scheduled,
    event_time_ms: scheduled + 20,
    cron: NATURAL_CRON,
    outcome: "ok",
    marker: CYCLE_MARKER,
  };
}

describe("D2 promotion proof refusal boundary", () => {
  it("accepts only the reviewed aggregate query and rejects semantic mutations", () => {
    expect(() => assertAggregateSql(AGGREGATE_SQL)).not.toThrow();
    expect(() => assertAggregateSql(AGGREGATE_SQL.replace("COUNT(*)", "candidate.id"))).toThrow(
      /differs/,
    );
    expect(() => assertAggregateSql(`${AGGREGATE_SQL}; DELETE FROM attachment_objects`)).toThrow(
      /differs/,
    );
  });

  it.each([
    " M cipher-store-cf/src/index.ts\n",
    "M  cipher-store-cf/src/index.ts\n",
    "?? cipher-store-cf/proof.json\n",
  ])("refuses tracked, staged, or untracked scoped dirt", (status) => {
    expect(() => assertCleanCipherStatus(status)).toThrow(/dirty/);
  });

  it("parses exactly one read-only aggregate row", () => {
    expect(parseAggregateOutput(d1Output())).toEqual(snapshot());
    expect(() =>
      parseAggregateOutput(d1Output(snapshot(), {
        changes: 1,
        changed_db: true,
        rows_written: 1,
      })),
    ).toThrow(/write/);
    expect(() =>
      parseAggregateOutput(d1Output({
        ...snapshot(),
        attachment_id: "do-not-retain",
      })),
    ).toThrow(/unexpected fields/);
  });

  it("refuses missing or duplicate migration 0007 and inconsistent partitions", () => {
    expect(() => parseAggregateOutput(d1Output(snapshot({ migration_0007_count: 0 })))).toThrow(
      /migration/,
    );
    expect(() => parseAggregateOutput(d1Output(snapshot({ migration_0007_count: 2 })))).toThrow(
      /migration/,
    );
    expect(() =>
      parseAggregateOutput(d1Output(snapshot({
        stale_total: 1,
        stale_unmarked: 0,
        stale_retryable: 0,
      }))),
    ).toThrow(/partitions/);
  });

  it("binds exactly one 100-percent Worker version to commit and subtree", () => {
    const fixture = workerMetadata();
    expect(parseDeploymentAndVersion(fixture.deployment, fixture.version, SOURCE)).toEqual({
      version_id: VERSION_ID,
      source_tag: sourceTag(SOURCE),
      traffic_percentage: 100,
    });
    const untagged = workerMetadata("");
    expect(() =>
      parseDeploymentAndVersion(untagged.deployment, untagged.version, SOURCE),
    ).toThrow(/unknown|mismatched/);
    const split = workerMetadata(sourceTag(SOURCE), 50);
    expect(() =>
      parseDeploymentAndVersion(split.deployment, split.version, SOURCE),
    ).toThrow(/100 percent/);
  });

  it("accepts only a successful scheduled marker, never an HTTP/manual shape", () => {
    expect(parseCycleWitness({
      outcome: "ok",
      eventTimestamp: 2_020,
      event: { cron: NATURAL_CRON, scheduledTime: 2_000 },
      logs: [{ level: "log", message: [CYCLE_MARKER] }],
    })).toEqual(cycle());
    expect(() => parseCycleWitness({
      outcome: "ok",
      eventTimestamp: 2_010,
      event: {
        cron: NATURAL_CRON,
        scheduledTime: 2_000,
        request: { url: "/__scheduled" },
      },
      logs: [{ level: "log", message: [CYCLE_MARKER] }],
    })).toThrow(/manual/);
  });

  it("does not mistake marking-for-retry for reclamation", () => {
    const worker = parseDeploymentAndVersion(
      workerMetadata().deployment,
      workerMetadata().version,
      SOURCE,
    );
    expect(() => validateProof({
      source: SOURCE,
      worker,
      before: snapshot(),
      after: snapshot({
        stale_total: 1,
        stale_unmarked: 0,
        stale_retryable: 1,
      }),
      beforeCompletedMs: 1_000,
      afterStartedMs: 3_000,
      cycles: [cycle()],
    })).toThrow(/did not decrease/);
  });

  it("revalidates exact source binding and the measurement clock at proof assembly", () => {
    const worker = parseDeploymentAndVersion(
      workerMetadata().deployment,
      workerMetadata().version,
      SOURCE,
    );
    const base = {
      source: SOURCE,
      worker,
      before: snapshot(),
      after: snapshot({ stale_total: 0, stale_unmarked: 0 }),
      beforeCompletedMs: 1_000,
      afterStartedMs: 3_000,
      cycles: [cycle()],
    };
    expect(() => validateProof({
      ...base,
      worker: { ...worker, source_tag: `unknown-${"c".repeat(40)}` },
    })).toThrow(/exact source/);
    expect(() => validateProof({
      ...base,
      beforeCompletedMs: 3_000,
      afterStartedMs: 1_000,
    })).toThrow(/window/);
    expect(() => validateProof({
      ...base,
      cycles: [{ ...cycle(), event_time_ms: 900 }],
    })).toThrow(/outside/);
  });

  it("requires a nonvacuous before state and exactly one in-window cycle", () => {
    const worker = parseDeploymentAndVersion(
      workerMetadata().deployment,
      workerMetadata().version,
      SOURCE,
    );
    const base = {
      source: SOURCE,
      worker,
      before: snapshot(),
      after: snapshot({ stale_total: 0, stale_unmarked: 0 }),
      beforeCompletedMs: 1_000,
      afterStartedMs: 3_000,
    };
    expect(() => validateProof({ ...base, cycles: [] })).toThrow(/absent|ambiguous/);
    expect(() => validateProof({ ...base, cycles: [cycle(), cycle(2_500)] })).toThrow(
      /absent|ambiguous/,
    );
    expect(() => validateProof({
      ...base,
      before: snapshot({ stale_total: 0, stale_unmarked: 0 }),
      cycles: [cycle()],
    })).toThrow(/vacuous/);
  });

  it("emits only aggregate-safe proof fields", () => {
    const worker = parseDeploymentAndVersion(
      workerMetadata().deployment,
      workerMetadata().version,
      SOURCE,
    );
    const proof = validateProof({
      source: SOURCE,
      worker,
      before: snapshot(),
      after: snapshot({ stale_total: 0, stale_unmarked: 0 }),
      beforeCompletedMs: 1_000,
      afterStartedMs: 3_000,
      cycles: [cycle()],
    });
    const serialized = JSON.stringify(proof);
    expect(serialized).not.toMatch(/author_email|attachment_id|object_key|upload_id|ciphertext|token/i);
    expect(proof.stale_reservations.reclaimed).toBe(1);
  });

  it("builds no migration, row-delete, R2-abort, or scheduler-invocation command", () => {
    const plan = reviewedCommandPlan({
      wranglerPath: "/exact/wrangler",
      archiveDir: "/tmp/exact-source",
      source: SOURCE,
      versionId: VERSION_ID,
    });
    expect(() => assertReviewedCommandPlan(plan)).not.toThrow();
    const rendered = plan.flatMap((item) => item.args).join(" ");
    expect(rendered).not.toMatch(/migrations apply|r2 .*delete|abort|test-scheduled|__scheduled/i);

    const mutated = plan.map((item) => ({ ...item, args: [...item.args] }));
    mutated.push({
      purpose: "manual-scheduler",
      file: "curl",
      args: ["https://worker/__scheduled"],
    });
    expect(() => assertReviewedCommandPlan(mutated)).toThrow(/forbidden/);

    const manualDelete = plan.map((item) => ({ ...item, args: [...item.args] }));
    manualDelete.push({
      purpose: "manual-delete",
      file: "/exact/wrangler",
      args: ["d1", "execute", "db", "--command", "DELETE FROM attachment_objects"],
    });
    expect(() => assertReviewedCommandPlan(manualDelete)).toThrow(/forbidden/);

    const manualAbort = plan.map((item) => ({ ...item, args: [...item.args] }));
    manualAbort.push({
      purpose: "manual-r2-abort",
      file: "operator-tool",
      args: ["abort", "multipart"],
    });
    expect(() => assertReviewedCommandPlan(manualAbort)).toThrow(/forbidden/);
  });
});
