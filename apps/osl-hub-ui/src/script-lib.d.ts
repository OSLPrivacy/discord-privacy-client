/**
 * Type declarations for the plain-JS helpers under `scripts/lib/`.
 *
 * `t7-07` imports these from a strict-mode TS test, which fails with TS7016
 * ("implicitly has an 'any' type") because they are `.mjs` with no types. The
 * alternative — turning on `allowJs` — would change how the whole app is
 * compiled just to satisfy two test-only helpers, so declare them instead.
 *
 * Deliberately loose: these are harness utilities, not product code, and a
 * precise mirror of their shape would be another thing to keep in sync.
 */
declare module "*/scripts/lib/cdp-harness.mjs" {
  export function locateChrome(): string;
  export class CDPClient {
    send(method: string, params?: Record<string, unknown>): Promise<unknown>;
    close(): void;
  }
  export function connectToChrome(
    chromeChild: unknown,
    options?: { timeoutMs?: number },
  ): Promise<CDPClient>;
  /**
   * Spreads the underlying connection, so the returned object carries whatever
   * `connectToChrome` provides plus `child`, `openPage` and `close`. Typed as an
   * index signature because the spread makes an exact shape both unstable and
   * not worth pinning for a test harness.
   */
  export function launchChrome(options?: {
    chromePath?: string;
    args?: readonly string[];
    timeoutMs?: number;
  }): Promise<{
    child: unknown;
    openPage(): Promise<any>;
    close(): Promise<void>;
    [key: string]: any;
  }>;
}

declare module "*/scripts/lib/csp-mirror.mjs" {
  export function shippedHubCsp(manifestPath?: string): string;
  export function shippedHubCspHeaders(manifestPath?: string): Record<string, string>;
}
