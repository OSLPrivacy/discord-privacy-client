import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

async function responseFingerprint(response: Response): Promise<{
  status: number;
  headers: [string, string][];
  body: Uint8Array;
}> {
  return {
    status: response.status,
    headers: [...response.headers.entries()].sort(([a], [b]) => a.localeCompare(b)),
    body: new Uint8Array(await response.arrayBuffer()),
  };
}

describe("blob fetch route", () => {
  it("makes malformed and missing blob ids indistinguishable", async () => {
    const malformed = await SELF.fetch("https://cipher.test/v1/blob/abcd", {
      headers: {
        "cf-connecting-ip": "198.51.100.80",
        "x-osl-fetch-token": "0123456789abcdef0123456789abcdef",
      },
    });
    const missing = await SELF.fetch(
      "https://cipher.test/v1/blob/0123456789abcdef",
      {
        headers: {
          "cf-connecting-ip": "198.51.100.80",
          "x-osl-fetch-token": "0123456789abcdef0123456789abcdef",
        },
      },
    );

    expect(await responseFingerprint(malformed)).toEqual(
      await responseFingerprint(missing),
    );
  });
});
