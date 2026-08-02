import assert from "node:assert/strict";
import test from "node:test";
import { altSvc, probe } from "./test-onion-altsvc.mjs";
test("T11-T16 records every Alt-Svc header without treating h3 as onion proof", async () => {
  assert.deepEqual(altSvc("HTTP/2 200\r\nalt-svc: h3=\":443\"\r\nAlt-Svc: h2=\":443\"\r\n"), ['h3=":443"', 'h2=":443"']);
  const result = await probe("https://example.test", [], "socks5h://proxy:9050", async (_cmd, args) => { assert.ok(args.includes("socks5h://proxy:9050")); return { stdout: "HTTP/2 200\r\nalt-svc: h3=\":443\"\r\n\r\n" }; });
  assert.deepEqual(result.alt_svc, ['h3=":443"']);
});
