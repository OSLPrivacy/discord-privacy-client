import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type SourceFile = { path: string; source: string };

type ProductionPath = {
  moduleDeclared: boolean;
  rustConsumer: boolean;
  tauriHandlerDefined: boolean;
  handlerRegistered: boolean;
  uiInvokePresent: boolean;
  uiAdapterImported: boolean;
  uiCall: boolean;
  lifecycleCacheClearEdge: boolean;
  productionReachable: boolean;
};

const EXPECTED_PUBLIC_PROTOTYPE_TYPES = [
  "ScreenRect",
  "ExternalContextBinding",
  "VerifiedFieldKind",
  "ComposerCalibration",
  "WindowObservation",
  "OverlayHiddenReason",
  "ComposerOverlayDecision",
  "ComposerOverlayGuard",
  "EncryptedCarrierBinding",
  "DecryptedHitTarget",
  "DecryptionOverlayGuard",
  "OverlayCacheTier",
  "VisibleCacheLimits",
  "VisiblePlaintextCache",
  "EncryptedLocalOverlayCacheRecord",
  "EncryptedLocalCacheLimits",
] as const;

const FALSE_SHIPPING_CLAIMS = [
  /\bThe external overlay is enabled\b/iu,
  /\bOSL overlays fail closed on every (?:focus|geometry|context)[^.\n]{0,120}\bchange\b/iu,
  /\bMoving, resizing, minimizing,[^.\n]{0,180}\bhides it immediately\b/iu,
  /\bOnly visible messages[^.\n]{0,180}\bplaintext in RAM\b/iu,
  /\bFree is capped at 250 messages[^.\n]{0,120}\bPro at 1[,.]?000\b/iu,
  /\bVisible plaintext is limited to 250\s*\/\s*1[,.]?000 messages\b/iu,
  /\bzeroized on [^.\n]{0,160}\bcontext loss\b[^.\n]{0,100}\bapp switch\b/iu,
  /\bThe shipping overlay uses VisiblePlaintextCache\b/iu,
  /\bOverlay history is persisted in an encrypted local cache\b/iu,
] as const;

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

function sourceFiles(directory: URL, suffix: string): SourceFile[] {
  const files: SourceFile[] = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const url = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, directory);
    if (entry.isDirectory()) {
      files.push(...sourceFiles(url, suffix));
    } else if (entry.name.endsWith(suffix)) {
      files.push({ path: url.pathname, source: readFileSync(url, "utf8") });
    }
  }
  return files;
}

function publicPrototypeTypes(source: string): string[] {
  return [...source.matchAll(
    /^pub\s+(?:struct|enum|type|trait)\s+([A-Z][A-Za-z0-9_]*)\b/gmu,
  )].map((match) => match[1]);
}

