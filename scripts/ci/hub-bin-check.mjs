#!/usr/bin/env node
// D-267. The hub-binary check that CANNOT come back green without having
// compiled the hub binary.
//
// THE DEFECT
//
//   apps/osl-hub's [[bin]] carries `required-features = ["desktop"]`. Cargo's
//   answer to `--bins` with that feature absent is not an error: it selects NO
//   binary target, prints
//
//     warning: target filter `bins` specified, but no targets matched; this is a no-op
//
//   and EXITS 0. Measured on this tree with `compile_error!()` injected at
//   src/main.rs:2 -- exit 0, in 1.03 seconds. Any hub-binary verification that
//   omits `--features desktop` is vacuous, and a mutation lane has already had a
//   mutant "pass" that way.
//
//   It compounds. With `apps/osl-hub-ui/dist` absent the UN-MUTATED control also
//   exits 101, because `tauri::generate_context!()` panics at compile time. So
//   neither exit code carried information on its own: 0 could mean "nothing was
//   built" and 101 could mean "the frontend is not built". One lane worked around
//   it by diffing error counts (control 2, mutant 3).
//
// WHY THIS IS A SCRIPT AND NOT A `compile_error!` GUARD IN THE CRATE
//
//   The obvious mechanism -- a second `[[bin]]` with no `required-features`
//   holding `#[cfg(not(feature = "desktop"))] compile_error!(...)`, so that
//   `--bins` always matches something -- was designed, and then REJECTED on
//   evidence. tauri-cli 2.11.1 (the version tauri-action runs for the release)
//   resolves the app binary in `crates/tauri-cli/src/interface/rust.rs`:
//
//     let is_main = file_name == self.cargo_package_settings.name || file_name == default_run;
//     ...
//     binaries.iter().find(|x| x.main())
//       .context("failed to find main binary, make sure you have a
//                 `package > default-run` in the Cargo.toml file")?
//
//   This package is named `osl-hub` and its binary is `osl-privacy-hub`, so
//   nothing matches by name; the release works today only via the single-binary
//   fallback. A second `[[bin]]` DESTROYS that fallback and the release build
//   fails outright unless `default-run` is also added -- and even then, every
//   `[[bin]]` whose required-features are satisfied becomes a `BundleBinary`,
//   so the guard would be copied into the signed installer. Trading a false
//   green in a check for a stray executable inside a privacy product's
//   installer, on a release path that cannot be exercised from this host, is
//   not a trade worth making. `required-features` itself is NOT removed: see
//   apps/osl-hub/Cargo.toml.
//
// WHAT THIS DOES INSTEAD
//
//   It owns feature and target selection (a caller cannot omit `--features
//   desktop`, because a caller cannot pass features at all), and then it does
//   the thing the omission made impossible: it READS CARGO'S OWN RECORD OF WHAT
//   IT COMPILED and refuses to report success unless the hub binary target is
//   in it. A green here means a hub binary was compiled. That is the property
//   `cargo check --bins` could not offer.
//
// USAGE
//
//   node scripts/ci/hub-bin-check.mjs [check|test] [--jobs N] [-- <test args>]
//   node scripts/ci/hub-bin-check.mjs --self-test   # prove the refusals fire
//   node scripts/ci/hub-bin-check.mjs --audit       # find vacuous invocations in the tree
//
// EXIT CODES
//
//   0  the hub binary compiled (and, for `test`, its suite passed)
//   1  the hub binary FAILED -- a real failure, named by cargo
//   2  refused: the caller tried to select features or targets
//   3  VACUOUS: cargo exited 0 having compiled no hub binary. This is D-267.
//   4  refused: the embedded frontend has not been built
//   9  the harness itself is invalid (self-test could not run)
//   other: cargo's own exit code, passed through

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "..", "..");

export const MANIFEST = "apps/osl-hub/Cargo.toml";
export const BIN_NAME = "osl-privacy-hub";
export const REQUIRED_FEATURE = "desktop";
const DIST_INDEX = join(REPO, "apps/osl-hub-ui/dist/index.html");

// A caller that can choose these can choose the vacuous combination, which is
// the entire defect. This entry point chooses them.
const FORBIDDEN_CALLER_FLAGS =
  /^--(features|no-default-features|all-features|manifest-path|package|workspace|bins?|lib|tests?|benches?|bench|examples?|all-targets)(=|$)/;

// ---------------------------------------------------------------------------
// The grader. Pure, so the self-test can starve it.
// ---------------------------------------------------------------------------

