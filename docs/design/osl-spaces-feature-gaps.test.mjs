import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./osl-spaces-feature-gaps.md", import.meta.url), "utf8");
const match = source.match(/## Contract vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the Spaces handoff must provide machine-checkable contract vectors");
const contract = JSON.parse(match[1]);

function resolveExpiry({ message, channel, space }) {
  return message ?? channel ?? space;
}

function aggregateLabel({ confirmed, unconfirmedActive, departedUnconfirmed }) {
  return `${confirmed} confirmed; ${unconfirmedActive} active recipient unconfirmed; ${departedUnconfirmed} departed recipient unconfirmed`;
}

test("a Space resolves expiry once with message, channel, then Space precedence", () => {
  assert.deepEqual(contract.expiryPrecedence, ["message", "channel", "space"]);
  assert.equal(resolveExpiry({ message: "m", channel: "c", space: "s" }), "m");
  assert.equal(resolveExpiry({ message: null, channel: "c", space: "s" }), "c");
  assert.equal(resolveExpiry({ message: null, channel: null, space: "s" }), "s");
});

test("a departed member remains visibly unconfirmed instead of pinning an active aggregate", () => {
  const aggregate = contract.departureAggregate;
  assert.equal(aggregate.departedUnconfirmed, 1);
  assert.equal(aggregateLabel(aggregate), aggregate.display);
});

test("a removal tombstone blocks future fan-out and replay without claiming a retroactive wipe", () => {
  assert.deepEqual(contract.tombstone, {
    stopsFutureFanout: true,
    preservesSenderAttribution: true,
    rejectsPreRemovalReplay: true,
    retroactiveContentWipe: false,
  });
});
