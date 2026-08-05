/**
 * The two release contracts, run as a step of `npm test` rather than as tests.
 *
 * D-262. Every gate in this project that guards a test lives inside the test
 * run, so deleting the guard deletes the guarding. Both checks below are also
 * asserted from vitest — `test-node/d2-test-closure.test.ts` and
 * `test-node/d2-0010-release-contract.test.ts` — and both of THOSE are ordinary
 * files someone can delete.
 *
 * This entry point closes that loop from outside the suite, because it is
 * invoked by `package.json`'s `test` script and `package.json` is inside
 * `D2_RELEASE_SOURCE_SHA256`. Dropping the step to silence a red gate edits a
 * pinned file, which turns the release-source contract red instead. There is no
 * edit that removes both without moving the digest, and moving the digest is
 * the one operation that has to be justified per file.
 *
 * Run first in `npm test` so a contract failure is the first thing reported and
 * cannot be lost under suite output.
 */

import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { D2_RELEASE_SOURCE_SHA256 } from "./d2-0010-release-contract.ts";
import {
  deriveReleaseSourceFiles,
  localModuleClosure,
  readReleaseRoots,
  releaseSourceManifestSha256,
} from "./d2-release-source-manifest.ts";
import { assertD2TestClosure, readD2ClosureSources } from "./d2-test-closure.ts";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const closure = assertD2TestClosure(readD2ClosureSources(projectRoot));

const roots = readReleaseRoots(projectRoot);
const files = deriveReleaseSourceFiles(projectRoot);
const shipped = localModuleClosure(projectRoot, roots.main);
const unpinned = shipped.filter((file) => !files.includes(file));
if (unpinned.length > 0) {
  throw new Error(
    "D2 release source manifest: modules reach the Worker entry point from "
    + `outside the derived release set: ${unpinned.join(", ")}`,
  );
}

const observed = releaseSourceManifestSha256(projectRoot, files);
if (observed !== D2_RELEASE_SOURCE_SHA256) {
  throw new Error(
    "D2 migration-0010 release contract: derived release source digest "
    + `${observed} does not match the pinned D2_RELEASE_SOURCE_SHA256 `
    + `${D2_RELEASE_SOURCE_SHA256}. Re-anchoring requires a per-file account `
    + `of every changed byte (${files.length} files derived from `
    + `${roots.roots.join(", ")} plus the release config files).`,
  );
}

console.log(
  `[d2-contract-gate] release source ${files.length} files, entry closure `
  + `${shipped.length} modules, digest ${observed.slice(0, 12)}…; test closure `
  + `${closure.propertyTestSuites} property suites, `
  + Object.entries(closure.testFiles)
    .map(([directory, count]) => `${directory}/=${count}`)
    .join(" ")
  + " test files",
);
