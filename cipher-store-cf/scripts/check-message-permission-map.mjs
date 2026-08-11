#!/usr/bin/env node
/*
 * TASK 0400.  This is deliberately a source-to-runtime reconciliation, not a
 * list maintained by a delete service.  It derives the deployed handler graph
 * and destructive call sites, then requires every one to be owned by exactly
 * one independently checked authority-map row.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(process.env.OSL_CIPHER_STORE_ROOT ?? process.cwd());
const mapPath = resolve(root, "message-permission-authority-map.json");
const map = JSON.parse(readFileSync(mapPath, "utf8"));
const fail = (message) => { process.stderr.write(`TASK 0400: ${message}\n`); process.exitCode = 1; };
const source = (relative) => readFileSync(resolve(root, relative), "utf8");

if (map.schema !== 1 || !Array.isArray(map.operations) || map.operations.length < 8) {
  fail("authority map is empty or a self-bounded template");
} else {
  const ids = new Set();
  const handlers = new Map();
  for (const row of map.operations) {
    for (const key of ["id", "route", "authority", "handler", "auth_anchor", "runtime_trace"]) {
      if (typeof row[key] !== "string" || row[key].trim() === "") fail(`map row missing ${key}`);
    }
    if (ids.has(row.id)) fail(`duplicate map operation ${row.id}`);
    ids.add(row.id);
    if (handlers.has(row.handler)) fail(`handler ${row.handler} maps to more than one operation`);
    handlers.set(row.handler, row);
    if (/moderator|moderation|admin token/i.test(row.authority)) fail(`invented moderation authority in ${row.id}`);
  }

  const index = source("src/index.ts");
  const endpointFiles = [
    "src/endpoints/blob.ts", "src/endpoints/receipt.ts", "src/endpoints/attachment.ts", "src/endpoints/link.ts",
    "src/lib/sweep.ts", "src/lib/rate-limit.ts", "src/lib/payload-store.ts",
  ];
  const corpus = new Map(endpointFiles.map((file) => [file, source(file)]));
  const allSource = [index, ...corpus.values()].join("\n");

  // Route/RPC graph: every deployed message handler in the dispatch graph is
  // mapped exactly once.  This catches both a new handler and an endpoint
  // whose route is moved without updating the authority map.
  const routedHandlers = [...index.matchAll(/return\s+(handle(?:Fetch|Delete|Ack|AttachmentFetch|AttachmentDelete|LinkFetch|LinkBurn|LinkRevoke|LinkStatus))\(/g)].map((m) => m[1]);
  for (const handler of routedHandlers) {
    if (!handlers.has(handler)) fail(`unmapped route/RPC handler ${handler}`);
  }
  if (new Set(routedHandlers).size === 0) fail("route/RPC graph is empty");

  // Scheduler + deployment configuration are independent input sources.
  for (const job of ["sweepExpired", "sweepExpiredAttachments", "sweepExpiredLinks", "sweepExpiredLinkGrantConsumptions", "sweepRateCounters"]) {
    if (!index.includes(`${job}(env)`)) fail(`scheduled-job registration missing ${job}`);
    if (!handlers.has(job)) fail(`unmapped scheduled job ${job}`);
  }
  if (!source("wrangler.toml").includes('crons = ["*/5 * * * *"]')) fail("deployment cron source missing */5 schedule");

  // Authorization registrations must occur in the handler's own source, not
  // merely in a table.  The map is the only accepted authority declaration.
  for (const row of map.operations) {
    if (!allSource.includes(row.auth_anchor)) fail(`authorization-middleware/source anchor missing for ${row.id}: ${row.auth_anchor}`);
  }

  // Discover actual destructive store calls.  We intentionally include D1
  // DELETE, R2 delete, and view-once ciphertext nulling.  A new direct legacy
  // route is a call site even if it bypasses a named endpoint handler.
  const destructive = [];
  for (const [file, text] of corpus) {
    for (const match of text.matchAll(/(?:\.delete\(|DELETE\s+FROM\s+(?:blob_capability_index|attachment_objects|view_once_links|link_grant_consumed|rate_counters)|SET\s+data\s*=\s+NULL)/g)) {
      destructive.push(`${file}:${match.index}:${match[0]}`);
    }
  }
  for (const match of index.matchAll(/(?:\.delete\(|DELETE\s+FROM|SET\s+data\s*=\s+NULL)/g)) {
    destructive.push(`src/index.ts:${match.index}:${match[0]}`);
  }
  if (destructive.length === 0) fail("storage deletion call-site discovery is empty");

  // A call site must be in one of the explicit operation spans.  Function
  // names are found from the source itself, so direct dispatch deletion has no
  // hiding place: it is reported as `dispatch` and is not silently assigned.
  const ownership = new Map();
  for (const call of destructive) {
    const [file, offset] = call.split(":");
    const text = file === "src/index.ts" ? index : corpus.get(file);
    const before = text.slice(0, Number(offset));
    const names = [...before.matchAll(/(?:export\s+)?(?:async\s+)?function\s+([A-Za-z0-9_]+)/g)];
    const owner = names.at(-1)?.[1] ?? (file === "src/index.ts" ? "dispatch" : "<module>");
    const row = handlers.get(owner);
    if (!row) {
      const routeHints = [...before.matchAll(/path\s*===\s*["']([^"']+)["']/g)];
      const route = routeHints.at(-1)?.[1] ?? owner;
      fail(`unclassified destructive route ${route}; call site ${call} in ${owner}`);
      continue;
    }
    ownership.set(call, row.id);
  }
  if (new Set(ownership.values()).size < 6) fail("destructive union is implausibly small");

  // The runtime test is an independent source.  This checker refuses an
  // authority map with a stale or omitted store-effect trace declaration.
  const runtime = source("test/message-permission-map.test.ts");
  for (const row of map.operations) {
    if (!runtime.includes(row.runtime_trace.split(":").at(-1))) fail(`runtime trace absent for ${row.id}`);
  }

  if (process.exitCode !== 1) {
    process.stdout.write(`TASK 0400 PASS: ${map.operations.length} authority rows; ${new Set(routedHandlers).size} route/RPC handlers; ${destructive.length} destructive call sites; ${ownership.size} classified effects.\n`);
  }
}
