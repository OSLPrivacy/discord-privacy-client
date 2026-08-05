// D-248 — node-side loader for the ONE pinned UTS #39 artifact.
//
// This is NOT a second Unicode implementation. It loads exactly the bytes the
// Worker loads (`src/lib/unicode-identifier/dist/osl_uts39_wasm_bg.wasm`) and
// calls exactly the same exported function. Only the loader differs, because
// the Worker gets the module from Wrangler's `CompiledWasm` rule and node has
// to read it off disk.
//
// The policy it applies is the same policy `src/lib/username.ts` applies, and
// the two are pinned together by `test/fixtures/username-skeletons.json`:
// `test/unit/username.test.ts` asserts the Worker side against that file and
// `scripts/backfill-username-skeletons.test.ts` asserts this side against it,
// so a change to either that is not made to the other turns a suite red.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const DIST = join(HERE, "..", "src/lib/unicode-identifier/dist");

const { analyze_identifier, initSync } = await import(
  join(DIST, "osl_uts39_wasm.js")
);
initSync({ module: new WebAssembly.Module(readFileSync(join(DIST, "osl_uts39_wasm_bg.wasm"))) });

export class UsernameNotAnalyzable extends Error {
  constructor(reason) {
    super(`username is not an acceptable identifier: ${reason}`);
    this.name = "UsernameNotAnalyzable";
  }
}

function analyze(value) {
  const parsed = JSON.parse(analyze_identifier(value));
  if (typeof parsed?.normalized !== "string" || typeof parsed?.skeleton !== "string" ||
      typeof parsed?.identifierAllowed !== "boolean" || typeof parsed?.hasExcessMarks !== "boolean") {
    throw new Error("UTS #39 artifact returned an invalid analysis");
  }
  return parsed;
}

/// Mirror of `usernameSkeleton()` in `src/lib/username.ts`. Read the comments
/// there for why each refusal exists and why the skeleton is folded a second
/// time through the artifact's own normalizer.
export function usernameSkeleton(username) {
  const analysis = analyze(username);
  if (analysis.normalized !== username) {
    throw new UsernameNotAnalyzable("not canonical under UTS #39 normalization");
  }
  if (!analysis.identifierAllowed) {
    throw new UsernameNotAnalyzable("outside the UTS #39 identifier profile");
  }
  if (analysis.hasExcessMarks) {
    throw new UsernameNotAnalyzable("too many consecutive combining marks");
  }
  if (analysis.skeleton.length === 0) {
    throw new UsernameNotAnalyzable("empty skeleton");
  }
  const folded = analyze(analysis.skeleton).normalized;
  if (folded.length === 0) throw new UsernameNotAnalyzable("empty folded skeleton");
  return folded;
}
