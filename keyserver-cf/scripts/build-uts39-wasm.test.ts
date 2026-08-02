import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
import { describe, expect, it } from "vitest";

const script = join(process.cwd(), "scripts/build-uts39-wasm.sh");

function measure(bundleBytes: number): { exitStatus: number; report: Record<string, unknown> } {
  const root = mkdtempSync(join(tmpdir(), "uts39-spike-"));
  const bundle = join(root, "bundle");
  const report = join(root, "measurement.json");
  try {
    mkdirSync(bundle);
    writeFileSync(join(bundle, "worker.wasm"), randomBytes(bundleBytes));
    let exitStatus = 0;
    try {
      execFileSync(script, ["--measure-only", bundle, report], { stdio: "pipe" });
    } catch (error) {
      exitStatus = (error as { status?: number }).status ?? 1;
    }
    return { exitStatus, report: JSON.parse(readFileSync(report, "utf8")) as Record<string, unknown> };
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

describe("UTS #39 Worker WASM spike kill criterion", () => {
  it("refuses an over-limit compressed bundle and records a passing measurement", () => {
    const killed = measure(2_700_000);
    expect(killed.exitStatus).toBe(1);
    expect(killed.report.withinLimit).toBe(false);

    const accepted = measure(1_024);
    expect(accepted.exitStatus).toBe(0);
    expect(accepted.report.withinLimit).toBe(true);
    expect(accepted.report.compressedBytes).toBeLessThan(2_621_440);
  });
});
