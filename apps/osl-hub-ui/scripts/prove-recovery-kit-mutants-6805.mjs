#!/usr/bin/env node
// TASK 6805 — prove a recovery-kit *upload* cannot pass as a *label*.
//
// TASK 6804 built the loader and shipped a check for it. A check nobody has
// ever seen fail is decoration, so this file breaks the loader on purpose, one
// defect at a time, and requires 6804 to go red every time.
//
// How a mutant is run, and why it is a throwaway copy:
//
//   * Before a mutant is applied the target file's exact bytes are copied into
//     a throwaway directory (`<tmp>/osl-6805-<id>-XXXX/pristine/`), and the
//     mutated bytes are written next to them (`.../mutant/`). Only then is the
//     mutant written into the worktree.
//   * 6804's own check runs, unmodified, against the mutated tree.
//   * The worktree file is restored from the throwaway pristine copy, its
//     SHA-256 is compared against the hash taken before anything moved, and the
//     throwaway directory is deleted. Every copy is discarded; nothing a mutant
//     touched survives this script, including when it throws.
//
// A mutant is only counted when the check exits 1. A mutant the check survives
// is reported by route and by the SHA-256 of the file that carried it, and
// fails this proof — that is the whole point of the exercise.
//
// Starvation (OSL6805_STARVE=<name>) removes one ingredient of the proof and
// must turn it red, because a proof that survives its own ingredients being
// taken away is not measuring them:
//
//   mutant           — drop a whole route from the matrix
//   negative-file    — take 6804's corrupt-kit negative away from the matrix
//   source-identity  — take the kit's identity binding away
//   windows-picker   — take 6804's picker wiring assertions away
//   restoration      — do not put a mutant back before the final green run
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(scriptDir, "..");
const repoRoot = resolve(uiRoot, "..", "..");
const CHECK_6804 = join(scriptDir, "check-recovery-kit-upload-6804.mjs");
const starve = process.env.OSL6805_STARVE ?? "";

/** Every route the "do" line of TASK 6805 names. Coverage is checked before a
 * single mutant runs, so a starved matrix cannot pass by running fewer. */
const REQUIRED_ROUTES = ["page", "picker", "byte", "validation", "import", "leak"];

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const rel = (path) => path.slice(repoRoot.length + 1);

const HUB = (name) => join(repoRoot, "apps", "osl-hub", "src", name);
const UI = (name) => join(uiRoot, "src", name);

/**
 * One defect. `find` must appear exactly once in the pristine file — a mutant
 * that silently matched nothing (or matched twice) would be a mutant that never
 * ran, so `apply` refuses.
 */
