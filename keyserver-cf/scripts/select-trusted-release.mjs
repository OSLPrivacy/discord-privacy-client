#!/usr/bin/env node
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { runAdmissionCli } from "./admit-readiness-archive.mjs";
import {
  createSenderFilterDeploymentReceipt,
} from "./sender-filter-deployment-contract.mjs";
import {
  loadSenderFilterReceiptInputs,
} from "./create-sender-filter-deployment-receipt.mjs";
import { selectTrustedRelease } from "./trusted-release-selection-contract.mjs";
import {
  TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS,
  verifyDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-contract.mjs";
import {
  consumeDeploymentEvidenceOnce,
  loadCommittedMigrationClosure,
  loadDeploymentEvidenceVerifierChallenge,
  readDeploymentEvidenceReceipt,
} from "./deployment-evidence-receipt-io.mjs";
import {
  UNPROVISIONED_DEPLOYMENT_EVIDENCE_VERIFIER_STORE,
} from "./deployment-evidence-verifier-store.mjs";

const FLAGS = Object.freeze([
  "--expected-commit",
  "--archive-dir",
  "--artifact",
  "--expected-active-version",
  "--producer-receipt",
]);

function usage() {
  throw new Error(
    "usage: node scripts/select-trusted-release.mjs " +
      "--expected-commit <full-git-commit> --archive-dir <archive-directory> " +
      "--artifact <A|B> --expected-active-version <worker-version-uuid> " +
      "--producer-receipt <absolute-signed-receipt-path>",
  );
}

export function parseTrustedReleaseArgs(argv) {
  if (argv.length !== FLAGS.length * 2) usage();
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (
      !FLAGS.includes(flag) ||
      value === undefined ||
      Object.hasOwn(values, flag)
    ) {
      usage();
    }
    values[flag] = value;
  }
  if (!/^[0-9a-f]{40}$/.test(values["--expected-commit"] ?? "")) {
    throw new Error("expected commit must be a full 40-character object id");
  }
  if (!["A", "B"].includes(values["--artifact"])) {
    throw new Error("artifact must be exactly A or B");
  }
  if (
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      values["--expected-active-version"] ?? "",
    )
  ) {
    throw new Error("expected active Worker version must be a UUID");
  }
  return {
    expectedCommit: values["--expected-commit"],
    archiveDir: path.resolve(values["--archive-dir"]),
    artifact: values["--artifact"],
    expectedActiveVersion: values["--expected-active-version"],
    producerReceiptPath: values["--producer-receipt"],
  };
}

export async function runTrustedReleaseSelectionCli(
  argv,
  dependencies = {},
) {
  const options = parseTrustedReleaseArgs(argv);
  const now = dependencies.now ?? (() => Date.now());
  const repoRoot =
    dependencies.repoRoot ??
    path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const loadSource =
    dependencies.loadSource ??
    ((commit) => loadSenderFilterReceiptInputs(repoRoot, commit));
  const admit =
    dependencies.admit ??
    ((args) => runAdmissionCli(args, { write: () => {} }));
  const write = dependencies.write ?? ((text) => process.stdout.write(text));
  const loadMigrations =
    dependencies.loadMigrations ??
    ((commit) => loadCommittedMigrationClosure(repoRoot, commit));
  const loadProducerReceipt =
    dependencies.loadProducerReceipt ?? readDeploymentEvidenceReceipt;
  const trustedProducers =
    dependencies.trustedProducers ??
    TRUSTED_DEPLOYMENT_EVIDENCE_PRODUCERS;
  const verifierStore =
    dependencies.verifierStore ??
    UNPROVISIONED_DEPLOYMENT_EVIDENCE_VERIFIER_STORE;

  const producerReceipt = await loadProducerReceipt(
    options.producerReceiptPath,
  );
  if (
    !producerReceipt ||
    typeof producerReceipt !== "object" ||
    Array.isArray(producerReceipt) ||
    typeof producerReceipt.producer_key_id !== "string" ||
    !trustedProducers[producerReceipt.producer_key_id]
  ) {
    throw new Error(
      "deployment evidence producer is not independently trusted",
    );
  }
  const verifierChallenge = await loadDeploymentEvidenceVerifierChallenge(
    producerReceipt.producer_key_id,
    { nowMs: now(), verifierStore },
  );
  const expectedMigrations = await loadMigrations(options.expectedCommit);
  const sourceReceipt = createSenderFilterDeploymentReceipt({
    ...(await loadSource(options.expectedCommit)),
    nowMs: now(),
  });
  const admissionArgs = [
    "--expected-commit",
    options.expectedCommit,
    "--archive-dir",
    options.archiveDir,
    "--artifact",
    options.artifact,
    "--expected-active-version",
    options.expectedActiveVersion,
  ];
  const readinessReceipt = await admit(admissionArgs);
  const selection = selectTrustedRelease({
    ...options,
    sourceReceipt,
    readinessReceipt,
    producerReceipt,
    verifierChallenge,
    expectedMigrations,
    trustedProducers,
    nowMs: now(),
  });
  const verifiedProducerReceipt = verifyDeploymentEvidenceReceipt(
    producerReceipt,
    {
      archiveId: readinessReceipt.archive_id,
      artifact: options.artifact,
      artifactBundles: readinessReceipt.artifact_bundles,
      expectedCommit: options.expectedCommit,
      expectedDeploymentId: readinessReceipt.deployment_id,
      expectedMigrations,
      expectedWorkerVersion: readinessReceipt.active_worker_version,
      verifierChallenge,
    },
    { trustedProducers, nowMs: now() },
  );
  await consumeDeploymentEvidenceOnce(verifiedProducerReceipt, {
    verifierStore,
  });
  write(`${JSON.stringify(selection, null, 2)}\n`);
  return selection;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  runTrustedReleaseSelectionCli(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