/**
 * Did cargo say, in its own machine-readable stream, that it worked on the hub
 * binary target?
 *
 * Both record kinds count. `compiler-artifact` is emitted on success (including
 * for FRESH units, so a warm cache still proves selection). `compiler-message`
 * carries the same `target` block and is emitted when the unit fails to
 * compile -- without it a genuine compile error in main.rs would be
 * misdiagnosed as vacuity.
 */
export function sawTheHubBinary(cargoStdout) {
  for (const line of cargoStdout.split("\n")) {
    const text = line.trim();
    if (!text.startsWith("{")) continue;
    let record;
    try {
      record = JSON.parse(text);
    } catch {
      continue;
    }
    const target = record.target;
    if (!target || target.name !== BIN_NAME) continue;
    if (!Array.isArray(target.kind) || !target.kind.includes("bin")) continue;
    if (record.reason === "compiler-artifact" || record.reason === "compiler-message") return true;
  }
  return false;
}

/**
 * The whole point. `cargoExit === 0 && !seenBin` is D-267 and it is a FAILURE
 * here, not a pass.
 */
export function grade({ cargoExit, seenBin }) {
  if (!seenBin && cargoExit === 0) {
    return {
      exit: 3,
      verdict: "VACUOUS",
      why:
        `cargo exited 0 and compiled NO \`${BIN_NAME}\` binary target. This check proves ` +
        `NOTHING about the hub binary and is refused (D-267). The binary carries ` +
        `required-features = ["${REQUIRED_FEATURE}"]; without that feature cargo silently ` +
        `selects no target and succeeds.`,
    };
  }
  if (!seenBin) {
    return {
      exit: cargoExit,
      verdict: "FAILED BEFORE THE BINARY",
      why:
        `cargo exited ${cargoExit} without ever compiling the \`${BIN_NAME}\` binary target. ` +
        `This failure is NOT a statement about the binary -- read cargo's output above for what ` +
        `actually broke (a build script, a dependency, or feature resolution).`,
    };
  }
  if (cargoExit !== 0) {
    return {
      exit: cargoExit,
      verdict: "FAILED",
      why: `the \`${BIN_NAME}\` binary was compiled and cargo exited ${cargoExit}. The errors above are real.`,
    };
  }
  return { exit: 0, verdict: "OK", why: `the \`${BIN_NAME}\` binary target was compiled.` };
}

// ---------------------------------------------------------------------------
// Running it for real
// ---------------------------------------------------------------------------

function refuseWithoutFrontend() {
  if (existsSync(DIST_INDEX) && statSync(DIST_INDEX).isFile()) return;
  process.stderr.write(
    "hub-bin-check: REFUSED -- the embedded frontend has not been built.\n" +
      "  apps/osl-hub-ui/dist/index.html is missing.\n" +
      `  \`--features ${REQUIRED_FEATURE}\` embeds that directory at COMPILE time ` +
      "(tauri::generate_context!),\n" +
      "  so this check cannot say anything about the hub binary until it exists.\n" +
      "  Run:  (cd apps/osl-hub-ui && npm ci && npm run build)\n" +
      "  Refused here rather than letting cargo compile the whole crate and die as\n" +
      "  `error: proc macro panicked` with exit 101 -- the same exit code a genuine\n" +
      "  compile error in the binary produces. That collision is the second half of D-267.\n",
  );
  return 4;
}

export function cargoArgs(mode, { jobs, passthrough }) {
  const selection =
    mode === "test"
      ? ["test", "--manifest-path", MANIFEST, "--features", REQUIRED_FEATURE, "--bin", BIN_NAME]
      : ["check", "--manifest-path", MANIFEST, "--features", REQUIRED_FEATURE, "--bins"];
  const args = [...selection, "--message-format", "json"];
  if (jobs) args.push("-j", String(jobs));
  if (mode === "test") args.push("--", "--test-threads=1", ...passthrough);
  else if (passthrough.length) args.push(...passthrough);
  return args;
}

function renderDiagnostics(stdout) {
  let warnings = 0;
  const errors = [];
  for (const line of stdout.split("\n")) {
    const text = line.trim();
    if (!text.startsWith("{")) {
      // In `test` mode the test harness's own output ("test result: ok. 116
      // passed; 0 failed") shares this stream with cargo's JSON. Swallowing it
      // left the step with an exit code and no counts, which is most of what a
      // reader needs from a test run.
      if (text) process.stdout.write(`${line}\n`);
      continue;
    }
    let record;
    try {
      record = JSON.parse(text);
    } catch {
      continue;
    }
    if (record.reason !== "compiler-message" || !record.message) continue;
    const level = record.message.level;
    if (level === "error" || level === "error: internal compiler error") {
      if (record.message.rendered) errors.push(record.message.rendered);
    } else if (level === "warning") {
      warnings += 1;
    }
    // cargo also streams the test harness's own stdout for `test`; it is not a
    // compiler-message, so it reaches the terminal through stderr inheritance.
  }
  for (const rendered of errors) process.stderr.write(rendered.endsWith("\n") ? rendered : `${rendered}\n`);
  if (warnings) process.stderr.write(`hub-bin-check: ${warnings} warning(s) suppressed from this report.\n`);
  return { errorCount: errors.length, warningCount: warnings };
}

