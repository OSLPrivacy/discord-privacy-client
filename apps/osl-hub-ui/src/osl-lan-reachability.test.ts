import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CURRENT_LAN_AVAILABILITY_CLAIM =
  /\bIt does include\b[^.\n]{0,1200}\bsame[- ]LAN collaboration\b|\bLAN rooms?\b[^.\n]{0,160}\b(?:are|is)\s+(?:implemented|available|included|shipping)\b|\bFree collaboration\b[^.\n]{0,160}\buses\b[^.\n]{0,160}\bLAN rooms?\b/iu;
const CURRENT_PLUGIN_AVAILABILITY_CLAIM =
  /\bfirst executable surface is implemented for\s+`?\.oslmod`?\s+command packs\b|\bcurrent desktop build\b(?:(?!\b(?:does not|doesn't|doesn’t|no executable)\b)[^\n]){0,240}\b(?:runs?|executes?|supports?|offers?)\b[^\n]{0,240}(?:\.oslmod\b|\bplugins?\b|\bcommand packs?\b)|\.oslmod\b[^\n]{0,160}\b(?:is|are)\s+(?:available|supported|executable)\b/iu;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("OSL LAN production reachability", () => {
  it("does not sell the implemented-but-unwired LAN room prototype", () => {
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const uiLanAdapter = readFileSync(new URL("./osl-collab.ts", import.meta.url), "utf8");
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const rustLan = readFileSync(new URL("../../osl-hub/src/osl_lan.rs", import.meta.url), "utf8");
    const rustCollab = readFileSync(new URL("../../osl-hub/src/osl_collab.rs", import.meta.url), "utf8");
    const architecture = readFileSync(
      new URL("../../../docs/design/osl-notes-architecture.md", import.meta.url),
      "utf8",
    );
    const creativeSuite = readFileSync(
      new URL("../../../docs/design/osl-creative-suite.md", import.meta.url),
      "utf8",
    );
    const handler = sourceBetween(rustMain, "tauri::generate_handler![", "\n    ]);");
    const commandNames = [
      "host_osl_lan_room",
      "join_osl_lan_room",
      "sync_hosted_osl_lan_room",
      "sync_joined_osl_lan_room",
      "stop_osl_lan_room",
    ];
    const uiFunctions = [
      "hostOslLanRoom",
      "joinOslLanRoom",
      "syncOslLanRoom",
      "stopOslLanRoom",
    ];

    expect(rustLan).toContain("pub fn host(");
    expect(rustLan).toContain("pub fn join(");
    expect(rustLan).toContain("pub fn stop(");
    expect(rustCollab).toContain("pub fn create_invitation(");
    for (const command of commandNames) expect(uiLanAdapter).toContain(command);

    const modulesDeclared =
      /\bpub mod osl_lan\s*;/u.test(rustLib) && /\bpub mod osl_collab\s*;/u.test(rustLib);
    const commandsRegistered = commandNames.every((command) =>
      new RegExp(`\\b${command}\\b`, "u").test(handler)
    );
    const uiImported = /from\s+["']\.\/osl-collab["']/u.test(uiMain);
    const uiCalled = uiFunctions.every((name) =>
      new RegExp(`\\b${name}\\s*\\(`, "u").test(uiMain)
    );
    const productionReachable =
      modulesDeclared && commandsRegistered && uiImported && uiCalled;
    const publicDocs = `${architecture}\n${creativeSuite}`;

    expect(productionReachable).toBe(false);
    if (!productionReachable) {
      expect(publicDocs).not.toMatch(CURRENT_LAN_AVAILABILITY_CLAIM);
    }

    expect(
      "It does include encrypted same-LAN collaboration.",
    ).toMatch(CURRENT_LAN_AVAILABILITY_CLAIM);
    expect(
      "Free direct LAN rooms are implemented with encrypted frames.",
    ).toMatch(CURRENT_LAN_AVAILABILITY_CLAIM);
    expect(
      "Free collaboration uses direct encrypted LAN rooms.",
    ).toMatch(CURRENT_LAN_AVAILABILITY_CLAIM);
    expect(architecture).toContain(
      "implemented in isolated source modules but is not declared, registered, or reachable",
    );
    expect(creativeSuite).toContain(
      "those source properties are not shipping product behavior",
    );
  });

  it("does not sell the implemented-but-unwired .oslmod interpreter", () => {
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const uiPluginAdapter = readFileSync(
      new URL("./osl-plugin-runtime.ts", import.meta.url),
      "utf8",
    );
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const rustPlugins = readFileSync(
      new URL("../../osl-hub/src/osl_plugins.rs", import.meta.url),
      "utf8",
    );
    const creativeSuite = readFileSync(
      new URL("../../../docs/design/osl-creative-suite.md", import.meta.url),
      "utf8",
    );
    const handler = sourceBetween(rustMain, "tauri::generate_handler![", "\n    ]);");
    const commandNames = ["inspect_osl_plugin_asset", "run_osl_plugin_command"];
    const uiFunctions = ["inspectOslPluginAsset", "runOslPluginCommand"];

    expect(rustPlugins).toContain("pub fn inspect(");
    expect(rustPlugins).toContain("pub fn run(");
    expect(rustPlugins).toContain("Config::default()");
    expect(rustPlugins).toContain("config.consume_fuel(true)");
    for (const command of commandNames) expect(uiPluginAdapter).toContain(command);

    const moduleDeclared = /\bpub mod osl_plugins\s*;/u.test(rustLib);
    const commandsRegistered = commandNames.every((command) =>
      new RegExp(`\\b${command}\\b`, "u").test(handler)
    );
    const uiImported = /from\s+["']\.\/osl-plugin-runtime["']/u.test(uiMain);
    const uiCalled = uiFunctions.every((name) =>
      new RegExp(`\\b${name}\\s*\\(`, "u").test(uiMain)
    );
    const productionReachable =
      moduleDeclared && commandsRegistered && uiImported && uiCalled;

    expect(moduleDeclared).toBe(false);
    expect(commandsRegistered).toBe(false);
    expect(uiImported).toBe(false);
    expect(uiCalled).toBe(false);
    expect(productionReachable).toBe(false);
    if (!productionReachable) {
      expect(creativeSuite).not.toMatch(CURRENT_PLUGIN_AVAILABILITY_CLAIM);
    }

    expect(
      "The first executable surface is implemented for `.oslmod` command packs.",
    ).toMatch(CURRENT_PLUGIN_AVAILABILITY_CLAIM);
    expect(
      "The current desktop build runs encrypted .oslmod plugins in a WASM sandbox.",
    ).toMatch(CURRENT_PLUGIN_AVAILABILITY_CLAIM);
    expect(creativeSuite).toContain(
      "The current desktop build does not declare the module, register its inspect/run commands, or call its UI adapter",
    );
  });
});
