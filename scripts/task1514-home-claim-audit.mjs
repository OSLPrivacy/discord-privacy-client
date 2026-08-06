#!/usr/bin/env node

import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const htmlPath = path.join(root, "docs/prototypes/osl-hub/index.html");
const appJsPath = path.join(root, "docs/prototypes/osl-hub/app.js");
const servicesPath = path.join(root, "apps/osl-hub-ui/src/services.ts");
const pricingPath = path.join(root, "data/pricing.json");

const html = readFileSync(htmlPath, "utf8");
const appJs = readFileSync(appJsPath, "utf8");
const services = readFileSync(servicesPath, "utf8");
const pricing = JSON.parse(readFileSync(pricingPath, "utf8"));

function requireContains(haystack, needle, label) {
  if (!haystack.includes(needle)) {
    throw new Error(`${label} missing exact string: ${needle}`);
  }
  return needle;
}

function capability(id) {
  const row = pricing.capability_registry.find((candidate) => candidate.id === id);
  if (!row) throw new Error(`missing pricing capability ${id}`);
  return row;
}

function textOnly(markup) {
  return markup
    .replace(/<script[\s\S]*?<\/script>/gu, " ")
    .replace(/<style[\s\S]*?<\/style>/gu, " ")
    .replace(/<[^>]*>/gu, " ")
    .replace(/\s+/gu, " ")
    .trim();
}

const homeStart = html.indexOf('<section class="page active" data-page="home"');
const homeEnd = html.indexOf('<section class="page" data-page="inbox"', homeStart);
if (homeStart < 0 || homeEnd <= homeStart) throw new Error("could not isolate prototype Home section");
const homeMarkup = html.slice(homeStart, homeEnd);
const homeText = textOnly(homeMarkup);

