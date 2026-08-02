#!/usr/bin/env node

// Keep the master decision's detailed-source index deliverable: links must
// resolve to substantive local documents, not merely prose that names them.
import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const DETAILED_SOURCES = [
  'docs/design/osl-plan-b-safety-first.md',
  'docs/design/osl-plan-a-ship-first.md',
  'docs/OSL-DISCORD-STATE-MAP.md',
  'docs/design/osl-adapter-playbook.md',
  'docs/security/osl-audit-2026-07-26-codex.md',
  'docs/design/osl-requirements-addendum-2026-07-26.md',
  'docs/design/osl-completion-plan-2026-07-26.md',
  'docs/qa/two-identity-p2p-verification.md',
  'docs/testing/hub-release-candidate-vm-gate.md',
  'docs/plans/scrub-to-spec-plan.md',
  'docs/plans/scrub-autoscrub-architecture.md',
  'docs/plans/scrub-detection-opus5-plan.md',
];

export const RESUME_HERE_FIELDS = [
  'Resume here',
  'Current verified state:',
  'Exact build/worktree:',
  'Current owner and exclusive files:',
  'Next unblocked action:',
  'Command/scenario:',
  'Expected result:',
  'Known blocker/risk:',
  'Master/internal-checklist rows to update on completion:',
];

export function validateDetailedSources(read = (file) => readFileSync(file, 'utf8')) {
  const missing = [];
  for (const relative of DETAILED_SOURCES) {
    const file = path.join(root, relative);
    if (!existsSync(file)) {
      missing.push(relative);
      continue;
    }
    const source = read(file).trim();
    if (source.length < 80 || !source.endsWith(RESUME_HERE_FIELDS.join('\n'))) missing.push(relative);
  }
  return missing;
}

const missing = validateDetailedSources();
if (missing.length) {
  console.error(`detailed-source-index: missing or empty: ${missing.join(', ')}`);
  process.exit(1);
}
console.log(`detailed-source-index: ${DETAILED_SOURCES.length} sources present.`);
