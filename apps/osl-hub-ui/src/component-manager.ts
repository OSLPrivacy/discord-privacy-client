/**
 * The optional-component state used by Settings' "Add features" surface.
 * A component's fallback is part of its definition so removal cannot leave a
 * dependent feature claiming that it is still available.
 */
export type OptionalComponent = Readonly<{
  id: string;
  featureName: string;
  fallback: string;
}>;

export type ComponentFeatureState = Readonly<{
  id: string;
  featureName: string;
  availability: "available" | "fallback";
  detail: string;
}>;

export type ComponentManager = Readonly<{
  installedIds: readonly string[];
  features: readonly ComponentFeatureState[];
}>;

/**
 * Applies the selected onboarding downloads to the same state shape used by
 * Settings. Unknown ids are rejected rather than silently reported as ready.
 */
export function componentManagerFromOnboarding(
  components: readonly OptionalComponent[],
  selectedIds: readonly string[],
): ComponentManager {
  return buildManager(components, selectedIds);
}

/** Installs a Settings-selected component without changing unrelated state. */
export function installComponent(
  manager: ComponentManager,
  components: readonly OptionalComponent[],
  componentId: string,
): ComponentManager {
  knownComponent(components, componentId);
  return buildManager(components, [...manager.installedIds, componentId]);
}

/**
 * Removes an optional component. Its feature remains present as an explicit
 * fallback, never as an unavailable-looking success state.
 */
export function removeComponent(
  manager: ComponentManager,
  components: readonly OptionalComponent[],
  componentId: string,
): ComponentManager {
  knownComponent(components, componentId);
  return buildManager(components, manager.installedIds.filter(id => id !== componentId));
}

function buildManager(
  components: readonly OptionalComponent[],
  installedIds: readonly string[],
): ComponentManager {
  const componentIds = new Set(components.map(component => component.id));
  for (const id of installedIds) {
    if (!componentIds.has(id)) throw new Error(`Unknown optional component: ${id}`);
  }

  const installed = new Set(installedIds);
  return {
    installedIds: components.filter(component => installed.has(component.id)).map(component => component.id),
    features: components.map(component => installed.has(component.id)
      ? {
          id: component.id,
          featureName: component.featureName,
          availability: "available",
          detail: `${component.featureName} is available.`,
        }
      : {
          id: component.id,
          featureName: component.featureName,
          availability: "fallback",
          detail: component.fallback,
        }),
  };
}

function knownComponent(components: readonly OptionalComponent[], componentId: string): void {
  if (!components.some(component => component.id === componentId)) {
    throw new Error(`Unknown optional component: ${componentId}`);
  }
}
