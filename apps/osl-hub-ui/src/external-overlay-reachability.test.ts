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
  uiDirectCall: boolean;
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

function blankExceptNewlines(value: string): string {
  return value.replace(/[^\n]/gu, " ");
}

/**
 * Remove comments without damaging comment markers inside strings.
 *
 * Reachability is a source-level truth gate, so commented-out imports, handlers
 * and invokes must not count. Keeping byte positions stable also lets the
 * callable-range checks below compare invocation offsets safely.
 */
function stripComments(source: string, language: "rust" | "typescript"): string {
  let output = "";
  let index = 0;
  while (index < source.length) {
    if (language === "rust") {
      const raw = source.slice(index).match(/^r(#+)?"/u);
      if (raw) {
        const hashes = raw[1] ?? "";
        const delimiter = `"${hashes}`;
        const end = source.indexOf(delimiter, index + raw[0].length);
        const next = end < 0 ? source.length : end + delimiter.length;
        output += source.slice(index, next);
        index = next;
        continue;
      }
    }
    const char = source[index];
    const startsRustChar = language === "rust"
      && char === "'"
      && /^'(?:\\.|[^\\'\n])'/u.test(source.slice(index));
    if (char === '"' || char === "`" || (char === "'" && (language === "typescript" || startsRustChar))) {
      const quote = char;
      let next = index + 1;
      while (next < source.length) {
        if (source[next] === "\\") {
          next += 2;
          continue;
        }
        next += 1;
        if (source[next - 1] === quote) break;
      }
      output += source.slice(index, next);
      index = next;
      continue;
    }
    if (source.startsWith("//", index)) {
      const end = source.indexOf("\n", index + 2);
      const next = end < 0 ? source.length : end;
      output += blankExceptNewlines(source.slice(index, next));
      index = next;
      continue;
    }
    if (source.startsWith("/*", index)) {
      let depth = 1;
      let next = index + 2;
      while (next < source.length && depth > 0) {
        if (source.startsWith("/*", next)) {
          depth += 1;
          next += 2;
        } else if (source.startsWith("*/", next)) {
          depth -= 1;
          next += 2;
        } else {
          next += 1;
        }
      }
      output += blankExceptNewlines(source.slice(index, next));
      index = next;
      continue;
    }
    output += char;
    index += 1;
  }
  return output;
}

function topLevelArguments(value: string): string[] {
  const arguments_: string[] = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < value.length; index += 1) {
    if (value[index] === "(") depth += 1;
    else if (value[index] === ")") depth -= 1;
    else if (value[index] === "," && depth === 0) {
      arguments_.push(value.slice(start, index).trim());
      start = index + 1;
    }
  }
  arguments_.push(value.slice(start).trim());
  return arguments_.filter(Boolean);
}

/**
 * Possible truth values for one cfg expression when `test` is false.
 * Unknown platform/feature predicates may be either value.
 */
function cfgValuesWithoutTest(expression: string): Set<boolean> {
  const value = expression.trim();
  if (value === "test") return new Set([false]);
  for (const operation of ["all", "any", "not"] as const) {
    const prefix = `${operation}(`;
    if (!value.startsWith(prefix) || !value.endsWith(")")) continue;
    const arguments_ = topLevelArguments(value.slice(prefix.length, -1))
      .map(cfgValuesWithoutTest);
    if (operation === "not") {
      const argument = arguments_[0] ?? new Set([false, true]);
      return new Set([...argument].map((candidate) => !candidate));
    }
    if (operation === "all") {
      if (arguments_.length === 0) return new Set([true]);
      return new Set([
        ...(arguments_.every((argument) => argument.has(true)) ? [true] : []),
        ...(arguments_.some((argument) => argument.has(false)) ? [false] : []),
      ]);
    }
    if (arguments_.length === 0) return new Set([false]);
    return new Set([
      ...(arguments_.some((argument) => argument.has(true)) ? [true] : []),
      ...(arguments_.every((argument) => argument.has(false)) ? [false] : []),
    ]);
  }
  return new Set([false, true]);
}

