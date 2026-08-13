/**
 * TASK 4664 — one live, itemised view of the current month's data meter.
 *
 * Both surfaces deliberately receive the same `DataMeterSnapshot`.  The
 * rendered total is derived from the six rendered rows; callers cannot supply
 * a second, potentially stale total for either the Settings screen or hover.
 */

export const DATA_METER_CLASSES = [
  "background connection",
  "messages",
  "attachments",
  "stories and posts",
  "voice",
  "multi-device sync",
] as const;

export type DataMeterClass = typeof DATA_METER_CLASSES[number];
export type DataMeterCounters = Readonly<Record<DataMeterClass, number>>;

export interface DataMeterSnapshot {
  /** Counters read from the persisted class-debit ledger, never display state. */
  readonly counters: DataMeterCounters;
}

export interface DataMeterView {
  readonly rows: readonly Readonly<{ byteClass: DataMeterClass; bytes: number }>[];
  readonly totalBytes: number;
}

function checkedByteCount(value: number, byteClass: DataMeterClass): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`data meter ${byteClass} counter must be a non-negative safe integer`);
  }
  return value;
}

export function dataMeterView(snapshot: DataMeterSnapshot): DataMeterView {
  let totalBytes = 0;
  const rows = DATA_METER_CLASSES.map((byteClass) => {
    const bytes = checkedByteCount(snapshot.counters[byteClass], byteClass);
    if (!Number.isSafeInteger(totalBytes + bytes)) throw new Error("data meter total exceeds safe integer range");
    totalBytes += bytes;
    return { byteClass, bytes };
  });
  return { rows, totalBytes };
}

function escapeHtml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;");
}

export function formatMeterBytes(bytes: number): string {
  return `${bytes.toLocaleString("en-US")} bytes`;
}

export function dataThisMonthMarkup(snapshot: DataMeterSnapshot): string {
  const view = dataMeterView(snapshot);
  const rows = view.rows.map(({ byteClass, bytes }) =>
    `<div class="data-meter-row" data-data-meter-class="${byteClass}" data-data-meter-bytes="${bytes}"><span>${escapeHtml(byteClass)}</span><output>${formatMeterBytes(bytes)}</output></div>`,
  ).join("");
  return `<section class="data-this-month" data-data-this-month aria-labelledby="data-this-month-title"><h2 id="data-this-month-title">Data this month</h2><div class="data-meter-rows">${rows}</div><div class="data-meter-total" data-data-meter-total="${view.totalBytes}"><strong>Total</strong><output>${formatMeterBytes(view.totalBytes)}</output></div></section>`;
}

/** Compact persistent readout. Its tooltip is the same row markup as Settings. */
export function dataMeterHoverMarkup(snapshot: DataMeterSnapshot): string {
  const view = dataMeterView(snapshot);
  const rows = view.rows.map(({ byteClass, bytes }) =>
    `<div data-data-meter-class="${byteClass}" data-data-meter-bytes="${bytes}"><span>${escapeHtml(byteClass)}</span><output>${formatMeterBytes(bytes)}</output></div>`,
  ).join("");
  return `<span class="data-meter-hover in-dom-tooltip-anchor" data-data-meter-hover data-data-meter-total="${view.totalBytes}" tabindex="0" aria-label="Data this month: ${formatMeterBytes(view.totalBytes)}"><span aria-hidden="true">Data ${formatMeterBytes(view.totalBytes)}</span><span class="in-dom-tooltip data-meter-hover-tooltip" role="tooltip"><strong>Data this month</strong>${rows}<div><strong>Total</strong><output>${formatMeterBytes(view.totalBytes)}</output></div></span></span>`;
}

/**
 * Read-model bridge for persisted meter events. Replacing it after a durable
 * debit immediately updates both mounted surfaces; no route change is needed.
 */
export class LiveDataMeter {
  #snapshot: DataMeterSnapshot;
  #listeners = new Set<(snapshot: DataMeterSnapshot) => void>();

  constructor(snapshot: DataMeterSnapshot) {
    dataMeterView(snapshot);
    this.#snapshot = snapshot;
  }

  snapshot(): DataMeterSnapshot {
    return this.#snapshot;
  }

  replace(snapshot: DataMeterSnapshot): void {
    dataMeterView(snapshot);
    this.#snapshot = snapshot;
    for (const listener of this.#listeners) listener(snapshot);
  }

  subscribe(listener: (snapshot: DataMeterSnapshot) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }
}

/** Bind all current screen/hover mounts to the one persisted-counter source. */
export function bindDataMeterSurfaces(root: ParentNode, meter: LiveDataMeter): () => void {
  const render = (snapshot: DataMeterSnapshot): void => {
    root.querySelectorAll<HTMLElement>("[data-data-this-month]").forEach((element) => { element.outerHTML = dataThisMonthMarkup(snapshot); });
    root.querySelectorAll<HTMLElement>("[data-data-meter-hover]").forEach((element) => { element.outerHTML = dataMeterHoverMarkup(snapshot); });
  };
  render(meter.snapshot());
  return meter.subscribe(render);
}
