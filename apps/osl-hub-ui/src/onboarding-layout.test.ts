import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  RESUMABLE_ONBOARDING_ROUTES,
  resumeOnboardingRoute,
  type OnboardingResumeStorage,
} from "./onboarding-resume";
import { nextOnboardingRoute, ONBOARDING_SEQUENCE, previousOnboardingRoute } from "./onboarding-sequence";
import { onboardingPaintDecision } from "./ui-behavior";
import { onboardingPasswordRoleContent } from "./password-roles";
import { onboardingSendingMarkup } from "./onboarding-sending";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

const RESUME_STORAGE_KEY = "osl-onboarding-resume-v1";

/** Declarations only, so a rule quoted inside a comment never satisfies a check. */
function declarations(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//gu, "");
}

function fakeResumeStorage(seed: Record<string, string>): OnboardingResumeStorage {
  const items = new Map(Object.entries(seed));
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => { items.set(key, value); },
    removeItem: (key) => { items.delete(key); },
  };
}

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("clean onboarding sign in", () => {
  it("removes the redundant account-access strip", () => {
    expect(source).not.toContain("ACCOUNT ACCESS");
    expect(source).not.toContain('class="onboarding-brand"');
  });

  it("uses account state to make create, finish, or unlock the one clear primary action", () => {
    expect(source).toContain('const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired"');
    expect(source).toContain('partialIdentity ? "Finish setup"');
    expect(source).toContain('data-onboarding="${route}"');
    expect(source).toContain('class="signin-recovery" data-onboarding="import"');
    expect(source).not.toContain('class="button signin-create" data-onboarding="create"');
    expect(source).not.toContain("Sign in to OSL");
    expect(source).not.toContain("Welcome back");
    expect(source).not.toContain("Open your private OSL workspace on this device.");
    expect(source).not.toContain("Your service passwords stay on each service's own sign-in page.");
  });

  // Sign in and Create account are ONE component, by Liam's ruling. If they ever
  // become two copies they will drift, which is exactly what the handoff's
  // "sign-in keeps 8px, create is 2px" note would have caused.
  it("builds both entry screens from a single component", () => {
    expect(source).toContain("function entryScreenContent(label: string, icon: string, route: OnboardingRoute)");
    expect(source).toContain('entryScreenContent("Sign in", signinLockIcon(), "unlock")');
    expect(source).toContain('signinPlusIcon(), "create"');
    // One markup block, so the two screens cannot diverge.
    expect(source.match(/class="signin-lock-column"/g)).toHaveLength(1);
    expect(source.match(/class="signin-ghost-mark"/g)).toHaveLength(1);
  });

  it("uses one unified visual system across both entry screens", () => {
    // 2px on BOTH. The handoff proposed different radii per screen and left the
    // call to the design owner; he unified them.
    expect(styles).toMatch(/\.signin-unlock\s*\{[^}]*border-radius:\s*2\.2px/s);
    expect(styles).not.toMatch(/\.signin-unlock\s*\{[^}]*border-radius:\s*8px/s);
    // Spacing: mark -> button 10px, button -> link 16px.
    expect(styles).toMatch(/\.signin-ghost-mark[^}]*margin:\s*0 0 11px/s);
    expect(styles).toMatch(/\.signin-recovery\s*\{[^}]*margin-top:\s*17\.6px/s);
    // The link is the brand's body face, deliberately not the button's UI face.
    expect(styles).toMatch(/\.signin-recovery[^}]*Source Sans 3 Variable/s);
    // Both icons share one seat, so neither can drift from the right edge.
    expect(styles).toMatch(/\.signin-icon\s*\{[^}]*right:\s*15\.4px/s);
    // The plus turns a quarter turn on the same spring as the shackle.
    expect(styles).toMatch(/\.signin-plus[^}]*cubic-bezier\(\.34,\s*1\.56,\s*\.64,\s*1\)/s);
    expect(styles).toMatch(/hover \.signin-plus[^}]*rotate\(90deg\)/s);
    // No scale() on the column. Transforming it resamples rendered text and the
    // labels come out blurry -- 110% is baked into the real sizes instead.
    expect(styles).not.toMatch(/\.signin-lock-column[^}]*scale\(/s);
    // The lock needs a 24 box: its body reaches y=20 and the 2px stroke y=21,
    // so a 20 box clips the bottom edge off.
    expect(source).toContain('signin-icon signin-lock" viewBox="0 0 24 24"');
  });

  // 2026-08-06 redesign. The device-lock screen is mark, one action, one way
  // out. The handoff is explicit that the heading, explainer, divider and
  // footnote must not come back, so each is pinned as an absence here -- a
  // redesign that only adds assertions for what it added would let the old
  // furniture drift back in unnoticed.
  it("shows the returning user a bare lock screen: mark, one action, one way out", () => {
    expect(source).toContain('if (returning) return entryScreenContent(');
    expect(source).toContain('class="signin-card signin-lock-screen"');
    expect(source).toContain('class="signin-ghost-mark" src="${oslGhostMarkUrl}"');
    expect(source).toContain('entryScreenContent("Sign in", signinLockIcon(), "unlock")');
    expect(source).toContain('class="signin-recovery" data-onboarding="import"');
    expect(source).toContain("Use recovery phrase");
    expect(source).toContain('import oslGhostMarkUrl from "./assets/Ghost-white.svg"');
  });

  it("does not re-add the furniture the redesign removed", () => {
    expect(source).not.toContain("Unlock first to add another identity in Settings.");
    expect(source).not.toContain("Unlock this device to continue protecting your existing accounts");
    expect(source).not.toContain('returning ? "Sign in"');
    expect(source).not.toContain('returning ? "Unlock this device"');
    expect(source).not.toContain("signin-divider");
    expect(source).not.toContain("signin-new");
  });

  // The mark carries its own #080c0d field. If the window behind it is any
  // other value the mark stops reading as a mark and becomes a tile with a
  // visible edge, which is the one thing the handoff calls out by name.
  it("keeps the lock screen flush with the mark's own background", () => {
    expect(styles).toContain("--signin-bg: #080c0d");
    expect(styles).toContain("--signin-accent: #2ac0f0");
    expect(styles).toContain("--signin-border: #2a343a");
    expect(styles).toMatch(/\.signin-ghost-mark[^}]*border-radius:\s*23px/s);
    expect(styles).toMatch(/\.signin-unlock\s*\{[^}]*width:\s*321px/s);
    expect(styles).toMatch(/\.signin-lock-shackle[^}]*transform-origin:\s*16px 11px/s);
    expect(styles).toMatch(/\.signin-lock-shackle[^}]*cubic-bezier\(\.34,\s*1\.56,\s*\.64,\s*1\)/s);
  });

  // Protects the shape of the unlock screen: mark, ONE password field, ONE
  // action. Rebuilt 2026-08-08 against the real export (Sign In Final.dc.html):
  // the password step wears the sign-in entry skeleton -- sr-only heading,
  // ghost mark, password row with the eye INSIDE it, one outline submit with
  // the lock, quiet links. (The D80 rule this screen carries -- that no
  // alternate credential may be named in text, placeholder, sr-only label,
  // aria-label, title, autocomplete token or data- attribute -- is proven
  // against the RENDERED markup and the accessibility tree in
  // unlock-screen-single-credential.test.ts, which is where it belongs.)
  it("keeps password unlock to one field and one action", () => {
    expect(source).toContain('class="password-form unlock-form"');
    expect(source).toContain('class="signin-card signin-lock-screen signin-password-screen"');
    expect(source).not.toContain(">Enter your password</h1>");
    // The invented-spec chrome stays gone: no filled button, no boxed back
    // bar, no circular logo treatment on this screen -- and the heading is
    // for screen readers only, because canon shows no heading text at all.
    expect(source).not.toContain('<section class="unlock-card"');
    // Still exactly one credential row and one submit in the unlock branch.
    const start = source.indexOf('<section class="signin-card signin-lock-screen signin-password-screen"');
    const unlock = source.slice(start, source.indexOf("</section>`;", start));
    expect(unlock).not.toBe("");
    expect(unlock).toContain('<h1 id="route-heading" class="sr-only" tabindex="-1">Sign in</h1>');
    expect(unlock.match(/type="password"/gu) ?? []).toHaveLength(1);
    expect(unlock.match(/type="submit"/gu) ?? []).toHaveLength(1);
    expect(unlock).toContain('id="identity-password-submit" type="submit" disabled><span class="signin-unlock-label">Sign in</span>');
    expect(styles).toMatch(/\.unlock-form\s*\{\s*gap:\s*12px;/);
    expect(styles).toMatch(/\.unlock-form \.unlock-error:empty\s*\{\s*display:\s*none;/);
    // The eye is positioned inside the field's row, never floating beside it.
    expect(styles).toMatch(/\.signin-password-row \.password-eye\s*\{[\s\S]*?position:\s*absolute/);
  });

  it("uses a crisp accessible password visibility control", () => {
    const iconStart = source.indexOf("function passwordEyeIcon");
    const icon = source.slice(iconStart, source.indexOf("let services", iconStart));
    expect(icon).toContain('viewBox="0 0 20 20"');
    expect(icon).toContain('<circle cx="10" cy="10" r="2.25"/>');
    expect(styles).toMatch(/\.password-input-row \.password-eye\s*\{[\s\S]*?width:\s*44px;[\s\S]*?min-height:\s*44px;/);
    expect(styles).toMatch(/\.password-eye svg\s*\{[^}]*stroke-linecap:\s*round;/s);
  });

  it("keeps the welcome surface compact and centered", () => {
    expect(styles).toMatch(/\.onboarding-welcome\s*\{[^}]*width:\s*min\(440px/s);
    expect(styles).toMatch(/\.signin-card\s*\{[^}]*text-align:\s*center/s);
    expect(source).toContain('icons/icon-cyan.png');
  });

  it("keeps the custom titlebar unbranded and fully draggable beside accessible controls", () => {
    // desktopTitlebar() still renders the original full titlebar (its own
    // 44px row + dedicated drag strip) for bare-shell screens that have no
    // other header to dock into (onboarding, boot recovery, initial
    // loading) — see the ".app-frame.with-titlebar" modifier in styles.css.
    const titlebar = functionSource("desktopTitlebar", "desktopWindowControlsMarkup");
    expect(titlebar).toContain('class="desktop-drag-region" data-tauri-drag-region');
    expect(titlebar).not.toMatch(/>OSL<|<img|class="desktop-title"/);
    expect(titlebar).toContain('aria-label="Minimize"');
    expect(titlebar).toContain('id="window-maximize"');
    expect(titlebar).toContain('aria-label="Maximize"');
    expect(titlebar).not.toContain('id="window-fullscreen"');
    expect(titlebar).not.toContain('aria-label="Toggle fullscreen"');
    expect(titlebar).toContain('aria-label="Close"');
    expect(titlebar).toContain("activeNativeHostId");
    expect(titlebar.match(/<button id="window-/g) ?? []).toHaveLength(3);
    expect(styles).toMatch(/\.desktop-drag-region\s*\{[^}]*flex:\s*1 1 auto/s);
    expect(styles).toMatch(/\.window-controls button:disabled\s*\{[^}]*pointer-events:\s*none;/s);
  });

  it("docks the same window controls into the hub's own top row instead of a separate 44px strip", () => {
    // The hub route (renderWorkspace) has no separate desktop titlebar row:
    // the pale strip it used to render above the control row is gone. These
    // buttons render inline, docked into whichever top row the active hub
    // route already shows (workspace-header / home-command-bar /
    // guide-header / mullvad-host-header), via desktopWindowControlsMarkup()
    // and the ".desktop-top-row" wrapper built in renderWorkspace().
    const controls = functionSource("desktopWindowControlsMarkup", "bindDesktopTitlebar");
    expect(controls).not.toContain("desktop-titlebar");
    expect(controls).not.toContain("desktop-drag-region");
    expect(controls).toContain('aria-label="Minimize"');
    expect(controls).toContain('id="window-maximize"');
    expect(controls).toContain('aria-label="Maximize"');
    expect(controls).toContain('aria-label="Close"');
    expect(controls.match(/<button id="window-/g) ?? []).toHaveLength(3);
    const renderWorkspace = functionSource("renderWorkspace", "appLauncherStrip");
    expect(renderWorkspace).toContain('<div class="desktop-top-row shared-launcher-header-row" data-shared-launcher-header data-tauri-drag-region="deep">${trustedHeader()}${desktopWindowControlsMarkup()}</div>');
    expect(renderWorkspace).toContain('root.innerHTML = `<div class="app-frame"><div id="workspace-render-surface"></div></div>`;');
    expect(styles).toMatch(/\.app-frame\s*\{[^}]*grid-template-rows:\s*minmax\(0, 1fr\);/s);
    expect(styles).toMatch(/\.app-frame\.with-titlebar\s*\{[^}]*grid-template-rows:\s*44px minmax\(0, 1fr\);/s);
    // The docked row's height is one variable rather than a literal: the
    // controls used to be pinned at 44px while .workspace-header rendered 54
    // (48 in the QA shell), which left a notch beside them, and osl-hub's
    // TRUSTED_VERTICAL_RESERVE has to equal this exact height or the borrowed
    // native window is placed below the chrome with a dead band above it.
    // It is a floor, not a cap: two docked headers (.home-command-bar 72px,
    // .home-header 64px) are taller, and pinning the row to exactly this height
    // is what left the right end of those rows 18px short. The floor is what
    // TRUSTED_VERTICAL_RESERVE must equal, and .workspace-header -- the header
    // on the routes that actually host a borrowed native window -- is exactly
    // this height, so the floor still binds there.
    expect(styles).toMatch(/\.desktop-top-row\s*\{[^}]*min-height:\s*var\(--chrome-row-height\);/s);
    expect(styles).toMatch(
      /\.desktop-top-row\s*>\s*\.window-controls\s*\{[^}]*min-height:\s*var\(--chrome-row-height\);/s,
    );
    expect(styles).toMatch(/:root\s*\{[^}]*--chrome-row-height:\s*58px;/s);
    expect(styles).toMatch(/\.discord-qa-shell\s*\{\s*--chrome-row-height:\s*48px;/s);
  });

  it("paints the docked top row edge to edge so the header does not end in a seam", () => {
    // The row is a flex line of three items: the header stack, the optional
    // cleanup pill, and the window controls. The stack and the controls each
    // paint their own background; the pill's slot did not, so at 1440px wide
    // the header stopped dead around x=1128 and the page background showed
    // through to the controls, with the hairline under the row broken across
    // the same span. Every hub destination showed it.
    //
    // Asserted against declarations only: a rationale written in a comment must
    // never stand in for the rule. And it is asserted in the stylesheet, not in
    // the markup that emits the row -- the shipped CSP is `style-src 'self'`,
    // so a `style=` attribute carrying this would be dropped by the WebView
    // while a source-text assertion stayed green.
    const topRow = declarations(styles).match(/\n\.desktop-top-row \{([^}]*)\}/u)?.[1] ?? "";
    expect(topRow, ".desktop-top-row should be a top-level rule").not.toBe("");
    expect(topRow).toContain("background: var(--panel)");
    // A border-bottom here would consume a pixel of the row's content box, and
    // this row's height is what TRUSTED_VERTICAL_RESERVE must match, so the
    // hairline is drawn without taking layout space.
    expect(topRow).toContain("box-shadow: inset 0 -1px 0 var(--line)");
    expect(topRow).not.toMatch(/border-bottom/u);
    // and the row must not cap its own height, or the paint stops short of the
    // taller headers instead of short of the right edge -- the same seam turned
    // on its side, which is what shipped once the background was added.
    expect(topRow).toContain("align-items: stretch");
    expect(topRow).not.toMatch(/(?<!min-)height:/u);

    // The control block takes the row's height rather than setting its own.
    // .window-controls carries `height: 100%` for the bare-shell titlebar, and
    // any explicit cross size opts a flex item out of `align-items: stretch`.
    const controls = declarations(styles).match(/\n\.desktop-top-row > \.window-controls \{([^}]*)\}/u)?.[1] ?? "";
    expect(controls, ".desktop-top-row > .window-controls should be a top-level rule").not.toBe("");
    expect(controls).toContain("height: auto");
    // It paints neither: the row paints both across its whole width, and when
    // this block drew its own hairline it landed a pixel below the header's.
    expect(controls).not.toMatch(/background:/u);
    expect(controls).not.toMatch(/border-bottom:/u);
  });

  it("gives every setup step the same action row: Back, primary, then any skip beneath", () => {
    // Back/skip landed somewhere different on four consecutive steps -- above
    // the primary on `browser`, inline beside it on `mullvad`, below a loose
    // "Not now" on the two password steps. Order is decided by the sheet so no
    // step can order itself differently, and it is in the sheet because the
    // shipped CSP (`style-src 'self'`) drops inline styles.
    const css = declarations(styles);
    expect(css).toMatch(/\.onboarding-actions \.onboarding-back \{[^}]*order: -1/u);
    expect(css).toMatch(/\.onboarding-actions \.button\.primary \{[^}]*order: 0/u);
    const skip = css.match(/\.onboarding-actions \.text-button,\s*\n\.onboarding-actions \.browser-import-skip \{([^}]*)\}/u)?.[1] ?? "";
    expect(skip, "the skip-order rule should be a top-level rule").not.toBe("");
    expect(skip).toContain("order: 1");
    expect(skip).toContain("flex-basis: 100%");
    expect(css).toMatch(/\.onboarding-actions \{[^}]*flex-wrap: wrap/u);
    // `browser` stacked its whole row vertically, which put Back above the
    // primary button on the one step before the tour.
    expect(css).not.toContain("browser-import-actions-primary");
    expect(source).not.toContain("browser-import-actions-primary");

    // Steps that used to render their primary outside a shared action row.
    const pro = functionSource("proSetupContent", "tutorialContent");
    // 2026-08-06 redesign: the Pro step lost its action-row wrapper and its
    // solid button. Continue is the shared entry-screen button, Skip is the
    // shared quiet link, and Skip still follows Continue. The ORDER is what
    // this test was protecting, so that is what it still checks.
    expect(pro).toMatch(/class="signin-unlock pro-code-continue" type="submit">[^]*?Continue<\/span>[^]*?<button class="signin-recovery" id="skip-pro-setup"/u);
    expect(pro).not.toContain('<p class="eyebrow">Optional</p>');
    expect(pro).not.toContain('class="button primary" type="submit">Continue');
    // BOTH password steps were rebuilt from one shared function on 2026-08-06
    // (password-roles.ts): the submit sits inside the card and the escape sits
    // in a `.setup-footer.onboarding-actions` row underneath, which is the row
    // the sheet's order rules above govern and the row the global Back docks
    // into. So what has to hold for these two is not the old markup shape but
    // that, on EACH of them, the submit is still findable by the attribute the
    // binding uses and there is still a way out.
    for (const [role, next] of [["stealth", "burnpass"], ["burn", "pro"]] as const) {
      const rendered = onboardingPasswordRoleContent({
        role,
        configured: false,
        passwordEyeIcon: () => "",
        statusTag: () => "",
      });
      expect(rendered).toContain("data-onboarding-role-submit");
      // Submits the right form even though it is styled out of the shared row.
      expect(rendered).toMatch(new RegExp(`type="submit" form="setup-${role}-form" data-onboarding-role-submit`, "u"));
      expect(rendered).toContain('class="setup-footer onboarding-actions stealth-links"');
      expect(rendered).toContain(`data-skip-onboarding-password-role="${next}"`);
    }
    const burn = onboardingPasswordRoleContent({
      role: "burn",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    });
    // t15-b4: canSetOnboardingPasswordRole() REQUIRES `burnConfirmation`, so
    // without this input on the burn screen the burn password could never be
    // set at all -- the validator would refuse a value the form gave no way to
    // type. The rebuild kept it; this is what stops the next one dropping it.
    expect(burn).toContain('id="setup-burn-confirmation"');
    expect(burn).toContain('name="burnConfirmation"');
    const stealth = onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    });
    // ...and only burn has one. Stealth is not destructive and must not ask
    // anyone to type ERASE.
    expect(stealth).not.toContain("setup-burn-confirmation");
    expect(stealth).not.toContain("burnConfirmation");
    // The submit is no longer inside the form element, so the binding cannot
    // find it by walking the form's own subtree.
    expect(functionSource("bindOnboardingPasswordRole", "bindPasswordVisibility"))
      .toContain('document.querySelector<HTMLButtonElement>("[data-onboarding-role-submit]")');
    // Every step in the spine renders Back, `forward-secrecy` included.
    expect(functionSource("renderOnboarding", "onboardingContent")).toContain('"forward-secrecy"');
  });

  it("centres the stealth/burn 'Not now' escape hatch under its centred card", () => {
    // It is a <button>, so it is inline-block and pinned itself to the left
    // edge of the centred card above it on both password steps.
    const skip = declarations(styles).match(/\n\.onboarding-role-skip \{([^}]*)\}/u)?.[1] ?? "";
    expect(skip, ".onboarding-role-skip should be a top-level rule").not.toBe("");
    expect(skip).toContain("display: block");
    expect(skip).toContain("margin-inline: auto");
    // The class has to actually be on the control the steps render.
    expect(onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    })).toContain('class="text-button onboarding-role-skip"');
  });

  it("reflects the live maximized state on the maximize/restore control", () => {
    expect(source).toContain("async function refreshDesktopMaximizeControl");
    expect(source).toContain('getCurrentWindow().isMaximized()');
    expect(source).toContain("function applyMaximizeControlState");
    expect(source).toMatch(/applyMaximizeControlState[\s\S]*?maximized \? "Restore" : "Maximize"/);
    expect(source).toContain("desktopMaximizeListenerBound");
    expect(source).toContain("void refreshDesktopMaximizeControl();");
    const binding = functionSource("bindDesktopTitlebar", "renderOnboarding");
    expect(binding).toContain("appWindow.onResized(() => void refreshDesktopMaximizeControl())");
    expect(binding).toContain("if (!desktopMaximizeListenerBound)");
  });

  it("does not stack window-control listeners during no-op refreshes", () => {
    const binding = functionSource("bindDesktopTitlebar", "renderOnboarding");
    expect(binding).toContain('button.dataset.windowControlBound === "true"');
    expect(binding).toContain('button.dataset.windowControlBound = "true"');
  });
});

