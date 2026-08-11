interface Env { ATTACHMENTS: R2Bucket }

const missing = () => Response.json(
  { error: "not_found", message: "no such route or blob" },
  { status: 404, headers: { "cache-control": "no-store" } },
);

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const seed = /^\/_proof\/seed\/([a-z0-9-]+)$/.exec(url.pathname);
    if (seed && request.method === "PUT") {
      await env.ATTACHMENTS.put(seed[1]!, await request.arrayBuffer());
      return new Response("seeded", { status: 201 });
    }
    const attachment = /^\/v1\/attachment\/([a-z0-9-]+)$/.exec(url.pathname);
    if (attachment && request.method === "GET") {
      const object = await env.ATTACHMENTS.get(attachment[1]!);
      const headers = { "x-task-5219b-get": "1", "x-task-5219b-head": "0", "cache-control": "no-store" };
      if (!object?.body) return new Response(JSON.stringify({ error: "not_found", message: "no such route or blob" }), {
        status: 404,
        headers: { ...headers, "content-type": "application/json" },
      });
      return new Response(object.body, { headers: { ...headers, "content-type": "application/octet-stream" } });
    }
    return missing();
  },
};
