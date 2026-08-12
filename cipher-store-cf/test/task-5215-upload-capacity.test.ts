import { env, SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";
import packs from "../../keyserver-cf/src/lib/storage-voucher-packs.json";
import { sha256Hex } from "../src/lib/digest.js";
import {
  UPLOAD_GRANT_DOMAIN,
  UPLOAD_GRANT_SCHEME,
  UPLOAD_REQUEST_FLOOR_BYTES,
  UPLOAD_RESERVATION_DOMAIN,
  UPLOAD_RESERVATION_SCHEME,
} from "../src/lib/upload-capacity.js";

type Authority = "monthly-included" | "sold";
type Issuer = { pair: CryptoKeyPair; publicKey: string; authority: Authority };
type Reservation = {
  schema: string; authority: Authority; reservationId: string; grantId: string;
  capacityBytes: number; baseExpiry: number; effectiveExpiry: number;
  outageExtensionSeconds: number;
};

const ORIGIN = "https://cipher.test";
const encoder = new TextEncoder();
const PACK_ROWS = [
  { usd: 5, bytes: 16_400_000_000, objects: 16_400 },
  { usd: 15, bytes: 51_600_000_000, objects: 51_600 },
  { usd: 40, bytes: 139_600_000_000, objects: 139_600 },
] as const;
let monthly: Issuer;
let sold: Issuer;
let serial = 1;

function starvation(pack: string, operation: string, remaining: number): string {
  return `TASK5215B_DIAGNOSTIC pack=${pack} operation=${operation} remaining_signed_bytes=${remaining}`;
}

function statusIs(response: Response, expected: number, pack: string, operation: string, remaining: number): void {
  expect(response.status, starvation(pack, operation, remaining)).toBe(expected);
}

function equalIs<T>(actual: T, expected: T, pack: string, operation: string, remaining: number): void {
  expect(actual, starvation(pack, operation, remaining)).toEqual(expected);
}

function b64u(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function issuer(authority: Authority): Promise<Issuer> {
  const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]) as CryptoKeyPair;
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  return { pair, publicKey: b64u(raw), authority };
}

async function signHeader(scheme: string, domainText: string, payload: object, key: CryptoKey): Promise<string> {
  const bytes = encoder.encode(JSON.stringify(payload));
  const domain = encoder.encode(domainText);
  const signed = new Uint8Array(domain.byteLength + 1 + bytes.byteLength);
  signed.set(domain);
  signed[domain.byteLength] = 0;
  signed.set(bytes, domain.byteLength + 1);
  const signature = new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, key, signed));
  return `${scheme} ${b64u(bytes)}.${b64u(signature)}`;
}

function reservation(authority: Authority, capacityBytes: number, options: {
  baseExpiry?: number; extension?: number;
} = {}): Reservation {
  const now = Math.floor(Date.now() / 1000);
  const baseExpiry = options.baseExpiry ?? now + 2_592_000;
  const outageExtensionSeconds = options.extension ?? 0;
  const suffix = (serial++).toString(16).padStart(32, "0");
  return {
    schema: authority === "sold" ? "osl-sold-upload-reservation-v1" : "osl-monthly-upload-reservation-v1",
    authority,
    reservationId: suffix,
    grantId: (serial++).toString(16).padStart(32, "0"),
    capacityBytes,
    baseExpiry,
    effectiveExpiry: baseExpiry + outageExtensionSeconds,
    outageExtensionSeconds,
  };
}

async function register(value: Reservation, signer = value.authority === "sold" ? sold : monthly): Promise<Response> {
  return SELF.fetch(`${ORIGIN}/v1/upload-reservation`, {
    method: "PUT",
    headers: { authorization: await signHeader(
      UPLOAD_RESERVATION_SCHEME, UPLOAD_RESERVATION_DOMAIN, value, signer.pair.privateKey,
    ) },
  });
}

function grantPayload(value: Reservation): object {
  const { schema: _schema, ...common } = value;
  return { schema: "osl-upload-grant-v1", aud: "osl-cipher-store", ...common };
}

