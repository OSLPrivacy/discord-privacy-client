import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

export const READINESS_MANIFEST_FORMAT =
  "osl.keyserver.readiness-artifact.v2";
export const READINESS_ARCHIVE_FORMAT =
  "osl.keyserver.readiness-archive.v2";

export const READINESS_TOOL_PATHS = Object.freeze({
  builder: "keyserver-cf/scripts/build-readiness-artifacts.mjs",
  verifier: "keyserver-cf/scripts/admit-readiness-archive.mjs",
  contract: "keyserver-cf/scripts/readiness-artifact-contract.mjs",
});

export const READINESS_FILES = Object.freeze([
  "source.tar",
  "artifact-a.bridge.mjs",
  "artifact-a.bridge.meta.json",
  "artifact-a.bridge.manifest.json",
  "artifact-b.final.mjs",
  "artifact-b.final.meta.json",
  "artifact-b.final.manifest.json",
  "readiness-archive.json",
]);

export const BRIDGE_ALIASES = Object.freeze({
  "./endpoints/control-inbox.js":
    "./src/readiness/bridge/control-inbox.ts",
  "./endpoints/healthz.js":
    "./src/readiness/bridge/healthz.ts",
  "./lib/control-inbox-sweep.js":
    "./src/readiness/bridge/control-inbox-sweep.ts",
});

export const BRIDGE_REQUIRED_TEXT = Object.freeze([
  "reserved derived identity namespace requires root proof verification",
  "paid checkout is unavailable until prepaid-code redemption is ready",
  "A-pre-0031-bridge",
  "control inbox unavailable during schema transition",
]);

export const MIGRATION_0031_SURFACES = Object.freeze([
  "worker_schema_capabilities",
  "control_inbox_sender_disposition",
  "control_inbox_sender_reconciliation_started",
  "delivery_status",
  "delivery_reason",
  "delivery_attempts",
  "sender_disabled_first_seen_at",
  "delivery_next_retry_at",
  "delivery_retain_until",
]);

export const ARTIFACT_DEFINITIONS = Object.freeze({
  A: Object.freeze({
    artifact: "A",
    role: "pre-0031-bridge",
    bundle_file: "artifact-a.bridge.mjs",
    metafile_file: "artifact-a.bridge.meta.json",
    manifest_file: "artifact-a.bridge.manifest.json",
    aliases: BRIDGE_ALIASES,
    requires_0031: false,
    forbidden_after_reconciliation: true,
  }),
  B: Object.freeze({
    artifact: "B",
    role: "0031-aware-final",
    bundle_file: "artifact-b.final.mjs",
    metafile_file: "artifact-b.final.meta.json",
    manifest_file: "artifact-b.final.manifest.json",
    aliases: Object.freeze({}),
    requires_0031: true,
    forbidden_after_reconciliation: false,
  }),
});

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function canonicalJson(value) {
  if (Array.isArray(value)) {
    return `[${value.map((item) => canonicalJson(item)).join(",")}]`;
  }
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

export function readinessArchiveId(source, toolchain) {
  return sha256(
    Buffer.from(
      canonicalJson({
        format: READINESS_ARCHIVE_FORMAT,
        source,
        toolchain,
      }),
    ),
  );
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requireExactKeys(value, expected, label) {
  const actual = Object.keys(requireObject(value, label)).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) {
    throw new Error(`${label} fields are not exact`);
  }
}

function requireGitObject(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{40}$/.test(value)) {
    throw new Error(`${label} is not a full Git object id`);
  }
}

function requireSha256(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value)) {
    throw new Error(`${label} is not a SHA-256`);
  }
}

