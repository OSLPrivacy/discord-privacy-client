#!/usr/bin/env node
// TASK 6841 — prove the TASK 6840 text-attachment rename is complete and
// compatible.
//
// The proof is a red proof: for every resource surface the rename touched, put
// the old state back in a *throwaway copy* of the gate tree and require the
// TASK 6840 check to exit 1 naming that surface. Then require a restored copy
// to pass. Every copy is discarded.
//
// Three mutation families, matching the three ways the rename could be
// incomplete or incompatible:
//
//   stale-label     the retired PLAINTEXT name is back on a surface
//   disclosure      the "fully encrypted" guidance is gone from a surface
//   compatibility   the previous encrypted wire type is rejected
//
// plus locale bundles, which have no shipping instance today but are a resource
// surface the scan must police: if a bundle appears carrying the old label, or
// carrying the new label without the guidance, the check must go red naming it.
//
// A mutation that does not actually change the tree is a starved mutant and
// fails this proof, as does a missing restoration or a drifted gate.
//
// Usage: node prove-6841.mjs [--all-stages] [--only=<id prefix>] [--starve=<kind>]
// Env:   CARGO_TARGET_DIR (required), OSL_6841_GATE (default: the pinned SHA)
//
// `--starve` deliberately removes one leg of the proof so the proof's own
// guards can be shown to bite: `mutant` (an anchor that no longer matches),
// `locale` (drop the locale surfaces from the mutant set), `restoration` (never
// prove the restored tree passes), `gate` (run against a tree that is not the
// pinned TASK 6840 gate). Every one of them must exit 1.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const scratch = join(repoRoot, ".tmp-6841");
const gate = join(scratch, "gate");
const copies = join(scratch, "copies");
const allStages = process.argv.includes("--all-stages");
const onlyArg = (process.argv.find((a) => a.startsWith("--only=")) ?? "").slice(7);
const starve = (process.argv.find((a) => a.startsWith("--starve=")) ?? "").slice(9);

// The TASK 6840 gate commit (lane/q). Pinned by blob so a moved or rewritten
// gate fails loudly instead of silently proving nothing.
const GATE_COMMIT = process.env.OSL_6841_GATE ?? "65dc0f6ca110eed20524b9525fd6a2793a0be9d6";
const GATE_BLOBS = {
  "apps/osl-hub-ui/overlay.html": "54389a73e290c7e44dc50774ddbe8bd46db31af7",
  "apps/osl-hub-ui/src/main.ts": "c7c837b0fa11d8dc80aa26de85129d724f32ffb7",
  "apps/osl-hub-ui/README.md": "57feae88d9727df9d595736bc95b41427c9d13bd",
  "apps/osl-hub-ui/src/task-6840-text-attachment.test.ts": "872ce2b9ba2d5d9850177a7918fcb376e37eb1b4",
  "crates/ipc/src/attachment_wire.rs": "e3bdf6a49f334bba88f83ea577a1f1294fa54abd",
};

const OVERLAY = "apps/osl-hub-ui/overlay.html";
const MAIN = "apps/osl-hub-ui/src/main.ts";
const HELP = "apps/osl-hub-ui/README.md";
const WIRE = "crates/ipc/src/attachment_wire.rs";

const sh = (cmd, args, opts = {}) =>
  spawnSync(cmd, args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...opts });

const sha256 = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");

// Copies are hard-linked from the gate for speed, so a mutation must break the
// link before writing or it would corrupt every other copy.
const writeCopy = (abs, text) => {
  mkdirSync(dirname(abs), { recursive: true });
  if (existsSync(abs)) unlinkSync(abs);
  writeFileSync(abs, text);
};

// --- mutation primitives ----------------------------------------------------