function matchingDelimiter(
  source: string,
  openIndex: number,
  open: string,
  close: string,
): number {
  let depth = 0;
  for (let index = openIndex; index < source.length; index += 1) {
    if (source[index] === open) depth += 1;
    else if (source[index] === close) {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function rustItemEnd(source: string, attributeEnd: number): number {
  let cursor = attributeEnd;
  while (true) {
    while (/\s/u.test(source[cursor] ?? "")) cursor += 1;
    if (!source.startsWith("#[", cursor)) break;
    const close = matchingDelimiter(source, cursor + 1, "[", "]");
    if (close < 0) return source.length;
    cursor = close + 1;
  }
  let parens = 0;
  let brackets = 0;
  for (let index = cursor; index < source.length; index += 1) {
    const char = source[index];
    if (char === "(") parens += 1;
    else if (char === ")") parens -= 1;
    else if (char === "[") brackets += 1;
    else if (char === "]") brackets -= 1;
    else if (char === ";" && parens === 0 && brackets === 0) return index + 1;
    else if (char === "{" && parens === 0 && brackets === 0) {
      const close = matchingDelimiter(source, index, "{", "}");
      return close < 0 ? source.length : close + 1;
    }
  }
  return source.length;
}

function rustAttributeGroupStart(source: string, attributeStart: number): number {
  let start = attributeStart;
  while (start > 0) {
    let cursor = start;
    while (cursor > 0 && /\s/u.test(source[cursor - 1])) cursor -= 1;
    if (source[cursor - 1] !== "]") break;
    let depth = 1;
    let open = cursor - 2;
    while (open >= 0 && depth > 0) {
      if (source[open] === "]") depth += 1;
      else if (source[open] === "[") depth -= 1;
      open -= 1;
    }
    const bracket = open + 1;
    if (depth !== 0 || source[bracket - 1] !== "#") break;
    start = bracket - 1;
  }
  return start;
}

function stripTestOnlyRustItems(source: string): string {
  let production = source;
  let searchFrom = 0;
  while (searchFrom < production.length) {
    const match = /#\s*\[\s*cfg\s*\(/gu.exec(production.slice(searchFrom));
    if (!match || match.index === undefined) break;
    const start = searchFrom + match.index;
    const openParen = production.indexOf("(", start);
    const closeParen = matchingDelimiter(production, openParen, "(", ")");
    const closeBracket = closeParen < 0
      ? -1
      : production.indexOf("]", closeParen + 1);
    if (closeParen < 0 || closeBracket < 0) break;
    const cfgExpression = production.slice(openParen + 1, closeParen);
    if (cfgValuesWithoutTest(cfgExpression).has(true)) {
      searchFrom = closeBracket + 1;
      continue;
    }
    const removalStart = rustAttributeGroupStart(production, start);
    const end = rustItemEnd(production, closeBracket + 1);
    production = production.slice(0, removalStart)
      + blankExceptNewlines(production.slice(removalStart, end))
      + production.slice(end);
    searchFrom = removalStart;
  }
  return production;
}

function productionRustSource(source: string): string {
  return stripTestOnlyRustItems(stripComments(source, "rust"));
}

function productionTypeScriptSource(source: string): string {
  return stripComments(source, "typescript");
}

function isTestSpecFixturePath(path: string): boolean {
  const normalized = path.replaceAll("\\", "/");
  return /(?:^|\/)(?:__)?(?:tests?|specs?|fixtures?)(?:__)?(?:\/|$)/iu.test(normalized)
    || /(?:^|[._-])(?:test|spec|fixture)s?(?:[._-]|$)/iu.test(
      normalized.split("/").at(-1) ?? "",
    );
}

function tauriCommandBlocks(source: string): Array<{ name: string; block: string }> {
  const commands: Array<{ name: string; block: string }> = [];
  for (const marker of source.matchAll(/#\[tauri::command\]/gu)) {
    const start = marker.index;
    if (start === undefined) continue;
    const tail = source.slice(start);
    const header = tail.match(
      /^#\[tauri::command\]\s*(?:#\[[^\]]+\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(/u,
    );
    if (!header) continue;
    const openBrace = source.indexOf("{", start + header[0].length);
    const closeBrace = matchingDelimiter(source, openBrace, "{", "}");
    if (openBrace < 0 || closeBrace < 0) continue;
    commands.push({ name: header[1], block: source.slice(start, closeBrace + 1) });
  }
  return commands;
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

function externalOverlayNames(source: string, prototypeTypes: readonly string[]): Set<string> {
  const names = new Set(prototypeTypes);
  for (const statement of source.matchAll(/\buse\s+([^;]+);/gu)) {
    if (!/\bexternal_overlay\b/u.test(statement[1])) continue;
    for (const type of prototypeTypes) {
      const alias = statement[1].match(
        new RegExp(`\\b${escapeRegExp(type)}\\s+as\\s+([A-Z][A-Za-z0-9_]*)\\b`, "u"),
      )?.[1];
      if (alias) names.add(alias);
    }
  }
  for (const alias of source.matchAll(
    /\btype\s+([A-Z][A-Za-z0-9_]*)\s*=\s*([^;]+);/gu,
  )) {
    if (prototypeTypes.some((type) =>
      new RegExp(`\\b${escapeRegExp(type)}\\b`, "u").test(alias[2])
    )) names.add(alias[1]);
  }
  return names;
}

function invokeNames(source: string): Set<string> {
  const names = new Set<string>();
  for (const import_ of source.matchAll(
    /\bimport\s*\{([^}]+)\}\s*from\s*["']@tauri-apps\/api\/core["']/gu,
  )) {
    for (const binding of import_[1].split(",")) {
      const invoke = binding.trim().match(
        /^invoke(?:\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*))?$/u,
      );
      if (invoke) names.add(invoke[1] ?? "invoke");
    }
  }
  for (const import_ of source.matchAll(
    /\bimport\s+\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*)\s+from\s+["']@tauri-apps\/api\/core["']/gu,
  )) names.add(`${import_[1]}.invoke`);
  let changed = true;
  while (changed) {
    changed = false;
    for (const alias of source.matchAll(
      /\b(?:export\s+)?(?:const|let)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*([A-Za-z_$][A-Za-z0-9_$]*(?:\.invoke)?)\s*;/gu,
    )) {
      if (names.has(alias[2]) && !names.has(alias[1])) {
        names.add(alias[1]);
        changed = true;
      }
    }
    for (const destructured of source.matchAll(
      /\b(?:const|let)\s*\{\s*invoke(?:\s*:\s*([A-Za-z_$][A-Za-z0-9_$]*))?\s*\}\s*=\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*;/gu,
    )) {
      const localName = destructured[1] ?? "invoke";
      if (names.has(`${destructured[2]}.invoke`) && !names.has(localName)) {
        names.add(localName);
        changed = true;
      }
    }
  }
  return names;
}

type Invocation = { start: number; end: number };

function commandInvocations(source: string, command: string): Invocation[] {
  const commandName = escapeRegExp(command);
  const literal = `(?:"${commandName}"|'${commandName}'|\`${commandName}\`)`;
  const invocations: Invocation[] = [];
  for (const name of invokeNames(source)) {
    const call = new RegExp(
      `\\b${escapeRegExp(name)}\\s*(?:<[^;()]*>\\s*)?\\(\\s*${literal}`,
      "gu",
    );
    for (const match of source.matchAll(call)) {
      if (match.index !== undefined) {
        invocations.push({ start: match.index, end: match.index + match[0].length });
      }
    }
  }
  return invocations;
}

type CallableRange = {
  localName: string;
  start: number;
  bodyStart: number;
  end: number;
  exportedAs: Set<string>;
};

function callableRanges(source: string): CallableRange[] {
  const ranges: CallableRange[] = [];
  for (const function_ of source.matchAll(
    /\b(export\s+)?(?:async\s+)?function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(/gu,
  )) {
    const start = function_.index;
    if (start === undefined) continue;
    const openBrace = source.indexOf("{", start + function_[0].length);
    const closeBrace = matchingDelimiter(source, openBrace, "{", "}");
    if (openBrace < 0 || closeBrace < 0) continue;
    ranges.push({
      localName: function_[2],
      start,
      bodyStart: openBrace + 1,
      end: closeBrace + 1,
      exportedAs: new Set(function_[1] ? [function_[2]] : []),
    });
  }
  for (const constant of source.matchAll(
    /\b(export\s+)?const\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*(?:async\s*)?(?:(?:\([^;{}]*\)|[A-Za-z_$][A-Za-z0-9_$]*)\s*=>|function\b)/gu,
  )) {
    const start = constant.index;
    if (start === undefined) continue;
    let braces = 0;
    let parens = 0;
    let end = source.length;
    for (let index = start + constant[0].length; index < source.length; index += 1) {
      if (source[index] === "{") braces += 1;
      else if (source[index] === "}") braces -= 1;
      else if (source[index] === "(") parens += 1;
      else if (source[index] === ")") parens -= 1;
      else if (source[index] === ";" && braces === 0 && parens === 0) {
        end = index + 1;
        break;
      }
    }
    ranges.push({
      localName: constant[2],
      start,
      bodyStart: start + constant[0].length,
      end,
      exportedAs: new Set(constant[1] ? [constant[2]] : []),
    });
  }
  for (const export_ of source.matchAll(/\bexport\s*\{([^}]+)\}\s*;/gu)) {
    for (const binding of export_[1].split(",")) {
      const match = binding.trim().match(
        /^([A-Za-z_$][A-Za-z0-9_$]*)(?:\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*))?$/u,
      );
      if (!match) continue;
      const range = ranges.find(({ localName }) => localName === match[1]);
      if (range) range.exportedAs.add(match[2] ?? match[1]);
    }
  }
  return ranges;
}

function invokingWrappers(source: string, command: string): CallableRange[] {
  const invocations = commandInvocations(source, command);
  return callableRanges(source).filter((range) =>
    invocations.some(({ start }) => start >= range.start && start < range.end)
  );
}

function containingCallable(
  ranges: readonly CallableRange[],
  position: number,
): CallableRange | undefined {
  return ranges
    .filter((range) => position >= range.bodyStart && position < range.end)
    .sort((left, right) => (left.end - left.start) - (right.end - right.start))[0];
}

function reachableCallables(
  source: string,
  ranges: readonly CallableRange[],
): Set<CallableRange> {
  const roots = new Set<CallableRange>();
  const edges = new Map<CallableRange, Set<CallableRange>>();
  for (const target of ranges) {
    const call = new RegExp(`\\b${escapeRegExp(target.localName)}\\s*\\(`, "gu");
    for (const match of source.matchAll(call)) {
      const position = match.index;
      if (position === undefined) continue;
      if (position >= target.start && position < target.bodyStart) continue;
      const caller = containingCallable(ranges, position);
      if (!caller) roots.add(target);
      else if (caller !== target) {
        const callees = edges.get(caller) ?? new Set<CallableRange>();
        callees.add(target);
        edges.set(caller, callees);
      }
    }
  }
  const reachable = new Set(roots);
  const queue = [...roots];
  while (queue.length > 0) {
    const caller = queue.shift();
    if (!caller) continue;
    for (const callee of edges.get(caller) ?? []) {
      if (reachable.has(callee)) continue;
      reachable.add(callee);
      queue.push(callee);
    }
  }
  return reachable;
}

function positionIsReachable(
  ranges: readonly CallableRange[],
  reachable: ReadonlySet<CallableRange>,
  position: number,
): boolean {
  const container = containingCallable(ranges, position);
  return container === undefined || reachable.has(container);
}

function hasReachableNamedCall(source: string, name: string): boolean {
  const ranges = callableRanges(source);
  const reachable = reachableCallables(source, ranges);
  const call = new RegExp(`\\b${escapeRegExp(name)}\\s*\\(`, "gu");
  return [...source.matchAll(call)].some(({ index }) => {
    if (index === undefined) return false;
    if (ranges.some((range) => index >= range.start && index < range.bodyStart)) return false;
    return positionIsReachable(ranges, reachable, index);
  });
}

function hasReachableDirectCommandCall(source: string, command: string): boolean {
  const ranges = callableRanges(source);
  const reachable = reachableCallables(source, ranges);
  return commandInvocations(source, command).some(({ start }) =>
    positionIsReachable(ranges, reachable, start)
  );
}

function importedWrapperCalls(
  adapters: SourceFile[],
  uiSources: SourceFile[],
  command: string,
): Array<{ adapter: SourceFile; importer: SourceFile; localName: string }> {
  const calls: Array<{ adapter: SourceFile; importer: SourceFile; localName: string }> = [];
  for (const adapter of adapters) {
    const stem = adapter.path.split("/").at(-1)?.replace(/\.ts$/u, "");
    if (!stem) continue;
    const exports = invokingWrappers(adapter.source, command)
      .flatMap((wrapper) => [...wrapper.exportedAs]);
    if (exports.length === 0) continue;
    for (const importer of uiSources) {
      if (importer.path === adapter.path) continue;
      for (const import_ of importer.source.matchAll(
        /\bimport\s*\{([^}]+)\}\s*from\s*["']([^"']+)["']/gu,
      )) {
        const importedStem = import_[2].split("/").at(-1)?.replace(/\.ts$/u, "");
        if (importedStem !== stem) continue;
        for (const binding of import_[1].split(",")) {
          const match = binding.trim().match(
            /^([A-Za-z_$][A-Za-z0-9_$]*)(?:\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*))?$/u,
          );
          if (!match || !exports.includes(match[1])) continue;
          calls.push({ adapter, importer, localName: match[2] ?? match[1] });
        }
      }
    }
  }
  return calls;
}

function moduleStem(path: string): string {
  return path.replace(/\.(?:[cm]?ts|tsx)$/iu, "");
}

function resolveUiImport(
  importer: SourceFile,
  specifier: string,
  sources: readonly SourceFile[],
): SourceFile | undefined {
  if (!specifier.startsWith(".")) return undefined;
  const resolved = new URL(specifier, `file://${importer.path}`).pathname;
  return sources.find(({ path }) =>
    moduleStem(path) === moduleStem(resolved)
    || moduleStem(path) === `${moduleStem(resolved)}/index`
  );
}

function uiEntrySource(path: string): boolean {
  const basename = path.split("/").at(-1) ?? "";
  return /^(?:main|index|overlay|shield)\.(?:[cm]?ts|tsx)$/iu.test(basename)
    || /(?:^|[._-])entry(?:[._-]|$)/iu.test(basename);
}

function reachableUiSources(sources: readonly SourceFile[]): SourceFile[] {
  const reachable = new Set(
    sources.filter(({ path }) => uiEntrySource(path)).map(({ path }) => path),
  );
  const queue = sources.filter(({ path }) => reachable.has(path));
  while (queue.length > 0) {
    const importer = queue.shift();
    if (!importer) continue;
    for (const import_ of importer.source.matchAll(
      /\bimport(?:\s+[^"'()]+?\s+from\s+|\s*)["']([^"']+)["']/gu,
    )) {
      const imported = resolveUiImport(importer, import_[1], sources);
      if (!imported || reachable.has(imported.path)) continue;
      reachable.add(imported.path);
      queue.push(imported);
    }
  }
  return sources.filter(({ path }) => reachable.has(path));
}

function detectProductionPath(
  rustLib: string,
  prototypeSource: string,
  rustSources: SourceFile[],
  uiSources: SourceFile[],
): ProductionPath {
  const productionRustLib = productionRustSource(rustLib);
  const productionPrototype = productionRustSource(prototypeSource);
  const productionRust = rustSources
    .filter(({ path }) => !isTestSpecFixturePath(path))
    .map((file) => ({
      ...file,
      source: productionRustSource(file.source),
    }));
  const normalizedUi = uiSources
    .filter(({ path }) => !isTestSpecFixturePath(path))
    .map((file) => ({ ...file, source: productionTypeScriptSource(file.source) }));
  const productionUi = reachableUiSources(normalizedUi);
  const prototypeTypes = publicPrototypeTypes(productionPrototype);
  expect(prototypeTypes.length, "prototype public-type inventory must be nonempty")
    .toBeGreaterThan(0);
  const rustConsumers = productionRust.filter(
    ({ path }) => !path.endsWith("/external_overlay.rs"),
  );
  const rustConsumerFiles = rustConsumers.filter(({ source }) =>
    /\b(?:use\s+[^;\n]*\bexternal_overlay\b|(?:crate|self|super)?(?:::)?external_overlay\s*::)/u
      .test(source)
  );
  const genericHandlers = productionRust.flatMap(({ path, source }) => {
    const commands = tauriCommandBlocks(source);
    if (path.endsWith("/external_overlay.rs")) return commands;
    if (!rustConsumerFiles.some((consumer) => consumer.path === path)) return [];
    const references = externalOverlayNames(source, prototypeTypes);
    const prototypeReference = new RegExp(
      `(?:\\bexternal_overlay\\s*::|\\b(?:${[...references].map(escapeRegExp).join("|")})\\b)`,
      "u",
    );
    return commands.filter(({ block }) => prototypeReference.test(block));
  });
  const handlerNames = genericHandlers.map(({ name }) => name);
  const handlerBodies = productionRust.flatMap(({ source }) => generatedHandlerBodies(source));
  const handlerRegistered = handlerNames.some((name) =>
    handlerBodies.some((body) => new RegExp(`\\b${escapeRegExp(name)}\\b`, "u").test(body))
  );
  const invokingAdapters = productionUi.filter(({ source }) =>
    handlerNames.some((name) => commandInvocations(source, name).length > 0)
  );
  const adapterImports = handlerNames.flatMap((name) =>
    importedWrapperCalls(invokingAdapters, productionUi, name)
  );
  const uiCall = adapterImports.some(({ importer, localName }) =>
    hasReachableNamedCall(importer.source, localName)
  );
  const uiDirectCall = handlerNames.some((name) =>
    productionUi.some(({ source }) => hasReachableDirectCommandCall(source, name))
  );
  const stages = {
    moduleDeclared: /\bpub mod external_overlay\s*;/u.test(productionRustLib),
    rustConsumer: rustConsumerFiles.length > 0,
    tauriHandlerDefined: genericHandlers.length > 0,
    handlerRegistered,
    uiInvokePresent: invokingAdapters.length > 0,
    uiAdapterImported: adapterImports.length > 0,
    uiCall,
    uiDirectCall,
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
      && (stages.uiDirectCall || (stages.uiAdapterImported && stages.uiCall)),
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

const SYNTHETIC_EXTERNAL_OVERLAY = "pub struct ScreenRect;";
const SYNTHETIC_RUST_MAIN = [
  "use crate::external_overlay::ScreenRect;",
  "#[tauri::command]",
  "fn open_external_overlay(_value: ScreenRect) {}",
  "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }",
].join("\n");

function completeSyntheticPath(
  uiSources: SourceFile[],
  rustMain = SYNTHETIC_RUST_MAIN,
  rustMainPath = "/synthetic/main.rs",
): ProductionPath {
  const rustLib = "pub mod external_overlay;";
  return detectProductionPath(
    rustLib,
    SYNTHETIC_EXTERNAL_OVERLAY,
    [
      { path: "/synthetic/lib.rs", source: rustLib },
      { path: rustMainPath, source: rustMain },
      {
        path: "/synthetic/external_overlay.rs",
        source: SYNTHETIC_EXTERNAL_OVERLAY,
      },
    ],
    uiSources,
  );
}

const DIRECT_INVOKE_FORMS = [
  {
    name: "double-quoted",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      'invoke("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "single-quoted",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      "invoke('open_external_overlay');",
    ].join("\n"),
  },
  {
    name: "template-literal",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      "invoke(`open_external_overlay`);",
    ].join("\n"),
  },
  {
    name: "generic",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      'invoke<Promise<void>>("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "spaced",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      'invoke < Promise<void> > ( "open_external_overlay" );',
    ].join("\n"),
  },
  {
    name: "import-aliased",
    source: [
      'import { invoke as callNative } from "@tauri-apps/api/core";',
      'callNative("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "locally-aliased",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      "const callNative = invoke;",
      'callNative("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "local alias chain",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      "const firstAlias = invoke;",
      "const callNative = firstAlias;",
      'callNative("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "namespace authority",
    source: [
      'import * as tauriCore from "@tauri-apps/api/core";',
      'tauriCore.invoke("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "namespace-derived alias",
    source: [
      'import * as tauriCore from "@tauri-apps/api/core";',
      "const callNative = tauriCore.invoke;",
      'callNative("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "namespace-destructured alias",
    source: [
      'import * as tauriCore from "@tauri-apps/api/core";',
      "const { invoke: callNative } = tauriCore;",
      'callNative("open_external_overlay");',
    ].join("\n"),
  },
  {
    name: "called direct function",
    source: [
      'import { invoke } from "@tauri-apps/api/core";',
      "function openDirectly() {",
      '  return invoke("open_external_overlay");',
      "}",
      "openDirectly();",
    ].join("\n"),
  },
] as const;

const WRAPPER_FORMS = [
  {
    name: "exported const",
    adapter: [
      'import { invoke } from "@tauri-apps/api/core";',
      'export const openExternalOverlay = () => invoke("open_external_overlay");',
    ].join("\n"),
    main: [
      'import { openExternalOverlay } from "./external-overlay-adapter";',
      "openExternalOverlay();",
    ].join("\n"),
    call: "openExternalOverlay();",
  },
  {
    name: "import-aliased wrapper",
    adapter: [
      'import { invoke } from "@tauri-apps/api/core";',
      "export function openExternalOverlay() {",
      '  return invoke("open_external_overlay");',
      "}",
    ].join("\n"),
    main: [
      'import { openExternalOverlay as launchOverlay } from "./external-overlay-adapter";',
      "launchOverlay();",
    ].join("\n"),
    call: "launchOverlay();",
  },
  {
    name: "export-aliased const",
    adapter: [
      'import { invoke } from "@tauri-apps/api/core";',
      'const launch = () => invoke("open_external_overlay");',
      "export { launch as openExternalOverlay };",
    ].join("\n"),
    main: [
      'import { openExternalOverlay } from "./external-overlay-adapter";',
      "openExternalOverlay();",
    ].join("\n"),
    call: "openExternalOverlay();",
  },
  {
    name: "invoke-import alias",
    adapter: [
      'import { invoke as callNative } from "@tauri-apps/api/core";',
      "export function openExternalOverlay() {",
      '  return callNative("open_external_overlay");',
      "}",
    ].join("\n"),
    main: [
      'import { openExternalOverlay } from "./external-overlay-adapter";',
      "openExternalOverlay();",
    ].join("\n"),
    call: "openExternalOverlay();",
  },
  {
    name: "invoke-local alias",
    adapter: [
      'import { invoke } from "@tauri-apps/api/core";',
      "const callNative = invoke;",
      "export function openExternalOverlay() {",
      '  return callNative("open_external_overlay");',
      "}",
    ].join("\n"),
    main: [
      'import { openExternalOverlay } from "./external-overlay-adapter";',
      "openExternalOverlay();",
    ].join("\n"),
    call: "openExternalOverlay();",
  },
] as const;

describe("generic external overlay production reachability", () => {
  it("accepts every direct invoke spelling and fails when that invoke stage is removed", () => {
    for (const form of DIRECT_INVOKE_FORMS) {
      const positive = completeSyntheticPath([
        { path: "/synthetic/main.ts", source: form.source },
      ]);
      expect(positive.uiInvokePresent, `${form.name} invoke was missed`).toBe(true);
      expect(positive.uiDirectCall, `${form.name} direct call was missed`).toBe(true);
      expect(positive.uiAdapterImported).toBe(false);
      expect(positive.productionReachable, `${form.name} path was false-green`).toBe(true);

      const removed = completeSyntheticPath([
        {
          path: "/synthetic/main.ts",
          source: form.source.replace("open_external_overlay", "unrelated_command"),
        },
      ]);
      expect(removed.uiInvokePresent, `${form.name} removal did not fail`).toBe(false);
      expect(removed.uiDirectCall).toBe(false);
      expect(removed.productionReachable).toBe(false);
    }
  });

  it("accepts const/export/import/invoke aliases and fails when their call stage is removed", () => {
    for (const form of WRAPPER_FORMS) {
      const sources = [
        { path: "/synthetic/external-overlay-adapter.ts", source: form.adapter },
        { path: "/synthetic/main.ts", source: form.main },
      ];
      const positive = completeSyntheticPath(sources);
      expect(positive.uiInvokePresent, `${form.name} invoke was missed`).toBe(true);
      expect(positive.uiAdapterImported, `${form.name} import was missed`).toBe(true);
      expect(positive.uiCall, `${form.name} call was missed`).toBe(true);
      expect(positive.productionReachable, `${form.name} path was false-green`).toBe(true);

      const removed = completeSyntheticPath([
        sources[0],
        { ...sources[1], source: sources[1].source.replace(form.call, "") },
      ]);
      expect(removed.uiAdapterImported).toBe(true);
      expect(removed.uiCall, `${form.name} removal did not fail`).toBe(false);
      expect(removed.productionReachable).toBe(false);
    }
  });

  it("recognizes Rust import and type aliases in handlers with removal controls", () => {
    const directUi = [{
      path: "/synthetic/main.ts",
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        'invoke("open_external_overlay");',
      ].join("\n"),
    }];
    const importAlias = [
      "use crate::external_overlay::ScreenRect as Bounds;",
      "#[tauri::command]",
      "fn open_external_overlay(_value: Bounds) {}",
      "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }",
    ].join("\n");
    const typeAlias = [
      "use crate::external_overlay::ScreenRect;",
      "type Bounds = ScreenRect;",
      "#[tauri::command]",
      "fn open_external_overlay(_value: Bounds) {}",
      "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }",
    ].join("\n");

    expect(completeSyntheticPath(directUi, importAlias).productionReachable).toBe(true);
    expect(
      completeSyntheticPath(
        directUi,
        importAlias.replace(
          "use crate::external_overlay::ScreenRect as Bounds;",
          "use crate::other::Bounds;",
        ),
      ).productionReachable,
    ).toBe(false);
    expect(completeSyntheticPath(directUi, typeAlias).productionReachable).toBe(true);
    expect(
      completeSyntheticPath(
        directUi,
        typeAlias.replace("type Bounds = ScreenRect;", ""),
      ).productionReachable,
    ).toBe(false);
  });

  it("rejects commented and cfg(test)-only production-path decoys", () => {
    const directUi = [{
      path: "/synthetic/main.ts",
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        'invoke("open_external_overlay");',
      ].join("\n"),
    }];
    const rustCommentDecoy = [
      "// use crate::external_overlay::ScreenRect;",
      "/*",
      "#[tauri::command]",
      "fn open_external_overlay(_value: ScreenRect) {}",
      "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }",
      "*/",
    ].join("\n");
    const uiCommentDecoy = [{
      path: "/synthetic/main.ts",
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        '// invoke("open_external_overlay");',
        "/* invoke<unknown>(`open_external_overlay`); */",
      ].join("\n"),
    }];
    const testOnlyRust = [
      "#[cfg(test)]",
      "use crate::external_overlay::ScreenRect;",
      '#[cfg(all(test, feature = "qa"))]',
      "#[tauri::command]",
      "fn open_external_overlay(_value: ScreenRect) {}",
      "#[cfg(any(test))]",
      "fn run() { builder.invoke_handler(tauri::generate_handler![open_external_overlay]); }",
    ].join("\n");

    const rustDecoy = completeSyntheticPath(directUi, rustCommentDecoy);
    expect(rustDecoy.rustConsumer).toBe(false);
    expect(rustDecoy.tauriHandlerDefined).toBe(false);
    expect(rustDecoy.productionReachable).toBe(false);

    const uiDecoy = completeSyntheticPath(uiCommentDecoy);
    expect(uiDecoy.uiInvokePresent).toBe(false);
    expect(uiDecoy.uiDirectCall).toBe(false);
    expect(uiDecoy.productionReachable).toBe(false);

    const testDecoy = completeSyntheticPath(directUi, testOnlyRust);
    expect(testDecoy.rustConsumer).toBe(false);
    expect(testDecoy.tauriHandlerDefined).toBe(false);
    expect(testDecoy.handlerRegistered).toBe(false);
    expect(testDecoy.productionReachable).toBe(false);

    const mixedCfg = productionRustSource([
      '#[cfg(any(test, target_os = "windows"))]',
      "fn shipping_on_windows() {}",
    ].join("\n"));
    expect(mixedCfg).toContain("shipping_on_windows");
  });

  it("requires trusted Tauri invoke authority and production source roots", () => {
    for (const fake of [
      [
        "function invoke(_command: string) { return Promise.resolve(); }",
        'invoke("open_external_overlay");',
      ].join("\n"),
      [
        "const invoke = (_command: string) => Promise.resolve();",
        "const callNative = invoke;",
        'callNative("open_external_overlay");',
      ].join("\n"),
      [
        'import { invoke as callNative } from "./fake-tauri";',
        'callNative("open_external_overlay");',
      ].join("\n"),
    ]) {
      const result = completeSyntheticPath([
        { path: "/synthetic/main.ts", source: fake },
      ]);
      expect(result.uiInvokePresent).toBe(false);
      expect(result.uiDirectCall).toBe(false);
      expect(result.productionReachable).toBe(false);
    }

    const trusted = [
      'import { invoke as importedInvoke } from "@tauri-apps/api/core";',
      "const firstAlias = importedInvoke;",
      "const secondAlias = firstAlias;",
      'secondAlias("open_external_overlay");',
    ].join("\n");
    expect(
      completeSyntheticPath([{ path: "/synthetic/main.ts", source: trusted }])
        .productionReachable,
    ).toBe(true);
    expect(
      completeSyntheticPath([{
        path: "/synthetic/main.ts",
        source: trusted.replace(
          'secondAlias("open_external_overlay");',
          "",
        ),
      }]).productionReachable,
    ).toBe(false);

    const testOnlySource = [
      'import { invoke } from "@tauri-apps/api/core";',
      'invoke("open_external_overlay");',
    ].join("\n");
    for (const path of [
      "/synthetic/main.spec.ts",
      "/synthetic/main.test.ts",
      "/synthetic/main.fixture.ts",
      "/synthetic/spec/main.ts",
      "/synthetic/tests/main.ts",
      "/synthetic/fixtures/main.ts",
    ]) {
      const result = completeSyntheticPath([{ path, source: testOnlySource }]);
      expect(result.uiInvokePresent, `test-only source was admitted: ${path}`).toBe(false);
      expect(result.productionReachable).toBe(false);
    }
    for (const path of [
      "/synthetic/main.spec.rs",
      "/synthetic/main.test.rs",
      "/synthetic/main.fixture.rs",
      "/synthetic/spec/main.rs",
      "/synthetic/tests/main.rs",
      "/synthetic/fixtures/main.rs",
    ]) {
      const result = completeSyntheticPath(
        [{ path: "/synthetic/main.ts", source: trusted }],
        SYNTHETIC_RUST_MAIN,
        path,
      );
      expect(result.rustConsumer, `test-only Rust source was admitted: ${path}`).toBe(false);
      expect(result.productionReachable).toBe(false);
    }

    const unimported = completeSyntheticPath([{
      path: "/synthetic/dead-adapter.ts",
      source: testOnlySource,
    }]);
    expect(unimported.uiInvokePresent).toBe(false);
    expect(unimported.productionReachable).toBe(false);
  });

  it("requires direct and wrapper containers to be reachable from an entry", () => {
    const adapter = [
      'import { invoke } from "@tauri-apps/api/core";',
      "export function openExternalOverlay() {",
      '  return invoke("open_external_overlay");',
      "}",
    ].join("\n");
    const deadWrapperCall = [
      'import { openExternalOverlay as launch } from "./external-overlay-adapter";',
      "function neverCalled() {",
      "  launch();",
      "}",
    ].join("\n");
    const reachableWrapperCall = [
      'import { openExternalOverlay as launch } from "./external-overlay-adapter";',
      "function startOverlay() {",
      "  launch();",
      "}",
      "function boot() {",
      "  startOverlay();",
      "}",
      "boot();",
    ].join("\n");
    const dead = completeSyntheticPath([
      { path: "/synthetic/external-overlay-adapter.ts", source: adapter },
      { path: "/synthetic/main.ts", source: deadWrapperCall },
    ]);
    expect(dead.uiInvokePresent).toBe(true);
    expect(dead.uiAdapterImported).toBe(true);
    expect(dead.uiCall).toBe(false);
    expect(dead.productionReachable).toBe(false);

    const reachable = completeSyntheticPath([
      { path: "/synthetic/external-overlay-adapter.ts", source: adapter },
      { path: "/synthetic/main.ts", source: reachableWrapperCall },
    ]);
    expect(reachable.uiCall).toBe(true);
    expect(reachable.productionReachable).toBe(true);

    const removedEntryCall = completeSyntheticPath([
      { path: "/synthetic/external-overlay-adapter.ts", source: adapter },
      {
        path: "/synthetic/main.ts",
        source: reachableWrapperCall.replace("boot();", ""),
      },
    ]);
    expect(removedEntryCall.uiCall).toBe(false);
    expect(removedEntryCall.productionReachable).toBe(false);

    const reachableDirect = completeSyntheticPath([{
      path: "/synthetic/main.ts",
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        "function sendFromReachableChild() {",
        '  invoke("open_external_overlay");',
        "}",
        "function boot() { sendFromReachableChild(); }",
        "boot();",
      ].join("\n"),
    }]);
    expect(reachableDirect.uiDirectCall).toBe(true);
    expect(reachableDirect.productionReachable).toBe(true);

    const deadDirect = completeSyntheticPath([{
      path: "/synthetic/main.ts",
      source: [
        'import { invoke } from "@tauri-apps/api/core";',
        "function sendFromDeadChild() {",
        '  invoke("open_external_overlay");',
        "}",
      ].join("\n"),
    }]);
    expect(deadDirect.uiInvokePresent).toBe(true);
    expect(deadDirect.uiDirectCall).toBe(false);
    expect(deadDirect.productionReachable).toBe(false);
  });

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
    const rustSources = sourceFiles(new URL("../../osl-hub/src/", import.meta.url), ".rs");
    const productionUi = sourceFiles(new URL("./", import.meta.url), ".ts");
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
    expect(production.uiDirectCall).toBe(false);
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
    expect(imported.uiDirectCall).toBe(false);
    expect(called.uiCall).toBe(true);
    expect(called.uiDirectCall).toBe(false);
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
