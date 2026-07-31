#!/usr/bin/env node

// Claim-channel gate for the public crawl surface.
//
// The manifest declares where public claims can appear. This script fails if a
// declared channel has no concrete public-surface evidence or if crawl entries
// point outside existing textual files.

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const PRICING_PATH = path.join(REPO_ROOT, 'data', 'pricing.json');
const SELF_TEST = process.argv.includes('--self-test');
const MIN_PUBLIC_CLAIM_CHANNELS = 9;
const REQUIRED_CHANNELS = [
  'rendered-text',
  'document-title',
  'metadata-content',
  'accessible-name',
  'alt-title-placeholder-value',
  'public-data-attribute',
  'json-ld',
  'inline-script-copy',
  'textual-asset',
];
const H7_DIMENSION_IDS = [
  'scope',
  'price',
  'ongoing_monitoring',
  'removals',
  'data_handling',
  'completion_reporting',
  'limitations',
];
const H7_SOURCE_TYPES = new Set([
  'pricing',
  'plan_coverage',
  'monitoring_reporting',
  'removal_process',
  'privacy_policy',
  'terms',
  'limitations',
]);
const H7_REQUIRED_SOURCE_TYPES = [
  'pricing',
  'plan_coverage',
  'monitoring_reporting',
  'privacy_policy',
  'terms',
];
const H7_ALLOWED_HOSTS = new Set([
  'joindeleteme.com',
  'help.joindeleteme.com',
  'privacy.joindeleteme.com',
  'abine.com',
]);
const H7_CONFIDENCE = new Set(['high', 'medium', 'low']);
const H7_COMPARABILITY = new Set(['not_equivalent', 'narrowly_comparable']);
const H7_MAX_SOURCE_AGE_DAYS = 90;

function cliValue(name) {
  const prefix = `${name}=`;
  return process.argv.find((arg) => arg.startsWith(prefix))?.slice(prefix.length);
}

function isNonempty(value) {
  return typeof value === 'string' && value.trim().length > 0;
}

function list(value) {
  return Array.isArray(value) ? value : [];
}

function unique(values) {
  return new Set(values).size === values.length;
}

function readPricing() {
  return JSON.parse(readFileSync(PRICING_PATH, 'utf8'));
}

function h7AsOf(pricing) {
  return cliValue('--as-of') || pricing.research_comparisons?.h7_deleteme?.reviewed_on || new Date().toISOString().slice(0, 10);
}

function isoDate(value) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value ?? '')) return null;
  const parsed = new Date(`${value}T00:00:00Z`);
  if (Number.isNaN(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== value) return null;
  return parsed;
}

