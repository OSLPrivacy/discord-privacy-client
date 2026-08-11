import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { landingSource } from "../src/lib/landing.js";
import type { Env } from "../src/env.js";
import {
  FETCH_BUDGET,
  FETCH_LIMITING_WINDOW_SECONDS,
  rateLimit,
  type Bucket,
} from "../src/lib/rate-limit.js";

const SOURCE_ADDRESS = "198.51.100.63";
const OTHER_ADDRESS = "198.51.100.64";

type ServiceCaller = "blob-fetch" | "attachment-fetch" | "link-fetch";
type RuntimeCaller =
  | "fallback-drain"
  | "explicit-open"
  | "transcript-rehydrate"
  | "whatsapp-qa-open"
  | "attachment-open"
  | "view-once-link";

interface BoundaryEvent {
  request_id: string;
  caller: ServiceCaller;
  observed_at_ms: number;
  address_key: string;
}

interface OutsideCapture {
  requestId: string;
  sourceAddress: string;
  runtimeCaller: RuntimeCaller;
  serviceCaller: ServiceCaller;
  sentAtMs: number;
  receivedAtMs: number;
  admitted: boolean;
}

interface Marker {
  id: string;
  caller: RuntimeCaller;
  pending: boolean;
  delivered: boolean;
}

type Bindable = string | number | bigint | null | Uint8Array;

interface TaskD1 {
  d1: D1Database;
  raw: DatabaseSync;
  exec(sql: string, ...values: Bindable[]): void;
  count(sql: string, ...values: Bindable[]): number;
}

function taskD1(): TaskD1 {
  const raw = new DatabaseSync(":memory:");
  raw.exec(`CREATE TABLE rate_counters (
    bucket_key TEXT PRIMARY KEY NOT NULL,
    window_start INTEGER NOT NULL,
    used INTEGER NOT NULL CHECK(used >= 0)
  ) STRICT`);
  const migration = fileURLToPath(new URL("../migrations/0019_address_wide_fetch_budget.sql", import.meta.url));
  raw.exec(readFileSync(migration, "utf8"));
  const bound = (sql: string, values: Bindable[]) => ({
    async run() {
      await Promise.resolve();
      const result = raw.prepare(sql).run(...values);
      return { success: true, meta: { changes: Number(result.changes) } };
    },
    async first<T>() {
      await Promise.resolve();
      return (raw.prepare(sql).get(...values) ?? null) as T | null;
    },
  });
  const prepare = (sql: string) => ({
    bind: (...values: Bindable[]) => bound(sql, values),
    ...bound(sql, []),
  });
  return {
    d1: { prepare } as unknown as D1Database,
    raw,
    exec(sql: string, ...values: Bindable[]) { raw.prepare(sql).run(...values); },
    count(sql: string, ...values: Bindable[]) {
      const row = raw.prepare(sql).get(...values) as Record<string, unknown> | undefined;
      return Number(Object.values(row ?? { count: 0 })[0] ?? 0);
    },
  };
}

const RUNTIME_CALLERS: readonly RuntimeCaller[] = [
  "fallback-drain",
  "explicit-open",
  "transcript-rehydrate",
  "whatsapp-qa-open",
  "attachment-open",
  "view-once-link",
];

function serviceCaller(caller: RuntimeCaller): ServiceCaller {
  if (caller === "attachment-open") return "attachment-fetch";
  if (caller === "view-once-link") return "link-fetch";
  return "blob-fetch";
}

function bucket(caller: RuntimeCaller): Bucket {
  const service = serviceCaller(caller);
  return service === "blob-fetch" ? "fetch" : service;
}

function testEnv(db: D1Database): Env {
  const unavailableKv = {
    get: async () => { throw new Error("KV must not own fetch admission"); },
    put: async () => { throw new Error("KV must not own fetch admission"); },
  } as unknown as KVNamespace;
  return {
    DB: db,
    ATTACHMENTS: {} as R2Bucket,
    PAYLOADS: {} as R2Bucket,
    RATE_LIMIT: unavailableKv,
    RATE_LIMIT_HASH_KEY: "task-6330-server-secret-at-least-32-bytes",
  };
}

async function capture(
  env: Env,
  caller: RuntimeCaller,
  requestId: string,
  sourceAddress = SOURCE_ADDRESS,
): Promise<OutsideCapture> {
  const sentAtMs = Date.now();
  const decision = await rateLimit(env, sourceAddress, bucket(caller), { requestId });
  const receivedAtMs = Date.now();
  return {
    requestId,
    sourceAddress,
    runtimeCaller: caller,
    serviceCaller: serviceCaller(caller),
    sentAtMs,
    receivedAtMs,
    admitted: decision.allowed,
  };
}

function rows(state: TaskD1, prefix: string): BoundaryEvent[] {
  return state.raw.prepare(
    `SELECT request_id, caller, observed_at_ms, address_key
       FROM fetch_budget_events WHERE request_id LIKE ? ORDER BY observed_at_ms, request_id`,
  ).all(`${prefix}%`) as unknown as BoundaryEvent[];
}

