import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(process.env.TASK5048_HTML ?? new URL("./index.html", import.meta.url), "utf8");
const app = readFileSync(process.env.TASK5048_APP ?? new URL("./app.js", import.meta.url), "utf8");
const settings = html.slice(html.indexOf('data-page="settings"'));
const retiredRoutes = ["inbox", "people", "privacy", "activity", "connections"];

test("TASK 5048c removes the five sidebar routes", () => {
  for (const route of retiredRoutes) {
    assert.doesNotMatch(html, new RegExp(`data-page="${route}"`), `${route} page still exists`);
    assert.doesNotMatch(html, new RegExp(`data-page-target="${route}"`), `${route} sidebar route still exists`);
    assert.doesNotMatch(app, new RegExp(`^\\s*${route}: \\[`, "m"), `${route} page title route still exists`);
  }
  assert.match(html, /data-page="settings"/);
  assert.match(html, /Retired sidebar record/);
});

test("TASK 5048c rehomes every static inventory statement in Settings", () => {
  const statements = [
    "Private communication", "Inbox", "New OSL chat", "Rose", "OSL Chat · End-to-end encrypted", "Verified", "did the prototype work?", "testing it now", "Simulated conversation · no messages were loaded", "Open Secure Composer",
    "Identity and trust", "People", "Add person", "Verified OSL identity", "Discord · @rose.test", "Instagram · @rose.private", "Manage", "OSL test", "Two identities need review", "Discord · @osl.test", "X · unverified", "Verify", "Trust is specific", "A trusted person may receive OSL E2EE without bypassing file warnings, cleanup scope or account separation. OSL never merges identities from similar names or avatars.",
    "Simple by default", "Privacy", "Balanced", "Protection preset", "Basic", "Health, links and local warnings", "Recommended protection without automatic deletion", "Maximum", "Strict public-post checks and verified-contact rules", "Local scan", "Sensitive-history scan", "Included in Free", "Look for user-selected sensitive details in the selected test account. Findings stay on this device.", "Scope", "Run simulated scan", "After I send", "Retention", "Pro cleanup", "Automatic deletion", "Off by default on every connected account", "OSL text expiry", "Simulated in this prototype", "Review", "Always-On Retention Agent", "Customer-controlled PC, NAS or cloud VM", "Preview Pro", "3 items may reveal your home address or private routines", "Review calmly. Nothing was deleted or changed.", "Possible address", "A simulated old message may reveal a home address.", "Travel detail", "A simulated conversation may disclose when a home is empty.", "Account recovery clue", "A simulated message may contain a memorable recovery answer.", "Open message", "Show deletion steps", "Pro", "Ignore", "Simulated message location opened. No account was accessed.", "Guided deletion steps previewed. You remain in control of every platform action.", "Finding ignored locally. No platform data changed.", "Local simulated scan complete: 3 review items, 0 removals.",
    "Unlimited accounts in Free", "Connections", "Add test account", "Every account has isolated drafts, findings, policies and health. Expand a service to switch accounts.",
    "Proof, not promises", "Activity", "Export local copy", "Link tracking parameters removed", "Discord · Personal · Today 8:42 PM", "Local", "Instagram layout recipe changed", "Work · Composer moved to sidecar and recovered · Today 7:16 PM", "Recovered", "Cleanup request not started", "Facebook Messenger · Personal · User action required", "Held", "Removal guarantees stay separate", "Platform removal, OSL cryptographic expiry and local cache removal are shown as different results. Recipient captures are never reported as deleted.",
  ];
  for (const statement of statements) assert.ok(settings.includes(statement), `not rehomed: ${statement}`);
  assert.match(app, /function renderRetiredConnectionsRecord/);
  for (const statement of ["test accounts · Native + OSL modes", "Verified OSL recipient", "Native fallback", "Switch to ${escapeHtml(service.name)} ${escapeHtml(account.label)}"]) {
    assert.ok(app.includes(statement), `dynamic connection statement missing: ${statement}`);
  }
  console.log(`TASK5048C_ROUTES_REMOVED=${retiredRoutes.length}`);
  console.log(`TASK5048C_STATIC_STATEMENTS_REHOMED=${statements.length}`);
  console.log("TASK5048C_DYNAMIC_CONNECTION_STATEMENTS_REHOMED=4");
});
