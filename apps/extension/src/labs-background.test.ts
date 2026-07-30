import { afterEach, describe, expect, it, vi } from "vitest";

import {
  enableDedicatedWorkerExperiment,
  handleLabsContentStop,
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

describe("Labs page stop provenance", () => {
  afterEach(() => {
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
        query: vi.fn(async (query: { active?: boolean }) =>
          query.active ? [{ id: TAB_ID, url: PAGE_URL }] : [],
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
});
