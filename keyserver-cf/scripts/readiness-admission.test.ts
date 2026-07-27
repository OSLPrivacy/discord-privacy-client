import { execFileSync } from "node:child_process";
import {
  cp,
  mkdir,
  mkdtemp,
  readFile,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  ARTIFACT_DEFINITIONS,
  BRIDGE_ALIASES,
  BRIDGE_REQUIRED_TEXT,
  MIGRATION_0031_SURFACES,
  READINESS_ARCHIVE_FORMAT,
  READINESS_MANIFEST_FORMAT,
  READINESS_TOOL_PATHS,
  readinessArchiveId,
  sha256,
} from "./readiness-artifact-contract.mjs";
import { resolveCleanBuildSource } from "./build-readiness-artifacts.mjs";
import {
  activeDeployment,
  parseAdmissionArgs,
  READINESS_DATABASE_ID,
  READINESS_MARKERS_QUERY,
  READINESS_SCHEMA_QUERY,
  requireStableDeployment,
  runAdmissionCli,
  validateCapturedEvidence,
  validateProductionConfig,
  verifyArchiveDirectory,
} from "./admit-readiness-archive.mjs";

const COMMIT = "1".repeat(40);
const REPOSITORY_TREE = "2".repeat(40);
const KEYSERVER_TREE = "3".repeat(40);
const ACTIVE_VERSION = "11111111-2222-4333-8444-555555555555";

function jsonBytes(value: unknown): Buffer {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
}

function executableBundle(artifact: "A" | "B"): Buffer {
  const required =
    artifact === "A" ? BRIDGE_REQUIRED_TEXT : MIGRATION_0031_SURFACES;
  return Buffer.from(
    `const readinessTokens=${JSON.stringify(required)};\n` +
      "export default {fetch(){return new Response(readinessTokens[0]);}};\n",
  );
}

function metafile(artifact: "A" | "B"): Record<string, unknown> {
  const selected =
    artifact === "A"
      ? [
          "src/readiness/bridge/control-inbox.ts",
          "src/readiness/bridge/healthz.ts",
          "src/readiness/bridge/control-inbox-sweep.ts",
        ]
      : [
          "src/endpoints/control-inbox.ts",
          "src/endpoints/healthz.ts",
          "src/lib/control-inbox-sweep.ts",
        ];
  return {
    inputs: Object.fromEntries(
      ["src/index.ts", ...selected].map((input) => [input, { bytes: 1 }]),
    ),
    outputs: {
      "index.js": {
        entryPoint: "src/index.ts",
        inputs: Object.fromEntries(
          ["src/index.ts", ...selected].map((input) => [input, { bytesInOutput: 1 }]),
        ),
      },
    },
  };
}

