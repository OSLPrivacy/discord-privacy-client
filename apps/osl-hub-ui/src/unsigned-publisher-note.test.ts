// TASK 1603 - the unsigned / unknown-publisher install note.
//
// The finish line is checked here clause by clause:
//
//   * both warnings are named -- the unsigned-app block and the
//     unknown-publisher line -- in Windows' own words;
//   * exactly one sentence says neither of them is proof the installer is
//     unsafe;
//   * the exact checksum command and the exact expected value are both on the
//     note, and the expected value is a hash that was actually measured, not a
//     string typed into a source file;
//   * the note carries zero promises that Windows will trust OSL, counted by
//     the scanner rather than read off the prose -- and the sabotage tests
//     prove that counter can go red.

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import {
  CHECKSUM_LIMITS,
  NEITHER_IS_PROOF_SENTENCE,
  UNSIGNED_PUBLISHER_NOTE_TITLE,
  WINDOWS_INSTALL_WARNINGS,
  checksumCommand,
  checksumListLine,
  expectedHashValue,
  renderUnsignedPublisherNote,
  sentences,
  unsignedPublisherNoteMarkup,
  unsignedPublisherNotePromises,
  unsignedPublisherNoteRelease,
  unsignedPublisherNoteText,
} from "./unsigned-publisher-note";

/**
 * A real installer's bytes, hashed here. Nothing in this suite states a
 * checksum it did not compute.
 */
const INSTALLER_BYTES = Buffer.from(
  ["OSL installer fixture", "version=2.0.0", "build_fingerprint=MAPLE-4172", ""].join("\n"),
  "utf8",
);
const MEASURED_SHA256 = createHash("sha256").update(INSTALLER_BYTES).digest("hex");
const RELEASE = unsignedPublisherNoteRelease("OSL-2.0.0.exe", MEASURED_SHA256);

