import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const packageDirectory = resolve(scriptDirectory, "..");
const vitest = join(packageDirectory, "node_modules", ".bin", "vitest");
const testTarget = "src/task-5017-person-page-cut.test.ts";

const attacks = {
  "gap-row": {
    expected: /TASK5017 missing-record: expected exactly one 48h-person-page-posts-stories gap entry, found 0/u,
    mutate(copy) {
      const path = join(copy, "public", "shipped-gap-list.json");
      const document = JSON.parse(readFileSync(path, "utf8"));
      document.gaps = document.gaps.filter((gap) => gap.id !== "48h-person-page-posts-stories");
      writeFileSync(path, `${JSON.stringify(document, null, 2)}\n`);
    },
  },
  "registered-grey-posts": {
    expected: /TASK5017 leaked-surface: dynamically reached person-page posts\/story-ring control at route\/people: <button disabled aria-disabled="true" data-person-posts type="button">Their posts<\/button>/u,
    mutate(copy) {
      mutatePeopleRoute(copy,
        '<article class="person-card" aria-label="Shipping profile"><button disabled aria-disabled="true" ${[100,97,116,97,45,112,101,114,115,111,110,45,112,111,115,116,115].map((code) => String.fromCharCode(code)).join("")} type="button">Their posts</button></article>');
    },
  },
  "dynamic-story-ring": {
    expected: /TASK5017 leaked-surface: dynamically reached person-page posts\/story-ring control at route\/people: <button data-story-ring type="button">Stories<\/button>/u,
    mutate(copy) {
      // Keep the forbidden surface token out of source and packaged text. The
      // registered People route constructs the otherwise-unregistered control
      // only when its runtime tree is rendered, proving the third inventory
      // is independently effective.
      mutatePeopleRoute(copy,
        '<button ${[100,97,116,97,45,115,116,111,114,121,45,114,105,110,103].map((code) => String.fromCharCode(code)).join("")} type="button">Stories</button>');
    },
  },
};

function mutatePeopleRoute(copy, control) {
  const path = join(copy, "src", "main.ts");
  const source = readFileSync(path, "utf8");
  const anchor = "${peopleRows}</div></section></main>`;";
  const replacement = `\${peopleRows}</div></section>${control}</main>\`;`;
  if (!source.includes(anchor)) {
    throw new Error("TASK5017B mutation-anchor: registered People route markup not found");
  }
  writeFileSync(path, source.replace(anchor, replacement));
}

function makeThrowaway(name) {
  const made = spawnSync("mktemp", ["-d", `/tmp/osl-task-5017b-${name}.XXXXXX`], { encoding: "utf8" });
  if (made.status !== 0) throw new Error(`TASK5017B mktemp failed for ${name}: ${made.stderr.trim()}`);
  const root = made.stdout.trim();
  const copy = join(root, "apps", "osl-hub-ui");
  mkdirSync(join(root, "apps"), { recursive: true });
  cpSync(packageDirectory, copy, {
    recursive: true,
    filter: (path) => ![join(packageDirectory, "node_modules"), join(packageDirectory, "dist")].includes(path),
  });
  mkdirSync(join(root, "apps", "osl-hub", "icons"), { recursive: true });
  cpSync(resolve(packageDirectory, "../osl-hub/icons/icon-cyan.png"), join(root, "apps", "osl-hub", "icons", "icon-cyan.png"));
  symlinkSync(join(packageDirectory, "node_modules"), join(copy, "node_modules"), "dir");
  return { copy, root };
}

function runAttack(name, attack) {
  const { copy, root } = makeThrowaway(name);
  try {
    attack.mutate(copy);
    const proof = spawnSync(vitest, ["run", testTarget, "--maxWorkers=1", "--no-file-parallelism"], {
      cwd: copy,
      encoding: "utf8",
      env: { ...process.env, NO_COLOR: "1" },
    });
    const combined = `${proof.stdout}\n${proof.stderr}`;
    console.log(`TASK5017B attack=${name} test_exit=${proof.status}`);
    if (proof.status !== 1) throw new Error(`TASK5017B wrong-exit:${name}: expected 1, got ${proof.status}`);
    const namedFailure = combined.match(attack.expected);
    if (!namedFailure) {
      process.stdout.write(proof.stdout);
      process.stderr.write(proof.stderr);
      throw new Error(`TASK5017B unnamed-failure:${name}`);
    }
    console.log(`TASK5017B named=${namedFailure[0]}`);
    console.log(`TASK5017B caught=${name}`);
  } finally {
    rmSync(root, { recursive: true, force: true });
    console.log(`TASK5017B removed=${root}`);
  }
}

const omitIndex = process.argv.indexOf("--omit");
const omitted = omitIndex < 0 ? null : process.argv[omitIndex + 1];
if (omitIndex >= 0 && (!omitted || !(omitted in attacks))) {
  console.error(`TASK5017B invalid-omit:${omitted ?? "missing"}; choices=${Object.keys(attacks).join(",")}`);
  process.exit(2);
}

const caught = new Set();
for (const [name, attack] of Object.entries(attacks)) {
  if (name === omitted) continue;
  runAttack(name, attack);
  caught.add(name);
}

for (const name of Object.keys(attacks)) {
  if (!caught.has(name)) {
    console.error(`TASK5017B absent-attack:${name}`);
    process.exit(1);
  }
}
console.log(`TASK5017B attacks=${caught.size} copies_removed=${caught.size} result=pass`);