const MUTANTS = [
  // ---- "populate only the mock label" -----------------------------------
  {
    id: "page/mock-label-only",
    caughtBy: "source-wiring",
    route: "page",
    file: UI("main.ts"),
    defect: "the chip writes a 'Recovery kit loaded' label and never loads a kit",
    find: "          const loaded = await loadHubRecoveryKitFile(page);",
    replace: '          const loaded = null; if (status) status.textContent = "Recovery kit loaded";',
  },
  {
    id: "page/no-word-boxes",
    caughtBy: "source-wiring",
    route: "page",
    file: UI("account-recovery.ts"),
    defect: "the Forgot Password page keeps the chip but draws no numbered boxes",
    find: [
      "  const wordBoxes = Array.from({ length: 12 }, (_, index) => {",
      "    const position = index + 1;",
      "    return `<label class=\"recovery-word-box\" for=\"forgot-recovery-word-${position}\"><span>${position}</span>"
        + "<input id=\"forgot-recovery-word-${position}\" data-recovery-kit-word=\"forgot-password\" type=\"password\""
        + " autocomplete=\"off\" autocapitalize=\"none\" spellcheck=\"false\" /></label>`;",
      "  }).join(\"\");",
    ].join("\n"),
    replace: '  const wordBoxes = "";',
  },
  // ---- "detach each picker" --------------------------------------------
  {
    id: "picker/detached-dialog",
    caughtBy: "source-wiring",
    route: "picker",
    file: HUB("recovery_kit_picker.rs"),
    defect: "the installed Windows dialog is never opened; the path comes from the environment",
    find: [
      "        let selected = self",
      "            .app",
      "            .dialog()",
      "            .file()",
      "            .set_parent(&parent)",
      '            .set_title("Choose an OSL recovery kit")',
      '            .add_filter("OSL recovery kit", &[RECOVERY_KIT_EXTENSION])',
      "            .blocking_pick_file();",
      "        selected",
      "            .map(|file| file.into_path().map_err(|_| RecoveryKitRefusal::Unreadable))",
      "            .transpose()",
    ].join("\n"),
    replace: [
      "        let _ = (&parent, RECOVERY_KIT_EXTENSION, RecoveryKitRefusal::Unreadable);",
      "        // Detached: no dialog is opened and nothing the owner chose is read.",
      '        Ok(std::env::var("OSL_RECOVERY_KIT_PATH").ok().map(std::path::PathBuf::from))',
    ].join("\n"),
  },
  {
    id: "picker/renderer-names-the-file",
    caughtBy: "source-wiring",
    route: "picker",
    file: join(repoRoot, "apps", "osl-hub", "src", "main.rs"),
    defect: "the command takes a renderer-supplied path instead of reaching the picker",
    edits: [
      {
        find: "    page: String,\n) -> Result<Option<HubLoadedRecoveryKit>, String> {",
        replace: "    page: String,\n    path: String,\n) -> Result<Option<HubLoadedRecoveryKit>, String> {",
      },
      {
        find: "        let picker = DesktopRecoveryKitPicker::new(app);\n"
          + "        load_recovery_kit_through_picker(page, &picker, expected_account_id.as_deref())",
        replace: "        let _ = app;\n"
          + "        let selection = FrozenKitSelection::freeze(std::path::Path::new(&path))?;\n"
          + "        load_frozen_recovery_kit(page, &selection, expected_account_id.as_deref()).map(Some)",
      },
    ],
  },
  // ---- "ignore selected bytes" -----------------------------------------
  {
    id: "byte/second-read",
    caughtBy: "source-wiring",
    route: "byte",
    file: HUB("recovery_kit_file.rs"),
    defect: "validation re-opens the path instead of using the frozen buffer",
    find: "    let bytes = selection.bytes();\n    let validated_sha256 = sha256_hex(bytes);",
    replace:
      "    let reread = std::fs::read(selection.path()).map_err(|_| RecoveryKitRefusal::Unreadable)?;\n"
      + "    let bytes: &[u8] = &reread;\n    let validated_sha256 = sha256_hex(bytes);",
  },
  {
    id: "byte/hash-not-of-selection",
    caughtBy: "behaviour",
    route: "byte",
    file: HUB("recovery_kit_file.rs"),
    defect: "the reported validated hash is not the hash of the bytes that were read",
    find: "    let validated_sha256 = sha256_hex(bytes);",
    replace: "    let validated_sha256 = sha256_hex(b\"\");",
  },
  // ---- "accept a corrupt/wrong kit" -------------------------------------
  {
    id: "validation/accepts-corrupt",
    caughtBy: "behaviour",
    route: "validation",
    file: HUB("recovery_kit_file.rs"),
    defect: "a flipped byte is accepted because the digest is no longer compared",
    find:
      "    if !constant_time_eq(sha256_hex(body.as_bytes()).as_bytes(), declared_digest.as_bytes()) {\n"
      + "        return Err(RecoveryKitRefusal::Damaged);\n    }",
    replace:
      "    let _ = (sha256_hex(body.as_bytes()), declared_digest.as_bytes());",
  },
  {
    id: "validation/accepts-wrong-identity",
    caughtBy: "behaviour",
    route: "validation",
    file: HUB("recovery_kit_file.rs"),
    defect: "another account's kit is accepted because the identity is no longer compared",
    find:
      "    if derived_account_id(contents.phrase_for(RecoveryKitPage::RestoreAccount))? != contents.user_id()\n"
      + "    {\n        return Err(RecoveryKitRefusal::WrongIdentity);\n    }\n"
      + "    if let Some(expected) = expected_account_id {\n"
      + "        if expected != contents.user_id() {\n"
      + "            return Err(RecoveryKitRefusal::WrongIdentity);\n        }\n    }",
    replace: "    let _ = expected_account_id;",
  },
  // ---- "route both pages to one wrong importer" -------------------------
  {
    id: "import/one-phrase-for-both-pages",
    caughtBy: "behaviour",
    route: "import",
    file: HUB("recovery_kit_file.rs"),
    defect: "both pages are handed the identity phrase, so Forgot Password runs the wrong journey",
    find: "            RecoveryKitPage::ForgotPassword => self.password_phrase.as_str(),",
    replace: "            RecoveryKitPage::ForgotPassword => self.identity_phrase.as_str(),",
  },
  {
    id: "import/chip-imports-directly",
    caughtBy: "source-wiring",
    route: "import",
    file: UI("main.ts"),
    defect: "the chip calls the importer itself instead of filling the page's ordinary form",
    find: "          const phrase = document.querySelector<HTMLTextAreaElement>(phraseField(page));\n          if (phrase) phrase.value = loaded.words.join(\" \");",
    replace: "          await importHubOslIdentityPhrase(loaded.words.join(\" \"));",
  },
  // ---- "log one recovery word" ------------------------------------------
  {
    id: "leak/one-word-in-debug",
    caughtBy: "behaviour",
    route: "leak",
    file: HUB("recovery_kit_file.rs"),
    defect: "a Debug rendering prints the first recovery word",
    find: '            .field("words", &format_args!("[{} words withheld]", self.words.len()))',
    replace: '            .field("words", &self.words.first().map(|word| word.as_str().to_owned()))',
  },
];