async function writeFixture(directory: string) {
  await mkdir(directory);
  const sourceTar = Buffer.from("trusted exact source archive\n");
  const source = {
    commit: COMMIT,
    repository_tree: REPOSITORY_TREE,
    keyserver_tree: KEYSERVER_TREE,
    archive_file: "source.tar",
    archive_sha256: sha256(sourceTar),
    archive_bytes: sourceTar.byteLength,
  };
  const toolchain = {
    ...Object.fromEntries(
      Object.entries(READINESS_TOOL_PATHS).map(([name, toolPath], index) => [
        name,
        {
          path: toolPath,
          sha256: `${index + 4}`.repeat(64),
          bytes: index + 1,
        },
      ]),
    ),
    clean_checkout_required: true,
  };
  const archiveId = readinessArchiveId(source, toolchain);
  const records = [];
  await writeFile(path.join(directory, "source.tar"), sourceTar);
  for (const artifact of ["A", "B"] as const) {
    const definition = ARTIFACT_DEFINITIONS[artifact];
    const bundle = executableBundle(artifact);
    const meta = jsonBytes(metafile(artifact));
    const manifest = {
      format: READINESS_MANIFEST_FORMAT,
      archive_id: archiveId,
      artifact,
      role: definition.role,
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
        bundle_file: definition.bundle_file,
        bundle_sha256: sha256(bundle),
        bundle_bytes: bundle.byteLength,
        metafile_file: definition.metafile_file,
        metafile_sha256: sha256(meta),
        metafile_bytes: meta.byteLength,
      },
      policy: {
        requires_0031: definition.requires_0031,
        forbidden_after_reconciliation:
          definition.forbidden_after_reconciliation,
      },
    };
    const manifestBytes = jsonBytes(manifest);
    await writeFile(path.join(directory, definition.bundle_file), bundle);
    await writeFile(path.join(directory, definition.metafile_file), meta);
    await writeFile(
      path.join(directory, definition.manifest_file),
      manifestBytes,
    );
    records.push({
      artifact,
      role: definition.role,
      manifest_file: definition.manifest_file,
      manifest_sha256: sha256(manifestBytes),
      manifest_bytes: manifestBytes.byteLength,
      bundle_file: definition.bundle_file,
      bundle_sha256: sha256(bundle),
      bundle_bytes: bundle.byteLength,
      metafile_file: definition.metafile_file,
      metafile_sha256: sha256(meta),
      metafile_bytes: meta.byteLength,
      policy: manifest.policy,
    });
  }
  const index = {
    format: READINESS_ARCHIVE_FORMAT,
    archive_id: archiveId,
    source,
    toolchain,
    artifacts: records,
  };
  await writeFile(
    path.join(directory, "readiness-archive.json"),
    jsonBytes(index),
  );
  return {
    index,
    anchor: {
      commit: COMMIT,
      repositoryTree: REPOSITORY_TREE,
      keyserverTree: KEYSERVER_TREE,
      toolchain,
    },
  };
}

async function fixturePair() {
  const root = await mkdtemp(path.join(tmpdir(), "readiness-admission-test-"));
  const trustedDir = path.join(root, "trusted");
  const candidateDir = path.join(root, "candidate");
  const fixture = await writeFixture(trustedDir);
  await cp(trustedDir, candidateDir, { recursive: true });
  return { root, trustedDir, candidateDir, ...fixture };
}

async function relinkBundle(
  directory: string,
  artifact: "A" | "B",
  bundle: Buffer,
) {
  const definition = ARTIFACT_DEFINITIONS[artifact];
  const manifestPath = path.join(directory, definition.manifest_file);
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  manifest.build.bundle_sha256 = sha256(bundle);
  manifest.build.bundle_bytes = bundle.byteLength;
  const manifestBytes = jsonBytes(manifest);
  await writeFile(path.join(directory, definition.bundle_file), bundle);
  await writeFile(manifestPath, manifestBytes);
  const indexPath = path.join(directory, "readiness-archive.json");
  const index = JSON.parse(await readFile(indexPath, "utf8"));
  const record = index.artifacts.find(
    (entry: { artifact: string }) => entry.artifact === artifact,
  );
  record.bundle_sha256 = sha256(bundle);
  record.bundle_bytes = bundle.byteLength;
  record.manifest_sha256 = sha256(manifestBytes);
  record.manifest_bytes = manifestBytes.byteLength;
  await writeFile(indexPath, jsonBytes(index));
}

async function relinkManifest(
  directory: string,
  artifact: "A" | "B",
  mutate: (manifest: Record<string, any>) => void,
) {
  const definition = ARTIFACT_DEFINITIONS[artifact];
  const manifestPath = path.join(directory, definition.manifest_file);
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  mutate(manifest);
  const manifestBytes = jsonBytes(manifest);
  await writeFile(manifestPath, manifestBytes);
  const indexPath = path.join(directory, "readiness-archive.json");
  const index = JSON.parse(await readFile(indexPath, "utf8"));
  const record = index.artifacts.find(
    (entry: { artifact: string }) => entry.artifact === artifact,
  );
  record.manifest_sha256 = sha256(manifestBytes);
  record.manifest_bytes = manifestBytes.byteLength;
  await writeFile(indexPath, jsonBytes(index));
}

