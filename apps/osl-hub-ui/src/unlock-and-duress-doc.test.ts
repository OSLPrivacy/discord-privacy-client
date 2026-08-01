import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const unlockAndDuress = readFileSync(
  new URL("../../../docs/design/unlock-and-duress.md", import.meta.url),
  "utf8",
);

function markdownSection(source: string, heading: string): string {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() === heading);
  expect(start, `missing ${heading}`).toBeGreaterThanOrEqual(0);
  const end = lines.findIndex(
    (line, index) => index > start && line.startsWith("## "),
  );
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

describe("unlock-and-duress design contract", () => {
  it("T15-T20 documents the password-derived at-rest data key and its recovery consequence", () => {
    const cryptographicRole = markdownSection(
      unlockAndDuress,
      "## Cryptographic role",
    );
    const normalizedRole = cryptographicRole.replace(/\s+/gu, " ");

    expect(cryptographicRole).toContain("derive_file_storage_key");
    expect(normalizedRole).toContain(
      "without the unlock password or recovery phrase, encrypted local data cannot be decrypted",
    );
    expect(normalizedRole).toMatch(
      /recovery phrase.*wrapped copy of the file-storage key/iu,
    );
  });
});
