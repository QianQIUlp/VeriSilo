import { describe, expect, it } from "vitest";

import {
  canConfigureWslDistribution,
  requiresExplicitWslSelection,
} from "./wsl-selection.js";

describe("explicit WSL distribution selection", () => {
  it("never treats discovery order as a user selection", () => {
    const distributions = ["Ubuntu", "Debian"];
    expect(requiresExplicitWslSelection(distributions, "")).toBe(true);
    expect(canConfigureWslDistribution(distributions, "")).toBe(false);
    expect(canConfigureWslDistribution(distributions, "Debian")).toBe(true);
    expect(canConfigureWslDistribution(distributions, "Unknown")).toBe(false);
  });
});