const prototypeServices = [...appJs.matchAll(/id: "([a-z]+)",\n\s+name: "([^"]+)"/gu)].map((match) => match[2]);
const launchApps = [...services.matchAll(/homeApp\("([^"]+)", "([^"]+)"/gu)].map((match) => match[2]);
const currentAvailableLaunchApps = [...services.matchAll(/homeApp\("([^"]+)", "([^"]+)"/gu)]
  .filter((match) => !services.slice(match.index, services.indexOf("\n", match.index)).includes('"comingSoon"'))
  .map((match) => match[2]);

const nativeEvidence = [...services.matchAll(/\{ id: "([^"]+)", displayName: "([^"]+)", availability: "([^"]+)", supportStatus: "([^"]+)", carrierEvidence: "([^"]+)", deliveryEvidence: "([^"]+)", claimBlockers: \[([^\]]*)\], claimNote: "([^"]+)"/gu)]
  .map((match) => ({
    id: match[1],
    name: match[2],
    availability: match[3],
    supportStatus: match[4],
    carrierEvidence: match[5],
    deliveryEvidence: match[6],
    blockers: match[7].replaceAll('"', "").trim() || "none",
    note: match[8],
  }));

const requiredScrubDiscovery = capability("scrub-discovery").public_note;
const requiredScrubDeletion = capability("scrub-guided-deletion").public_note;
const requiredPro = pricing.tiers.pro.early_access_line;
const requiredAutoScrub = pricing.open_source.exception_sentence;

const currentScrubSentence = requireContains(
  html,
  "Look for user-selected sensitive details in the selected test account. Findings stay on this device.",
  "current Scrub prototype",
);
const currentScrubHomeSentence = requireContains(
  homeText,
  "A local preview may contain an old address. Nothing has been removed.",
  "current Home Scrub panel",
);
const currentProSentence = requireContains(
  html,
  "The Free scan and summary stay available. Pro organizes exact links, deletion steps and receipts without running hidden account automation.",
  "current Pro prototype dialog",
);

const evidenceProtectedText = capability("protected-text");
const evidenceScrubDiscovery = capability("scrub-discovery");
const evidenceScrubGuidedDeletion = capability("scrub-guided-deletion");
const evidenceLinkProtection = capability("link-protection");
const evidenceWebsiteScrub = capability("website-scrub");

const claims = [
  {
    text: requireContains(homeText, "Your text protection is ready", "Home"),
    support: "unsupported",
    evidence: `data/pricing.json capability_registry[protected-text].public_note=${JSON.stringify(evidenceProtectedText.public_note)}; native app catalog contains ${nativeEvidence.map((app) => `${app.name}:${app.supportStatus}/${app.carrierEvidence}/${app.deliveryEvidence}`).join(", ")}`,
  },
  {
    text: requireContains(homeText, "All nine launch companions keep their native features. Choose Native for ordinary service messaging or OSL Protected for a verified OSL recipient.", "Home"),
    support: "unsupported",
    evidence: `prototype app.js service_count=${prototypeServices.length} names=${prototypeServices.join(", ")}; current apps/osl-hub-ui/src/services.ts launch_app_count=${launchApps.length} names=${launchApps.join(", ")}; current_available_launch_apps=${currentAvailableLaunchApps.join(", ")}`,
  },
  {
    text: requireContains(homeText, "Review three sensitive-history findings", "Home"),
    support: "unsupported",
    evidence: `data/pricing.json capability_registry[scrub-discovery].status=${evidenceScrubDiscovery.status}; public_note=${JSON.stringify(evidenceScrubDiscovery.public_note)}`,
  },
  {
    text: currentScrubHomeSentence,
    support: "unsupported",
    evidence: `data/pricing.json capability_registry[scrub-guided-deletion].status=${evidenceScrubGuidedDeletion.status}; public_note=${JSON.stringify(evidenceScrubGuidedDeletion.public_note)}`,
  },
  {
    text: requireContains(homeText, "Checks 42 Links cleaned 11 Verified removals 0", "Home"),
    support: "unsupported",
    evidence: `data/pricing.json capability_registry[link-protection].status=${evidenceLinkProtection.status}; public_note=${JSON.stringify(evidenceLinkProtection.public_note)}; Home metric row is static markup in ${htmlPath}`,
  },
  {
    text: requireContains(homeText, "This prototype makes no network requests.", "Home"),
    support: "supported",
    evidence: `docs/prototypes/osl-hub/app.js contains fetch=${/\bfetch\s*\(/u.test(appJs)}, XMLHttpRequest=${/\bXMLHttpRequest\b/u.test(appJs)}, WebSocket=${/\bWebSocket\b/u.test(appJs)}`,
  },
  {
    text: requireContains(homeText, "Production OSL servers must never retain plaintext messages, social credentials, scan findings or cleanup receipts.", "Home"),
    support: "unsupported",
    evidence: `data/pricing.json capability_registry[website-scrub].status=${evidenceWebsiteScrub.status}; public_note=${JSON.stringify(evidenceWebsiteScrub.public_note)}; ledger plain sentence=${JSON.stringify(pricing.ledger_privacy.plain_sentence)}`,
  },
];

if (claims.length === 0) throw new Error("home claim count is zero");
if (claims.some((claim) => claim.support !== "supported" && claim.support !== "unsupported")) {
  throw new Error("at least one claim is unmarked");
}
if (new Set(claims.map((claim) => claim.text)).size !== claims.length) {
  throw new Error("duplicate claim text");
}

console.log(`home_claim_count=${claims.length}`);
for (const [index, claim] of claims.entries()) {
  console.log(`claim_${index + 1}_text=${JSON.stringify(claim.text)}`);
  console.log(`claim_${index + 1}_support=${claim.support}`);
  console.log(`claim_${index + 1}_evidence=${claim.evidence}`);
}
console.log(`unmarked_claims=${claims.filter((claim) => !claim.support).length}`);
console.log(`listed_claim_count=${claims.length}`);
console.log(`scrub_sentence_needing_change=${JSON.stringify(currentScrubSentence)}`);
console.log(`scrub_home_sentence_needing_change=${JSON.stringify(currentScrubHomeSentence)}`);
console.log(`scrub_required_discovery_sentence=${JSON.stringify(requiredScrubDiscovery)}`);
console.log(`scrub_required_guided_deletion_sentence=${JSON.stringify(requiredScrubDeletion)}`);
console.log(`pro_sentence_needing_change=${JSON.stringify(currentProSentence)}`);
console.log(`pro_required_sentence=${JSON.stringify(requiredPro)}`);
console.log(`autoscrub_required_sentence=${JSON.stringify(requiredAutoScrub)}`);