// ---------------------------------------------------------------------------
// Throwaway copies. Nothing below leaves a mutated byte in the worktree.
// ---------------------------------------------------------------------------

const pristineHashes = new Map();
for (const mutant of MUTANTS) {
  if (!pristineHashes.has(mutant.file)) {
    pristineHashes.set(mutant.file, sha256(readFileSync(mutant.file)));
  }
}
/** Files currently carrying a mutant, restored by `restoreEverything`. */
const outstanding = new Map();

function applyMutant(mutant) {
  const original = readFileSync(mutant.file);
  let text = original.toString("utf8");
  for (const edit of mutant.edits ?? [{ find: mutant.find, replace: mutant.replace }]) {
    const hits = text.split(edit.find).length - 1;
    assert.equal(hits, 1, `${mutant.id}: its target text must appear exactly once in ${rel(mutant.file)} (found ${hits})`);
    text = text.replace(edit.find, edit.replace);
  }
  const mutated = Buffer.from(text, "utf8");
  assert.notEqual(sha256(mutated), sha256(original), `${mutant.id}: the mutant must change ${rel(mutant.file)}`);
  const throwaway = mkdtempSync(join(tmpdir(), `osl-6805-${mutant.id.replace(/\W/gu, "-")}-`));
  mkdirSync(join(throwaway, "pristine"));
  mkdirSync(join(throwaway, "mutant"));
  copyFileSync(mutant.file, join(throwaway, "pristine", basename(mutant.file)));
  writeFileSync(join(throwaway, "mutant", basename(mutant.file)), mutated);
  writeFileSync(mutant.file, mutated);
  outstanding.set(mutant.file, throwaway);
  return {
    throwaway,
    pristineSha256: sha256(original),
    mutantSha256: sha256(mutated),
  };
}

