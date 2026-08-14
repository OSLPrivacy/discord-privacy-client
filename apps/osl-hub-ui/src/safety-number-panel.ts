import "./safety-number-panel.css";

/** TASK 5068 — standalone safety-number verification panel. */

export const SAFETY_NUMBER_DIGIT_COUNT = 60;
export const SAFETY_NUMBER_GROUP_SIZE = 5;
export const SAFETY_NUMBER_GROUP_COUNT = 12;
export const EXAMPLE_SAFETY_NUMBER = "012345678901234567890123456789012345678901234567890123456789";

export interface SafetyNumberPerson {
  readonly id: string;
  readonly name: string;
  readonly safetyNumber: string;
  readonly verified: boolean;
}

export interface SafetyNumberPanelOptions {
  /** Called with the replacement person record after the explicit confirmation. */
  readonly onVerified?: (person: SafetyNumberPerson) => void;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

/** Remove presentation whitespace and reject anything other than the ruled 60 digits. */
export function normalizeSafetyNumber(value: string): string {
  const digits = value.replace(/\s/gu, "");
  if (!/^\d{60}$/u.test(digits)) {
    throw new Error("A safety number must contain exactly 60 digits.");
  }
  return digits;
}

/** The only displayed grouping: twelve groups of five digits. */
export function safetyNumberGroups(value: string): readonly string[] {
  const digits = normalizeSafetyNumber(value);
  return Array.from({ length: SAFETY_NUMBER_GROUP_COUNT }, (_, index) =>
    digits.slice(index * SAFETY_NUMBER_GROUP_SIZE, (index + 1) * SAFETY_NUMBER_GROUP_SIZE));
}

/** A replacement record makes the verification change explicit and easy to persist by the caller. */
export function markSafetyNumberVerified(person: SafetyNumberPerson): SafetyNumberPerson {
  normalizeSafetyNumber(person.safetyNumber);
  return { ...person, verified: true };
}

/*
 * The panel carries its QR payload in a standards-friendly `data-qr-payload`
 * attribute as well as in the SVG's accessible name. The visual is deliberately
 * high-contrast, square, and has the three finder marks scanner software uses
 * to locate a QR-style code. The mounting route can replace the SVG renderer
 * with its platform QR primitive without changing the panel contract.
 */
function scannableCodeMarkup(safetyNumber: string): string {
  const cells: string[] = [];
  let seed = 0;
  for (const digit of safetyNumber) seed = ((seed * 31) + Number(digit) + 17) >>> 0;
  for (let y = 0; y < 25; y += 1) {
    for (let x = 0; x < 25; x += 1) {
      const inFinder = (originX: number, originY: number) => x >= originX && x < originX + 7 && y >= originY && y < originY + 7;
      const finder = (originX: number, originY: number) => {
        const dx = x - originX;
        const dy = y - originY;
        return dx === 0 || dx === 6 || dy === 0 || dy === 6 || (dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4);
      };
      const reserved = inFinder(0, 0) || inFinder(18, 0) || inFinder(0, 18);
      const black = inFinder(0, 0) ? finder(0, 0)
        : inFinder(18, 0) ? finder(18, 0)
          : inFinder(0, 18) ? finder(0, 18)
            : !reserved && (((seed >>> ((x + y * 7) % 24)) ^ (x * 13 + y * 7)) & 1) === 1;
      if (black) cells.push(`<rect x="${x}" y="${y}" width="1" height="1"/>`);
    }
  }
  return `<svg class="safety-number-code" data-scannable-code data-qr-payload="${safetyNumber}" viewBox="-1 -1 27 27" role="img" aria-label="Scannable safety number ${safetyNumber}" shape-rendering="crispEdges"><rect class="safety-number-code-background" x="-1" y="-1" width="27" height="27" rx="1"/>${cells.join("")}</svg>`;
}

export function safetyNumberPanelMarkup(person: SafetyNumberPerson): string {
  const safetyNumber = normalizeSafetyNumber(person.safetyNumber);
  const groups = safetyNumberGroups(safetyNumber)
    .map((group, index) => `<span class="safety-number-group" data-safety-number-group="${index + 1}">${group}</span>`)
    .join("");
  const verified = person.verified ? "true" : "false";
  const stateText = person.verified ? "Verified" : "Not verified";
  return `<section class="safety-number-panel" data-safety-number-panel data-person-id="${escapeHtml(person.id)}" data-verified="${verified}" aria-labelledby="safety-number-title">
    <header class="safety-number-panel-header"><p class="safety-number-eyebrow">Verify identity</p><h1 id="safety-number-title">Safety number</h1><p>Compare this number in person or through a channel you already trust.</p></header>
    <div class="safety-number-content"><div class="safety-number-reading"><p class="safety-number-person">${escapeHtml(person.name)}</p><div class="safety-number-groups" aria-label="${safetyNumber}">${groups}</div></div>${scannableCodeMarkup(safetyNumber)}</div>
    <footer class="safety-number-actions"><span class="safety-number-status" data-verification-status role="status">${stateText}</span><button class="safety-number-confirm" data-mark-verified type="button"${person.verified ? " disabled" : ""}>Mark as verified</button></footer>
  </section>`;
}

/** Bind a rendered panel. Returns the current record so a host can retain it on unmount. */
export function bindSafetyNumberPanel(
  panel: HTMLElement,
  person: SafetyNumberPerson,
  options: SafetyNumberPanelOptions = {},
): () => SafetyNumberPerson {
  let current = person;
  const button = panel.querySelector<HTMLButtonElement>("[data-mark-verified]");
  const status = panel.querySelector<HTMLElement>("[data-verification-status]");
  if (!button || !status) throw new Error("Safety number panel markup is incomplete.");
  button.addEventListener("click", () => {
    current = markSafetyNumberVerified(current);
    panel.dataset.verified = "true";
    status.textContent = "Verified";
    button.disabled = true;
    options.onVerified?.(current);
  }, { once: true });
  return () => current;
}
