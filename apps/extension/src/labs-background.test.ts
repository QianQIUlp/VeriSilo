import { afterEach, describe, expect, it, vi } from "vitest";

import {
  enableDedicatedWorkerExperiment,
  handleLabsContentStop,
  labsTabNavigated,
  stopDedicatedWorkerExperiment,
} from "./labs-background.js";

const TAB_ID = 7;
const PAGE_URL = "https://example.test/page";

function storageGet(
  storage: Record<string, unknown>,
  keys: unknown,
): Record<string, unknown> {
  if (keys === null) {
    return { ...storage };
  }
  if (typeof keys === "string") {
    return keys in storage ? { [keys]: storage[keys] } : {};
  }
  if (Array.isArray(keys)) {
    return Object.fromEntries(
      keys.flatMap((key) =>
        typeof key === "string" && key in storage ? [[key, storage[key]]] : [],
      ),
    );
  }
  return {};
}

function storageArea(storage: Record<string, unknown>) {
  return {
    get: vi.fn(async (keys: unknown) => storageGet(storage, keys)),
    set: vi.fn(async (items: Record<string, unknown>) => {
      Object.assign(storage, items);
    }),
    remove: vi.fn(async (keys: string | string[]) => {
      for (const key of Array.isArray(keys) ? keys : [keys]) {
        delete storage[key];
      }
    }),
  };
}

type CanaryExposure = {
  locationOrDocument: boolean;
  visibleCookie: boolean;
  localStorage: boolean;
  sessionStorage: boolean;
  serviceWorkerUrl: boolean;
  sameOriginIframe: boolean;
};

const CLEAR_CANARY_EXPOSURE: CanaryExposure = {
  locationOrDocument: false,
  visibleCookie: false,
  localStorage: false,
  sessionStorage: false,
  serviceWorkerUrl: false,
  sameOriginIframe: false,
};

function installMonitorChromeMock(
  scanResult: (
    scanNumber: number,
  ) => CanaryExposure | Promise<CanaryExposure> = () => CLEAR_CANARY_EXPOSURE,
  incognito = false,
  restoreFails = false,
) {
  const sessionStorage: Record<string, unknown> = {};
  const localStorage: Record<string, unknown> = {};
  const restoreCalls: string[] = [];
  let scanCount = 0;
  const executeScript = vi.fn(async (injection: unknown) => {
    const request = injection as {
      files?: string[];
      func?: { name?: string };
    };
    if (request.files !== undefined) {
      return [];
    }
    switch (request.func?.name) {
      case "observeDedicatedWorkerEnvironment":
        return [
          {
            result: {
              origin: "https://example.test",
              workerAvailable: true,
              constructorRestorable: true,
            },
          },
        ];
      case "installDedicatedWorkerExperiment":
        return [{ result: { active: true, constructorWrapped: true } }];
      case "verifyDedicatedWorkerExperiment":
        return [
          {
            result: {
              active: true,
              constructorWrapped: true,
              newWorkerHandshake: true,
              sameOriginIframeConsistent: true,
            },
          },
        ];
      case "scanCanaryExposure":
        scanCount += 1;
        return [{ result: await scanResult(scanCount) }];
      case "restoreDedicatedWorkerExperiment":
        restoreCalls.push(request.func.name ?? "restore");
        if (restoreFails) {
          throw new Error("destination is inaccessible");
        }
        return [{ result: { restored: true, constructorNative: true } }];
      default:
        throw new Error(`Unexpected script: ${request.func?.name ?? "files"}`);
    }
  });
  vi.stubGlobal("chrome", {
    permissions: {
      contains: vi.fn(async () => true),
      request: vi.fn(async () => true),
    },
    runtime: {
      sendNativeMessage: vi.fn(async () => {
        throw new Error("desktop runtime unavailable in test");
      }),
    },
    scripting: { executeScript },
    storage: {
      local: storageArea(localStorage),
      session: storageArea(sessionStorage),
    },
    tabs: {
      get: vi.fn(async () => ({
        id: TAB_ID,
        incognito,
        url: PAGE_URL,
      })),
      query: vi.fn(async () => [{ id: TAB_ID, incognito, url: PAGE_URL }]),
    },
  } as unknown as typeof chrome);
  return {
    localStorage,
    restoreCalls,
    scanCount: () => scanCount,
    sessionStorage,
  };
}

