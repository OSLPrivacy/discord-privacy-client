import { cp, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const vitest = path.join(packageRoot, "node_modules", ".bin", "vitest");
const focusedTests = [
  "test/integration/scheme1-prekey-owner-proof.test.ts",
  "test/integration/scheme1-contract-vectors.test.ts",
];

function replaceGuard(source, begin, end, replacement) {
  const start = source.indexOf(begin);
  const finish = source.indexOf(end);
  if (start < 0 || finish < 0 || finish <= start) {
    throw new Error(`mutation guard not found: ${begin}`);
  }
  return source.slice(0, start + begin.length) +
    `\n${replacement}\n  ` +
    source.slice(finish);
}

const mutants = [
  {
    name: "legacy-classifier",
    file: "src/endpoints/register.ts",
    begin: "// SCHEME1_LEGACY_CLASSIFIER_MUTATION_BEGIN",
    end: "// SCHEME1_LEGACY_CLASSIFIER_MUTATION_END",
    expectedFailure:
      "keeps legacy registration tagless and refuses stripped or ambiguous scheme-1 registration",
    replacement: `if (body.identity_scheme === 1) {
    return await handleCanonicalIdentityRegister(body, env);
  }`,
  },
  {
    name: "permissive-owner-proof",
    file: "src/lib/prekey-owner-proof.ts",
    begin: "// SCHEME1_OWNER_SIGNATURE_MUTATION_BEGIN",
    end: "// SCHEME1_OWNER_SIGNATURE_MUTATION_END",
    expectedFailure:
      "rejects a canonical-width but invalid owner-proof signature",
    replacement: `if (false) {
      throw new Error("OPK owner proof signature is invalid");
    }`,
  },
];

let failed = false;
for (const mutant of mutants) {
  const temporary = await mkdtemp(
    path.join(tmpdir(), `osl-scheme1-${mutant.name}-`),
  );
  const clone = path.join(temporary, "keyserver-cf");
  try {
    await cp(packageRoot, clone, {
      recursive: true,
      filter(source) {
        const relative = path.relative(packageRoot, source);
        const top = relative.split(path.sep)[0];
        return ![
          "node_modules",
          ".wrangler",
          ".dev.vars",
          "coverage",
        ].includes(top);
      },
    });
    await symlink(path.join(packageRoot, "node_modules"), path.join(clone, "node_modules"), "dir");
    const target = path.join(clone, mutant.file);
    const source = await readFile(target, "utf8");
    await writeFile(
      target,
      replaceGuard(source, mutant.begin, mutant.end, mutant.replacement),
      "utf8",
    );

    const result = spawnSync(
      vitest,
      [
        "run",
        "--config",
        "vitest.config.ts",
        ...focusedTests,
        "--maxWorkers=1",
        "--reporter=verbose",
      ],
      {
        cwd: clone,
        encoding: "utf8",
        env: process.env,
      },
    );
    const output = `${result.stdout}\n${result.stderr}`;
    if (result.status === 0 || !output.includes(mutant.expectedFailure)) {
      failed = true;
      console.error(
        `FAIL mutant was not killed by its designated assertion: ${mutant.name}`,
      );
      console.error(result.stdout);
      console.error(result.stderr);
    } else {
      const failureLine = output
        .split("\n")
        .find((line) => /failed|AssertionError|expected/i.test(line)) ??
        "focused suite exited nonzero";
      console.log(
        `PASS mutant killed: ${mutant.name}: ${mutant.expectedFailure}: ${failureLine.trim()}`,
      );
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

if (failed) process.exitCode = 1;
