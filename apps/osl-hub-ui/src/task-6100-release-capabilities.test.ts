import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  releaseCapabilitiesMarkup,
  releaseCapabilityMatrix,
  RELEASE_CAPABILITY_COPY,
  RELEASE_CONTENT_TYPES,
  RELEASE_INTEGRATIONS,
  SHIPPING_CARRIERS,
  SHIPPING_IMPLEMENTATION_ID,
  SHIPPING_STRIP_ADAPTERS,
} from "./release-capabilities";

const source = readFileSync(new URL("./release-capabilities.ts", import.meta.url), "utf8");

describe("TASK 6100 release capabilities surface", () => {
  it("keeps the 6101 frozen implementation id unchanged", () => {
    expect(SHIPPING_IMPLEMENTATION_ID).toBe("native.discord.text.v1");
    expect(source).toContain('"native.discord.text.v1"');
  });

  it("exports the exact 6101 carrier list row for row", () => {
    expect(SHIPPING_CARRIERS).toEqual(["Discord"]);
  });

  it("exports the exact 6101 Strip adapter list row for row", () => {
    expect(SHIPPING_STRIP_ADAPTERS).toEqual(["Discord"]);
  });

  it("renders the carrier-list sentence with no literal brackets", () => {
    expect(RELEASE_CAPABILITY_COPY.carrierListSentence).toBe(
      "This release can send through: Discord. All other integrations are unavailable and are not covered by send, offline, Tor or protection tests.",
    );
    expect(RELEASE_CAPABILITY_COPY.carrierListSentence).not.toMatch(/\[|]/);
  });

  it("renders the matrix-explanation sentence exactly", () => {
    expect(RELEASE_CAPABILITY_COPY.matrixExplanationSentence).toBe(
      "For each integration, only the content types marked Supported are covered by send, offline, Tor, and protection tests; text support does not imply attachment, paste, share, or streaming support.",
    );
  });

  it("renders the Strip help sentence with no literal brackets", () => {
    expect(RELEASE_CAPABILITY_COPY.stripHelpSentence).toBe(
      "Quick settings are verified only for: Discord. On every other integration, use full Settings; Strip controls may be unavailable.",
    );
    expect(RELEASE_CAPABILITY_COPY.stripHelpSentence).not.toMatch(/\[|]/);
  });

  it("renders the notification help sentence exactly", () => {
    expect(RELEASE_CAPABILITY_COPY.notificationHelpSentence).toBe(
      "Notification actions are informational shortcuts; confirm security, recovery and payment state inside the app before acting.",
    );
  });

  it("renders the independent-export sentence exactly", () => {
    expect(RELEASE_CAPABILITY_COPY.independentExportSentence).toBe(
      "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL.",
    );
  });

  it("renders the export key-storage sentence exactly", () => {
    expect(RELEASE_CAPABILITY_COPY.exportKeyStorageSentence).toBe(
      "Storing the archive and its key together defeats the encryption. Keep the key in a different protected location; anyone who obtains both can read the export.",
    );
  });

  it("renders the recovery-kit theft sentence exactly", () => {
    expect(RELEASE_CAPABILITY_COPY.recoveryKitTheftSentence).toBe(
      "Anyone who obtains this recovery kit may race to take over the account and revoke your devices. Store it encrypted and offline.",
    );
  });

  it("renders the succession sentence with resolved placeholders and no literal brackets", () => {
    const sentence = RELEASE_CAPABILITY_COPY.successionSentence("30 days", "Recovery contact");
    expect(sentence).toBe(
      "After 30 days without a successfully authenticated foreground owner action, ownership transfers automatically to Recovery contact and you may lose owner access. Background sync does not reset this timer.",
    );
    expect(sentence).not.toMatch(/\[|]/);
  });

  it("produces the complete 35-row matrix with exactly one Supported cell", () => {
    const matrix = releaseCapabilityMatrix();
    expect(matrix).toHaveLength(35);
    const supported = matrix.filter((cell) => cell.status === "Supported");
    expect(supported).toHaveLength(1);
    expect(supported[0]).toMatchObject({ integration: "discord", contentType: "text", status: "Supported" });
  });

  it("keeps the matrix row order equal to the frozen integration and content-type lists", () => {
    const matrix = releaseCapabilityMatrix();
    let index = 0;
    for (const integration of RELEASE_INTEGRATIONS) {
      for (const contentType of RELEASE_CONTENT_TYPES) {
        expect(matrix[index]).toMatchObject({ integration: integration.id, contentType: contentType.id });
        index += 1;
      }
    }
  });

  it("renders the surface with the exact carrier sentence, matrix, and adjacent note", () => {
    const markup = releaseCapabilitiesMarkup();
    expect(markup).toContain(RELEASE_CAPABILITY_COPY.carrierListSentence);
    expect(markup).toContain(RELEASE_CAPABILITY_COPY.matrixExplanationSentence);
    expect(markup).toContain('<table class="release-capability-matrix"');
    expect(markup).toContain('<td data-integration="discord" data-content-type="text" data-status="Supported">Supported</td>');
    expect(markup).toContain('<td data-integration="telegram" data-content-type="text" data-status="Unsupported">Unsupported</td>');
    const matrixIndex = markup.indexOf('<table class="release-capability-matrix"');
    const noteIndex = markup.indexOf(RELEASE_CAPABILITY_COPY.matrixExplanationSentence);
    expect(noteIndex).toBeGreaterThan(matrixIndex);
  });

  it("is reachable through the packaged Settings sidebar", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(main).toContain('"release-capabilities"');
    expect(main).toContain("releaseCapabilitiesMarkup()");
    expect(main).toContain('["release-capabilities", "Release capabilities"]');
  });
});