function discardCopy(mutant, copy, { restore = true } = {}) {
  // Without `restore` the copy is deliberately left in place (the `restoration`
  // starvation); the final `restoreEverything` still puts it back and discards
  // the directory, so no run can end with a mutated worktree.
  if (!restore) return;
  copyFileSync(join(copy.throwaway, "pristine", basename(mutant.file)), mutant.file);
  const back = sha256(readFileSync(mutant.file));
  assert.equal(back, copy.pristineSha256, `${mutant.id}: ${rel(mutant.file)} must be restored byte for byte`);
  outstanding.delete(mutant.file);
  rmSync(copy.throwaway, { recursive: true, force: true });
}

function restoreEverything() {
  for (const [file, throwaway] of outstanding) {
    try {
      copyFileSync(join(throwaway, "pristine", basename(file)), file);
    } finally {
      rmSync(throwaway, { recursive: true, force: true });
    }
  }
  outstanding.clear();
}

// ---------------------------------------------------------------------------
// Running 6804
// ---------------------------------------------------------------------------

/** Throwaway copies of 6804's own check, used only by the starvation modes.
 * They live beside the original so its relative paths still resolve. */
const starvedChecks = [];
function starvedCheckCopy(name, edit) {
  const path = join(scriptDir, `.6805-throwaway-${name}.mjs`);
  const text = readFileSync(CHECK_6804, "utf8");
  const edited = edit(text);
  assert.notEqual(edited, text, `starved copy ${name} must actually remove something`);
  writeFileSync(path, edited);
  starvedChecks.push(path);
  return path;
}

/** `OSL6805_DRYRUN=1` applies and discards every mutant without running 6804.
 * It proves the matrix still matches the tree (each mutant's target text is
 * present exactly once) in a second rather than in half an hour. It proves
 * nothing about the loader, so it never reports a verdict. */
const dryRun = process.env.OSL6805_DRYRUN === "1";

/** Does the mutated tree still build? Used only to tell a panicking test from a
 * build error; a mutant that merely fails to compile has proved nothing. */
function compiles() {
  const built = spawnSync(
    "cargo",
    ["test", "--no-default-features", "--features", "core", "--test", "task_6804_recovery_kit_upload", "--no-run"],
    { cwd: join(repoRoot, "apps", "osl-hub"), encoding: "utf8", timeout: 1_800_000 },
  );
  record(`TASK6805_COMPILE_PROBE exit=${built.status}`);
  return built.status === 0;
}

function run6804(checkPath = CHECK_6804, extraEnv = {}) {
  if (dryRun) return { status: null, output: "", reason: "dry-run", passed: false };
  const result = spawnSync(process.execPath, [checkPath], {
    cwd: repoRoot,
    encoding: "utf8",
    env: { ...process.env, ...extraEnv },
    timeout: 1_800_000,
  });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  const reason = (output.split("\n").find((line) => /TASK6804|AssertionError|expected|Error/u.test(line)) ?? "").trim().slice(0, 160);
  // Which guard did the work. A mutant that only breaks the *compile* proves
  // nothing about a guard, so the matrix says up front which catcher it expects
  // and a mismatch fails this proof.
  //
  // 6804's check captures cargo's own output rather than passing it through, so
  // "did not complete" alone cannot tell a panicking test from a build error.
  // That case is settled by building the target again on its own.
  const catcher = /error\[E\d+\]|could not compile|error: expected/u.test(output)
    ? "compile"
    : /at sourceWiringCheck/u.test(output)
      ? "source-wiring"
      : result.status === 0
        ? "nothing"
        : /did not complete/u.test(output) && !compiles()
          ? "compile"
          : "behaviour";
  return { status: result.status, output, reason, catcher, passed: result.status === 0 && output.includes("TASK6804_CHECK=PASS") };
}

const receipt = { baseline: null, restored: null, mutants: [], survived: [] };

function record(line) {
  console.log(line);
}

function proofFailure(message) {
  record(`TASK6805_PROOF=FAIL ${message}`);
  return 1;
}

// ---------------------------------------------------------------------------
// The proof
// ---------------------------------------------------------------------------

