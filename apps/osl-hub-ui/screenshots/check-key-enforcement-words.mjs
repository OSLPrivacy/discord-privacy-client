#!/usr/bin/env node
/**
 * TASK 5000 - the KEY enforcement words check.
 *
 * KEY means cryptographic, unbypassable. PRODUCT.txt fixes the exact copy:
 *
 *   KEY  cryptographic, unbypassable.
 *        "Not a rule. They do not have the key."
 *
 * This refuses a shipped role permission screen where any KEY-tagged row does
 * not show that exact sentence, where the number of KEY rows showing it is not
 * the number of KEY rows in TASK 4851's catalogue, or where a single RELAY or
 * TRUST row shows it. The sentence is spelled out here as well as in the
 * screen module, and the catalogue's KEY rows are read straight out of the
 * fixture file (TASK 4851's `osl-permission-catalogue print` output, byte for
 * byte), so a reworded promise or a retagged row is a failing check rather
 * than a quietly consistent screen.
 *
 * Two ways to run it, both ending in the same check:
 *
 *   node screenshots/check-key-enforcement-words.mjs
 *       renders the real screen from source (through Vite, so the module is
 *       the one the app ships) and checks what it drew.
 *
 *   node screenshots/check-key-enforcement-words.mjs <dump-file>
 *       checks a dump captured out of the live DOM by
 *       capture-role-permission-screen.mjs.
 *
 * Exit 0 prints the counts. Exit 1 names every problem it found. Swapping the
 * sentence on one row - in the source, or in a captured screen - makes it
 * exit 1.
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const CATALOGUE_PATH = path.join(APP_ROOT, "src", "fixtures", "permission-catalogue.txt");
/** PRODUCT.txt's exact copy for the KEY class. */
const KEY_SENTENCE = "Not a rule. They do not have the key.";
const ROLE_STATE = {
  roleName: "Moderator",
  allowed: [
    "read a text channel",
    "read channel history",
    "send a message",
    "attach pictures",
    "join voice",
    "timeout a member",
    "request clients hide a delivered message",
  ],
};

/** TASK 4851's catalogue, parsed here: the words of every row tagged `KEY`. */
function catalogueKeyRows(text) {
  const rows = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line.startsWith("- ")) continue;
    const match = /^(.*) `([A-Z]+)`$/u.exec(line.slice(2).trim());
    if (match && match[2] === "KEY") rows.push(match[1].trim());
  }
  return rows;
}

async function loadScreenModule() {
  const vite = await createServer({
    root: APP_ROOT,
    server: { middlewareMode: true },
    appType: "custom",
    logLevel: "error",
  });
  try {
    return {
      module: await vite.ssrLoadModule("/src/role-permission-rows.ts"),
      close: () => vite.close(),
    };
  } catch (error) {
    await vite.close();
    throw error;
  }
}

async function main() {
  const dumpPath = process.argv[2];
  const { module, close } = await loadScreenModule();
  try {
    const fixtureKeyRows = catalogueKeyRows(readFileSync(CATALOGUE_PATH, "utf8"));
    const dump = dumpPath
      ? readFileSync(dumpPath, "utf8")
      : module.rolePermissionScreenDump(ROLE_STATE);
    const report = module.checkKeyEnforcementWords(dump);

    const problems = [];
    if (module.ENFORCEMENT_SENTENCES.KEY !== KEY_SENTENCE) {
      problems.push(`the KEY sentence must read "${KEY_SENTENCE}", the module carries "${module.ENFORCEMENT_SENTENCES.KEY ?? ""}"`);
    }
    if (report.catalogueKeyRows !== fixtureKeyRows.length) {
      problems.push(`the screen's catalogue holds ${report.catalogueKeyRows} KEY rows, TASK 4851's catalogue holds ${fixtureKeyRows.length}`);
    }
    if (report.keyRowsShowing !== fixtureKeyRows.length) {
      problems.push(`expected ${fixtureKeyRows.length} KEY rows showing "${KEY_SENTENCE}", found ${report.keyRowsShowing}`);
    }
    if (report.nonKeyRowsShowingKey !== 0) {
      problems.push(`${report.nonKeyRowsShowingKey} RELAY or TRUST rows show the KEY words`);
    }
    if (problems.length > 0) {
      throw new Error(`KEY enforcement words check failed: ${problems.join("; ")}`);
    }

    console.log(`TASK5000 screen_source=${dumpPath ? path.resolve(dumpPath) : "rendered from /src/role-permission-rows.ts"}`);
    console.log(`TASK5000 catalogue_source=${CATALOGUE_PATH}`);
    console.log(`TASK5000 role=${report.roleName}`);
    console.log(`TASK5000 permission_rows=${report.rows}`);
    console.log(`TASK5000 catalogue_key_rows=${fixtureKeyRows.length}`);
    console.log(`TASK5000 key_rows_showing_key_words=${report.keyRowsShowing}`);
    console.log(`TASK5000 relay_or_trust_rows_showing_key_words=${report.nonKeyRowsShowingKey}`);
    console.log(`TASK5000 key_words=${KEY_SENTENCE}`);
  } finally {
    await close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