function run(mode, argv) {
  const refusal = refuseWithoutFrontend();
  if (refusal) return refusal;

  let jobs = null;
  const passthrough = [];
  let afterDashDash = false;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (afterDashDash) {
      passthrough.push(arg);
      continue;
    }
    if (arg === "--") {
      afterDashDash = true;
      continue;
    }
    if (arg === "--jobs" || arg === "-j") {
      jobs = argv[i + 1];
      i += 1;
      continue;
    }
    if (FORBIDDEN_CALLER_FLAGS.test(arg)) {
      process.stderr.write(
        `hub-bin-check: REFUSED -- \`${arg}\` selects features or targets, and this entry point ` +
          `owns both.\n  It exists because a caller that can omit \`--features ${REQUIRED_FEATURE}\` ` +
          `will eventually omit it (D-267).\n  For a wider target sweep use ` +
          `scripts/ci/hub-target-ratchet.mjs; for lints, scripts/ci/hub-lint-ratchet.mjs.\n`,
      );
      return 2;
    }
    process.stderr.write(`hub-bin-check: REFUSED -- unrecognised argument \`${arg}\`.\n`);
    return 2;
  }

  const args = cargoArgs(mode, { jobs, passthrough });
  process.stderr.write(`hub-bin-check: cargo ${args.join(" ")}\n`);
  const result = spawnSync("cargo", args, {
    cwd: REPO,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 512,
    stdio: ["ignore", "pipe", "inherit"],
  });
  if (result.error) {
    process.stderr.write(`hub-bin-check: could not run cargo: ${result.error.message}\n`);
    return 9;
  }
  const stdout = result.stdout ?? "";
  renderDiagnostics(stdout);
  const cargoExit = result.status === null ? 9 : result.status;
  const verdict = grade({ cargoExit, seenBin: sawTheHubBinary(stdout) });
  process.stderr.write(`hub-bin-check: ${verdict.verdict} -- ${verdict.why}\n`);
  return verdict.exit;
}

// ---------------------------------------------------------------------------
// --self-test: prove every refusal fires, including on a REAL starved input
// ---------------------------------------------------------------------------

const ARTIFACT_LINE = JSON.stringify({
  reason: "compiler-artifact",
  target: { kind: ["bin"], name: BIN_NAME },
});
const MESSAGE_LINE = JSON.stringify({
  reason: "compiler-message",
  target: { kind: ["bin"], name: BIN_NAME },
  message: { level: "error", rendered: "error[E0599]: no method named `record_cover`\n" },
});
const OTHER_TARGET_LINE = JSON.stringify({
  reason: "compiler-artifact",
  target: { kind: ["lib"], name: "osl_privacy_hub" },
});

