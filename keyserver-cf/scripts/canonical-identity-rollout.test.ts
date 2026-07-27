import {
  mkdtemp,
  readFile,
  rm,
  stat,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  canonicalIdentityBundleBytes,
  canonicalRolloutAdvanceBytes,
  deriveCanonicalOslIdentityId,
  type CanonicalIdentityBundle,
  validateCanonicalIdentityBundle,
} from "../src/lib/identity-authority.js";
import {
  handleSenderFilterRolloutRootAdvance,
} from "../src/endpoints/sender-filter-rollout-root.js";
import {
  buildGenesisProvisioning,
  GENESIS_DATABASE,
  loadCanonicalRecoveryManifest,
  provisionSenderFilterGenesis,
  runWranglerGenesisProvision,
} from "./provision-sender-filter-rollout-genesis.mjs";

const ROOT = path.resolve(import.meta.dirname, "..");
const SOURCE_COMMIT = "a".repeat(40);
const REPOSITORY_TREE = "b".repeat(40);
const KEYSERVER_TREE = "c".repeat(40);

function provisioningAdmission() {
  return {
    payload_sha256: "d".repeat(64),
    source: {
      commit: SOURCE_COMMIT,
      repository_tree: REPOSITORY_TREE,
      keyserver_tree: KEYSERVER_TREE,
    },
  };
}

function base64(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("base64");
}

function littleEndian32(value: bigint): Uint8Array {
  const bytes = new Uint8Array(32);
  let remaining = value;
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return bytes;
}

async function ed25519(): Promise<CryptoKeyPair> {
  return await crypto.subtle.generateKey(
    { name: "Ed25519" },
    true,
    ["sign", "verify"],
  ) as CryptoKeyPair;
}

async function rawPublic(key: CryptoKey): Promise<string> {
  return base64(
    new Uint8Array(await crypto.subtle.exportKey("raw", key)),
  );
}

async function sign(key: CryptoKey, bytes: Uint8Array): Promise<string> {
  return base64(
    new Uint8Array(
      await crypto.subtle.sign({ name: "Ed25519" }, key, bytes),
    ),
  );
}

async function identityFixture(revision = 1) {
  const root = await ed25519();
  const current = await ed25519();
  const rootB64 = await rawPublic(root.publicKey);
  const bundle: CanonicalIdentityBundle = {
    user_id: await deriveCanonicalOslIdentityId(rootB64),
    identity_scheme: 1,
    identity_revision: revision,
    ik_root_ed25519_pub: rootB64,
    ik_x25519_pub: base64(crypto.getRandomValues(new Uint8Array(32))),
    ik_ed25519_pub: await rawPublic(current.publicKey),
    ik_mlkem768_pub: base64(
      crypto.getRandomValues(new Uint8Array(1184)),
    ),
    ik_ratchet_initial_pub: base64(
      crypto.getRandomValues(new Uint8Array(32)),
    ),
    rn_capabilities: 1,
  };
  const canonical = canonicalIdentityBundleBytes(bundle);
  return {
    bundle,
    root,
    current,
    rootProof: await sign(root.privateKey, canonical),
    currentProof: await sign(current.privateKey, canonical),
  };
}

