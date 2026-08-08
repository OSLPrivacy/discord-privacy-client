import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

/**
 * TASK 4501. There used to be FOUR controls that turn private words on and off:
 * the eye on the Discord strip, a "Show decrypted text" tick box in each of the
 * two protected sheets, and a fourth on the paint-over window that worked,
 * defaulted to on, and was out of reach only because the box around it carried
 * a `hidden` mark. Four controls over one per-scope setting is four answers to
 * "is this protected right now", and the blueprint says there must be one.
 *
 * The counting lives in scripts/qa/osl-one-show-private-words-control.mjs so
 * that one definition of "a control" is shared by this test and by the audit
 * command. It counts structurally -- a pressable element, bound to a listener,
 * that reaches a call deciding a NEW value for the setting -- so a second
 * control added under any new name is still found.
 */
const checker = fileURLToPath(
  new URL("../../../scripts/qa/osl-one-show-private-words-control.mjs", import.meta.url),
);

/**
 * The checker exits 1 when it finds anything other than one control, so its
 * output has to be captured rather than thrown away -- otherwise a red run
 * reports a collection error instead of the control it found.
 */
function runChecker(): string {
  try {
    return execFileSync(process.execPath, [checker], { encoding: "utf8" });
  } catch (error) {
    const failed = error as { stdout?: string; stderr?: string };
    return `${failed.stdout ?? ""}${failed.stderr ?? ""}`;
  }
}

describe("show-private-words controls", () => {
  const output = runChecker();

  it("keeps exactly one control anywhere in the app, down from four", () => {
    expect(output).toContain("  before: 4    after: 1");
    expect(output).toContain("#discord-qa-transcript-visibility");
  });

  it("leaves none of them working but out of reach", () => {
    expect(output).toContain("working-but-hidden controls: 0");
    expect(output).toContain("handlers bound to a missing element: 0");
  });

  it("shows exactly one a person can press in the shipping build", () => {
    expect(output).toContain("pressable in the shipping build: 1");
  });

  it("surfaces no second control when every hidden mark comes off every box", () => {
    const stripped = output.slice(output.indexOf("WITH EVERY `hidden` MARK STRIPPED"));
    expect(stripped).toContain("controls: 1");
  });

  it("passes overall", () => {
    expect(output).toContain("\nPASS\n");
  });
});