function selfTest() {
  const failures = [];
  const check = (name, actual, expected) => {
    const ok = actual === expected;
    process.stdout.write(`  ${ok ? "OK  " : "FAIL"}  ${name}: got ${actual}, expected ${expected}\n`);
    if (!ok) failures.push(name);
  };

  process.stdout.write("hub-bin-check --self-test\n");
  process.stdout.write("\n(1) REAL STARVATION -- the exact D-267 invocation, run for real:\n");
  process.stdout.write(`      cargo check --manifest-path ${MANIFEST} --bins   (NO --features ${REQUIRED_FEATURE})\n`);
  const starved = spawnSync(
    "cargo",
    ["check", "--manifest-path", MANIFEST, "--bins", "--message-format", "json"],
    { cwd: REPO, encoding: "utf8", maxBuffer: 1024 * 1024 * 128 },
  );
  if (starved.error) {
    process.stdout.write(`  INVALID HARNESS: could not run cargo (${starved.error.message})\n`);
    return 9;
  }
  const starvedExit = starved.status;
  const starvedSaw = sawTheHubBinary(starved.stdout ?? "");
  process.stdout.write(`      cargo exit: ${starvedExit}; hub binary in cargo's record: ${starvedSaw}\n`);
  if (starvedExit !== 0 || starvedSaw) {
    process.stdout.write(
      "  INVALID HARNESS: the starved invocation no longer exits 0 with no binary, so this\n" +
        "  self-test cannot demonstrate the refusal it exists to demonstrate. Investigate\n" +
        "  before trusting any result here.\n",
    );
    return 9;
  }
  check("starved real run is refused as VACUOUS", grade({ cargoExit: starvedExit, seenBin: starvedSaw }).exit, 3);

  process.stdout.write("\n(2) synthetic streams:\n");
  check("bin artifact + exit 0 passes", grade({ cargoExit: 0, seenBin: sawTheHubBinary(ARTIFACT_LINE) }).exit, 0);
  check(
    "bin error message + exit 101 is a REAL failure, not vacuity",
    grade({ cargoExit: 101, seenBin: sawTheHubBinary(MESSAGE_LINE) }).exit,
    101,
  );
  check(
    "only a lib artifact + exit 0 is still VACUOUS",
    grade({ cargoExit: 0, seenBin: sawTheHubBinary(OTHER_TARGET_LINE) }).exit,
    3,
  );
  check("empty stream + exit 101 fails before the binary", grade({ cargoExit: 101, seenBin: false }).exit, 101);

  process.stdout.write("\n(3) the audit finds a planted vacuous invocation:\n");
  const vacuousOnly = (text) => auditText("planted.sh", text).filter((f) => f.kind === "VACUOUS").length;
  check("flags a bare --bins", vacuousOnly(`cargo check --manifest-path ${MANIFEST} --bins\n`), 1);
  check(
    "flags default target selection (no target flag at all)",
    vacuousOnly(`cargo test --manifest-path ${MANIFEST} -- --test-threads=1\n`),
    1,
  );
  check(
    "accepts the same line with --features desktop",
    vacuousOnly(`cargo check --manifest-path ${MANIFEST} --bins --features desktop\n`),
    0,
  );
  check("accepts a lib-only invocation", vacuousOnly(`cargo test --manifest-path ${MANIFEST} --features core --lib\n`), 0);
  check(
    "accepts an explicit integration-test selection",
    vacuousOnly(`cargo test --manifest-path ${MANIFEST} --features core --test '*' -- --test-threads=1\n`),
    0,
  );
  check(
    "does NOT read `-- --test-threads=1` as a target selector",
    vacuousOnly(`cargo test --manifest-path ${MANIFEST} --bins -- --test-threads=1\n`),
    1,
  );
  check("ignores cargo fmt", vacuousOnly(`cargo fmt --manifest-path ${MANIFEST} --all -- --check\n`), 0);
  check(
    "reports a shell-variable target selection as UNDETERMINED, not as a pass",
    auditText("planted.sh", `cargo test --manifest-path ${MANIFEST} "$@" -j 3\n`).filter(
      (f) => f.kind === "UNDETERMINED",
    ).length,
    1,
  );

  process.stdout.write(failures.length ? `\nSELF-TEST FAILED: ${failures.join(", ")}\n` : "\nself-test OK\n");
  return failures.length ? 1 : 0;
}

// ---------------------------------------------------------------------------
// --audit: no vacuous hub-binary invocation may be checked in
// ---------------------------------------------------------------------------

