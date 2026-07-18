import { describe, expect, it } from "vitest";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep } from "./service-guide";

describe("service guide state", () => {
  it("accepts only a bounded, non-sensitive resume marker", () => {
    expect(parseServiceGuideState('{"serviceId":"instagram","step":2}')).toEqual({ serviceId: "instagram", step: 2 });
    expect(parseServiceGuideState('{"serviceId":"whatsapp","step":1}')).toEqual({ serviceId: "whatsapp", step: 1 });
    expect(parseServiceGuideState('{"serviceId":"discord","step":3}')).toEqual({ serviceId: "discord", step: 2 });
    expect(parseServiceGuideState('{"serviceId":"instagram","step":4}')).toBeNull();
    expect(parseServiceGuideState('{"serviceId":"unknown","step":1}')).toBeNull();
    expect(parseServiceGuideState('{"serviceId":"x","step":1,"token":"secret"}')).toBeNull();
  });

  it("recognizes every catalog service id, including later enterprise entries", () => {
    expect(parseServiceGuideState('{"serviceId":"linkedin","step":0}')).toEqual({ serviceId: "linkedin", step: 0 });
  });

  it("keeps navigation within the three-step guide", () => {
    expect(previousServiceGuideStep(0)).toBe(0);
    expect(nextServiceGuideStep(0)).toBe(1);
    expect(nextServiceGuideStep(2)).toBe(2);
  });
});
