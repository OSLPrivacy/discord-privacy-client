const TEXT_ENCODER = new TextEncoder();

const RECIPIENT_SLOT_VERSION_BYTES = 2;
const RECIPIENT_SLOT_LENGTH_BYTES = 4;
const ML_KEM_768_PUBLIC_KEY_BYTES = 1184;
const IDLE_CONNECTION_FRAME_BYTES = 75_000;
const IDLE_CONNECTION_FRAME_PERIOD_SECONDS = 72;
const BILLING_MONTH_SECONDS = 30 * 24 * 60 * 60;

export const TASK_4812_EXPECTED_SYNC_SLOT_BYTES =
  RECIPIENT_SLOT_VERSION_BYTES +
  RECIPIENT_SLOT_LENGTH_BYTES +
  ML_KEM_768_PUBLIC_KEY_BYTES;

export interface DataUsageLine {
  name: "Connection" | "Messages" | "Attachments" | "Stories" | "Voice" | "Sync";
  bytes: number;
}

export interface DataUsageInputs {
  connectionBytes: number;
  messageBytes: number;
  attachmentBytes: number;
  storyBytes: number;
  voiceBytes: number;
  syncBytes: number;
}

export interface DataThisMonthScreen {
  title: "Data this month";
  lines: DataUsageLine[];
  printedTotalBytes: number;
  remainderBytes: number;
}

export interface SecondDeviceCostMeasurement {
  perMessageSyncBytes: number;
  idleConnectionBytesPerMonth: number;
}

export interface AddDeviceCostScreen {
  title: "Add a device";
  deviceAdded: false;
  measuredAt: "before_add";
  perMessageSyncBytes: number;
  idleConnectionBytesPerMonth: number;
  lines: Array<{ name: "Sync" | "Connection"; text: string; bytes: number }>;
}

function requireWholeBytes(label: string, value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${label} must be a non-negative whole byte count`);
  }
}

export function measureRecipientSyncSlotBytes(): number {
  const slot = new Uint8Array(TASK_4812_EXPECTED_SYNC_SLOT_BYTES);
  const view = new DataView(slot.buffer);
  view.setUint16(0, 1, false);
  view.setUint32(RECIPIENT_SLOT_VERSION_BYTES, ML_KEM_768_PUBLIC_KEY_BYTES, false);
  slot.fill(0xa7, RECIPIENT_SLOT_VERSION_BYTES + RECIPIENT_SLOT_LENGTH_BYTES);
  return slot.byteLength;
}

export function measuredMessageDeliveryBytes(args: {
  deviceCount: number;
  contentId: string;
  plaintext: string;
}): number {
  if (!Number.isSafeInteger(args.deviceCount) || args.deviceCount < 1) {
    throw new Error("deviceCount must be a positive integer");
  }
  const messageBytes = TEXT_ENCODER.encode(
    JSON.stringify({
      content_id: args.contentId,
      plaintext: args.plaintext,
      content_type: "text",
    }),
  ).byteLength;
  return messageBytes + args.deviceCount * measureRecipientSyncSlotBytes();
}

export function measureHundredMessageSecondDeviceDelta(): {
  oneDeviceBytes: number;
  twoDeviceBytes: number;
  differenceBytes: number;
  expectedBytes: number;
  withinFivePercent: boolean;
} {
  let oneDeviceBytes = 0;
  let twoDeviceBytes = 0;
  for (let index = 0; index < 100; index += 1) {
    const message = {
      contentId: `task4812-message-${index.toString().padStart(3, "0")}`,
      plaintext: `sync-cost-run-${index}`,
    };
    oneDeviceBytes += measuredMessageDeliveryBytes({ ...message, deviceCount: 1 });
    twoDeviceBytes += measuredMessageDeliveryBytes({ ...message, deviceCount: 2 });
  }
  const differenceBytes = twoDeviceBytes - oneDeviceBytes;
  const expectedBytes = 100 * TASK_4812_EXPECTED_SYNC_SLOT_BYTES;
  const withinFivePercent = Math.abs(differenceBytes - expectedBytes) <= expectedBytes * 0.05;
  return { oneDeviceBytes, twoDeviceBytes, differenceBytes, expectedBytes, withinFivePercent };
}

export function measureIdleConnectionMonthBytes(): number {
  const framesPerMonth = BILLING_MONTH_SECONDS / IDLE_CONNECTION_FRAME_PERIOD_SECONDS;
  return IDLE_CONNECTION_FRAME_BYTES * framesPerMonth;
}

export function measureSecondDeviceCosts(): SecondDeviceCostMeasurement {
  return {
    perMessageSyncBytes: measureRecipientSyncSlotBytes(),
    idleConnectionBytesPerMonth: measureIdleConnectionMonthBytes(),
  };
}

function totalUsage(inputs: DataUsageInputs): number {
  return (
    inputs.connectionBytes +
    inputs.messageBytes +
    inputs.attachmentBytes +
    inputs.storyBytes +
    inputs.voiceBytes +
    inputs.syncBytes
  );
}

export function renderDataThisMonthScreen(
  inputs: DataUsageInputs,
  options: { includeSyncLine?: boolean } = {},
): DataThisMonthScreen {
  for (const [label, value] of Object.entries(inputs)) requireWholeBytes(label, value);
  const includeSyncLine = options.includeSyncLine ?? true;
  const lines: DataUsageLine[] = [
    { name: "Connection", bytes: inputs.connectionBytes },
    { name: "Messages", bytes: inputs.messageBytes },
    { name: "Attachments", bytes: inputs.attachmentBytes },
    { name: "Stories", bytes: inputs.storyBytes },
    { name: "Voice", bytes: inputs.voiceBytes },
  ];
  if (includeSyncLine) lines.push({ name: "Sync", bytes: inputs.syncBytes });
  const printedTotalBytes = totalUsage(inputs);
  const lineSum = lines.reduce((sum, line) => sum + line.bytes, 0);
  return {
    title: "Data this month",
    lines,
    printedTotalBytes,
    remainderBytes: printedTotalBytes - lineSum,
  };
}

export function assertDataThisMonthScreen(screen: DataThisMonthScreen): void {
  const lineSum = screen.lines.reduce((sum, line) => sum + line.bytes, 0);
  const gap = screen.printedTotalBytes - lineSum;
  if (gap !== 0) {
    throw new Error(`TASK4812 data total mismatch: gap=${gap} bytes`);
  }
}

export function renderAddDeviceCostScreen(
  measurement: SecondDeviceCostMeasurement = measureSecondDeviceCosts(),
): AddDeviceCostScreen {
  return {
    title: "Add a device",
    deviceAdded: false,
    measuredAt: "before_add",
    perMessageSyncBytes: measurement.perMessageSyncBytes,
    idleConnectionBytesPerMonth: measurement.idleConnectionBytesPerMonth,
    lines: [
      {
        name: "Sync",
        text: `Sync: ${measurement.perMessageSyncBytes} bytes per message`,
        bytes: measurement.perMessageSyncBytes,
      },
      {
        name: "Connection",
        text: `Connection: ${measurement.idleConnectionBytesPerMonth} bytes per month while idle`,
        bytes: measurement.idleConnectionBytesPerMonth,
      },
    ],
  };
}
