export const PROMISE = "When retention expires, OSL schedules deletion automatically and retries failed cleanup without waiting for a person. OSL does not promise a staffed response time.";

export const FAILURE_EDGES = Object.freeze([
  "transient_provider_denial", "timeout", "partial_batch", "lost_receipt",
  "scheduler_kill", "worker_kill", "service_kill", "machine_restart",
]);

export const MUTANTS = Object.freeze([
  "disable_retry", "lose_due_on_restart", "require_acknowledgement",
  "alert_before_retries_finish", "suppress_terminal_report",
  "promise_5_minute", "promise_10_minute", "promise_15_minute",
]);

const LEASE = 10;
const BACKOFF = 5;

class TransientFailure extends Error {}
class UnrecoverableFailure extends Error {}

class Provider {
  constructor(objects) {
    this.objects = new Map(objects.map(object => [object.id, structuredClone(object)]));
    this.deleteCalls = new Map();
    this.failures = new Map();
  }

  exists(id) { return this.objects.has(id); }

  due(now) {
    return [...this.objects.values()].filter(object => object.dueAt <= now)
      .sort((a, b) => a.dueAt - b.dueAt || a.id.localeCompare(b.id));
  }

  async delete(id, failure) {
    const attempt = (this.failures.get(id) || 0) + 1;
    this.failures.set(id, attempt);
    if (failure === "transient_provider_denial" && attempt === 1) {
      throw new TransientFailure("provider_denial");
    }
    if (failure === "timeout" && attempt === 1) {
      throw new TransientFailure("provider_timeout");
    }
    if (failure === "terminal") throw new UnrecoverableFailure("policy_lock_permanent");
    if (!this.objects.has(id)) return;
    this.deleteCalls.set(id, (this.deleteCalls.get(id) || 0) + 1);
    this.objects.delete(id);
    if (failure === "lost_receipt" && attempt === 1) {
      throw new TransientFailure("provider_receipt_lost");
    }
  }
}

class DurableJobs {
  constructor(snapshot) {
    this.jobs = new Map((snapshot?.jobs || []).map(job => [job.id, structuredClone(job)]));
    this.reports = new Map((snapshot?.reports || []).map(report => [report.id, structuredClone(report)]));
  }

  snapshot() {
    return JSON.parse(JSON.stringify({ jobs: [...this.jobs.values()], reports: [...this.reports.values()] }));
  }

  discover(objects) {
    for (const object of objects) if (!this.jobs.has(object.id)) {
      this.jobs.set(object.id, {
        id: object.id, policy: object.policy, dueAt: object.dueAt,
        attempts: 0, leaseUntil: 0, retryAt: 0, state: "pending",
        terminalReason: null, terminalCount: 0,
      });
    }
  }

  claim(now) {
    const job = [...this.jobs.values()]
      .filter(item => item.state === "pending" && item.retryAt <= now && item.leaseUntil <= now)
      .sort((a, b) => a.dueAt - b.dueAt || a.id.localeCompare(b.id))[0];
    if (!job) return null;
    job.attempts += 1;
    job.leaseUntil = now + LEASE;
    return job;
  }

  complete(job) { this.jobs.delete(job.id); }

  retry(job, now) {
    job.leaseUntil = 0;
    job.retryAt = now + BACKOFF;
    job.terminalReason = null;
    job.terminalCount = 0;
  }

  unrecoverable(job, reason, now, early = false) {
    job.terminalCount = job.terminalReason === reason ? job.terminalCount + 1 : 1;
    job.terminalReason = reason;
    job.leaseUntil = 0;
    job.retryAt = now + BACKOFF;
    if (job.terminalCount >= 3 || early) {
      job.state = "terminal";
      const reportId = `${job.policy}:${job.id}:${reason}`;
      if (!this.reports.has(reportId)) this.reports.set(reportId, {
        id: reportId, policy: job.policy, object: job.id,
        oldestDueItem: job.id, attempts: job.attempts,
        terminalReason: reason, state: "pending",
      });
    }
  }
}

function fixtures(now) {
  const due = [
    ["free-denial", "free_7d", "transient_provider_denial"],
    ["pro-timeout", "pro_30d", "timeout"],
    ["downgrade-partial", "downgraded_pro_7d", "partial_batch"],
    ["backup-lost", "backup_cleanup", "lost_receipt"],
    ["free-scheduler", "free_7d", "scheduler_kill"],
    ["pro-worker", "pro_30d", "worker_kill"],
    ["downgrade-service", "downgraded_pro_7d", "service_kill"],
    ["backup-machine", "backup_cleanup", "machine_restart"],
  ].map(([id, policy, failure], index) => policyObject(id, policy, failure, now - 100 + index));
  const live = [
    ["free-live", "free_7d"], ["pro-live", "pro_30d"],
    ["downgrade-live", "downgraded_pro_7d"], ["backup-live", "backup_cleanup"],
  ].map(([id, policy], index) => policyObject(id, policy, null, now + 10_000 + index));
  return { due, live, all: [...due, ...live] };
}

