import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const backend = readFileSync(new URL("../../osl-hub/src/tor_pref.rs", import.meta.url), "utf8");
const packageConfig = JSON.parse(
  readFileSync(new URL("../../osl-hub/tauri.conf.json", import.meta.url), "utf8"),
) as { bundle: { externalBin: string[]; resources: Record<string, string> } };

describe("TASK 4914 bridge shipping wiring", () => {
  it("turns the first-run checkbox into the persisted bridge preference", () => {
    expect(main).toMatch(
      /querySelector<HTMLInputElement>\("\[data-tor-bridge\]"\)[\s\S]*checked \? "bridge" : "tor"/u,
    );
    expect(main).toContain('invoke("set_tor_preference", { preference: torOnboarding.choice })');
    expect(backend).toContain("TorPreference::Bridge");
    expect(backend).toContain('"--bridge-config".to_owned()');
    expect(backend).toContain('"--transport-program".to_owned()');

    expect(packageConfig.bundle.externalBin).toContain("binaries/osl-bridge-transport");
    expect(packageConfig.bundle.resources["../osl-tor-sidecar/assets/tor-bridges.txt"]).toBe("tor-bridges.txt");
    console.log("TASK4914_FIRST_RUN_BRIDGE_PREFERENCE=bridge");
    console.log("TASK4914_PACKAGED_TRANSPORTS=1");
    console.log("TASK4914_PACKAGED_BRIDGE_FILES=1");
  });
});