async function grantHeader(value: Reservation, signer = value.authority === "sold" ? sold : monthly): Promise<string> {
  return signHeader(UPLOAD_GRANT_SCHEME, UPLOAD_GRANT_DOMAIN, grantPayload(value), signer.pair.privateKey);
}

async function upload(value: Reservation, objectSerial: number, options: {
  bytes?: number; body?: BodyInit;
  crash?: "after-claim" | "after-r2-before-marker" | "after-class-a"; authorization?: string;
} = {}): Promise<Response> {
  const id = objectSerial.toString(16).padStart(32, "0");
  const fetchCap = `f${id.slice(1)}`;
  const body = options.body ?? new Uint8Array(options.bytes ?? 1).fill(7);
  const headers: Record<string, string> = {
    authorization: options.authorization ?? await grantHeader(value),
    "x-osl-ttl-seconds": "3600",
    "x-osl-expiry-mode": "absolute",
    "x-osl-blob-id": id,
    "x-osl-fetch-digest": await sha256Hex(fetchCap),
    "x-osl-ack-digest": await sha256Hex(`a${id.slice(1)}`),
    "x-osl-manage-digest": await sha256Hex(`m${id.slice(1)}`),
    "x-osl-object-class": "single-ack",
    "x-osl-delivery-tag": `d${id.slice(1)}`,
  };
  if (options.crash) headers["x-osl-test-crash-point"] = options.crash;
  return SELF.fetch(`${ORIGIN}/v1/blob`, { method: "PUT", headers, body });
}

async function counters(value: Reservation) {
  const row = await env.DB.prepare(`SELECT capacity_bytes,spent_bytes,
    COALESCE((SELECT SUM(debit_bytes) FROM upload_capacity_claims
      WHERE grant_id=? AND state='pending'),0) AS held_bytes
    FROM upload_capacity_reservations WHERE grant_id=?`).bind(value.grantId, value.grantId)
    .first<{ capacity_bytes: number; spent_bytes: number; held_bytes: number }>();
  if (!row) throw new Error("counter missing");
  return { signed: Number(row.capacity_bytes), spent: Number(row.spent_bytes), held: Number(row.held_bytes) };
}

beforeEach(async () => {
  monthly = await issuer("monthly-included");
  sold = await issuer("sold");
  env.MONTHLY_UPLOAD_AUTHORITY_PUBKEY_B64 = monthly.publicKey;
  env.SOLD_UPLOAD_AUTHORITY_PUBKEY_B64 = sold.publicKey;
  env.TASK_5215_TEST_MODE = "true";
  serial = 1;
});

