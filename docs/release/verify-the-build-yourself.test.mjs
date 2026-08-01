import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const guidePath = new URL("./verify-the-build-yourself.md", import.meta.url);

test("third-party build guide pins the clean-room release recipe", async () => {
  const guide = await readFile(guidePath, "utf8");

  assert.match(guide, /D:\\a\\discord-privacy-client\\discord-privacy-client/);
  assert.match(guide, /Rust \*\*1\.88\.0\*\*/);
  assert.match(guide, /Node\.js \*\*22\*\*/);
  assert.match(guide, /^npm install -g @tauri-apps\/cli@2\.11\.4$/m);
  assert.match(guide, /tauri build --features desktop/);
  assert.match(guide, /custom-protocol/);
  assert.match(guide, /bundle-type/);
  assert.match(guide, /Get-FileHash .* -Algorithm SHA256/);
  assert.match(guide, /released executable bytes do not reproduce from exact source/);
});
