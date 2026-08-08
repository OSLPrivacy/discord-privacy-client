// Records what the real TASK 1449 command answers for every request the TASK
// 1450 deletion review screen sends, into src/task-1450-backend-transcript.json.
//
// The screen's check replays that file instead of imitating the command, so a
// request shape that drifts from what the backend accepts has nowhere to hide:
// the replay has no reply for it and the check goes red.
//
// Run from the repo root:
//   CARGO_TARGET_DIR=<lane target> node apps/osl-hub-ui/scripts/task-1450-record-backend.mjs

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..", "..", "..");
const transcriptPath = path.join(scriptDir, "..", "src", "task-1450-backend-transcript.json");
const cargo = process.env.CARGO ?? path.join(process.env.HOME ?? "", ".cargo", "bin", "cargo");

// The four results of the TASK 1446 review fixture, in review order, with the
// decisions the check makes: two marked, one kept, one never reviewed. The
// locator is the review's own opaque result id -- no link, no path.
const FIXTURE_MESSAGES = [
  {
    accountId: "discord-account-alpha-1444",
    messageLocator: "discord-account-alpha-1444 0",
    decision: "markedForDeletion",
    reviewed: true,
  },
  {
    accountId: "discord-account-alpha-1444",
    messageLocator: "discord-account-alpha-1444 1",
    decision: "kept",
    reviewed: true,
  },
  {
    accountId: "telegram-account-beta-1444",
    messageLocator: "telegram-account-beta-1444 0",
    decision: "markedForDeletion",
    reviewed: true,
  },
  {
    accountId: "telegram-account-beta-1444",
    messageLocator: "telegram-account-beta-1444 1",
    decision: "pending",
    reviewed: false,
  },
];

function ask(command, request) {
  const result = spawnSync(
    cargo,
    [
      "run",
      "--locked",
      "--quiet",
      "--manifest-path",
      "apps/osl-hub/Cargo.toml",
      "-p",
      "osl-hub",
      "--no-default-features",
      "--features",
      "core,task-1256-only",
      "--example",
      "task_1450_marked_deletion_bridge",
      "--",
      command,
      JSON.stringify(request),
    ],
    { cwd: repoRoot, encoding: "utf8" },
  );
  if (result.status !== 0) {
    process.stderr.write(result.stderr ?? "");
    throw new Error(`bridge exited ${result.status} for ${command}`);
  }
  const reply = JSON.parse(result.stdout.trim().split("\n").pop());
  return { command, request, reply };
}

const exchanges = [];

// Free, both commands. The screen never draws either control on Free; these
// record what the command does if the plan is sent anyway.
exchanges.push(ask("count", { plan: "free", messages: FIXTURE_MESSAGES }));

// Pro asks for the final count first.
const proCount = ask("count", { plan: "pro", messages: FIXTURE_MESSAGES });
exchanges.push(proCount);
const token = proCount.reply.result.confirmationToken;

exchanges.push(
  ask("delete", {
    plan: "free",
    messages: FIXTURE_MESSAGES,
    confirmationToken: token,
    confirmed: true,
  }),
);

// Pro confirms the count it was shown.
exchanges.push(
  ask("delete", {
    plan: "pro",
    messages: FIXTURE_MESSAGES,
    confirmationToken: token,
    confirmed: true,
  }),
);

const transcript = {
  recordedFrom:
    "cargo run -p osl-hub --no-default-features --features core,task-1256-only --example task_1450_marked_deletion_bridge",
  command: "run_pro_marked_deletion_command (apps/osl-hub/src/pro_marked_deletion.rs, TASK 1449)",
  fixtureMessages: FIXTURE_MESSAGES,
  exchanges,
};

writeFileSync(transcriptPath, `${JSON.stringify(transcript, null, 2)}\n`, "utf8");
process.stdout.write(`TASK1450_TRANSCRIPT_EXCHANGES=${exchanges.length}\n`);
for (const exchange of exchanges) {
  process.stdout.write(
    `TASK1450_RECORDED command=${exchange.command} plan=${exchange.request.plan} reply=${JSON.stringify(exchange.reply)}\n`,
  );
}