/** Replace `from` with `to` in `file`; the anchor must occur exactly `times`. */
const replaceOnce = (file, from, to, times = 1) => ({
  file,
  apply: (root) => {
    const abs = join(root, file);
    const before = readFileSync(abs, "utf8");
    const seen = before.split(from).length - 1;
    if (seen !== times) {
      return { ok: false, why: `anchor ${JSON.stringify(from.slice(0, 60))} occurs ${seen}x, expected ${times}x` };
    }
    const after = before.split(from).join(to);
    if (after === before) return { ok: false, why: "mutation was a no-op" };
    writeCopy(abs, after);
    return { ok: true, delta: before.length - after.length };
  },
});

/** Create a brand-new file (used for the locale bundles). */
const createFile = (file, text) => ({
  file,
  apply: (root) => {
    const abs = join(root, file);
    if (existsSync(abs)) return { ok: false, why: "file already exists in the gate tree" };
    writeCopy(abs, text);
    return { ok: true, delta: -text.length };
  },
});

// --- the mutants ------------------------------------------------------------

const A11Y = "TEXT ATTACHMENT — fully encrypted like every OSL attachment";

const MUTANTS = [
  // ---- stale-label: restore PLAINTEXT in each resource surface -------------
  {
    id: "m01-overlay-visible-label",
    family: "stale-label",
    surface: OVERLAY,
    stage: "ui",
    mutation: replaceOnce(OVERLAY, ">TEXT ATTACHMENT</button>", ">PLAINTEXT ATTACHMENT</button>"),
  },
  {
    id: "m02-overlay-aria-label",
    family: "stale-label",
    surface: OVERLAY,
    stage: "ui",
    mutation: replaceOnce(OVERLAY, 'aria-label="TEXT ATTACHMENT', 'aria-label="PLAINTEXT ATTACHMENT'),
  },
  {
    id: "m03-overlay-tooltip",
    family: "stale-label",
    surface: OVERLAY,
    stage: "ui",
    mutation: replaceOnce(OVERLAY, 'title="TEXT ATTACHMENT', 'title="PLAINTEXT ATTACHMENT'),
  },
  {
    id: "m04-chat-visible-label",
    family: "stale-label",
    surface: MAIN,
    stage: "ui",
    mutation: replaceOnce(MAIN, ">TEXT ATTACHMENT</button>", ">PLAINTEXT ATTACHMENT</button>"),
  },
  {
    id: "m05-chat-aria-label",
    family: "stale-label",
    surface: MAIN,
    stage: "ui",
    mutation: replaceOnce(MAIN, 'aria-label="TEXT ATTACHMENT', 'aria-label="PLAINTEXT ATTACHMENT'),
  },
  {
    id: "m06-chat-tooltip",
    family: "stale-label",
    surface: MAIN,
    stage: "ui",
    mutation: replaceOnce(MAIN, 'title="TEXT ATTACHMENT', 'title="PLAINTEXT ATTACHMENT'),
  },
  {
    id: "m07-help-name",
    family: "stale-label",
    surface: HELP,
    stage: "ui",
    mutation: replaceOnce(HELP, "**TEXT ATTACHMENT**", "**PLAINTEXT ATTACHMENT**"),
  },

  // ---- disclosure: remove the encrypted guidance from each surface ---------
  {
    id: "m08-overlay-guidance",
    family: "disclosure",
    surface: OVERLAY,
    stage: "ui",
    mutation: replaceOnce(
      OVERLAY,
      '<span id="text-attachment-encryption-guidance" class="sr-only">Fully encrypted like every OSL attachment.</span>\n            ',
      "",
    ),
  },
  {
    id: "m09-chat-guidance",
    family: "disclosure",
    surface: MAIN,
    stage: "ui",
    mutation: replaceOnce(MAIN, "<small>Fully encrypted like every OSL attachment.</small>", ""),
  },
  {
    id: "m10-help-guidance",
    family: "disclosure",
    surface: HELP,
    stage: "ui",
    mutation: replaceOnce(
      HELP,
      "It is fully\nencrypted like every OSL attachment; the name",
      "The name",
    ),
  },
  {
    id: "m11-help-wire-disclosure",
    family: "disclosure",
    surface: HELP,
    stage: "ui",
    mutation: replaceOnce(
      HELP,
      " the name describes the file format, not\nits encryption or wire representation.",
      "",
    ),
  },

  // ---- locale: a bundle must not starve the scan --------------------------
  {
    id: "m12-locale-stale-label",
    family: "locale",
    surface: "apps/osl-hub-ui/src/locales/es-ES.json",
    stage: "ui",
    mutation: createFile(
      "apps/osl-hub-ui/src/locales/es-ES.json",
      `${JSON.stringify(
        {
          "composer.attachment.label": "PLAINTEXT ATTACHMENT",
          "composer.attachment.hint": "Adjuntar un archivo de texto.",
        },
        null,
        2,
      )}\n`,
    ),
  },
  {
    id: "m13-locale-missing-guidance",
    family: "locale",
    surface: "apps/osl-hub-ui/src/locales/fr-FR.json",
    stage: "ui",
    mutation: createFile(
      "apps/osl-hub-ui/src/locales/fr-FR.json",
      `${JSON.stringify(
        {
          "composer.attachment.label": "TEXT ATTACHMENT",
          "composer.attachment.hint": "Joindre un fichier texte.",
        },
        null,
        2,
      )}\n`,
    ),
  },

  // ---- compatibility: reject the previous encrypted wire type -------------
  {
    id: "m14-reject-v1-wire",
    family: "compatibility",
    surface: WIRE,
    stage: "wire",
    mutation: replaceOnce(
      WIRE,
      ") -> Result<(Vec<u8>, String), AttachmentWireError> {\n    let magic_off = find_payload_offset(file_bytes).ok_or(AttachmentWireError::MagicNotFound)?;",
      ") -> Result<(Vec<u8>, String), AttachmentWireError> {\n" +
        "    // TASK 6841 mutant: drop support for the previous encrypted wire type.\n" +
        "    if !file_bytes\n" +
        "        .windows(OSL_ATT_MAGIC_V2.len())\n" +
        "        .any(|w| w == OSL_ATT_MAGIC_V2)\n" +
        "    {\n" +
        "        return Err(AttachmentWireError::MagicNotFound);\n" +
        "    }\n" +
        "    let magic_off = find_payload_offset(file_bytes).ok_or(AttachmentWireError::MagicNotFound)?;",
    ),
  },
];

