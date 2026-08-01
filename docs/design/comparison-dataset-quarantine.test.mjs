import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const manifestPath = path.join(repositoryRoot, "data", "public-surface-manifest.json");
const comparisonDataset = "assets/data/apps.json";

test("comparison dataset is quarantined from the public-surface manifest", () => {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const publishedPaths = [...(manifest.html ?? []), ...(manifest.assets ?? [])];

  assert.ok(
    !publishedPaths.includes(comparisonDataset),
    `${comparisonDataset} is quarantined and must not be declared as a public surface`,
  );
});
