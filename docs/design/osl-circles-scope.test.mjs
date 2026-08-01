import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const scopePath = new URL("./osl-circles-scope.md", import.meta.url);
const source = readFileSync(scopePath, "utf8");
const match = source.match(/## Contract test vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(match, "the Circles scope must expose machine-readable contract vectors");
const contract = JSON.parse(match[1]);

test("Circles are the server/community product, not named post recipients", () => {
  assert.equal(contract.version, 1);
  assert.equal(contract.circles.product, "osl-native-server-community");
  assert.equal(contract.privateAudiences.product, "named-post-recipients");
  assert.equal(contract.privateAudiences.isCircle, false);
});

test("a Circle retains the D66 community capability boundary", () => {
  assert.deepEqual(contract.circles.requiredCapabilities, [
    "membership",
    "channels",
    "roles-permissions",
    "moderation",
    "join-leave",
  ]);
  assert.equal(contract.circles.shippingStatus, "not-implemented");
  assert.equal(contract.privateAudiences.shippingStatus, "not-implemented");
});