async function relinkMetafile(
  directory: string,
  artifact: "A" | "B",
  metafileBytes: Buffer,
) {
  const definition = ARTIFACT_DEFINITIONS[artifact];
  await writeFile(
    path.join(directory, definition.metafile_file),
    metafileBytes,
  );
  await relinkManifest(directory, artifact, (manifest) => {
    manifest.build.metafile_sha256 = sha256(metafileBytes);
    manifest.build.metafile_bytes = metafileBytes.byteLength;
  });
  const indexPath = path.join(directory, "readiness-archive.json");
  const index = JSON.parse(await readFile(indexPath, "utf8"));
  const record = index.artifacts.find(
    (entry: { artifact: string }) => entry.artifact === artifact,
  );
  record.metafile_sha256 = sha256(metafileBytes);
  record.metafile_bytes = metafileBytes.byteLength;
  await writeFile(indexPath, jsonBytes(index));
}

async function relinkArchiveIdentity(
  directory: string,
  mutate: (index: Record<string, any>) => void,
) {
  const indexPath = path.join(directory, "readiness-archive.json");
  const index = JSON.parse(await readFile(indexPath, "utf8"));
  mutate(index);
  index.archive_id = readinessArchiveId(index.source, index.toolchain);
  for (const artifact of ["A", "B"] as const) {
    const definition = ARTIFACT_DEFINITIONS[artifact];
    const manifestPath = path.join(directory, definition.manifest_file);
    const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
    manifest.archive_id = index.archive_id;
    manifest.source = index.source;
    const manifestBytes = jsonBytes(manifest);
    await writeFile(manifestPath, manifestBytes);
    const record = index.artifacts.find(
      (entry: { artifact: string }) => entry.artifact === artifact,
    );
    record.manifest_sha256 = sha256(manifestBytes);
    record.manifest_bytes = manifestBytes.byteLength;
  }
  await writeFile(indexPath, jsonBytes(index));
}

function admissionArgs(
  directory: string,
  artifact: "A" | "B" = "A",
): string[] {
  return [
    "--expected-commit",
    COMMIT,
    "--archive-dir",
    directory,
    "--artifact",
    artifact,
    "--expected-active-version",
    ACTIVE_VERSION,
  ];
}

function admissionDependencies(
  fixture: Awaited<ReturnType<typeof fixturePair>>,
  captured = evidence(Date.parse("2026-07-27T12:00:00Z")),
) {
  const now = Date.parse("2026-07-27T12:00:00Z");
  return {
    prepare: async () => ({
      anchor: fixture.anchor,
      trustedDir: fixture.trustedDir,
    }),
    capture: async () => captured,
    write: () => {},
    now: () => now,
  };
}

function evidence(
  nowMs: number,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    database: "osl-keyserver-prod",
    database_id: READINESS_DATABASE_ID,
    environment: "production",
    schema_query_sha256: sha256(Buffer.from(READINESS_SCHEMA_QUERY)),
    markers_query_sha256: sha256(Buffer.from(READINESS_MARKERS_QUERY)),
    schema_output_sha256: "6".repeat(64),
    markers_output_sha256: "7".repeat(64),
    deployment_status_before_sha256: "8".repeat(64),
    deployment_status_after_sha256: "9".repeat(64),
    captured_started_at: new Date(nowMs - 2_000).toISOString(),
    captured_finished_at: new Date(nowMs - 1_000).toISOString(),
    database_unix_time: Math.floor((nowMs - 1_000) / 1_000),
    deployment_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    deployment_created_on: "2026-07-27T00:34:24.935251Z",
    active_worker_version: ACTIVE_VERSION,
    active_worker_percentage: 100,
    capability_table_exists: 1,
    control_inbox_sender_disposition: 1,
    control_inbox_sender_reconciliation_started: null,
    ...overrides,
  };
}

