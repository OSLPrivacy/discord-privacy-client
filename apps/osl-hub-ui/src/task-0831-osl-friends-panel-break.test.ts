// TASK 0831 - break OSL Friends panel.
//
// Task 0830's friend-row check reads each Home row off the drawn panel and requires the row to
// carry its own friend's name and its own friend's avatar. This task proves that check can go red:
// a *test copy* of the panel module is made with one deliberate identity mix - the avatar drawn in
// each row is taken from the next friend's picture instead of the row's own - and 0830's check is
// pointed at that copy and has to exit 1.
//
// How the copy is made, and why it is honest:
//
//   * The shipped panel module (`osl-friends-panel-routing.ts`) is COPIED to a scratch directory
//     and mutated there. The product file is never written to; its sha256 is recorded before and
//     after and compared.
//   * 0830's test file is copied BYTE FOR BYTE - the copy's sha256 is asserted equal to the
//     original's. It imports `./osl-friends-panel-routing`, so in the scratch directory it resolves
//     to the copy sitting next to it. The red run and 0830's own green run are therefore literally
//     the same check text, differing only in which panel module it loads.
//   * Two scratch copies are built: `mixed` (the identity mix applied) and `clean` (the module
//     copied unchanged). Each is run in its own child `vitest run` and its exit code read. The
//     clean copy is the control: it proves the copying machinery itself is not what turns the
//     check red, so exit 1 from the mixed copy is caused by the mixed identity data and nothing
//     else.
//
// The scratch directories are created inside the run and deleted in `finally`, so nothing that
// fails is left behind in `src/` for another run to collect.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const SRC_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_DIR = resolve(SRC_DIR, "..");
const VITEST_BIN = join(PACKAGE_DIR, "node_modules", ".bin", "vitest");

/** The shipped panel module the copy is taken from. It is only ever read. */
const PANEL_MODULE = join(SRC_DIR, "osl-friends-panel-routing.ts");
/** Task 0830's friend-row check. It is only ever read, and copied byte for byte. */
const FRIEND_ROW_CHECK = join(SRC_DIR, "task-0830-osl-friends-panel-rows.test.ts");

/** The two friends 0830 created, and the picture each one has. Used only to read the child's log. */
const ADA_ROW_ID = "friend:f8e88443aa9253d65fd9fe5e9c05b855407d0133446703dd45408d0567b8e7c7";
const CLEO_ROW_ID = "friend:065a8d10a8f0d9b449c53523ce8b36b8880f2fdbf618903dd017d66a075dda72";
/** Ada Friend's permitted picture - the one that must never appear in Cleo Friend's row. */
const ADA_PICTURE = "data:image/gif;base64,R0lGODlhAQABAIAAAAD/ACwAAAAAAQABAAACAkQBADs=";

