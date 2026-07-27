import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const GENERIC_OVERLAY_SYMBOLS = [
  "ComposerOverlayGuard",
  "DecryptionOverlayGuard",
  "VisiblePlaintextCache",
  "EncryptedLocalOverlayCacheRecord",
] as const;

const FALSE_SHIPPING_CLAIMS = [
  /\bThe external overlay is enabled\b/iu,
  /\bOSL overlays fail closed on every (?:focus|geometry|context)[^.\n]{0,120}\bchange\b/iu,
  /\bMoving, resizing, minimizing,[^.\n]{0,180}\bhides it immediately\b/iu,
  /\bOnly visible messages[^.\n]{0,180}\bplaintext in RAM\b/iu,
  /\bFree is capped at 250 messages[^.\n]{0,120}\bPro at 1[,.]?000\b/iu,
  /\bVisible plaintext is limited to 250\s*\/\s*1[,.]?000 messages\b/iu,
  /\bPlaintext is zeroized on [^.\n]{0,120}\bcontext loss\b[^.\n]{0,80}\bapp switch\b/iu,
  /\bThe shipping overlay uses VisiblePlaintextCache\b/iu,
  /\bOverlay history is persisted in an encrypted local cache\b/iu,
] as const;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function sourceFiles(directory: URL, suffix: string): Array<{ url: URL; source: string }> {
  const files: Array<{ url: URL; source: string }> = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const url = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, directory);
    if (entry.isDirectory()) {
      files.push(...sourceFiles(url, suffix));
    } else if (entry.name.endsWith(suffix)) {
      files.push({ url, source: readFileSync(url, "utf8") });
    }
  }
  return files;
}

function hasLifecycleCacheClearEdge(source: string): boolean {
  const namesTheLifecycle =
    /\b(?:context_(?:lost|changed)|context loss|app_switch|app switch|switch_app)\b/iu.test(source);
  const constructsCache = /\bVisiblePlaintextCache\b/u.test(source);
  const clearsCache = /\b(?:cache|plaintext_cache|visible_cache)\s*\.\s*lock\s*\(\s*\)/u.test(
    source,
  );
  return namesTheLifecycle && constructsCache && clearsCache;
}

