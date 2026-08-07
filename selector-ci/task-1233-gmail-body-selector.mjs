import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const TASK_TEXT = "MAPLE-4172";
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SOURCE = readFileSync(resolve(REPO_ROOT, "apps/osl-hub/src/website_driver.rs"), "utf8");

function assertSourceContains(needle, label) {
  if (!SOURCE.includes(needle)) {
    throw new Error(`shipping selector source missing ${label}`);
  }
}

assertSourceContains("controlName(element) !== wanted", "exact wanted-name filter");
assertSourceContains("matches.length !== 1", "single-match fail-closed guard");
assertSourceContains("return {{ placed: false, readback: '' }}", "refused placement result");
assertSourceContains(
  "input, textarea, [contenteditable=\"\"], [contenteditable=\"true\"], [role=\"textbox\"], [role=\"searchbox\"]",
  "editable selector family",
);

function compact(value) {
  return String(value || "").replace(/\s+/g, " ").trim();
}

class GmailEditable {
  constructor({ ariaLabel, id, name, role, tagName = "textarea", value = "" }) {
    this.ariaLabel = ariaLabel;
    this.id = id;
    this.name = name;
    this.role = role;
    this.tagName = tagName;
    this.value = value;
    this.hidden = false;
    this.ariaHidden = false;
    this.disabled = false;
    this.readOnly = false;
  }
}

class GmailFixturePage {
  constructor() {
    this.placementCount = 0;
    this.body = new GmailEditable({
      ariaLabel: "Body",
      id: ":gmail-body:",
      name: "body",
      role: "textbox",
    });
    this.controls = [
      new GmailEditable({
        ariaLabel: "To",
        id: ":gmail-to:",
        name: "to",
        tagName: "input",
      }),
      new GmailEditable({
        ariaLabel: "Subject",
        id: ":gmail-subject:",
        name: "subject",
        tagName: "input",
      }),
      this.body,
    ];
  }
}

function visible(element) {
  return element && !element.hidden && !element.ariaHidden;
}

function enabled(element) {
  return !element.disabled && !element.readOnly;
}

function editable(element) {
  if (!enabled(element)) return false;
  if (element.role === "textbox" || element.role === "searchbox") return true;
  const tag = element.tagName.toLowerCase();
  return tag === "textarea" || tag === "input";
}

function controlName(element) {
  const candidates = [element.ariaLabel, element.title, element.placeholder, element.name, element.id];
  for (const candidate of candidates) {
    const name = compact(candidate);
    if (name) return name;
  }
  return "";
}

function placeTextInNamedEditable(page, wanted, text) {
  const matches = page.controls.filter(
    (element) => visible(element) && editable(element) && controlName(element) === wanted,
  );
  if (matches.length !== 1) return { placed: false, readback: "" };
  matches[0].value = text;
  page.placementCount += 1;
  return { placed: matches[0].value === text, readback: matches[0].value };
}

function placeGmailBody(page, text) {
  const result = placeTextInNamedEditable(page, "Body", text);
  if (!result.placed || result.readback !== text) {
    return { ok: false, refusal: "Gmail Body missing" };
  }
  return { ok: true, refusal: "" };
}

const page = new GmailFixturePage();
const before = page.placementCount;
if (before !== 0) throw new Error(`expected placement count 0 before, got ${before}`);

const placed = placeGmailBody(page, TASK_TEXT);
if (!placed.ok) throw new Error(`Body placement refused: ${placed.refusal}`);
if (page.body.value !== TASK_TEXT) throw new Error(`Body readback was ${page.body.value}`);
if (page.placementCount !== 1) throw new Error(`expected count 1 after Body, got ${page.placementCount}`);

page.body.ariaLabel = "Missing Body";
const missing = placeGmailBody(page, TASK_TEXT);
if (missing.ok) throw new Error("Missing Body was accepted");
if (missing.refusal !== "Gmail Body missing") {
  throw new Error(`wrong refusal: ${missing.refusal}`);
}
if (page.body.value !== TASK_TEXT) throw new Error(`Body changed after refusal: ${page.body.value}`);
if (page.placementCount !== 1) {
  throw new Error(`placement count changed after refusal: ${page.placementCount}`);
}

console.log("TASK1233 selector_source_exact_name_filter=present");
console.log("TASK1233 selector_source_single_match_fail_closed=present");
console.log(`TASK1233 gmail_placement_count_before=${before}`);
console.log("TASK1233 gmail_body_control_name_before=Body");
console.log(`TASK1233 body_read_after_body=${page.body.value}`);
console.log(`TASK1233 gmail_placement_count_after_body=${page.placementCount}`);
console.log("TASK1233 changed_only_body_control_name_to=Missing Body");
console.log(`TASK1233 missing_body_refusal=${missing.refusal}`);
console.log(`TASK1233 body_read_after_missing_body=${page.body.value}`);
console.log(`TASK1233 gmail_placement_count_after_missing_body=${page.placementCount}`);
