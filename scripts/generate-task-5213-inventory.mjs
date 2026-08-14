import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const keyIndex = await readFile(path.join(root, "keyserver-cf/src/index.ts"), "utf8");
const storageIndex = await readFile(path.join(root, "cipher-store-cf/src/index.ts"), "utf8");
const keyConstructor = await readFile(path.join(root, "keyserver-cf/src/lib/person-facing-result.ts"), "utf8");
const storageConstructor = await readFile(path.join(root, "cipher-store-cf/src/lib/person-facing-result.ts"), "utf8");

const K = "keyserver-cf/src/lib/person-facing-result.ts";
const S = "cipher-store-cf/src/lib/person-facing-result.ts";
const routes = [
  ["POST", "/v1/control-inbox", "person", "relay", K, 'path === "/v1/control-inbox"'],
  ["GET", "/v1/control-inbox/:id", "protocol", "relay", "keyserver-cf/src/endpoints/control-inbox.ts", "/^\\/v1\\/control-inbox\\/([^/]+)$/"],
  ["DELETE", "/v1/control-inbox/:id", "protocol", "relay", "keyserver-cf/src/endpoints/control-inbox.ts", "/^\\/v1\\/control-inbox\\/([^/]+)$/"],
  ["GET", "/v1/sender-filter-capability-floor/:id", "protocol", "relay", "keyserver-cf/src/endpoints/sender-filter-capability-floor.ts", "/^\\/v1\\/sender-filter-capability-floor\\/([^/]+)$/"],
  ["GET", "/redeem", "presentation", "payment_voucher", "keyserver-cf/src/endpoints/redeem-page.ts", 'path === "/redeem"'],
  ["GET", "/redeem/:code", "presentation", "payment_voucher", "keyserver-cf/src/endpoints/redeem-page.ts", "/^\\/redeem\\/([^/]+)$/"],
  ["POST", "/v1/register", "person", "key_server", K, 'path === "/v1/register"'],
  ["POST", "/v1/usernames/claim", "person", "key_server", K, 'path === "/v1/usernames/claim"'],
  ["POST", "/v1/usernames/lookup", "person", "key_server", K, 'path === "/v1/usernames/lookup"'],
  ["GET", "/v1/healthz", "protocol", "key_server", "keyserver-cf/src/endpoints/healthz.ts", 'path === "/v1/healthz"'],
  ["GET", "/v1/pubkeys/:id", "protocol", "key_server", "keyserver-cf/src/endpoints/pubkeys.ts", "/^\\/v1\\/pubkeys\\/([^/]+)$/"],
  ["DELETE", "/v1/pubkeys/:id", "protocol", "key_server", "keyserver-cf/src/endpoints/unregister.ts", "/^\\/v1\\/pubkeys\\/([^/]+)$/"],
  ["POST", "/v1/account-ownership/challenge", "protocol", "key_server", "keyserver-cf/src/endpoints/account-ownership-challenge.ts", 'path === "/v1/account-ownership/challenge"'],
  ["POST", "/v1/account-ownership/proof", "protocol", "key_server", "keyserver-cf/src/endpoints/account-ownership-proof.ts", 'path === "/v1/account-ownership/proof"'],
  ["GET", "/v1/prekey-bundle/:id", "protocol", "key_server", "keyserver-cf/src/endpoints/prekey-bundle.ts", "/^\\/v1\\/prekey-bundle\\/([^/]+)$/"],
  ["POST", "/v1/prekey-bundle/replenish", "protocol", "key_server", "keyserver-cf/src/endpoints/prekey-bundle.ts", 'path === "/v1/prekey-bundle/replenish"'],
  ["GET", "/v1/wrapped-keys/:id", "protocol", "key_server", "keyserver-cf/src/endpoints/wrapped-keys.ts", "/^\\/v1\\/wrapped-keys\\/([^/]+)$/"],
  ["POST", "/v1/wrapped-keys", "protocol", "key_server", "keyserver-cf/src/endpoints/wrapped-keys.ts", 'path === "/v1/wrapped-keys"'],
  ["POST", "/v1/wrapped-keys/:id/opened", "protocol", "key_server", "keyserver-cf/src/endpoints/wrapped-keys.ts", "/^\\/v1\\/wrapped-keys\\/([^/]+)\\/opened$/"],
  ["DELETE", "/v1/wrapped-keys", "protocol", "key_server", "keyserver-cf/src/endpoints/wrapped-keys.ts", 'path === "/v1/wrapped-keys"'],
  ["POST", "/v1/link-grant", "protocol", "key_server", "keyserver-cf/src/endpoints/link-grant.ts", 'path === "/v1/link-grant"'],
  ["POST", "/v1/license/redeem", "person", "payment_voucher", K, 'path === "/v1/license/redeem"'],
  ["POST", "/v1/license/validate", "person", "payment_voucher", K, 'path === "/v1/license/validate"'],
  ["GET", "/v/:id", "presentation", "storage", "cipher-store-cf/src/lib/landing.ts", "/^\\/v\\/[^/]{1,128}\\/?$/"],
  ["HEAD", "/v/:id", "presentation", "storage", "cipher-store-cf/src/lib/landing.ts", "/^\\/v\\/[^/]{1,128}\\/?$/"],
  ["POST", "/v/:id/fetch", "person", "storage", S, "/^\\/v\\/([^/]{1,128})\\/fetch$/"],
  ["POST", "/v/:id/burn", "protocol", "storage", "cipher-store-cf/src/endpoints/link.ts", "/^\\/v\\/([^/]{1,128})\\/burn$/"],
  ["POST", "/v1/link", "person", "storage", S, 'path === "/v1/link"'],
  ["POST", "/v1/link/:id/status", "person", "storage", S, "/^\\/v1\\/link\\/([0-9a-f]{32})\\/status$/"],
  ["DELETE", "/v1/link/:id", "person", "storage", S, "/^\\/v1\\/link\\/([0-9a-f]{32})$/"],
  ["PUT", "/v1/blob", "person", "storage", S, 'path === "/v1/blob"'],
  ["GET", "/v1/blob/:id", "person", "storage", S, "/^\\/v1\\/blob\\/([0-9a-fA-F]+)$/"],
  ["POST", "/v1/blob/:id/ack", "person", "storage", S, "/^\\/v1\\/blob\\/([0-9a-f]{32})\\/ack$/"],
  ["DELETE", "/v1/blob/:id", "person", "storage", S, "/^\\/v1\\/blob\\/([0-9a-fA-F]+)$/"],
  ["POST", "/v1/attachment", "person", "storage", S, 'path === "/v1/attachment"'],
  ["POST", "/v1/attachment/session", "person", "storage", S, 'path === "/v1/attachment/session"'],
  ["PUT", "/v1/attachment/:id/part/:part", "person", "storage", S, "/^\\/v1\\/attachment\\/([0-9a-f]{32})\\/part\\/(\\d+)$/"],
  ["POST", "/v1/attachment/:id/complete", "person", "storage", S, "/^\\/v1\\/attachment\\/([0-9a-f]{32})\\/complete$/"],
  ["GET", "/v1/attachment/:id", "person", "storage", S, "/^\\/v1\\/attachment\\/([0-9a-f]+)$/"],
  ["DELETE", "/v1/attachment/:id", "person", "storage", S, "/^\\/v1\\/attachment\\/([0-9a-f]+)$/"],
].map(([method, route, classification, kind, constructor, probe]) => ({
  method, route, classification, kind, constructor, probe,
}));

for (const item of routes) {
  const source = item.kind === "storage" ? storageIndex : keyIndex;
  if (!source.includes(item.probe)) {
    throw new Error(`TASK5213b deployed_route="${item.method} ${item.route}" field=deployed.route: dispatcher probe missing`);
  }
  if (item.classification === "person") {
    const constructor = item.kind === "storage" ? storageConstructor : keyConstructor;
    const routeStem = item.route.replaceAll(":id", "").replaceAll(":part", "").replaceAll(":code", "");
    if (!constructor.includes(routeStem.split("/").filter(Boolean).at(-1) ?? item.route)) {
      throw new Error(`TASK5213b deployed_route="${item.method} ${item.route}" field=response_constructor: person adapter route missing`);
    }
  }
}

const inventory = {
  schema: "osl.service-person-results.inventory.v1",
  routes: routes.map(({ probe: _probe, ...item }) => item),
};
process.stdout.write(`${JSON.stringify(inventory, null, 2)}\n`);