let exitCode = 0;
try {
  // 0. Matrix coverage, before anything expensive. Starving a mutant is caught
  //    here rather than by a smaller matrix quietly passing.
  const matrix = starve === "mutant" ? MUTANTS.filter((mutant) => mutant.route !== "leak") : MUTANTS;
  const covered = [...new Set(matrix.map((mutant) => mutant.route))];
  const missing = REQUIRED_ROUTES.filter((route) => !covered.includes(route));
  if (missing.length > 0) {
    record(`TASK6805_STARVED ingredient=mutant missing_routes=${missing.join(",")}`);
    throw new Error(`the mutation matrix is missing ${missing.join(",")}`);
  }
  record(`TASK6805_MATRIX routes=${covered.join(",")} mutants=${matrix.length}`);

  // The starvation modes each run the smallest subset that shows the loss.
  const only = {
    "": null,
    mutant: null,
    "negative-file": ["validation/accepts-corrupt"],
    "source-identity": [],
    "windows-picker": ["picker/detached-dialog"],
    restoration: ["page/no-word-boxes"],
  }[starve] ?? null;
  // `OSL6805_ONLY=<id,id>` re-judges named mutants without repeating the whole
  // matrix. It reports a subset and never a verdict, so it cannot be mistaken
  // for the proof.
  const subset = (process.env.OSL6805_ONLY ?? "").split(",").filter(Boolean);
  const selected = subset.length > 0
    ? matrix.filter((mutant) => subset.includes(mutant.id))
    : only === null ? matrix : matrix.filter((mutant) => only.includes(mutant.id));
  assert.equal(selected.length, subset.length > 0 ? subset.length : selected.length, "OSL6805_ONLY must name mutants that exist");

  // The check 6804 is run with. Two starvation modes hand the mutants a
  // deliberately weakened copy of it.
  let checkPath = CHECK_6804;
  let checkEnv = {};
  if (starve === "negative-file") {
    checkPath = starvedCheckCopy("no-corrupt-negative", (text) =>
      text
        .replace(/  const corrupt = Buffer\.from\(source\);\n  corrupt\[corrupt\.length - 2\] \^= 1;\n  writeFileSync\(join\(fixtureRoot, "kit-corrupt\.oslkit"\), corrupt\);\n/u, "")
        .replace(/\["corrupt-byte", "damaged"\], /u, ""));
  }
  if (starve === "windows-picker") {
    checkPath = starvedCheckCopy("no-picker-wiring", (text) =>
      text
        .replace(/  assert\.match\(picker, \/blocking_pick_file\\\(\\\)\/u\);\n/u, "")
        .replace(/  assert\.match\(picker, \/add_filter[^\n]*\n/u, ""));
  }
  if (starve === "source-identity") {
    // 6804's own switch for removing the kit's identity binding.
    checkEnv = { OSL6804_STARVE: "identity-comparison" };
  }

  // 1. The pristine build must pass before a mutant means anything.
  if (!dryRun && subset.length === 0 && starve !== "negative-file" && starve !== "windows-picker" && starve !== "restoration") {
    const baseline = run6804(checkPath, checkEnv);
    receipt.baseline = baseline;
    record(`TASK6805_BASELINE exit=${baseline.status} verdict=${baseline.passed ? "PASS" : "FAIL"} reason=${JSON.stringify(baseline.reason)}`);
    if (!baseline.passed) {
      exitCode = proofFailure(`the pristine build must import both valid kits before any mutant is judged (starve=${starve || "none"})`);
    }
  }

  // 2. Every mutant, each in its own throwaway copy.
  for (const mutant of exitCode === 0 ? selected : []) {
    const copy = applyMutant(mutant);
    let result;
    try {
      // A mutant that is meant to be caught by *behaviour* is built first, on
      // its own clock. 6804's check gives each phase five minutes, and a Rust
      // mutant's rebuild can eat that by itself -- a red on the stopwatch is not
      // a red from a guard. Building here also settles "does this even compile".
      const built = mutant.caughtBy === "behaviour" && !dryRun ? compiles() : true;
      result = built
        ? run6804(checkPath, checkEnv)
        : { status: 1, output: "", reason: "the mutant does not build", catcher: "compile", passed: false };
    } finally {
      // `restoration` starvation deliberately leaves the last mutant in place.
      discardCopy(mutant, copy, { restore: starve !== "restoration" });
    }
    const caught = dryRun || (result.status === 1 && !result.passed);
    const rightGuard = dryRun || result.catcher === mutant.caughtBy;
    const red = caught && rightGuard;
    const verdict = red ? "RED" : caught ? "MISCAUGHT" : "SURVIVED";
    record(
      `TASK6805_MUTANT route=${mutant.route} id=${mutant.id} file=${rel(mutant.file)}`
      + ` mutant_sha256=${copy.mutantSha256} pristine_sha256=${copy.pristineSha256}`
      + ` exit=${result.status} caught_by=${dryRun ? "dry-run" : result.catcher} expected_catcher=${mutant.caughtBy}`
      + ` verdict=${verdict} reason=${JSON.stringify(result.reason)}`,
    );
    receipt.mutants.push({ ...mutant, ...copy, exit: result.status, red, catcher: result.catcher });
    if (!red) receipt.survived.push(`${mutant.route}:${mutant.id}:${copy.mutantSha256}:${verdict}:caught_by=${dryRun ? "dry-run" : result.catcher}`);
  }

  if (exitCode === 0 && receipt.survived.length > 0) {
    exitCode = proofFailure(`mutants survived: ${receipt.survived.join(" ")}`);
  }

  // 3. The restored build must import both valid kits again.
  if (!dryRun && subset.length === 0 && (exitCode === 0 || starve === "restoration")) {
    const restored = run6804(CHECK_6804);
    receipt.restored = restored;
    record(`TASK6805_RESTORED exit=${restored.status} verdict=${restored.passed ? "PASS" : "FAIL"} reason=${JSON.stringify(restored.reason)}`);
    if (!restored.passed) {
      exitCode = proofFailure(`the restored build must import both valid kits (starve=${starve || "none"})`);
    }
  }

  // 4. Nothing may still carry a mutant, whatever happened above.
  const drifted = [...pristineHashes]
    .filter(([file, hash]) => sha256(readFileSync(file)) !== hash)
    .map(([file]) => rel(file));
  if (drifted.length > 0 && starve !== "restoration") {
    exitCode = proofFailure(`files left mutated: ${drifted.join(",")}`);
  }

  if (dryRun) {
    record(`TASK6805_DRYRUN=OK mutants=${receipt.mutants.length} (no verdict: 6804 was not run)`);
  } else if (subset.length > 0) {
    record(`TASK6805_SUBSET=${exitCode === 0 ? "RED" : "NOT-RED"} mutants=${receipt.mutants.length} (no verdict: this is not the whole matrix)`);
  } else if (exitCode === 0) {
    record(
      `TASK6805_PROOF=PASS routes=${covered.length} mutants=${receipt.mutants.length}`
      + ` red=${receipt.mutants.filter((mutant) => mutant.red).length} survived=0`
      + ` baseline=${receipt.baseline?.passed ? "PASS" : "SKIPPED"} restored=${receipt.restored?.passed ? "PASS" : "SKIPPED"}`,
    );
  }
} catch (error) {
  record(`TASK6805_PROOF=FAIL ${error instanceof Error ? error.message : String(error)}`);
  exitCode = 1;
} finally {
  restoreEverything();
  for (const path of starvedChecks) rmSync(path, { force: true });
  const drifted = [...pristineHashes].filter(([file, hash]) => sha256(readFileSync(file)) !== hash).map(([file]) => rel(file));
  if (drifted.length > 0) {
    record(`TASK6805_PROOF=FAIL files could not be restored: ${drifted.join(",")}`);
    exitCode = 1;
  }
}
process.exit(exitCode);
