import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import {
  BRIDGE_ALIASES,
  BRIDGE_REQUIRED_TEXT,
  MIGRATION_0031_SURFACES,
  READINESS_MANIFEST_FORMAT,
  readinessArchiveId,
  sha256,
  validateReadinessSelection,
  verifyReadinessBundle,
} from "./readiness-artifact-contract.mjs";
import {
  handleControlInboxDelete,
  handleControlInboxGet,
  handleControlInboxPost,
} from "../src/readiness/bridge/control-inbox.js";
import {
  sweepExpiredControlInboxRows,
} from "../src/readiness/bridge/control-inbox-sweep.js";
import { handleHealthz } from "../src/readiness/bridge/healthz.js";
import type { Env } from "../src/env.js";

function manifest(
  artifact: "A" | "B",
  bundle: Uint8Array,
): Record<string, unknown> {
  const source = {
    commit: "a".repeat(40),
    repository_tree: "b".repeat(40),
    keyserver_tree: "c".repeat(40),
    archive_file: "source.tar",
    archive_sha256: "d".repeat(64),
    archive_bytes: 1,
  };
  const tool = (name: "builder" | "verifier" | "contract") => ({
    path: {
      builder: "keyserver-cf/scripts/build-readiness-artifacts.mjs",
      verifier: "keyserver-cf/scripts/admit-readiness-archive.mjs",
      contract: "keyserver-cf/scripts/readiness-artifact-contract.mjs",
    }[name],
    sha256: "e".repeat(64),
    bytes: 1,
  });
  const toolchain = {
    builder: tool("builder"),
    verifier: tool("verifier"),
    contract: tool("contract"),
    clean_checkout_required: true,
  };
  return {
    format: READINESS_MANIFEST_FORMAT,
    archive_id: readinessArchiveId(source, toolchain),
    artifact,
    role: artifact === "A" ? "pre-0031-bridge" : "0031-aware-final",
    source,
    build: {
      entrypoint: "src/index.ts",
      aliases: artifact === "A" ? BRIDGE_ALIASES : {},
      command: [
        "deploy",
        "src/index.ts",
        "--config",
        "wrangler.toml",
        "--dry-run",
        "--minify",
      ],
      wrangler_version: "4.110.0",
      bundle_file:
        artifact === "A" ? "artifact-a.bridge.mjs" : "artifact-b.final.mjs",
      bundle_sha256: sha256(bundle),
      bundle_bytes: bundle.byteLength,
      metafile_file:
        artifact === "A"
          ? "artifact-a.bridge.meta.json"
          : "artifact-b.final.meta.json",
      metafile_sha256: "f".repeat(64),
      metafile_bytes: 1,
    },
    policy: {
      requires_0031: artifact === "B",
      forbidden_after_reconciliation: artifact === "A",
    },
  };
}

