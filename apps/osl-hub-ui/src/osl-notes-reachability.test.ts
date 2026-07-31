import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const CURRENT_NOTES_AVAILABILITY_CLAIMS = [
  /\bOSL Notes is (?:the|an?|a free,?)\s+[^.\n]{0,180}\bworkspace\b/iu,
  /\bThe default experience is useful without plugins or cloud accounts\b/iu,
] as const;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("OSL Notes production reachability", () => {
  it("does not sell the implemented-but-unwired Notes workspace", () => {
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const rustNotes = readFileSync(
      new URL("../../osl-hub/src/osl_notes.rs", import.meta.url),
      "utf8",
    );
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const uiNotes = readFileSync(new URL("./osl-notes.ts", import.meta.url), "utf8");
    const architecture = readFileSync(
      new URL("../../../docs/design/osl-notes-architecture.md", import.meta.url),
      "utf8",
    );
    const creativeSuite = readFileSync(
      new URL("../../../docs/design/osl-creative-suite.md", import.meta.url),
      "utf8",
    );
    const checklist = readFileSync(
      new URL("../../../docs/design/osl-internal-build-checklist.md", import.meta.url),
      "utf8",
    );
    const handler = sourceBetween(rustMain, "tauri::generate_handler![", "\n    ]);");
    const routeDeclaration = uiMain.match(/\btype Route\s*=\s*[^;]+;/u)?.[0] ?? "";
    const commandNames = [
      "list_osl_notes",
      "save_osl_note",
      "list_osl_note_revisions",
      "restore_osl_note_revision",
      "list_osl_saved_searches",
      "save_osl_saved_search",
      "delete_osl_saved_search",
      "trash_osl_note",
      "restore_osl_note",
      "permanently_delete_osl_note",
    ];
    const requiredCommandNames = [
      "list_osl_notes",
      "save_osl_note",
      "trash_osl_note",
      "restore_osl_note",
      "permanently_delete_osl_note",
    ];
    const uiFunctions = [
      "listOslNotes",
      "saveOslNote",
      "listOslNoteRevisions",
      "restoreOslNoteRevision",
      "listOslSavedSearches",
      "saveOslSavedSearch",
      "deleteOslSavedSearch",
      "trashOslNote",
      "restoreOslNote",
      "permanentlyDeleteOslNote",
    ];
    const requiredUiFunctions = [
      "listOslNotes",
      "saveOslNote",
      "trashOslNote",
      "restoreOslNote",
      "permanentlyDeleteOslNote",
    ];

    expect(rustNotes).toContain("pub fn list()");
    expect(rustNotes).toContain("pub fn upsert(");
    expect(rustNotes).toContain("pub fn trash(");
    expect(rustNotes).toContain("pub fn restore(");
    expect(rustNotes).toContain("pub fn permanently_delete(");
    expect(rustNotes).toContain("ipc::main_password::encrypt_at_rest(");
    expect(rustNotes).toContain("ipc::main_password::decrypt_at_rest(");
    expect(rustNotes).toContain("crate::atomic_file::write_recoverable(");
    for (const command of commandNames) expect(uiNotes).toContain(`"${command}"`);
    expect(uiNotes).toContain("export function notesWorkspaceMarkup(");

    const moduleDeclared = /\bpub mod osl_notes\s*;/u.test(rustLib);
    const anyNativeCommandDefined = commandNames.some((command) =>
      new RegExp(`\\b(?:async\\s+)?fn\\s+${command}\\s*\\(`, "u").test(rustMain)
    );
    const requiredNativeCommandsDefined = requiredCommandNames.every((command) =>
      new RegExp(`\\b(?:async\\s+)?fn\\s+${command}\\s*\\(`, "u").test(rustMain)
    );
    const anyCommandRegistered = commandNames.some((command) =>
      new RegExp(`\\b${command}\\b`, "u").test(handler)
    );
    const requiredCommandsRegistered = requiredCommandNames.every((command) =>
      new RegExp(`\\b${command}\\b`, "u").test(handler)
    );
    const uiImported = /from\s+["']\.\/osl-notes["']/u.test(uiMain);
    const anyUiFunctionCalled = uiFunctions.some((name) =>
      new RegExp(`\\b${name}\\s*\\(`, "u").test(uiMain)
    );
    const requiredUiFunctionsCalled = requiredUiFunctions.every((name) =>
      new RegExp(`\\b${name}\\s*\\(`, "u").test(uiMain)
    );
    const notesRouteDeclared = /["']osl-notes["']/u.test(routeDeclaration);
    const productionReachable =
      moduleDeclared
      && requiredNativeCommandsDefined
      && requiredCommandsRegistered
      && uiImported
      && requiredUiFunctionsCalled
      && notesRouteDeclared;

    expect(routeDeclaration).not.toBe("");
    expect(moduleDeclared).toBe(false);
    expect(anyNativeCommandDefined).toBe(false);
    expect(requiredNativeCommandsDefined).toBe(false);
    for (const command of commandNames) {
      expect(handler).not.toMatch(new RegExp(`\\b${command}\\b`, "u"));
    }
    expect(anyCommandRegistered).toBe(false);
    expect(requiredCommandsRegistered).toBe(false);
    expect(uiImported).toBe(false);
    for (const name of uiFunctions) {
      expect(uiMain).not.toMatch(new RegExp(`\\b${name}\\s*\\(`, "u"));
    }
    expect(anyUiFunctionCalled).toBe(false);
    expect(requiredUiFunctionsCalled).toBe(false);
    expect(notesRouteDeclared).toBe(false);
    expect(productionReachable).toBe(false);

    const publicClaims = `${architecture}\n${creativeSuite}`;
    if (!productionReachable) {
      for (const claim of CURRENT_NOTES_AVAILABILITY_CLAIMS) {
        expect(publicClaims).not.toMatch(claim);
      }
    }

    expect(uiMain).toContain('{ id: "osl-notes", name: "OSL Notes", available: false }');
    expect(uiMain).toContain("OSL Notes is planned for a later release");
    expect(architecture).toContain(
      "The planned OSL Notes product is intended to be a free, local-first",
    );
    expect(architecture).toContain(
      "OSL Notes is not exposed by the current production UI",
    );
    expect(creativeSuite).toContain(
      "its production renderer neither imports nor calls the Notes wrappers",
    );
    expect(creativeSuite).toContain(
      "source prototypes, not shipping product behavior",
    );
    expect(checklist).toContain(
      "Basic notes/link/search/file UX** — partial specialized work; Hub integration unproved.",
    );

    const formerClaims = [
      "OSL Notes is a free, local-first knowledge and creative workspace inside OSL.",
      "The default experience is useful without plugins or cloud accounts.",
      "OSL Notes is the local encrypted workspace for writing and project files.",
    ];
    for (const mutation of formerClaims) {
      expect(
        CURRENT_NOTES_AVAILABILITY_CLAIMS.some((claim) => claim.test(mutation)),
        `current Notes claim escaped the mutation gate: ${mutation}`,
      ).toBe(true);
    }
    for (const planned of [
      "The planned OSL Notes product is intended to be a local-first workspace.",
      "This document specifies the planned OSL Notes product: a local encrypted workspace.",
    ]) {
      expect(
        CURRENT_NOTES_AVAILABILITY_CLAIMS.some((claim) => claim.test(planned)),
        `planned Notes wording was rejected: ${planned}`,
      ).toBe(false);
    }
  });
});