describe("fresh-account continuation", () => {
  it("persists and resumes every current post-account setup step without accepting legacy app routes", () => {
    const pending = functionSource("pendingOnboardingRoute", "beginServiceOnboarding");
    const renderOnboarding = functionSource("renderOnboarding", "onboardingContent");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    // T15-A8: the allow-list moved into ./onboarding-resume so `recovery` could
    // join it as a first-class resumable step. Assert the policy itself, not
    // the inlined comparisons it replaced.
    expect(pending).toContain("resumeOnboardingRoute(localStorage, onboardingResumeStorageKey)");
    for (const route of ["pro", "privacy", "defaults", "tor", "sending", "cover", "silent-visible", "visibility", "passwords", "burnpass", "mullvad", "browser", "tutorial"] as const) {
      expect(RESUMABLE_ONBOARDING_ROUTES).toContain(route);
      expect(resumeOnboardingRoute(fakeResumeStorage({ [RESUME_STORAGE_KEY]: route }), RESUME_STORAGE_KEY)).toBe(route);
    }
    for (const rejected of ["apps", "detected", "install"]) {
      expect(RESUMABLE_ONBOARDING_ROUTES as readonly string[]).not.toContain(rejected);
      expect(resumeOnboardingRoute(fakeResumeStorage({ [RESUME_STORAGE_KEY]: rejected }), RESUME_STORAGE_KEY)).toBeNull();
    }
    // A stale/unknown stored step is still cleared rather than carried around;
    // that now happens inside the resume policy module.
    const stale = fakeResumeStorage({ [RESUME_STORAGE_KEY]: "apps" });
    expect(resumeOnboardingRoute(stale, RESUME_STORAGE_KEY)).toBeNull();
    expect(stale.getItem(RESUME_STORAGE_KEY)).toBeNull();
    expect(renderOnboarding).toContain("persistCurrentOnboardingRoute()");
    expect(source).not.toContain('pendingOnboardingRoute() ?? "mullvad"');
    // The QA shell build swaps the default first-setup-step target ("pro" -> "sending")
    // via onboardingRouteForBuild, but the resumed-route precedence is unchanged.
    expect(bootstrap).toContain('pendingOnboardingRoute() ?? onboardingRouteForBuild("passwords")');
  });

  it("combines connected, browser-history, and remaining apps in one chooser", () => {
    const choice = functionSource("tutorialContent", "selectedNativeApps");
    expect(choice).toContain("Choose apps");
    expect(choice).toContain("Connected");
    expect(choice).toContain("Seen in your browser history");
    expect(choice).toContain("Other apps");
    expect(choice).toContain("groupOnboardingApps");
    expect(choice).toContain('data-onboarding-app-choice="${app.id}"');
    expect(choice).toContain("Nothing opens during setup");
    expect(choice).toContain('nativeCatalogBusy ? "Checking Windows…" : "Continue"');
  });

  // Protects: browser import reaches the combined chooser and then Home with
  // nothing wedged in between, and Back retraces it exactly. The tour used to
  // sit in that gap; since 2026-08-06 it does not, so the gap is asserted to
  // be empty rather than to contain it.
  it("routes browser import directly through the combined chooser to Home", () => {
    const branches = { detected: false, install: false };

    expect(nextOnboardingRoute("browser", branches)).toBe("apps");
    expect(previousOnboardingRoute("apps", branches)).toBe("browser");
    expect(nextOnboardingRoute("apps", branches)).toBeNull();
    expect(nextOnboardingRoute("browser", branches)).not.toBe("tutorial");
  });

  it("persists Home choices without installing, opening, or adopting native sessions", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const intent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    expect(intent).toContain("selectedOnboardingAppsStorageKey");
    expect(intent).toContain("selectedOnboardingApps");
    expect(intent).not.toContain("savedNativeApps");
    expect(intent).not.toContain("persistSavedAccountPreferences");
    expect(intent).not.toContain("installNativeApp");
    expect(intent).not.toContain("openNativeHostedApp");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?completeOnboarding\(\)/);
  });

  it("never infers native-app routing from an unknown catalog", () => {
    const completeness = functionSource("isCompleteNativeCatalog", "hasSelectedInstalledNativeApps");
    const chooser = functionSource("ensureNativeCatalogForAppChoice", "selectedNativeAppIntent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    // D-190. This used to pin `catalog.length === supportedNativeAppIds.size` and
    // `ids.size === supportedNativeAppIds.size`. Equality was an accident of the
    // two sets being the same size the day it was written: once `f02104ac0`
    // narrowed `supportedNativeAppIds` to `{discord}` while `list_native_apps` kept
    // returning all five `NATIVE_APPS` rows, the predicate was false for EVERY real
    // catalog and Continue became a no-op. Pinning the line kept this test green
    // through the whole regression, because the line never changed.
    //
    // Coverage is the invariant that was actually wanted, and it is pinned both
    // ways here. What the predicate DOES is proven by execution against the real
    // backend catalog in `onboarding-app-choice-deadend.test.ts`.
    expect(completeness).toContain("every((appId) => ids.has(appId))");
    expect(completeness).not.toContain("catalog.length === supportedNativeAppIds.size");
    expect(completeness).not.toContain("ids.size === supportedNativeAppIds.size");
    expect(chooser).toContain("hasSelectedNativeAppChoice()");
    expect(chooser).not.toContain("nativeAppsReady");
    expect(chooser).toContain('withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs)');
    expect(chooser).toContain("if (!isCompleteNativeCatalog(catalog))");
    expect(chooser).toContain("Couldn’t check Windows apps. Try again.");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?await ensureNativeCatalogForAppChoice\(\)[\s\S]*?persistCombinedHomeChoices\(\)/);
  });

  it("offers only supported native app choices while keeping unsupported helpers unreachable", () => {
    const installedChoice = functionSource("hasSelectedInstalledNativeApps", "hasSelectedMissingNativeApps");
    const nativeSelection = functionSource("selectedNativeAppIntent", "detectedAppsContent");
    const detected = functionSource("detectedAppsContent", "installMissingAppsContent");
    const discordChoices = functionSource("discordSessionModeChoices", "detectedAppsContent");
    expect(installedChoice).toContain('app.availability === "installed" && app.isolatedProfileAvailable');
    expect(nativeSelection).toContain('if (!nativeSessionModeConfirmed(nativeId)) return undefined;');
    expect(nativeSelection).toContain('if (existingNativeSessionRequested(appId)) return nativeId;');
    expect(nativeSelection).toContain('savedAccountMode === "use" && savedNativeApps.has(nativeId) && catalogApp?.availability === "installed" && catalogApp.isolatedProfileAvailable');
    expect(nativeSelection).toContain("onboardingServiceSetup");
    expect(nativeSelection).toContain("selectedOnboardingApps.has(appId)");
    expect(nativeSelection).toContain('savedAccountMode !== "clean"');
    expect(nativeSelection).toContain('nativeSessionModeForApp(nativeId) === "dedicated"');
    expect(detected).toContain('selectedNativeApps().filter((app) => app.availability === "installed")');
    expect(discordChoices).toContain('data-discord-session-mode="dedicated"');
    expect(discordChoices).toContain('data-discord-session-mode="existingSession"');
    expect(discordChoices).toContain('"Use existing account"');
    expect(discordChoices).toContain(">Use separate account</button>");
    expect(discordChoices).toContain('role="group"');
    expect(discordChoices).not.toContain('role="radio"');
    expect(detected).toContain('nativeSessionModeSettingChoices("discord", "Discord")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("telegram", "Telegram")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("signal", "Signal")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("whatsapp", "WhatsApp")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("outlook", "Outlook")');
    expect(functionSource("selectedNativeApps", "hasSelectedNativeAppChoice")).toContain("supportedNativeAppIds.has(app.id)");
    expect(source).toContain('if (supportedNativeAppIds.has(app.id as NativeAppId))');
    expect(source).toContain("A separate ${app.displayName} app account is unavailable");
    expect(functionSource("serviceGuideContent", "settingsContent")).toContain("supportedNativeAppIds.has(activeHomeAppId as NativeAppId)");
    expect(functionSource("serviceGuideContent", "settingsContent")).not.toMatch(/activeHomeAppId === "telegram"[\s\S]*?telegramSessionModeChoices\(\)/);
    expect(source).not.toMatch(/const sessionChoices = onboardingServiceSetup[\s\S]*?\? ""/);
    expect(source).not.toContain("Uses your signed-in ${name} window without copying its session.");
    expect(source).not.toContain("${name} stays outside OSL capture protection.");
  });

  it("turns one app choice into a persisted Home tile without opening it", () => {
    const defaultIntent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(binding).toMatch(/data-onboarding-app-choice[\s\S]*?selectedOnboardingApps\.add\(appId\)/);
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?ensureNativeCatalogForAppChoice\(\)[\s\S]*?persistCombinedHomeChoices\(\)/);
    expect(defaultIntent).toContain("selectedOnboardingAppsStorageKey");
    expect(defaultIntent).not.toContain("savedNativeApps");
    expect(binding).not.toMatch(/#continue-app-choice[\s\S]*?openNativeHostedApp/);
  });

  it("records an explicit empty app choice instead of treating it as legacy no-preference state", () => {
    const persistence = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    const workspace = functionSource("workspaceContent", "peopleListMarkup");
    expect(persistence).toContain("hasExplicitOnboardingAppSelection = true");
    expect(workspace).toContain("hasExplicitOnboardingAppSelection || rememberedHomeApps.size");
    expect(workspace).toMatch(/hasExplicitOnboardingAppSelection \|\| rememberedHomeApps\.size[\s\S]*?launchableHomeApps\.filter/);
  });

  it("does not open each selected service during fresh setup", () => {
    const apps = functionSource("tutorialContent", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(apps).not.toContain("Connect your apps");
    expect(apps).not.toContain("Open selected app");
    expect(apps).not.toContain("data-connect-app-choice");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?await completeOnboarding\(\)/);
  });

  it("offers multi-source selection behind one protected importer contract", () => {
    const tutorial = functionSource("tutorialContent", "selectedNativeApps");
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(tutorial).not.toContain("data-browser-import");
    expect(browser).toContain("Find saved browser accounts");
    expect(browser).toContain("Optional.");
    expect(browser).toContain("browserLogo(profile.browserId)");
    expect(browser).toContain('<fieldset class="browser-detected-sources"');
    expect(browser).toContain("Choose saved browser areas");
    expect(browser).not.toContain("Import all detected browsers");
    expect(browser).not.toContain("data-browser-select-all");
    expect(browser).toContain('data-browser-profile="${escapeHtml(key)}"');
    expect(browser).toContain("selectedBrowserProfileKeys.has(key)");
    expect(browser).toContain("Read only a bounded history snapshot after this consent");
    expect(browser).toContain('id="import-saved-accounts"');
    expect(browser).not.toContain('id="install-firefox"');
    expect(browser.match(/id="import-saved-accounts"/g)).toHaveLength(1);
    expect(browser).toContain('id="continue-browser-import" type="button" ${browserImportBusy || browserImportCancelling ? "disabled" : ""}>${secondaryLabel}');
    expect(browser).toContain('browserImportBusy ? "Wait for scan..." : "Not now"');
    expect(browser).not.toContain("Manual export");
    expect(browser).not.toContain("Prepare export in");
    expect(browser).not.toContain("How it works");
    expect(browser).toContain('selectionReady ? "Check selected" : "Choose areas"');
    expect(browser).not.toContain("Import selected · pending");
    expect(browser).toContain("OSL never reads browser databases before consent.");
    expect(browser).toContain("The encrypted account hints were saved and verified.");
    expect(browser).toContain("browserImportQueueIndex + 1");
    expect(browser).not.toContain("Done with");
    expect(browser).not.toMatch(/Finish \$\{escapeHtml\(currentName\)\}/);
    expect(browser).toContain('class="button primary" id="import-saved-accounts"');
    expect(browser.match(/class="button primary"/g)).toHaveLength(1);
    expect(binding).toContain("selectedBrowserProfileKeys.size === 0 || browserImportBusy");
    expect(binding).toContain("browserImportQueue = selectedProfiles.map((profile) => profile.browserId)");
    expect(binding).toContain('querySelectorAll<HTMLInputElement>("[data-browser-profile]")');
    expect(binding).toContain("browserProfiles.some((profile) => browserProfileKey(profile) === key)");
    expect(binding).not.toContain('querySelector<HTMLInputElement>("[data-browser-select-all]")');
    expect(binding).toContain("for (let index = 0; index < selectedProfiles.length; index += 1)");
    expect(binding).toContain("grantBrowserProfileConsent(");
    expect(binding).toContain("scanConsentedBrowserProfile(");
    expect(binding).toContain("loadDetectedBrowserFootprint(scanReceipts)");
    expect(binding).toContain("applyNativeBrowserFootprint(hydration)");
    expect(binding).toContain("if (runEpoch !== browserImportRunEpoch) return");
    expect(binding).toMatch(/grantBrowserProfileConsent\([\s\S]*?scanConsentedBrowserProfile\([\s\S]*?loadDetectedBrowserFootprint\(scanReceipts\)[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?browserImportRunEpoch \+= 1[\s\S]*?localStorage\.removeItem\(pendingKey\)/);
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).not.toContain("openBrowserImport(");
    expect(binding).not.toContain("beginProtectedBrowserImport(");
    expect(binding).not.toContain("finishProtectedBrowserImport(");
    expect(binding).not.toContain("window.confirm");
    expect(source).not.toContain("browserPasswordImportOptIn");
    expect(source).not.toContain("data-browser-password-import");
    expect(source).not.toContain("browser-password-import-opt-in");
  });

  it("keeps the normal-profile default browser behind explicit truthful consent", () => {
    const choices = functionSource("browserSessionModeChoices", "detectedAppsContent");
    const binding = functionSource("bindSavedAccountControls", "bindBrowserImportControls");
    expect(source).toContain('let useDefaultBrowserCompanion = localStorage.getItem("osl-default-browser-companion-v1") === "true"');
    expect(choices).toContain('data-browser-session-mode="isolatedOsl"');
    expect(choices).toContain('data-browser-session-mode="existingBrowser"');
    expect(choices).toContain("Browser account");
    expect(choices).toContain("New account");
    expect(choices).not.toContain("Use existing account");
    expect(choices).not.toContain("Use separate account");
    expect(binding).toContain('requested !== "isolatedOsl" && requested !== "existingBrowser"');
    expect(binding).toContain('useDefaultBrowserCompanion = requested === "existingBrowser"');
    expect(binding).toContain('localStorage.setItem("osl-default-browser-companion-v1", String(useDefaultBrowserCompanion))');
    expect(source).toContain('loadDefaultBrowserCompanionStatus(), "Check default browser"');
    expect(source).toContain("defaultBrowserCompanionStatus = currentBrowserCompanionStatus");
    expect(source).toContain("await detachDefaultBrowserCompanion().catch(() => undefined)");
    expect(source).toContain("resizeDefaultBrowserCompanion()");
    expect(source).toContain("focusDefaultBrowserCompanion()");
  });

  it("places the combined app choice immediately after browser import", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    const entry = functionSource("enterCombinedAppChoice", "persistCombinedHomeChoices");
    expect(browser).not.toContain("unsupported");
    expect(browser).not.toContain("unavailable in this build");
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(binding).toContain("loadDetectedBrowserFootprint(scanReceipts)");
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).not.toContain("#install-firefox");
    expect(binding).not.toContain("window.confirm");
    expect(entry).not.toContain("selectedOnboardingApps.add");
    expect(entry).not.toContain("selectedOnboardingAppsStorageKey");
  });

  it("turns selected browser areas into one bounded attended scan", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    expect(browser).toContain("browserImportBusy");
    expect(browser).toContain("Checking selected areas...");
    expect(browser).toContain("browserImportFailureNotice");
    expect(browser).toContain('role="alert"');
    expect(binding).toContain("const selectedProfiles = browserProfiles.filter");
    expect(binding).toContain("selectedBrowserProfileKeys.clear()");
    expect(binding).toContain("const grant = await grantBrowserProfileConsent(");
    expect(binding).toContain("const receipt = await scanConsentedBrowserProfile(");
    expect(binding).toContain('browserImportFailureNotice = localActionError(failure, "Saved browser account check did not finish")');
    expect(binding.indexOf("selectedBrowserProfileKeys.clear()")).toBeLessThan(binding.indexOf("const grant = await grantBrowserProfileConsent("));
    expect(binding.indexOf("const grant = await grantBrowserProfileConsent(")).toBeLessThan(binding.indexOf("const receipt = await scanConsentedBrowserProfile("));
  });

  it("refreshes browser readiness when the import page is entered or resumed", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const refresh = functionSource("refreshBrowserImportReadiness", "importIdentityForm");
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const advance = functionSource("advanceOnboardingConnection", "ensureNativeCatalogForAppChoice");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(browser).toContain("browserReadinessBusy");
    expect(refresh).toContain('listBrowserProfilesForConsent(),');
    expect(refresh).toContain('"List saved browser areas"');
    expect(refresh).toContain("setBrowserProfiles(profiles)");
    expect(refresh).toContain("selectedBrowserProfileKeys = new Set(");
    expect(continuation).toContain("advanceOnboardingConnection(completedAppId)");
    expect(advance).toContain("void completeOnboarding()");
    expect(continuation).not.toContain("clearServiceOnboardingResume()");
    expect(bootstrap).toMatch(/onboardingRoute === "browser"[\s\S]*?refreshBrowserImportReadiness\(\)/);
  });

  it("keeps prior completed-import state scoped to the active OSL identity", () => {
    const key = functionSource("activeBrowserAccountsReadyStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(key).toContain("core.readiness.activeOslUserId");
    expect(key).toContain("encodeURIComponent(owner)");
    expect(key).toContain("return owner ?");
    expect(refresh).toContain("const activeOwner = core.readiness.activeOslUserId");
    expect(refresh).toContain("browserFootprintImports = []");
    expect(binding).toMatch(/for \(let index = 0; index < selectedProfiles\.length; index \+= 1\)[\s\S]*?loadDetectedBrowserFootprint\(scanReceipts\)/);
    expect(binding).toMatch(/loadDetectedBrowserFootprint\(scanReceipts\)[\s\S]*?applyNativeBrowserFootprint\(hydration\)/);
    expect(source).not.toMatch(/localStorage\.setItem\(savedAccountsReadyStorageKey\s*,/);
  });

  it("clears legacy pending import state without resuming an unmanaged browser", () => {
    const pendingKey = functionSource("activeBrowserImportPendingStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    expect(pendingKey).toContain("core.readiness.activeOslUserId");
    expect(pendingKey).toContain("encodeURIComponent(owner)");
    expect(pendingKey).toContain("browserImportPendingStorageKey");
    expect(refresh).not.toContain("browserMigrationAwaitingConfirmation");
    expect(source).toMatch(/function commitRender[\s\S]*?refreshActiveBrowserAccountsReady\(\)/);
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?localStorage\.removeItem\(pendingKey\)/);
    expect(source).not.toMatch(/localStorage\.setItem\(browserImportPendingStorageKey\s*,/);
  });

  it("returns from each service to the remaining app queue before completion", () => {
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const workspace = functionSource("bindWorkspace", "ttlSeconds");
    const finishStart = workspace.indexOf('querySelector("#service-guide-finish")');
    const exitStart = workspace.indexOf('querySelector("#service-guide-exit")');
    const nativeBackStart = workspace.indexOf('querySelector("#native-app-back")');
    expect(finishStart).toBeGreaterThanOrEqual(0);
    expect(exitStart).toBeGreaterThan(finishStart);
    expect(nativeBackStart).toBeGreaterThan(exitStart);
    expect(continuation).toContain("advanceOnboardingConnection(completedAppId)");
    expect(workspace.slice(finishStart, exitStart)).toContain("advanceOnboardingConnection(activeHomeAppId)");
    expect(workspace.slice(exitStart, nativeBackStart)).toContain("clearServiceOnboardingResume()");
    expect(functionSource("completeSixStepOnboarding", "completeOnboarding")).toContain("clearServiceOnboardingResume()");
  });

  it("shows the recovery title without the removed grey subtitle", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(recovery).toContain("Save your recovery kit");
    expect(recovery).not.toContain("OSL cannot retrieve these later");
    expect(recovery).not.toContain('class="compact-lead"');
  });

  it("uses the shared centred layout for the empty recovery screen", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(recovery).toMatch(/No recovery secret is available[\s\S]*?\/section>/);
    expect(recovery).toContain('class="onboarding-centered-step recovery-empty"');
    expect(recovery).toContain('id="recovery-no-secret-continue"');
    expect(recovery).not.toContain('data-onboarding="pro"');
    expect(binding).toMatch(/#recovery-no-secret-continue[\s\S]*?applyRecoveryKitAction\(\{ kind: "continue" \}\)[\s\S]*?onboardingRoute = onboardingRouteForBuild\("passwords"\)/);
    expect(styles).toMatch(/\.onboarding-centered-step\s*\{[^}]*width:\s*min\(440px,\s*100%\);[^}]*margin:\s*auto;[^}]*text-align:\s*center;/s);
  });

  it("continues from saved recovery material through the word check into optional Pro setup", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(recovery).toContain('id="copy-recovery-kit"');
    expect(recovery).toContain('recoverySavedAcknowledged ? "checked" : ""');
    // Continue is held until the box is ticked. aria-disabled rides along so the
    // held state is announced, not just drawn at 35% opacity.
    expect(recovery).toContain('recoverySavedAcknowledged ? "" : "disabled aria-disabled');
    expect(binding).toMatch(/#copy-recovery-kit[\s\S]*?navigator\.clipboard\.writeText\(kit\)[\s\S]*?Recovery kit copied — save it, then confirm below/);
    expect(binding).not.toMatch(/#copy-recovery-kit[\s\S]*?recoverySavedAcknowledged = true/);
    // T15-A7: the checkbox and Continue now go through the recovery-kit
    // reducer, which is what makes "saved" and "not saved" a state the app can
    // still see after a restart instead of a module-local boolean.
    expect(binding).toMatch(/recoverySaved\?\.addEventListener\("change"[\s\S]*?applyRecoveryKitAction\(\{ kind: "set-saved-acknowledged", acknowledged: recoverySaved\.checked \}\)[\s\S]*?recoveryContinue\.disabled = !recoverySavedAcknowledged/);
    expect(binding).toMatch(/#recovery-continue[\s\S]*?recoverySavedAcknowledged[\s\S]*?onboardingRoute = "recovery-check"/);
    // The saved acknowledgement is committed only after the native word check
    // passes, and the owner-reviewed password ordering still follows that gate.
    expect(source).toMatch(/#recovery-word-check-continue[\s\S]*?recoveryWordCheckContinueDisabled\(recoveryWordCheckState\)[\s\S]*?applyRecoveryKitAction\(\{ kind: "continue" \}\)[\s\S]*?onboardingRoute = pendingOnboardingRoute\(\) \?\? onboardingRouteForBuild\("passwords"\)/);
  });

  it("starts every recovery screen unacknowledged and clears recovery state on full cleanup", () => {
    const password = functionSource("bindPasswordForm", "bindImportForm");
    const imported = functionSource("bindImportForm", "continueOnboardingFromService");
    const burn = functionSource("executeBurn", "ttlSeconds");
    expect(password).toMatch(/recoveryBundle = \{[\s\S]*?recoverySavedAcknowledged = false;[\s\S]*?onboardingRoute = "recovery"/);
    expect(imported).toMatch(/recoveryBundle = \{[\s\S]*?recoverySavedAcknowledged = false;[\s\S]*?onboardingRoute = "recovery"/);
    expect(burn).toMatch(/localStorage\.clear\(\);[\s\S]*?recoveryBundle = null;[\s\S]*?recoverySavedAcknowledged = false;/);
  });

  it("drops the recovery next-steps cards without losing the Mullvad offer", () => {
    // 2026-08-06 restyle. The Mullvad and Android cards left this screen. Android
    // was a coming-soon, which the owner banned; Mullvad is a real offer, so the
    // rule is that it survived as its own step rather than being deleted with the
    // card it happened to sit in.
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(functionSource("recoveryKitStateNow", "applyRecoveryKitAction"))
      .toContain("captureProven: recoveryCaptureGate.canRender()");
    expect(recovery).toMatch(/visibleRecoverySecrets\(state\)[\s\S]*?recoveryKitSecretCardsMarkup/);
    expect(source).not.toContain("secure-recovery-next-steps");
    expect(source).not.toContain("Android device");
    expect(styles).not.toContain(".secure-recovery-next-steps");
    expect(source).toContain('if (onboardingRoute === "mullvad") return mullvadSetupContent();');
  });

  // Protects: every setup step gets the shared Back row and there is no
  // "skip the rest of setup" escape anywhere. "tutorial" left this list on
  // 2026-08-06 with the tour -- it renders its own Back (see
  // onboarding-tour.test.ts), so the shared row must NOT dock into it, and the
  // absence is asserted so re-adding it cannot ship two Backs again.
  it("keeps setup sequential without a global completion shortcut", () => {
    const onboardingRender = functionSource("renderOnboarding", "onboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingRender).toContain('id="onboarding-back"');
    expect(onboardingRender).not.toContain('id="skip-onboarding"');
    expect(onboardingRender).not.toContain("Skip · manual setup");
    expect(onboardingRender).toContain('["pro", "forward-secrecy", "privacy", "defaults", "tor", "sending", "cover", "silent-visible", "passwords", "burnpass", "browser", "detected", "install", "apps", "mullvad"]');
    expect(onboardingRender).toContain('["pro", "forward-secrecy", "privacy", "defaults", "tor", "sending", "cover", "visibility", "passwords", "burnpass", "browser", "detected", "install", "apps", "mullvad"]');
    expect(onboardingRender).not.toContain('"tutorial"');
    expect(onboardingRender).not.toContain('"scrub"].includes(onboardingRoute)');
    expect(binding).not.toContain('document.querySelector("#skip-onboarding")');
    expect(binding).toContain('document.querySelector("#onboarding-back")?.addEventListener("click"');
  });

  it("completes after persisting the single chooser without opening apps", () => {
    const apps = functionSource("tutorialContent", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(apps).toContain('id="continue-app-choice"');
    expect(apps).not.toContain('id="continue-connect-app"');
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?await completeOnboarding\(\)/);
  });

  it("persists only non-sensitive chooser state before Home", () => {
    const intent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    expect(intent).toContain("selectedOnboardingAppsStorageKey");
    expect(intent).not.toContain("savedNativeApps");
    expect(intent).not.toContain("persistSavedAccountPreferences");
    expect(intent).not.toContain("account");
    expect(intent).not.toContain("password");
  });

  it("uses the approved order and defers Scrub until after onboarding", () => {
    const indexOf = (route: (typeof ONBOARDING_SEQUENCE)[number]): number => ONBOARDING_SEQUENCE.indexOf(route);

    // These are the durable dependencies of the setup spine. New, independent
    // setup screens may be inserted without making this test a refactor tripwire.
    expect(indexOf("privacy")).toBeLessThan(indexOf("tor"));
    expect(indexOf("tor")).toBeLessThan(indexOf("defaults"));
    expect(indexOf("tor")).toBeLessThan(indexOf("sending"));
    expect(indexOf("defaults")).toBeLessThan(indexOf("sending"));
    expect(indexOf("sending")).toBeLessThan(indexOf("cover"));
    expect(indexOf("cover")).toBeLessThan(indexOf("silent-visible"));
    // 2026-08-08, owner's review (UI-FEEDBACK.txt): the stealth and burn
    // password steps come IMMEDIATELY BEFORE the Pro code.
    expect(indexOf("passwords")).toBeLessThan(indexOf("burnpass"));
    expect(indexOf("burnpass")).toBeLessThan(indexOf("pro"));
    expect(indexOf("burnpass")).toBe(indexOf("pro") - 1);
    expect(indexOf("browser")).toBeLessThan(indexOf("detected"));
    // 2026-08-06: the tour left the first-run spine on the owner's instruction.
    // The route and its steps still exist for Settings -> About to replay; what
    // must not come back is walking a new person through it before they have
    // used the app once.
    expect(ONBOARDING_SEQUENCE).not.toContain("tutorial");
    expect(ONBOARDING_SEQUENCE).not.toContain("scrub");
    expect(nextOnboardingRoute(ONBOARDING_SEQUENCE.at(-1)!, { detected: true, install: true })).toBeNull();
  });

  it("completes first run into the useful Balanced default", () => {
    const normalizer = functionSource("balancedFirstRunSetup", "completeSixStepOnboarding");
    const completion = functionSource("completeSixStepOnboarding", "completeOnboarding");
    const wrapper = functionSource("completeOnboarding", "bindPasswordForm");
    expect(normalizer).toContain("const sendMode = state.sendMode");
    expect(normalizer).not.toContain('state.sendMode === "manual" ? "clipboard" : state.sendMode');
    expect(normalizer).toContain('placementMode: "atomic"');
    expect(normalizer).toContain("needsRiskAcceptance(sendMode) && state.acceptedRisk && state.acceptedRiskForMode === sendMode");
    expect(completion).toContain("if (!canCompleteSetup(completedSetup)) throw new Error");
    expect(completion).toContain("setup = completedSetup");
    expect(completion).toContain("saveOnboardingPreferences({ onboardingComplete: true, setup, coverInsertion, showPlaintextPreview: true, windowCaptureEnabled, forwardSecrecyMode })");
    expect(completion).toContain("saveOnboardingPreferences({ onboardingComplete: true, setup, showPlaintextPreview: true, windowCaptureEnabled, rnWirePolicyRequested, forwardSecrecyMode })");
    expect(completion).toContain("onboardingComplete = true");
    expect(completion).toContain("clearServiceOnboardingResume()");
    expect(completion).toContain("resetOnboardingBranch()");
    expect(completion).toContain("resetOnboardingConnections()");
    expect(completion).toContain("await refreshIdentityScopedState()");
    expect(completion).toContain("nativeApps = await loadNativeApps().catch(() => nativeApps)");
    expect(completion).toContain('route = "home"');
    expect(completion).toContain("clearPrivacyScanState()");
    expect(wrapper).toMatch(/try \{[\s\S]*?await completeSixStepOnboarding\(\);[\s\S]*?\} catch/);
  });

  it("offers Pro activation after fresh account creation without storing the code in the renderer", () => {
    const content = functionSource("proSetupContent", "tutorialContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const activation = functionSource("activatePro", "requestClearProActivation");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(content).toContain("Enter Pro code");
    expect(content).toContain('id="activation-form"');
    expect(content).toContain('id="continue-pro-ready"');
    // One ready screen, not two: its Continue goes through the pro-onboarding
    // seam (continueFromProOnboarding -> forward-secrecy), never a hard link.
    expect(content).not.toContain('data-onboarding="sending"');
    expect(binding).toContain('"#activation-form"');
    expect(activation).toContain("validateHubActivationCode(activationCode)");
    expect(activation).toContain("proOnboardingReadyResult = true");
    expect(activation).toContain('onboardingRoute === "pro"');
    expect(content).not.toMatch(/localStorage|sessionStorage/);
    expect(bootstrap).toContain('onboardingRoute = "welcome"');
  });

  it("makes the optional Pro Skip control explicitly advance to privacy", () => {
    const content = functionSource("proSetupContent", "tutorialContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(content).toContain('id="skip-pro-setup"');
    expect(binding).toMatch(/#skip-pro-setup[\s\S]*?addEventListener\("click"[\s\S]*?continueFromProOnboarding\("skipped"\)\.route[\s\S]*?render\(\)/);
  });

  // Protects the claim boundary, which is the whole point of this screen: OSL
  // HANDS OFF to Mullvad and reports only what is installed. It may not imply
  // it has tunnel access, carries traffic, or controls the VPN. The screen was
  // rebuilt on 2026-08-06 into one status card whose dot carries the state, so
  // the wording moved ("Optional network privacy", "found session") while
  // every id and every forbidden claim stayed exactly where they were.
  it("offers an optional fixed Mullvad handoff without claiming tunnel access", () => {
    const content = functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(content).toContain("Optional network privacy");
    expect(content).toContain('id="install-mullvad"');
    expect(content).toContain('id="found-session-mullvad"');
    expect(content).toContain("Found session");
    expect(content).toContain("install");
    // Canon Mullvad.dc.html names the quiet way out "Skip", not "Not now".
    expect(content).toContain("Skip");
    expect(content).toContain('id="continue-mullvad"');
    expect(content).toContain('id="skip-mullvad"');
    expect(content).not.toMatch(/mullvad-connected|mullvad-autostart|refresh-mullvad|Mullvad pixels|does not copy or read/);
    // It says only what is INSTALLED -- one line per availability state, and
    // the state itself is carried by the card, not by a sentence.
    expect(content).toContain("Mullvad is installed on this device");
    expect(content).toContain("Mullvad is not installed. Windows can install it for you");
    expect(content).toContain('data-mullvad-state="${state}"');
    // ...and never a claim about the tunnel, the traffic, or being connected.
    // Broader than the exact strings that used to be banned, so a reworded
    // version of the same claim is caught too.
    for (const claim of [
      /\btunnel/iu,
      /\bconnected\b/iu,
      /\bencrypt/iu,
      /your traffic/iu,
      /through OSL/iu,
      /OSL (?:routes|protects|connects|secures)/iu,
      /\bVPN is\b/iu,
    ]) {
      expect(content, `the Mullvad screen must not claim ${claim.source}`).not.toMatch(claim);
    }
    expect(binding).toContain("openMullvadInstallPage()");
    expect(binding).toContain("confirmMullvadFoundSession()");
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?continueMullvadSetup\(\)/);
    expect(binding).toMatch(/#skip-mullvad[\s\S]*?skipMullvadSetup\(\)/);
  });

  it("keeps Mullvad installation and hosting behind one setup action", () => {
    const action = functionSource("runMullvadSetupAction", "bindPasswordForm");
    expect(action).toMatch(/installMullvad\(\)[\s\S]*?Date\.now\(\) \+ 180_000/);
    expect(action).toMatch(/loadMullvadStatus\(\)[\s\S]*?hostMullvadUntilReady\(/);
    expect(action).toContain('hostMullvadUntilReady("Open Mullvad inside OSL")');
    const guardedHost = functionSource("hostMullvadWithDeadline", "hostMullvadUntilReady");
    expect(guardedHost).toContain("label, 30_000");
    expect(guardedHost).toMatch(/hostAttempt\.then[\s\S]*?restoreMullvadWindow\(\)/);
    const readinessRetry = functionSource("hostMullvadUntilReady", "runMullvadSetupAction");
    expect(readinessRetry).toContain('["appNotInstalled", "existingSessionUnavailable", "windowOperationRejected"]');
    expect(readinessRetry).toContain("Date.now() < deadline");
    expect(source).toMatch(/async function validateNativeSurfaces[\s\S]*?hostMullvadWithDeadline\("Reopen Mullvad"\)[\s\S]*?mullvadWindowHosted = true/);
    expect(action).not.toContain("check again when it finishes");
    expect(functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup")).toContain('class="mullvad-setup-notice" role="status"');
  });

  it("refreshes Mullvad after an unfinished setup is unlocked", () => {
    const passwordBinding = functionSource("bindPasswordForm", "bindImportForm");
    expect(passwordBinding.match(/onboardingRoute === "mullvad"\) void refreshMullvadSetup\(\)/g)).toHaveLength(2);
  });

  // Protects: onboarding offers exactly the send modes the send model has -- no
  // fewer (a mode the app supports but the setup hides is a mode the owner cannot
  // consent to) and no more -- with the honest limit stated on the same screen,
  // and without the deleted stepper that animated only one of them.
  it("offers exactly the sending choices the send model has", () => {
    const content = onboardingSendingMarkup({ mode: "manual", riskAccepted: false, captureEnabled: false, captureApplied: false });
    expect([...content.matchAll(/data-send-mode="([^"]+)"/gu)].map((match) => match[1]))
      .toEqual(["manual", "clipboard", "double", "single"]);
    expect(content).toContain('id="route-heading"');
    expect(content).not.toContain("Highest risk");
    expect(content).toMatch(/cannot prove where it is sending[^<]*sends nothing/iu);
    expect(source).not.toContain("manualSendingAnimationMarkup");
    expect(source).not.toContain('step(1, "Write")');
  });

  it("delegates cover insertion to its own module and keeps the retired markup out", () => {
    // 2026-08-06 restyle. The screen moved to src/onboarding-cover.ts; what is
    // pinned here is that main.ts no longer carries a second copy of it.
    const content = functionSource("coverDraftSetupContent", "onboardingPasswordRoleContent");
    expect(content).toContain("onboardingCoverMarkup(coverInsertion)");
    expect(content).not.toContain("cover-atomic-preview");
    expect(content).not.toContain("cover-typing-preview");
    expect(content).not.toContain("LOOKS GOOD");
  });

  // Protects: setup only collects passwords that are actually wired, and it never
  // claims screen-capture resistance it does not have -- it says plainly when the
  // protection is off, and when this device cannot enforce it at all.
  it("collects only wired password roles and exposes only real capture resistance", () => {
    const stealthPassword = onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    });
    const burnPassword = onboardingPasswordRoleContent({
      role: "burn",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    });
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    // The capture section moved onto the sending screen as one row, so the honesty
    // rule it carried is checked against all three states that row can be in.
    const capture = (captureEnabled: boolean, captureApplied: boolean): string =>
      onboardingSendingMarkup({ mode: "manual", riskAccepted: false, captureEnabled, captureApplied });
    const off = capture(false, false);
    const unsupported = capture(true, false);
    const applied = capture(true, true);
    expect(stealthPassword).toContain("Stealth password");
    expect(burnPassword).toContain("Burn password");
    expect(stealthPassword).toContain('data-onboarding-password-role="stealth"');
    expect(stealthPassword).toContain("Current password");
    expect(stealthPassword).toContain("Set password");
    for (const state of [off, unsupported, applied]) {
      expect(state).toContain('id="window-capture-enabled"');
      expect(state).toContain('type="checkbox"');
      expect(state).not.toContain("Decrypt display");
      expect(state).not.toContain("Unavailable during setup");
      expect(state).not.toContain('id="decrypt-display"');
    }
    // Three states, three different true sentences. Only the state the platform
    // is actually enforcing may claim the protection is on; the other two say the
    // screen is exposed, including the one where the device cannot enforce it at
    // all -- a switch that is ON but unenforced must never read as protection.
    const note = (markup: string): string => {
      const found = /<small>([^<]+)<\/small>\s*<\/span>\s*<span class="snd-capture-control"/u.exec(markup);
      expect(found, "the capture row must state its status").not.toBeNull();
      return found![1]!;
    };
    expect(new Set([note(off), note(unsupported), note(applied)]).size).toBe(3);
    expect(note(off)).toMatch(/^off\b/iu);
    expect(note(off)).toMatch(/can be captured/iu);
    expect(note(unsupported)).toMatch(/device cannot/iu);
    expect(note(unsupported)).toMatch(/can be captured/iu);
    expect(note(applied)).toMatch(/active on this device/iu);
    expect(note(applied)).not.toMatch(/can be captured|cannot|not active|off\b/iu);
    expect(binding).toContain("setScreenshotProtection(windowCaptureEnabled)");
  });

  it("advances password-role setup only after an explicit valid form submission", () => {
    const binding = functionSource("bindOnboardingPasswordRole", "bindPasswordVisibility");
    expect(binding).toContain('form.addEventListener("submit"');
    expect(binding).toContain("if (!submit || submit.disabled || !error) return");
    expect(binding).toMatch(/current\.addEventListener\("input", validate\)[\s\S]*?alternate\.addEventListener\("input", validate\)[\s\S]*?confirm\.addEventListener\("input", validate\)/);
    expect(binding).not.toMatch(/current\.addEventListener\("(?:click|focus)"/);
    for (const eventName of ["click", "focus", "pointerdown", "input"]) {
      const listener = new RegExp(`(?:current|alternate|confirm)\\.addEventListener\\("${eventName}"[\\s\\S]{0,180}?onboardingRoute`);
      expect(binding).not.toMatch(listener);
    }
    expect(onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "",
      statusTag: () => "",
    })).toContain('data-skip-onboarding-password-role="burnpass"');
    const onboardingBinding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingBinding).toContain('querySelectorAll<HTMLButtonElement>("button[data-password-role-next]")');
    expect(onboardingBinding).not.toContain('querySelectorAll<HTMLButtonElement>("[data-password-role-next]")');
    expect(onboardingBinding).toContain('querySelectorAll<HTMLButtonElement>("button[data-skip-onboarding-password-role]")');
    expect(onboardingBinding).toMatch(/button\[data-skip-onboarding-password-role\][\s\S]*?onboardingRoute = next/);
    expect(binding).toContain("form.dataset.onboardingPasswordNext as OnboardingRoute");
    expect(binding).not.toContain("form.dataset.passwordRoleNext");
  });

  it("re-reads readiness when password setup reports a failure", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding).toContain('submit.textContent = setupMode ? "Creating account…" : "Unlocking…"');
    expect(binding).toContain('form.setAttribute("aria-busy", "true")');
    expect(binding).toMatch(/catch \(failure\)[\s\S]*?withNativeDeadline\(loadCoreIntegration\(\), "Check OSL account"/);
    expect(binding).toContain('readiness.bootstrapStatus === "ready" && readiness.unlocked');
    expect(binding).toContain('readiness.bootstrapStatus === "passwordRequired"');
    expect(binding).toMatch(/readiness\.bootstrapStatus === "passwordRequired"[\s\S]*?unlockHubPasswordGate\(secret\)/);
    expect(binding).toContain('readiness.bootstrapStatus === "setupRequired" && readiness.identityLoaded');
    expect(binding).not.toContain("password setup did not finish");
  });

  it("does not replace a focused onboarding control during an unchanged background refresh", () => {
    const rendering = functionSource("renderOnboarding", "onboardingContent");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    // An identical repaint nobody asked for is skipped, and a changed one that
    // would land on top of a half-typed password is deferred until it will not.
    expect(onboardingPaintDecision({
      markupUnchanged: true,
      shellMounted: true,
      sameRouteAsRendered: true,
      passwordEditInProgress: false,
      forced: false,
    })).toBe("skip-unchanged");
    expect(onboardingPaintDecision({
      markupUnchanged: false,
      shellMounted: true,
      sameRouteAsRendered: true,
      passwordEditInProgress: true,
      forced: false,
    })).toBe("defer-sensitive-edit");
    // A step change is painted even mid-typing: the field being guarded belongs
    // to a screen that is going away.
    expect(onboardingPaintDecision({
      markupUnchanged: false,
      shellMounted: true,
      sameRouteAsRendered: false,
      passwordEditInProgress: true,
      forced: false,
    })).toBe("paint");
    expect(rendering).toContain("onboardingPaintDecision(");
    expect(bootstrap).toContain("renderWhenIdle();");
    expect(bootstrap).not.toContain('route === "onboarding" ? render() : renderWhenIdle()');
  });

  it("reloads the encrypted service registry after first password setup and recovery", () => {
    const createBinding = functionSource("bindPasswordForm", "bindImportForm");
    const importBinding = functionSource("bindImportForm", "renderWorkspace");
    expect(createBinding).toMatch(/setupHubMainPassword\(secret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
    expect(importBinding).toMatch(/setupHubMainPassword\(passwordSecret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
  });

  it("reloads password roles immediately after setup and unlock", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding.match(/loadHubPasswordRoleStatus\(\)/g)?.length).toBeGreaterThanOrEqual(2);
  });
});
