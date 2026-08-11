import { describe, expect, it } from "vitest";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(process.cwd());
const script = resolve(root, "scripts/task-5219b-proof.mjs");

function run(mutant: string, extra: Record<string, string> = {}) {
  const result = spawnSync(process.execPath, [script, `--mutant=${mutant}`], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, ...extra },
  });
  return { status: result.status, output: `${result.stdout}${result.stderr}` };
}

describe("TASK 5219b one fetch cannot hide two Class B operations", () => {
  it.each([
    ["restored-head", "expected delta 1 actual 2"],
    ["disconnected-shipping", "disconnected shipping route"],
    ["spy-only", "provider-observed Class B evidence is empty"],
  ])("makes the %s candidate exit 1 despite its local endpoint", (mutant, reason) => {
    const result = run(mutant);
    expect(result.status).toBe(1);
    expect(result.output).toContain("local endpoint PASS");
    expect(result.output).toContain(reason);
    expect(result.output).toContain("missing=404");
    expect(result.output).toContain("exact-body=true");
  });

  it("fails closed when no mutation was supplied", () => {
    const result = run("none");
    expect(result.status).toBe(1);
    expect(result.output).toContain("absent mutant");
  });

  it("accepts only mutually agreeing non-empty provider, Worker, and client evidence", () => {
    const result = run("none", {
      TASK_5219B_MUTANT: "restored-head,disconnected-shipping,spy-only",
      TASK_5219B_PROVIDER_EXPECTED: "1",
      TASK_5219B_PROVIDER_ACTUAL: "1",
      TASK_5219B_WORKER_EVIDENCE: "get=1 head=0 route=attachment-fetch",
      TASK_5219B_CLIENT_EVIDENCE: "existing=200 missing=404 missing-body={\"error\":\"not_found\",\"message\":\"no such route or blob\"}",
    });
    expect(result.status).toBe(0);
    expect(result.output).toContain("TASK 5219b PASS");
  });
});
