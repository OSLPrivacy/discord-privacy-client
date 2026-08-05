export function analyze_identifier(identifier: string): string;
export function initSync(input: { module: BufferSource | WebAssembly.Module }): WebAssembly.Exports;

/**
 * `osl_uts39_wasm.js` ends with `export default __wbg_init;` (the async
 * loader). This declaration file described only the two named exports that
 * `runtime.ts` consumes, so `import init from "./dist/osl_uts39_wasm.js"` in
 * `spike-worker.ts` resolved -- under `esModuleInterop` -- to the module
 * NAMESPACE rather than to that function, and `init()` was not callable.
 * Declared here to match the emitted JS, not to make the import quiet.
 */
export default function __wbg_init(
  module_or_path?:
    | { module_or_path: BufferSource | WebAssembly.Module | Response | string | URL }
    | BufferSource
    | WebAssembly.Module
    | Response
    | Promise<Response>
    | string
    | URL,
): Promise<WebAssembly.Exports>;
