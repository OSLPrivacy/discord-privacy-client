#!/usr/bin/env node

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const RUNTIME_SOURCE_ROOTS = ["apps/osl-hub/src", "apps/osl-hub-ui/src"];
const TRANSPARENCY_LOG_REFERENCE = /\b(?:rekor|sigstore|transparency[-_ ]?log)\b/i;

async function sourceFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(root, entry.name);
      if (entry.isDirectory()) return sourceFiles(entryPath);
      return entry.isFile() ? [entryPath] : [];
    }),
  );
  return files.flat();
}

export async function findRuntimeTransparencyReferences(repositoryRoot) {
  const roots = RUNTIME_SOURCE_ROOTS.map((runtimeRoot) =>
    path.join(repositoryRoot, runtimeRoot),
  );
  const files = (await Promise.all(roots.map(sourceFiles))).flat();
  const references = [];

  for (const file of files) {
    const contents = await readFile(file, "utf8");
    if (TRANSPARENCY_LOG_REFERENCE.test(contents)) {
      references.push(path.relative(repositoryRoot, file));
    }
  }
  return references.sort();
}

export async function assertNoRuntimeTransparencyQueries(repositoryRoot) {
  const references = await findRuntimeTransparencyReferences(repositoryRoot);
  if (references.length > 0) {
    throw new Error(
      `runtime transparency-log reference is forbidden: ${references.join(", ")}`,
    );
  }
}

const invokedPath = process.argv[1] && path.resolve(process.argv[1]);
const modulePath = fileURLToPath(import.meta.url);
if (invokedPath === modulePath) {
  const repositoryRoot = path.resolve(path.dirname(modulePath), "..");
  await assertNoRuntimeTransparencyQueries(repositoryRoot);
}
