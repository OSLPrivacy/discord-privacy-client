import { describe, expect, it } from "vitest";
import {
  DATA_METER_CLASSES,
  LiveDataMeter,
  dataMeterHoverMarkup,
  dataMeterView,
  dataThisMonthMarkup,
  type DataMeterSnapshot,
} from "./data-meter-itemisation";

const initial: DataMeterSnapshot = {
  // Independent persisted class-counter fixture: post (2048), story (1024),
  // archive copy (512), and the independently observed voice debit (4708).
  counters: {
    "background connection": 4096,
    messages: 37,
    attachments: 1_048_576,
    "stories and posts": 3584,
    voice: 4708,
    "multi-device sync": 8192,
  },
};

describe("TASK 4664 — data meter itemisation", () => {
  it("shows identical six class lines and an exact row sum on Settings and hover", () => {
    const view = dataMeterView(initial);
    expect(view.rows.map((row) => row.byteClass)).toEqual(DATA_METER_CLASSES);
    expect(view.rows.find((row) => row.byteClass === "stories and posts")?.bytes).toBe(3584);
    expect(view.rows.find((row) => row.byteClass === "voice")?.bytes).toBe(4708);
    expect(view.totalBytes).toBe(1_069_193);

    const settings = dataThisMonthMarkup(initial);
    const hover = dataMeterHoverMarkup(initial);
    for (const byteClass of DATA_METER_CLASSES) {
      expect(settings).toContain(`data-data-meter-class="${byteClass}"`);
      expect(hover).toContain(`data-data-meter-class="${byteClass}"`);
    }
    expect(settings).toContain('data-data-meter-total="1069193"');
    expect(hover).toContain('data-data-meter-total="1069193"');
  });

  it("updates both mounted surfaces for one additional story upload without reopening", () => {
    const meter = new LiveDataMeter(initial);
    const updates: DataMeterSnapshot[] = [];
    const unbind = meter.subscribe((snapshot) => updates.push(snapshot));
    meter.replace({ counters: { ...initial.counters, "stories and posts": 4818 } });
    expect(updates).toHaveLength(1);
    const settings = dataThisMonthMarkup(updates[0]);
    const hover = dataMeterHoverMarkup(updates[0]);
    expect(settings).toContain('data-data-meter-class="stories and posts" data-data-meter-bytes="4818"');
    expect(hover).toContain('data-data-meter-class="stories and posts" data-data-meter-bytes="4818"');
    expect(settings).toContain('data-data-meter-total="1070427"');
    expect(hover).toContain('data-data-meter-total="1070427"');
    unbind();
  });

  it("rejects invalid persisted counters instead of displaying a mis-summed total", () => {
    expect(() => dataMeterView({ counters: { ...initial.counters, voice: -1 } })).toThrow(/voice counter/);
  });
});
