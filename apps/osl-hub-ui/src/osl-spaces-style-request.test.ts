import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type StyleRequest = {
  stylesheet: string;
  forbidden: string[];
  namespace: string;
  requiredStateClasses: string[];
  prohibitedIndicators: string[];
};

function styleRequest(): StyleRequest {
  const document = readFileSync(
    new URL("../../../docs/design/osl-spaces-style-request.md", import.meta.url),
    "utf8",
  );
  const match = document.match(/```json\n([\s\S]*?)\n```/u);
  if (!match) throw new Error("Spaces style request is missing its machine-readable handoff");
  return JSON.parse(match[1]) as StyleRequest;
}

describe("Spaces style handoff", () => {
  it("gives T7 a CSP-safe namespace and every honest state from H6", () => {
    expect(styleRequest()).toEqual({
      stylesheet: "apps/osl-hub-ui/src/styles.css",
      forbidden: ["inline-style", "runtime-style"],
      namespace: "osl-space",
      requiredStateClasses: [
        "osl-space-state--offline",
        "osl-space-state--stale-roster",
        "osl-space-state--burn-queued",
        "osl-space-state--removal-unconfirmed",
        "osl-space-state--ack-unconfirmed",
      ],
      prohibitedIndicators: ["presence", "last-seen", "online-dot"],
    });
  });
});
