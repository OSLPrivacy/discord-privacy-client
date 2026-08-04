// Shared extractor for the Rust command registry.
//
// Lives here because BOTH ledger 3 (acl) and ledger 4 (commands) need it, and
// ledger 4 already imports from ledger 3 -- importing back would be an ESM
// cycle that deadlocks on a top-level await. One parser, not two: D-137 was
// caused by two ledgers disagreeing about the same set of commands.
import { read, blankComments, lineOf, lineIndex } from "./io.mjs";

export function rustRegistry(root) {
  const registry = new Map();
  const problems = [];
  const surfaceRel = "apps/osl-hub/src/hub_command_surface.rs";
  const mainRel = "apps/osl-hub/src/main.rs";
  const surface = blankComments(read(root, surfaceRel));
  const main = blankComments(read(root, mainRel));

  const macroStart = surface.indexOf("macro_rules! hub_tauri_commands");
  const macroEnd = surface.indexOf("macro_rules! hub_tauri_command_names", macroStart);
  if (macroStart < 0 || macroEnd <= macroStart) {
    problems.push({ id: "missing-input:hub_tauri_commands", detail: "hub_tauri_commands registry macro anchor is missing or moved", sites: [`${surfaceRel}:1`] });
  } else {
    const body = surface.slice(macroStart, macroEnd);
    for (const [name, sites] of collectIdentifiers(body, surfaceRel, surface, macroStart)) {
      for (const site of sites) add(registry, name, site);
    }
  }

  const marker = "invoke_handler(tauri::generate_handler![";
  let literalLists = 0;
  for (let cursor = main.indexOf(marker); cursor >= 0; cursor = main.indexOf(marker, cursor + 1)) {
    const listStart = cursor + marker.length;
    const end = main.indexOf("]);", listStart);
    if (end < 0) continue;
    literalLists += 1;
    const body = main.slice(listStart, end);
    for (const [name, sites] of collectIdentifiers(body, mainRel, main, listStart)) {
      for (const site of sites) add(registry, name, site);
    }
  }
  if (literalLists === 0) {
    problems.push({ id: "missing-input:literal-generate-handler", detail: "no literal tauri::generate_handler list was found in main.rs; signal/alternate registries would be invisible", sites: [`${mainRel}:1`] });
  }
  if (registry.size === 0) {
    problems.push({ id: "empty-input:command-registry", detail: "Rust command registry extraction returned zero commands", sites: [`${surfaceRel}:1`, `${mainRel}:1`] });
  }
  return { registry, problems };
}

function collectIdentifiers(body, rel, fullSource, offset = 0) {
  const starts = lineIndex(fullSource);
  const out = new Map();
  for (const m of body.matchAll(/\b([a-z][a-z0-9_]+)\b/g)) {
    const name = m[1];
    if (["macro_rules", "callback", "tauri", "generate_handler", "cfg", "feature"].includes(name)) continue;
    add(out, name, `${rel}:${lineOf(starts, offset + m.index)}`);
  }
  return out;
}

function add(map, id, site) {
  if (!map.has(id)) map.set(id, []);
  map.get(id).push(site);
}
