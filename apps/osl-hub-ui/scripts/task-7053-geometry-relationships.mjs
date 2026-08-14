/**
 * TASK 7053 — geometry is a second, advisory comparison after the structural
 * control inventory.  It compares relations in the reviewed 1280x800 capture,
 * never individual pixel coordinates or dimensions.
 */
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { auditStructuralInventories } from "./task-7050-structural-inventory.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
export const PINNED_CANVAS = Object.freeze({ width: 1280, height: 800 });
const WIDTH_RATIO_TOLERANCE = 0.15;

function quoted(value) {
  return JSON.stringify(value);
}

function pageName(page) {
  return `design page ${quoted(page)}`;
}

function routeName(route) {
  return `build route ${quoted(route)}`;
}

function controlsFrom(screen) {
  return Array.isArray(screen?.controls) ? screen.controls : [];
}

function structuralInput(input) {
  return {
    manifest: input.manifest,
    routes: input.routes,
    designInventories: Object.fromEntries(Object.entries(input.designScreens ?? {}).map(([page, screen]) => [page, controlsFrom(screen)])),
    buildInventories: Object.fromEntries(Object.entries(input.buildScreens ?? {}).map(([route, screen]) => [route, controlsFrom(screen)])),
  };
}

function validCanvas(canvas) {
  return canvas && canvas.width === PINNED_CANVAS.width && canvas.height === PINNED_CANVAS.height;
}

function rectangle(control) {
  const raw = control?.bounds ?? control?.rect;
  if (!raw || typeof raw !== "object") return null;
  const left = raw.left ?? raw.x;
  const top = raw.top ?? raw.y;
  const width = raw.width;
  const height = raw.height;
  if (![left, top, width, height].every(Number.isFinite) || width <= 0 || height <= 0) return null;
  return { left, top, width, height };
}

function namedControls(screen, page, route, side) {
  const findings = [];
  if (!validCanvas(screen?.canvas)) {
    findings.push(`${pageName(page)}: ${routeName(route)} cannot compare GEOMETRY because the ${side} capture is not pinned to ${PINNED_CANVAS.width}x${PINNED_CANVAS.height}.`);
  }
  const controls = new Map();
  for (const control of controlsFrom(screen)) {
    if (typeof control?.name !== "string" || !control.name.trim()) continue;
    const rect = rectangle(control);
    if (!rect) {
      findings.push(`${pageName(page)}: ${routeName(route)} cannot compare GEOMETRY because ${side} control ${quoted(control.name)} has no usable capture bounds.`);
      continue;
    }
    controls.set(control.name.trim(), { label: control.label?.trim() || control.name.trim(), rect });
  }
  return { controls, findings };
}

// A shared row is an overlap relationship between vertical spans.  It is
// invariant under a whole-screen nudge, unlike comparing a top coordinate.
function sharesRow(left, right) {
  const overlap = Math.min(left.top + left.height, right.top + right.height) - Math.max(left.top, right.top);
  return overlap > 0;
}

function aboveFold(rect) {
  // Fold membership is a relationship to the fixed capture canvas, not a
  // screen-coordinate equality test.
  return rect.top < PINNED_CANVAS.height;
}

function widthRelationshipChanged(designLeft, designRight, buildLeft, buildRight) {
  const designRatio = designLeft.width / designRight.width;
  const buildRatio = buildLeft.width / buildRight.width;
  return Math.abs(designRatio - buildRatio) / designRatio > WIDTH_RATIO_TOLERANCE;
}

/**
 * Compare only named controls that structure has made matchable.  A changed
 * row relation is one finding; it intentionally does not also manufacture a
 * reading-order finding for the same pair.
 */
