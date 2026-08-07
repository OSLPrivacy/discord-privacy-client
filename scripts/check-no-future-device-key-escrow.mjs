#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

export const SURFACES = [
  {
    name: "key server tables",
    patterns: ["keyserver-cf/migrations/**/*.sql", "keyserver-cf/migrations-contract/**/*.sql"],
    roots: ["keyserver-cf/migrations", "keyserver-cf/migrations-contract"],
    extensions: [".sql"],
  },
  {
    name: "blob store",
    patterns: [
      "cipher-store-cf/migrations/**/*.sql",
      "cipher-store-cf/src/endpoints/blob.ts",
      "cipher-store-cf/src/endpoints/receipt.ts",
      "cipher-store-cf/src/lib/payload-store.ts",
      "cipher-store-cf/src/env.ts",
    ],
    roots: ["cipher-store-cf/migrations"],
    files: [
      "cipher-store-cf/src/endpoints/blob.ts",
      "cipher-store-cf/src/endpoints/receipt.ts",
      "cipher-store-cf/src/lib/payload-store.ts",
      "cipher-store-cf/src/env.ts",
    ],
    extensions: [".sql"],
  },
  {
    name: "local sealed store",
    patterns: [
      "crates/ipc/src/secure_local_store.rs",
      "crates/ipc/src/scope_blobs_file.rs",
      "crates/ipc/src/rn_plaintext_cache.rs",
      "apps/osl-hub/src/account_recovery.rs",
      "apps/osl-hub/src/message_expiry.rs",
      "apps/osl-hub/src/osl_profile.rs",
      "apps/osl-hub-ui/src/main.ts",
    ],
    files: [
      "crates/ipc/src/secure_local_store.rs",
      "crates/ipc/src/scope_blobs_file.rs",
      "crates/ipc/src/rn_plaintext_cache.rs",
      "apps/osl-hub/src/account_recovery.rs",
      "apps/osl-hub/src/message_expiry.rs",
      "apps/osl-hub/src/osl_profile.rs",
      "apps/osl-hub-ui/src/main.ts",
    ],
  },
  {
    name: "sync payload shapes",
    patterns: [
      "crates/ipc/src/ordinary_sync.rs",
      "apps/osl-hub-ui/src/state.ts",
      "apps/osl-hub-ui/src/device-transfer.ts",
      "apps/osl-hub-ui/src/device-transfer-source.ts",
    ],
    files: [
      "crates/ipc/src/ordinary_sync.rs",
      "apps/osl-hub-ui/src/state.ts",
      "apps/osl-hub-ui/src/device-transfer.ts",
      "apps/osl-hub-ui/src/device-transfer-source.ts",
    ],
  },
  {
    name: "backup/export payloads",
    patterns: [
      "apps/osl-hub/src/update_state_backup.rs",
      "apps/osl-hub/src/update_apply.rs",
      "apps/osl-hub-ui/src/local-message-import.ts",
      "apps/osl-hub-ui/src/osl-import.ts",
      "apps/osl-hub-ui/src/osl-export.ts",
    ],
    files: [
      "apps/osl-hub/src/update_state_backup.rs",
      "apps/osl-hub/src/update_apply.rs",
      "apps/osl-hub-ui/src/local-message-import.ts",
      "apps/osl-hub-ui/src/osl-import.ts",
      "apps/osl-hub-ui/src/osl-export.ts",
    ],
  },
];

const FORBIDDEN_FIELD =
  /\b[A-Za-z0-9_]*(?:message[_-]?key|messageKey|chain[_-]?key|chainKey|root[_-]?key|rootKey)[A-Za-z0-9_]*\b/g;

function walk(directory, extensions) {
  if (!existsSync(directory)) return [];
  const entries = readdirSync(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(absolute, extensions));
    } else if (entry.isFile() && extensions.includes(path.extname(entry.name))) {
      files.push(absolute);
    }
  }
  return files;
}

function surfaceFiles(root, surface) {
  const files = new Set();
  for (const relative of surface.files ?? []) {
    const absolute = path.join(root, relative);
    if (existsSync(absolute) && statSync(absolute).isFile()) files.add(absolute);
  }
  for (const relative of surface.roots ?? []) {
    const absolute = path.join(root, relative);
    for (const file of walk(absolute, surface.extensions ?? [])) files.add(file);
  }
  return [...files].sort((left, right) => left.localeCompare(right));
}

function displayPath(root, absolute) {
  const relative = path.relative(root, absolute);
  return relative.startsWith("..") ? absolute : relative;
}

export function scanFutureDeviceKeyEscrow(root = REPO_ROOT) {
  const resolvedRoot = path.resolve(root);
  const checked = [];
  const hits = [];

  for (const surface of SURFACES) {
    const files = surfaceFiles(resolvedRoot, surface);
    checked.push({
      name: surface.name,
      files: files.length,
      patterns: surface.patterns,
    });

    for (const file of files) {
      const source = readFileSync(file, "utf8");
      const lines = source.split(/\r?\n/u);
      for (let index = 0; index < lines.length; index += 1) {
        const line = lines[index];
        for (const match of line.matchAll(FORBIDDEN_FIELD)) {
          hits.push({
            surface: surface.name,
            file: displayPath(resolvedRoot, file),
            line: index + 1,
            field: match[0],
          });
        }
      }
    }
  }

  return { root: resolvedRoot, checked, hits };
}

export function formatReport(result) {
  const lines = [
    "TASK4806 future-device key escrow refusal scan",
    `root: ${result.root}`,
    "looked:",
  ];
  for (const surface of result.checked) {
    lines.push(
      `- ${surface.name}: ${surface.files} files (${surface.patterns.join(", ")})`,
    );
  }
  lines.push(`hits: ${result.hits.length}`);
  for (const hit of result.hits) {
    lines.push(`${hit.surface}: ${hit.file}:${hit.line} field ${hit.field}`);
  }
  return lines.join("\n");
}

function main() {
  const root = process.argv[2] ? path.resolve(process.argv[2]) : REPO_ROOT;
  const result = scanFutureDeviceKeyEscrow(root);
  console.log(formatReport(result));
  if (result.hits.length > 0) process.exit(1);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