function hasAffirmative(text, pattern) {
  for (const match of text.matchAll(new RegExp(pattern.source, `${pattern.flags.includes('i') ? 'i' : ''}g`))) {
    const before = text.slice(Math.max(0, match.index - 105), match.index);
    const after = text.slice(match.index + match[0].length, match.index + match[0].length + 40);
    const negatedBefore = /\b(?:cannot|can't|decline(?:s|d)?|no|not|does not|doesn't|is not|isn't|never|without|unknown)\b[^.!?;]{0,105}$/i.test(before);
    const unknownAfter = /^[^.!?;]{0,35}\b(?:unknown|not stated|not specified|not established)\b/i.test(after);
    if (!negatedBefore && !unknownAfter) return true;
  }
  return false;
}

function h7Prose(comparison) {
  const parts = [];
  for (const dimension of list(comparison?.dimensions)) {
    parts.push(
      dimension?.conflict_explanation,
      dimension?.osl_limitation,
      dimension?.bounded_conclusion,
    );
    for (const fact of list(dimension?.deleteme_facts)) {
      parts.push(fact?.paraphrase, fact?.qualifiers);
    }
    for (const unknown of list(dimension?.unknowns)) {
      parts.push(unknown?.reason);
    }
  }
  return parts.filter(isNonempty).join(' ');
}

function validateH7Comparison(pricing, asOf) {
  const errors = [];
  const add = (code, message) => errors.push(`H7_${code}: ${message}`);
  const asOfDate = isoDate(asOf);
  if (!asOfDate) add('AS_OF', `--as-of must be a real ISO date, got ${JSON.stringify(asOf)}`);

  if (pricing.manifest_version !== 7) add('VERSION', 'manifest_version must be exactly 7');
  const research = pricing.research_comparisons;
  if (!research || typeof research !== 'object' || Array.isArray(research)) {
    add('SCHEMA', 'research_comparisons must be an object');
  }
  if (research?.schema_version !== 1) add('VERSION', 'research_comparisons.schema_version must be exactly 1');

  const comparison = research?.h7_deleteme;
  if (!comparison || typeof comparison !== 'object' || Array.isArray(comparison)) {
    add('SCHEMA', 'research_comparisons.h7_deleteme must be an object');
  }
  if (comparison?.comparison_version !== 1) add('VERSION', 'h7_deleteme.comparison_version must be exactly 1');
  if (comparison?.id !== 'h7-deleteme-us-consumer') add('IDENTITY', 'comparison id must be h7-deleteme-us-consumer');

  const market = comparison?.market_scope;
  if (market?.country !== 'US'
    || market?.audience !== 'consumer'
    || JSON.stringify(market?.plans) !== JSON.stringify(['Standard', 'Premium'])
    || JSON.stringify(market?.excluded) !== JSON.stringify(['business', 'international'])) {
    add('MARKET_SCOPE', 'market scope must remain US consumer Standard/Premium, excluding business and international');
  }

  const sources = list(comparison?.sources);
  if (sources.length < 8) add('SOURCE_FLOOR', `at least 8 official sources are required; found ${sources.length}`);
  const sourceIds = sources.map((source) => source?.id).filter(isNonempty);
  if (sourceIds.length !== sources.length || !unique(sourceIds)) add('SOURCE_ID', 'every source needs a unique nonempty id');
  const sourceById = new Map(sources.map((source) => [source?.id, source]));
  const seenSourceTypes = new Set();
  const sourceAccessDates = [];
  let newestAccessTime = Number.NEGATIVE_INFINITY;

  for (const [index, source] of sources.entries()) {
    const label = isNonempty(source?.id) ? source.id : `source[${index}]`;
    for (const field of ['publisher', 'source_type', 'url', 'title', 'section', 'published_or_effective_on', 'accessed_on']) {
      if (!isNonempty(source?.[field])) add('SOURCE_METADATA', `${label}.${field} must be nonempty`);
    }
    if (source?.publisher !== 'DeleteMe / Abine') add('SOURCE_PUBLISHER', `${label} must name DeleteMe / Abine as publisher`);
    const refreshEvidence = source?.refresh_evidence;
    if (!refreshEvidence || typeof refreshEvidence !== 'object' || Array.isArray(refreshEvidence)) {
      add('SOURCE_REFRESH', `${label}.refresh_evidence must document maintained source-refresh evidence`);
    } else {
      for (const field of ['checked_on', 'method', 'result']) {
        if (!isNonempty(refreshEvidence?.[field])) add('SOURCE_REFRESH', `${label}.refresh_evidence.${field} must be nonempty`);
      }
      if (refreshEvidence?.checked_on !== source?.accessed_on) add('SOURCE_REFRESH', `${label}.refresh_evidence.checked_on must match accessed_on`);
      if (refreshEvidence?.method !== 'manual_official_source_review') add('SOURCE_REFRESH', `${label}.refresh_evidence.method must be manual_official_source_review`);
      if (!/\bofficial DeleteMe\/Abine source\b/i.test(refreshEvidence?.result ?? '')
        || !/\bH7 comparison\b/i.test(refreshEvidence?.result ?? '')) {
        add('SOURCE_REFRESH', `${label}.refresh_evidence.result must say the official DeleteMe/Abine source was refreshed for the H7 comparison`);
      }
      if (!new RegExp(`\\bon ${source?.accessed_on}\\b`).test(refreshEvidence?.result ?? '')
        || !/\b(live URL\/title|indexed-content) checks\b/i.test(refreshEvidence?.result ?? '')) {
        add('SOURCE_REFRESH', `${label}.refresh_evidence.result must retain the access date plus live URL/title or indexed-content check detail`);
      }
    }
    if (!H7_SOURCE_TYPES.has(source?.source_type)) add('SOURCE_TYPE', `${label} has unsupported source_type ${JSON.stringify(source?.source_type)}`);
    else seenSourceTypes.add(source.source_type);
    try {
      const url = new URL(source?.url);
      if (url.protocol !== 'https:' || !H7_ALLOWED_HOSTS.has(url.hostname)) add('SOURCE_DOMAIN', `${label} must use HTTPS on an official DeleteMe/Abine host`);
    } catch {
      add('SOURCE_DOMAIN', `${label} has an invalid URL`);
    }
    const accessed = isoDate(source?.accessed_on);
    if (!accessed) {
      add('SOURCE_DATE', `${label}.accessed_on must be a real ISO date`);
    } else if (asOfDate) {
      sourceAccessDates.push(source.accessed_on);
      newestAccessTime = Math.max(newestAccessTime, accessed.getTime());
      const ageDays = Math.floor((asOfDate - accessed) / 86_400_000);
      if (ageDays < 0) add('SOURCE_FUTURE', `${label} was accessed after --as-of=${asOf}`);
      else if (ageDays > H7_MAX_SOURCE_AGE_DAYS) add('SOURCE_STALE', `${label} is ${ageDays} days old at --as-of=${asOf}; maximum is ${H7_MAX_SOURCE_AGE_DAYS}`);
    }
  }
  for (const sourceType of H7_REQUIRED_SOURCE_TYPES) {
    if (!seenSourceTypes.has(sourceType)) add('SOURCE_CLASS', `required source class ${sourceType} is absent`);
  }

  const reviewed = isoDate(comparison?.reviewed_on);
  if (!reviewed) {
    add('REVIEW_DATE', 'reviewed_on must be a real ISO date');
  } else {
    if (asOfDate && reviewed > asOfDate) add('REVIEW_DATE', `reviewed_on cannot be after --as-of=${asOf}`);
    if (Number.isFinite(newestAccessTime) && reviewed.getTime() < newestAccessTime) add('REVIEW_DATE', 'reviewed_on cannot precede the newest source access date');
  }
  if (reviewed && sourceAccessDates.length === sources.length) {
    const staleRefreshSources = sources.filter((source) => source.accessed_on !== comparison.reviewed_on).map((source) => source.id);
    if (staleRefreshSources.length > 0) add('SOURCE_REFRESH', `source access dates must match reviewed_on; stale refresh evidence: ${staleRefreshSources.join(', ')}`);
  }

  const dimensions = list(comparison?.dimensions);
  const dimensionIds = dimensions.map((dimension) => dimension?.id).filter(isNonempty);
  if (dimensions.length !== H7_DIMENSION_IDS.length || !H7_DIMENSION_IDS.every((id) => dimensionIds.includes(id))) {
    add('DIMENSION_SET', `dimensions must be exactly: ${H7_DIMENSION_IDS.join(', ')}`);
  }
  if (dimensionIds.length !== dimensions.length || !unique(dimensionIds)) add('DIMENSION_ID', 'every dimension needs a unique nonempty id');
  const capabilityById = new Map(list(pricing.capability_registry).map((entry) => [entry?.id, entry]));
  for (const capabilityId of ['scrub-discovery', 'scrub-guided-deletion', 'autoscrub']) {
    const capability = capabilityById.get(capabilityId);
    if (!capability || capability.status !== 'Planned') add('OSL_STATUS', `${capabilityId} must resolve to exact status Planned`);
    if (!capability || capability.sellable !== false) add('OSL_SELLABILITY', `${capabilityId} must remain explicitly sellable:false`);
  }

  const factIds = [];
  let unknownCount = 0;
  let conflictCount = 0;
  let limitationCount = 0;
  for (const [index, dimension] of dimensions.entries()) {
    const label = isNonempty(dimension?.id) ? dimension.id : `dimension[${index}]`;
    if (!['established', 'conflicted'].includes(dimension?.status)) add('DIMENSION_STATUS', `${label}.status must be established or conflicted`);
    const facts = list(dimension?.deleteme_facts);
    if (facts.length === 0) add('FACT_FLOOR', `${label} needs at least one established, source-bound fact`);
    for (const [factIndex, fact] of facts.entries()) {
      const factLabel = isNonempty(fact?.id) ? fact.id : `${label}.fact[${factIndex}]`;
      if (!isNonempty(fact?.id)) add('FACT_ID', `${factLabel} needs a nonempty id`);
      else factIds.push(fact.id);
      for (const field of ['paraphrase', 'qualifiers']) {
        if (!isNonempty(fact?.[field])) add('FACT_METADATA', `${factLabel}.${field} must be nonempty`);
      }
      if (!H7_CONFIDENCE.has(fact?.confidence)) add('FACT_CONFIDENCE', `${factLabel}.confidence must be high, medium, or low`);
      const factSources = list(fact?.source_ids);
      if (factSources.length === 0) add('FACT_SOURCE', `${factLabel} must cite at least one source`);
      for (const sourceId of factSources) {
        if (!sourceById.has(sourceId)) add('DANGLING_SOURCE', `${factLabel} cites missing source ${JSON.stringify(sourceId)}`);
      }
    }
    const unknowns = list(dimension?.unknowns);
    unknownCount += unknowns.length;
    for (const [unknownIndex, unknown] of unknowns.entries()) {
      const unknownLabel = `${label}.unknown[${unknownIndex}]`;
      if (!isNonempty(unknown?.field) || !isNonempty(unknown?.reason)) add('UNKNOWN_METADATA', `${unknownLabel} needs field and reason`);
      const attempted = list(unknown?.attempted_source_ids);
      if (attempted.length === 0) add('UNKNOWN_SOURCES', `${unknownLabel} needs attempted_source_ids`);
      for (const sourceId of attempted) {
        if (!sourceById.has(sourceId)) add('DANGLING_SOURCE', `${unknownLabel} cites missing attempted source ${JSON.stringify(sourceId)}`);
      }
    }
    if (dimension?.status === 'conflicted') {
      conflictCount += 1;
      const cited = new Set(facts.flatMap((fact) => list(fact?.source_ids)));
      if (!isNonempty(dimension?.conflict_explanation) || cited.size < 2) add('CONFLICT', `${label} needs a nonempty conflict explanation and at least two sources`);
    }
    const refs = list(dimension?.osl_manifest_refs);
    if (refs.length === 0) add('OSL_REF', `${label} needs at least one OSL manifest reference`);
    for (const [refIndex, ref] of refs.entries()) {
      const refLabel = `${label}.osl_manifest_refs[${refIndex}]`;
      const match = /^\/capability_registry\/([a-z0-9-]+)$/.exec(ref?.json_pointer ?? '');
      if (!match) {
        add('OSL_REF', `${refLabel} must use /capability_registry/<stable-id>, never a numeric array index`);
        continue;
      }
      const capability = capabilityById.get(match[1]);
      if (!capability) add('OSL_REF', `${refLabel} points to missing capability ${match[1]}`);
      else if (ref?.expected_status !== capability.status) add('OSL_STATUS', `${refLabel} expected ${JSON.stringify(ref?.expected_status)} but manifest says ${capability.status}`);
    }
    if (!isNonempty(dimension?.osl_limitation)) add('OSL_LIMITATION', `${label}.osl_limitation must be nonempty`);
    else limitationCount += 1;
    if (!H7_COMPARABILITY.has(dimension?.comparability)) add('COMPARABILITY', `${label}.comparability must avoid equivalence claims`);
    if (!isNonempty(dimension?.bounded_conclusion)) add('CONCLUSION', `${label}.bounded_conclusion must be nonempty`);
  }
  if (!unique(factIds)) add('FACT_ID', 'fact ids must be unique across the comparison');
  if (unknownCount === 0) add('UNKNOWN_FLOOR', 'at least one material unknown must be retained');
  if (conflictCount === 0) add('CONFLICT', 'at least one source conflict must be retained');
  if (limitationCount !== H7_DIMENSION_IDS.length) add('OSL_LIMITATION', 'every dimension must retain an OSL limitation');

  const allH7Text = h7Prose(comparison);
  if (hasAffirmative(allH7Text, /\b(?:OSL is better|OSL is a replacement for DeleteMe|cheaper than DeleteMe|equivalent to DeleteMe)\b/i)) {
    add('WINNER_LANGUAGE', 'comparison must not promote OSL as the winner, replacement, cheaper service, or equivalent');
  }
  if (hasAffirmative(allH7Text, /\bStandard-US plan\b[^.!?]{0,80}\b986\b|\b986\b[^.!?]{0,80}\bStandard-US plan\b/i)) {
    add('SCOPE_SEMANTICS', '986 must remain a broader catalog figure, not the Standard-US included-site count');
  }
  if (hasAffirmative(allH7Text, /\b(?:continuous|24\/7|around-the-clock)\b[^.!?]{0,80}\b(?:monitor|scan)|\b(?:monitor|scan)[^.!?]{0,80}\b(?:continuous|24\/7|around-the-clock)\b/i)) {
    add('MONITORING_CLAIM', 'monitoring claims must not imply literal continuous per-broker scanning');
  }
  if (hasAffirmative(allH7Text, /\bfirst privacy report\b[^.!?]{0,100}\b(?:proves|is proof|confirms|completed everything)\b/i)) {
    add('COMPLETION_PROOF', 'first privacy report must not be promoted to removal-completion proof');
  }
  if (hasAffirmative(allH7Text, /\bguarantee(?:s|d)?\b[^.!?]{0,120}\b(?:third party|every|all|remove|removal)\b/i)) {
    add('REMOVAL_GUARANTEE', 'removal claims must not guarantee third-party action');
  }
  const removals = dimensions.find((dimension) => dimension?.id === 'removals');
  if (!list(removals?.deleteme_facts).some((fact) => fact?.id === 'member-confirmation-required')) {
    add('MEMBER_CONFIRMATION', 'removals must retain the member-confirmation limitation');
  }
  return errors;
}

function runH7SelfTest(pricing, asOf) {
  const baselineErrors = validateH7Comparison(pricing, asOf);
  let failures = 0;
  const comparison = pricing.research_comparisons?.h7_deleteme;
  const mutated = structuredClone(pricing);
  if (mutated.research_comparisons?.h7_deleteme?.sources?.[0]?.refresh_evidence) {
    mutated.research_comparisons.h7_deleteme.sources[0].refresh_evidence.result = 'The source was reviewed.';
  }
  const refreshMutationCaught = validateH7Comparison(mutated, asOf)
    .some((error) => error.startsWith('H7_SOURCE_REFRESH:'));
  const maintainedSourcesClean = baselineErrors.length === 0
    && comparison?.reviewed_on === asOf
    && comparison?.sources?.length >= 8
    && comparison.sources.every((source) => source.accessed_on === comparison.reviewed_on
      && source.refresh_evidence?.checked_on === source.accessed_on
      && source.refresh_evidence?.method === 'manual_official_source_review'
      && /\bofficial DeleteMe\/Abine source\b/i.test(source.refresh_evidence?.result ?? '')
      && /\bH7 comparison\b/i.test(source.refresh_evidence?.result ?? '')
      && new RegExp(`\\bon ${source.accessed_on}\\b`).test(source.refresh_evidence?.result ?? '')
      && /\b(live URL\/title|indexed-content) checks\b/i.test(source.refresh_evidence?.result ?? ''));

  const namedTestPass = maintainedSourcesClean && refreshMutationCaught;
  console.log(`  ${namedTestPass ? 'passed ' : 'FAILED '} Promote the clean H7 DeleteMe candidate with maintained source-refresh evidence`);
  if (!namedTestPass) {
    failures += 1;
    for (const error of baselineErrors) console.log(`    ${error}`);
    if (!refreshMutationCaught) console.log('    H7_SOURCE_REFRESH mutation was not caught');
  }

  const mutations = [
    ['missing maintained source-refresh evidence', 'H7_SOURCE_REFRESH', (fixture) => { delete fixture.research_comparisons.h7_deleteme.sources[0].refresh_evidence; }],
    ['third-party source domain', 'H7_SOURCE_DOMAIN', (fixture) => { fixture.research_comparisons.h7_deleteme.sources[0].url = 'https://example.com/deleteme-review'; }],
    ['empty source list', 'H7_SOURCE_FLOOR', (fixture) => { fixture.research_comparisons.h7_deleteme.sources = []; }],
    ['missing required dimension', 'H7_DIMENSION_SET', (fixture) => { fixture.research_comparisons.h7_deleteme.dimensions.pop(); }],
    ['autoscrub promoted above Planned', 'H7_OSL_STATUS', (fixture) => { fixture.capability_registry.find((capability) => capability.id === 'autoscrub').status = 'Beta'; }],
  ];
  for (const [name, code, mutate] of mutations) {
    const fixture = structuredClone(pricing);
    mutate(fixture);
    const caught = validateH7Comparison(fixture, asOf).some((error) => error.startsWith(`${code}:`));
    console.log(`  ${caught ? 'caught ' : 'MISSED '} ${name}`);
    if (!caught) failures += 1;
  }
  if (failures > 0) throw new Error(`check-claims self-test: ${failures} H7 fixtures failed`);
}

function readManifest() {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
  if (manifest.schema_version !== 1 || manifest.manifest_id !== 'osl-public-surface') {
    throw new Error('public surface manifest must use schema_version 1 and manifest_id osl-public-surface');
  }
  if (!Array.isArray(manifest.claim_channels) || manifest.claim_channels.length < MIN_PUBLIC_CLAIM_CHANNELS) {
    throw new Error(`public surface manifest must declare at least ${MIN_PUBLIC_CLAIM_CHANNELS} claim channels`);
  }
  for (const channel of REQUIRED_CHANNELS) {
    if (!manifest.claim_channels.includes(channel)) throw new Error(`public surface manifest is missing ${channel}`);
  }
  if (!Array.isArray(manifest.html) || manifest.html.length === 0) throw new Error('public surface manifest must declare HTML pages');
  if (!Array.isArray(manifest.assets) || manifest.assets.length === 0) throw new Error('public surface manifest must declare assets');
  return manifest;
}

function readEntry(entry) {
  if (typeof entry !== 'string' || path.isAbsolute(entry) || entry.includes('..')) {
    throw new Error(`manifest entry is not a safe repo-relative path: ${String(entry)}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`manifest entry is missing on disk: ${entry}`);
  return { entry, fullPath, text: readFileSync(fullPath, 'utf8') };
}

function stripHtmlNoise(text) {
  return text
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function extractJsonLd(file) {
  const blocks = [];
  const regex = /<script\b[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi;
  for (const match of file.text.matchAll(regex)) {
    const raw = match[1].trim();
    const parsed = JSON.parse(raw);
    blocks.push({ entry: file.entry, name: parsed.name || parsed['@type'] || 'json-ld' });
  }
  return blocks;
}

function evidenceForChannel(channel, files) {
  if (channel === 'rendered-text') {
    return files.html
      .map((file) => ({ entry: file.entry, textLength: stripHtmlNoise(file.text).length }))
      .filter((item) => item.textLength >= 20);
  }
  if (channel === 'document-title') {
    return files.html
      .map((file) => ({ entry: file.entry, title: file.text.match(/<title>([\s\S]*?)<\/title>/i)?.[1]?.trim() || '' }))
      .filter((item) => item.title.length > 0);
  }
  if (channel === 'metadata-content') {
    return files.html.flatMap((file) => [...file.text.matchAll(/<meta\b[^>]*\bcontent=["']([^"']+)["'][^>]*>/gi)]
      .map((match) => ({ entry: file.entry, content: match[1] })));
  }
  if (channel === 'accessible-name') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\baria-(?:label|labelledby)=["']([^"']+)["']|<label\b[^>]*>([\s\S]*?)<\/label>/gi)]
      .map((match) => ({ entry: file.entry, value: (match[1] || stripHtmlNoise(match[2] || '')).slice(0, 80) })));
  }
  if (channel === 'alt-title-placeholder-value') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\b(?:alt|title|placeholder|value)=["']([^"']*)["']/gi)]
      .filter((match) => match[1].trim().length > 0)
      .map((match) => ({ entry: file.entry, value: match[1].trim().slice(0, 80) })));
  }
  if (channel === 'public-data-attribute') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\bdata-[a-zA-Z0-9_-]+=/g)]
      .map((match) => ({ entry: file.entry, attribute: match[0].slice(0, -1) })));
  }
  if (channel === 'json-ld') {
    return files.html.flatMap(extractJsonLd);
  }
  if (channel === 'inline-script-copy') {
    return files.assets
      .filter((file) => file.entry.endsWith('.js') && /\.(?:textContent|innerText|innerHTML)\b|`[^`]{20,}`|["'][^"']{20,}["']/.test(file.text))
      .map((file) => ({ entry: file.entry, bytes: Buffer.byteLength(file.text, 'utf8') }));
  }
  if (channel === 'textual-asset') {
    const allowed = new Set(files.manifest.textual_asset_extensions || []);
    return files.assets
      .filter((file) => allowed.has(path.extname(file.entry).toLowerCase()) && file.text.trim().length > 0)
      .map((file) => ({ entry: file.entry, bytes: Buffer.byteLength(file.text, 'utf8') }));
  }
  throw new Error(`unknown public claim channel: ${channel}`);
}

function run() {
  const manifest = readManifest();
  const html = manifest.html.map(readEntry);
  const assets = manifest.assets.map(readEntry);
  const files = { manifest, html, assets, all: [...html, ...assets] };
  const channelEvidence = {};
  const failures = [];
  for (const channel of manifest.claim_channels) {
    const evidence = evidenceForChannel(channel, files);
    channelEvidence[channel] = evidence;
    if (evidence.length === 0) failures.push(channel);
  }

  console.log('\ncheck-claims summary');
  console.log(`  public HTML pages : ${html.length}`);
  console.log(`  public assets     : ${assets.length}`);
  console.log(`  claim channels    : ${manifest.claim_channels.length}`);
  for (const channel of manifest.claim_channels) {
    console.log(`  ${channel.padEnd(28)} ${channelEvidence[channel].length}`);
  }
  if (failures.length > 0) {
    throw new Error(`claim channels have no public-surface evidence: ${failures.join(', ')}`);
  }

  const pricing = readPricing();
  const asOf = h7AsOf(pricing);
  const h7Failures = validateH7Comparison(pricing, asOf);
  console.log(`  h7-deleteme-source-refresh ${h7Failures.length === 0 ? 'pass' : 'FAIL'}`);
  if (h7Failures.length > 0) {
    throw new Error(`H7 DeleteMe comparison failed:\n${h7Failures.join('\n')}`);
  }
  if (SELF_TEST) {
    console.log(`\ncheck-claims self-test (H7 DeleteMe manifest, as of ${asOf}):`);
    runH7SelfTest(pricing, asOf);
  }
  console.log('\ncheck-claims: complete.');
}

try {
  run();
} catch (error) {
  console.error(`check-claims: fatal error: ${error.stack || error.message}`);
  process.exit(1);
}