describe("canonical identity and sender-filter rollout authority", () => {
  it("derives an opaque identity and makes both Ed25519 proofs cover the full bundle", async () => {
    const fixture = await identityFixture();
    const validated = await validateCanonicalIdentityBundle(
      fixture.bundle,
      fixture.rootProof,
      fixture.currentProof,
    );
    expect(fixture.bundle.user_id).toMatch(/^osl1_[a-z2-7]{52}$/);
    expect(validated.bundle_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(
      await deriveCanonicalOslIdentityId(
        "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=",
      ),
    ).toBe(
      "osl1_a7uth77dnlmwyottbw25mluobunifo3yfnvrb2x2bz4xqk4qtwha",
    );

    // A verifier that authenticates only the current Ed25519 key or only a
    // subset of the bundle would accept this X25519 substitution.
    const substituted = {
      ...fixture.bundle,
      ik_x25519_pub: base64(
        crypto.getRandomValues(new Uint8Array(32)),
      ),
    };
    await expect(
      validateCanonicalIdentityBundle(
        substituted,
        fixture.rootProof,
        fixture.currentProof,
      ),
    ).rejects.toThrow(/full-bundle root proof is invalid/);

    const wrongRoot = await identityFixture();
    await expect(
      validateCanonicalIdentityBundle(
        {
          ...fixture.bundle,
          ik_root_ed25519_pub: wrongRoot.bundle.ik_root_ed25519_pub,
        },
        fixture.rootProof,
        fixture.currentProof,
      ),
    ).rejects.toThrow(/user_id is not derived/);

    // RFC 8032 requires S < L. Replacing S with L preserves the 64-byte and
    // base64 shapes, so a length-only decoder or permissive Ed25519 verifier
    // would false-green this proof.
    const nonCanonicalRootProof = Buffer.from(
      fixture.rootProof,
      "base64",
    );
    nonCanonicalRootProof.set(
      littleEndian32(
        (1n << 252n) + 27742317777372353535851937790883648493n,
      ),
      32,
    );
    await expect(
      validateCanonicalIdentityBundle(
        fixture.bundle,
        nonCanonicalRootProof.toString("base64"),
        fixture.currentProof,
      ),
    ).rejects.toThrow(/canonical Ed25519 encoding/);

    // RFC 8032 also rejects an encoded point whose recovered x is zero while
    // the x-sign bit is one. A y<p-only check would accept this y=1 form.
    const xZeroWithSign = new Uint8Array(32);
    xZeroWithSign[0] = 1;
    xZeroWithSign[31] = 0x80;
    const nonCanonicalRoot = base64(xZeroWithSign);
    await expect(
      deriveCanonicalOslIdentityId(nonCanonicalRoot),
    ).rejects.toThrow(/canonical Ed25519 encoding/);

    const nonCanonicalRProof = Buffer.from(fixture.rootProof, "base64");
    nonCanonicalRProof.set(xZeroWithSign, 0);
    await expect(
      validateCanonicalIdentityBundle(
        fixture.bundle,
        nonCanonicalRProof.toString("base64"),
        fixture.currentProof,
      ),
    ).rejects.toThrow(/canonical Ed25519 encoding/);

    const wrongCurrentProof = Buffer.from(fixture.currentProof, "base64");
    wrongCurrentProof[0] = (wrongCurrentProof[0] ?? 0) ^ 1;
    await expect(
      validateCanonicalIdentityBundle(
        fixture.bundle,
        fixture.rootProof,
        wrongCurrentProof.toString("base64"),
      ),
    ).rejects.toThrow(/current Ed25519 key proof is invalid/);
  });

  it("advances durable state by one exact CAS and refuses stale caller restore without retry", async () => {
    const fixture = await identityFixture();
    const validated = await validateCanonicalIdentityBundle(
      fixture.bundle,
      fixture.rootProof,
      fixture.currentProof,
    );
    const state = {
      root_user_id: fixture.bundle.user_id,
      root_ed25519_pub: fixture.bundle.ik_root_ed25519_pub,
      identity_bundle_sha256: validated.bundle_sha256,
      capability_version: 1,
      monotonic_version: 1,
      last_observation_sha256: "1".repeat(64),
      provisioned_at_ms: Date.now() - 1000,
      updated_at_ms: Date.now() - 1000,
    };
    let updateCalls = 0;
    const database = {
      prepare(sql: string) {
        return {
          bind(...values: unknown[]) {
            return {
              async first() {
                if (sql.includes("sender_filter_rollout_root")) {
                  return { ...state };
                }
                if (sql.includes("FROM users")) {
                  return {
                    ...fixture.bundle,
                    identity_bundle_proof_sig: fixture.rootProof,
                    registration_sig: fixture.currentProof,
                  };
                }
                throw new Error("unexpected SELECT");
              },
              async run() {
                updateCalls += 1;
                expect(sql).toContain("AND monotonic_version = ?");
                expect(sql).not.toMatch(/\b(?:INSERT|DELETE)\b/);
                const nextVersion = values[0] as number;
                const observation = values[1] as string;
                const expectedVersion = values[5] as number;
                if (state.monotonic_version !== expectedVersion) {
                  return { meta: { changes: 0 } };
                }
                state.monotonic_version = nextVersion;
                state.last_observation_sha256 = observation;
                return { meta: { changes: 1 } };
              },
            };
          },
          async first() {
            return { ...state };
          },
        };
      },
    };
    const timestamp = Date.now();
    const requestId = "R".repeat(43);
    const observation = "a".repeat(64);
    const canonical = canonicalRolloutAdvanceBytes({
      root_user_id: fixture.bundle.user_id,
      expected_monotonic_version: 1,
      observation_sha256: observation,
      timestamp_ms: timestamp,
      request_id: requestId,
    });
    const requestBody = {
      root_user_id: fixture.bundle.user_id,
      expected_monotonic_version: 1,
      observation_sha256: observation,
      timestamp_ms: timestamp,
      request_id: requestId,
      signature_b64: await sign(fixture.root.privateKey, canonical),
    };
    const environment = { DB: database } as any;
    const first = await handleSenderFilterRolloutRootAdvance(
      new Request("http://test/v1/internal/sender-filter-rollout-root/advance", {
        method: "POST",
        body: JSON.stringify(requestBody),
      }),
      environment,
    );
    expect(first.status).toBe(200);
    expect(state.monotonic_version).toBe(2);

    // Replaying a caller-restored version-1 request performs one conditional
    // UPDATE, receives changes=0, and returns conflict. It never reloads and
    // retries with version 2.
    const stale = await handleSenderFilterRolloutRootAdvance(
      new Request("http://test/v1/internal/sender-filter-rollout-root/advance", {
        method: "POST",
        body: JSON.stringify(requestBody),
      }),
      environment,
    );
    expect(stale.status).toBe(409);
    expect(state.monotonic_version).toBe(2);
    expect(updateCalls).toBe(2);
  });

  it("pins production genesis to D1 administration and never places the raw nonce in SQL", () => {
    const nonce = Buffer.alloc(32, 7);
    const built = buildGenesisProvisioning(
      nonce,
      1_800_000_000_000,
      provisioningAdmission(),
    );
    expect(built.manifest.database).toBe(GENESIS_DATABASE);
    expect(built.manifest.genesis_nonce).toHaveLength(43);
    expect(built.sql).toContain(built.manifest.genesis_nonce_sha256);
    expect(built.sql).not.toContain(built.manifest.genesis_nonce);

    let invocation: string[] = [];
    const output = runWranglerGenesisProvision(
      built.sql,
      built.expectedReadback,
      ((_command: string, args: string[]) => {
        invocation = args;
        return {
          status: 0,
          stdout: JSON.stringify([
            { success: true, results: [built.expectedReadback] },
          ]),
          stderr: "",
        };
      }) as any,
    );
    expect(output).toEqual(built.expectedReadback);
    expect(invocation).toContain("execute");
    expect(invocation).toContain(GENESIS_DATABASE);
    expect(invocation).toContain("--remote");
    expect(invocation).toContain("--config");
    expect(invocation).toContain("wrangler.toml");
  });

  it("retains one canonical private nonce after ambiguous mutation and refuses a second remote call", async () => {
    const directory = await mkdtemp(
      path.join(tmpdir(), "osl-rollout-genesis-"),
    );
    const recoveryPath = path.join(directory, "canonical-genesis.json");
    const recoveryOptions = {
      expectedPath: recoveryPath,
      expectedUid: process.getuid(),
    };
    try {
      const built = buildGenesisProvisioning(
        Buffer.alloc(32, 9),
        1_800_000_000_000,
        provisioningAdmission(),
      );
      let remoteCalled = 0;
      await expect(
        provisionSenderFilterGenesis(
          recoveryPath,
          built.manifest,
          built.sql,
          () => {
            remoteCalled += 1;
            throw new Error("ambiguous remote transport failure");
          },
          recoveryOptions,
        ),
      ).rejects.toThrow(/protected recovery manifest retained/);
      expect(remoteCalled).toBe(1);
      expect(
        await loadCanonicalRecoveryManifest(
          recoveryPath,
          recoveryOptions,
        ),
      ).toEqual(built.manifest);
      expect((await stat(recoveryPath)).mode & 0o777).toBe(0o600);

      const replacement = buildGenesisProvisioning(
        Buffer.alloc(32, 10),
        1_800_000_000_001,
        provisioningAdmission(),
      );
      await expect(
        provisionSenderFilterGenesis(
          recoveryPath,
          replacement.manifest,
          replacement.sql,
          () => {
            remoteCalled += 1;
          },
          recoveryOptions,
        ),
      ).rejects.toMatchObject({ code: "EEXIST" });
      expect(remoteCalled).toBe(1);
      expect(
        (await loadCanonicalRecoveryManifest(
          recoveryPath,
          recoveryOptions,
        )).genesis_nonce,
      ).toBe(built.manifest.genesis_nonce);
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  it("keeps identity, rollout, migration, and provisioning callers on shipping entrypoints", async () => {
    const [index, register, packageJson, migration] = await Promise.all([
      readFile(path.join(ROOT, "src/index.ts"), "utf8"),
      readFile(path.join(ROOT, "src/endpoints/register.ts"), "utf8"),
      readFile(path.join(ROOT, "package.json"), "utf8"),
      readFile(
        path.join(
          ROOT,
          "migrations/0033_canonical_identity_rollout_authority.sql",
        ),
        "utf8",
      ),
    ]);
    expect(register).toContain("handleCanonicalIdentityRegister(body, env)");
    expect(index).toContain(
      'if (path === "/v1/register") return await handleRegister(request, env)',
    );
    expect(index).toContain("handleSenderFilterRolloutRootProvision(request, env)");
    expect(index).toContain("handleSenderFilterRolloutRootAdvance(request, env)");
    expect(packageJson).toContain("sender-filter:provision-genesis");
    expect(packageJson).toContain("canonical-rollout:admit-provisioning");
    expect(migration).toContain("sender_filter_rollout_root_no_delete");
    expect(migration).toContain("admission_receipt_sha256 TEXT NOT NULL UNIQUE");
    expect(migration).toContain(
      "NEW.monotonic_version = OLD.monotonic_version + 1",
    );
    expect(migration).toContain("users_rollout_root_no_delete");
  });
});
