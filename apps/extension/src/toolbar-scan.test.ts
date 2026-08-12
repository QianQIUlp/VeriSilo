import { afterEach, describe, expect, it, vi } from "vitest";

import {
  armToolbarScan,
  clearToolbarScanForTab,
  consumeToolbarScan,
  PENDING_TOOLBAR_SCAN_TTL_MS,
} from "./toolbar-scan.js";

const PENDING_KEY = "scan:pending-toolbar-authorization";

afterEach(() => {
  vi.unstubAllGlobals();
});

function installChromeMock(initial?: unknown) {
  const values: Record<string, unknown> = {};
  if (initial !== undefined) {
    values[PENDING_KEY] = initial;
  }
  const setBadgeText = vi.fn(async () => undefined);
  const setBadgeBackgroundColor = vi.fn(async () => undefined);
  const setTitle = vi.fn(async () => undefined);
  vi.stubGlobal("chrome", {
    storage: {
      session: {
        get: vi.fn(async (key: string) => ({ [key]: values[key] })),
        set: vi.fn(async (update: Record<string, unknown>) => {
          Object.assign(values, update);
        }),
        remove: vi.fn(async (key: string) => {
          delete values[key];
        }),
      },
    },
    action: { setBadgeText, setBadgeBackgroundColor, setTitle },
  });
  return { values, setBadgeText, setBadgeBackgroundColor, setTitle };
}

describe("toolbar-authorized scan", () => {
  it("marks only the requested tab with a visible authorization badge", async () => {
    const chromeMock = installChromeMock();

    await armToolbarScan(42, 1_000);

    expect(chromeMock.values[PENDING_KEY]).toEqual({
      tabId: 42,
      requestedAt: 1_000,
    });
    expect(chromeMock.setBadgeText).toHaveBeenCalledWith({
      tabId: 42,
      text: "1",
    });
  });

  it("consumes a fresh request only from the same tab", async () => {
    const chromeMock = installChromeMock({ tabId: 42, requestedAt: 1_000 });

    await expect(consumeToolbarScan(7, 2_000)).resolves.toBe(false);
    expect(chromeMock.values[PENDING_KEY]).toEqual({
      tabId: 42,
      requestedAt: 1_000,
    });
    await expect(consumeToolbarScan(42, 2_000)).resolves.toBe(true);
    expect(chromeMock.values[PENDING_KEY]).toBeUndefined();
    expect(chromeMock.setBadgeText).toHaveBeenLastCalledWith({
      tabId: 42,
      text: "",
    });
  });

  it("does not run a scan from an expired toolbar request", async () => {
    installChromeMock({ tabId: 42, requestedAt: 1_000 });

    await expect(
      consumeToolbarScan(42, 1_000 + PENDING_TOOLBAR_SCAN_TTL_MS + 1),
    ).resolves.toBe(false);
  });

  it("clears a pending request when its tab navigates or closes", async () => {
    const chromeMock = installChromeMock({ tabId: 42, requestedAt: 1_000 });

    await clearToolbarScanForTab(42);

    expect(chromeMock.values[PENDING_KEY]).toBeUndefined();
    expect(chromeMock.setBadgeText).toHaveBeenCalledWith({
      tabId: 42,
      text: "",
    });
  });
});
