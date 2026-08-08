import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { DECOY_DOCUMENT_TITLE, enterDecoyDocument } from "./stealth-surface";

const PRIVATE_WORKSPACE_NAME = "MAPLE-STEALTH-3665";
const windowsDump = JSON.parse(readFileSync(
  new URL("../screenshots/artifacts/task-3665-windows-screen-tree.json", import.meta.url),
  "utf8",
)) as {
  privateWorkspaceRecordCountBeforeUnlock: number;
  privateWorkspaceName: string;
  enteredStealthPassword: string;
  screenTree: Array<{ name: string }>;
  windowTitles: string[];
};

function occurrences(values: readonly string[], needle: string): number {
  return values.reduce((count, value) => {
    let offset = 0;
    let found = value.indexOf(needle, offset);
    while (found !== -1) {
      count += 1;
      offset = found + needle.length;
      found = value.indexOf(needle, offset);
    }
    return count;
  }, 0);
}

describe("TASK 3665 stealth screen identity", () => {
  it("leaves one neutral decoy name and no private screen or title names", () => {
    const privateWorkspaces = Array.from(
      { length: windowsDump.privateWorkspaceRecordCountBeforeUnlock },
      () => ({ name: windowsDump.privateWorkspaceName }),
    );
    const documentLike = { title: "OSL Privacy" };

    expect(privateWorkspaces).toHaveLength(1);
    expect(windowsDump.enteredStealthPassword).toBe("stealth-3665");
    enterDecoyDocument(documentLike);

    const windowsScreenTree = windowsDump.screenTree.map((node) => node.name);
    const dumpedSurface = [...windowsScreenTree, ...windowsDump.windowTitles];
    expect(windowsScreenTree[0]).toBe(documentLike.title);
    const oslCount = occurrences(dumpedSurface, "OSL");
    const privateNameCount = occurrences(dumpedSurface, PRIVATE_WORKSPACE_NAME);
    const decoyCount = occurrences(dumpedSurface, DECOY_DOCUMENT_TITLE);

    console.info(`TASK3665_PRIVATE_WORKSPACE_RECORD_COUNT=${privateWorkspaces.length}`);
    console.info(`TASK3665_WINDOWS_SCREEN_TREE=${JSON.stringify(windowsScreenTree)}`);
    console.info(`TASK3665_WINDOWS_WINDOW_TITLES=${JSON.stringify(windowsDump.windowTitles)}`);
    console.info(`TASK3665_OSL_OCCURRENCES=${oslCount}`);
    console.info(`TASK3665_PRIVATE_NAME_OCCURRENCES=${privateNameCount}`);
    console.info(`TASK3665_DECOY_WORKSPACE_OCCURRENCES=${decoyCount}`);

    expect(oslCount).toBe(0);
    expect(privateNameCount).toBe(0);
    expect(decoyCount).toBe(1);
  });
});
