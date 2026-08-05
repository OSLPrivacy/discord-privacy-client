import { afterEach, describe, expect, it } from "vitest";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { runProbe } from "../scripts/deletion-durability-probe.mjs";

const servers: ReturnType<typeof createServer>[] = [];

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve) => server.close(() => resolve()))));
});

function start({ objectStatus = 404, locks = [] as unknown[] } = {}) {
  const server = createServer((request, response) => {
    if (request.url === "/v1/blob" && request.method === "POST") return response.writeHead(201).end();
    if (request.url?.includes("/ack") && request.method === "POST") return response.writeHead(204).end();
    if (request.url?.startsWith("/v1/blob/") && request.method === "DELETE") return response.writeHead(204).end();
    if (request.url?.includes("/objects/") && request.method === "HEAD") return response.writeHead(objectStatus).end();
    if (request.url?.endsWith("/lock")) return response.end(JSON.stringify({ success: true, result: locks }));
    if (request.url?.includes("/r2/buckets/")) return response.end(JSON.stringify({ success: true, result: { name: "payloads" } }));
    response.writeHead(404).end();
  });
  servers.push(server);
  return new Promise<string>((resolve) => server.listen(0, "127.0.0.1", () => resolve(`http://127.0.0.1:${(server.address() as AddressInfo).port}`)));
}

// Both cases passed `worker: "https://preview.workers.dev"` -- a REAL hostname
// -- while pointing only `apiBase` at the local fixture. So `runProbe`'s very
// first call, the upload, went out to the internet and died on
// `getaddrinfo ENOTFOUND preview.workers.dev`, on the GitHub runner exactly as
// locally. Neither test ever reached the R2 HEAD assertion it exists to make:
// the second one "passed its rejects.toThrow" only in the sense that something
// threw, and it was DNS, not the probe's refusal.
//
// The fixture server already answers every worker route the probe uses
// (/v1/blob POST, /ack, DELETE) alongside the control-plane ones, so it was
// built to serve both bases. Pointing `worker` at it makes these hermetic and
// makes the assertions execute.
describe("deletion durability probe", () => {
  it("passes only after direct R2 HEAD observes the ACK-deleted object as missing", async () => {
    const base = await start();
    await expect(runProbe({ worker: base, accountId: "account", bucket: "payloads", token: "token", apiBase: base })).resolves.toMatchObject({ r2HeadStatus: 404, lockRules: 0 });
  });

  it("fails loudly when the direct R2 read still observes the deleted object", async () => {
    const base = await start({ objectStatus: 200 });
    await expect(runProbe({ worker: base, accountId: "account", bucket: "payloads", token: "token", apiBase: base })).rejects.toThrow("deleted payload is still observable");
  });
});
