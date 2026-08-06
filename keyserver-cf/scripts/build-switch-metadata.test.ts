import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  activeRuntimeSwitchesFromWranglerToml,
  oneBuildVersionFromUpdateManifestSource,
} from "./build-switch-metadata.ts";

const PACKAGE_ROOT = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const SCRIPT = path.join(PACKAGE_ROOT, "scripts", "build-switch-metadata.ts");

function runMetadataCommand(args: string[] = []) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: PACKAGE_ROOT,
    encoding: "utf8",
  });
}

describe("build switch metadata", () => {
  it("prints the one-build version and every active runtime switch", () => {
    const result = runMetadataCommand();
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("one-build version: 0.0.1");
    expect(result.stdout).toContain("active runtime switches:");
    expect(result.stdout).toContain("- CRYPTO_BTC_ENABLED=true");
    expect(result.stdout).toContain("- CRYPTO_XMR_ENABLED=true");
    expect(result.stdout).toContain("- CRYPTO_DONATION_BTC_ENABLED=true");
    expect(result.stdout).toContain("- CRYPTO_DONATION_XMR_ENABLED=true");
  });

  it("exits 1 when one active runtime switch is omitted from the record", () => {
    const result = runMetadataCommand(["--omit", "CRYPTO_XMR_ENABLED"]);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain(
      "build-switch metadata omits active runtime switch CRYPTO_XMR_ENABLED",
    );
  });

  it("derives active switches from the tracked runtime vars", () => {
    const switches = activeRuntimeSwitchesFromWranglerToml(`
[vars]
CRYPTO_BTC_ENABLED = "true"
LINK_GRANT_ENABLED = "false"
CRYPTO_XMR_ENABLED = "true"

[[routes]]
pattern = "keyserver.oslprivacy.com/*"
`);
    expect(switches).toEqual([
      { name: "CRYPTO_BTC_ENABLED", value: "true" },
      { name: "CRYPTO_XMR_ENABLED", value: "true" },
    ]);
  });

  it("derives the one-build version from the update manifest source", () => {
    expect(oneBuildVersionFromUpdateManifestSource(
      'export const PRODUCTION_VERSION = "0.0.1";',
    )).toBe("0.0.1");
  });
});
