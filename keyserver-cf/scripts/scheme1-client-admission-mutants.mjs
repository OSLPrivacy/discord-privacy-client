import {
  cp,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import {
  SCHEME1_CLIENT_ADMISSION_MUTANTS,
} from "./scheme1-client-admission-mutant-catalog.mjs";

const packageRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const vitest = path.join(packageRoot, "node_modules", ".bin", "vitest");
const contractFile = "scripts/scheme1-client-admission-contract.mjs";
const focusedTest = "scripts/scheme1-client-admission.test.ts";

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

let failed = false;
for (const mutant of SCHEME1_CLIENT_ADMISSION_MUTANTS) {
  const temporary = await mkdtemp(
    path.join(tmpdir(), `osl-scheme1-client-${mutant.name}-`),
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
    await symlink(
      path.join(packageRoot, "node_modules"),
      path.join(clone, "node_modules"),
      "dir",
    );
    const target = path.join(clone, contractFile);
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
        "vitest.node.config.ts",
        focusedTest,
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
    if (
      result.status === 0 ||
      !output.includes(mutant.expectedFailure)
    ) {
      failed = true;
      console.error(
        `FAIL mutant was not killed by its designated assertion: ${mutant.name}`,
      );
      console.error(result.stdout);
      console.error(result.stderr);
    } else {
      console.log(
        `PASS mutant killed: ${mutant.name}: ${mutant.expectedFailure}`,
      );
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

if (failed) process.exitCode = 1;
