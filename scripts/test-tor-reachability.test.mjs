import assert from "node:assert/strict";
import test from "node:test";
import { assessTorResponse, parseCurlResponse, probeTorReachability } from "./test-tor-reachability.mjs";

const healthyTorResponse = {
  status: 200,
  headers: { "content-type": "application/json", "cf-ipcountry": "T1" },
  body: '{"ok":true}',
};

test("accepts a JSON health response from a T1-classified Tor request", () => {
  assert.deepEqual(assessTorResponse(healthyTorResponse, { expectedCountry: "T1" }), {
    json: { ok: true },
    observedCountry: "T1",
  });
});

test("refuses a Cloudflare challenge even when it claims success", () => {
  assert.throws(
    () => assessTorResponse({ ...healthyTorResponse, headers: { ...healthyTorResponse.headers, "cf-mitigated": "challenge" } }),
    /Cloudflare returned a challenge/,
  );
});

test("refuses a response whose country does not prove the requested T1 path", () => {
  assert.throws(() => assessTorResponse(healthyTorResponse, { expectedCountry: "US" }), /expected cf-ipcountry US/);
});

test("runs curl through the supplied SOCKS proxy and validates its response", async () => {
  let receivedArgs;
  const result = await probeTorReachability({
    proxy: "socks5h://127.0.0.1:19050",
    expectedCountry: "T1",
    runCurl: async (args) => {
      receivedArgs = args;
      return { stdout: "HTTP/2 200\r\ncontent-type: application/json\r\ncf-ipcountry: T1\r\n\r\n{\"ok\":true}" };
    },
  });
  assert.equal(receivedArgs[receivedArgs.indexOf("--proxy") + 1], "socks5h://127.0.0.1:19050");
  assert.deepEqual(result.json, { ok: true });
});

test("parses curl's final response after an HTTP redirect", () => {
  const parsed = parseCurlResponse("HTTP/2 301\r\nlocation: /next\r\n\r\nHTTP/2 200\r\ncontent-type: application/json\r\n\r\n{\"ok\":true}");
  assert.equal(parsed.status, 200);
  assert.deepEqual(JSON.parse(parsed.body), { ok: true });
});
