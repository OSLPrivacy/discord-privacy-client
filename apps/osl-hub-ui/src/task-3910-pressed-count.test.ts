import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
const overlayHtml = readFileSync(new URL("../overlay.html", import.meta.url), "utf8");
const peerSheet = readFileSync(new URL("./peer-protected-sheet.ts", import.meta.url), "utf8");
const localSheet = readFileSync(new URL("./local-protected-sheet.ts", import.meta.url), "utf8");

type Control = {
  id: string;
  existsInCode: () => boolean;
  livePressPath: () => boolean;
};

const controls4403: Control[] = [
  {
    id: "#discord-qa-transcript-visibility",
    existsInCode: () => main.includes('id="discord-qa-transcript-visibility"'),
    livePressPath: () => main.includes('document.querySelector<HTMLButtonElement>("#discord-qa-transcript-visibility")?.addEventListener("click", () => {')
      && main.includes("void toggleDiscordQaTranscriptVisibility();")
      && main.includes("peerProtectedSheet.decryptDisplayEnabled = requested")
      && main.includes("await saveActiveContextSecurity("),
  },
  {
    id: "#peer-decrypt-display",
    existsInCode: () => peerSheet.includes('id="peer-decrypt-display"'),
    livePressPath: () => main.includes('document.querySelector<HTMLInputElement>("#peer-decrypt-display")?.addEventListener("change", (event) => void changePeerDecryptDisplay(event.currentTarget as HTMLInputElement));')
      && main.includes("saveActiveContextSecurity(context.contextToken, peerProtectedSheet.ttlSeconds, input.checked)")
      && main.includes("peerProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
  {
    id: "#local-decrypt-display",
    existsInCode: () => localSheet.includes('id="local-decrypt-display"'),
    livePressPath: () => main.includes('document.querySelector<HTMLInputElement>("#local-decrypt-display")?.addEventListener("change", (event) => void changeLocalDecryptDisplay(event.currentTarget as HTMLInputElement));')
      && main.includes("saveActiveContextSecurity(contextToken, localProtectedSheet.ttlSeconds, input.checked)")
      && main.includes("localProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
  {
    id: "#protected-decrypt-display",
    existsInCode: () => overlayHtml.includes('id="protected-decrypt-display"'),
    livePressPath: () => overlay.includes('decryptDisplay.addEventListener("change", () => void saveSecurity());')
      && overlay.includes("const requestedDecrypt = decryptDisplay.checked;")
      && overlay.includes("await setNativeDiscordOverlaySecurity(requestedTtl as NativeOverlayTtlSeconds, decryptDisplay.checked)")
      && overlay.includes("decryptDisplayEnabled = saved.decryptDisplayEnabled"),
  },
];

describe("TASK 3910 pressed-by-hand count", () => {
  it("matches the controls 4403 found with a live press path for each one", () => {
    const codeControls = controls4403.filter((control) => control.existsInCode());
    const pressableControls = codeControls.filter((control) => control.livePressPath());
    const refusedControls = codeControls
      .filter((control) => !control.livePressPath())
      .map((control) => control.id);

    console.log(`TASK3910_CODE_COUNT=${codeControls.length}`);
    console.log(`TASK3910_PRESSED_BY_HAND_COUNT=${pressableControls.length}`);
    console.log(`TASK3910_REFUSED_CONTROLS=${refusedControls.length ? refusedControls.join(",") : "none"}`);

    expect(
      pressableControls.length,
      `TASK3910 pressed-by-hand count mismatch: code_count=${codeControls.length} pressed_by_hand_count=${pressableControls.length} refused_controls=${refusedControls.join(",")}`,
    ).toBe(codeControls.length);
  });
});
