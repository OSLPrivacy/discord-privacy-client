import { describe, expect, it } from "vitest";

import {
  componentManagerFromOnboarding,
  installComponent,
  removeComponent,
  type OptionalComponent,
} from "./component-manager";

const components: readonly OptionalComponent[] = [
  {
    id: "local-ai-model",
    featureName: "Natural-looking cover text",
    fallback: "The word-bank carrier remains available.",
  },
  {
    id: "tor",
    featureName: "Tor routing",
    fallback: "OSL connects directly, without an anonymity layer.",
  },
];

describe("T15-T27 later component management", () => {
  it("makes a component installed later reach its onboarding-installed state", () => {
    const selectedAtOnboarding = componentManagerFromOnboarding(components, ["local-ai-model"]);
    const installedLater = installComponent(
      componentManagerFromOnboarding(components, []),
      components,
      "local-ai-model",
    );

    expect(installedLater).toEqual(selectedAtOnboarding);
  });

  it("degrades removed components to their stated fallback instead of claiming availability", () => {
    const installed = componentManagerFromOnboarding(components, ["local-ai-model", "tor"]);

    expect(removeComponent(installed, components, "local-ai-model")).toMatchObject({
      installedIds: ["tor"],
      features: [
        {
          id: "local-ai-model",
          availability: "fallback",
          detail: "The word-bank carrier remains available.",
        },
        { id: "tor", availability: "available" },
      ],
    });
  });

  it("rejects an unknown component instead of reporting it as installed", () => {
    const manager = componentManagerFromOnboarding(components, []);

    expect(() => installComponent(manager, components, "unknown")).toThrow("Unknown optional component");
  });
});
