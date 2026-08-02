import { describe, expect, it } from "vitest";

import { componentPickerScreen, type ComponentManifestEntry } from "./component-picker";

const components: readonly ComponentManifestEntry[] = [
  {
    id: "local-ai-model",
    displayName: "Local AI model",
    measuredSizeBytes: 1_879_048_192,
    withoutIt: "The free word-bank carrier still works.",
  },
  {
    id: "tor",
    displayName: "Tor",
    measuredSizeBytes: 31_457_280,
    withoutIt: "OSL connects directly, without an anonymity layer.",
  },
];

describe("T15-T26 component picker", () => {
  it("shows every optional component's measured size and absence consequence", () => {
    const screen = componentPickerScreen(components);

    expect(screen.components).toEqual([
      {
        id: "local-ai-model",
        displayName: "Local AI model",
        size: "1.75 GB",
        withoutIt: "The free word-bank carrier still works.",
        selected: false,
      },
      {
        id: "tor",
        displayName: "Tor",
        size: "30 MB",
        withoutIt: "OSL connects directly, without an anonymity layer.",
        selected: false,
      },
    ]);
  });

  it("rejects a component whose size or absence consequence has not been measured and stated", () => {
    expect(() => componentPickerScreen([
      { ...components[0], measuredSizeBytes: 0 },
    ])).toThrow("measured size");
    expect(() => componentPickerScreen([
      { ...components[0], withoutIt: "  " },
    ])).toThrow("absence consequence");
  });
});
