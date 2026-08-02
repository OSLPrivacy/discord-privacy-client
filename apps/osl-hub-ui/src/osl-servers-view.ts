import { oslSpacesSurfaceMarkup } from "./osl-spaces";

/** The shipping destination for OSL's Discord-equivalent encrypted servers. */
export function oslServersViewMarkup(statusTag: (label: string) => string): string {
  return oslSpacesSurfaceMarkup({ statusTag });
}
