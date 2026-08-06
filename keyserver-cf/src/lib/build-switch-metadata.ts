export interface RuntimeSwitchState {
  name: string;
  value: "true";
}

export interface BuildSwitchTestMetadata {
  record: "osl.keyserver.build-switches.v1";
  one_build_version: string;
  active_runtime_switches: readonly RuntimeSwitchState[];
}

export const BUILD_SWITCH_ONE_BUILD_VERSION = "0.0.1";

export const BUILD_SWITCH_TEST_METADATA = {
  record: "osl.keyserver.build-switches.v1",
  one_build_version: BUILD_SWITCH_ONE_BUILD_VERSION,
  active_runtime_switches: [
    { name: "CRYPTO_BTC_ENABLED", value: "true" },
    { name: "CRYPTO_XMR_ENABLED", value: "true" },
    { name: "CRYPTO_DONATION_BTC_ENABLED", value: "true" },
    { name: "CRYPTO_DONATION_XMR_ENABLED", value: "true" },
  ],
} as const satisfies BuildSwitchTestMetadata;

export function validateBuildSwitchTestMetadata(
  metadata: BuildSwitchTestMetadata,
  expectedActiveSwitches: readonly RuntimeSwitchState[],
  expectedOneBuildVersion = BUILD_SWITCH_ONE_BUILD_VERSION,
): string[] {
  const errors: string[] = [];
  if (metadata.one_build_version !== expectedOneBuildVersion) {
    errors.push(
      `build-switch metadata version ${metadata.one_build_version} does not match ${expectedOneBuildVersion}`,
    );
  }

  const seen = new Set<string>();
  for (const sw of metadata.active_runtime_switches) {
    if (seen.has(sw.name)) {
      errors.push(`build-switch metadata duplicates active runtime switch ${sw.name}`);
    }
    seen.add(sw.name);
    if (sw.value !== "true") {
      errors.push(`build-switch metadata active runtime switch ${sw.name} is not true`);
    }
  }

  const expectedNames = expectedActiveSwitches.map((sw) => sw.name);
  const actualNames = metadata.active_runtime_switches.map((sw) => sw.name);
  for (const expected of expectedNames) {
    if (!actualNames.includes(expected)) {
      errors.push(`build-switch metadata omits active runtime switch ${expected}`);
    }
  }
  for (const actual of actualNames) {
    if (!expectedNames.includes(actual)) {
      errors.push(`build-switch metadata includes inactive runtime switch ${actual}`);
    }
  }

  const expectedOrder = expectedNames.join(",");
  const actualOrder = actualNames.join(",");
  if (expectedOrder !== actualOrder) {
    errors.push(
      `build-switch metadata switch order ${actualOrder} does not match active runtime order ${expectedOrder}`,
    );
  }
  return errors;
}

export function formatBuildSwitchTestMetadata(metadata: BuildSwitchTestMetadata): string {
  return [
    `one-build version: ${metadata.one_build_version}`,
    "active runtime switches:",
    ...metadata.active_runtime_switches.map((sw) => `- ${sw.name}=${sw.value}`),
  ].join("\n");
}
