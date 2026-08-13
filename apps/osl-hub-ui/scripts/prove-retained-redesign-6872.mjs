#!/usr/bin/env node

import { spawnSync } from "node:child_process";

const mutations = [
  ["starve-page", "PAGE_COUNT"],
  ["duplicate-page", "UNIQUE_PAGE_IDS"],
  ["starve-state", "ROUTE_STATE page="],
  ["starve-width", "WIDTH_STARVED page="],
  ["starve-theme", "THEMES expected="],
  ["starve-a11y", "accessibility-starved"],
  ["restore-deleted", "DELETED_ROUTE_REACHABLE page="],
  ["detach-capability", "CAPABILITY_FEED_MISSING page="],
  ["screenshot-only", "screenshot-only-markup"],
];

for (const [mutation, expected] of mutations) {
  const result = spawnSync(
    process.execPath,
    ["node_modules/vite-node/vite-node.mjs", "scripts/check-retained-redesign-6872.ts"],
    {
      cwd: process.cwd(),
      encoding: "utf8",
      env: { ...process.env, OSL6872_MUTATION: mutation },
    },
  );
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  if (result.status !== 1 || !output.includes(expected) || !output.includes("TASK6872_RESULT status=red")) {
    process.stderr.write(output);
    throw new Error(`TASK6872_MUTATION_NOT_CAUGHT mutation=${mutation} rc=${result.status} expected=${expected}`);
  }
  console.log(`TASK6872_MUTATION mutation=${mutation} exit=${result.status} named=${expected}`);
}

const green = spawnSync(
  process.execPath,
  ["node_modules/vite-node/vite-node.mjs", "scripts/check-retained-redesign-6872.ts"],
  { cwd: process.cwd(), encoding: "utf8", env: { ...process.env, OSL6872_MUTATION: "" } },
);
process.stdout.write(green.stdout ?? "");
process.stderr.write(green.stderr ?? "");
if (green.status !== 0 || !(green.stdout ?? "").includes("TASK6872_RESULT status=green")) {
  throw new Error(`TASK6872_RESTORATION_FAILED rc=${green.status}`);
}
console.log(`TASK6872_MUTATION_PROOF mutations=${mutations.length} restored_exit=0`);