function sha256(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

function countOf(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** The row-drawing call site in the shipped module, and the mixed-identity version of it. */
const ROW_MAP_BEFORE = "const list = rows.map((row) =>";
const ROW_MAP_AFTER = "const list = rows.map((row, index) =>";
const AVATAR_BEFORE = "${friendAvatarMarkup(row)}";
const AVATAR_AFTER = "${friendAvatarMarkup(pictureOfAnotherFriend(row, rows, index))}";

/**
 * THE BREAK. One friend's picture is returned in another friend's row: the avatar for the row at
 * `index` is drawn from the picture stored on the NEXT friend's row, while the id, name, initial
 * and colour stay the row's own. That is mixed identity data - a row whose name says one friend
 * and whose face is another's.
 */
const MIXING_HELPER = `
/** TASK 0831 break: draw this row with the *next* friend's picture instead of its own. */
function pictureOfAnotherFriend(
  row: HomeFriendRow,
  rows: readonly HomeFriendRow[],
  index: number,
): HomeFriendRow {
  const other = rows[(index + 1) % rows.length];
  return { ...row, picture: other.picture, pictureStatus: other.pictureStatus };
}
`;

function mixIdentities(moduleSource: string): string {
  expect(countOf(moduleSource, ROW_MAP_BEFORE)).toBe(1);
  expect(countOf(moduleSource, AVATAR_BEFORE)).toBe(1);
  const mixed = moduleSource
    .replace(ROW_MAP_BEFORE, ROW_MAP_AFTER)
    .replace(AVATAR_BEFORE, AVATAR_AFTER) + MIXING_HELPER;
  expect(mixed).not.toBe(moduleSource);
  expect(countOf(mixed, AVATAR_AFTER)).toBe(1);
  return mixed;
}

interface ScratchRun {
  directory: string;
  /** The exit code of the child `vitest run` over the copied check. */
  status: number;
  output: string;
  /** sha256 of the copied check file - proves it is 0830's check unchanged. */
  checkSha: string;
  /** sha256 of the copied panel module. */
  moduleSha: string;
}

const scratchDirectories: string[] = [];

/**
 * Build one scratch copy - 0830's check byte for byte, next to a copy of the panel module, either
 * mixed or clean - and run the check over it in a child process. Returns that child's exit code.
 */
function runCopiedCheck(name: string, panelSource: string): ScratchRun {
  const directory = join(SRC_DIR, `task-0831-scratch-${name}`);
  scratchDirectories.push(directory);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory, { recursive: true });

  const checkSource = readFileSync(FRIEND_ROW_CHECK, "utf8");
  writeFileSync(join(directory, "osl-friends-panel-routing.ts"), panelSource);
  writeFileSync(join(directory, "friend-row-check.test.ts"), checkSource);

  const child = spawnSync(
    VITEST_BIN,
    ["run", "--reporter=basic", `task-0831-scratch-${name}/friend-row-check`],
    { cwd: PACKAGE_DIR, encoding: "utf8", env: { ...process.env, CI: "1", NO_COLOR: "1" } },
  );
  const output = `${child.stdout ?? ""}${child.stderr ?? ""}`;
  return {
    directory,
    status: child.status ?? -1,
    output,
    checkSha: sha256(checkSource),
    moduleSha: sha256(panelSource),
  };
}

/** Pull the child's own `TASK_0830 ... row friend_id=<id> ...` line for one row out of its log. */
function rowLogLine(output: string, friendId: string): string {
  const line = output
    .split("\n")
    .find((candidate) =>
      candidate.includes("fixture=permitted-picture-rule")
      && candidate.includes(`row friend_id=${friendId}`)
    );
  return line?.trim() ?? "";
}

describe("TASK 0831 - break OSL Friends panel", () => {
  const shippedPanelSource = readFileSync(PANEL_MODULE, "utf8");
  const shippedPanelSha = sha256(shippedPanelSource);
  const shippedCheckSha = sha256(readFileSync(FRIEND_ROW_CHECK, "utf8"));

  afterAll(() => {
    for (const directory of scratchDirectories) rmSync(directory, { recursive: true, force: true });
  });

  it("the friend-row check exits 1 for mixed identity data, and 0 for the same copy unmixed", () => {
    const clean = runCopiedCheck("clean", shippedPanelSource);
    const mixed = runCopiedCheck("mixed", mixIdentities(shippedPanelSource));

    console.log(
      `TASK_0831 clean_copy_exit=${clean.status} mixed_copy_exit=${mixed.status}`
      + ` check_copied_byte_for_byte=${clean.checkSha === shippedCheckSha && mixed.checkSha === shippedCheckSha}`
      + ` clean_module_sha_matches_shipped=${clean.moduleSha === shippedPanelSha}`
      + ` mixed_module_sha_matches_shipped=${mixed.moduleSha === shippedPanelSha}`,
    );
    console.log(`TASK_0831 mixed_copy_cleo_row=${rowLogLine(mixed.output, CLEO_ROW_ID)}`);
    console.log(`TASK_0831 mixed_copy_ada_row=${rowLogLine(mixed.output, ADA_ROW_ID)}`);
    console.log(`TASK_0831 clean_copy_cleo_row=${rowLogLine(clean.output, CLEO_ROW_ID)}`);

    // The check the child ran is 0830's check, byte for byte, in both runs.
    expect(clean.checkSha).toBe(shippedCheckSha);
    expect(mixed.checkSha).toBe(shippedCheckSha);
    // The only difference between the two children is the panel module beside that check.
    expect(clean.moduleSha).toBe(shippedPanelSha);
    expect(mixed.moduleSha).not.toBe(shippedPanelSha);

    // THE FINISH LINE: mixed identity data makes the friend-row check exit 1.
    expect(mixed.status).toBe(1);
    // ... and the identical copy without the mix exits 0, so exit 1 is the mix, not the copying.
    expect(clean.status).toBe(0);
  }, 180_000);

  it("the mixed copy really does put one friend's picture in the other friend's row", () => {
    const mixed = runCopiedCheck("mixed-read", mixIdentities(shippedPanelSource));
    const cleoRow = rowLogLine(mixed.output, CLEO_ROW_ID);
    const adaRow = rowLogLine(mixed.output, ADA_ROW_ID);

    console.log(
      `TASK_0831 mixed_exit=${mixed.status}`
      + ` ada_picture_in_cleo_row=${cleoRow.includes(`picture_src=${ADA_PICTURE}`)}`
      + ` cleo_row_names=${cleoRow.includes("name=Cleo Friend")}`
      + ` ada_row_lost_her_picture=${adaRow.includes("picture_src=none")}`,
    );
    console.log(`TASK_0831 mixed_first_failure=${
      mixed.output.split("\n").find((line) => line.includes("AssertionError"))?.trim() ?? "none"
    }`);

    expect(mixed.status).toBe(1);
    // Cleo Friend's row - her id, her name - is drawn with Ada Friend's picture.
    expect(cleoRow).toContain("name=Cleo Friend");
    expect(cleoRow).toContain(`picture_src=${ADA_PICTURE}`);
    // And Ada Friend's own row lost her picture to the mix.
    expect(adaRow).toContain("name=Ada Friend");
    expect(adaRow).toContain("picture_src=none");
    // The child reported failures, not an error collecting or importing.
    expect(mixed.output).toContain("AssertionError");
    expect(mixed.output).not.toContain("Failed to load");
  }, 180_000);

  it("the shipped panel module was never written to, and the scratch copies are gone after", () => {
    const after = sha256(readFileSync(PANEL_MODULE, "utf8"));
    console.log(
      `TASK_0831 shipped_module_sha_before=${shippedPanelSha.slice(0, 16)}`
      + ` shipped_module_sha_after=${after.slice(0, 16)} unchanged=${after === shippedPanelSha}`
      + ` scratch_dirs=${scratchDirectories.length}`,
    );
    expect(after).toBe(shippedPanelSha);
    // Every scratch directory this file made is inside src/ and named for this task, so the
    // afterAll cleanup can only ever remove copies it created.
    for (const directory of scratchDirectories) {
      expect(directory.startsWith(join(SRC_DIR, "task-0831-scratch-"))).toBe(true);
    }
  });
});
