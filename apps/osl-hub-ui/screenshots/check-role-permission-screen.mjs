#!/usr/bin/env node
/**
 * TASK 4852 - the role permission screen check.
 *
 * It refuses a role screen that shows a permission row without its enforcement
 * tag, or without the honest sentence for that class:
 *
 *   KEY   Not a rule. They do not have the key.
 *   RELAY OSL's relay refuses it. It still cannot read what you write.
 *   TRUST A modified app could ignore this. Everyone else's app will still hide it.
 *
 * Two ways to run it, both ending in the same check:
 *
 *   node screenshots/check-role-permission-screen.mjs
 *       renders the real screen from source (through Vite, so the module is the
 *       one the app ships) and checks what it drew.
 *
 *   node screenshots/check-role-permission-screen.mjs <dump-file>
 *       checks a dump captured out of the live DOM by
 *       capture-role-permission-screen.mjs.
 *
 * Exit 0 prints the counts. Exit 1 names every problem it found. Deleting any
 * one of the three sentences - from the source, or from a captured screen -
 * makes it exit 1.
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
/**
 * The three sentences the screen owes the reader, spelled out here as well as
 * in the screen module, so a reworded promise is a failing check rather than a
 * quietly consistent screen.
 */
const REQUIRED_SENTENCES = {
  KEY: "Not a rule. They do not have the key.",
  RELAY: "OSL's relay refuses it. It still cannot read what you write.",
  TRUST: "A modified app could ignore this. Everyone else's app will still hide it.",
};
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
    const dump = dumpPath
      ? readFileSync(dumpPath, "utf8")
      : module.rolePermissionScreenDump(ROLE_STATE);
    const report = module.checkRolePermissionScreenDump(dump);
    const sentences = module.ENFORCEMENT_SENTENCES;

    const reworded = Object.entries(REQUIRED_SENTENCES)
      .filter(([tag, sentence]) => sentences[tag] !== sentence || !dump.includes(sentence))
      .map(([tag, sentence]) => `the ${tag} sentence must read "${sentence}", the screen shows "${sentences[tag] ?? ""}"`);
    if (reworded.length > 0) {
      throw new Error(`role permission screen check failed: ${reworded.join("; ")}`);
    }

    console.log(`TASK4852 screen_source=${dumpPath ? path.resolve(dumpPath) : "rendered from /src/role-permission-rows.ts"}`);
    console.log(`TASK4852 role=${report.roleName}`);
    console.log(`TASK4852 permission_rows=${report.rows}`);
    console.log(`TASK4852 enforcement_tags=${report.tags}`);
    console.log(`TASK4852 row_sentences=${report.sentences}`);
    console.log(`TASK4852 key_rows=${report.sentenceCounts.KEY}`);
    console.log(`TASK4852 relay_rows=${report.sentenceCounts.RELAY}`);
    console.log(`TASK4852 trust_rows=${report.sentenceCounts.TRUST}`);
    console.log(`TASK4852 legend_sentences=${report.legendSentences}`);
    console.log(`TASK4852 key_sentence=${sentences.KEY}`);
    console.log(`TASK4852 relay_sentence=${sentences.RELAY}`);
    console.log(`TASK4852 trust_sentence=${sentences.TRUST}`);
  } finally {
    await close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
