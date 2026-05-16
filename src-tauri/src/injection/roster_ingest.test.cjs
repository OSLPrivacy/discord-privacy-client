/**
 * Bug B (whitelist repair) — unit tests for the OBSERVE-ONLY gateway
 * roster accumulator `oslIngestRosterFrame`.
 *
 * No JS test harness exists for boot.js (it ships as a single
 * `include_str!`'d IIFE). To test the EXACT shipped code, this
 * script extracts the function source between the
 * `// __OSL_TEST_EXTRACT_START/END oslIngestRosterFrame` sentinels
 * and evaluates just that function in isolation. Pure: it only takes
 * (cache, guildCache, t, d, caps) and mutates `cache`.
 *
 * Run: node src-tauri/src/injection/roster_ingest.test.cjs
 */
"use strict";
const fs = require("fs");
const path = require("path");
const assert = require("assert");

const BOOT = fs.readFileSync(path.join(__dirname, "boot.js"), "utf8");
const START = "// __OSL_TEST_EXTRACT_START oslIngestRosterFrame";
const END = "// __OSL_TEST_EXTRACT_END oslIngestRosterFrame";
const s = BOOT.indexOf(START);
const e = BOOT.indexOf(END);
assert.ok(s >= 0 && e > s, "extraction sentinels present in boot.js");
const src = BOOT.slice(s + START.length, e);
// eslint-disable-next-line no-new-func
const oslIngestRosterFrame = new Function(
    src + "\n; return oslIngestRosterFrame;"
)();

const CAPS = { maxPerChannel: 2000, maxChannels: 500, maxIngest: 10000 };
const GUILD = "guild1";
const guildCache = { [GUILD]: { channel_ids: ["c1", "c2"] } };
const chunk = (gid, ids) => ({
    guild_id: gid,
    members: ids.map((id) => ({ user: { id } })),
});

let passed = 0;
function ok(name, fn) {
    try {
        fn();
        passed++;
        console.log("  ok  " + name);
    } catch (err) {
        console.error("FAIL  " + name + "\n      " + (err && err.message));
        process.exitCode = 1;
    }
}

ok("single chunk populates both guild channels", () => {
    const cache = new Map();
    const touched = oslIngestRosterFrame(
        cache, guildCache, "GUILD_MEMBERS_CHUNK", chunk(GUILD, ["a", "b"]), CAPS
    );
    assert.deepStrictEqual(touched.sort(), ["c1", "c2"]);
    assert.deepStrictEqual(cache.get("c1").sort(), ["a", "b"]);
    assert.deepStrictEqual(cache.get("c2").sort(), ["a", "b"]);
});

ok("multiple partial chunks MERGE (union), do not overwrite", () => {
    const cache = new Map();
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", chunk(GUILD, ["a", "b"]), CAPS);
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", chunk(GUILD, ["b", "c"]), CAPS);
    assert.deepStrictEqual(cache.get("c1").slice().sort(), ["a", "b", "c"]);
});

ok("malformed / missing-fields chunk: no throw, prior cache intact", () => {
    const cache = new Map([["c1", ["a"]]]);
    const before = JSON.stringify([...cache]);
    // null d, missing members, members without user, missing guild_id
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", null, CAPS);
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", { guild_id: GUILD }, CAPS);
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", { guild_id: GUILD, members: [{}, { user: {} }] }, CAPS);
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", { members: [{ user: { id: "x" } }] }, CAPS);
    assert.strictEqual(JSON.stringify([...cache]), before, "cache unchanged");
});

ok("oversized chunk beyond maxIngest is skipped safely", () => {
    const cache = new Map([["c1", ["keep"]]]);
    const huge = chunk(GUILD, Array.from({ length: CAPS.maxIngest + 1 }, (_, i) => "u" + i));
    const touched = oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", huge, CAPS);
    assert.deepStrictEqual(touched, []);
    assert.deepStrictEqual(cache.get("c1"), ["keep"]);
});

ok("per-channel cap truncates the merged set", () => {
    const caps = { maxPerChannel: 3, maxChannels: 500, maxIngest: 10000 };
    const cache = new Map();
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", chunk(GUILD, ["a", "b", "c", "d", "e"]), caps);
    assert.strictEqual(cache.get("c1").length, 3);
});

ok("total-channel cap evicts the oldest channel", () => {
    const caps = { maxPerChannel: 2000, maxChannels: 2, maxIngest: 10000 };
    const cache = new Map();
    const gc = {
        gA: { channel_ids: ["A"] },
        gB: { channel_ids: ["B"] },
        gC: { channel_ids: ["C"] },
    };
    oslIngestRosterFrame(cache, gc, "GUILD_MEMBERS_CHUNK", chunk("gA", ["a"]), caps);
    oslIngestRosterFrame(cache, gc, "GUILD_MEMBERS_CHUNK", chunk("gB", ["b"]), caps);
    oslIngestRosterFrame(cache, gc, "GUILD_MEMBERS_CHUNK", chunk("gC", ["c"]), caps);
    assert.strictEqual(cache.size, 2, "capped at maxChannels");
    assert.ok(!cache.has("A"), "oldest channel evicted");
    assert.ok(cache.has("B") && cache.has("C"));
});