function git(repo: string, args: string[]): string {
  return execFileSync("git", ["-C", repo, ...args], {
    encoding: "utf8",
  }).trim();
}

async function gitFixture() {
  const repo = await mkdtemp(path.join(tmpdir(), "readiness-git-fixture-"));
  await mkdir(path.join(repo, "keyserver-cf", "scripts"), { recursive: true });
  for (const toolPath of Object.values(READINESS_TOOL_PATHS)) {
    await writeFile(path.join(repo, toolPath), `${toolPath}\n`);
  }
  await writeFile(path.join(repo, "keyserver-cf", "fixture.txt"), "clean\n");
  execFileSync("git", ["init", "-q", repo]);
  git(repo, ["config", "user.email", "readiness@test.invalid"]);
  git(repo, ["config", "user.name", "Readiness Test"]);
  git(repo, ["add", "keyserver-cf"]);
  git(repo, ["commit", "-qm", "fixture"]);
  return {
    repo,
    head: git(repo, ["rev-parse", "HEAD"]),
    tree: git(repo, ["rev-parse", "HEAD^{tree}"]),
    blob: git(repo, ["rev-parse", "HEAD:keyserver-cf/fixture.txt"]),
  };
}

describe("trusted readiness archive admission", () => {
  it("requires an exact operator commit, archive, artifact, and active version CLI", () => {
    expect(() =>
      parseAdmissionArgs([
        "--expected-commit",
        "HEAD",
        "--archive-dir",
        "/tmp/archive",
        "--artifact",
        "A",
        "--expected-active-version",
        ACTIVE_VERSION,
      ]),
    ).toThrow(/full 40-character/);
    expect(() =>
      parseAdmissionArgs([
        "--expected-commit",
        COMMIT,
        "--archive-dir",
        "/tmp/archive",
        "--artifact",
        "A",
        "--expected-active-version",
        ACTIVE_VERSION,
        "--evidence",
        "/tmp/caller.json",
      ]),
    ).toThrow(/usage/);
  });

  it("rejects missing, tree, blob, and stale commit objects", async () => {
    const fixture = await gitFixture();
    await expect(
      resolveCleanBuildSource(
        fixture.repo,
        "f".repeat(40),
        path.join(tmpdir(), "out"),
      ),
    ).rejects.toThrow();
    for (const objectId of [fixture.tree, fixture.blob]) {
      await expect(
        resolveCleanBuildSource(
          fixture.repo,
          objectId,
          path.join(tmpdir(), "out"),
        ),
      ).rejects.toThrow();
    }
    await writeFile(
      path.join(fixture.repo, "keyserver-cf", "fixture.txt"),
      "successor\n",
    );
    git(fixture.repo, ["add", "keyserver-cf/fixture.txt"]);
    git(fixture.repo, ["commit", "-qm", "successor"]);
    await expect(
      resolveCleanBuildSource(
        fixture.repo,
        fixture.head,
        path.join(tmpdir(), "out"),
      ),
    ).rejects.toThrow(/not the checked-out HEAD/);
  });

  it("recomputes repository, keyserver, and tool identities from the expected commit", async () => {
    const fixture = await gitFixture();
    const anchor = await resolveCleanBuildSource(
      fixture.repo,
      fixture.head,
      path.join(tmpdir(), "readiness-unused-output"),
    );
    expect(anchor.commit).toBe(fixture.head);
    expect(anchor.repositoryTree).toBe(fixture.tree);
    expect(anchor.keyserverTree).toBe(
      git(fixture.repo, ["rev-parse", `${fixture.head}:keyserver-cf`]),
    );
    for (const [name, toolPath] of Object.entries(READINESS_TOOL_PATHS)) {
      const bytes = await readFile(path.join(fixture.repo, toolPath));
      expect(anchor.toolchain[name]).toEqual({
        path: toolPath,
        sha256: sha256(bytes),
        bytes: bytes.byteLength,
      });
    }
  });

  it("rejects tracked, staged, deleted, and untracked dirty invocation", async () => {
    const mutations = [
      async (fixture: Awaited<ReturnType<typeof gitFixture>>) => {
        await writeFile(
          path.join(fixture.repo, "keyserver-cf", "fixture.txt"),
          "modified\n",
        );
      },
      async (fixture: Awaited<ReturnType<typeof gitFixture>>) => {
        await writeFile(
          path.join(fixture.repo, "keyserver-cf", "fixture.txt"),
          "staged\n",
        );
        git(fixture.repo, ["add", "keyserver-cf/fixture.txt"]);
      },
      async (fixture: Awaited<ReturnType<typeof gitFixture>>) => {
        await unlink(path.join(fixture.repo, "keyserver-cf", "fixture.txt"));
      },
      async (fixture: Awaited<ReturnType<typeof gitFixture>>) => {
        await writeFile(path.join(fixture.repo, "untracked"), "untracked\n");
      },
    ];
    for (const mutate of mutations) {
      const fixture = await gitFixture();
      await mutate(fixture);
      await expect(
        resolveCleanBuildSource(
          fixture.repo,
          fixture.head,
          path.join(tmpdir(), "out"),
        ),
      ).rejects.toThrow(/completely clean checkout/);
    }
  });

  it("rejects self-consistent stale commit and tree identities", async () => {
    for (const field of [
      "commit",
      "repository_tree",
      "keyserver_tree",
    ] as const) {
      const stale = await fixturePair();
      await relinkArchiveIdentity(stale.candidateDir, (index) => {
        index.source[field] = "a".repeat(40);
      });
      await expect(
        verifyArchiveDirectory(stale.candidateDir, stale.anchor),
        field,
      ).rejects.toThrow(/expected Git objects/);
    }
  });

  it("rejects a predecessor source.tar with every caller hash relinked", async () => {
    const replaced = await fixturePair();
    const predecessor = Buffer.from("predecessor source archive\n");
    await writeFile(
      path.join(replaced.candidateDir, "source.tar"),
      predecessor,
    );
    await relinkArchiveIdentity(replaced.candidateDir, (index) => {
      index.source.archive_sha256 = sha256(predecessor);
      index.source.archive_bytes = predecessor.byteLength;
    });
    await expect(
      runAdmissionCli(
        admissionArgs(replaced.candidateDir),
        admissionDependencies(replaced),
      ),
    ).rejects.toThrow(/differs from trusted rebuild/);
  });

  it("rejects swapped pairs and marker-string fake bundles", async () => {
    const swapped = await fixturePair();
    const a = await readFile(
      path.join(swapped.candidateDir, "artifact-a.bridge.mjs"),
    );
    const b = await readFile(
      path.join(swapped.candidateDir, "artifact-b.final.mjs"),
    );
    await writeFile(path.join(swapped.candidateDir, "artifact-a.bridge.mjs"), b);
    await writeFile(path.join(swapped.candidateDir, "artifact-b.final.mjs"), a);
    await expect(
      verifyArchiveDirectory(swapped.candidateDir, swapped.anchor),
    ).rejects.toThrow(/bundle/);

    const fake = await fixturePair();
    await relinkBundle(
      fake.candidateDir,
      "A",
      Buffer.from(BRIDGE_REQUIRED_TEXT.join("\n")),
    );
    await expect(
      verifyArchiveDirectory(fake.candidateDir, fake.anchor),
    ).rejects.toThrow(/trusted command failed/);
  });

  it("rejects a fake manifest plus executable fake bundle after all local relinking", async () => {
    const fixture = await fixturePair();
    const fake = Buffer.from(
      `const tokens=${JSON.stringify(BRIDGE_REQUIRED_TEXT)};` +
        "export default {fetch(){return new Response(tokens.join(','));}};\n",
    );
    await relinkBundle(fixture.candidateDir, "A", fake);
    await relinkManifest(fixture.candidateDir, "A", (manifest) => {
      manifest.build.wrangler_version = "9.9.9";
    });
    await expect(
      runAdmissionCli(
        admissionArgs(fixture.candidateDir),
        admissionDependencies(fixture),
      ),
    ).rejects.toThrow(/differs from trusted rebuild/);
  });

  it("rejects missing and crossed index links", async () => {
    const mutations = [
      (index: Record<string, any>) => {
        index.artifacts[0].manifest_sha256 = "0".repeat(64);
      },
      (index: Record<string, any>) => {
        index.artifacts[1].metafile_sha256 = "0".repeat(64);
      },
      (index: Record<string, any>) => {
        const manifest = index.artifacts[0].manifest_file;
        index.artifacts[0].manifest_file =
          index.artifacts[1].manifest_file;
        index.artifacts[1].manifest_file = manifest;
      },
      (index: Record<string, any>) => {
        const artifact = index.artifacts[0].artifact;
        index.artifacts[0].artifact = index.artifacts[1].artifact;
        index.artifacts[1].artifact = artifact;
      },
    ];
    for (const mutate of mutations) {
      const fixture = await fixturePair();
      const indexPath = path.join(
        fixture.candidateDir,
        "readiness-archive.json",
      );
      const index = JSON.parse(await readFile(indexPath, "utf8"));
      mutate(index);
      await writeFile(indexPath, jsonBytes(index));
      await expect(
        verifyArchiveDirectory(fixture.candidateDir, fixture.anchor),
      ).rejects.toThrow();
    }
  });

  it("rejects builder/verifier source swaps after archive-id relinking", async () => {
    const fixture = await fixturePair();
    await relinkArchiveIdentity(fixture.candidateDir, (index) => {
      const builder = index.toolchain.builder;
      const verifier = index.toolchain.verifier;
      [builder.sha256, verifier.sha256] = [verifier.sha256, builder.sha256];
      [builder.bytes, verifier.bytes] = [verifier.bytes, builder.bytes];
    });
    await expect(
      verifyArchiveDirectory(fixture.candidateDir, fixture.anchor),
    ).rejects.toThrow(/source is not pinned/);
  });

  it("rejects a caller-authored metafile after every local hash is relinked", async () => {
    const fixture = await fixturePair();
    const fakeMetafile = metafile("A");
    (fakeMetafile.inputs as Record<string, unknown>)["caller/fake.ts"] = {
      bytes: 1,
    };
    await relinkMetafile(
      fixture.candidateDir,
      "A",
      jsonBytes(fakeMetafile),
    );
    await expect(
      runAdmissionCli(
        admissionArgs(fixture.candidateDir),
        admissionDependencies(fixture),
      ),
    ).rejects.toThrow(/differs from trusted rebuild/);
  });

  it("pins production Worker, environment, D1 binding, name, and id together", () => {
    const config = `name = "oslprivacy-keyserver"
[vars]
DEPLOYMENT_ENV = "production"
[[d1_databases]]
binding = "DB"
database_name = "osl-keyserver-prod"
database_id = "${READINESS_DATABASE_ID}"
`;
    expect(validateProductionConfig(config)).toBe(true);
    for (const mutation of [
      config.replace('DEPLOYMENT_ENV = "production"', 'DEPLOYMENT_ENV = "preview"'),
      config.replace(READINESS_DATABASE_ID, "00000000-0000-4000-8000-000000000000"),
      config.replace('binding = "DB"', 'binding = "OTHER"'),
      `${config}
[[d1_databases]]
binding = "OTHER"
database_name = "osl-keyserver-prod"
database_id = "${READINESS_DATABASE_ID}"
`,
    ]) {
      expect(() => validateProductionConfig(mutation)).toThrow();
    }
  });

  it("requires fresh exact D1/deployment provenance", () => {
    const now = Date.parse("2026-07-27T12:00:00Z");
    expect(validateCapturedEvidence(evidence(now), ACTIVE_VERSION, now)).toEqual({
      capability_table_exists: 1,
      control_inbox_sender_disposition: 1,
      control_inbox_sender_reconciliation_started: null,
    });
    for (const mutation of [
      { database: "caller-db" },
      { database_id: "00000000-0000-4000-8000-000000000000" },
      { environment: "preview" },
      { schema_query_sha256: "0".repeat(64) },
      { markers_query_sha256: "0".repeat(64) },
      { deployment_status_before_sha256: "not-a-hash" },
      { deployment_status_after_sha256: "not-a-hash" },
      { active_worker_version: "99999999-2222-4333-8444-555555555555" },
      { active_worker_percentage: 99 },
      { deployment_id: "not-a-deployment" },
      { deployment_created_on: "2026-07-28T12:00:00Z" },
      { database_unix_time: Math.floor((now - 20_000) / 1_000) },
      {
        captured_started_at: new Date(now - 500_000).toISOString(),
        captured_finished_at: new Date(now - 400_000).toISOString(),
        database_unix_time: Math.floor((now - 400_000) / 1_000),
      },
    ]) {
      expect(
        () => validateCapturedEvidence(evidence(now, mutation), ACTIVE_VERSION, now),
        JSON.stringify(mutation),
      ).toThrow();
    }
  });

  it("requires one exact active deployment before and after D1 capture", () => {
    const status = {
      id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      created_on: "2026-07-27T00:34:24.935251Z",
      versions: [{ version_id: ACTIVE_VERSION, percentage: 100 }],
    };
    const before = activeDeployment(status, ACTIVE_VERSION);
    expect(
      requireStableDeployment(before, activeDeployment(status, ACTIVE_VERSION)),
    ).toEqual(before);
    expect(() =>
      activeDeployment(status, "99999999-2222-4333-8444-555555555555"),
    ).toThrow(/operator expectation/);
    expect(() =>
      requireStableDeployment(before, {
        ...before,
        deployment_id: "ffffffff-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      }),
    ).toThrow(/changed during D1 capture/);
  });

  it("admits only the trusted bytes and enforces migration ordering", async () => {
    const fixture = await fixturePair();
    const now = Date.parse("2026-07-27T12:00:00Z");
    const args = (artifact: "A" | "B") =>
      admissionArgs(fixture.candidateDir, artifact);
    const dependencies = (
      captured: Record<string, unknown>,
    ) => ({
      prepare: async () => ({
        anchor: fixture.anchor,
        trustedDir: fixture.trustedDir,
      }),
      capture: async () => captured,
      write: () => {},
      now: () => now,
    });
    const admitted = await runAdmissionCli(
      args("A"),
      dependencies(evidence(now)),
    );
    expect(admitted).toMatchObject({
      format: "osl.keyserver.readiness-admission.v2",
      admitted: true,
      expected_commit: COMMIT,
      artifact: "A",
      active_worker_version: ACTIVE_VERSION,
      capability_table_exists: 1,
      markers: {
        control_inbox_sender_disposition: 1,
        control_inbox_sender_reconciliation_started: null,
      },
    });
    expect(admitted.artifact_bundles).toEqual(
      Object.fromEntries(
        fixture.index.artifacts.map((entry) => [
          entry.artifact,
          entry.bundle_sha256,
        ]),
      ),
    );
    expect(admitted.artifact_bundles.A).not.toBe(
      admitted.artifact_bundles.B,
    );
    await expect(
      runAdmissionCli(
        args("A"),
        dependencies(
          evidence(now, {
            control_inbox_sender_reconciliation_started: 1,
          }),
        ),
      ),
    ).rejects.toThrow(/artifact A is forbidden/);
    await expect(
      runAdmissionCli(
        args("B"),
        dependencies(
          evidence(now, {
            control_inbox_sender_disposition: null,
            control_inbox_sender_reconciliation_started: null,
          }),
        ),
      ),
    ).rejects.toThrow(
      /migration 0031 capability table and exact disposition marker/,
    );
  });
});
