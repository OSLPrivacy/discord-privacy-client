import { spawnSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, "..");
const repoRoot = resolve(packageDir, "../..");
const sourceTest = resolve(packageDir, "src/verification-warning.test.ts");
const breakTest = resolve(packageDir, "src/verification-warning-0747-break.test.ts");

const original = readFileSync(sourceTest, "utf8");
const broken = original
  .replaceAll("TASK-0746", "TASK-0747")
  .replace(
    "VerificationWarningMemory,\n  verificationWarningDecision,",
    "VerificationWarningMemory,\n  verificationWarningDecision as shippingVerificationWarningDecision,",
  )
  .replace(
    "type VerificationWarningDecision,\n  type VerificationWarningSetting,",
    "type VerificationWarningConversation,\n  type VerificationWarningDecision,\n  type VerificationWarningMoment,\n  type VerificationWarningSetting,",
  )
  .replace(
    "interface WarningRun {",
    `function verificationWarningDecision(
  setting: VerificationWarningSetting,
  conversation: VerificationWarningConversation,
  moment: VerificationWarningMoment,
  memory = new VerificationWarningMemory(),
): VerificationWarningDecision {
  if (!conversation.checked && setting === "never" && moment === "prepare-send") {
    return { warn: true, surface: "before-send", opensScreen: false };
  }
  return shippingVerificationWarningDecision(setting, conversation, moment, memory);
}

interface WarningRun {`,
  )
  .replace(
    'expect(resultLine(bySetting.never)).toBe("TASK-0747 never open-warnings=0 prepare-warnings=0 rendered-warnings=0 opened-screens=0 total-warnings=0 sequence=open-1:none,open-2:none,prepare:none");',
    `const neverLine = resultLine(bySetting.never);
    if (bySetting.never.prepareSend.warn) {
      throw new Error(\`unexpected warning: \${neverLine}\`);
    }
    expect(neverLine).toBe("TASK-0747 never open-warnings=0 prepare-warnings=0 rendered-warnings=0 opened-screens=0 total-warnings=0 sequence=open-1:none,open-2:none,prepare:none");`,
  );

if (broken === original) {
  throw new Error("0747 break proof could not construct the test copy");
}

try {
  writeFileSync(breakTest, broken, { flag: "wx" });
  const proof = spawnSync(
    "npm",
    ["--prefix", "apps/osl-hub-ui", "exec", "vitest", "run", "src/verification-warning-0747-break.test.ts"],
    { cwd: repoRoot, encoding: "utf8" },
  );
  process.stdout.write(proof.stdout);
  process.stderr.write(proof.stderr);
  console.log(`proof_exit=${proof.status}`);

  const combined = `${proof.stdout}\n${proof.stderr}`;
  if (proof.status !== 1 || !combined.includes("unexpected warning")) {
    process.exitCode = 1;
  }
} finally {
  rmSync(breakTest, { force: true });
}