// --- copy management --------------------------------------------------------

const freshCopy = (id) => {
  const dest = join(copies, id);
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(copies, { recursive: true });
  const cp = sh("cp", ["-al", gate, dest]);
  if (cp.status !== 0) throw new Error(`cp -al failed for ${id}: ${cp.stderr}`);
  // vitest resolves from the copy; point it at the installed tree.
  const nm = join(dest, "apps", "osl-hub-ui", "node_modules");
  if (!existsSync(nm)) symlinkSync(join(repoRoot, "apps", "osl-hub-ui", "node_modules"), nm, "dir");
  return dest;
};

const runCheck = (root, stage) => {
  const r = sh(process.execPath, [join(here, "run-6840.mjs"), root, `--stage=${stage}`]);
  return { code: r.status === null ? 1 : r.status, text: `${r.stdout ?? ""}${r.stderr ?? ""}` };
};

// --- 0. coverage, then gate integrity ---------------------------------------

const log = [];
const emit = (line) => {
  log.push(line);
  console.log(line);
};

let hardFail = false;

// Every family and every resource surface the rename touched has to be
// mutated. Dropping one is a starved proof, not a smaller proof.
const REQUIRED_FAMILIES = ["stale-label", "disclosure", "locale", "compatibility"];
const REQUIRED_SURFACES = [OVERLAY, MAIN, HELP, WIRE];
const active = starve === "locale" ? MUTANTS.filter((m) => m.family !== "locale") : MUTANTS;

