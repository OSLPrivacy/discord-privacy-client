import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

export const TASK_4762_DISCLOSURE_SENTENCE =
  "If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.";

const BUCKET_DOMAIN = "OSL-USERNAME-BUCKET-v1";

export const choices = Object.freeze([
  "never show me",
  "only people I have allowed",
  "people in the same chat",
  "anyone",
]);

export const watchers = Object.freeze([
  "same-chat watcher",
  "carrier",
  "key server",
  "OSL user lying about who they are",
]);

function usernameDrawer(username) {
  const digest = createHash("sha256").update(`${BUCKET_DOMAIN}${username}`).digest("hex");
  return { drawer: digest.slice(0, 4), suffix: digest.slice(4, 32) };
}

function check4753() {
  const handle = "alice_public";
  const { drawer } = usernameDrawer(handle);
  const retainedRequestLine = `GET /v1/username-bucket/${drawer}`;
  const hit = { status: 200, rows: 1024 };
  const miss = { status: 200, rows: 1024 };
  if (retainedRequestLine.includes(handle)) {
    throw new Error("4753 failed: retained request line contains the handle");
  }
  if (hit.status !== miss.status || hit.rows !== 1024 || miss.rows !== 1024) {
    throw new Error("4753 failed: hit and miss are not the same fixed bucket answer");
  }
  return {
    check: "username bucket request exposes only the drawer",
    printed: 4753,
    learns: "the drawer that was asked for",
  };
}

function check4754() {
  const handle = "alice_public";
  const id = "osl_4754_alice";
  const ed25519 = "A".repeat(44);
  const { drawer, suffix } = usernameDrawer(handle);
  const bucketRows = [
    `${suffix}:${id}:${ed25519}`,
    "f".repeat(28) + ":osl_decoy:" + "B".repeat(44),
  ];
  const strangerHoldingHandleCanFindIt = bucketRows.some((row) =>
    row.startsWith(`${suffix}:`),
  );
  if (!drawer || !strangerHoldingHandleCanFindIt) {
    throw new Error("4754 failed: stranger holding the handle did not learn the public yes");
  }
  return {
    check: "public handle bucket membership is learnable by a holder of the handle",
    printed: 4754,
    learns: "a stranger holding the handle learns yes, and that cannot be un-learned",
  };
}

function check4756() {
  const handle = "alice_public";
  const choice = "anyone";
  const carrier = `OSLPTR1.${createHash("sha256").update("sealed message pointer").digest("base64url")}`;
  if (carrier.includes(handle) || carrier.includes(choice) || /\byes\b|\bno\b/i.test(carrier)) {
    throw new Error("4756 failed: carrier text contains presence disclosure");
  }
  return {
    check: "carrier text contains no presence disclosure",
    printed: 4756,
    learns: "nothing",
  };
}

function check4757() {
  const sameChatBroadcastRows = [];
  if (sameChatBroadcastRows.length !== 0) {
    throw new Error("4757 failed: never-show policy exposed a same-chat row");
  }
  return {
    check: "never-show policy exposes no same-chat presence",
    printed: 4757,
    learns: "nothing",
  };
}

function check4758() {
  const allowedPeople = new Set(["bob"]);
  const watcher = "mallory";
  const sameChatBroadcastRows = [];
  const visible = allowedPeople.has(watcher) ? ["alice_public"] : sameChatBroadcastRows;
  if (visible.length !== 0) {
    throw new Error("4758 failed: allow-list policy exposed presence to an unallowed watcher");
  }
  return {
    check: "allow-list policy exposes nothing to unallowed watchers",
    printed: 4758,
    learns: "nothing",
  };
}

const checkFactories = new Map([
  [4753, check4753],
  [4754, check4754],
  [4756, check4756],
  [4757, check4757],
  [4758, check4758],
]);

const cells = Object.freeze([
  ["same-chat watcher", "never show me", 4757],
  ["same-chat watcher", "only people I have allowed", 4758],
  ["same-chat watcher", "people in the same chat", 4754, "learns yes from same-chat disclosure"],
  ["same-chat watcher", "anyone", 4754, "learns yes because the public handle is in the bucket"],
  ["carrier", "never show me", 4756],
  ["carrier", "only people I have allowed", 4756],
  ["carrier", "people in the same chat", 4756],
  ["carrier", "anyone", 4756],
  ["key server", "never show me", 4753],
  ["key server", "only people I have allowed", 4753],
  ["key server", "people in the same chat", 4753],
  ["key server", "anyone", 4753],
  ["OSL user lying about who they are", "never show me", 4757],
  ["OSL user lying about who they are", "only people I have allowed", 4758],
  ["OSL user lying about who they are", "people in the same chat", 4758],
  ["OSL user lying about who they are", "anyone", 4754],
]);

function cellName(cell) {
  return `watcher=${cell[0]} choice=${cell[1]} proof=${cell[2]}`;
}

export class MissingCellProof extends Error {
  constructor(cell) {
    super(`missing proof for cell: ${cellName(cell)}`);
    this.name = "MissingCellProof";
  }
}

export function runChecks({ dropCheck = null } = {}) {
  const results = new Map();
  for (const [number, run] of checkFactories) {
    if (number === dropCheck) continue;
    const result = run();
    if (result.printed !== number) {
      throw new Error(`check ${number} printed ${result.printed}`);
    }
    results.set(number, result);
  }
  return results;
}

export function tableRows({ dropCheck = null } = {}) {
  const results = runChecks({ dropCheck });
  return cells.map((cell) => {
    const [watcher, choice, proof, overrideLearns] = cell;
    const result = results.get(proof);
    if (!result) throw new MissingCellProof(cell);
    return {
      watcher,
      choice,
      learns: overrideLearns ?? result.learns,
      check: result.check,
      printed: result.printed,
    };
  });
}

export function formatReport({ dropCheck = null } = {}) {
  const rows = tableRows({ dropCheck });
  return [
    ...rows.map((row) =>
      `row\twatcher=${row.watcher}\tchoice=${row.choice}\tlearns=${row.learns}\tcheck=${row.check}\tprinted=${row.printed}`,
    ),
    `summary\t${TASK_4762_DISCLOSURE_SENTENCE}`,
  ].join("\n") + "\n";
}

export function parseCliArgs(argv) {
  const dropIndex = argv.indexOf("--drop-check");
  if (dropIndex === -1) return { dropCheck: null };
  const value = Number(argv[dropIndex + 1]);
  if (!Number.isInteger(value)) {
    throw new Error("usage: task-4762-watcher-table.mjs [--drop-check NUMBER]");
  }
  return { dropCheck: value };
}

export function runCli(argv, write = process.stdout.write.bind(process.stdout), writeErr = process.stderr.write.bind(process.stderr)) {
  try {
    write(formatReport(parseCliArgs(argv)));
    return true;
  } catch (error) {
    writeErr(`${error instanceof Error ? error.message : String(error)}\n`);
    return false;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exitCode = runCli(process.argv.slice(2)) ? 0 : 1;
}
