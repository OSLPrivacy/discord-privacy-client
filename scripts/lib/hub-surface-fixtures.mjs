import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const LIB_DIR = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.dirname(path.dirname(LIB_DIR));
export const UI_ROOT = path.join(REPO_ROOT, 'apps', 'osl-hub-ui');

export const HUB_SCREEN_ROUTES = ['home', 'inbox', 'people', 'privacy', 'activity', 'connections', 'mullvad', 'osl-chat', 'osl-mail', 'osl-servers', 'signal-qa'];
export const HUB_ONBOARDING_STEPS = ['pro', 'welcome', 'create', 'import', 'unlock', 'account-recovery', 'recovery', 'mullvad', 'sending', 'defaults', 'cover', 'passwords', 'burnpass', 'privacy', 'tutorial', 'detected', 'install', 'apps', 'browser', 'decoy'];
export const HUB_SETTINGS_SECTIONS = ['account', 'apps', 'scrub', 'cleanup', 'notifications', 'appearance', 'about'];
export const HUB_DIALOG_SURFACES = ['friends', 'people-in-chat', 'whitelist-roster', 'native-protect-friend', 'scrub-review', 'burn', 'owned-confirmation', 'update', 'osl-chat-settings'];

async function withHubUiTestModule(vitestName, callback) {
  const requireFromUi = createRequire(path.join(UI_ROOT, 'package.json'));
  const { createServer: createViteServer } = requireFromUi('vite');
  const previousVitest = process.env.VITEST;
  const previousStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  const values = new Map();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, String(value)),
      removeItem: (key) => values.delete(key),
      clear: () => values.clear(),
    },
  });
  process.env.VITEST = vitestName;
  const vite = await createViteServer({
    root: UI_ROOT,
    configFile: false,
    appType: 'custom',
    logLevel: 'error',
    server: { middlewareMode: true, watch: { ignored: ['**/*'] } },
  });
  try {
    const { __oslHubUiTest: ui } = await vite.ssrLoadModule('/src/main.ts');
    return await callback(ui, vite);
  } finally {
    await vite.close();
    if (previousVitest === undefined) delete process.env.VITEST;
    else process.env.VITEST = previousVitest;
    if (previousStorage) Object.defineProperty(globalThis, 'localStorage', previousStorage);
    else delete globalThis.localStorage;
  }
}

async function buildHubScreenshotSurfaces(ui, vite) {
  const surfaces = [];
  const add = (name, markup) => surfaces.push({ kind: 'screen', name, markup });

  for (const route of HUB_SCREEN_ROUTES) {
    ui.reset({ coreReady: true, servicesChecked: true });
    add(`route:${route}`, ui.renderRouteShell(route));
  }
  for (const destination of HUB_ONBOARDING_STEPS) {
    ui.reset({ coreReady: true, servicesChecked: true });
    add(`onboarding:${destination}`, ui.renderOnboardingRoute(destination));
  }
  for (const section of HUB_SETTINGS_SECTIONS) {
    ui.reset({ coreReady: true, servicesChecked: true });
    add(`settings:${section}`, ui.renderSettingsSection(section));
  }
  ui.reset({ coreReady: true, servicesChecked: true });
  add('service:discord', ui.renderServiceHeader('discord'));
  // The default test reset intentionally keeps every overlay closed, which
  // makes `renderProtectedSheets()` empty. A screenshot surface must instead
  // render the shipping local-protection component in its initial open state.
  const { blankLocalProtectedModel, localProtectedSheetMarkup } = await vite.ssrLoadModule('/src/local-protected-sheet.ts');
  add('protected-sheets', localProtectedSheetMarkup(blankLocalProtectedModel(true), 'manual'));
  return surfaces;
}

export async function hubScreenshotSurfaceMarkup(vitestName = 'screenshot-claim-gate') {
  return withHubUiTestModule(vitestName, buildHubScreenshotSurfaces);
}

export async function hubTabTravelSurfaceMarkup(vitestName = 'hub-tab-travel-gate') {
  return withHubUiTestModule(vitestName, async (ui, vite) => {
    const screens = await buildHubScreenshotSurfaces(ui, vite);
    const dialogs = [];
    for (const name of HUB_DIALOG_SURFACES) {
      ui.reset({ coreReady: true, servicesChecked: true });
      dialogs.push({ kind: 'dialog', name: `dialog:${name}`, markup: ui.renderDialogSurfaceForTest(name) });
    }
    return [...screens, ...dialogs];
  });
}
