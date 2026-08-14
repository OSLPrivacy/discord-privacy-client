import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const renderer = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const native = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const broker = readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");

describe("TASK3520 Covertext button wiring", () => {
  it("uses the plain button as the wordbank choice without another chooser", () => {
    const clickStart = renderer.indexOf(
      'document.querySelector<HTMLButtonElement>("#native-discord-covertext")',
    );
    const clickEnd = renderer.indexOf(
      'document.querySelector<HTMLButtonElement>("#discord-qa-run-test")',
      clickStart,
    );
    expect(clickStart).toBeGreaterThan(-1);
    expect(clickEnd).toBeGreaterThan(clickStart);
    const click = renderer.slice(clickStart, clickEnd);

    expect(click).toContain('invoke<boolean>("select_native_discord_covertext_writer")');
    expect(click).not.toMatch(/<(?:select|option)|aria-haspopup/iu);
    expect(native).toContain("state.set_wordbank_writer_selected(true)");
    expect(broker).toContain("ProseTokenCoverWriter::Covertext");
    expect(broker).toContain("prose_token_send_with_client_and_writer");

    console.info("TASK3520 plain_button=Covertext backend_writer=layered_wordbank choice_lists=0");
  });
});