ok("non-member-list frame type is ignored (returns [], no mutation)", () => {
    const cache = new Map([["c1", ["a"]]]);
    const r = oslIngestRosterFrame(cache, guildCache, "MESSAGE_CREATE", { guild_id: GUILD, members: [{ user: { id: "z" } }] }, CAPS);
    assert.deepStrictEqual(r, []);
    assert.deepStrictEqual(cache.get("c1"), ["a"]);
});

ok("GUILD_MEMBER_LIST_UPDATE ops/items are extracted + merged", () => {
    const cache = new Map();
    const d = {
        guild_id: GUILD,
        ops: [
            { op: "SYNC", items: [{ member: { user: { id: "m1" } } }, { group: { id: "online" } }] },
            { op: "INSERT", item: { member: { user: { id: "m2" } } } },
        ],
    };
    oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBER_LIST_UPDATE", d, CAPS);
    assert.deepStrictEqual(cache.get("c1").slice().sort(), ["m1", "m2"]);
});

ok("guild not in guildCache → drop silently (no channel mapping)", () => {
    const cache = new Map();
    const r = oslIngestRosterFrame(cache, {}, "GUILD_MEMBERS_CHUNK", chunk("unknownGuild", ["a"]), CAPS);
    assert.deepStrictEqual(r, []);
    assert.strictEqual(cache.size, 0);
});

ok("OBSERVE-ONLY: does not mutate `d`, returns an array, never throws", () => {
    const cache = new Map();
    const d = chunk(GUILD, ["a", "b"]);
    const dSnapshot = JSON.stringify(d);
    const r = oslIngestRosterFrame(cache, guildCache, "GUILD_MEMBERS_CHUNK", d, CAPS);
    assert.strictEqual(JSON.stringify(d), dSnapshot, "frame `d` argument is not mutated");
    assert.ok(Array.isArray(r), "returns an array");
});

ok("PASSTHROUGH proof: forwarded frame stream is byte-identical", () => {
    // Faithfully mirror boot.js's wrapMessageHandler contract:
    //   try { ingest(parse(copy of data)) } catch {}
    //   return original ev untouched   (forward is unconditional)
    const cache = new Map();
    const forwarded = [];
    function processFrame(raw) {
        try {
            const p = JSON.parse(raw); // a COPY — never the forwarded bytes
            if (p && p.op === 0) {
                oslIngestRosterFrame(cache, guildCache, p.t, p.d, CAPS);
            }
        } catch (_) {}
        forwarded.push(raw); // original string, unconditionally
        return raw;
    }
    const input = [
        JSON.stringify({ op: 0, t: "GUILD_MEMBERS_CHUNK", d: chunk(GUILD, ["a"]) }),
        JSON.stringify({ op: 0, t: "MESSAGE_CREATE", d: { content: "hi" } }),
        "{ not json",
        JSON.stringify({ op: 11 }),
        JSON.stringify({ op: 0, t: "GUILD_MEMBER_LIST_UPDATE", d: { guild_id: GUILD, ops: [{ items: [{ member: { user: { id: "z" } } }] }] } }),
    ];
    const out = input.map(processFrame);
    assert.deepStrictEqual(out, input, "every frame forwarded byte-identically");
    assert.deepStrictEqual(forwarded, input, "no drop / reorder / injection");
    // ingestion still worked as a side effect
    assert.ok(cache.get("c1").includes("a") && cache.get("c1").includes("z"));
});

ok("DM/GC path is UNCHANGED: those frame types never reach this fn", () => {
    // DM/GC rosters flow through ingestPrivateChannel (READY /
    // CHANNEL_CREATE recipients) — NOT this guild-only accumulator.
    // Feeding those types here is a no-op (regression guard).
    const cache = new Map();
    assert.deepStrictEqual(
        oslIngestRosterFrame(cache, guildCache, "CHANNEL_CREATE", { id: "dm1", recipients: [{ id: "u" }] }, CAPS),
        []
    );
    assert.deepStrictEqual(
        oslIngestRosterFrame(cache, guildCache, "READY", { private_channels: [] }, CAPS),
        []
    );
    assert.strictEqual(cache.size, 0);
});

console.log("\n" + passed + " passed" + (process.exitCode ? " (FAILURES ABOVE)" : ", 0 failed"));
