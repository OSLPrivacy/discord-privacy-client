import { describe, expect, it } from "vitest";
import { providerLogo, serviceLogo } from "./logos";

describe("bundled service marks", () => {
  it("renders every launch service without a network asset", () => {
    for (const id of ["discord", "telegram", "instagram", "snapchat", "email", "x", "messenger", "signal", "whatsapp", "slack", "linkedin", "teams"] as const) {
      const logo = serviceLogo(id);
      expect(logo).toContain("<svg");
      expect(logo).not.toMatch(/https?:\/\//);
    }
  });

  it("renders all fixed email providers locally", () => {
    for (const id of ["gmail", "outlook", "proton", "fastmail", "yahoo", "aol", "gmx", "maildotcom", "icloud"]) {
      expect(providerLogo(id)).toContain("<svg");
    }
  });

  it("uses clean vector provider marks instead of the old four-square placeholder", () => {
    const outlook = providerLogo("outlook");
    const proton = providerLogo("proton");
    expect(outlook).toContain('aria-label="Microsoft Outlook"');
    expect(outlook).toContain("provider-logo-outlook");
    expect(outlook).toContain('fill="#1490df"');
    expect(proton).toContain('aria-label="Proton Mail"');
    expect(proton).toContain("provider-logo-proton");
    expect(proton).toContain('fill="#6d4aff"');
    expect(proton).not.toContain('fill="currentColor"');
    expect(providerLogo("yahoo")).toContain('aria-label="Yahoo Mail"');
    expect(providerLogo("aol")).not.toContain("<text");
  });
});
