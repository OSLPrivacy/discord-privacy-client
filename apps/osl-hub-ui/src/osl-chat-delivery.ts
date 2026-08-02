/**
 * Connect OSL Chat delivery to T1's already-open realtime frame stream.
 *
 * A wakeup is produced by the native constant-rate client, not by a renderer
 * timer.  The frame has already paid the network-observability cost; this
 * listener only asks the established delivery runtime to consume local inbox
 * state.  It deliberately never creates a timer or starts network work.
 */
export interface OslChatFrameSource {
  onWakeup(callback: () => void): Promise<() => void>;
}

export interface OslChatFrameDelivery {
  sync(): Promise<void>;
}

export async function attachOslChatFrameDelivery(
  source: OslChatFrameSource,
  delivery: OslChatFrameDelivery,
): Promise<() => void> {
  let draining = false;
  return source.onWakeup(() => {
    if (draining) return;
    draining = true;
    void delivery.sync().finally(() => { draining = false; });
  });
}
