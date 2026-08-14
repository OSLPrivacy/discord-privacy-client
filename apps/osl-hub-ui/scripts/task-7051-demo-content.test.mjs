import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { auditStructuralInventories } from "./task-7050-structural-inventory.mjs";
import { classifyPageText, comparePageText } from "./task-7051-demo-content.mjs";

const page = "Conversation.dc.html";
const route = "inbox/conversation";
const controls = [{ name: "send", label: "Send reply", destination: "inbox/send" }];
const source = `
  <main><h1>Conversation</h1><p>Your messages are protected on this device.</p>
  <button>Send reply</button><section role="list" aria-label="Message history">
    <article role="listitem"><img alt="Avery avatar"><strong>Avery</strong><p>Meet at the station.</p><time>09:41</time><span>briefing.pdf</span></article>
    <article role="listitem"><img alt="Bryn avatar"><strong>Bryn</strong><p>I will bring the map.</p><time>09:43</time><span>route.png</span></article>
    <article role="listitem"><img alt="Cato avatar"><strong>Cato</strong><p>See you there.</p><time>09:44</time><span>notes.txt</span></article>
  </section><p>{{ syntheticSlot }}</p></main>`;
const swapped = source
  .replaceAll("Avery", "Dara").replaceAll("Bryn", "Eli").replaceAll("Cato", "Fenn")
  .replace("Meet at the station.", "Bring three blue lanterns.")
  .replace("I will bring the map.", "The ferry leaves at dawn.")
  .replace("See you there.", "Use the north entrance.")
  .replace("briefing.pdf", "observatory.zip").replace("route.png", "signal.jpg").replace("notes.txt", "ledger.csv")
  .replace("09:41", "18:02").replace("09:43", "18:07").replace("09:44", "18:12");

function fixture(designMarkup, buildMarkup) {
  return {
    routes: [route],
    manifest: [{ kind: "routed", page, route }],
    designInventories: { [page]: controls },
    buildInventories: { [route]: controls },
    designPages: { [page]: designMarkup },
    buildPages: { [route]: buildMarkup },
  };
}

test("TASK 7051 derives demo content from semantic rows, never names", () => {
  const inventory = classifyPageText(source);
  assert.deepEqual(inventory.shipping.map(({ kind, text }) => [kind, text]), [
    ["heading", "Conversation"],
    ["fixed sentence", "Your messages are protected on this device."],
    ["control label", "Send reply"],
  ]);
  assert.equal(inventory.demoLists.length, 1);
  assert.equal(inventory.demoLists[0].name, "Message history");
  assert.equal(inventory.demoLists[0].rows, 3);
  assert.equal(inventory.demo.filter((entry) => entry.kind === "repeated-row avatar").length, 3);
  assert.equal(inventory.demo.some((entry) => entry.kind === "slot value"), true);
  assert.equal(inventory.demo.every((entry) => !("text" in entry)), true);
  assert.equal(comparePageText({ page, route, designMarkup: source, buildMarkup: swapped }).ok, true);
  assert.equal(auditStructuralInventories(fixture(source, swapped)).ok, true);
});

test("TASK 7051 keeps control, consent, warning, and error wording shipping inside a demo row", () => {
  const markup = `<section role="list" aria-label="Entries"><article role="listitem"><p>synthetic body</p><button>Remove entry</button><div role="alert">Cannot remove this entry.</div><div data-consent>Delete after confirmation.</div></article></section>`;
  const copy = classifyPageText(markup).shipping.map(({ kind, text }) => [kind, text]);
  assert.deepEqual(copy, [
    ["control label", "Remove entry"],
    ["warning or error wording", "Cannot remove this entry."],
    ["consent wording", "Delete after confirmation."],
  ]);
  assert.equal(comparePageText({ page, route, designMarkup: markup, buildMarkup: markup.replace("Remove entry", "Erase entry") }).ok, false);
});

