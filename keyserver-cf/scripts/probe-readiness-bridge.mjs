#!/usr/bin/env node
import { readFile } from "node:fs/promises";
import process from "node:process";
import { Miniflare } from "miniflare";
import {
  readAndVerifyReadinessArtifact,
} from "./readiness-artifact-contract.mjs";

function expectedJson(response, status, body) {
  if (response.status !== status) {
    throw new Error(`expected HTTP ${status}, got ${response.status}`);
  }
  return response.json().then((actual) => {
    if (JSON.stringify(actual) !== JSON.stringify(body)) {
      throw new Error(
        `unexpected response: ${JSON.stringify(actual)}; expected ${JSON.stringify(body)}`,
      );
    }
  });
}

async function main() {
  const [manifestPath, bundlePath, metafilePath] = process.argv.slice(2);
  if (
    !manifestPath ||
    !bundlePath ||
    !metafilePath ||
    process.argv.length !== 5
  ) {
    throw new Error(
      "usage: node scripts/probe-readiness-bridge.mjs " +
        "<artifact-a.manifest.json> <artifact-a.bridge.mjs> " +
        "<artifact-a.bridge.meta.json>",
    );
  }
  const manifest = await readAndVerifyReadinessArtifact(
    manifestPath,
    bundlePath,
    metafilePath,
  );
  if (manifest.artifact !== "A") {
    throw new Error("bridge probe requires artifact A");
  }

  // No DB binding is supplied. Every successful assertion below therefore
  // also proves that the bridge route did not query or write D1.
  const worker = new Miniflare({
    modules: true,
    script: await readFile(bundlePath, "utf8"),
    compatibilityDate: "2026-07-15",
    compatibilityFlags: ["nodejs_compat"],
    ratelimits: {
      RATE_LIMIT_5: {
        namespace_id: "1926072701",
        simple: { limit: 100_000, period: 60 },
      },
      RATE_LIMIT_10: {
        namespace_id: "1926072702",
        simple: { limit: 100_000, period: 60 },
      },
      RATE_LIMIT_120: {
        namespace_id: "1926072703",
        simple: { limit: 100_000, period: 60 },
      },
      RATE_LIMIT_1200: {
        namespace_id: "1926072704",
        simple: { limit: 100_000, period: 60 },
      },
      RATE_LIMIT_3600: {
        namespace_id: "1926072705",
        simple: { limit: 100_000, period: 60 },
      },
    },
  });
  try {
    await expectedJson(
      await worker.dispatchFetch("https://bridge.invalid/v1/healthz"),
      200,
      {
        ok: true,
        readiness_artifact: "A-pre-0031-bridge",
      },
    );
    await expectedJson(
      await worker.dispatchFetch("https://bridge.invalid/v1/register", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          user_id: `osl1_${"a".repeat(32)}`,
          ik_x25519_pub: "x",
          ik_ed25519_pub: "x",
          ik_mlkem768_pub: "x",
          registration_sig: "x",
        }),
      }),
      400,
      {
        error:
          "reserved derived identity namespace requires root proof verification",
      },
    );
    await expectedJson(
      await worker.dispatchFetch(
        "https://bridge.invalid/v1/checkout-session",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: "{}",
        },
      ),
      503,
      {
        error:
          "paid checkout is unavailable until prepaid-code redemption is ready",
      },
    );
    for (const [url, init] of [
      ["https://bridge.invalid/v1/control-inbox/recipient", undefined],
      [
        "https://bridge.invalid/v1/control-inbox",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: "{}",
        },
      ],
      [
        "https://bridge.invalid/v1/control-inbox/00",
        {
          method: "DELETE",
          headers: { "content-type": "application/json" },
          body: "{}",
        },
      ],
    ]) {
      await expectedJson(
        await worker.dispatchFetch(url, init),
        503,
        { error: "control inbox unavailable during schema transition" },
      );
    }
  } finally {
    await worker.dispose();
  }
  process.stdout.write("bridge runtime proof: 6/6 passed\n");
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.stack : error}\n`);
  process.exitCode = 1;
});
