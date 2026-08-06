import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { continuePasswordSetup } from "./password-setup-continue";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("TASK0019 password setup continue bypass refusal", () => {
  it("refuses direct continue calls without a valid password or with mismatched passwords", () => {
    const beforeValidPassword = continuePasswordSetup("create", "", "");
    console.log(
      `TASK0019 direct_call=continuePasswordSetup scenario=before_valid_password accepted=${beforeValidPassword.accepted} setup_step=${beforeValidPassword.route} reason=${beforeValidPassword.accepted ? "accepted" : beforeValidPassword.reason}`,
    );
    expect(beforeValidPassword).toMatchObject({
      accepted: false,
      route: "create",
      reason: "invalid-password",
    });

    const mismatchedPasswords = continuePasswordSetup("create", "correct-horse", "different-horse");
    console.log(
      `TASK0019 direct_call=continuePasswordSetup scenario=mismatched_passwords accepted=${mismatchedPasswords.accepted} setup_step=${mismatchedPasswords.route} reason=${mismatchedPasswords.accepted ? "accepted" : mismatchedPasswords.reason}`,
    );
    expect(mismatchedPasswords).toMatchObject({
      accepted: false,
      route: "create",
      reason: "mismatched-passwords",
    });

    expect(continuePasswordSetup("create", "correct-horse", "correct-horse")).toEqual({
      accepted: true,
      route: "recovery",
    });
  });

  it("runs the direct continue refusal before the native password setup call", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    const refusal = binding.indexOf('continuePasswordSetup("create", password.value, confirm?.value ?? "")');
    const setupCall = binding.indexOf("setupHubMainPassword(secret)");
    console.log(
      `TASK0019 production_guard=bindPasswordForm refusal_before_setup=${refusal >= 0 && setupCall >= 0 && refusal < setupCall} refusal_index=${refusal} setup_call_index=${setupCall}`,
    );
    expect(refusal).toBeGreaterThanOrEqual(0);
    expect(setupCall).toBeGreaterThanOrEqual(0);
    expect(refusal).toBeLessThan(setupCall);
  });
});