export function compareRouteGeometry({ page, route, designScreen, buildScreen }) {
  const design = namedControls(designScreen, page, route, "design");
  const build = namedControls(buildScreen, page, route, "build");
  const unavailable = [...design.findings, ...build.findings];
  if (unavailable.length) return { comparable: false, findings: [], unavailable };

  const findings = [];
  const common = [...design.controls.keys()].filter((name) => build.controls.has(name));
  for (const name of common) {
    const designed = design.controls.get(name);
    const built = build.controls.get(name);
    if (aboveFold(designed.rect) !== aboveFold(built.rect)) {
      findings.push(`${pageName(page)}: control ${quoted(designed.label)} is ${aboveFold(designed.rect) ? "above" : "below"} the fold in design but ${aboveFold(built.rect) ? "above" : "below"} it on ${routeName(route)}.`);
    }
  }

  for (let index = 0; index < common.length; index += 1) {
    for (let next = index + 1; next < common.length; next += 1) {
      const leftName = common[index];
      const rightName = common[next];
      const designedLeft = design.controls.get(leftName);
      const designedRight = design.controls.get(rightName);
      const builtLeft = build.controls.get(leftName);
      const builtRight = build.controls.get(rightName);
      const designSharesRow = sharesRow(designedLeft.rect, designedRight.rect);
      const buildSharesRow = sharesRow(builtLeft.rect, builtRight.rect);
      if (designSharesRow !== buildSharesRow) {
        findings.push(`${pageName(page)}: controls ${quoted(designedLeft.label)} and ${quoted(designedRight.label)} ${designSharesRow ? "share a row in design but are stacked" : "are stacked in design but share a row"} on ${routeName(route)}.`);
        continue;
      }
      if (!designSharesRow && ((designedLeft.rect.top < designedRight.rect.top) !== (builtLeft.rect.top < builtRight.rect.top))) {
        findings.push(`${pageName(page)}: controls ${quoted(designedLeft.label)} and ${quoted(designedRight.label)} have a different top-to-bottom reading order on ${routeName(route)}.`);
      }
      if (widthRelationshipChanged(designedLeft.rect, designedRight.rect, builtLeft.rect, builtRight.rect)) {
        findings.push(`${pageName(page)}: controls ${quoted(designedLeft.label)} and ${quoted(designedRight.label)} have a different relative-width relationship on ${routeName(route)}.`);
      }
    }
  }
  return { comparable: true, findings, unavailable: [] };
}

function uniqueRoutePairs(manifest) {
  if (!Array.isArray(manifest)) return [];
  return manifest.flatMap((row) => {
    if (row?.kind !== "routed" || !row.page) return [];
    const routes = row.shippingRoutes ?? (typeof row.route === "string" ? [row.route] : []);
    return Array.isArray(routes) && routes.length === 1 ? [{ page: row.page, route: routes[0] }] : [];
  });
}

/** Run structural comparison first, then attach lower-priority geometry. */
export function auditScreenRelationships(input) {
  const structural = auditStructuralInventories(structuralInput(input));
  const geometryFindings = [];
  const geometryUnavailable = [];
  for (const { page, route } of uniqueRoutePairs(input.manifest)) {
    const result = compareRouteGeometry({
      page,
      route,
      designScreen: input.designScreens?.[page],
      buildScreen: input.buildScreens?.[route],
    });
    geometryFindings.push(...result.findings);
    geometryUnavailable.push(...result.unavailable);
  }
  return {
    ok: structural.ok && geometryUnavailable.length === 0,
    structuralFindings: structural.findings,
    geometryFindings,
    geometryUnavailable,
  };
}

function report(result) {
  console.error("TASK7053 STRUCTURAL FINDINGS:");
  if (result.structuralFindings.length) {
    for (const finding of result.structuralFindings) console.error(`TASK7053 STRUCTURAL DIFFERENCE: ${finding}`);
  } else {
    console.error("TASK7053 STRUCTURAL PASS controls agree control-for-control");
  }
  console.error("TASK7053 GEOMETRY ADVISORIES:");
  for (const finding of result.geometryFindings) console.error(`TASK7053 GEOMETRY ADVISORY: ${finding}`);
  for (const finding of result.geometryUnavailable) console.error(`TASK7053 GEOMETRY UNAVAILABLE: ${finding}`);
  if (!result.geometryFindings.length && !result.geometryUnavailable.length) console.error("TASK7053 GEOMETRY PASS relationship findings=0");
}

async function main() {
  const index = process.argv.indexOf("--fixture");
  if (index === -1 || !process.argv[index + 1]) throw new Error("TASK7053 needs --fixture with captured screens");
  const fixture = JSON.parse(await readFile(path.resolve(process.cwd(), process.argv[index + 1]), "utf8"));
  const result = auditScreenRelationships(fixture);
  report(result);
  if (!result.ok) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(`TASK7053 GEOMETRY UNAVAILABLE: ${error.message}`); process.exitCode = 1; });
}
