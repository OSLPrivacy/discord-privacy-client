/**
 * Data the installer measures for an optional component before presenting it
 * at onboarding. Keeping this boundary explicit prevents the UI from turning
 * planning estimates into download-size claims.
 */
export type ComponentManifestEntry = Readonly<{
  id: string;
  displayName: string;
  measuredSizeBytes: number;
  withoutIt: string;
}>;

export type ComponentPickerItem = Readonly<{
  id: string;
  displayName: string;
  size: string;
  withoutIt: string;
  selected: false;
}>;

export type ComponentPickerScreen = Readonly<{
  title: string;
  introductoryCopy: string;
  components: readonly ComponentPickerItem[];
}>;

/**
 * Builds the first-run optional-component picker. Selection deliberately
 * starts empty: encryption's word-bank path ships with the base app and no
 * optional download is required for a functional install.
 */
export function componentPickerScreen(
  components: readonly ComponentManifestEntry[],
): ComponentPickerScreen {
  return {
    title: "Choose optional downloads",
    introductoryCopy: "OSL works without these downloads. You can add them later.",
    components: components.map(component => ({
      id: component.id,
      displayName: component.displayName,
      size: formatMeasuredSize(component.measuredSizeBytes),
      withoutIt: statedAbsenceConsequence(component.withoutIt),
      selected: false,
    })),
  };
}

function formatMeasuredSize(bytes: number): string {
  if (!Number.isSafeInteger(bytes) || bytes <= 0) {
    throw new Error("Each optional component needs a positive measured size before it can be shown.");
  }

  if (bytes >= GIB) return `${formatNumber(bytes / GIB)} GB`;
  if (bytes >= MIB) return `${formatNumber(bytes / MIB)} MB`;
  if (bytes >= KIB) return `${formatNumber(bytes / KIB)} KB`;
  return `${bytes} bytes`;
}

function statedAbsenceConsequence(consequence: string): string {
  if (consequence.trim().length === 0) {
    throw new Error("Each optional component needs a stated absence consequence before it can be shown.");
  }
  return consequence;
}

function formatNumber(value: number): string {
  return value.toLocaleString("en-US", { maximumFractionDigits: 2 });
}

const KIB = 1024;
const MIB = KIB * KIB;
const GIB = MIB * KIB;
