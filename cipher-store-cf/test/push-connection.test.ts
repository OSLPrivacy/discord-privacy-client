import connectionSource from "../src/realtime/connection.ts?raw";
import {
  EMPTY_FRAME,
  FRAME_BYTES,
  IDLE_TICK,
  encodeWakeupFrame,
} from "../src/realtime/connection.js";
import { describe, expect, it } from "vitest";

const TAG = "a".repeat(32);
const BLOB = "b".repeat(32);

describe("T1-T51 push Durable Object", () => {
  it("emits only fixed-size delivery_tag/blob_id wakeup frames", () => {
    const real = encodeWakeupFrame({ delivery_tag: TAG, blob_id: BLOB });

    expect(IDLE_TICK).toHaveLength(FRAME_BYTES);
    expect(EMPTY_FRAME).toHaveLength(FRAME_BYTES);
    expect(real).toHaveLength(FRAME_BYTES);
    expect(JSON.parse(EMPTY_FRAME.trim())).toEqual({
      delivery_tag: "0".repeat(32),
      blob_id: "0".repeat(32),
    });
    expect(JSON.parse(real.trim())).toEqual({ delivery_tag: TAG, blob_id: BLOB });
    expect(() => encodeWakeupFrame({ delivery_tag: "P", blob_id: BLOB })).toThrow();
  });

  it("keeps idle ticks in the hibernation auto-response path, not an alarm", () => {
    // Cost-shape sabotage: replacing the auto-response with an alarm/timer
    // wakes the object per tick, so this assertion must turn red.
    expect(connectionSource).toContain("this.ctx.setWebSocketAutoResponse(AUTO_RESPONSE)");
    expect(connectionSource).toContain("this.ctx.setWebSocketAutoResponse()");
    expect(connectionSource).not.toMatch(/setAlarm|\balarm\s*\(|setTimeout|setInterval/);
  });
});
