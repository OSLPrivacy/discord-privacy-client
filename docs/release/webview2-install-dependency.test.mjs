import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const configPath = new URL("../../apps/osl-hub/tauri.conf.json", import.meta.url);
const contractPath = new URL("./webview2-install-dependency.md", import.meta.url);

test("Windows installer network dependency is intentional and disclosed", async () => {
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const contract = await readFile(contractPath, "utf8");
  const installMode = config.bundle.windows.webviewInstallMode;

  assert.deepEqual(installMode, { type: "downloadBootstrapper", silent: true });
  assert.match(contract, /Decision: acceptable for the standard installer/i);
  assert.match(contract, /requires an internet connection during installation/i);
  assert.match(contract, /not an offline installer/i);
  assert.match(contract, /must not present installation as complete/i);
  assert.match(contract, /offlineInstaller/i);
  assert.match(contract, /D44/i);
});
