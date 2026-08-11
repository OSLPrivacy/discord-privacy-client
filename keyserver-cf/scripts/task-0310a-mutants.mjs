import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const project = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = mkdtempSync(join(tmpdir(), "task-0310a-mutants-"));
const endpoint = "src/endpoints/private-contact-links.ts";
const files = [
  endpoint,
  "src/task-0310a-worker.ts",
  "src/env.ts",
  "src/lib/http.ts",
  "test/integration/task-0310a-private-contact-links.test.ts",
  "test/apply-migrations.ts",
  "migrations/0050_private_contact_links.sql",
  "vitest.task0310a.config.ts",
  "wrangler.task0310a.toml",
  "package.json",
  "tsconfig.json",
];

function copyProject(destination) {
  for (const relative of files) {
    const target = join(destination, relative);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(project, relative), target);
  }
  for (const relative of [
    "apps/osl-hub-ui/src/onboarding-identity.ts",
    "apps/osl-hub-ui/src/adapters.ts",
  ]) {
    const target = join(dirname(destination), relative);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(join(project, "..", relative), target);
  }
  symlinkSync(join(project, "node_modules"), join(destination, "node_modules"), "dir");
}

const mutations = [
  {
    name: "expiry",
    from: "        AND terminal_state <> 'revoked'\n        AND expires_at_unix_seconds > ?`,\n  ).bind(now, digest, now).run();",
    to: "        AND terminal_state <> 'revoked'\n        /* TASK0310A mutation: expiry condition removed */`,\n  ).bind(now, digest).run();",
    expected: /TASK0310A expiry leaked link OSLCL2\.[A-Za-z0-9_-]{43} remained usable/u,
  },
  {
    name: "revocation",
    from: "        AND terminal_state <> 'revoked'\n",
    to: "        /* TASK0310A mutation: authoritative revocation check removed */\n",
    expected: /TASK0310A revocation leaked link OSLCL2\.[A-Za-z0-9_-]{43} remained usable/u,
  },
];

try {
  for (const mutation of mutations) {
    const destination = join(root, mutation.name);
    mkdirSync(destination, { recursive: true });
    copyProject(destination);
    const sourcePath = join(destination, endpoint);
    const source = readFileSync(sourcePath, "utf8");
    const occurrences = source.split(mutation.from).length - 1;
    if (occurrences !== 1) throw new Error(`${mutation.name}: expected one mutation site, found ${occurrences}`);
    writeFileSync(sourcePath, source.replace(mutation.from, mutation.to));
    const result = spawnSync(
      join(destination, "node_modules/.bin/vitest"),
      ["run", "--config", "vitest.task0310a.config.ts", "--reporter=verbose", "--silent=false"],
      { cwd: destination, encoding: "utf8", env: { ...process.env, NO_COLOR: "1" } },
    );
    const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
    process.stdout.write(output);
    if (result.status !== 1 || !mutation.expected.test(output)) {
      throw new Error(`${mutation.name}: expected unchanged journey exit 1 naming leaked link; exit=${result.status}`);
    }
    console.log(`TASK0310A_MUTATION name=${mutation.name} exit=1 leaked_link_named=true throwaway=${destination}`);
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log("TASK0310A_MUTATIONS expiry=red revocation=red restored_source=unchanged throwaways_removed=true");