function productionRustSource(source: string): string {
  return source.split(/\n#\[cfg\(test\)\]/u, 1)[0];
}

function tauriCommandBlocks(source: string): Array<{ name: string; block: string }> {
  return source
    .split(/(?=#\[tauri::command\])/u)
    .filter((block) => block.startsWith("#[tauri::command]"))
    .flatMap((block) => {
      const name = block.match(
        /^#\[tauri::command\]\s*(?:#\[[^\]]+\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(/u,
      )?.[1];
      return name === undefined ? [] : [{ name, block }];
    });
}

function generatedHandlerBodies(source: string): string[] {
  return [...source.matchAll(/tauri::generate_handler!\[([\s\S]*?)\]\)/gu)]
    .map((match) => match[1]);
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

function detectProductionPath(
  rustLib: string,
  prototypeSource: string,
  rustSources: SourceFile[],
  uiSources: SourceFile[],
): ProductionPath {
  const prototypeTypes = publicPrototypeTypes(prototypeSource);
  expect(prototypeTypes.length, "prototype public-type inventory must be nonempty")
    .toBeGreaterThan(0);
  const typeReference = new RegExp(
    `\\b(?:${prototypeTypes.map(escapeRegExp).join("|")})\\b`,
    "u",
  );
  const prototypeReference = new RegExp(
    `(?:\\bexternal_overlay\\s*::|${typeReference.source})`,
    "u",
  );
  const rustConsumers = rustSources.filter(
    ({ path }) => !path.endsWith("/external_overlay.rs"),
  );
  const rustConsumerFiles = rustConsumers.filter(({ source }) =>
    /\b(?:use\s+[^;\n]*\bexternal_overlay\b|(?:crate|self|super)?(?:::)?external_overlay\s*::)/u
      .test(source)
  );
  const genericHandlers = rustSources.flatMap(({ path, source }) => {
    const commands = tauriCommandBlocks(source);
    if (path.endsWith("/external_overlay.rs")) return commands;
    if (!rustConsumerFiles.some((consumer) => consumer.path === path)) return [];
    return commands.filter(({ block }) => prototypeReference.test(block));
  });
  const handlerNames = genericHandlers.map(({ name }) => name);
  const handlerBodies = rustSources.flatMap(({ source }) => generatedHandlerBodies(source));
  const handlerRegistered = handlerNames.some((name) =>
    handlerBodies.some((body) => new RegExp(`\\b${escapeRegExp(name)}\\b`, "u").test(body))
  );
  const invokingAdapters = uiSources.filter(({ source }) =>
    handlerNames.some((name) =>
      new RegExp(
        `\\binvoke(?:<[^>]+>)?\\s*\\(\\s*["']${escapeRegExp(name)}["']`,
        "u",
      ).test(source)
    )
  );
  const adapterImports = invokingAdapters.flatMap((adapter) => {
    const stem = adapter.path.split("/").at(-1)?.replace(/\.ts$/u, "");
    if (stem === undefined) return [];
    const importPattern = new RegExp(
      `\\bfrom\\s+["'][^"']*${escapeRegExp(stem)}["']`,
      "u",
    );
    return uiSources
      .filter(({ path, source }) => path !== adapter.path && importPattern.test(source))
      .map((importer) => ({ adapter, importer }));
  });
  const importedWrapperNames = adapterImports.flatMap(({ adapter }) =>
    [...adapter.source.matchAll(
      /\bexport\s+(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(/gu,
    )].map((match) => match[1])
  );
  const uiCall = importedWrapperNames.some((name) =>
    adapterImports.some(({ importer }) =>
      new RegExp(`\\b${escapeRegExp(name)}\\s*\\(`, "u").test(importer.source)
    )
  );
  const stages = {
    moduleDeclared: /\bpub mod external_overlay\s*;/u.test(rustLib),
    rustConsumer: rustConsumerFiles.length > 0,
    tauriHandlerDefined: genericHandlers.length > 0,
    handlerRegistered,
    uiInvokePresent: invokingAdapters.length > 0,
    uiAdapterImported: adapterImports.length > 0,
    uiCall,
    lifecycleCacheClearEdge: rustConsumers.some(({ source }) =>
      hasLifecycleCacheClearEdge(source)
    ),
  };
  return {
    ...stages,
    productionReachable:
      stages.moduleDeclared
      && stages.rustConsumer
      && stages.tauriHandlerDefined
      && stages.handlerRegistered
      && stages.uiInvokePresent
      && stages.uiAdapterImported
      && stages.uiCall,
  };
}

function syntheticPath(
  prototypeSource: string,
  stage: "dormant" | "consumer" | "handler" | "registered" | "imported" | "called",
  prototypeType = "ScreenRect",
): ProductionPath {
  const includes = (minimum: Exclude<typeof stage, "dormant">): boolean => {
    const order = ["consumer", "handler", "registered", "imported", "called"] as const;
    return stage !== "dormant" && order.indexOf(stage) >= order.indexOf(minimum);
  };
  const rustLib = "pub mod external_overlay;";
  const rustMain = [
    includes("consumer") ? `use crate::external_overlay::${prototypeType};` : "",
    includes("handler")
      ? `#[tauri::command]\nfn open_external_overlay(_value: ${prototypeType}) {}`
      : "",
    includes("registered")
      ? "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }"
      : "",
  ].join("\n");
  const uiAdapter = includes("imported")
    ? [
      'import { invoke } from "@tauri-apps/api/core";',
      "export function openExternalOverlay() {",
      '  return invoke("open_external_overlay");',
      "}",
    ].join("\n")
    : "";
  const uiMain = includes("imported")
    ? [
      'import { openExternalOverlay } from "./external-overlay-adapter";',
      includes("called") ? "openExternalOverlay();" : "",
    ].join("\n")
    : "";
  return detectProductionPath(
    rustLib,
    prototypeSource,
    [
      { path: "/synthetic/lib.rs", source: rustLib },
      { path: "/synthetic/main.rs", source: rustMain },
      { path: "/synthetic/external_overlay.rs", source: prototypeSource },
    ],
    [
      { path: "/synthetic/external-overlay-adapter.ts", source: uiAdapter },
      { path: "/synthetic/main.ts", source: uiMain },
    ],
  );
}

describe("generic external overlay production reachability", () => {
  it("derives the complete prototype surface and rejects every unwired shipping claim", () => {
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");
    const rustMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const externalOverlay = readFileSync(
      new URL("../../osl-hub/src/external_overlay.rs", import.meta.url),
      "utf8",
    );
    const nativeUi = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
    const nativeAdapter = readFileSync(
      new URL("./native-overlay-adapter.ts", import.meta.url),
      "utf8",
    );
    const contract = readFileSync(
      new URL("../../../docs/design/external-overlay-security-contract.md", import.meta.url),
      "utf8",
    );
    const stateMap = readFileSync(
      new URL("../../../docs/OSL-DISCORD-STATE-MAP.md", import.meta.url),
      "utf8",
    );
    const checklist = readFileSync(
      new URL("../../../docs/design/osl-internal-build-checklist.md", import.meta.url),
      "utf8",
    );
    const readme = readFileSync(new URL("../../../README.md", import.meta.url), "utf8");
    const rustSources = sourceFiles(new URL("../../osl-hub/src/", import.meta.url), ".rs")
      .map((file) => ({ ...file, source: productionRustSource(file.source) }));
    const productionUi = sourceFiles(new URL("./", import.meta.url), ".ts").filter(
      ({ path }) => !path.endsWith(".test.ts"),
    );
    const repositoryDocs = [
      { path: "/README.md", source: readme },
      ...sourceFiles(new URL("../../../docs/", import.meta.url), ".md"),
    ];
    const prototypeTypes = publicPrototypeTypes(externalOverlay);
    const production = detectProductionPath(
      rustLib,
      externalOverlay,
      rustSources,
      productionUi,
    );

    expect(prototypeTypes).toEqual([...EXPECTED_PUBLIC_PROTOTYPE_TYPES]);
    expect(new Set(prototypeTypes).size).toBe(16);

    // Positive controls preserve the real implemented-but-unwired behavior.
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
    expect(visibleCache).toContain("self.evict_expired(now_ms);");
    expect(visibleCache).toContain("self.entries.clear();");
    expect(cacheDrop).toContain("self.lock();");
    expect(externalOverlay).toContain(
      "Format contract for a possible future SSD cache. It is not wired to storage",
    );

    expect(production.moduleDeclared).toBe(true);
    expect(production.rustConsumer).toBe(false);
    expect(production.tauriHandlerDefined).toBe(false);
    expect(production.handlerRegistered).toBe(false);
    expect(production.uiInvokePresent).toBe(false);
    expect(production.uiAdapterImported).toBe(false);
    expect(production.uiCall).toBe(false);
    expect(production.lifecycleCacheClearEdge).toBe(false);
    expect(production.productionReachable).toBe(false);

    // Each generic reachability stage has an independent failure-capable positive.
    const dormant = syntheticPath(externalOverlay, "dormant");
    const consumer = syntheticPath(externalOverlay, "consumer");
    const handler = syntheticPath(externalOverlay, "handler");
    const registered = syntheticPath(externalOverlay, "registered");
    const imported = syntheticPath(externalOverlay, "imported");
    const called = syntheticPath(externalOverlay, "called");
    expect(dormant.rustConsumer).toBe(false);
    expect(consumer.rustConsumer).toBe(true);
    expect(handler.tauriHandlerDefined).toBe(true);
    expect(handler.handlerRegistered).toBe(false);
    expect(registered.handlerRegistered).toBe(true);
    expect(registered.uiInvokePresent).toBe(false);
    expect(imported.uiInvokePresent).toBe(true);
    expect(imported.uiAdapterImported).toBe(true);
    expect(imported.uiCall).toBe(false);
    expect(called.uiCall).toBe(true);
    expect(called.productionReachable).toBe(true);
    for (const type of prototypeTypes) {
      expect(
        syntheticPath(externalOverlay, "handler", type).tauriHandlerDefined,
        `production-path detector missed public prototype type: ${type}`,
      ).toBe(true);
    }

    // A separate production native Discord overlay remains the shipping positive control.
    const handlerBody = generatedHandlerBodies(rustMain).join("\n");
    expect(rustMain).toContain("mod native_discord_overlay;");
    expect(rustMain).toMatch(/\bfn get_native_discord_overlay_state\s*\(/u);
    expect(rustMain).toMatch(/\basync fn prepare_native_discord_overlay_text\s*\(/u);
    expect(handlerBody).toMatch(/\bget_native_discord_overlay_state\b/u);
    expect(handlerBody).toMatch(/\bprepare_native_discord_overlay_text\b/u);
    expect(handlerBody).toMatch(/\bopen_native_discord_overlay_text\b/u);
    expect(nativeUi).toContain('from "./native-overlay-adapter"');
    expect(nativeUi).toMatch(/\bprepareNativeDiscordOverlayText\s*\(/u);
    expect(nativeUi).toMatch(/\bopenNativeDiscordOverlayText\s*\(/u);
    expect(nativeAdapter).toContain('invoke<unknown>("prepare_native_discord_overlay_text"');
    expect(nativeAdapter).toContain('invoke<unknown>("open_native_discord_overlay_text"');

    // The claim root is the real README plus every Markdown file under docs/.
    expect(repositoryDocs.length, "repository documentation root was empty").toBeGreaterThan(25);
    expect(repositoryDocs.every(({ source }) => source.trim().length > 0)).toBe(true);
    const docPaths = repositoryDocs.map(({ path }) => path);
    for (const required of [
      "/README.md",
      "/docs/design/external-overlay-security-contract.md",
      "/docs/OSL-DISCORD-STATE-MAP.md",
      "/docs/design/osl-internal-build-checklist.md",
      "/docs/reports/crypto-lane-2026-07-26.md",
    ]) {
      expect(
        docPaths.some((path) => path.endsWith(required)),
        `required claim surface was not scanned: ${required}`,
      ).toBe(true);
    }
    const repositoryClaimCorpus = repositoryDocs
      .map(({ path, source }) => `\n<!-- ${path} -->\n${source}`)
      .join("\n");
    for (const claim of FALSE_SHIPPING_CLAIMS) {
      expect(repositoryClaimCorpus).not.toMatch(claim);
    }

    expect(contract).toContain("compiled but uncalled source prototype");
    expect(contract).toContain("Those are prototype state-machine properties, not shipping behavior.");
    expect(contract).toContain("No production lifecycle constructs or calls this cache.");
    expect(stateMap).toContain("The former generic-overlay contradiction is resolved");
    expect(stateMap).toContain("implemented-unwired source prototype");
    const stateMapOverlaySection = sourceBetween(stateMap, "### 8.3", "\n### 8.4");
    for (const type of prototypeTypes) expect(stateMapOverlaySection).toContain(`\`${type}\``);
    expect(checklist).toContain(
      "C6 · Window/composer lifecycle** — adoption, drag, minimize, focus, close, first launch,",
    );
    expect(checklist).toContain(
      "D5 · Timed deletion** — scheduler/ledger pieces exist; production wiring and all lifecycle",
    );

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
      "Plaintext is zeroized on eviction, lock, context loss, app switch, and drop.",
      'The docs say plaintext is *"zeroized on eviction, **lock**, context loss, app switch, and drop."*',
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
