import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { blankLocalProtectedModel, localProtectedSheetMarkup } from "./local-protected-sheet";
import { blankPeerProtectedModel, peerProtectedSheetMarkup } from "./peer-protected-sheet";
import {
  bindProtectedTextBoxShortcutGuards,
  COVER_MESSAGE_BOX_RULE,
  preventProtectedTextShortcut,
  PROTECTED_TEXT_BOX_RULE,
  PROTECTED_TEXT_SHORTCUTS,
  type ProtectedTextShortcut,
} from "./protected-box-shortcuts";

interface TextBoxFixture {
  id: string;
  value: string;
  readOnly: boolean;
  protectedRule: string;
  coverRule: string;
}

interface ShortcutObservation {
  boxId: string;
  shortcut: ProtectedTextShortcut;
  privateMark: string;
  prevented: boolean;
  clipboardAfter: string;
  valueAfter: string;
}

const privatePrefix = "OSL3551_PRIVATE_MARK_";

function attributes(source: string): Record<string, string> {
  const parsed: Record<string, string> = {};
  const pattern = /([^\s=/>]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/gu;
  let match = pattern.exec(source);
  while (match !== null) {
    parsed[match[1]] = match[2] ?? match[3] ?? match[4] ?? "";
    match = pattern.exec(source);
  }
  return parsed;
}

function textareas(markup: string): TextBoxFixture[] {
  const fields: TextBoxFixture[] = [];
  const pattern = /<textarea\b([^>]*)>([\s\S]*?)<\/textarea>/gu;
  let match = pattern.exec(markup);
  while (match !== null) {
    const attrs = attributes(match[1] ?? "");
    fields.push({
      id: attrs.id ?? "",
      value: match[2] ?? "",
      readOnly: Object.hasOwn(attrs, "readonly"),
      protectedRule: attrs["data-osl-protected-box-rule"] ?? "",
      coverRule: attrs["data-osl-cover-message-box"] ?? "",
    });
    match = pattern.exec(markup);
  }
  return fields;
}

function fixedProtectedShortcutScreen(): TextBoxFixture[] {
  const local = {
    ...blankLocalProtectedModel(true),
    chatLabel: "Local audit",
    context: {
      contextToken: "ctx-local-3551",
      serviceId: "discord",
      accountId: "account-3551",
      conversationId: "local-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    },
    draft: "local private seed",
    capsule: "DPC0::public-local-cover-3551",
  };
  const peer = {
    ...blankPeerProtectedModel(true),
    displayName: "Peer audit",
    context: {
      contextToken: "ctx-peer-3551",
      serviceId: "discord",
      accountId: "account-3551",
      personId: "person-3551",
      peerOslUserId: "osl-user-3551",
      scopeApproved: true,
    },
    draft: "peer private seed",
    coverText: "public peer cover 3551",
  };
  return [
    ...textareas(localProtectedSheetMarkup(local, "manual")),
    ...textareas(peerProtectedSheetMarkup(peer, [])),
  ];
}

function shortcutKey(shortcut: ProtectedTextShortcut): string {
  return shortcut.slice("Ctrl+".length).toLowerCase();
}

function defaultShortcut(
  box: TextBoxFixture,
  shortcut: ProtectedTextShortcut,
  clipboard: string,
): { box: TextBoxFixture; clipboard: string } {
  if (shortcut === "Ctrl+C") return { box, clipboard: box.value };
  if (shortcut === "Ctrl+X") return { box: { ...box, value: "" }, clipboard: box.value };
  if (shortcut === "Ctrl+V" && !box.readOnly) return { box: { ...box, value: clipboard }, clipboard };
  return { box, clipboard };
}

function exerciseProtectedShortcut(
  box: TextBoxFixture,
  shortcut: ProtectedTextShortcut,
  privateMark: string,
  enforceRule: boolean,
): ShortcutObservation {
  let working = { ...box, value: privateMark };
  let clipboard = "OSL3551_PUBLIC_CLIPBOARD_SEED";
  let prevented = false;
  const event = {
    key: shortcutKey(shortcut),
    ctrlKey: true,
    preventDefault: () => { prevented = true; },
  };
  if (enforceRule) preventProtectedTextShortcut(event);
  if (!prevented) {
    const result = defaultShortcut(working, shortcut, clipboard);
    working = result.box;
    clipboard = result.clipboard;
  }
  return { boxId: box.id, shortcut, privateMark, prevented, clipboardAfter: clipboard, valueAfter: working.value };
}

function countPrivateHits(values: readonly string[], privateMarks: readonly string[]): number {
  let hits = 0;
  for (const value of values) {
    for (const mark of privateMarks) {
      if (value.includes(mark)) hits += 1;
    }
  }
  return hits;
}

describe("task 3551 protected-box shortcuts", () => {
  it("checks every protected box shortcut against the clipboard on the fixed test screen", () => {
    const screen = fixedProtectedShortcutScreen();
    const protectedBoxes = screen.filter((box) => box.protectedRule === PROTECTED_TEXT_BOX_RULE);
    const coverMessageBoxes = screen.filter((box) => box.coverRule === COVER_MESSAGE_BOX_RULE);
    expect(protectedBoxes.length).toBeGreaterThan(0);
    expect(coverMessageBoxes.length).toBeGreaterThan(0);

    let ordinaryClipboard = "";
    const ordinaryMark = "OSL3551_ORDINARY_CLIPBOARD_MARK";
    ordinaryClipboard = defaultShortcut({
      id: "ordinary-control-box",
      value: ordinaryMark,
      readOnly: false,
      protectedRule: "",
      coverRule: "",
    }, "Ctrl+C", ordinaryClipboard).clipboard;
    expect(ordinaryClipboard).toBe(ordinaryMark);

    const observations = protectedBoxes.flatMap((box, index) => {
      const privateMark = `${privatePrefix}${index}_${box.id}`;
      return PROTECTED_TEXT_SHORTCUTS.map((shortcut) => exerciseProtectedShortcut(box, shortcut, privateMark, true));
    });
    const privateMarks = [...new Set(observations.map((entry) => entry.privateMark))];
    const clipboardPrivateMarkHits = countPrivateHits(observations.map((entry) => entry.clipboardAfter), privateMarks);
    const coverMessagePrivateMarkHits = countPrivateHits(coverMessageBoxes.map((box) => box.value), privateMarks);

    for (const box of protectedBoxes) {
      const perBox = observations.filter((entry) => entry.boxId === box.id);
      expect(perBox.map((entry) => entry.shortcut)).toEqual(PROTECTED_TEXT_SHORTCUTS);
      expect(perBox.every((entry) => entry.prevented)).toBe(true);
      expect(perBox.every((entry) => entry.valueAfter === entry.privateMark)).toBe(true);
    }
    expect(clipboardPrivateMarkHits).toBe(0);
    expect(coverMessagePrivateMarkHits).toBe(0);

    const redPath = protectedBoxes.flatMap((box, index) => {
      const privateMark = `${privatePrefix}RED_${index}_${box.id}`;
      return PROTECTED_TEXT_SHORTCUTS.map((shortcut) => exerciseProtectedShortcut(box, shortcut, privateMark, false));
    });
    expect(countPrivateHits(redPath.map((entry) => entry.clipboardAfter), redPath.map((entry) => entry.privateMark))).toBeGreaterThan(0);

    const report = {
      finishLine: "TASK 3551",
      protectedBoxCount: protectedBoxes.length,
      coverMessageBoxCount: coverMessageBoxes.length,
      shortcutsPerBox: protectedBoxes.map((box) => ({ boxId: box.id, shortcuts: PROTECTED_TEXT_SHORTCUTS })),
      ordinaryCopyClipboard: ordinaryClipboard,
      protectedRule: PROTECTED_TEXT_BOX_RULE,
      clipboardPrivateMarkHits,
      coverMessagePrivateMarkHits,
      redPathClipboardPrivateMarkHits: countPrivateHits(redPath.map((entry) => entry.clipboardAfter), redPath.map((entry) => entry.privateMark)),
    };
    console.log(`TASK 3551 protected-box-shortcuts report ${JSON.stringify(report)}`);
  });

  it("binds the runtime guard to the real protected-box selector", () => {
    let prevented = false;
    const protectedBox = {
      addEventListener: vi.fn((type: string, listener: (event: { key: string; ctrlKey?: boolean; preventDefault(): void }) => void) => {
        if (type === "keydown") listener({ key: "c", ctrlKey: true, preventDefault: () => { prevented = true; } });
      }),
    };
    const coverMessageBox = { addEventListener: vi.fn() };
    const root = {
      querySelectorAll: vi.fn((selector: string) => {
        if (selector === `[data-osl-protected-box-rule="${PROTECTED_TEXT_BOX_RULE}"]`) return [protectedBox];
        if (selector === `[data-osl-cover-message-box="${COVER_MESSAGE_BOX_RULE}"]`) return [coverMessageBox];
        return [];
      }),
    };

    expect(bindProtectedTextBoxShortcutGuards(root)).toBe(1);
    expect(prevented).toBe(true);
    expect(protectedBox.addEventListener).toHaveBeenCalledWith("copy", expect.any(Function));
    expect(protectedBox.addEventListener).toHaveBeenCalledWith("cut", expect.any(Function));
    expect(protectedBox.addEventListener).toHaveBeenCalledWith("paste", expect.any(Function));
    expect(coverMessageBox.addEventListener).toHaveBeenCalledWith("paste", expect.any(Function));

    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain("bindProtectedTextBoxShortcutGuards(document)");
  });
});
