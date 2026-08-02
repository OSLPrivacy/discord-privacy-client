import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const PREFIX = "3b1a";
const NAMES = ["privacy0","privacy82905","privacy275304","privacy302313","privacy354075","privacy380089","privacy500646","privacy575982","privacy626892","privacy716652","privacy744090","privacy749894","privacy802509","privacy834745","privacy998951","privacy1033794","privacy1063005","privacy1090817","privacy1183991","privacy1201720","privacy1270351","privacy1603226","privacy1772475","privacy1988388","privacy2068845","privacy2138827","privacy2215147","privacy2225396","privacy2258654","privacy2470336","privacy2514578","privacy2612209","privacy2730907","privacy2970268","privacy2985602","privacy2990089","privacy3158178","privacy3163793","privacy3253619","privacy3306232","privacy3638265","privacy3649446","privacy3709701","privacy3900703","privacy3902698","privacy4029952","privacy4070999","privacy4264069","privacy4267955","privacy4286121"];

async function response(): Promise<{ bytes: Uint8Array; rows: string[] }> {
  const result = await SELF.fetch(`http://test/v1/username-bucket/${PREFIX}`, { headers: { "cf-connecting-ip": "203.0.113.101" } });
  expect(result.status).toBe(200);
  const bytes = new Uint8Array(await result.arrayBuffer());
  return { bytes, rows: new TextDecoder().decode(bytes).trim().split("\n") };
}
async function insert(index: number): Promise<void> {
  const user = `osl_${String(index).padStart(52, "a")}`;
  const key = btoa(String.fromCharCode(...new Uint8Array(32).fill(index + 1)));
  await env.DB.prepare(`INSERT INTO users (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub, ik_x25519_signature, registered_at) VALUES (?, ?, ?, ?, ?, ?)`)
    .bind(user, "x", key, "x", "x", "now").run();
  await env.DB.prepare(`INSERT INTO username_directory (username, username_skeleton, display_username, user_id, friend_code, claimed_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)`)
    .bind(NAMES[index]!, NAMES[index]!, NAMES[index]!, user, "code", "now", "now").run();
}

describe("username-bucket privacy floor", () => {
  it("keeps empty, one-real, and fifty-real buckets byte-identical and 1024 rows", async () => {
    const empty = await response();
    await insert(0); const one = await response();
    for (let i = 1; i < NAMES.length; i++) await insert(i);
    const fifty = await response();
    expect(empty.rows).toHaveLength(1024);
    expect(one.rows).toHaveLength(1024);
    expect(fifty.rows).toHaveLength(1024);
    expect(one.bytes.byteLength).toBe(empty.bytes.byteLength);
    expect(fifty.bytes.byteLength).toBe(empty.bytes.byteLength);
  });
});
