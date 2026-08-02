import { DurableObject } from "cloudflare:workers";
import type { Env } from "../env.js";

/**
 * T1's wire size, in ASCII characters. Both idle replies and real wakeups
 * are this exact size, so their traffic shape is indistinguishable.
 */
export const FRAME_BYTES = 2048;

const IDENTIFIER_HEX_LENGTH = 32;
const ZERO_IDENTIFIER = "0".repeat(IDENTIFIER_HEX_LENGTH);

// Idle traffic is deliberately a single exact value. Cloudflare can answer it
// without waking a hibernating Durable Object or incurring duration charges.
export const IDLE_TICK = " ".repeat(FRAME_BYTES);

export interface Wakeup {
  delivery_tag: string;
  blob_id: string;
}

function isIdentifier(value: string): boolean {
  return /^[0-9a-f]{32}$/.test(value);
}

/** Build the only server-to-client data-frame shape. */
export function encodeWakeupFrame(wakeup: Wakeup): string {
  if (!isIdentifier(wakeup.delivery_tag) || !isIdentifier(wakeup.blob_id)) {
    throw new Error("wakeup identifiers must be 32 lowercase hexadecimal characters");
  }

  const body = JSON.stringify({
    delivery_tag: wakeup.delivery_tag,
    blob_id: wakeup.blob_id,
  });
  return body.padEnd(FRAME_BYTES, " ");
}

// The decoy has precisely the same public fields as a real wakeup. It never
// carries P, a fetch capability, an ack capability, or any other authority.
export const EMPTY_FRAME = encodeWakeupFrame({
  delivery_tag: ZERO_IDENTIFIER,
  blob_id: ZERO_IDENTIFIER,
});

const AUTO_RESPONSE = new WebSocketRequestResponsePair(IDLE_TICK, EMPTY_FRAME);

/**
 * One anonymous, hibernatable connection.
 *
 * The caller chooses an opaque per-connection Durable Object id. A real storage
 * match queues exactly one wakeup on the live socket, clears the idle mapping,
 * and is emitted by the ordinary handler on the next client tick. Nothing is
 * persisted after the socket closes; T6 supplies fresh matches after reconnect.
 */
export class PushConnection extends DurableObject<Env> {
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.ctx.setWebSocketAutoResponse(AUTO_RESPONSE);
  }

  override async fetch(request: Request): Promise<Response> {
    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("WebSocket upgrade required", { status: 426 });
    }

    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair) as [WebSocket, WebSocket];
    for (const existing of this.ctx.getWebSockets()) existing.close(1012, "replaced");
    this.ctx.acceptWebSocket(server);
    return new Response(null, { status: 101, webSocket: client });
  }

  /**
   * Called only by the storage match path after it finds a live tag. False
   * means that this connection is offline; no wakeup or authority is retained.
   */
  deliver(wakeup: Wakeup): boolean {
    if (!isIdentifier(wakeup.delivery_tag) || !isIdentifier(wakeup.blob_id)) {
      throw new Error("invalid wakeup");
    }

    const socket = this.ctx.getWebSockets()[0];
    if (!socket) return false;

    socket.serializeAttachment(encodeWakeupFrame(wakeup));
    // A genuine hit is the only reason to wake this object. Clearing the
    // mapping makes the next fixed client tick reach webSocketMessage().
    this.ctx.setWebSocketAutoResponse();
    return true;
  }

  override webSocketMessage(socket: WebSocket, message: string | ArrayBuffer): void {
    if (typeof message !== "string" || message.length !== FRAME_BYTES) {
      socket.close(1003, "fixed text tick required");
      return;
    }

    const frame = socket.deserializeAttachment();
    socket.serializeAttachment(null);
    socket.send(typeof frame === "string" ? frame : EMPTY_FRAME);
    this.ctx.setWebSocketAutoResponse(AUTO_RESPONSE);
  }
}
