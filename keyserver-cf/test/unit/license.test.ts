import { describe, expect, it } from "vitest";
import {
  generateLicenseKey,
  hashLicense,
  normalizeLicense,
  validateChecksum,
} from "../../src/lib/license.js";

const SECRET = "osl-license-test-secret-v1";

describe("license format", () => {
  it("matches OSL-XXXX-XXXX-XXXX-XXXX with Crockford alphabet", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    expect(plaintext).toMatch(/^OSL-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/);
    // 4 ("OSL-") + 4 (data) + 1 (dash) + 4 + 1 + 4 + 1 + 4 = 23
    expect(plaintext.length).toBe(23);
  });

  it("hash is 64 lowercase hex chars", async () => {
    const { hash } = await generateLicenseKey(SECRET);
    expect(hash).toMatch(/^[0-9a-f]{64}$/);
  });

  it("hash matches independent SHA-256 of plaintext", async () => {
    const { plaintext, hash } = await generateLicenseKey(SECRET);
    expect(await hashLicense(plaintext)).toBe(hash);
  });

  it("generates distinct keys across 1000 iterations", async () => {
    const seen = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      const { plaintext } = await generateLicenseKey(SECRET);
      seen.add(plaintext);
    }
    expect(seen.size).toBe(1000);
  });
});

describe("validateChecksum", () => {
  it("verifies a freshly generated license", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    expect(await validateChecksum(plaintext, SECRET)).toBe(true);
  });

  it("rejects a license with a flipped body char (typo)", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    // Find a body char to mutate (first char after "OSL-").
    const orig = plaintext[4]!;
    const replacement = orig === "2" ? "3" : "2";
    const typo = plaintext.slice(0, 4) + replacement + plaintext.slice(5);
    expect(await validateChecksum(typo, SECRET)).toBe(false);
  });

  it("rejects a license with a flipped checksum char", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    // Last char of plaintext is the second checksum char.
    const orig = plaintext[plaintext.length - 1]!;
    const replacement = orig === "2" ? "3" : "2";
    const tampered =
      plaintext.slice(0, plaintext.length - 1) + replacement;
    expect(await validateChecksum(tampered, SECRET)).toBe(false);
  });

  it("rejects a license generated under a different secret", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    expect(await validateChecksum(plaintext, "different-secret")).toBe(false);
  });

  it("rejects malformed input", async () => {
    expect(await validateChecksum("not-a-license", SECRET)).toBe(false);
    expect(await validateChecksum("", SECRET)).toBe(false);
    expect(await validateChecksum("OSL-XX", SECRET)).toBe(false);
  });
});

describe("normalizeLicense", () => {
  it("collapses Crockford ambiguous chars to canonical", () => {
    // O→0, I→1, L→1; case-insensitive.
    expect(normalizeLicense("osl-Oloo-ILll-2222-3333")).toBe(
      "OSL-0100-1111-2222-3333",
    );
  });

  it("strips dashes / spaces / case", () => {
    expect(normalizeLicense("osl 1234 5678 90AB CDEF")).toBe(
      "OSL-1234-5678-90AB-CDEF",
    );
  });

  it("returns null for wrong-length input", () => {
    expect(normalizeLicense("OSL-1234")).toBe(null);
    expect(normalizeLicense("OSL-1234-5678-90AB-CDEFX")).toBe(null);
  });

  it("returns null for non-Crockford chars after normalisation", () => {
    // 'U' is not in Crockford alphabet (and isn't normalised away).
    expect(normalizeLicense("OSL-UUUU-UUUU-UUUU-UUUU")).toBe(null);
  });

  it("makes validateChecksum tolerate user-style input", async () => {
    const { plaintext } = await generateLicenseKey(SECRET);
    const noisy = plaintext.toLowerCase().replace(/-/g, " ");
    expect(await validateChecksum(noisy, SECRET)).toBe(true);
  });
});
