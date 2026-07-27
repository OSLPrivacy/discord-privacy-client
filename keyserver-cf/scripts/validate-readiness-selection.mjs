#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import process from "node:process";
import {
  readAndVerifyReadinessArtifact,
  validateReadinessSelection,
} from "./readiness-artifact-contract.mjs";

async function main() {
  const [manifestPath, bundlePath, evidencePath] = process.argv.slice(2);
  if (
    !manifestPath ||
    !bundlePath ||
    !evidencePath ||
    process.argv.length !== 5
  ) {
    throw new Error(
      "usage: node scripts/validate-readiness-selection.mjs <manifest.json> <bundle.mjs> <read-only-d1-evidence.json>",
    );
  }
  const manifest = await readAndVerifyReadinessArtifact(
    manifestPath,
    bundlePath,
  );
  const evidence = JSON.parse(await readFile(evidencePath, "utf8"));
  validateReadinessSelection(manifest, evidence);
  process.stdout.write(`selection allowed: artifact ${manifest.artifact}\n`);
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
  process.exitCode = 1;
});
