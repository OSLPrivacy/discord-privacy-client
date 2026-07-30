export const extensionPermissions = ["notes:read-selected", "notes:create", "notes:update-selected", "assets:read-selected", "assets:create-derived", "render:filter", "formats:import", "formats:export", "ui:theme", "ui:command"] as const;
export type ExtensionPermission = typeof extensionPermissions[number];

export interface NotesExtensionManifest {
  manifestVersion: 1;
  id: string;
  name: string;
  version: string;
  description: string;
  kind: "theme" | "template" | "command-pack" | "importer" | "exporter" | "brush-pack" | "filter-pack" | "codec-pack";
  permissions: ExtensionPermission[];
}
export interface OslPluginInspection { manifest: NotesExtensionManifest; entrypoint: "osl_run"; memoryLimitBytes: number; fuelLimit: number; ambientAccess: false; }
export interface OslPluginProject { format: "osl-plugin-project-v1"; assetId: string; manifest: NotesExtensionManifest; }
export interface OslPluginRunReceipt { result: number; fuelConsumed: number; memoryLimitBytes: number; ambientAccess: false; }

const keys = ["manifestVersion", "id", "name", "version", "description", "kind", "permissions"];
const permissions = new Set<string>(extensionPermissions);

export function parseNotesExtensionManifest(raw: unknown): NotesExtensionManifest | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const value = raw as Record<string, unknown>;
  if (Object.keys(value).length !== keys.length || !Object.keys(value).every((key) => keys.includes(key))) return null;
  if (value.manifestVersion !== 1 || typeof value.id !== "string" || !/^[a-z0-9][a-z0-9.-]{2,63}$/u.test(value.id)) return null;
  if (typeof value.name !== "string" || value.name.length < 1 || value.name.length > 80 || typeof value.description !== "string" || value.description.length > 240) return null;
  if (typeof value.version !== "string" || !/^\d+\.\d+\.\d+(?:-[a-z0-9.-]+)?$/u.test(value.version)) return null;
  if (!["theme", "template", "command-pack", "importer", "exporter", "brush-pack", "filter-pack", "codec-pack"].includes(String(value.kind))) return null;
  if (!Array.isArray(value.permissions) || value.permissions.length > extensionPermissions.length || !value.permissions.every((item) => typeof item === "string" && permissions.has(item))) return null;
  if (new Set(value.permissions).size !== value.permissions.length) return null;
  const manifest = value as unknown as NotesExtensionManifest;
  if (manifest.kind === "theme" && manifest.permissions.some((permission) => permission !== "ui:theme")) return null;
  if (manifest.kind === "template" && manifest.permissions.some((permission) => !["notes:create", "ui:command"].includes(permission))) return null;
  if (manifest.kind === "importer" && manifest.permissions.some((permission) => !["formats:import", "assets:create-derived", "notes:create"].includes(permission))) return null;
  if (manifest.kind === "exporter" && manifest.permissions.some((permission) => !["formats:export", "assets:read-selected", "notes:read-selected"].includes(permission))) return null;
  if (["brush-pack", "filter-pack"].includes(manifest.kind) && manifest.permissions.some((permission) => !["render:filter", "assets:read-selected", "assets:create-derived", "ui:command"].includes(permission))) return null;
  if (manifest.kind === "codec-pack" && manifest.permissions.some((permission) => !["formats:import", "formats:export", "assets:read-selected", "assets:create-derived"].includes(permission))) return null;
  return manifest;
}

const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
export function parseOslPluginInspection(raw: unknown): OslPluginInspection | null {
  if (!record(raw) || Object.keys(raw).length !== 5 || raw.entrypoint !== "osl_run" || raw.ambientAccess !== false || raw.memoryLimitBytes !== 32 * 1024 * 1024 || raw.fuelLimit !== 2_000_000) return null;
  const manifest = parseNotesExtensionManifest(raw.manifest); return manifest?.kind === "command-pack" && manifest.permissions.length === 1 && manifest.permissions[0] === "ui:command" ? { manifest, entrypoint: "osl_run", memoryLimitBytes: Number(raw.memoryLimitBytes), fuelLimit: Number(raw.fuelLimit), ambientAccess: false } : null;
}
export function pluginProjectBody(assetId: string, inspection: OslPluginInspection): string { return JSON.stringify({ format: "osl-plugin-project-v1", assetId, manifest: inspection.manifest } satisfies OslPluginProject); }
export function parseOslPluginProject(body: string): OslPluginProject | null { try { const value: unknown = JSON.parse(body); if (!record(value) || Object.keys(value).length !== 3 || value.format !== "osl-plugin-project-v1" || typeof value.assetId !== "string" || !/^[a-f0-9]{32}$/u.test(value.assetId)) return null; const manifest = parseNotesExtensionManifest(value.manifest); return manifest?.kind === "command-pack" && manifest.permissions.length === 1 && manifest.permissions[0] === "ui:command" ? { format: "osl-plugin-project-v1", assetId: value.assetId, manifest } : null; } catch { return null; } }
export function parseOslPluginRunReceipt(raw: unknown): OslPluginRunReceipt | null { if (!record(raw) || Object.keys(raw).length !== 4 || !Number.isSafeInteger(raw.result) || !Number.isSafeInteger(raw.fuelConsumed) || Number(raw.fuelConsumed) < 0 || Number(raw.fuelConsumed) > 2_000_000 || raw.memoryLimitBytes !== 32 * 1024 * 1024 || raw.ambientAccess !== false) return null; return raw as unknown as OslPluginRunReceipt; }

export function permissionSummary(permission: ExtensionPermission): string {
  return ({
    "notes:read-selected": "Read only notes you explicitly share with it",
    "notes:create": "Create new notes after an action you invoke",
    "notes:update-selected": "Edit only the open note after confirmation",
    "assets:read-selected": "Read only media assets explicitly selected by you",
    "assets:create-derived": "Create a new derived asset without replacing its source",
    "render:filter": "Run a bounded filter in the isolated render worker",
    "formats:import": "Decode a user-selected file through the import broker",
    "formats:export": "Encode a user-selected project through the export broker",
    "ui:theme": "Change allowlisted OSL Notes visual tokens",
    "ui:command": "Add commands to the Notes command menu",
  })[permission];
}

export const extensionSafetyContract = Object.freeze({
  network: false,
  filesystem: false,
  processExecution: false,
  secrets: false,
  backgroundExecution: false,
  crossIdentityAccess: false,
});
