import type { HomeAppCatalogEntry, NativeApp } from "./services";

type OnboardingApp = Pick<HomeAppCatalogEntry, "id" | "linked">;
type NativeAppAvailability = Pick<NativeApp, "id" | "availability">;

export interface OnboardingAppGroups<T extends OnboardingApp> {
  connected: T[];
  browserHistory: T[];
  other: T[];
}

export function groupOnboardingApps<T extends OnboardingApp>({
  apps,
  nativeApps,
  savedAccountsReady,
  importedBrowserAppIds,
}: {
  apps: readonly T[];
  nativeApps: readonly NativeAppAvailability[];
  savedAccountsReady: boolean;
  importedBrowserAppIds: ReadonlySet<HomeAppCatalogEntry["id"]>;
}): OnboardingAppGroups<T> {
  const connected: T[] = [];
  const browserHistory: T[] = [];
  const other: T[] = [];

  for (const app of apps) {
    const native = nativeApps.find((candidate) => candidate.id === app.id);
    if (app.linked || native?.availability === "installed") {
      connected.push(app);
    } else if (savedAccountsReady && importedBrowserAppIds.has(app.id)) {
      browserHistory.push(app);
    } else {
      other.push(app);
    }
  }

  return { connected, browserHistory, other };
}
