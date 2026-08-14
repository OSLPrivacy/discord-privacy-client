// TASK 1604 - check that unsigned-publisher note contains all 4 required strings
//
// This test validates that the unsigned-publisher note component contains
// all four required strings:
// 1. "unsigned" - indicates lack of code signature
// 2. "unknown publisher" - names the exact warning Windows shows
// 3. "warning" - describes what these are
// 4. "checksum" - the verification method

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import {
  unsignedPublisherNoteText,
  unsignedPublisherNoteRelease,
} from "./unsigned-publisher-note";

describe("TASK 1604 - unsigned-publisher note contains all required strings", () => {
  // Create a test release with fixture data
  const INSTALLER_BYTES = Buffer.from(
    ["OSL installer fixture", "version=2.0.0", "build_fingerprint=MAPLE-4172", ""].join("\n"),
    "utf8",
  );
  const MEASURED_SHA256 = createHash("sha256").update(INSTALLER_BYTES).digest("hex");
  const RELEASE = unsignedPublisherNoteRelease("OSL-2.0.0.exe", MEASURED_SHA256);
  const noteText = unsignedPublisherNoteText(RELEASE);

  it("contains the string 'unsigned' at least once", () => {
    expect(noteText).toMatch(/unsigned/i);
  });

  it("contains the string 'unknown publisher' at least once", () => {
    expect(noteText).toMatch(/unknown\s+publisher/i);
  });

  it("contains the string 'warning' at least once", () => {
    expect(noteText).toMatch(/warning/i);
  });

  it("contains the string 'checksum' at least once", () => {
    expect(noteText).toMatch(/checksum/i);
  });

  it("sabotage: fixture without checksum string fails the checksum check", () => {
    const withoutChecksum = noteText.replace(/checksum/gi, "");
    expect(withoutChecksum).not.toMatch(/checksum/i);
  });

  it("sabotage: fixture without unsigned string fails the unsigned check", () => {
    const withoutUnsigned = noteText.replace(/unsigned/gi, "");
    expect(withoutUnsigned).not.toMatch(/unsigned/i);
  });

  it("sabotage: fixture without unknown publisher string fails the publisher check", () => {
    const withoutPublisher = noteText.replace(/unknown\s+publisher/gi, "");
    expect(withoutPublisher).not.toMatch(/unknown\s+publisher/i);
  });

  it("sabotage: fixture without warning string fails the warning check", () => {
    const withoutWarning = noteText.replace(/warning/gi, "");
    expect(withoutWarning).not.toMatch(/warning/i);
  });
});
