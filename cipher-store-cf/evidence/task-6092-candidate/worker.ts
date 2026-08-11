interface Env {
  DB: D1Database;
  OBJECTS: R2Bucket;
  TASK6092_ADMIN_TOKEN: string;
  TASK6092_PLAINTEXT_MUTANT: string;
}

const JSON_HEADERS = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
};

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), { status, headers: JSON_HEADERS });
}

function hexId(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{32}$/.test(value);
}

function admin(request: Request, env: Env): boolean {
  const supplied = request.headers.get("authorization") ?? "";
  return supplied === `Bearer ${env.TASK6092_ADMIN_TOKEN}`;
}

async function isFrozen(env: Env): Promise<boolean> {
  const row = await env.DB.prepare(
    "SELECT frozen FROM service_state WHERE singleton = 1",
  ).first<{ frozen: number }>();
  return row?.frozen === 1;
}

async function store(request: Request, env: Env): Promise<Response> {
  if (await isFrozen(env)) return json({ error: "frozen" }, 503);
  let body: Record<string, unknown>;
  try {
    body = await request.json<Record<string, unknown>>();
  } catch {
    return json({ error: "bad_json" }, 400);
  }
  if (!hexId(body.id)
      || typeof body.recipient !== "string"
      || !/^[0-9a-f]{32}$/.test(body.recipient)
      || (body.kind !== "text" && body.kind !== "file")
      || typeof body.offline !== "boolean"
      || typeof body.envelope_b64 !== "string") {
    return json({ error: "bad_record" }, 400);
  }
  let envelope: Uint8Array;
  try {
    envelope = Uint8Array.from(atob(body.envelope_b64), (char) => char.charCodeAt(0));
  } catch {
    return json({ error: "bad_envelope" }, 400);
  }
  if (envelope.byteLength < 120) return json({ error: "short_envelope" }, 400);

  const objectKey = `messages/${body.id}`;
  await env.OBJECTS.put(objectKey, envelope, {
    httpMetadata: { contentType: "application/octet-stream" },
  });
  await env.DB.prepare(
    `INSERT INTO messages
      (message_id, recipient_id, kind, offline, object_key, envelope, consumed, created_at)
     VALUES (?, ?, ?, ?, ?, ?, 0, ?)`
  ).bind(
    body.id,
    body.recipient,
    body.kind,
    body.offline ? 1 : 0,
    objectKey,
    envelope,
    Date.now(),
  ).run();

  // Live break-it seam. The normal request carries ciphertext only. The
  // mutant writes one byte that every generated plaintext canary begins with,
  // while leaving the encrypted object and ordinary readback untouched.
  if (env.TASK6092_PLAINTEXT_MUTANT === "d1-one-byte") {
    await env.DB.prepare(
      "INSERT INTO surface_leaks (surface, message_id, leak_bytes) VALUES (?, ?, ?)",
    ).bind("primary_database.surface_leaks.leak_bytes", body.id, new Uint8Array([0x7e])).run();
  }
  return json({ id: body.id, object_key: objectKey, mutant: env.TASK6092_PLAINTEXT_MUTANT }, 201);
}

async function fetchMessage(id: string, env: Env): Promise<Response> {
  if (!hexId(id)) return json({ error: "not_found" }, 404);
  const row = await env.DB.prepare(
    "SELECT object_key, consumed FROM messages WHERE message_id = ? LIMIT 1",
  ).bind(id).first<{ object_key: string; consumed: number }>();
  if (!row || row.consumed !== 0) return json({ error: "not_found" }, 404);
  const object = await env.OBJECTS.get(row.object_key);
  if (!object) return json({ error: "not_found" }, 404);
  return new Response(object.body, {
    status: 200,
    headers: { "content-type": "application/octet-stream", "cache-control": "no-store" },
  });
}

async function consume(id: string, env: Env): Promise<Response> {
  if (await isFrozen(env)) return json({ error: "frozen" }, 503);
  const changed = await env.DB.prepare(
    "UPDATE messages SET consumed = 1 WHERE message_id = ? AND consumed = 0",
  ).bind(id).run();
  if ((changed.meta.changes ?? 0) !== 1) return json({ error: "already_consumed" }, 409);
  return json({ consumed: true });
}

async function inbox(url: URL, env: Env): Promise<Response> {
  const recipient = url.searchParams.get("recipient") ?? "";
  if (!/^[0-9a-f]{32}$/.test(recipient)) return json({ error: "bad_recipient" }, 400);
  const rows = await env.DB.prepare(
    `SELECT message_id, kind, offline, object_key
       FROM messages
      WHERE recipient_id = ? AND consumed = 0
      ORDER BY created_at, message_id`,
  ).bind(recipient).all();
  return json({ items: rows.results });
}

async function inventory(env: Env): Promise<Response> {
  const listed = await env.OBJECTS.list({ limit: 1000 });
  return json({
    objects: listed.objects.map((object) => ({ key: object.key, size: object.size, etag: object.etag })),
    truncated: listed.truncated,
  });
}

async function cleanup(env: Env): Promise<Response> {
  let deleted = 0;
  while (true) {
    const page = await env.OBJECTS.list({ limit: 1000 });
    if (page.objects.length === 0) break;
    await env.OBJECTS.delete(page.objects.map((object) => object.key));
    deleted += page.objects.length;
  }
  return json({ deleted });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/healthz") return json({ ok: true, mutant: env.TASK6092_PLAINTEXT_MUTANT });
    if (url.pathname === "/v1/messages" && request.method === "POST") return store(request, env);
    if (url.pathname === "/v1/inbox" && request.method === "GET") return inbox(url, env);
    const message = /^\/v1\/messages\/([0-9a-f]{32})$/.exec(url.pathname);
    if (message && request.method === "GET") return fetchMessage(message[1]!, env);
    const consumed = /^\/v1\/messages\/([0-9a-f]{32})\/consume$/.exec(url.pathname);
    if (consumed && request.method === "POST") return consume(consumed[1]!, env);
    if (url.pathname === "/__task6092/inventory" && request.method === "GET") {
      if (!admin(request, env)) return json({ error: "forbidden" }, 403);
      return inventory(env);
    }
    if (url.pathname === "/__task6092/cleanup" && request.method === "POST") {
      if (!admin(request, env)) return json({ error: "forbidden" }, 403);
      return cleanup(env);
    }
    return json({ error: "not_found" }, 404);
  },
};