function policyObject(id, policy, failure, dueAt) {
  if (policy === "free_7d") {
    return { id, policy, failure, dueAt, createdAt: dueAt - 7 * 86_400 };
  }
  if (policy === "pro_30d") {
    return { id, policy, failure, dueAt, createdAt: dueAt - 30 * 86_400 };
  }
  if (policy === "downgraded_pro_7d") {
    const downgradedAt = dueAt - 7 * 86_400;
    const originalExpiresAt = dueAt + 10 * 86_400;
    return { id, policy, failure, dueAt, downgradedAt, originalExpiresAt };
  }
  return { id, policy, failure, dueAt, backupCompletedAt: dueAt - 30 * 86_400 };
}

function failureFor(object, starved) {
  return starved === `failure:${object.failure}` ? null : object.failure;
}

export async function runRecoveryCampaign({ mutant = null, starved = null } = {}) {
  let now = 2_000_000_000;
  const fixture = fixtures(now);
  const provider = new Provider(fixture.all);
  let durable = new DurableJobs();
  const observedEdges = new Set();
  const expectedDue = fixture.due.map(object => object.id);
  const preserve = fixture.live.map(object => object.id);
  const byId = new Map(fixture.all.map(object => [object.id, object]));

  // A scheduler invocation is killed before discovery. The next invocation
  // independently queries provider time/inventory and rediscovers the item.
  if (starved !== "failure:scheduler_kill") observedEdges.add("scheduler_kill");
  durable.discover(provider.due(now));

  // A Worker dies after leasing one due job and before touching the provider.
  const worker = durable.jobs.get("pro-worker");
  if (worker && starved !== "failure:worker_kill") {
    worker.attempts += 1; worker.leaseUntil = now + LEASE;
    observedEdges.add("worker_kill");
  }

  // A machine restart rehydrates only the durable snapshot. The provider is
  // independent, so due work is still discoverable even if local memory died.
  if (starved !== "failure:machine_restart") observedEdges.add("machine_restart");
  durable = mutant === "lose_due_on_restart"
    ? new DurableJobs()
    : new DurableJobs(durable.snapshot());
  if (mutant !== "lose_due_on_restart") durable.discover(provider.due(now));

  let partialStopped = false;
  for (let round = 0; round < 30; round += 1) {
    now += LEASE + BACKOFF;
    if (mutant !== "lose_due_on_restart") durable.discover(provider.due(now));
    let processed = 0;
    while (true) {
      const job = durable.claim(now);
      if (!job) break;
      const object = byId.get(job.id);
      const failure = failureFor(object, starved);
      if (failure) observedEdges.add(failure);
      if (failure === "worker_kill" && job.attempts === 1) continue;
      if (failure === "service_kill" && job.attempts === 1) {
        await provider.delete(job.id, null);
        observedEdges.add("service_kill");
        continue;
      }
      if (failure === "partial_batch" && !partialStopped) {
        partialStopped = true;
        durable.retry(job, now);
        break;
      }
      try {
        // An existence oracle closes the lost-receipt/service-kill ambiguity
        // before another delete call, preserving exactly-once provider effect.
        if (!provider.exists(job.id)) durable.complete(job);
        else {
          await provider.delete(job.id, failure);
          durable.complete(job);
        }
      } catch (error) {
        if (error instanceof UnrecoverableFailure) durable.unrecoverable(job, error.message, now);
        else if (mutant === "disable_retry") job.state = "abandoned";
        else durable.retry(job, now);
      }
      processed += 1;
      if (processed > 100) throw new Error("recovery loop unbounded");
    }
  }

  return {
    provider, durable, expectedDue, preserve, observedEdges, now,
    policies: new Set(fixture.all.map((object) => object.policy)),
    oracleObjects: fixture.all,
  };
}

export async function runTerminalCampaign({ mutant = null } = {}) {
  let now = 2_100_000_000;
  const object = { id: "terminal-object", policy: "pro_30d", dueAt: now - 1, failure: "terminal" };
  const provider = new Provider([object]);
  const durable = new DurableJobs();
  durable.discover([object]);
  const telegram = [];
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    now += LEASE + BACKOFF;
    const job = durable.claim(now);
    if (!job) throw new Error(`terminal object missing attempt=${attempt}`);
    try { await provider.delete(job.id, "terminal"); }
    catch (error) {
      durable.unrecoverable(
        job,
        error.message,
        now,
        mutant === "alert_before_retries_finish",
      );
    }
    if (attempt < 3 && durable.reports.size !== 0) {
      throw new Error(`report alert before recoverable retries finish object=${job.id} attempt=${attempt}`);
    }
  }
  for (const report of durable.reports.values()) {
    if (mutant === "suppress_terminal_report") continue;
    if (mutant === "require_acknowledgement") continue;
    if (report.state === "pending") {
      telegram.push({
        path: "https://api.telegram.org/bot<secret>/sendMessage",
        text: `policy=${report.policy}\nobject=${report.object}\noldest_due_item=${report.oldestDueItem}\nattempts=${report.attempts}\nterminal_reason=${report.terminalReason}`,
      });
      report.state = "delivered";
    }
  }
  // Re-running never sends a delivered report again and requires no ack.
  return { provider, durable, telegram };
}
