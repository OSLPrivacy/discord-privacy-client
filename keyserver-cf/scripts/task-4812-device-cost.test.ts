import { describe, expect, it } from "vitest";
import {
  assertDataThisMonthScreen,
  measureHundredMessageSecondDeviceDelta,
  measureSecondDeviceCosts,
  renderAddDeviceCostScreen,
  renderDataThisMonthScreen,
  TASK_4812_EXPECTED_SYNC_SLOT_BYTES,
  type DataUsageInputs,
} from "../src/lib/device-cost-meter.js";

describe("TASK 4812 second-device data cost meter", () => {
  it("measures the second-device per-message sync slot from a 100-message run", () => {
    const run = measureHundredMessageSecondDeviceDelta();
    console.log(
      `TASK4812_MESSAGE_RUN one_device=${run.oneDeviceBytes} ` +
      `two_device=${run.twoDeviceBytes} difference=${run.differenceBytes} ` +
      `expected=${run.expectedBytes} within_5_percent=${run.withinFivePercent}`,
    );

    expect(TASK_4812_EXPECTED_SYNC_SLOT_BYTES).toBe(1190);
    expect(run.expectedBytes).toBe(100 * 1190);
    expect(run.withinFivePercent).toBe(true);
  });

  it("lists connection, messages, attachments, stories, voice, and sync under Data this month", () => {
    const measured = measureSecondDeviceCosts();
    const usage: DataUsageInputs = {
      connectionBytes: measured.idleConnectionBytesPerMonth,
      messageBytes: 77_000,
      attachmentBytes: 42_000,
      storyBytes: 8_000,
      voiceBytes: 3_000,
      syncBytes: measured.perMessageSyncBytes * 100,
    };
    const omitSync = process.env.TASK4812_OMIT_SYNC_FROM_METER === "1";
    const screen = renderDataThisMonthScreen(usage, { includeSyncLine: !omitSync });
    const lineNames = screen.lines.map((line) => line.name).join(",");
    const lineSum = screen.lines.reduce((sum, line) => sum + line.bytes, 0);

    console.log(
      `TASK4812_DATA_SCREEN lines=${screen.lines.length} names=${lineNames} ` +
      `line_sum=${lineSum} printed_total=${screen.printedTotalBytes} ` +
      `remainder=${screen.remainderBytes}`,
    );

    if (!omitSync) {
      expect(screen.lines).toHaveLength(6);
      expect(lineNames).toBe("Connection,Messages,Attachments,Stories,Voice,Sync");
    }
    assertDataThisMonthScreen(screen);
    expect(screen.remainderBytes).toBe(0);
  });

  it("renders the add-device cost before adding the device from measured values", () => {
    const measured = measureSecondDeviceCosts();
    const screen = renderAddDeviceCostScreen(measured);
    const screenText = screen.lines.map((line) => line.text).join(" | ");

    console.log(
      `TASK4812_ADD_DEVICE_SCREEN device_added=${screen.deviceAdded} ` +
      `measured_at=${screen.measuredAt} per_message=${screen.perMessageSyncBytes} ` +
      `idle_month=${screen.idleConnectionBytesPerMonth} text="${screenText}"`,
    );

    expect(screen.deviceAdded).toBe(false);
    expect(screen.measuredAt).toBe("before_add");
    expect(screen.perMessageSyncBytes).toBe(measured.perMessageSyncBytes);
    expect(screen.idleConnectionBytesPerMonth).toBe(measured.idleConnectionBytesPerMonth);
    expect(screenText).toContain(`${measured.perMessageSyncBytes} bytes per message`);
    expect(screenText).toContain(`${measured.idleConnectionBytesPerMonth} bytes per month while idle`);
  });
});
