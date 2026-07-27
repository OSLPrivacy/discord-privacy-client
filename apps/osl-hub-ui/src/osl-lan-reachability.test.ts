import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CURRENT_LAN_AVAILABILITY_CLAIM =
  /\bIt does include\b[^.\n]{0,1200}\bsame[- ]LAN collaboration\b|\bLAN rooms?\b[^.\n]{0,160}\b(?:are|is)\s+(?:implemented|available|included|shipping)\b|\bFree collaboration\b[^.\n]{0,160}\buses\b[^.\n]{0,160}\bLAN rooms?\b/iu;

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
});