for (const family of REQUIRED_FAMILIES) {
  const n = active.filter((m) => m.family === family).length;
  emit(`TASK6841_COVERAGE family=${family} mutants=${n}`);
  if (n === 0) {
    emit(`TASK6841_FAIL reason=starved-family family=${family} detail=no mutation covers this family`);
    hardFail = true;
  }
}
for (const surface of REQUIRED_SURFACES) {
  const n = active.filter((m) => m.surface === surface).length;
  emit(`TASK6841_COVERAGE surface=${surface} mutants=${n}`);
  if (n === 0) {
    emit(`TASK6841_FAIL reason=starved-surface surface=${surface} detail=no mutation covers this surface`);
    hardFail = true;
  }
}
if (hardFail) {
  emit("TASK6841 verdict=FAIL");
  process.exit(1);
}

// The gate tree is itself a throwaway copy: materialise it from the pinned
// TASK 6840 commit, never from this lane's working tree.
if (!existsSync(gate)) {
  mkdirSync(gate, { recursive: true });
  const tar = sh("bash", [
    "-c",
    `git archive ${GATE_COMMIT} | tar -x -C ${JSON.stringify(gate)}`,
  ], { cwd: repoRoot });
  if (tar.status !== 0) {
    console.log(`TASK6841_FAIL reason=gate-unavailable detail=${tar.stderr?.trim()}`);
    process.exit(1);
  }
}

emit(`TASK6841_GATE commit=${GATE_COMMIT.slice(0, 9)}`);
for (const [path, blob] of Object.entries(GATE_BLOBS)) {
  const fromGit = sh("git", ["rev-parse", `${GATE_COMMIT}:${path}`], { cwd: repoRoot }).stdout?.trim();
  const abs = join(gate, path);
  const onDisk = existsSync(abs)
    ? sh("git", ["hash-object", abs], { cwd: repoRoot }).stdout?.trim()
    : "<absent>";
  const ok = fromGit === blob && onDisk === blob;
  emit(`TASK6841_GATE_BLOB path=${path} expected=${blob.slice(0, 12)} commit=${(fromGit ?? "?").slice(0, 12)} copy=${onDisk.slice(0, 12)} ok=${ok}`);
  if (!ok) hardFail = true;
}
if (hardFail) {
  emit("TASK6841_FAIL reason=gate-drift detail=the TASK 6840 gate tree is not the pinned one");
  process.exit(1);
}

// --- 1. mutants -------------------------------------------------------------

const selected = active.filter((m) => (onlyArg ? m.id.startsWith(onlyArg) : true));
const results = [];
for (const m of selected) {
  const root = freshCopy(m.id);
  // `--starve=mutant` points the first mutation at an anchor the gate does not
  // contain, so the file is never actually changed.
  const mutation =
    starve === "mutant" && m === selected[0]
      ? replaceOnce(m.mutation.file, "ANCHOR THAT IS NOT IN THE GATE TREE", "x")
      : m.mutation;
  const applied = mutation.apply(root);
  if (!applied.ok) {
    emit(`TASK6841_FAIL reason=starved-mutant mutant=${m.id} surface=${m.surface} detail=${applied.why}`);
    rmSync(root, { recursive: true, force: true });
    hardFail = true;
    continue;
  }
  // The mutated file must differ from the gate byte-for-byte.
  const changed = !existsSync(join(gate, mutation.file))
    ? true
    : sha256(join(root, mutation.file)) !== sha256(join(gate, mutation.file));
  if (!changed) {
    emit(`TASK6841_FAIL reason=starved-mutant mutant=${m.id} surface=${m.surface} detail=file identical to gate after mutation`);
    rmSync(root, { recursive: true, force: true });
    hardFail = true;
    continue;
  }

  const stage = allStages && m.stage === "ui" ? "all" : m.stage;
  const { code, text } = runCheck(root, stage);
  const namesSurface = text.includes(`surface=${m.surface}`);
  const namesFamily =
    m.family === "compatibility"
      ? text.includes("class=compatibility")
      : m.family === "locale"
        ? text.includes("class=locale")
        : text.includes(`class=${m.family}`);
  const red = code === 1;
  const pass = red && namesSurface && namesFamily;
  const detail = (text.match(/^TASK6841_RED .*$/gmu) ?? []).slice(0, 3).join(" | ") || "<no red line>";

  emit(
    `TASK6841_MUTANT id=${m.id} family=${m.family} surface=${m.surface} stage=${stage} ` +
      `exit=${code} names_surface=${namesSurface} names_class=${namesFamily} verdict=${pass ? "RED-OK" : "PROOF-FAIL"}`,
  );
  emit(`    ${detail}`);
  results.push({ ...m, code, pass, detail });
  if (!pass) hardFail = true;
  rmSync(root, { recursive: true, force: true });
}

