#!/usr/bin/env node

// Bind public capability labels to the evidence authority.  The website
// already renders badges from capability_registry; this closes the remaining
// gap by rejecting a registry status which disagrees with the allowlist row.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const allowlistPath = path.join(root, 'docs/design/osl-public-claim-allowlist.md');
const pricingPath = path.join(root, 'data/pricing.json');

// These labels intentionally point at the exact allowlist wording rather than
// duplicating a status. A new public capability needs an explicit binding.
export const ALLOWLIST_BINDINGS = {
  burn: 'Bilateral burn',
  'scrub-discovery': 'Scrub discovery ("find the accounts you left behind")',
  'scrub-guided-deletion': 'Scrub guided deletion handoff',
  autoscrub: 'AutoScrub',
  'image-send': 'View-once, timed deletion, attachments',
  'group-protection': 'OSL Spaces',
};

function badgeForEvidenceStatus(status) {
  if (/verified-live/.test(status)) return 'Available';
  if (/runtime-proven|test-proven-only/.test(status)) return 'Beta';
  if (/implemented-unwired|designed-only|unknown-recheck-required/.test(status)) return 'Planned';
  return null;
}

export function allowlistBadge(source, label) {
  const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const row = new RegExp('^\\|\\s*' + escaped + '\\s*\\|\\s*`?([^|`]+)`?', 'mu').exec(source);
  if (!row) return null;
  return badgeForEvidenceStatus(row[1]);
}

export function validateAllowlistSync(allowlist, pricing, bindings = ALLOWLIST_BINDINGS) {
  const errors = [];
  const registry = Array.isArray(pricing.capability_registry) ? pricing.capability_registry : [];
  for (const [id, label] of Object.entries(bindings)) {
    const capability = registry.find((entry) => entry?.id === id);
    if (!capability) {
      errors.push(`missing capability_registry row for ${id}`);
      continue;
    }
    const expected = allowlistBadge(allowlist, label);
    if (!expected) {
      errors.push(`allowlist row ${JSON.stringify(label)} has no parseable evidence status for ${id}`);
    } else if (capability.status !== expected) {
      errors.push(`${id} is ${capability.status} but allowlist ${JSON.stringify(label)} earns ${expected}`);
    }
  }
  return errors;
}

function run() {
  const errors = validateAllowlistSync(readFileSync(allowlistPath, 'utf8'), JSON.parse(readFileSync(pricingPath, 'utf8')));
  if (errors.length) throw new Error(`allowlist sync failed:\n${errors.join('\n')}`);
  console.log(`check-allowlist-sync: ${Object.keys(ALLOWLIST_BINDINGS).length} evidence bindings agree.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { run(); } catch (error) { console.error(`check-allowlist-sync: ${error.message}`); process.exit(1); }
}