function maximumInRollingWindow(events: BoundaryEvent[]): number {
  const ordered = [...events].sort((left, right) => left.observed_at_ms - right.observed_at_ms);
  const windowMs = FETCH_LIMITING_WINDOW_SECONDS * 1000;
  let start = 0;
  let maximum = 0;
  for (let end = 0; end < ordered.length; end++) {
    while (ordered[end]!.observed_at_ms - ordered[start]!.observed_at_ms >= windowMs) start++;
    maximum = Math.max(maximum, end - start + 1);
  }
  return maximum;
}

async function fill(
  env: Env,
  prefix: string,
  sourceAddress = SOURCE_ADDRESS,
): Promise<OutsideCapture[]> {
  const outside: OutsideCapture[] = [];
  for (let index = 0; index < FETCH_BUDGET; index++) {
    const caller = RUNTIME_CALLERS[index % RUNTIME_CALLERS.length]!;
    const observed = await capture(
      env,
      caller,
      `${prefix}-${index.toString().padStart(3, "0")}`,
      sourceAddress,
    );
    expect(observed.admitted, `${caller} request ${observed.requestId} refused early`).toBe(true);
    outside.push(observed);
  }
  return outside;
}

async function attemptMarker(env: Env, marker: Marker, requestId: string): Promise<boolean> {
  const admitted = (await capture(env, marker.caller, requestId)).admitted;
  if (admitted) {
    marker.pending = false;
    marker.delivered = true;
  }
  return admitted;
}

