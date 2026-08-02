import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type VoiceGate = {
  decision: string;
  availability: string;
  requiredBeforeBuild: string[];
  prohibitedClaims: string[];
  unavoidableMetadata: string[];
};

function voiceGate(): VoiceGate {
  const document = readFileSync(
    new URL("../../../docs/design/osl-enclaves-voice-feasibility.md", import.meta.url),
    "utf8",
  );
  const match = document.match(/```json\n([\s\S]*?)\n```/u);
  if (!match) throw new Error("Voice feasibility decision is missing its machine-readable gate");
  return JSON.parse(match[1]) as VoiceGate;
}

describe("Enclaves voice feasibility gate", () => {
  it("keeps voice out of v1 until its privacy and transport prerequisites are proven", () => {
    expect(voiceGate()).toEqual({
      decision: "post-v1-separate-track",
      availability: "coming-later-not-implemented",
      requiredBeforeBuild: [
        "authenticated-enclave-membership",
        "authenticated-voice-signaling",
        "end-to-end-media-key-distribution-and-rotation",
        "sfu-and-turn-operations",
        "cross-platform-call-security-and-reliability-tests",
      ],
      prohibitedClaims: [
        "voice-is-available",
        "voice-hides-real-time-participation-metadata",
        "voice-is-zero-knowledge-to-the-media-operator",
      ],
      unavoidableMetadata: [
        "a-participant-connected-to-a-room",
        "connection-and-disconnection-times",
        "media-flow-timing-and-volume",
        "network-address-information-without-an-additional-relay",
      ],
    });
  });
});