test("TASK 7051 classifies standalone timestamps, filler counts, avatars, files, and slots by field placement", () => {
  const markup = `<main><time>09:41</time><span data-demo-count>3 members</span><span data-avatar>new portrait</span><span data-sample-file>sample.pdf</span><span>{{ itemValue }}</span></main>`;
  const inventory = classifyPageText(markup);
  assert.deepEqual(inventory.shipping, []);
  assert.deepEqual(inventory.demo.map((entry) => entry.kind).sort(), ["avatar field", "count field", "sample-file field", "slot value", "timestamp"]);
  assert.equal(comparePageText({ page, route, designMarkup: markup, buildMarkup: markup.replace("09:41", "18:02").replace("3 members", "9 members").replace("new portrait", "other portrait").replace("sample.pdf", "other.zip") }).ok, true);
});

test("TASK 7051 makes shipping changes red by their exact text", () => {
  for (const [before, after] of [
    ["Conversation", "Discussion"],
    ["Send reply", "Transmit reply"],
    ["Your messages are protected on this device.", "Your messages are stored elsewhere."],
  ]) {
    const result = auditStructuralInventories(fixture(source, source.replace(before, after)));
    assert.equal(result.ok, false);
    assert.match(result.findings.join("\n"), new RegExp(before.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  }
});

test("TASK 7051 compares demo row shape and count, not demo words", () => {
  const twoRows = swapped.replace(/\s*<article role="listitem"><img alt="Fenn avatar">[\s\S]*?<\/article>/u, "");
  const result = auditStructuralInventories(fixture(source, twoRows));
  assert.equal(result.ok, false);
  assert.match(result.findings.join("\n"), /demo list "Message history" has different row count — design has 3 rows and build route "inbox\/conversation" has 2\./u);
});

test("TASK 7051 command keeps a throwaway demo-text swap green and makes wording/count mutations red", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7051-"));
  try {
    const originalPage = path.join(directory, "Conversation.dc.html");
    const demoCopy = path.join(directory, "Conversation-demo-copy.dc.html");
    await writeFile(originalPage, source);
    await writeFile(demoCopy, swapped);
    const script = new URL("./task-7050-structural-inventory.mjs", import.meta.url).pathname;
    const run = async (name, designMarkup, buildMarkup) => {
      const fixturePath = path.join(directory, `${name}.json`);
      await writeFile(fixturePath, JSON.stringify(fixture(designMarkup, buildMarkup)));
      return spawnSync(process.execPath, [script, "--fixture", fixturePath], { encoding: "utf8" });
    };
    const original = await run("original", await readFile(originalPage, "utf8"), source);
    const demoChanged = await run("demo-changed", await readFile(demoCopy, "utf8"), source);
    assert.equal(original.status, 0);
    assert.equal(demoChanged.status, 0);
    for (const [before, after, name] of [
      ["Conversation", "Discussion", "heading"],
      ["Send reply", "Transmit reply", "control"],
      ["Your messages are protected on this device.", "Your messages are stored elsewhere.", "sentence"],
    ]) {
      const result = await run(name, source.replace(before, after), source);
      assert.equal(result.status, 1);
      assert.match(result.stderr, new RegExp(before.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
      assert.match(result.stderr, new RegExp(after.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    }
    const twoRows = swapped.replace(/\s*<article role="listitem"><img alt="Fenn avatar">[\s\S]*?<\/article>/u, "");
    const count = await run("two-rows", source, twoRows);
    assert.equal(count.status, 1);
    assert.match(count.stderr, /demo list "Message history" has different row count — design has 3 rows and build route "inbox\/conversation" has 2\./u);
    console.log(`TASK7051_THROWAWAY original_exit=${original.status} demo_swap_exit=${demoChanged.status} heading_exit=1 control_exit=1 sentence_exit=1 count_exit=${count.status} list="Message history" rows=3/2`);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7051 source has no hand-written sample name list", async () => {
  const classifier = await readFile(new URL("./task-7051-demo-content.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(classifier, /Mara|Kit|Frontier|Avery|Bryn|Cato|Dara|Eli|Fenn/u);
});
