import { describe, expect, it } from "vitest";
import { nextServiceGuideStep, parseServiceGuideState, previousServiceGuideStep } from "./service-guide";

describe("service guide state", () => {
  it("accepts only a bounded, non-sensitive resume marker", () => {
    expect(parseServiceGuideState('{"serviceId":"telegram","step":2}')).toEqual({ serviceId: "telegram", step: 2 });
    expect(parseServiceGuideState('{"serviceId":"whatsapp","step":1}')).toEqual({ serviceId: "whatsapp", step: 1 });
    expect(parseServiceGuideState('{"serviceId":"discord","step":3}')).toEqual({ serviceId: "discord", step: 2 });
    expect(parseServiceGuideState('{"serviceId":"telegram","step":4}')).toBeNull();
    expect(parseServiceGuideState('{"serviceId":"unknown","step":1}')).toBeNull();
    expect(parseServiceGuideState('{"serviceId":"email","step":1,"token":"secret"}')).toBeNull();
  });

  it("rejects the LinkedIn guide marker superseded by the owner ruling on 2026-08-05", () => {
    expect(parseServiceGuideState('{"serviceId":"linkedin","step":0}')).toBeNull();
  });

  it("keeps navigation within the three-step guide", () => {
    expect(previousServiceGuideStep(0)).toBe(0);
    expect(nextServiceGuideStep(0)).toBe(1);
    expect(nextServiceGuideStep(2)).toBe(2);
  });
});
