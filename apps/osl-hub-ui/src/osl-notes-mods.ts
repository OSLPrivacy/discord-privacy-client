export const extensionPermissions = [
  "notes:read-selected",
  "notes:create",
  "notes:update-selected",
] as const;

export type ExtensionPermission = typeof extensionPermissions[number];

export interface NotesExtensionManifest {
  manifestVersion: 1;
  id: string;
  name: string;
  version: string;
  permissions: ExtensionPermission[];
}

export function parseNotesExtensionManifest(value: unknown): NotesExtensionManifest | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const manifest = value as Record<string, unknown>;
  if (manifest.manifestVersion !== 1 || typeof manifest.id !== "string" || !/^[a-z0-9][a-z0-9.-]{2,63}$/u.test(manifest.id)) return null;
  if (typeof manifest.name !== "string" || manifest.name.length < 1 || manifest.name.length > 80) return null;
  if (typeof manifest.version !== "string" || !/^\d+\.\d+\.\d+$/u.test(manifest.version)) return null;
  if (!Array.isArray(manifest.permissions) || !manifest.permissions.every((permission) => extensionPermissions.includes(permission as ExtensionPermission))) return null;
  return manifest as unknown as NotesExtensionManifest;
}

export const notesModsAvailability = {
  available: false,
  reason: "Third-party Notes extensions remain disabled until the sandboxed runtime ships.",
} as const;
