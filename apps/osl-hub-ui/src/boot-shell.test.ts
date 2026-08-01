import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * The first frame OSL paints.
 *
 * Everything here is asserted against the two files the WebView actually loads
 * -- index.html and boot-shell.css -- and never against the text of a
 * TypeScript function. That distinction is the whole point of this surface:
 * the shipped CSP is `style-src 'self'` with no 'unsafe-inline', no nonce and
 * no hash (apps/osl-hub/tauri.conf.json), so a runtime <style> element or an
 * inline `style` attribute is dropped by the engine and renders nothing at all.
 * A test that read a CSS string out of main.ts once passed for an entire
 * release while the primary navigation shipped as unstyled native buttons.
 */
const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const bootShell = readFileSync(new URL("./boot-shell.css", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** Declarations only, so a colour quoted in a rationale is not read as code. */
function declarations(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//gu, "");
}

describe("static boot shell", () => {
  it("fills #app with markup the parser can paint before any script runs", () => {
    // The window is on screen at ~32ms; bootstrap() cannot draw into it until
    // ~2.5s. An empty #app is 2.5 seconds of the user agent's white default
    // page, then a hard cut to a near-black application.
    expect(html).not.toMatch(/<div id="app"><\/div>/u);
    expect(html).toMatch(/<div id="app">[\s\S]*<div class="boot-shell">/u);
    expect(html).toContain('<div class="boot-shell-seal" aria-hidden="true"></div>');
    expect(html).toContain("Opening OSL");
  });

  it("makes its stylesheet available before application code runs", () => {
    // The document links it, so it is a stylesheet of the page from the moment
    // the parser reaches <head>. Vite may fold it into the same emitted chunk
    // as styles.css -- that is fine, every <link> in <head> blocks the first
    // paint anyway, and the thing being avoided here is not a second file, it
    // is having the rules reachable only after 388 kB of JavaScript has parsed
    // and run.
    expect(html).toContain('<link rel="stylesheet" href="/src/boot-shell.css" />');
  });

  it("needs no script to appear and no script to go away", () => {
    // bootstrap() overwrites root.innerHTML, which removes the shell for free.
    // Anything that had to be toggled by JS would be back to waiting on the
    // bundle, and would leave the shell stuck on screen if the bundle failed.
    // Comments stripped first: a rationale that names `<style>` as the thing
    // being avoided is not a `<style>` element.
    const documentMarkup = html.replace(/<!--[\s\S]*?-->/gu, "");
    const body = documentMarkup.slice(documentMarkup.indexOf("<body>"));
    const shell = body.slice(body.indexOf('<div id="app">'), body.indexOf("<script"));
    expect(shell).toContain("boot-shell");
    expect(shell).not.toMatch(/<script/u);
    expect(shell).not.toMatch(/\son[a-z]+=/u);
    // No inline `style` attribute and no <style> element anywhere: the CSP
    // drops both silently, so either one is styling that renders as nothing.
    expect(documentMarkup).not.toMatch(/\sstyle="/u);
    expect(documentMarkup).not.toMatch(/<style[\s>]/u);
  });

  it("paints OSL's own dark surface rather than the user agent's white page", () => {
    const shellRules = declarations(bootShell);
    expect(shellRules).toMatch(/:root\s*\{[^}]*background:\s*#0a0a0a/u);
    expect(shellRules).toMatch(/\.boot-shell\s*\{[^}]*background:\s*#0a0a0a/u);
  });

  it("pins its literal palette to the values styles.css declares", () => {
    // boot-shell.css cannot use var(--bg): the custom properties live in
    // styles.css, which the shell exists to cover for. The literals are
    // duplicated deliberately, so they are pinned here instead -- a palette
    // change in styles.css that is not mirrored fails this test rather than
    // shipping a boot frame in the old colours.
    const root = styles.slice(styles.indexOf(":root {"), styles.indexOf("}", styles.indexOf(":root {")));
    expect(root).toMatch(/--bg:\s*#0a0a0a;/u);
    expect(root).toMatch(/--line:\s*#2a2a2a;/u);
    expect(root).toMatch(/--subtle:\s*#888;/u);
    expect(styles).toMatch(/\.desktop-titlebar\s*\{[^}]*background:\s*#080808/u);

    const shellRules = declarations(bootShell);
    expect(shellRules).toMatch(/\.boot-shell-titlebar\s*\{[^}]*background:\s*#080808/u);
    expect(shellRules).toMatch(/\.boot-shell-lines > span\s*\{[^}]*background:\s*#2a2a2a/u);
    expect(shellRules).toMatch(/\.boot-shell\s*\{[^}]*color:\s*#888/u);
  });

  it("commits to dark rather than following the OS", () => {
    // :root in styles.css is unconditionally dark; light only applies once
    // applyTheme() sets :root[data-theme="light"]. A shell that followed
    // prefers-color-scheme would put a light frame in front of the dark app for
    // every first-run install, because initializeThemePreference() hands a fresh
    // profile "dark" whatever the OS reports.
    expect(declarations(bootShell)).not.toContain("prefers-color-scheme");
  });

  it("reserves the titlebar row so the seal does not jump when the app takes over", () => {
    const shellRules = declarations(bootShell);
    expect(shellRules).toMatch(/\.boot-shell\s*\{[^}]*grid-template-rows:\s*44px minmax\(0, 1fr\)/u);
    // The same row height .app-frame.with-titlebar uses for the real screen.
    expect(styles).toMatch(/\.app-frame\.with-titlebar\s*\{[^}]*grid-template-rows:\s*44px minmax\(0, 1fr\)/u);
    // And the same seal geometry as .loading-seal / .loading-logo.
    expect(shellRules).toMatch(/\.boot-shell-seal\s*\{[^}]*width:\s*104px/u);
    expect(shellRules).toMatch(/\.boot-shell-seal\s*\{[^}]*background-size:\s*76px 76px/u);
    expect(styles).toMatch(/\.loading-seal\s*\{[^}]*width:\s*104px/u);
    expect(styles).toMatch(/\.loading-logo\s*\{[^}]*width:\s*76px/u);
  });

  it("reaches the logo through url(), which Vite rewrites to an emitted asset", () => {
    // img-src is 'self'. A background-image url() in a stylesheet is rewritten
    // by Vite to the hashed path it actually emits; a hand-written path in
    // index.html would not be, and would 404 in the packaged build.
    expect(declarations(bootShell)).toContain('background-image: url("./assets/logo-mark.svg")');
  });
});