// --- 2. restoration ---------------------------------------------------------

if (starve === "restoration") {
  emit("TASK6841_FAIL reason=starved-restoration detail=the restored tree was never proven to pass");
  emit("TASK6841 verdict=FAIL");
  process.exit(1);
}

const restored = freshCopy("restored");
let restorationOk = true;
for (const [path, blob] of Object.entries(GATE_BLOBS)) {
  const onDisk = sh("git", ["hash-object", join(restored, path)], { cwd: repoRoot }).stdout?.trim();
  const ok = onDisk === blob;
  emit(`TASK6841_RESTORED path=${path} blob=${onDisk?.slice(0, 12)} matches_gate=${ok}`);
  if (!ok) restorationOk = false;
}
for (const stray of ["apps/osl-hub-ui/src/locales/es-ES.json", "apps/osl-hub-ui/src/locales/fr-FR.json"]) {
  const present = existsSync(join(restored, stray));
  emit(`TASK6841_RESTORED stray=${stray} present=${present}`);
  if (present) restorationOk = false;
}
if (!restorationOk) {
  emit("TASK6841_FAIL reason=starved-restoration detail=restored copy is not the gate tree");
  hardFail = true;
}

const green = runCheck(restored, "all");
const scanLine = (green.text.match(/^TASK6840_SCAN .*$/mu) ?? ["<none>"])[0];
const vitestLine = (green.text.match(/^TASK6841_VITEST .*$/mu) ?? ["<none>"])[0];
const wireLine = (green.text.match(/^TASK6841_WIRE .*$/mu) ?? ["<none>"])[0];
const legacyLine = (green.text.match(/TASK6840_LEGACY_TEXT_OPEN [^\n]*/u) ?? ["<none>"])[0];
emit(`TASK6841_RESTORED_RUN exit=${green.code}`);
emit(`    ${scanLine}`);
emit(`    ${vitestLine}`);
emit(`    ${wireLine}`);
emit(`    ${legacyLine}`);
if (green.code !== 0) hardFail = true;
rmSync(restored, { recursive: true, force: true });

// --- 3. verdict -------------------------------------------------------------

const byFamily = {};
for (const r of results) {
  byFamily[r.family] ??= { total: 0, red: 0 };
  byFamily[r.family].total += 1;
  if (r.pass) byFamily[r.family].red += 1;
}
const surfaces = new Set(selected.map((m) => m.surface));
emit(
  `TASK6841_SUMMARY mutants=${selected.length} red_ok=${results.filter((r) => r.pass).length} ` +
    `surfaces=${surfaces.size} ` +
    Object.entries(byFamily).map(([k, v]) => `${k}=${v.red}/${v.total}`).join(" ") +
    ` restored_exit=${green.code}`,
);

// Copies are gone; prove it.
const leftovers = existsSync(copies) ? sh("ls", ["-A", copies]).stdout.trim() : "";
emit(`TASK6841_COPIES_DISCARDED leftovers=${leftovers === "" ? 0 : leftovers.split("\n").length}`);
if (leftovers !== "") hardFail = true;

if (selected.length === 0 || results.length !== selected.length) {
  emit("TASK6841_FAIL reason=starved-mutant-set detail=not every mutant ran");
  hardFail = true;
}

// Discard every copy, the gate tree included.
if (!process.argv.includes("--keep-gate")) {
  rmSync(gate, { recursive: true, force: true });
  emit(`TASK6841_GATE_DISCARDED present=${existsSync(gate)}`);
}

emit(`TASK6841 verdict=${hardFail ? "FAIL" : "PROVEN"}`);
process.exit(hardFail ? 1 : 0);