function bytes(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

function throwingEnv(): Env {
  return new Proxy(
    {},
    {
      get(_target, property) {
        throw new Error(`bridge accessed env.${String(property)}`);
      },
    },
  ) as Env;
}

describe("readiness artifact closure", () => {
  it("binds exactly the three 0031-facing imports to bridge modules", () => {
    expect(BRIDGE_ALIASES).toEqual({
      "./endpoints/control-inbox.js":
        "./src/readiness/bridge/control-inbox.ts",
      "./endpoints/healthz.js":
        "./src/readiness/bridge/healthz.ts",
      "./lib/control-inbox-sweep.js":
        "./src/readiness/bridge/control-inbox-sweep.ts",
    });
    expect(Object.keys(BRIDGE_ALIASES)).toHaveLength(3);
  });

  it("bridge health, routes, and sweep are nonvacuous and never touch env or D1", async () => {
    const env = throwingEnv();
    const health = await handleHealthz(env);
    expect(health.status).toBe(200);
    expect(await health.json()).toEqual({
      ok: true,
      readiness_artifact: "A-pre-0031-bridge",
    });
    const responses = await Promise.all([
      handleControlInboxPost(new Request("https://test/inbox"), env),
      handleControlInboxGet(new Request("https://test/inbox/u"), env, "u"),
      handleControlInboxDelete(
        new Request("https://test/inbox/00"),
        env,
        "00",
      ),
    ]);
    expect(responses.map((response) => response.status)).toEqual([503, 503, 503]);
    for (const response of responses) {
      await expect(response.json()).resolves.toEqual({
        error: "control inbox unavailable during schema transition",
      });
    }
    expect(
      await sweepExpiredControlInboxRows(
        new Proxy(
          {},
          {
            get(_target, property) {
              throw new Error(`bridge accessed DB.${String(property)}`);
            },
          },
        ) as D1Database,
      ),
    ).toEqual({
      inboxRows: 0,
      requestReceipts: 0,
      senderStates: {
        examined: 0,
        reenabled: 0,
        retryable: 0,
        quarantined: 0,
        retired: 0,
      },
    });
  });

  it("accepts only a hash-bound bridge with both safety refusals and no 0031 surface", () => {
    const bundle = bytes(BRIDGE_REQUIRED_TEXT.join("\n"));
    expect(verifyReadinessBundle(manifest("A", bundle), bundle).artifact).toBe(
      "A",
    );

    for (const required of BRIDGE_REQUIRED_TEXT) {
      const mutated = bytes(
        BRIDGE_REQUIRED_TEXT.filter((text) => text !== required).join("\n"),
      );
      expect(
        () => verifyReadinessBundle(manifest("A", mutated), mutated),
        required,
      ).toThrow(/missing required refusal/);
    }
    for (const forbidden of MIGRATION_0031_SURFACES) {
      const mutated = bytes(`${BRIDGE_REQUIRED_TEXT.join("\n")}\n${forbidden}`);
      expect(
        () => verifyReadinessBundle(manifest("A", mutated), mutated),
        forbidden,
      ).toThrow(/contains migration 0031 surface/);
    }
    const tampered = bytes(`${BRIDGE_REQUIRED_TEXT.join("\n")}\ntampered`);
    expect(() =>
      verifyReadinessBundle(manifest("A", bundle), tampered)
    ).toThrow(/byte count|SHA-256/);
  });

  it("requires every 0031 schema and reconciliation token in final", () => {
    const bundle = bytes(MIGRATION_0031_SURFACES.join("\n"));
    expect(verifyReadinessBundle(manifest("B", bundle), bundle).artifact).toBe(
      "B",
    );
    for (const required of MIGRATION_0031_SURFACES) {
      const mutated = bytes(
        MIGRATION_0031_SURFACES.filter((text) => text !== required).join("\n"),
      );
      expect(
        () => verifyReadinessBundle(manifest("B", mutated), mutated),
        required,
      ).toThrow(/final bundle is missing/);
    }
  });

  it("forbids bridge rollback after reconciliation and final before 0031", () => {
    const bridgeBundle = bytes(BRIDGE_REQUIRED_TEXT.join("\n"));
    const finalBundle = bytes(MIGRATION_0031_SURFACES.join("\n"));
    const bridge = manifest("A", bridgeBundle);
    const final = manifest("B", finalBundle);

    expect(
      validateReadinessSelection(bridge, {
        control_inbox_sender_disposition: null,
        control_inbox_sender_reconciliation_started: null,
      }),
    ).toBe(true);
    expect(
      validateReadinessSelection(bridge, {
        control_inbox_sender_disposition: 1,
        control_inbox_sender_reconciliation_started: null,
      }),
    ).toBe(true);
    expect(() =>
      validateReadinessSelection(bridge, {
        control_inbox_sender_disposition: 1,
        control_inbox_sender_reconciliation_started: 1,
      })
    ).toThrow(/artifact A is forbidden/);
    expect(() =>
      validateReadinessSelection(final, {
        control_inbox_sender_disposition: null,
        control_inbox_sender_reconciliation_started: null,
      })
    ).toThrow(/requires the exact migration 0031 marker/);
    expect(
      validateReadinessSelection(final, {
        control_inbox_sender_disposition: 1,
        control_inbox_sender_reconciliation_started: null,
      }),
    ).toBe(true);
    expect(() =>
      validateReadinessSelection(final, {
        control_inbox_sender_disposition: 0,
        control_inbox_sender_reconciliation_started: 0,
      })
    ).toThrow(/must be exactly 1 or null/);
  });

  it("keeps the rollback marker before the candidate query in final source", async () => {
    const source = await readFile(
      new URL("../src/lib/control-inbox-sweep.ts", import.meta.url),
      "utf8",
    );
    const markerWrite = source.indexOf(
      "INSERT INTO worker_schema_capabilities",
    );
    const candidateRead = source.indexOf("SELECT ci.id");
    const statusWrite = source.indexOf("UPDATE control_inbox");
    expect(markerWrite).toBeGreaterThan(0);
    expect(candidateRead).toBeGreaterThan(markerWrite);
    expect(statusWrite).toBeGreaterThan(candidateRead);
  });
});
