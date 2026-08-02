import { describe, expect, it } from "vitest";
import {
  isOhttpReadyBlobFetch,
  ohttpBlobFetchResponse,
} from "../src/endpoints/blob-request-profile.js";

describe("OHTTP-ready blob fetch profile", () => {
  it("keeps fetch authority in X-OSL-Fetch-Cap and returns no cookie or redirect", () => {
    const request = new Request("https://cipher.test/v1/blob/0123456789abcdef0123456789abcdef", {
      headers: { "X-OSL-Fetch-Cap": "0123456789abcdef0123456789abcdef" },
    });
    expect(isOhttpReadyBlobFetch(request)).toBe(true);
    expect(isOhttpReadyBlobFetch(new Request(
      "https://cipher.test/v1/blob/0123456789abcdef0123456789abcdef?fetch_cap=0123456789abcdef0123456789abcdef",
    ))).toBe(false);

    const response = ohttpBlobFetchResponse(new Uint8Array([1, 2, 3]));
    expect(response.status).toBe(200);
    expect(response.headers.get("set-cookie")).toBeNull();
    expect(response.headers.get("location")).toBeNull();
    expect(response.headers.get("content-length")).toBe("3");
  });
});
