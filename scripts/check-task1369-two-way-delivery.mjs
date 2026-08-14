#!/usr/bin/env node

import { readFile } from "node:fs/promises";

const expectedDirections = [
  {
    sender: "task1369-alice-copy",
    recipient: "task1369-bob-copy",
    markerPrefix: "TASK1369-Alice-to-Bob-",
  },
  {
    sender: "task1369-bob-copy",
    recipient: "task1369-alice-copy",
    markerPrefix: "TASK1369-Bob-to-Alice-",
  },
];

function usage() {
  console.error("usage: check-task1369-two-way-delivery.mjs <task-1369-command-output>");
}

function parseReceipts(output) {
  const receiptPattern =
    /^TASK1369 copy=(\S+) received_from=(\S+) exact_text="([^"]+)" count=(\d+) total_messages=(\d+)$/u;

  return output
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .map((line) => receiptPattern.exec(line))
    .filter((match) => match !== null)
    .map((match) => ({
      recipient: match[1],
      sender: match[2],
      exactText: match[3],
      count: Number.parseInt(match[4], 10),
      totalMessages: Number.parseInt(match[5], 10),
    }));
}

export function checkTwoWayDeliveryProof(output) {
  const receipts = parseReceipts(output);
  const failures = [];

  for (const expected of expectedDirections) {
    const matching = receipts.filter(
      (receipt) =>
        receipt.sender === expected.sender &&
        receipt.recipient === expected.recipient &&
        receipt.exactText.startsWith(expected.markerPrefix) &&
        receipt.count === 1,
    );

    if (matching.length !== 1) {
      failures.push(
        `missing delivery direction: ${expected.sender} -> ${expected.recipient} ` +
          `(expected one count=1 receipt, observed ${matching.length})`,
      );
    }
  }

  return failures;
}

async function main() {
  const [fixturePath, ...extraArguments] = process.argv.slice(2);
  if (fixturePath === undefined || extraArguments.length !== 0) {
    usage();
    process.exitCode = 2;
    return;
  }

  let output;
  try {
    output = await readFile(fixturePath, "utf8");
  } catch (error) {
    console.error(`unable to read two-way delivery proof: ${error.message}`);
    process.exitCode = 2;
    return;
  }

  const failures = checkTwoWayDeliveryProof(output);
  if (failures.length > 0) {
    for (const failure of failures) {
      console.error(`TASK1613 ${failure}`);
    }
    process.exitCode = 1;
    return;
  }

  console.log("TASK1613 two-way delivery proof passed directions=2");
}

if (process.argv[1] !== undefined && import.meta.url === new URL(process.argv[1], "file:").href) {
  await main();
}
