import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T2-81 shipping two-tier burn status wiring", () => {
  it("renders the server tier only after the real local burn result and never derives it from the dialog spinner", () => {
    expect(main).toMatch(/from\s*["']\.\/destruct-status["']/u);
    expect(main).toMatch(/function burnDialogMarkup\(\)[\s\S]*destructStatusMarkup\(\{ action: "burn", local: "complete", server: burnResult\.destructServerStatus \}\)/u);
    expect(main).toMatch(/destructServerStatus: revocation\.acknowledged \? "confirmed" : "not-confirmed"/u);
    expect(main).toMatch(/destructServerStatus: result\.remoteCleanupComplete \? "confirmed" : "not-confirmed"/u);
  });
});
