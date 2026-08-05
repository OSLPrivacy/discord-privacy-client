/// D-278 / D-274 — this file used to be one assertion:
///
///   expect(PushConnection.name).toBe("PushConnection");
///
/// under the title *"exports the class named by the shipping Wrangler
/// binding"*. It never read the binding. Measured, both directions:
///
///   * renaming `class_name` in `wrangler.toml` and leaving the Worker alone —
///     a deployment that binds a class the entry point does not export, i.e.
///     exactly the failure the title names — passed at **exit 0**;
///   * renaming the class *declaration* while exporting it under the name the
///     binding uses — a deployment Wrangler resolves without complaint, since
///     it binds the EXPORT name and not the class identifier — failed at
///     **exit 1**.
///
/// So it was blind to the break and loud on the harmless change. Both come
/// from the same mistake: it graded a spelling that the `import` above it had
/// already fixed, instead of the relation between two files.
///
/// What is graded below is that relation: every Durable Object class the
/// shipping config names must be exported by the module the shipping config
/// deploys. Nothing here spells "PushConnection" — the names come out of
/// `wrangler.toml`, so a rename that moves both sides together is silent, and
/// a rename that moves one is not.

import { describe, expect, it } from "vitest";

import * as entry from "../src/index.js";
import wranglerConfig from "../wrangler.toml?raw";

/// The Worker entry module's export table, keyed by the name a binding would
/// name. `import *` is deliberate: naming an export in the import statement is
/// what made the old assertion tautological.
const entryExports = entry as unknown as Record<string, unknown>;

/// Strip TOML comments so a `class_name` written inside the prose in
/// `wrangler.toml` — that file is mostly prose — is never mistaken for a
/// shipping declaration.
function uncommented(toml: string): string {
  return toml
    .split("\n")
    .map((line) => (line.trimStart().startsWith("#") ? "" : line))
    .join("\n");
}

/// Every `[[durable_objects.bindings]]` block's `name` / `class_name` pair.
function durableObjectBindings(toml: string): { binding: string; className: string }[] {
  const bindings: { binding: string; className: string }[] = [];
  const blocks = uncommented(toml).split(/^\s*\[\[durable_objects\.bindings\]\]\s*$/m).slice(1);
  for (const block of blocks) {
    const body = block.split(/^\s*\[/m)[0] ?? "";
    const binding = /^\s*name\s*=\s*"([^"]+)"/m.exec(body)?.[1];
    const className = /^\s*class_name\s*=\s*"([^"]+)"/m.exec(body)?.[1];
    if (binding !== undefined && className !== undefined) bindings.push({ binding, className });
  }
  return bindings;
}

/// Every class named by a `[[migrations]]` block, whichever list it sits in.
/// A migration that creates or renames a class is naming a class this Worker
/// has to be able to instantiate, exactly like a binding does.
function migrationClasses(toml: string): string[] {
  const classes: string[] = [];
  const blocks = uncommented(toml).split(/^\s*\[\[migrations\]\]\s*$/m).slice(1);
  for (const block of blocks) {
    const body = block.split(/^\s*\[/m)[0] ?? "";
    for (const list of body.matchAll(/^\s*new(?:_sqlite)?_classes\s*=\s*\[([^\]]*)\]/gm)) {
      for (const name of (list[1] ?? "").matchAll(/"([^"]+)"/g)) {
        if (name[1] !== undefined) classes.push(name[1]);
      }
    }
    for (const rename of body.matchAll(/to\s*=\s*"([^"]+)"/g)) {
      if (rename[1] !== undefined) classes.push(rename[1]);
    }
  }
  return classes;
}

describe("T1-51 realtime Durable Object entry wiring", () => {
  /// A parse that found nothing would make every assertion below vacuous, and
  /// a config with no Durable Object at all is not a state this Worker can be
  /// in — the realtime lane is one hibernatable DO per connection. So the
  /// parse is asserted to have found something before it is trusted.
  it("reads real bindings out of the shipping Wrangler config", () => {
    expect(wranglerConfig.length).toBeGreaterThan(1000);
    expect(durableObjectBindings(wranglerConfig).length).toBeGreaterThanOrEqual(1);
    expect(migrationClasses(wranglerConfig).length).toBeGreaterThanOrEqual(1);

    // And the parser must be able to say no: a binding whose class_name is
    // absent is not silently dropped into an empty list.
    expect(durableObjectBindings('[[durable_objects.bindings]]\nname = "X"\n')).toEqual([]);
    expect(
      durableObjectBindings('[[durable_objects.bindings]]\nname = "X"\nclass_name = "Y"\n'),
    ).toEqual([{ binding: "X", className: "Y" }]);

    // Commented-out prose in wrangler.toml may not be read as a declaration.
    expect(
      durableObjectBindings('# [[durable_objects.bindings]]\n# class_name = "Ghost"\n'),
    ).toEqual([]);
  });

  it("exports the class named by the shipping Wrangler binding", () => {
    for (const { binding, className } of durableObjectBindings(wranglerConfig)) {
      expect(
        typeof entryExports[className],
        `wrangler.toml binds ${binding} to class ${className}, which src/index.ts does not export`,
      ).toBe("function");
    }
  });

  it("exports every Durable Object class the shipping migrations name", () => {
    for (const className of migrationClasses(wranglerConfig)) {
      expect(
        typeof entryExports[className],
        `a [[migrations]] block names class ${className}, which src/index.ts does not export`,
      ).toBe("function");
    }
  });

  /// A class may be bound only if a migration introduced it: Wrangler refuses a
  /// SQLite-backed Durable Object class that no migration tag creates, so a
  /// binding no migration accounts for is a deploy that fails after this config
  /// has already been accepted everywhere else.
  it("accounts for every bound class in a migration", () => {
    const declared = new Set(migrationClasses(wranglerConfig));
    for (const { binding, className } of durableObjectBindings(wranglerConfig)) {
      expect(
        declared.has(className),
        `wrangler.toml binds ${binding} to class ${className}, which no [[migrations]] block creates`,
      ).toBe(true);
    }
  });
});
