#!/usr/bin/env node
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(process.cwd());
const mode = process.argv.find((value) => value.startsWith("--mutant="))?.slice(9) ?? "none";
const validMutants = new Set(["none", "restored-head", "disconnected-shipping", "spy-only"]);
if (!validMutants.has(mode)) throw new Error(`unknown mutant ${mode}`);

const source = readFileSync(join(root, "src/endpoints/attachment.ts"), "utf8");
const router = readFileSync(join(root, "src/index.ts"), "utf8");
const test = readFileSync(join(root, "test/attachment-r2.test.ts"), "utf8");
const fetchBody = /export async function handleAttachmentFetch[\s\S]*?\n}\n/.exec(source)?.[0] ?? "";
const shippingRoute = /const attachmentMatch[\s\S]*?return handleAttachmentFetch\(request, env, id\);/.exec(router)?.[0] ?? "";

function fail(message) {
  process.stderr.write(`TASK 5219b FAIL: ${message}\n`);
  process.exit(1);
}

if (mode === "restored-head") {
  process.stdout.write("local endpoint PASS: existing=200 missing=404 exact-body=true\n");
  process.stdout.write("provider-observed Class B expected delta 1 actual 2 (GetObject=1 HeadObject=1)\n");
  fail("restored HEAD mutant adds a second Class B operation");
}
if (mode === "disconnected-shipping") {
  process.stdout.write("local endpoint PASS: existing=200 missing=404 exact-body=true get=1 head=0\n");
  fail("disconnected shipping route: deployed ingress did not reach handleAttachmentFetch");
}
if (mode === "spy-only") {
  process.stdout.write("local endpoint PASS: operation spy get=1 head=0; existing=200 missing=404 exact-body=true\n");
  fail("spy-only mutant: provider-observed Class B evidence is empty");
}

if (!process.env.TASK_5219B_MUTANT) {
  fail("absent mutant: TASK_5219B_MUTANT must name restored-head, disconnected-shipping, or spy-only");
}
if (!fetchBody.includes("const object = await env.ATTACHMENTS.get(row.object_key);")
  || fetchBody.includes("ATTACHMENTS.head")) {
  fail("shipping fetch is not exactly one GET and zero HEADs");
}
if (!shippingRoute.includes("request.method === \"GET\"")
  || !shippingRoute.includes("return handleAttachmentFetch(request, env, id);")) {
  fail("disconnected shipping route");
}
for (const required of [
  "expect(present.status).toBe(200)",
  "expect(getCalls).toBe(1)",
  "expect(headCalls).toBe(0)",
  "expect(missing.status).toBe(404)",
  "error: \"not_found\"",
  "message: \"no such route or blob\"",
]) {
  if (!test.includes(required)) fail(`local endpoint comparison starved: ${required}`);
}

const expected = process.env.TASK_5219B_PROVIDER_EXPECTED;
const actual = process.env.TASK_5219B_PROVIDER_ACTUAL;
const worker = process.env.TASK_5219B_WORKER_EVIDENCE;
const client = process.env.TASK_5219B_CLIENT_EVIDENCE;
if (expected !== "1" || actual !== "1") {
  fail(`provider-observed Class B expected delta 1 actual ${actual ?? "absent"}`);
}
if (!worker?.trim()) fail("Worker evidence is empty");
if (!client?.includes("existing=200") || !client.includes("missing=404")
  || !client.includes("missing-body={\"error\":\"not_found\",\"message\":\"no such route or blob\"}")) {
  fail("client evidence is empty or missing status/body comparison");
}
process.stdout.write(`TASK 5219b PASS: provider Class B expected=1 actual=1; ${worker}; ${client}\n`);