describe("Labs page stop provenance", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("terminates on a page signal without trusting its leak classification", async () => {
    const sessionStorage: Record<string, unknown> = {};
    const localStorage: Record<string, unknown> = {};
    const restoreCalls: string[] = [];
    const executeScript = vi.fn(async (injection: unknown) => {
      const request = injection as {
        files?: string[];
        func?: { name?: string };
      };
      if (request.files !== undefined) {
        return [];
      }
      switch (request.func?.name) {
        case "observeDedicatedWorkerEnvironment":
          return [
            {
              result: {
                origin: "https://example.test",
                workerAvailable: true,
                constructorRestorable: true,
              },
            },
          ];
        case "installDedicatedWorkerExperiment":
          return [{ result: { active: true, constructorWrapped: true } }];
        case "verifyDedicatedWorkerExperiment":
          return [
            {
              result: {
                active: true,
                constructorWrapped: true,
                newWorkerHandshake: true,
                sameOriginIframeConsistent: true,
              },
            },
          ];
        case "restoreDedicatedWorkerExperiment":
          restoreCalls.push(request.func.name ?? "restore");
          return [{ result: { restored: true, constructorNative: true } }];
        default:
          throw new Error(
            `Unexpected script: ${request.func?.name ?? "files"}`,
          );
      }
    });
    vi.stubGlobal("chrome", {
      permissions: {
        contains: vi.fn(async () => true),
        request: vi.fn(async () => true),
      },
      runtime: {
        sendNativeMessage: vi.fn(async () => {
          throw new Error("desktop runtime unavailable in test");
        }),
      },
      scripting: { executeScript },
      storage: {
        local: storageArea(localStorage),
        session: storageArea(sessionStorage),
      },
      tabs: {
        get: vi.fn(async () => ({
          id: TAB_ID,
          incognito: false,
          url: PAGE_URL,
        })),
        query: vi.fn(async (query: { active?: boolean }) =>
          query.active ? [{ id: TAB_ID, incognito: false, url: PAGE_URL }] : [],
        ),
      },
    } as unknown as typeof chrome);

    await enableDedicatedWorkerExperiment();
    const active = sessionStorage["labs:status"] as {
      runId: string;
    };
    await expect(
      handleLabsContentStop(
        {
          type: "verisilo_labs_stop",
          runId: active.runId,
          stopCode: "worker_canary_leak",
        },
        { tab: { id: TAB_ID }, frameId: 0 } as chrome.runtime.MessageSender,
      ),
    ).resolves.toEqual({ stopped: true });

    expect(sessionStorage["labs:status"]).toMatchObject({
      enabled: false,
      state: "failed",
      lastReceipt: {
        stopCode: "verification_failed",
        restore: { attempted: true, succeeded: true },
      },
    });
    expect(sessionStorage["labs:status"]).not.toMatchObject({
      state: "leak_detected",
    });
    expect(Object.values(localStorage)).toContainEqual(
      expect.objectContaining({
        stopCode: "verification_failed",
        state: "failed",
      }),
    );
    expect(restoreCalls).toHaveLength(1);

    await enableDedicatedWorkerExperiment();
    const secondActive = sessionStorage["labs:status"] as {
      runId: string;
    };
    await expect(
      handleLabsContentStop(
        {
          type: "verisilo_labs_stop",
          runId: secondActive.runId,
          stopCode: "worker_error",
        },
        { tab: { id: TAB_ID }, frameId: 0 } as chrome.runtime.MessageSender,
      ),
    ).resolves.toEqual({ stopped: true });
    expect(sessionStorage["labs:status"]).toMatchObject({
      state: "failed",
      lastReceipt: { stopCode: "worker_error" },
    });
    expect(restoreCalls).toHaveLength(2);
  });

  it("restores with the precise stop code when a delayed canary leak appears", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-30T00:00:00.000Z"));
    const harness = installMonitorChromeMock((scanNumber) => ({
      ...CLEAR_CANARY_EXPOSURE,
      visibleCookie: scanNumber >= 2,
    }));

    await enableDedicatedWorkerExperiment();
    expect(harness.scanCount()).toBe(1);
    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      enabled: true,
      state: "best_effort",
    });

    await vi.advanceTimersByTimeAsync(5_000);
    await vi.waitFor(() => expect(harness.restoreCalls).toHaveLength(1));

    expect(harness.scanCount()).toBe(2);
    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      enabled: false,
      state: "leak_detected",
      lastReceipt: {
        stopCode: "cookie_canary_leak",
        restore: { attempted: true, succeeded: true },
      },
    });
    expect(Object.values(harness.localStorage)).toContainEqual(
      expect.objectContaining({
        stopCode: "cookie_canary_leak",
        state: "leak_detected",
      }),
    );

    await vi.advanceTimersByTimeAsync(20_000);
    expect(harness.scanCount()).toBe(2);
  });

  it("cancels the pending canary monitor when the run is stopped", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-30T00:00:00.000Z"));
    const harness = installMonitorChromeMock();

    await enableDedicatedWorkerExperiment();
    expect(harness.scanCount()).toBe(1);

    await stopDedicatedWorkerExperiment();
    expect(harness.restoreCalls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(20_000);

    expect(harness.scanCount()).toBe(1);
    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      enabled: false,
      state: "restored",
      lastReceipt: { stopCode: "user_requested" },
    });
  });

  it("restores the live realm before stopping on same-document navigation", async () => {
    const harness = installMonitorChromeMock();

    await enableDedicatedWorkerExperiment();
    labsTabNavigated(TAB_ID);
    await vi.waitFor(() =>
      expect(harness.sessionStorage["labs:status"]).toMatchObject({
        enabled: false,
      }),
    );

    expect(harness.restoreCalls).toHaveLength(1);
    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      enabled: false,
      state: "restored",
      lastReceipt: {
        stopCode: "site_navigation",
        restore: { attempted: true, succeeded: true },
      },
    });
  });

  it("treats an inaccessible navigation destination as a discarded old realm", async () => {
    const harness = installMonitorChromeMock(
      () => CLEAR_CANARY_EXPOSURE,
      false,
      true,
    );

    await enableDedicatedWorkerExperiment();
    labsTabNavigated(TAB_ID);
    await vi.waitFor(() =>
      expect(harness.sessionStorage["labs:status"]).toMatchObject({
        enabled: false,
      }),
    );

    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      state: "restored",
      lastReceipt: {
        stopCode: "site_navigation",
        restore: { attempted: true, succeeded: true },
      },
    });
  });

  it("does not hide an explicit-stop restoration failure", async () => {
    const harness = installMonitorChromeMock(
      () => CLEAR_CANARY_EXPOSURE,
      false,
      true,
    );

    await enableDedicatedWorkerExperiment();
    await stopDedicatedWorkerExperiment();

    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      state: "failed",
      lastReceipt: {
        stopCode: "user_requested",
        restore: { attempted: true, succeeded: false },
      },
    });
  });

  it("keeps an Incognito run and receipt out of shared local storage", async () => {
    const harness = installMonitorChromeMock(() => CLEAR_CANARY_EXPOSURE, true);

    await enableDedicatedWorkerExperiment();
    await stopDedicatedWorkerExperiment();

    expect(harness.sessionStorage["labs:status"]).toMatchObject({
      enabled: false,
      state: "restored",
      lastReceipt: { stopCode: "user_requested" },
    });
    expect(Object.keys(harness.localStorage)).toEqual([]);
  });
});
