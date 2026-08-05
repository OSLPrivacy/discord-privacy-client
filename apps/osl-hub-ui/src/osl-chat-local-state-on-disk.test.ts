// D-108 — the byte-level proof, against a real file, not a type.
//
// The claim under test is not "we called the encrypting store". It is: after
// the app runs, does the correspondent identifier appear in the bytes that
// reached disk? So every assertion here reads the file back with
// `readFileSync` and searches raw bytes, and every absence assertion is paired
// with a positive control that runs the SAME scanner with the SAME needle over
// the SAME file at a moment when the string IS present. An absence assertion
// whose search is silently broken proves nothing at all.
//
// What is faithful and what is substituted, stated plainly:
//   - faithful: the production path. `loadUiPreferences()` and
//     `persistOslChatUnread()` are the shipping functions, and the store is
//     built by the shipping `ensureOslChatSecureLocalStore()` from a key that
//     arrives over the real command name.
//   - substituted: the container. WebKit persists the webview's localStorage
//     to `~/.local/share/org.oslprivacy.hub/localstorage/tauri_localhost_0.localstorage`
//     (a SQLite `ItemTable` holding keys and UTF-16LE values); this test uses a
//     file-backed `Storage` that flushes every mutation to a real file. The
//     container cannot make a substring present or absent, which is exactly why
//     the assertion is byte-level, and both UTF-8 and UTF-16LE encodings of the
//     needle are scanned so the real file's encoding is covered too.

import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";


// D-251: every `it()` below deliberately re-loads `./main` with its own
// selectors / storage / stubs, so the import CANNOT be hoisted into a single
// `beforeAll` without destroying what the tests check. `src/main.ts` is ~10k
// lines and one load costs ~2.5 s cold, which left almost nothing of vitest's
// default 5,000 ms budget for the behaviour under test: on a busy machine these
// tests died with `Test timed out in 5000ms` before reaching an assertion.
// The budget below covers MODULE LOADING, not the behaviour -- no assertion
// depends on it, and every assertion is unchanged.
const MODULE_RELOAD_BUDGET_MS = 30_000;

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// A correspondent identifier of the shape OSL Chat actually keys on: the peer's
// OSL user id. This is the string that must not survive to disk.
const CORRESPONDENT = "osl-user-7f2ac41d9b5e4c08";
const SECOND_CORRESPONDENT = "osl-user-a19bd3e6c04f7285";

const previewKey = "osl-chat-previews-visible-v1";
const mutedKey = "osl-chat-muted-people-v1";
const unreadKey = "osl-chat-unread-v1";
const notificationKey = "osl-chat-notifications-v1";

/// A `Storage` whose every mutation lands in a real file, so the test can read
/// the persisted bytes rather than an in-memory map. `flush` writes the whole
/// snapshot the way WebKit rewrites its localStorage file.
class FileBackedStorage implements Storage {
  readonly values = new Map<string, string>();

  constructor(readonly path: string) {
    this.flush();
  }

  private flush(): void {
    const records: string[] = [];
    for (const [key, value] of this.values) records.push(`${key}\u0000${value}`);
    writeFileSync(this.path, records.join(""), "utf8");
  }

  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); this.flush(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); this.flush(); }
  removeItem(key: string): void { this.values.delete(key); this.flush(); }
}

interface ByteScan {
  readonly hit: boolean;
  readonly encoding: "utf8" | "utf16le" | null;
  readonly offset: number;
  readonly excerpt: string;
}

/// THE scanner. One implementation, used for both the absence assertions and
/// their positive controls, so a broken needle fails the control first.
function scanPersistedBytes(path: string, needle: string): ByteScan {
  const bytes = readFileSync(path);
  for (const encoding of ["utf8", "utf16le"] as const) {
    const offset = bytes.indexOf(Buffer.from(needle, encoding));
    if (offset >= 0) {
      return {
        hit: true,
        encoding,
        offset,
        excerpt: bytes.subarray(Math.max(0, offset - 24), offset + needle.length + 24).toString("latin1"),
      };
    }
  }
  return { hit: false, encoding: null, offset: -1, excerpt: "" };
}

function installGlobals(storage: Storage): void {
  vi.stubGlobal("localStorage", storage);
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
  });
}

/// The key the native `get_osl_chat_local_state_key` command returns: 32 bytes,
/// base64url, no padding — the same encoding `URL_SAFE_NO_PAD.encode` produces.
const localStateKeyB64 = Buffer.from(new Uint8Array(32).fill(0x5a)).toString("base64url");

function stubKeyCommand(result: string | Error = localStateKeyB64): void {
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command !== "get_osl_chat_local_state_key") return undefined;
    if (result instanceof Error) throw result;
    return result;
  });
}

/// The physical localStorage key SecureLocalStore writes a logical key under:
/// `<namespace>:<base64url(utf8(logicalKey))>` — see `#storageKey`.
function sealedStorageKeyFor(logicalKey: string): string {
  return `osl-secure-local-store-v1:${Buffer.from(logicalKey, "utf8").toString("base64url")}`;
}

function newStorageFile(): string {
  return join(mkdtempSync(join(tmpdir(), "osl-chat-local-state-")), "tauri_localhost_0.localstorage");
}