function requirePositiveBytes(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${label} must be nonzero`);
  }
}

export function validateSourceIdentity(sourceValue) {
  const source = requireObject(sourceValue, "source");
  requireExactKeys(
    source,
    [
      "commit",
      "repository_tree",
      "keyserver_tree",
      "archive_file",
      "archive_sha256",
      "archive_bytes",
    ],
    "source",
  );
  requireGitObject(source.commit, "source.commit");
  requireGitObject(source.repository_tree, "source.repository_tree");
  requireGitObject(source.keyserver_tree, "source.keyserver_tree");
  if (source.archive_file !== "source.tar") {
    throw new Error("source archive filename is not exact");
  }
  requireSha256(source.archive_sha256, "source.archive_sha256");
  requirePositiveBytes(source.archive_bytes, "source.archive_bytes");
  return source;
}

export function validateToolchain(toolchainValue) {
  const toolchain = requireObject(toolchainValue, "toolchain");
  requireExactKeys(
    toolchain,
    ["builder", "verifier", "contract", "clean_checkout_required"],
    "toolchain",
  );
  if (toolchain.clean_checkout_required !== true) {
    throw new Error("clean checkout is not required");
  }
  for (const [name, expectedPath] of Object.entries(READINESS_TOOL_PATHS)) {
    const tool = requireObject(toolchain[name], `toolchain.${name}`);
    requireExactKeys(tool, ["path", "sha256", "bytes"], `toolchain.${name}`);
    if (tool.path !== expectedPath) {
      throw new Error(`toolchain.${name} path is not exact`);
    }
    requireSha256(tool.sha256, `toolchain.${name}.sha256`);
    requirePositiveBytes(tool.bytes, `toolchain.${name}.bytes`);
  }
  return toolchain;
}

export function validateReadinessManifest(manifestValue, expectedArchiveId) {
  const manifest = requireObject(manifestValue, "manifest");
  requireExactKeys(
    manifest,
    ["format", "archive_id", "artifact", "role", "source", "build", "policy"],
    "manifest",
  );
  if (manifest.format !== READINESS_MANIFEST_FORMAT) {
    throw new Error("unknown readiness manifest format");
  }
  const definition = ARTIFACT_DEFINITIONS[manifest.artifact];
  if (!definition) throw new Error("readiness artifact must be A or B");
  requireSha256(manifest.archive_id, "manifest.archive_id");
  if (
    expectedArchiveId !== undefined &&
    manifest.archive_id !== expectedArchiveId
  ) {
    throw new Error("manifest archive id does not match archive index");
  }
  validateSourceIdentity(manifest.source);
  if (manifest.role !== definition.role) {
    throw new Error("manifest role does not match its artifact");
  }
  const build = requireObject(manifest.build, "manifest.build");
  requireExactKeys(
    build,
    [
      "entrypoint",
      "aliases",
      "command",
      "wrangler_version",
      "bundle_file",
      "bundle_sha256",
      "bundle_bytes",
      "metafile_file",
      "metafile_sha256",
      "metafile_bytes",
    ],
    "manifest.build",
  );
  if (build.entrypoint !== "src/index.ts") {
    throw new Error("readiness entrypoint is not src/index.ts");
  }
  if (build.bundle_file !== definition.bundle_file) {
    throw new Error("bundle filename does not match its artifact");
  }
  if (build.metafile_file !== definition.metafile_file) {
    throw new Error("metafile filename does not match its artifact");
  }
  if (JSON.stringify(build.aliases) !== JSON.stringify(definition.aliases)) {
    throw new Error("build aliases do not match the artifact closure");
  }
  if (
    !Array.isArray(build.command) ||
    !build.command.includes("--dry-run") ||
    !build.command.includes("--minify") ||
    build.command.includes("--env")
  ) {
    throw new Error("build command is not the production-config dry run");
  }
  if (typeof build.wrangler_version !== "string" ||
      !/^\d+\.\d+\.\d+$/.test(build.wrangler_version)) {
    throw new Error("Wrangler version is not exact");
  }
  requireSha256(build.bundle_sha256, "build.bundle_sha256");
  requirePositiveBytes(build.bundle_bytes, "build.bundle_bytes");
  requireSha256(build.metafile_sha256, "build.metafile_sha256");
  requirePositiveBytes(build.metafile_bytes, "build.metafile_bytes");
  const policy = requireObject(manifest.policy, "manifest.policy");
  requireExactKeys(
    policy,
    ["requires_0031", "forbidden_after_reconciliation"],
    "manifest.policy",
  );
  if (
    policy.requires_0031 !== definition.requires_0031 ||
    policy.forbidden_after_reconciliation !==
      definition.forbidden_after_reconciliation
  ) {
    throw new Error(`artifact ${manifest.artifact} policy is not fail closed`);
  }
  return manifest;
}

export function validateReadinessMetafile(artifact, metafileValue) {
  const definition = ARTIFACT_DEFINITIONS[artifact];
  if (!definition) throw new Error("metafile artifact must be A or B");
  const metafile = requireObject(metafileValue, "metafile");
  const inputs = Object.keys(requireObject(metafile.inputs, "metafile.inputs"));
  if (inputs.length === 0 || !inputs.includes("src/index.ts")) {
    throw new Error("metafile input closure is empty or lacks src/index.ts");
  }
  const bridgeInputs = [
    "src/readiness/bridge/control-inbox.ts",
    "src/readiness/bridge/healthz.ts",
    "src/readiness/bridge/control-inbox-sweep.ts",
  ];
  const finalInputs = [
    "src/endpoints/control-inbox.ts",
    "src/endpoints/healthz.ts",
    "src/lib/control-inbox-sweep.ts",
  ];
  const required = artifact === "A" ? bridgeInputs : finalInputs;
  const forbidden = artifact === "A" ? finalInputs : bridgeInputs;
  for (const input of required) {
    if (!inputs.includes(input)) {
      throw new Error(`metafile is missing required ${artifact} input: ${input}`);
    }
  }
  for (const input of forbidden) {
    if (inputs.includes(input)) {
      throw new Error(`metafile contains forbidden ${artifact} input: ${input}`);
    }
  }
  const outputs = Object.values(
    requireObject(metafile.outputs, "metafile.outputs"),
  );
  const executable = outputs.filter(
    (output) => output?.entryPoint === "src/index.ts",
  );
  if (executable.length !== 1) {
    throw new Error("metafile must contain one executable src/index.ts output");
  }
  return metafile;
}

export function verifyReadinessBundle(manifestValue, bundleBytes) {
  const manifest = validateReadinessManifest(manifestValue);
  if (bundleBytes.byteLength !== manifest.build.bundle_bytes) {
    throw new Error("bundle byte count does not match manifest");
  }
  if (sha256(bundleBytes) !== manifest.build.bundle_sha256) {
    throw new Error("bundle SHA-256 does not match manifest");
  }
  const text = new TextDecoder().decode(bundleBytes);
  if (manifest.artifact === "A") {
    for (const required of BRIDGE_REQUIRED_TEXT) {
      if (!text.includes(required)) {
        throw new Error(`bridge is missing required refusal: ${required}`);
      }
    }
    for (const forbidden of MIGRATION_0031_SURFACES) {
      if (text.includes(forbidden)) {
        throw new Error(`bridge contains migration 0031 surface: ${forbidden}`);
      }
    }
  } else {
    for (const required of MIGRATION_0031_SURFACES) {
      if (!text.includes(required)) {
        throw new Error(`final bundle is missing migration 0031 surface: ${required}`);
      }
    }
  }
  return manifest;
}

export async function readAndVerifyReadinessArtifact(
  manifestPath,
  bundlePath,
  metafilePath,
  expectedArchiveId,
) {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  validateReadinessManifest(manifest, expectedArchiveId);
  const bundle = await readFile(bundlePath);
  verifyReadinessBundle(manifest, bundle);
  const metafileBytes = await readFile(metafilePath);
  if (metafileBytes.byteLength !== manifest.build.metafile_bytes) {
    throw new Error("metafile byte count does not match manifest");
  }
  if (sha256(metafileBytes) !== manifest.build.metafile_sha256) {
    throw new Error("metafile SHA-256 does not match manifest");
  }
  validateReadinessMetafile(
    manifest.artifact,
    JSON.parse(metafileBytes.toString("utf8")),
  );
  return manifest;
}

export function validateReadinessArchiveIndex(indexValue) {
  const index = requireObject(indexValue, "archive index");
  requireExactKeys(
    index,
    ["format", "archive_id", "source", "toolchain", "artifacts"],
    "archive index",
  );
  if (index.format !== READINESS_ARCHIVE_FORMAT) {
    throw new Error("unknown readiness archive format");
  }
  const source = validateSourceIdentity(index.source);
  const toolchain = validateToolchain(index.toolchain);
  requireSha256(index.archive_id, "archive index id");
  if (index.archive_id !== readinessArchiveId(source, toolchain)) {
    throw new Error("archive index id is not derived from source and toolchain");
  }
  if (!Array.isArray(index.artifacts) || index.artifacts.length !== 2) {
    throw new Error("archive index must contain exactly two artifacts");
  }
  const seen = new Set();
  for (const entryValue of index.artifacts) {
    const entry = requireObject(entryValue, "archive artifact entry");
    requireExactKeys(
      entry,
      [
        "artifact",
        "role",
        "manifest_file",
        "manifest_sha256",
        "manifest_bytes",
        "bundle_file",
        "bundle_sha256",
        "bundle_bytes",
        "metafile_file",
        "metafile_sha256",
        "metafile_bytes",
        "policy",
      ],
      "archive artifact entry",
    );
    const definition = ARTIFACT_DEFINITIONS[entry.artifact];
    if (!definition || seen.has(entry.artifact)) {
      throw new Error("archive index artifacts are missing or duplicated");
    }
    seen.add(entry.artifact);
    if (
      entry.role !== definition.role ||
      entry.manifest_file !== definition.manifest_file ||
      entry.bundle_file !== definition.bundle_file ||
      entry.metafile_file !== definition.metafile_file
    ) {
      throw new Error(`archive index ${entry.artifact} filenames/role drifted`);
    }
    for (const field of [
      "manifest_sha256",
      "bundle_sha256",
      "metafile_sha256",
    ]) {
      requireSha256(entry[field], `archive entry ${field}`);
    }
    for (const field of [
      "manifest_bytes",
      "bundle_bytes",
      "metafile_bytes",
    ]) {
      requirePositiveBytes(entry[field], `archive entry ${field}`);
    }
    if (
      entry.policy.requires_0031 !== definition.requires_0031 ||
      entry.policy.forbidden_after_reconciliation !==
        definition.forbidden_after_reconciliation
    ) {
      throw new Error(`archive index ${entry.artifact} policy drifted`);
    }
  }
  return index;
}

export function validateReadinessSelection(manifestValue, evidenceValue) {
  const manifest = validateReadinessManifest(manifestValue);
  const evidence = requireObject(evidenceValue, "database evidence");
  requireExactKeys(
    evidence,
    [
      "control_inbox_sender_disposition",
      "control_inbox_sender_reconciliation_started",
    ],
    "database evidence",
  );
  const disposition = evidence.control_inbox_sender_disposition;
  const reconciliation = evidence.control_inbox_sender_reconciliation_started;
  if (![null, 1].includes(disposition)) {
    throw new Error("disposition marker must be exactly 1 or null");
  }
  if (![null, 1].includes(reconciliation)) {
    throw new Error("reconciliation marker must be exactly 1 or null");
  }
  if (reconciliation === 1 && disposition !== 1) {
    throw new Error("reconciliation evidence is internally inconsistent");
  }
  if (manifest.artifact === "A" && reconciliation === 1) {
    throw new Error(
      "artifact A is forbidden after control-inbox reconciliation starts",
    );
  }
  if (manifest.artifact === "B" && disposition !== 1) {
    throw new Error("artifact B requires the exact migration 0031 marker");
  }
  return true;
}