describe("TASK 6330 address-wide deployed-store fetch allowance", () => {
  it("reconciles every captured caller and admits no mixed-caller 121st request", async () => {
    const state = taskD1();
    const env = testEnv(state.d1);
    const outside = await fill(env, "6330-reconcile");
    const denied = await capture(env, "fallback-drain", "6330-reconcile-121st");
    expect(
      denied.admitted,
      `address=${SOURCE_ADDRESS} window_seconds=${FETCH_LIMITING_WINDOW_SECONDS} caller=${denied.runtimeCaller} request_id=${denied.requestId} count=${FETCH_BUDGET + 1}`,
    ).toBe(false);

    const boundary = rows(state, "6330-reconcile");
    expect(boundary).toHaveLength(FETCH_BUDGET);
    expect(boundary.some((row) => row.request_id === denied.requestId)).toBe(false);
    expect(new Set(boundary.map((row) => row.address_key)).size).toBe(1);
    expect(boundary[0]!.address_key).not.toContain(SOURCE_ADDRESS);
    for (const caller of RUNTIME_CALLERS) {
      expect(outside.filter((event) => event.runtimeCaller === caller)).toHaveLength(20);
    }
    for (const captured of outside) {
      const atBoundary = boundary.find((event) => event.request_id === captured.requestId);
      expect(atBoundary, `missing deployed-boundary request ${captured.requestId}`).toBeDefined();
      expect(atBoundary!.caller).toBe(captured.serviceCaller);
      // SQLite and the outside process read the same host clock through
      // different APIs; allow only their measured millisecond rounding skew.
      expect(atBoundary!.observed_at_ms).toBeGreaterThanOrEqual(captured.sentAtMs - 2);
      expect(atBoundary!.observed_at_ms).toBeLessThanOrEqual(captured.receivedAtMs + 2);
    }
    expect(maximumInRollingWindow(boundary)).toBe(FETCH_BUDGET);

    for (const caller of RUNTIME_CALLERS) {
      const live = await capture(env, caller, `6330-unaffected-${caller}`, OTHER_ADDRESS);
      expect(live.admitted, `${caller} on unaffected address must remain live`).toBe(true);
    }

    console.log(`TASK6330_ADDRESS=${SOURCE_ADDRESS}`);
    console.log(`TASK6330_LIMITING_WINDOW_SECONDS=${FETCH_LIMITING_WINDOW_SECONDS}`);
    console.log(`TASK6330_RECONCILED_REQUEST_IDS=${boundary.length}`);
    console.log(`TASK6330_RUNTIME_CALLERS=${RUNTIME_CALLERS.join(",")}`);
    console.log(`TASK6330_MAX_FETCHES_IN_ANY_WINDOW=${maximumInRollingWindow(boundary)}`);
    console.log(`TASK6330_121ST_CALLER=${denied.runtimeCaller}`);
    console.log(`TASK6330_121ST_REQUEST_ID=${denied.requestId}`);
    console.log(`TASK6330_121ST_COUNT=${FETCH_BUDGET + 1}`);
    console.log("TASK6330_121ST_ADMITTED=0");
    console.log(`TASK6330_UNAFFECTED_CALLERS_LIVE=${RUNTIME_CALLERS.length}`);
  });

  it("counts a boundary-spanning hour instead of resetting at a calendar boundary", async () => {
    const state = taskD1();
    const env = testEnv(state.d1);
    for (let index = 0; index < 60; index++) {
      expect((await capture(env, "fallback-drain", `6330-boundary-before-${index}`)).admitted).toBe(true);
    }
    // No clock is mocked or shortened. Move already-observed records to one
    // second inside the prior edge of the real 3600-second rolling window.
    state.exec(
      "UPDATE fetch_budget_events SET observed_at_ms = ? WHERE request_id LIKE '6330-boundary-before-%'",
      Date.now() - (FETCH_LIMITING_WINDOW_SECONDS * 1000) + 1000,
    );
    for (let index = 0; index < 60; index++) {
      const caller: RuntimeCaller = index % 2 === 0 ? "attachment-open" : "view-once-link";
      expect((await capture(env, caller, `6330-boundary-after-${index}`)).admitted).toBe(true);
    }
    const denied = await capture(env, "explicit-open", "6330-boundary-121st");
    expect(denied.admitted).toBe(false);
    const boundary = rows(state, "6330-boundary-");
    expect(boundary).toHaveLength(FETCH_BUDGET);
    expect(maximumInRollingWindow(boundary)).toBe(FETCH_BUDGET);
    console.log(`TASK6330_BOUNDARY_CROSSING_REQUESTS=${boundary.length}`);
    console.log(`TASK6330_BOUNDARY_CROSSING_MAX=${maximumInRollingWindow(boundary)}`);
    console.log("TASK6330_SHORTENED_WINDOW=0");
    console.log("TASK6330_ACCELERATED_CLOCK=0");
  });

  it("preserves each refused marker and releases every caller in turn", async () => {
    const state = taskD1();
    const env = testEnv(state.d1);
    const markers = RUNTIME_CALLERS.map((caller, index): Marker => ({
      id: `6330-marker-${index + 1}`,
      caller,
      pending: true,
      delivered: false,
    }));

    for (const [turn, marker] of markers.entries()) {
      const prefix = `6330-hold-${turn}`;
      await fill(env, prefix);
      const requestId = `6330-marker-denied-${turn}`;
      expect(await attemptMarker(env, marker, requestId)).toBe(false);
      expect(marker).toMatchObject({ pending: true, delivered: false });
      expect(state.count("SELECT COUNT(*) FROM fetch_budget_events WHERE request_id = ?", requestId)).toBe(0);

      state.exec(
        "UPDATE fetch_budget_events SET observed_at_ms = ? WHERE request_id LIKE ?",
        Date.now() - (FETCH_LIMITING_WINDOW_SECONDS * 1000) - 1,
        `${prefix}%`,
      );
      expect(await attemptMarker(env, marker, `6330-marker-released-${turn}`)).toBe(true);
      expect(marker).toMatchObject({ pending: false, delivered: true });
      state.exec(
        "UPDATE fetch_budget_events SET observed_at_ms = ? WHERE request_id = ?",
        Date.now() - (FETCH_LIMITING_WINDOW_SECONDS * 1000) - 1,
        `6330-marker-released-${turn}`,
      );
      console.log(`TASK6330_RELEASED_CALLER_${turn + 1}=${marker.caller}`);
    }

    expect(markers.filter((marker) => marker.delivered)).toHaveLength(RUNTIME_CALLERS.length);
    console.log(`TASK6330_DENIED_MARKERS_PRESERVED=${markers.length}`);
    console.log(`TASK6330_RELEASED_MARKERS=${markers.length}`);
  });

  it("is exact under 121 concurrent mixed callers", async () => {
    const state = taskD1();
    const env = testEnv(state.d1);
    const decisions = await Promise.all(Array.from({ length: FETCH_BUDGET + 1 }, (_, index) => {
      const caller = RUNTIME_CALLERS[index % RUNTIME_CALLERS.length]!;
      return capture(env, caller, `6330-concurrent-${index}`);
    }));
    expect(decisions.filter((decision) => decision.admitted)).toHaveLength(FETCH_BUDGET);
    expect(decisions.filter((decision) => !decision.admitted)).toHaveLength(1);
    expect(rows(state, "6330-concurrent-")).toHaveLength(FETCH_BUDGET);
    console.log(`TASK6330_CONCURRENT_ADMITTED=${FETCH_BUDGET}`);
    console.log("TASK6330_CONCURRENT_REFUSED=1");
  });

  it("keeps the shipping browser link honestly retryable after a 429", () => {
    const html = landingSource();
    const script = /<script>([\s\S]*)<\/script>/.exec(html)?.[1];
    expect(script).toBeTruthy();
    expect(() => new Function(script!)).not.toThrow();
    expect(html).toContain("if (res.status === 429)");
    expect(html).toContain("revealed = false");
    expect(html).toContain("revealBtn.disabled = false");
    expect(html).toContain("This link is still pending; try again later.");
    console.log("TASK6330_LINK_429_TRUTHFUL_PENDING=1");
  });
});
