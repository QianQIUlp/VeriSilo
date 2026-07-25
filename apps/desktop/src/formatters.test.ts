import { describe, expect, it } from "vitest";

import { describeNetwork } from "./formatters.js";

describe("desktop formatters", () => {
  it("never describes a direct Silo as proxy protected", () => {
    expect(describeNetwork({ mode: "direct", proxyRequired: false })).toContain(
      "直连",
    );
  });
});
