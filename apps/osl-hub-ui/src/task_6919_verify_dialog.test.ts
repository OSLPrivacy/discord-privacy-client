import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  decodeSafetyNumberScannablePayload,
  safetyNumberPanelMarkup,
  safetyNumberScannablePayload,
} from "./safety-number-panel";

const DIGITS = 60;
const GROUPS = 12;
const GROUP_SIZE = 5;
const UI_ROOT = process.env.TASK6919_UI_ROOT ?? process.cwd();
const SOURCE_ROOT = join(UI_ROOT, "src");
const WORKTREE_ROOT = process.env.TASK6919_WORKTREE_ROOT ?? join(UI_ROOT, "..", "..");

function source(relative: string): string {
  return readFileSync(join(WORKTREE_ROOT, relative), "utf8");
}

function bundle(fill: number, transport: number, pq: number, ratchet: number | null): readonly string[] {
  const encoded = (length: number, value: number) => Buffer.alloc(length, value).toString("base64");
  return [encoded(32, fill), encoded(32, transport), encoded(1184, pq), ratchet === null ? "" : encoded(32, ratchet)];
}

/** Independent rendering of the documented 3083 v4 reference construction. */
function referencePairNumber(a: readonly string[], b: readonly string[]): string {
  const identity = (candidate: readonly string[]) => Buffer.from(candidate[0], "base64");
  const [low, high] = Buffer.compare(identity(a), identity(b)) < 0 ? [a, b] : [b, a];
  const hash = createHash("sha512");
  hash.update("OSL-SAFETY-NUMBER-v4");
  for (const candidate of [low, high]) {
    for (const component of candidate) {
      const bytes = component === "" ? Buffer.alloc(0) : Buffer.from(component, "base64");
      const length = Buffer.alloc(4);
      length.writeUInt32BE(bytes.length);
      hash.update(length);
      hash.update(bytes);
    }
  }
  const digest = hash.digest();
  return Array.from({ length: GROUPS }, (_, group) =>
    (digest.readUIntBE(group * 5, 5) % 100_000).toString().padStart(GROUP_SIZE, "0"),
  ).join(" ");
}

function renderedDigits(markup: string): string {
  return markup.match(/<code class="verification-code"[^>]*>([^<]+)<\/code>/u)?.[1] ?? "";
}

function payload(markup: string): string {
  return markup.match(/data-safety-number-payload="([^"]+)"/u)?.[1] ?? "";
}

describe("TASK 6919 complete safety-number verify dialog gate", () => {
  it("renders the bilateral reference number on two clients as twelve groups of five and scans the same 60 digits", () => {
    const alice = bundle(0x11, 0x12, 0x13, 0x14);
    const bob = bundle(0x21, 0x22, 0x23, 0x24);
    const fromAlice = referencePairNumber(alice, bob);
    const fromBob = referencePairNumber(bob, alice);
    const aliceDialog = safetyNumberPanelMarkup(fromAlice);
    const bobDialog = safetyNumberPanelMarkup(fromBob);
    const aliceDigits = renderedDigits(aliceDialog);
    const bobDigits = renderedDigits(bobDialog);

    expect(fromAlice, "TASK6919_KEY reference requires both identity keys").toBe(fromBob);
    for (const [client, rendered] of [["alice", aliceDigits], ["bob", bobDigits]] as const) {
      expect(rendered.replaceAll(" ", ""), `TASK6919_DIGIT_COUNT client=${client} expected=60`).toHaveLength(DIGITS);
      expect(rendered.split(" "), `TASK6919_GROUP client=${client} expected=12x5`).toHaveLength(GROUPS);
      expect(rendered.split(" ").every((group) => /^\d{5}$/u.test(group)), `TASK6919_GROUP client=${client} expected=5`).toBe(true);
      expect(rendered, `TASK6919_KEY client=${client} reference pair`).toBe(fromAlice);
      expect(decodeSafetyNumberScannablePayload(payload(client === "alice" ? aliceDialog : bobDialog)), `TASK6919_DIGIT_COUNT client=${client} code must carry 60 rendered digits`).toBe(rendered.replaceAll(" ", ""));
    }
    expect(safetyNumberScannablePayload(fromAlice), "TASK6919_DIGIT_COUNT reference code payload=60").toBe(fromAlice.replaceAll(" ", ""));
    console.info(`TASK6919_DIALOG clients=2 digits=${DIGITS} groups=${GROUPS} group_size=${GROUP_SIZE} reference=${fromAlice} decoded=${payload(aliceDialog)}`);
  });

  it("has one 5068 renderer, no dialog-owned digits, and Accept binds the requested pair", () => {
    const main = readFileSync(join(SOURCE_ROOT, "main.ts"), "utf8");
    const panel = readFileSync(join(SOURCE_ROOT, "safety-number-panel.ts"), "utf8");
    const security = source("apps/osl-hub/src/security.rs");
    const tofu = source("crates/ipc/src/tofu.rs");

    expect((main.match(/safetyNumberPanelMarkup\(/gu) ?? []).length, "TASK6919_GROUP renderer_count=1").toBe(1);
    expect(main, "TASK6919_KEY dialog passes the one HubPerson safety number").toContain("safetyNumberPanelMarkup(copy.code)");
    expect(main, "TASK6919_KEY accept requested pair").toContain("verifyHubPersonAndRefresh(request.personId, typedVerificationCode)");
    expect(main, "TASK6919_KEY adapter pair binding").toContain("verifyHubPerson(personId, safetyNumber)");
    expect(main, "TASK6919_GROUP no dialog-owned verification renderer").not.toContain('class="verification-code"');
    expect(panel, "TASK6919_DIGIT_COUNT panel requires 60 digits").toContain("/^\\d{60}$/u");
    expect(panel, "TASK6919_GROUP panel requires 12 groups of five").toContain("/^(?:\\d{5})(?: \\d{5}){11}$/u");
    expect(panel, "TASK6919_DIGIT_COUNT scannable code present").toContain("safety-number-qr");
    expect(panel, "TASK6919_DIGIT_COUNT code uses its 60-digit payload").toContain("qrModulesSvg(payload)");
    expect(security, "TASK6919_KEY verification derives from the requested pair").toContain("let expected = safety_number_for_pair(core, &expected_bundle)?;");
    expect(security, "TASK6919_KEY security calls shipping pair reference").toContain("ipc::tofu::safety_number_pair(&mine, peer)");
    expect(tofu, "TASK6919_KEY pair reference absorbs both keys").toContain("absorb_canonical_bundle(&mut hasher, low)?;");
    expect(tofu, "TASK6919_KEY pair reference absorbs the other key").toContain("absorb_canonical_bundle(&mut hasher, high)?;");
    expect(tofu, "TASK6919_GROUP reference has twelve groups").toContain("const SAFETY_NUMBER_GROUPS: usize = 12;");
    expect(tofu, "TASK6919_DIGIT_COUNT reference has five decimal digits per group").toContain("format!(\"{value:05}\")");
    console.info("TASK6919_BINDING renderer=1 pair=personId reference=both-identity-keys accept=request.personId");
  });
});