describe("TASK 1603 unsigned-publisher install note", () => {
  it("names both the unsigned warning and the unknown-publisher warning", () => {
    const text = unsignedPublisherNoteText(RELEASE);
    const markup = unsignedPublisherNoteMarkup(RELEASE);

    expect(WINDOWS_INSTALL_WARNINGS.map((warning) => warning.id)).toEqual(["unsigned", "unknown-publisher"]);
    expect(text).toContain("Windows protected your PC");
    expect(text).toContain("Unknown publisher");
    expect(text).toContain("unsigned-app warning");
    expect(text).toContain("unknown-publisher warning");
    // Two different dialogs at two different moments, not one warning told twice.
    expect(text).toContain("Microsoft Defender SmartScreen");
    expect(text).toContain("User Account Control");
    expect(markup).toContain('data-warning="unsigned"');
    expect(markup).toContain('data-warning="unknown-publisher"');
    expect(text).toContain("unsigned");
  });

  it("says in exactly one sentence that neither warning is proof the installer is unsafe", () => {
    const text = unsignedPublisherNoteText(RELEASE);
    const claims = sentences(text).filter(
      (sentence) => /neither/iu.test(sentence) && /proof/iu.test(sentence) && /unsafe/iu.test(sentence),
    );

    expect(claims).toHaveLength(1);
    expect(claims[0]).toBe(NEITHER_IS_PROOF_SENTENCE);
    // One sentence, not a paragraph wearing one sentence's clothes.
    expect(sentences(NEITHER_IS_PROOF_SENTENCE)).toHaveLength(1);
  });

  it("gives the exact checksum command and the exact expected value", () => {
    const command = checksumCommand(RELEASE.installerName);
    const markup = unsignedPublisherNoteMarkup(RELEASE);

    expect(command).toBe("Get-FileHash -Algorithm SHA256 -LiteralPath .\\OSL-2.0.0.exe");
    expect(expectedHashValue(RELEASE)).toBe(MEASURED_SHA256.toUpperCase());
    expect(checksumListLine(RELEASE)).toBe(`${MEASURED_SHA256}  OSL-2.0.0.exe`);
    expect(markup).toContain(`<code>${command}</code>`);
    expect(markup).toContain(`<code>${MEASURED_SHA256.toUpperCase()}</code>`);
    expect(markup).toContain(`<code>${MEASURED_SHA256}  OSL-2.0.0.exe</code>`);
    expect(markup).toContain(`data-expected-sha256="${MEASURED_SHA256}"`);
  });

  it("carries zero promises that Windows will trust the installer", () => {
    const promises = unsignedPublisherNotePromises(unsignedPublisherNoteText(RELEASE));

    expect(promises).toEqual([]);
    expect(unsignedPublisherNoteMarkup(RELEASE)).toContain('data-promise-count="0"');
    // It says the opposite, plainly.
    expect(CHECKSUM_LIMITS.join(" ")).toContain("does not remove either warning");
    expect(CHECKSUM_LIMITS.join(" ")).toContain("blocked outright");
  });

  it("sabotage: the promise scanner goes red on each promise it exists to stop", () => {
    const honest = unsignedPublisherNoteText(RELEASE);
    const promised = [
      ["windows-will-trust", "Install it once and Windows will trust OSL from then on."],
      ["warning-will-clear", "The SmartScreen warning will disappear after a few installs."],
      ["no-warning-promised", "Later releases arrive with no warnings at all."],
      ["will-not-warn", "After this, Windows will not warn you again."],
      ["becomes-trusted", "By the next release the installer is trusted."],
      ["verified-publisher", "Windows shows OSL as a verified publisher."],
      ["safe-to-bypass", "It is safe to ignore both of these boxes."],
      ["guaranteed-safe", "We guarantee this installer is safe."],
      ["reputation-will-fix-it", "Its reputation will settle down shortly."],
    ] as const;

    for (const [id, sentence] of promised) {
      expect(unsignedPublisherNotePromises(`${honest}\n${sentence}`)).toContain(id);
    }
    // Put it back: the honest text is clean again.
    expect(unsignedPublisherNotePromises(honest)).toEqual([]);
  });

  it("sabotage: a checksum nobody measured cannot reach the note", () => {
    expect(() => unsignedPublisherNoteRelease("OSL-2.0.0.exe", "")).toThrow(/64-character hex SHA-256/u);
    expect(() => unsignedPublisherNoteRelease("OSL-2.0.0.exe", "coming soon")).toThrow(/64-character hex SHA-256/u);
    expect(() => unsignedPublisherNoteRelease("OSL-2.0.0.exe", MEASURED_SHA256.slice(0, 63))).toThrow(
      /64-character hex SHA-256/u,
    );
    expect(() => unsignedPublisherNoteRelease("OSL-2.0.0.exe", `${MEASURED_SHA256}0`)).toThrow(
      /64-character hex SHA-256/u,
    );
    expect(() => unsignedPublisherNoteRelease("osl-setup.msi", MEASURED_SHA256)).toThrow(/release installer name/u);
    // Upper-case input from Get-FileHash is normalised, not rejected.
    expect(unsignedPublisherNoteRelease("OSL-2.0.0.exe", MEASURED_SHA256.toUpperCase()).sha256).toBe(MEASURED_SHA256);
  });

  it("is read-only text with nothing to press", () => {
    // There is no DOM in this suite (no jsdom in this package), so the markup is
    // checked as text here and mounted for real in the browser capture,
    // screenshots/task-1603-unsigned-publisher-note-capture.test.mjs.
    const markup = unsignedPublisherNoteMarkup(RELEASE);

    expect(markup).toContain(`<h1 id="unsigned-publisher-note-title">${UNSIGNED_PUBLISHER_NOTE_TITLE}</h1>`);
    expect(markup).not.toMatch(/<button|<a\s|<input|<form/u);
    expect(typeof renderUnsignedPublisherNote).toBe("function");
  });
});