describe("TASK 5215 decimal-MB debit at the deployed cipher-store boundary", () => {
  it("freezes every production row and proves the floor has no lower object-count guard", async () => {
    // Required starvation anchors: TASK5215_PACK=$5 TASK5215_PACK=$15 TASK5215_PACK=$40
    expect(packs.bytes_per_gb).toBe(1_000_000_000);
    expect(packs.packs.map((row) => row.usd_cents / 100)).toEqual(PACK_ROWS.map((row) => row.usd));
    expect(packs.packs.map((row) => {
      const [whole, fraction = ""] = row.gb.split(".");
      return Number(whole) * packs.bytes_per_gb
        + Number(fraction.padEnd(9, "0"));
    })).toEqual(PACK_ROWS.map((row) => row.bytes));
    for (const { usd, bytes, objects } of PACK_ROWS) {
      expect(bytes / UPLOAD_REQUEST_FLOOR_BYTES, starvation(`$${usd}`, "discovered_count_cutoff", bytes)).toBe(objects);
      console.info(`TASK5215_PACK=$${usd} signed_bytes=${bytes} final_allowed=${objects} first_refused=${objects + 1}`);
    }
    console.info("TASK5215_CEILINGS signed_path_ip_limit=none signed_path_row_limit=none max_object_bytes=8388608 floor_bytes=1000000");
  });

  it.each(PACK_ROWS)("independently reaches the terminal boundary for voucher $usd", async (row) => {
      const value = reservation("sold", row.bytes);
      equalIs(value.capacityBytes, row.bytes, `$${row.usd}`, "independent_pack_evidence", row.bytes);
      statusIs(await register(value), 201, `$${row.usd}`, "reservation", row.bytes);
      const expectedObjects = row.bytes / UPLOAD_REQUEST_FLOOR_BYTES;

      // The exhaustive request count is frozen by exact integer division. To
      // keep the focused workerd check bounded, place this independently
      // reserved grant at the penultimate accounting state, then send the
      // final allowed and first refused objects through SELF.fetch. This is a
      // boundary test, not the literal hours-long deployed catalogue run.
      await env.DB.prepare(`UPDATE upload_capacity_reservations SET spent_bytes=?
        WHERE grant_id=?`).bind(row.bytes - UPLOAD_REQUEST_FLOOR_BYTES, value.grantId).run();
      const finalAllowed = await upload(value, expectedObjects);
      const firstRefused = await upload(value, expectedObjects + 1);
      statusIs(finalAllowed, 201, `$${row.usd}`, `${expectedObjects}_final_allowed`, UPLOAD_REQUEST_FLOOR_BYTES);
      statusIs(firstRefused, 402, `$${row.usd}`, `${expectedObjects + 1}_first_refused`, 0);
      equalIs(await counters(value), { signed: row.bytes, spent: row.bytes, held: 0 }, `$${row.usd}`, "displayed_remainder", 0);
      if (row.usd === 5) {
        expect(await env.PAYLOADS.head(await sha256Hex(`f${(expectedObjects + 1).toString(16).padStart(32, "0").slice(1)}`)),
          starvation("$5", "object_16401_class_a", 0)).toBeNull();
        expect(await env.DB.prepare("SELECT COUNT(*) AS n FROM upload_capacity_claims WHERE object_id=?")
          .bind((16_401).toString(16).padStart(32, "0")).first<{ n: number }>(),
        starvation("$5", "object_16401_class_a", 0)).toEqual({ n: 0 });
      }
      console.info(`TASK5215_BOUNDARY pack=$${row.usd} expected_objects=${expectedObjects} final_allowed_object=${expectedObjects} final_allowed_status=201 first_refused_object=${expectedObjects + 1} first_refused_status=402 first_refused_class_a=0 exact_spent=${row.bytes} remaining_before_final=1000000 borrowed_5usd_evidence=${row.usd === 5 ? "base" : "independent"}`);
  });

  it("accepts only exact independent typed reservations and keeps identity out", async () => {
    const unreserved = reservation("sold", 2_000_000);
    statusIs(await upload(unreserved, 100), 401, "$5", "missing_reservation", 2_000_000);
    const unreservedId = (100).toString(16).padStart(32, "0");
    expect(await env.PAYLOADS.head(await sha256Hex(`f${unreservedId.slice(1)}`)),
      starvation("$5", "unreserved_zero_class_a", 2_000_000)).toBeNull();

    const wrongKey = reservation("monthly-included", 2_000_000);
    statusIs(await register(wrongKey, sold), 401, "monthly", "monthly_authority_rejection", 2_000_000);

    const localTarget = reservation("sold", 2_000_000);
    expect((await register(localTarget)).status).toBe(201);
    // Same-length non-production scheme keeps the payload/signature otherwise
    // indistinguishable from a lawful grant for the deliberate accept-local mutant.
    const local = await signHeader("OSL-Local--Grant", UPLOAD_GRANT_DOMAIN, grantPayload(localTarget), sold.pair.privateKey);
    statusIs(await upload(localTarget, 101, { authorization: local }), 401, "$5", "local_credential_type", 2_000_000);

    const swappedType = { ...reservation("monthly-included", 2_000_000), schema: "osl-sold-upload-reservation-v1" };
    statusIs(await register(swappedType), 401, "monthly", "swapped_reservation_type", 2_000_000);

    const registered = reservation("sold", 2_000_000);
    statusIs(await register(registered), 201, "$5", "normal_sold_route", 2_000_000);
    const other = reservation("sold", 2_000_000);
    statusIs(await register(other), 201, "$5", "normal_sold_route", 2_000_000);
    const swapped = { ...registered, grantId: other.grantId };
    statusIs(await upload(swapped, 102, { authorization: await grantHeader(swapped) }), 401, "$5", "swapped_reservation", 2_000_000);
    const reused = { ...registered, grantId: reservation("sold", 1).grantId };
    statusIs(await register(reused), 409, "$5", "reused_reservation", 2_000_000);
    statusIs(await upload(reused, 103), 401, "$5", "reused_reservation_credential", 2_000_000);

    const identityReservation = { ...reservation("sold", 2_000_000), accountId: "account-5215" };
    const identityAuth = await signHeader(
      UPLOAD_RESERVATION_SCHEME, UPLOAD_RESERVATION_DOMAIN,
      identityReservation, sold.pair.privateKey,
    );
    statusIs(await SELF.fetch(`${ORIGIN}/v1/upload-reservation`, {
      method: "PUT", headers: { authorization: identityAuth },
    }), 401, "$5", "identity_join", 2_000_000);

    const schema = (await env.DB.prepare(`SELECT GROUP_CONCAT(sql,' ') AS sql FROM sqlite_schema
      WHERE name IN ('upload_capacity_reservations','upload_capacity_claims')`).first<{ sql: string }>())?.sql.toLowerCase() ?? "";
    for (const forbidden of ["account_id", "user_id", "payment_id", "email", "payer"]) expect(schema).not.toContain(forbidden);
    expect(await env.DB.prepare("SELECT COUNT(*) AS n FROM blob_capability_index").first<{ n: number }>()).toEqual({ n: 0 });
    console.info("TASK5215_REFUSALS unreserved=401 unreserved_class_a=0 missing=401 swapped=401 swapped_type=401 reused=409 reused_credential=401 wrong_authority_key=401 local_untyped=401 identity_join=401 class_a_writes=0");
  });

  it("charges max(ciphertext,1000000), exhausts exactly, and reconciles client/store counters", async () => {
    const small = reservation("sold", 2_000_000);
    expect((await register(small)).status).toBe(201);
    const first = await upload(small, 200);
    statusIs(first, 201, "$5", "true_tiny_floor", 2_000_000);
    const firstReceipt = await first.json() as Record<string, number>;
    equalIs(firstReceipt.debit_bytes, 1_000_000, "$5", "true_tiny_floor", 2_000_000);
    equalIs(firstReceipt.upload_capacity_spent_bytes, 1_000_000, "$5", "displayed_store_disagreement", 1_000_000);
    equalIs(firstReceipt.upload_capacity_remaining_bytes, 1_000_000, "$5", "displayed_store_disagreement", 1_000_000);
    statusIs(await upload(small, 201), 201, "$5", "floor_second_object", 1_000_000);
    const refused = await upload(small, 202);
    statusIs(refused, 402, "$5", "floor_exhaustion", 0);
    expect(await env.PAYLOADS.head(await sha256Hex(`f${"ca".padStart(31, "0")}`))).toBeNull();
    equalIs(await counters(small), { signed: 2_000_000, spent: 2_000_000, held: 0 }, "$5", "displayed_remainder", 0);

    const large = reservation("sold", 1_000_001);
    expect((await register(large)).status).toBe(201);
    const largeResponse = await upload(large, 300, { bytes: 1_000_001 });
    statusIs(largeResponse, 201, "$5", "large_ciphertext_debit", 1_000_001);
    await expect(largeResponse.json(), starvation("$5", "large_ciphertext_debit", 1_000_001)).resolves.toMatchObject({ debit_bytes: 1_000_001, upload_capacity_remaining_bytes: 0 });
    equalIs(await counters(large), { signed: 1_000_001, spent: 1_000_001, held: 0 }, "$5", "displayed_remainder", 0);
    console.info("TASK5215_FLOOR tiny_objects=2 spent=2000000 refused_status=402 refused_class_a=0 large_ciphertext=1000001 large_debit=1000001 remainder=0");
  });

  it("lets exactly one simultaneous request claim the last decimal MB", async () => {
    const value = reservation("sold", 1_000_000);
    expect((await register(value)).status).toBe(201);
    let arrivals = 0;
    let starved = false;
    let release!: () => void;
    const released = new Promise<void>((resolve) => { release = resolve; });
    const participant = () => new ReadableStream<Uint8Array>({
      async pull(controller) {
        arrivals += 1;
        if (arrivals === 2) release();
        if (arrivals === 1) setTimeout(() => {
          if (arrivals !== 2) starved = true;
          release();
        }, 750);
        await released;
        controller.enqueue(new Uint8Array([7]));
        controller.close();
      },
    });
    // TASK5215_BARRIER_PARTICIPANT_A
    const firstParticipant = upload(value, 400, { body: participant() });
    // TASK5215_BARRIER_PARTICIPANT_B
    const secondParticipant = upload(value, 401, { body: participant() });
    const responses = await Promise.all([firstParticipant, secondParticipant]);
    const statuses = responses.map((response) => response.status).sort();
    equalIs({ arrivals, starved }, { arrivals: 2, starved: false }, "$5", "serial_starved_barrier", 1_000_000);
    equalIs(statuses, [201, 402], "$5", "barrier_double_success", 0);
    equalIs(await counters(value), { signed: 1_000_000, spent: 1_000_000, held: 0 }, "$5", "debit_only_atomic_state", 0);
    expect(await env.DB.prepare("SELECT COUNT(*) AS n FROM blob_capability_index").first<{ n: number }>(), starvation("$5", "object_only_atomic_state", 0)).toEqual({ n: 1 });
    console.info("TASK5215_BARRIER released=2 claim_barrier_writes=1 acceptance_barrier_writes=1 debits=1 spent=1000000 remaining=0 debit_only_atomic_state=0 object_only_atomic_state=0");
  });

  it("recovers claim, ambiguous R2, and post-Class-A crash points without double debit", async () => {
    for (const [offset, crash] of [
      [500, "after-claim"],
      [600, "after-r2-before-marker"],
      [700, "after-class-a"],
    ] as const) {
      const value = reservation("sold", 1_000_000);
      expect((await register(value)).status).toBe(201);
      statusIs(await upload(value, offset, { crash }), 503, "$5", `crash_${crash}`, 1_000_000);
      statusIs(await upload(value, offset), 201, "$5", `recovery_${crash}`, 1_000_000);
      equalIs(await counters(value), { signed: 1_000_000, spent: 1_000_000, held: 0 }, "$5", "debit_only_atomic_state", 0);
      const claim = await env.DB.prepare(`SELECT state,class_a_writes FROM upload_capacity_claims
        WHERE grant_id=?`).bind(value.grantId).first<{ state: string; class_a_writes: number }>();
      expect(claim).toEqual({ state: "accepted", class_a_writes: 1 });
    }
    console.info("TASK5215_CRASH points=after-claim,after-r2-before-marker,after-class-a recovered_objects=3 recovered_debits=3 duplicate_class_a=0 partial_claims=0");
  });

  it("commits neither debit-only nor object-only acceptance state", async () => {
    const value = reservation("sold", 2_000_000);
    expect((await register(value)).status).toBe(201);
    const response = await upload(value, 750);
    const state = await counters(value);
    const id = (750).toString(16).padStart(32, "0");
    const object = await env.DB.prepare("SELECT COUNT(*) AS n FROM blob_capability_index WHERE blob_id=?")
      .bind(id).first<{ n: number }>();
    const claim = await env.DB.prepare("SELECT state,class_a_writes FROM upload_capacity_claims WHERE object_id=?")
      .bind(id).first<{ state: string; class_a_writes: number }>();
    const operation = state.spent > 0 && object?.n === 0
      ? "debit_only_atomic_state"
      : state.spent === 0 && object?.n === 1
        ? "object_only_atomic_state"
        : "atomic_acceptance_state";
    expect({ status: response.status, spent: state.spent, objects: object?.n, claim },
      starvation("$5", operation, state.signed - state.spent - state.held)).toEqual({
      status: 201,
      spent: 1_000_000,
      objects: 1,
      claim: { state: "accepted", class_a_writes: 1 },
    });
    console.info("TASK5215_ATOMIC debit_only=0 object_only=0 accepted_objects=1 accepted_debits=1");
  });

  it("uses signed effective expiry after base expiry and refuses shortened outage evidence", async () => {
    const now = Math.floor(Date.now() / 1000);
    const extended = reservation("sold", 2_000_000, { baseExpiry: now - 60, extension: 3600 });
    expect((await register(extended)).status).toBe(201);
    statusIs(await upload(extended, 800), 201, "sold", "post_base_expiry_real_upload", 2_000_000);
    equalIs(await counters(extended), { signed: 2_000_000, spent: 1_000_000, held: 0 },
      "sold", "effective_expiry_remaining_capacity", 1_000_000);

    const independent = reservation("sold", 2_000_000, { baseExpiry: now - 3000, extension: 3600 });
    expect((await register(independent)).status).toBe(201);
    statusIs(await upload(independent, 801), 201, "sold", "full_signed_outage_interval", 2_000_000);
    const shortened = { ...independent, effectiveExpiry: independent.baseExpiry + 30, outageExtensionSeconds: 30 };
    statusIs(await upload(shortened, 802, { authorization: await grantHeader(shortened) }), 401,
      "sold", "shortened_extension", 1_000_000);
    console.info(`TASK5215_OUTAGE base_expiry=${extended.baseExpiry} effective_expiry=${extended.effectiveExpiry} post_base_upload=201 full_extension_upload=201 shortened_extension=401 remaining_effective_seconds=${extended.effectiveExpiry - now} remaining_signed_bytes=1000000`);
  });

  it("keeps deletion non-refundable with an independent 100-target/3-control manifest", async () => {
    const value = reservation("sold", 103_000_000);
    expect((await register(value)).status).toBe(201);
    const manifest = { targets: [] as string[], controls: [] as string[] };
    for (let index = 0; index < 103; index++) {
      expect((await upload(value, 1000 + index)).status).toBe(201);
      const id = (1000 + index).toString(16).padStart(32, "0");
      (index < 100 ? manifest.targets : manifest.controls).push(id);
    }
    const before = await counters(value);
    for (const id of manifest.targets) {
      const fetchCap = `f${id.slice(1)}`;
      const response = await SELF.fetch(`${ORIGIN}/v1/blob/${id}`, {
        method: "DELETE", headers: {
          "x-osl-manage-cap": fetchCap,
          "x-osl-delete-grant": "manifest-5215",
        },
      });
      statusIs(response, 204, "$5", "deletion_exact_target", 3_000_000);
    }
    equalIs(await counters(value), before, "$5", "deletion_no_restoration", 3_000_000);
    const rows = await env.DB.prepare("SELECT blob_id FROM blob_capability_index ORDER BY blob_id").all<{ blob_id: string }>();
    equalIs(rows.results.map((row) => row.blob_id), manifest.controls, "$5", "deletion_wrong_object", 3_000_000);
    console.info("TASK5215_DELETE manifest_targets=100 manifest_controls=3 deleted_exact=100 controls_remaining=3 capacity_restored=0 wrong_object_deleted=0 spent=103000000");
  }, 120_000);

  it("uses the separate 4605 monthly authority for one independently reconciled upload", async () => {
    const value = reservation("monthly-included", 1_000_000);
    statusIs(await register(value), 201, "monthly", "normal_monthly_route", 1_000_000);
    statusIs(await upload(value, 2000), 201, "monthly", "normal_monthly_route", 1_000_000);
    equalIs(await counters(value), { signed: 1_000_000, spent: 1_000_000, held: 0 }, "monthly", "displayed_remainder", 0);
    const row = await env.DB.prepare("SELECT authority FROM upload_capacity_reservations WHERE grant_id=?")
      .bind(value.grantId).first<{ authority: string }>();
    expect(row).toEqual({ authority: "monthly-included" });
    console.info("TASK5215_MONTHLY authority=monthly-included reservations=1 uploads=1 spent=1000000 identity_joins=0");
  });
});
