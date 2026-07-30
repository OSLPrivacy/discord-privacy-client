import { describe, expect, it } from "vitest";
import {
  isOslPrimaryDestination,
  oslPrimaryDestinationValues,
  oslPrimaryDestinations,
  oslSettingsDestination,
  type OslPrimaryDestination,
} from "./state";

describe("six-destination information architecture contract", () => {
  it("defines the fixed primary destination order with Settings outside it", () => {
    expect(oslPrimaryDestinationValues).toEqual([
      "home",
      "inbox",
      "people",
      "privacy",
      "activity",
      "connections",
    ]);
    expect(new Set(oslPrimaryDestinationValues).size).toBe(6);
    expect(oslPrimaryDestinationValues).not.toContain(oslSettingsDestination);
    expect(oslSettingsDestination).toBe("settings");
  });

  it("keeps every destination definition aligned with the design contract", () => {
    const definitionsById: Record<OslPrimaryDestination, {
      label: string;
      userQuestion: string;
      primaryAction: string;
    }> = {
      home: {
        label: "Home",
        userQuestion: "Am I protected, and what needs attention?",
        primaryAction: "Fix the most important issue",
      },
      inbox: {
        label: "Inbox",
        userQuestion: "Where are my conversations?",
        primaryAction: "Start a private conversation",
      },
      people: {
        label: "People",
        userQuestion: "Who do I trust and where do I know them?",
        primaryAction: "Add or verify a person",
      },
      privacy: {
        label: "Privacy",
        userQuestion: "What will OSL do for me?",
        primaryAction: "Review or change protection",
      },
      activity: {
        label: "Activity",
        userQuestion: "What did OSL actually do?",
        primaryAction: "Review an item needing attention",
      },
      connections: {
        label: "Connections",
        userQuestion: "Which accounts and devices are connected?",
        primaryAction: "Connect a service",
      },
    };

    expect(oslPrimaryDestinations.map((destination) => destination.id)).toEqual(oslPrimaryDestinationValues);
    for (const destination of oslPrimaryDestinations) {
      expect(destination).toMatchObject(definitionsById[destination.id]);
      expect(destination.mainContent.trim()).not.toBe("");
    }
  });

  it("does not expose implementation concepts as destination copy", () => {
    const visibleCopy = oslPrimaryDestinations
      .flatMap((destination) => [
        destination.label,
        destination.userQuestion,
        destination.mainContent,
        destination.primaryAction,
      ])
      .join("\n");

    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
  });

  it("refuses unknown or secondary routes as primary destinations", () => {
    for (const destination of oslPrimaryDestinationValues) {
      expect(isOslPrimaryDestination(destination)).toBe(true);
    }
    expect(isOslPrimaryDestination("settings")).toBe(false);
    expect(isOslPrimaryDestination("service")).toBe(false);
    expect(isOslPrimaryDestination(null)).toBe(false);
  });
});
