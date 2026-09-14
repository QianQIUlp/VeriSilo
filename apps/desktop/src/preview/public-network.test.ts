import { afterEach, describe, expect, it, vi } from "vitest";
import { runDesktopNetworkCheck } from "./public-network.js";

afterEach(() => vi.unstubAllGlobals());

describe("public website network demo", () => {
  it("returns explicit simulated evidence without making network requests", async () => {
    const fetch = vi.fn(() => {
      throw new Error("The public demo must not probe a visitor");
    });
    vi.stubGlobal("fetch", fetch);
    const result = await runDesktopNetworkCheck();
    expect(fetch).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).toContain("203.0.113.7");
    expect(JSON.stringify(result)).toContain("不会探测你的 IP 或 DNS");
  });
});
