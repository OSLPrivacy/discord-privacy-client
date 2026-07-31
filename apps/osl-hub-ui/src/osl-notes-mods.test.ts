import { describe, expect, it } from "vitest";
import { extensionSafetyContract, parseNotesExtensionManifest, parseOslPluginInspection, parseOslPluginProject, parseOslPluginRunReceipt, pluginProjectBody } from "./osl-notes-mods";

describe("OSL Notes extension manifests", () => {
  it("accepts a bounded declarative command pack", () => expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "dev.example.tasks", name: "Tasks", version: "1.0.0", description: "Task commands", kind: "command-pack", permissions: ["ui:command", "notes:update-selected"] })).not.toBeNull());
  it("rejects unknown permissions and extra fields", () => {
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "dev.example.bad", name: "Bad", version: "1.0.0", description: "", kind: "command-pack", permissions: ["network:any"] })).toBeNull();
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "dev.example.bad", name: "Bad", version: "1.0.0", description: "", kind: "theme", permissions: [], script: "run" })).toBeNull();
  });
  it("has no ambient desktop capabilities", () => expect(Object.values(extensionSafetyContract)).toEqual([false, false, false, false, false, false]));
  it("allows creative packs only their narrow broker capabilities", () => {
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "dev.example.psd", name: "PSD importer", version: "1.0.0", description: "Local decoder", kind: "importer", permissions: ["formats:import", "assets:create-derived"] })).not.toBeNull();
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "dev.example.psd", name: "PSD importer", version: "1.0.0", description: "Bad decoder", kind: "importer", permissions: ["formats:export"] })).toBeNull();
  });
});

describe("executable extension boundary", () => {
  const manifest = { manifestVersion: 1 as const, id: "org.example.counter", name: "Counter", version: "1.0.0", description: "Pure command", kind: "command-pack" as const, permissions: ["ui:command" as const] };
  it("accepts only fixed no-ambient inspection receipts", () => { const receipt = { manifest, entrypoint: "osl_run", memoryLimitBytes: 32 * 1024 * 1024, fuelLimit: 2_000_000, ambientAccess: false }; expect(parseOslPluginInspection(receipt)).toEqual(receipt); expect(parseOslPluginInspection({ ...receipt, ambientAccess: true })).toBeNull(); });
  it("round-trips an encrypted plugin project pointer", () => { const inspection = parseOslPluginInspection({ manifest, entrypoint: "osl_run", memoryLimitBytes: 32 * 1024 * 1024, fuelLimit: 2_000_000, ambientAccess: false })!; expect(parseOslPluginProject(pluginProjectBody("a".repeat(32), inspection))?.manifest.id).toBe(manifest.id); });
  it("rejects forged or over-budget run receipts", () => { expect(parseOslPluginRunReceipt({ result: 42, fuelConsumed: 10, memoryLimitBytes: 32 * 1024 * 1024, ambientAccess: false })?.result).toBe(42); expect(parseOslPluginRunReceipt({ result: 42, fuelConsumed: 2_000_001, memoryLimitBytes: 32 * 1024 * 1024, ambientAccess: false })).toBeNull(); });
});
