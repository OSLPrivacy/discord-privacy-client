import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { homeAppsFromServices, type LinkedService } from "./services";

const carrierIds = ["discord", "telegram", "signal", "whatsapp"] as const;
const mailIds = ["gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud", "tuta"] as const;

function fixtureServices(): LinkedService[] {
  return [
    ...carrierIds.map((id, sidebarOrder) => ({
      id,
      displayName: id,
      sidebarGlyph: id.slice(0, 2),
      sidebarOrder,
      category: "consumer" as const,
      launchState: "available" as const,
      supportsNativePreview: true,
      supportsProtectedPreview: true,
      accounts: [],
    })),
    {
      id: "email" as const,
      displayName: "Email",
      sidebarGlyph: "EM",
      sidebarOrder: carrierIds.length,
      category: "consumer" as const,
      launchState: "available" as const,
      supportsNativePreview: true,
      supportsProtectedPreview: true,
      accounts: mailIds.map((provider) => ({
        id: `mail-${provider}`,
        label: provider,
        displayHandle: "Sign in",
        state: "notLinked" as const,
        provider,
      })),
    },
  ];
}

describe("TASK 5008 Home service rows", () => {
  it("renders exactly the shipped social and mail-service tiles, and a carrier cut removes only its tile", () => {
    const fixture = fixtureServices();
    const catalog = homeAppsFromServices(fixture);
    const social = catalog.filter((app) => app.section === "social");
    const email = catalog.filter((app) => app.section === "email");

    console.log(`TASK5008 social_tiles=${social.length} ids=${social.map((app) => app.id).join(",")}`);
    console.log(`TASK5008 email_tiles=${email.length} ids=${email.map((app) => app.id).join(",")}`);
    expect(social.map((app) => app.id)).toEqual(carrierIds);
    expect(email.map((app) => app.id)).toEqual(mailIds);

    const cut = homeAppsFromServices(fixture.filter((service) => service.id !== "telegram"));
    const cutSocial = cut.filter((app) => app.section === "social");
    console.log(`TASK5008 cut_social_tiles=${cutSocial.length} removed=telegram`);
    expect(cutSocial.map((app) => app.id)).toEqual(["discord", "signal", "whatsapp"]);
    expect(cut.filter((app) => app.section === "email")).toHaveLength(9);
  });

  it("keeps data-driven row markup circular, editable, and bound to the named Strip launcher", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(main).toContain('<h2>Social</h2>');
    expect(main).toContain('<h2>Email</h2>');
    expect(main).toContain('data-home-app="${app.id}"');
    expect(main).toContain('data-tile-toggle="${escapeHtml(id)}"');
    expect(main).toContain("void openHomeAppFromLauncher(appId, intent)");
    expect(css).toContain(".home-dashboard .home-app-section:not(.home-osl-section) .app-logo-plate");
    expect(css).toContain("border-radius: 50%;");
    console.log("TASK5008 strip_launcher=data-home-app -> openHomeAppFromLauncher(appId, intent)");
    console.log("TASK5008 tile_edit_mode=data-tile-toggle");
  });
});