/// Exactly what the pre-fix build wrote: plaintext correspondent identifiers,
/// through `localStorage.setItem`. Verified against 08552e5d0's main.ts, which
/// wrote all four of these keys in the clear.
function seedLegacyPlaintextProfile(storage: Storage): void {
  storage.setItem(mutedKey, JSON.stringify([CORRESPONDENT]));
  storage.setItem(unreadKey, JSON.stringify({ [CORRESPONDENT]: 3, [SECOND_CORRESPONDENT]: 1 }));
  storage.setItem(previewKey, "false");
  storage.setItem(notificationKey, JSON.stringify([{
    id: `notice-${CORRESPONDENT}`,
    title: `New message from ${CORRESPONDENT}`,
    detail: "New encrypted message",
    createdAt: "2026-08-03T22:34:00.000Z",
  }]));
}

beforeEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  mocks.invoke.mockReset();
});

describe("OSL Chat local state on disk", () => {
  it("removes the correspondent identifier from the persisted bytes of a pre-existing plaintext profile", async () => {
    const path = newStorageFile();
    const storage = new FileBackedStorage(path);
    installGlobals(storage);
    seedLegacyPlaintextProfile(storage);

    // POSITIVE CONTROL, same scanner, same needle, same file. If this does not
    // report a hit the absence assertion below is meaningless and the test must
    // fail here rather than pass silently.
    const before = scanPersistedBytes(path, CORRESPONDENT);
    expect(before.hit, "positive control: the scanner must find the correspondent while it IS on disk").toBe(true);
    expect(before.offset).toBeGreaterThanOrEqual(0);

    stubKeyCommand();
    const main = await import("./main");
    await main.loadUiPreferences();

    const after = scanPersistedBytes(path, CORRESPONDENT);
    expect(after.hit, `correspondent still on disk at offset ${after.offset}: ${after.excerpt}`).toBe(false);
    expect(scanPersistedBytes(path, SECOND_CORRESPONDENT).hit).toBe(false);

    // The plaintext keys are gone, not merely shadowed by an encrypted copy.
    expect(storage.getItem(mutedKey)).toBeNull();
    expect(storage.getItem(unreadKey)).toBeNull();
    expect(storage.getItem(previewKey)).toBeNull();
    expect(storage.getItem(notificationKey)).toBeNull();

    // must_not_change: the app still has its own correspondents in memory.
    const snapshot = main.oslChatUiPreferenceSnapshot();
    expect(snapshot.mutedPeople).toEqual([CORRESPONDENT]);
    expect(snapshot.unread).toEqual([[CORRESPONDENT, 3], [SECOND_CORRESPONDENT, 1]]);
    expect(snapshot.previewsVisible).toBe(false);
  }, MODULE_RELOAD_BUDGET_MS);

  it("keeps a live write of the correspondent out of the bytes and still reads it back after a restart", async () => {
    const path = newStorageFile();
    const storage = new FileBackedStorage(path);
    installGlobals(storage);
    seedLegacyPlaintextProfile(storage);
    stubKeyCommand();

    const main = await import("./main");
    await main.loadUiPreferences();

    const sealedUnreadKey = sealedStorageKeyFor(unreadKey);
    const beforeWrite = storage.getItem(sealedUnreadKey);
    expect(beforeWrite, "the migration must have produced a sealed unread record").toBeTruthy();

    // The shipping writer, not a test double. Every SecureLocalStore.setItem
    // draws a fresh nonce, so a real write changes the stored bytes; a no-op
    // (the pre-fix behaviour, `if (!oslChatSecureStore) return`) does not.
    await main.persistOslChatUnread();
    expect(storage.getItem(sealedUnreadKey), "persistOslChatUnread must actually write, not return early")
      .not.toBe(beforeWrite);

    const after = scanPersistedBytes(path, CORRESPONDENT);
    expect(after.hit, `correspondent leaked at offset ${after.offset}: ${after.excerpt}`).toBe(false);

    // POSITIVE CONTROL for this file, at this moment: write the same identifier
    // in the clear the way the pre-fix build did, and the same scanner must
    // report a hit — then remove it and the hit must go away.
    storage.setItem("osl-chat-positive-control", CORRESPONDENT);
    const control = scanPersistedBytes(path, CORRESPONDENT);
    expect(control.hit, "positive control: the scanner must find the correspondent when it IS present").toBe(true);
    storage.removeItem("osl-chat-positive-control");
    expect(scanPersistedBytes(path, CORRESPONDENT).hit).toBe(false);

    // must_not_change: restart — fresh module, same file, no legacy plaintext
    // left to fall back to — and the correspondents come back.
    vi.resetModules();
    installGlobals(storage);
    stubKeyCommand();
    const restarted = await import("./main");
    await restarted.loadUiPreferences();
    expect(restarted.oslChatUiPreferenceSnapshot().unread)
      .toEqual([[CORRESPONDENT, 3], [SECOND_CORRESPONDENT, 1]]);
    expect(restarted.oslChatUiPreferenceSnapshot().mutedPeople).toEqual([CORRESPONDENT]);
  }, MODULE_RELOAD_BUDGET_MS);

  it("persists nothing rather than plaintext when the native side refuses the key", async () => {
    const path = newStorageFile();
    const storage = new FileBackedStorage(path);
    installGlobals(storage);
    stubKeyCommand(new Error("OSL: no storage-key authority for OSL Chat local state"));

    const main = await import("./main");
    await main.loadUiPreferences();
    await main.persistOslChatUnread();

    expect(scanPersistedBytes(path, CORRESPONDENT).hit).toBe(false);
    expect(storage.getItem(unreadKey), "a refused key must not fall back to plaintext").toBeNull();
    expect([...storage.values.keys()].filter((key) => key.startsWith("osl-secure-local-store-v1:"))).toEqual([]);
  }, MODULE_RELOAD_BUDGET_MS);
});
