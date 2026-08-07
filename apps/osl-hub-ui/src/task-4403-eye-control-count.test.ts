import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
const overlayHtml = readFileSync(new URL("../overlay.html", import.meta.url), "utf8");
const peerSheet = readFileSync(new URL("./peer-protected-sheet.ts", import.meta.url), "utf8");
const localSheet = readFileSync(new URL("./local-protected-sheet.ts", import.meta.url), "utf8");

const RECORDED_SHIPPING_PRESSABLE_BEFORE = 2;

type ShippingEyeControl = {
  id: string;
  existsInCode: () => boolean;
  shipsToPeople: () => boolean;
  hasLivePressPath: () => boolean;
};

function elementTagById(source: string, id: string): string {
  const match = new RegExp(`<[^>]+id="${id}"[^>]*>`, "u").exec(source);
  return match?.[0] ?? "";
}

function overlayRuntimeControlsTag(): string {
  const match = /<div\s+class="overlay-runtime-controls"[^>]*>/u.exec(overlayHtml);
  return match?.[0] ?? "";
}

const shippingEyeControls: ShippingEyeControl[] = [
  {
    id: "#peer-decrypt-display",
    existsInCode: () => peerSheet.includes('id="peer-decrypt-display"'),
    shipsToPeople: () => true,
    hasLivePressPath: () => main.includes('document.querySelector<HTMLInputElement>("#peer-decrypt-display")?.addEventListener("change", (event) => void changePeerDecryptDisplay(event.currentTarget as HTMLInputElement));')
      && main.includes("saveActiveContextSecurity(context.contextToken, peerProtectedSheet.ttlSeconds, input.checked)")
      && main.includes("peerProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
  {
    id: "#local-decrypt-display",
    existsInCode: () => localSheet.includes('id="local-decrypt-display"'),
    shipsToPeople: () => true,
    hasLivePressPath: () => main.includes('document.querySelector<HTMLInputElement>("#local-decrypt-display")?.addEventListener("change", (event) => void changeLocalDecryptDisplay(event.currentTarget as HTMLInputElement));')
      && main.includes("saveActiveContextSecurity(contextToken, localProtectedSheet.ttlSeconds, input.checked)")
      && main.includes("localProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
  {
    id: "#protected-decrypt-display",
    existsInCode: () => overlayHtml.includes('id="protected-decrypt-display"'),
    shipsToPeople: () => !/\shidden(?:\s|>|=)/u.test(overlayRuntimeControlsTag()),
    hasLivePressPath: () => overlay.includes('decryptDisplay.addEventListener("change", () => void saveSecurity());')
      && overlay.includes("const requestedDecrypt = decryptDisplay.checked;")
      && overlay.includes("await setNativeDiscordOverlaySecurity(requestedTtl as NativeOverlayTtlSeconds, decryptDisplay.checked)")
      && overlay.includes("decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
  {
    id: "#discord-qa-transcript-visibility",
    existsInCode: () => main.includes('id="discord-qa-transcript-visibility"'),
    shipsToPeople: () => false,
    hasLivePressPath: () => main.includes('document.querySelector<HTMLButtonElement>("#discord-qa-transcript-visibility")?.addEventListener("click", () => {')
      && main.includes("void toggleDiscordQaTranscriptVisibility();")
      && main.includes("peerProtectedSheet.decryptDisplayEnabled = requested")
      && main.includes("await saveActiveContextSecurity("),
  },
];

describe("TASK 4403 shipping eye control count", () => {
  it("matches the recorded before figure for controls a person can press", () => {
    const codeControls = shippingEyeControls.filter((control) => control.existsInCode());
    const shippingControls = codeControls.filter((control) => control.shipsToPeople() && control.hasLivePressPath());
    const hiddenInShipping = codeControls
      .filter((control) => !control.shipsToPeople())
      .map((control) => control.id);
    const delta = shippingControls.length - RECORDED_SHIPPING_PRESSABLE_BEFORE;
    const added = delta > 0
      ? shippingControls
        .slice(RECORDED_SHIPPING_PRESSABLE_BEFORE)
        .map((control) => control.id)
      : [];

    console.log(`TASK4403_CODE_COUNT=${codeControls.length}`);
    console.log(`TASK4403_RECORDED_SHIPPING_BEFORE=${RECORDED_SHIPPING_PRESSABLE_BEFORE}`);
    console.log(`TASK4403_SHIPPING_PRESSABLE_COUNT=${shippingControls.length}`);
    console.log(`TASK4403_SHIPPING_PRESSABLE_DELTA=${delta >= 0 ? `+${delta}` : delta}`);
    console.log(`TASK4403_SHIPPING_PRESSABLE_CONTROLS=${shippingControls.map((control) => control.id).join(",")}`);
    console.log(`TASK4403_HIDDEN_IN_SHIPPING=${hiddenInShipping.join(",") || "none"}`);
    console.log(`TASK4403_PAINT_OVER_BOX_TAG=${overlayRuntimeControlsTag()}`);
    console.log(`TASK4403_PAINT_OVER_INPUT_TAG=${elementTagById(overlayHtml, "protected-decrypt-display")}`);

    expect(
      shippingControls.length,
      `TASK4403 shipping pressable count changed: recorded_before=${RECORDED_SHIPPING_PRESSABLE_BEFORE} actual=${shippingControls.length} delta=${delta >= 0 ? `+${delta}` : delta} added=${added.join(",") || "none"}`,
    ).toBe(RECORDED_SHIPPING_PRESSABLE_BEFORE);
  });
});