// STATED LIMITS, in the D-268 tradition -- a checker that hides what it cannot
// see is worse than one that says so:
//
//  * This reads TEXT. A command whose target selection arrives through a shell
//    variable is reported as UNDETERMINED, never silently passed.
//  * A cargo invocation is matched inside a window of the 3 lines before and 7
//    after the manifest path, so that argument arrays spread over several lines
//    (scripts/ci/*.mjs) are read whole. A `--features desktop` more than 7 lines
//    away from the manifest path is not seen.
//  * It cannot see an invocation that never spells the manifest path -- one
//    assembled from a variable, or run after a `cd apps/osl-hub`.
const DYNAMIC = /(\$\{?[A-Za-z_@*][^\s]*|\$\()/;

// `--test-threads` is NOT a target selector, and reading it as one is how a
// check like this quietly stops checking. `--test` only counts when a target
// name or glob follows it.
const SELECTS_NON_BIN_TARGETS = /--lib\b|--tests\b|--test\s+\S|--examples?\s|--example\b|--bench(es)?\b|--doc\b/;
const HAS_DESKTOP_FEATURE = /--features(\s|=)[^-]*\bdesktop\b/;
const COMPILING_CARGO_COMMAND = /\bcargo[a-z-]*\s+(check|build|test|clippy|run|rustc)\b/;
// The argument-array form: `['clippy', '--manifest-path', 'apps/osl-hub/...']`.
// `fmt` compiles nothing and selects no targets, so it is not a candidate.
const ARRAY_FORM_VERB = /(^|\s)(check|build|test|clippy|rustc)(\s|$)/;
const ARRAY_FORM_FMT = /(^|\s)fmt(\s|$)/;

/** One implementation, used by both --audit and --self-test. */
export function auditText(path, text) {
  const lines = text.replace(/\\\n\s*/g, " ").split("\n");
  const findings = [];
  for (let i = 0; i < lines.length; i += 1) {
    if (!lines[i].includes(MANIFEST)) continue;
    // The manifest path must appear as an ARGUMENT, on the same line as the
    // flag that introduces it or as part of the command itself. Otherwise
    // `app_manifest = "apps/osl-hub/Cargo.toml"` -- a binding in a contract
    // test, three lines above an unrelated command tuple -- reads as an
    // invocation, and a checker with a standing false positive is a checker
    // people learn to override.
    if (!lines[i].includes("--manifest-path") && !COMPILING_CARGO_COMMAND.test(lines[i])) continue;
    const window = lines
      .slice(Math.max(0, i - 3), i + 8)
      .join(" ")
      .replace(/['"`,]/g, " ")
      .replace(/\s+/g, " ");
    const isCommand =
      COMPILING_CARGO_COMMAND.test(window) ||
      (window.includes("--manifest-path") && ARRAY_FORM_VERB.test(window) && !ARRAY_FORM_FMT.test(window));
    if (!isCommand) continue;
    if (SELECTS_NON_BIN_TARGETS.test(window)) continue;
    if (HAS_DESKTOP_FEATURE.test(window)) continue;
    findings.push({
      path,
      line: i + 1,
      text: lines[i].trim().slice(0, 140),
      // DYNAMIC is tested against the COMMAND, not the window. Tested against
      // the window it downgraded a planted `cargo check --bins` to
      // UNDETERMINED because an unrelated `$(...)` sat four lines below it --
      // a checker that quietly reclassifies real findings as unreadable is the
      // same false green in a new costume. Caught by mutation (d).
      kind: DYNAMIC.test(lines[i]) ? "UNDETERMINED" : "VACUOUS",
    });
  }
  return findings;
}

function auditTree() {
  const listed = spawnSync("git", ["ls-files"], { cwd: REPO, encoding: "utf8", maxBuffer: 1024 * 1024 * 64 });
  if (listed.status !== 0) {
    process.stderr.write("hub-bin-check --audit: `git ls-files` failed; cannot audit an unknown tree.\n");
    return 9;
  }
  const vacuous = [];
  const undetermined = [];
  let scanned = 0;
  for (const path of listed.stdout.split("\n")) {
    if (!path.trim()) continue;
    if (path === "scripts/ci/hub-bin-check.mjs") continue; // this file quotes the defect on purpose
    let text;
    try {
      text = readFileSync(join(REPO, path), "utf8");
    } catch {
      continue;
    }
    if (!text.includes(MANIFEST)) continue;
    scanned += 1;
    for (const finding of auditText(path, text)) {
      (finding.kind === "VACUOUS" ? vacuous : undetermined).push(finding);
    }
  }
  process.stdout.write(`hub-bin-check --audit: ${scanned} tracked file(s) mention ${MANIFEST}\n`);

  if (undetermined.length) {
    process.stdout.write(
      `\nUNDETERMINED (${undetermined.length}) -- target selection comes from a shell variable, so no\n` +
        "static reading of the text can grade it. Listed, not passed:\n",
    );
    for (const f of undetermined) process.stdout.write(`  ${f.path}:${f.line}  ${f.text}\n`);
  }
  if (vacuous.length) {
    process.stdout.write(
      `\nVACUOUS (${vacuous.length}) -- selects binary targets of ${MANIFEST} without ` +
        `--features ${REQUIRED_FEATURE},\nso the hub binary is silently skipped and the check proves nothing (D-267):\n`,
    );
    for (const f of vacuous) process.stdout.write(`  ${f.path}:${f.line}  ${f.text}\n`);
    process.stdout.write("\nFix: route it through scripts/ci/hub-bin-check.mjs, or add --features desktop.\n");
    return 1;
  }
  process.stdout.write(`\nno vacuous hub-binary invocation in the tree (${undetermined.length} undetermined).\n`);
  return 0;
}

// ---------------------------------------------------------------------------

function main(argv) {
  if (argv.includes("--self-test")) return selfTest();
  if (argv.includes("--audit")) return auditTree();
  const mode = argv[0] === "test" ? "test" : "check";
  const rest = argv[0] === "test" || argv[0] === "check" ? argv.slice(1) : argv;
  return run(mode, rest);
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  process.exit(main(process.argv.slice(2)));
}
