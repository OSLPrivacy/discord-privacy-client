import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const TAURI_CONFIG = path.join(REPO_ROOT, 'apps', 'osl-hub', 'tauri.conf.json');

/**
 * Return the CSP shipped by the Hub Tauri manifest.  The render gate must not
 * carry a second, drifting copy of this policy.
 */
export function shippedHubCsp(manifestPath = TAURI_CONFIG) {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const csp = manifest?.app?.security?.csp;
  if (typeof csp !== 'string' || csp.trim() === '') {
    throw new Error(`missing app.security.csp in ${manifestPath}`);
  }
  return csp;
}

export function shippedHubCspHeaders(manifestPath) {
  return { 'content-security-policy': shippedHubCsp(manifestPath) };
}
