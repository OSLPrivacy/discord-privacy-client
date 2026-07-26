import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import { parseOslPluginInspection, parseOslPluginRunReceipt, type OslPluginInspection, type OslPluginRunReceipt } from "./osl-notes-mods";

export async function inspectOslPluginAsset(assetId: string): Promise<OslPluginInspection | null> { if (!isTauriRuntime() || !/^[a-f0-9]{32}$/u.test(assetId)) return null; return parseOslPluginInspection(await invoke("inspect_osl_plugin_asset", { assetId })); }
export async function runOslPluginCommand(assetId: string, input: number): Promise<OslPluginRunReceipt | null> { if (!isTauriRuntime() || !/^[a-f0-9]{32}$/u.test(assetId) || !Number.isSafeInteger(input)) return null; return parseOslPluginRunReceipt(await invoke("run_osl_plugin_command", { assetId, input })); }