describe("generic external overlay production reachability", () => {
  it("keeps implemented prototype facts separate from shipping guarantees", () => {
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const externalOverlay = readFileSync(
      new URL("../../osl-hub/src/external_overlay.rs", import.meta.url),
      "utf8",
    );
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const nativeUi = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
    const nativeAdapter = readFileSync(
      new URL("./native-overlay-adapter.ts", import.meta.url),
      "utf8",
    );
    const contract = readFileSync(
      new URL("../../../docs/design/external-overlay-security-contract.md", import.meta.url),
      "utf8",
    );
    const checklist = readFileSync(
      new URL("../../../docs/design/osl-internal-build-checklist.md", import.meta.url),
      "utf8",
    );
    const readme = readFileSync(new URL("../../../README.md", import.meta.url), "utf8");
    const handler = sourceBetween(rustMain, "tauri::generate_handler![", "\n    ]);");
    const rustConsumers = sourceFiles(
      new URL("../../osl-hub/src/", import.meta.url),
      ".rs",
    ).filter(({ url }) => !url.pathname.endsWith("/external_overlay.rs"));
    const productionUi = sourceFiles(new URL("./", import.meta.url), ".ts").filter(
      ({ url }) => !url.pathname.endsWith(".test.ts"),
    );
    const rustConsumerCorpus = rustConsumers.map(({ source }) => source).join("\n");
    const productionUiCorpus = productionUi.map(({ source }) => source).join("\n");
    const genericOverlayReference = new RegExp(
      `\\b(?:external_overlay|${GENERIC_OVERLAY_SYMBOLS.join("|")})\\b`,
      "u",
    );
    const tauriCommandBlocks = [rustMain, externalOverlay].flatMap((source) =>
      source
        .split(/(?=#\[tauri::command\])/u)
        .filter((block) => block.startsWith("#[tauri::command]"))
    );
    const genericOverlayHandlerNames = tauriCommandBlocks.flatMap((block) => {
      if (!genericOverlayReference.test(block)) return [];
      const name = block.match(
        /^#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(/u,
      )?.[1];
      return name === undefined ? [] : [name];
    });

    // Positive controls: the prototype has real refusal and bounded-memory behavior.
    const composerGuard = sourceBetween(
      externalOverlay,
      "impl ComposerOverlayGuard {",
      "\n}\n\n#[derive(Debug, Clone, Eq, PartialEq)]\npub struct EncryptedCarrierBinding",
    );
    const decryptionGuard = sourceBetween(
      externalOverlay,
      "impl DecryptionOverlayGuard {",
      "\n}\n\n#[derive(Debug, Clone, Copy, Eq, PartialEq)]\npub enum OverlayCacheTier",
    );
    const visibleCache = sourceBetween(
      externalOverlay,
      "impl VisiblePlaintextCache {",
      "\n}\n\nimpl Drop for VisiblePlaintextCache",
    );
    const cacheDrop = sourceBetween(
      externalOverlay,
      "impl Drop for VisiblePlaintextCache {",
      "\n}\n\n/// Format contract for a possible future SSD cache.",
    );

    expect(composerGuard).toContain("OverlayHiddenReason::ContextChanged");
    expect(composerGuard).toContain("OverlayHiddenReason::WindowMovedOrResized");
    expect(composerGuard).toContain("OverlayHiddenReason::WindowNotForeground");
    expect(composerGuard).toContain("OverlayHiddenReason::GeometryUncertain");
    expect(composerGuard).toContain("OverlayHiddenReason::PasswordOrLoginField");
    expect(decryptionGuard).toContain("self.visible.clear();");
    expect(externalOverlay).toContain("plaintext: Zeroizing<String>");
    expect(externalOverlay).toContain("max_messages: 250");
    expect(externalOverlay).toContain("max_messages: 1_000");
    expect(externalOverlay).toContain("ttl: Duration::from_secs(30 * 60)");
    expect(externalOverlay).toContain("ttl: Duration::from_secs(2 * 60 * 60)");
    expect(visibleCache).toContain("self.evict_expired(now_ms);");
    expect(visibleCache).toContain("self.entries.clear();");
    expect(cacheDrop).toContain("self.lock();");
    expect(externalOverlay).toContain(
      "Format contract for a possible future SSD cache. It is not wired to storage",
    );

    // The module is compiled, but every independent production-reachability leg is absent.
    const moduleDeclared = /\bpub mod external_overlay\s*;/u.test(rustLib);
    const anyRustConsumer = GENERIC_OVERLAY_SYMBOLS.some((symbol) =>
      new RegExp(`\\b${symbol}\\b`, "u").test(rustConsumerCorpus)
    );
    const tauriHandlerDefined = genericOverlayHandlerNames.length > 0;
    const anyHandlerRegistered = genericOverlayHandlerNames.some((name) =>
      new RegExp(`\\b${name}\\b`, "u").test(handler)
    );
    const uiAdapterImported = /from\s+["'][^"']*external-overlay[^"']*["']/u.test(
      productionUiCorpus,
    );
    const anyUiCall =
      GENERIC_OVERLAY_SYMBOLS.some((symbol) =>
        new RegExp(`\\b${symbol}\\s*\\(`, "u").test(productionUiCorpus)
      )
      || /\b(?:open|show|create|mount|enable|observe|render|prepare)ExternalOverlay[A-Za-z0-9_]*\s*\(/u
        .test(productionUiCorpus);
    const lifecycleCacheClearEdge = rustConsumers.some(({ source }) =>
      hasLifecycleCacheClearEdge(source)
    );
    const productionReachable =
      moduleDeclared
      && anyRustConsumer
      && tauriHandlerDefined
      && anyHandlerRegistered
      && uiAdapterImported
      && anyUiCall;

    expect(moduleDeclared).toBe(true);
    expect(anyRustConsumer).toBe(false);
    expect(tauriHandlerDefined).toBe(false);
    expect(anyHandlerRegistered).toBe(false);
    expect(uiAdapterImported).toBe(false);
    expect(anyUiCall).toBe(false);
    expect(lifecycleCacheClearEdge).toBe(false);
    expect(productionReachable).toBe(false);

    // Positive controls: the separately implemented native Discord overlay is shipping-wired.
    expect(rustMain).toContain("mod native_discord_overlay;");
    expect(rustMain).toMatch(/\bfn get_native_discord_overlay_state\s*\(/u);
    expect(rustMain).toMatch(/\basync fn prepare_native_discord_overlay_text\s*\(/u);
    expect(handler).toMatch(/\bget_native_discord_overlay_state\b/u);
    expect(handler).toMatch(/\bprepare_native_discord_overlay_text\b/u);
    expect(handler).toMatch(/\bopen_native_discord_overlay_text\b/u);
    expect(nativeUi).toContain('from "./native-overlay-adapter"');
    expect(nativeUi).toMatch(/\bprepareNativeDiscordOverlayText\s*\(/u);
    expect(nativeUi).toMatch(/\bopenNativeDiscordOverlayText\s*\(/u);
    expect(nativeAdapter).toContain('invoke<unknown>("prepare_native_discord_overlay_text"');
    expect(nativeAdapter).toContain('invoke<unknown>("open_native_discord_overlay_text"');
    expect(uiMain).toContain('from "./native-overlay-adapter"');

    if (!productionReachable) {
      const guardedClaims = `${readme}\n${contract}\n${productionUiCorpus}`;
      for (const claim of FALSE_SHIPPING_CLAIMS) expect(guardedClaims).not.toMatch(claim);
    }
    expect(contract).toContain("compiled but uncalled source prototype");
    expect(contract).toContain("Those are prototype state-machine properties, not shipping behavior.");
    expect(contract).toContain("No production lifecycle constructs or calls this cache.");
    expect(contract).toMatch(
      /Context loss\s+and app switching do not call its `lock\(\)`/u,
    );
    expect(contract).toMatch(
      /These are not shipping\s+zeroization or retention guarantees\./u,
    );
    expect(contract).toMatch(/type is only a \*\*format contract\*\* today/u);
    expect(checklist).toContain(
      "C6 · Window/composer lifecycle** — adoption, drag, minimize, focus, close, first launch,",
    );
    expect(checklist).toContain(
      "D5 · Timed deletion** — scheduler/ledger pieces exist; production wiring and all lifecycle",
    );

    // Mutation controls prove both the lifecycle detector and copy gate can turn positive.
    expect(
      hasLifecycleCacheClearEdge(
        "fn context_changed(cache: &mut VisiblePlaintextCache) { cache.lock(); }",
      ),
    ).toBe(true);
    expect(hasLifecycleCacheClearEdge("fn context_changed() { hide_overlay(); }")).toBe(false);
    for (const mutation of [
      "The external overlay is enabled.",
      "OSL overlays fail closed on every focus, geometry or context change.",
      "Moving, resizing, minimizing, changing focus, changing account or chat, or losing geometry certainty hides it immediately.",
      "Only visible messages and a small adjacent scroll buffer may exist as plaintext in RAM.",
      "Free is capped at 250 messages / 4 MiB / 30 minutes; Pro at 1,000 messages.",
      "Visible plaintext is limited to 250/1,000 messages.",
      "Plaintext is zeroized on lock, context loss and app switch.",
      "Plaintext is zeroized on eviction, lock, context loss, app switch, and drop.",
      "The shipping overlay uses VisiblePlaintextCache.",
      "Overlay history is persisted in an encrypted local cache.",
    ]) {
      expect(
        FALSE_SHIPPING_CLAIMS.some((claim) => claim.test(mutation)),
        `false shipping claim escaped the mutation gate: ${mutation}`,
      ).toBe(true);
    }
    for (const prototypeFact of [
      "Within the prototype, VisiblePlaintextCache has bounded entries.",
      "A future adapter may implement the external-overlay design.",
      "The shipping desktop uses a separate native overlay.",
    ]) {
      expect(
        FALSE_SHIPPING_CLAIMS.some((claim) => claim.test(prototypeFact)),
        `implemented-but-unwired fact was rejected: ${prototypeFact}`,
      ).toBe(false);
    }
  });
});
